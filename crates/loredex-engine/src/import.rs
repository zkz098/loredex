//! OpenWiki importer: bring an existing `langchain-ai/openwiki` bundle
//! (Google OKF v0.2 shape — a directory of interlinked Markdown pages with
//! a YAML frontmatter where `type` is the only required field, standard
//! relative links, reserved `index.md`/`log.md`) into the loredex store.
//!
//! Trust model is preserved: imported bodies without resolvable
//! `[[cite:path:Lx-Ly]]` evidence land as `status: draft`, so `loredex plan`
//! picks them up for citation backfill instead of laundering unverified
//! prose into the verified set.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use loredex_core::{dedup_citations, parse_citations, PageMeta, Status, WikiError, WikiPage};
use loredex_git::GitRepo;

use crate::{now_iso, pages_dir, playbook_registry, record_evidence_with, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    OpenWiki,
}

impl ImportFormat {
    pub fn parse(s: &str) -> Option<ImportFormat> {
        match s.to_lowercase().as_str() {
            "openwiki" | "okf" => Some(ImportFormat::OpenWiki),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ImportedPage {
    pub id: String,
    pub title: String,
    pub page_type: Option<String>,
    pub status: Status,
    pub evidence: usize,
    pub overwritten: bool,
}

#[derive(Debug)]
pub struct SkippedFile {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct ImportReport {
    pub imported: Vec<ImportedPage>,
    pub skipped: Vec<SkippedFile>,
    /// page types newly registered in wiki.toml `[page_types.*]`
    pub registered_types: Vec<String>,
}

/// Dispatch for `loredex import --format <f>`.
pub fn import_dir(
    repo: &GitRepo,
    wiki_dir: &Path,
    src: &Path,
    format: ImportFormat,
    prefix: Option<&str>,
    dry_run: bool,
    force: bool,
) -> Result<ImportReport> {
    match format {
        ImportFormat::OpenWiki => import_openwiki(repo, wiki_dir, src, prefix, dry_run, force),
    }
}

struct BundleFile {
    rel: String,   // slash-separated path relative to src, e.g. "modules/core.md"
    path: PathBuf, // absolute
    id: String,    // candidate page id (post-sanitize, incl. prefix)
}

pub fn import_openwiki(
    repo: &GitRepo,
    wiki_dir: &Path,
    src: &Path,
    prefix: Option<&str>,
    dry_run: bool,
    force: bool,
) -> Result<ImportReport> {
    if !src.is_dir() {
        return Err(WikiError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("import source is not a directory: {}", src.display()),
        )));
    }
    let mut report = ImportReport::default();
    let view = repo.view();
    let head_commit = view.head_commit().unwrap_or_default().to_string();

    // 1. enumerate bundle files and assign ids
    let mut files: Vec<BundleFile> = Vec::new();
    let mut id_owner: BTreeMap<String, String> = BTreeMap::new();
    let mut rel_to_id: BTreeMap<String, String> = BTreeMap::new();
    let mut walk = vec![src.to_path_buf()];
    while let Some(dir) = walk.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !name.starts_with('.') {
                    walk.push(path); // hidden dirs (.claims, ...) are bundle metadata
                }
                continue;
            }
            let Some(rel) = rel_path(src, &path) else {
                continue;
            };
            if !rel.ends_with(".md") || name == "INSTRUCTIONS.md" {
                continue;
            }
            if rel == "index.md" || rel == "log.md" {
                report.skipped.push(SkippedFile {
                    path: rel,
                    reason: "reserved bundle file (loredex generates its own index)".into(),
                });
                continue;
            }
            let stem = rel.trim_end_matches(".md").replace('\\', "/");
            let id = make_id(&stem, prefix);
            if loredex_core::validate_page_id(&id).is_err() {
                report.skipped.push(SkippedFile {
                    path: rel,
                    reason: format!("cannot derive a valid page id from `{stem}`"),
                });
                continue;
            }
            if let Some(prev) = id_owner.insert(id.clone(), rel.clone()) {
                report.skipped.push(SkippedFile {
                    path: rel,
                    reason: format!("page id `{id}` already taken by {prev}"),
                });
                continue;
            }
            if crate::index::is_generated(&id) {
                report.skipped.push(SkippedFile {
                    path: rel,
                    reason: format!("`{id}` is a loredex-generated page id"),
                });
                id_owner.remove(&id);
                continue;
            }
            rel_to_id.insert(rel.clone(), id.clone());
            files.push(BundleFile { rel, path, id });
        }
    }
    files.sort_by(|a, b| a.rel.cmp(&b.rel));

    // 2. per-file import
    let existing_types: Vec<String> = playbook_registry(wiki_dir).keys().cloned().collect();
    let mut new_types: BTreeMap<String, String> = BTreeMap::new();
    for f in &files {
        let text = std::fs::read_to_string(&f.path).map_err(WikiError::Io)?;
        let (fm, body) = split_frontmatter(&text);
        let fm_str = |k: &str| -> Option<String> {
            fm.as_ref()
                .and_then(|v| v.get(k))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };
        let page_type = fm_str("type");
        let title = fm_str("title").unwrap_or_else(|| title_from_body_or_stem(&body, &f.rel));
        let mut tags: Vec<String> = fm
            .as_ref()
            .and_then(|v| v.get("tags"))
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|t| t.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if !tags.iter().any(|t| t == "imported") {
            tags.push("imported".into());
        }

        let body = rewrite_md_links(&body, &f.rel, &rel_to_id);

        if let Some(t) = &page_type {
            if !existing_types.iter().any(|k| k == t) && !new_types.contains_key(t) {
                new_types.insert(t.clone(), title.clone());
            }
        }

        // citations: pin what pins, draft what doesn't
        let parsed = parse_citations(&body).unwrap_or_default();
        let cites = dedup_citations(parsed);
        let mut evidence = Vec::new();
        let mut status = Status::Draft;
        if !cites.is_empty() {
            let mut all_ok = true;
            for c in &cites {
                match record_evidence_with(&view, &c.path, c.lines) {
                    Ok(ev) => evidence.push(ev),
                    Err(_) => {
                        all_ok = false;
                        break;
                    }
                }
            }
            if all_ok {
                status = Status::Verified;
            } else {
                evidence.clear();
            }
        }

        let target = pages_dir(wiki_dir).join(format!("{}.md", f.id));
        let overwritten = target.exists();
        if overwritten && !force {
            report.skipped.push(SkippedFile {
                path: f.rel.clone(),
                reason: format!("page `{}` exists (use --force to overwrite)", f.id),
            });
            continue;
        }
        report.imported.push(ImportedPage {
            id: f.id.clone(),
            title: title.clone(),
            page_type: page_type.clone(),
            status,
            evidence: evidence.len(),
            overwritten,
        });
        if dry_run {
            continue;
        }
        std::fs::create_dir_all(target.parent().expect("pages dir")).map_err(WikiError::Io)?;
        let page = WikiPage {
            meta: PageMeta {
                id: f.id.clone(),
                title,
                status,
                generated_at: now_iso(),
                commit: head_commit.clone(),
                evidence,
                tags,
                page_type,
            },
            body: body.into(),
            source_path: Some(target),
        };
        page.save_in_place()?;
    }

    // 3. register unknown imported types so `lint` does not flag them
    if !new_types.is_empty() && !dry_run {
        register_types(wiki_dir, &new_types)?;
    }
    report.registered_types = new_types.keys().cloned().collect();
    Ok(report)
}

/// slash-separated relative path inside src, rejecting anything odd.
fn rel_path(src: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(src)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

fn make_id(stem: &str, prefix: Option<&str>) -> String {
    let segs: Vec<String> = stem
        .split('/')
        .map(sanitize_segment)
        .filter(|s| !s.is_empty())
        .collect();
    let joined = segs.join("/");
    match prefix {
        Some(p) => {
            let p = sanitize_segment(p);
            if p.is_empty() {
                joined
            } else {
                format!("{p}/{joined}")
            }
        }
        None => joined,
    }
}

fn sanitize_segment(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.trim().chars() {
        if c == '.' && out.is_empty() {
            continue; // no dotfiles / hidden segments as ids
        }
        if c.is_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// `---` fenced YAML frontmatter, lenient: absent or unparseable -> None,
/// body is then everything (minus the fence when it looked like one).
fn split_frontmatter(text: &str) -> (Option<serde_yaml::Value>, String) {
    let trimmed = text.trim_start_matches('\u{feff}');
    let Some(rest) = trimmed.strip_prefix("---\n") else {
        return (None, text.to_string());
    };
    let Some(end) = rest.find("\n---") else {
        return (None, text.to_string());
    };
    let yaml = &rest[..end];
    let mut body = rest[end + 4..].to_string();
    if let Some(b) = body.strip_prefix('\n') {
        body = b.to_string();
    }
    let value = serde_yaml::from_str::<serde_yaml::Value>(yaml).ok();
    (value, body)
}

fn title_from_body_or_stem(body: &str, rel: &str) -> String {
    for line in body.lines() {
        if let Some(h) = line.strip_prefix("# ") {
            let h = h.trim();
            if !h.is_empty() {
                return h.to_string();
            }
        }
    }
    rel.trim_end_matches(".md")
        .rsplit('/')
        .next()
        .unwrap_or(rel)
        .to_string()
}

/// Convert relative markdown links `](foo.md)` that resolve to an imported
/// bundle file into loredex cross-page links `[[wiki:id]]`.
fn rewrite_md_links(body: &str, rel: &str, rel_to_id: &BTreeMap<String, String>) -> String {
    static MD_LINK_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\[([^\]]*)\]\(([^)\s]+\.md)(?:#[^)]*)?\)")
            .expect("static markdown link regex")
    });
    let re = &*MD_LINK_RE;
    let dir = rel
        .rsplit_once('/')
        .map(|(d, _)| d.to_string())
        .unwrap_or_default();
    let mut out = String::new();
    let mut last = 0;
    for caps in re.captures_iter(body) {
        let m = caps.get(0).unwrap();
        let target = normalize_target(&caps[2], &dir);
        if let Some(id) = rel_to_id.get(&target) {
            out.push_str(&body[last..m.start()]);
            out.push_str(&format!("[[wiki:{id}]]"));
            last = m.end();
        }
    }
    out.push_str(&body[last..]);
    out
}

/// Lexical resolution of a markdown link target against the linking file's
/// directory; `/`-leading targets are bundle-root relative.
fn normalize_target(link: &str, cur_dir: &str) -> String {
    let link = link.trim_start_matches("./");
    let joined = if link.starts_with('/') {
        link.trim_start_matches('/').to_string()
    } else if cur_dir.is_empty() {
        link.to_string()
    } else {
        format!("{cur_dir}/{link}")
    };
    let mut stack: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

/// Append `[page_types.<t>]` stubs to wiki.toml so imported types stay
/// lint-clean while carrying an honest "imported" provenance note.
fn register_types(wiki_dir: &Path, types: &BTreeMap<String, String>) -> Result<()> {
    let path = wiki_dir.join("wiki.toml");
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for t in types.keys() {
        text.push_str(&format!(
            "\n[page_types.\"{t}\"]\ndescription = \"imported page type (from openwiki bundle)\"\n"
        ));
    }
    std::fs::write(&path, text).map_err(WikiError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_sanitized() {
        assert_eq!(make_id("modules/core-api", None), "modules/core-api");
        assert_eq!(make_id("My Module v2", None), "My-Module-v2");
        assert_eq!(make_id("a", Some("ow")), "ow/a");
        assert_eq!(sanitize_segment(".."), "");
    }

    #[test]
    fn link_targets_normalize() {
        assert_eq!(
            normalize_target("./architecture.md", "modules"),
            "modules/architecture.md"
        );
        assert_eq!(normalize_target("/index.md", "modules"), "index.md");
        assert_eq!(normalize_target("../other/x.md", "a/b"), "a/other/x.md");
    }

    #[test]
    fn frontmatter_is_lenient() {
        let (fm, body) = split_frontmatter("---\ntype: architecture\ntitle: X\n---\n# hello\n");
        assert_eq!(
            fm.unwrap().get("type").unwrap().as_str(),
            Some("architecture")
        );
        assert_eq!(body, "# hello\n");
        let (none, b) = split_frontmatter("# no fm\n\ntext");
        assert!(none.is_none());
        assert_eq!(b, "# no fm\n\ntext");
        let (bad, _) = split_frontmatter("---\n[unclosed\n---\nbody");
        assert!(bad.is_none());
    }
}
