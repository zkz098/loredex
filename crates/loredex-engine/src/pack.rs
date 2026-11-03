//! Context packs: a token-budgeted, trust-filtered bundle of the wiki pages
//! relevant to one module plus the source snippets they cite. This is the
//! read side of the evidence contract: an agent gets verified prose and the
//! actual code it points at in one artifact instead of paying for the whole
//! repository (or trusting stale Markdown).
//!
//! By default only `verified`, git-fresh pages are included; stale pages and
//! drafts are listed as skipped so the caller knows what was withheld and can
//! pass the matching flag to override. Every snippet is re-read from HEAD and
//! secret-shaped lines are masked before they reach the output.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use loredex_core::{Status, WikiPage};
use loredex_git::GitView;
use serde::Serialize;

use crate::evidence::in_module;
use crate::{check_page_with, index::is_generated, load_pages, prime_view, redact, Result};

#[derive(Debug, Clone, Serialize)]
pub struct PackOptions {
    /// Approximate token ceiling for the rendered markdown.
    pub budget_tokens: usize,
    /// Include pages whose evidence no longer matches HEAD (marked as stale).
    pub include_stale: bool,
    /// Include `draft` pages (explicitly unverified captures).
    pub include_drafts: bool,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            budget_tokens: 8_000,
            include_stale: false,
            include_drafts: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PackPage {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_type: Option<String>,
    pub status: String,
    pub stale_now: bool,
    pub evidence_count: usize,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackEvidence {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<[u32; 2]>,
    /// pages whose bodies cite this exact path/range
    pub pages: Vec<String>,
    pub estimated_tokens: usize,
    /// lines replaced by `redact::mask_line` because they matched a secret rule
    pub masked_lines: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackSkip {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextPack {
    pub module: String,
    pub head: String,
    pub budget_tokens: usize,
    pub estimated_tokens: usize,
    pub truncated: bool,
    pub pages: Vec<PackPage>,
    /// deduplicated snippets re-read from HEAD (path, line range is unique)
    pub evidence: Vec<PackEvidence>,
    pub skipped: Vec<PackSkip>,
    /// the artifact itself, ready to paste into a prompt
    pub markdown: String,
}

struct Selected {
    page: WikiPage,
    cited: usize,
    stale: bool,
}

/// Build a context pack for `module` (a module name from `modules`, or a
/// directory prefix; `.` means the whole repository).
pub fn context_pack(
    view: &GitView,
    wiki_dir: &Path,
    module: &str,
    opts: &PackOptions,
) -> Result<ContextPack> {
    let pages = load_pages(wiki_dir)?;
    prime_view(view, &pages)?;
    let head = view.head_commit()?.to_string();
    let budget = opts.budget_tokens.max(16);

    let mut selected = Vec::new();
    let mut skipped = Vec::new();
    for page in pages {
        if is_generated(&page.meta.id) {
            continue;
        }
        let cited = page
            .meta
            .evidence
            .iter()
            .filter(|e| in_module(&e.path, module))
            .count();
        if cited == 0 {
            continue;
        }
        let report = check_page_with(&page, view)?;
        let stale = report.is_stale || page.meta.status == Status::Stale;
        if stale && !opts.include_stale {
            skipped.push(PackSkip {
                id: page.meta.id.clone(),
                reason: "stale evidence: run loredex auto/refresh, or pass include_stale".into(),
            });
            continue;
        }
        if page.meta.status == Status::Draft && !opts.include_drafts {
            skipped.push(PackSkip {
                id: page.meta.id.clone(),
                reason: "status `draft`: unverified capture, or pass include_drafts".into(),
            });
            continue;
        }
        selected.push(Selected { page, cited, stale });
    }
    // Most-cited pages first: for a module, they are the closest thing the
    // wiki has to a primary explanation.
    selected.sort_by(|a, b| {
        b.cited
            .cmp(&a.cited)
            .then(a.page.meta.id.cmp(&b.page.meta.id))
    });

    let mut markdown = String::new();
    let mut remaining = budget;
    let mut truncated = false;
    let stale_included = selected.iter().filter(|s| s.stale).count();
    let draft_included = selected
        .iter()
        .filter(|s| s.page.meta.status != Status::Verified)
        .count();

    let mut header = format!(
        "# Context Pack: {module}\n\nEvidence-backed wiki context at HEAD {}, budget ~{budget} tokens.\n",
        short(&head)
    );
    if stale_included > 0 {
        header.push_str(&format!(
            "\n> warning: {stale_included} stale page(s) included on request; their claims are UNVERIFIED.\n"
        ));
    }
    if draft_included > 0 {
        header.push_str(&format!(
            "\n> warning: {draft_included} draft page(s) included; their claims are UNVERIFIED.\n"
        ));
    }
    append_budgeted(&mut markdown, &mut remaining, &mut truncated, &header);

    if !selected.is_empty() {
        append_budgeted(
            &mut markdown,
            &mut remaining,
            &mut truncated,
            "\n## Pages\n",
        );
    }

    let mut included_ids: BTreeSet<String> = BTreeSet::new();
    let mut pack_pages = Vec::new();
    for (index, sel) in selected.iter().enumerate() {
        if remaining == 0 {
            skipped.push(PackSkip {
                id: sel.page.meta.id.clone(),
                reason: "token budget exhausted".into(),
            });
            continue;
        }
        let page_type = sel.page.meta.page_type.as_deref().unwrap_or("unclassified");
        let stale_label = if sel.stale {
            " · STALE (included on request)"
        } else {
            ""
        };
        let header = format!(
            "\n### `{}` — {}\n_{page_type} · {} evidence{stale_label}_\n\n",
            sel.page.meta.id,
            sel.page.meta.title,
            sel.page.meta.evidence.len(),
        );
        let header_tokens = estimate_tokens(&header);
        if header_tokens > remaining {
            truncated = true;
            skipped.push(PackSkip {
                id: sel.page.meta.id.clone(),
                reason: "token budget exhausted".into(),
            });
            break;
        }
        markdown.push_str(&header);
        remaining -= header_tokens;

        let body = sel.page.body.trim();
        let body_block = if body.is_empty() {
            String::new()
        } else {
            format!("{body}\n")
        };
        let body_tokens = estimate_tokens(&body_block);
        let included_body = if body_tokens <= remaining {
            remaining -= body_tokens;
            body_block.clone()
        } else {
            let (part, _) = truncate_to_tokens(&body_block, remaining);
            remaining = 0;
            truncated = true;
            part
        };
        markdown.push_str(&included_body);
        included_ids.insert(sel.page.meta.id.clone());
        pack_pages.push(PackPage {
            id: sel.page.meta.id.clone(),
            title: sel.page.meta.title.clone(),
            page_type: sel.page.meta.page_type.clone(),
            status: sel.page.meta.status.as_str().to_string(),
            stale_now: sel.stale,
            evidence_count: sel.page.meta.evidence.len(),
            estimated_tokens: header_tokens + estimate_tokens(&included_body),
        });
        if truncated && remaining == 0 {
            for rest in selected.iter().skip(index + 1) {
                skipped.push(PackSkip {
                    id: rest.page.meta.id.clone(),
                    reason: "token budget exhausted".into(),
                });
            }
            break;
        }
    }

    // Gather evidence only for pages that actually made it into the pack, so
    // the snippet appendix never describes prose the reader did not receive.
    let mut map: BTreeMap<(String, Option<[u32; 2]>), EvidenceAgg> = BTreeMap::new();
    for sel in &selected {
        if !included_ids.contains(&sel.page.meta.id) {
            continue;
        }
        for e in &sel.page.meta.evidence {
            if !in_module(&e.path, module) {
                continue;
            }
            let agg = map.entry((e.path.clone(), e.lines)).or_insert_with(|| {
                let content = e
                    .lines
                    .and_then(|_| view.head_file_content(&e.path).ok().flatten());
                let (snippet, secret_lines) = match (e.lines, content) {
                    (Some([lo, hi]), Some(content)) => (
                        view.head_lines(&e.path, lo, hi).ok().flatten(),
                        redact::scan_range(&content, lo, hi)
                            .into_iter()
                            .map(|f| f.line)
                            .collect(),
                    ),
                    _ => (None, BTreeSet::new()),
                };
                EvidenceAgg {
                    pages: BTreeSet::new(),
                    snippet,
                    secret_lines,
                }
            });
            agg.pages.insert(sel.page.meta.id.clone());
        }
    }

    if !map.is_empty() && remaining > 0 {
        append_budgeted(
            &mut markdown,
            &mut remaining,
            &mut truncated,
            "\n## Evidence\n",
        );
    }
    let mut pack_evidence = Vec::new();
    for ((path, lines), agg) in map {
        if remaining == 0 {
            skipped.push(PackSkip {
                id: format!("evidence:{path}"),
                reason: "token budget exhausted".into(),
            });
            continue;
        }
        let range = lines
            .map(|[lo, hi]| format!(":L{lo}-L{hi}"))
            .unwrap_or_default();
        let page_ids: Vec<String> = agg.pages.into_iter().collect();
        let cited_by = page_ids.join(", ");
        let (body, masked) = match (lines.is_some(), &agg.snippet) {
            (true, Some(snippet)) => render_snippet(snippet, &path, &agg.secret_lines),
            (true, None) => (
                "_cited lines are not available at HEAD; treat this evidence as unverified._\n"
                    .to_string(),
                0,
            ),
            (false, _) => (
                "_whole-file evidence; open the file at HEAD._\n".to_string(),
                0,
            ),
        };
        let block = format!("\n### `{path}`{range}\n_cited by: {cited_by}_\n\n{body}",);
        let block_tokens = estimate_tokens(&block);
        let included = if block_tokens <= remaining {
            remaining -= block_tokens;
            block
        } else {
            let (part, _) = truncate_to_tokens(&block, remaining);
            remaining = 0;
            truncated = true;
            part
        };
        let included_tokens = estimate_tokens(&included);
        markdown.push_str(&included);
        pack_evidence.push(PackEvidence {
            path,
            lines,
            pages: page_ids,
            estimated_tokens: included_tokens,
            masked_lines: masked,
        });
        if truncated && remaining == 0 {
            break;
        }
    }

    Ok(ContextPack {
        module: module.to_string(),
        head,
        budget_tokens: budget,
        estimated_tokens: estimate_tokens(&markdown),
        truncated,
        pages: pack_pages,
        evidence: pack_evidence,
        skipped,
        markdown,
    })
}

struct EvidenceAgg {
    pages: BTreeSet<String>,
    snippet: Option<String>,
    /// 1-based offsets within `snippet` flagged by `redact::scan_range`
    secret_lines: BTreeSet<usize>,
}

fn render_snippet(snippet: &str, path: &str, secret_lines: &BTreeSet<usize>) -> (String, usize) {
    let mut masked = 0usize;
    let mut lines = Vec::new();
    for (i, line) in snippet.lines().enumerate() {
        if !secret_lines.contains(&(i + 1)) {
            lines.push(line.to_string());
        } else {
            masked += 1;
            lines.push(redact::mask_line(line));
        }
    }
    (
        format!("```{}\n{}\n```\n", fence_lang(path), lines.join("\n")),
        masked,
    )
}

fn fence_lang(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or_default();
    match ext {
        "rs" => "rust",
        "ts" | "tsx" => "ts",
        "js" | "jsx" | "mjs" | "cjs" => "js",
        "py" => "python",
        "go" => "go",
        "toml" => "toml",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "md" => "markdown",
        "sh" | "bash" | "zsh" | "fish" => "bash",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" => "cpp",
        "java" => "java",
        "rb" => "ruby",
        "cs" => "csharp",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "sql" => "sql",
        "html" => "html",
        "css" => "css",
        _ => "",
    }
}

/// Approximate token count without a tokenizer: ~4 ASCII characters per
/// token, one token per non-ASCII character. Deliberately conservative for
/// code (which is mostly ASCII) and usable for CJK prose.
pub fn estimate_tokens(text: &str) -> usize {
    let mut ascii = 0usize;
    let mut other = 0usize;
    for c in text.chars() {
        if c.is_ascii() {
            ascii += 1;
        } else {
            other += 1;
        }
    }
    ascii.div_ceil(4) + other
}

fn append_budgeted(out: &mut String, remaining: &mut usize, truncated: &mut bool, block: &str) {
    let tokens = estimate_tokens(block);
    if tokens <= *remaining {
        out.push_str(block);
        *remaining -= tokens;
    } else {
        let (part, _) = truncate_to_tokens(block, *remaining);
        out.push_str(&part);
        *remaining = 0;
        *truncated = true;
    }
}

fn truncate_to_tokens(text: &str, max_tokens: usize) -> (String, bool) {
    if estimate_tokens(text) <= max_tokens {
        return (text.to_string(), false);
    }
    if max_tokens == 0 {
        return (String::new(), true);
    }
    let mut ascii = 0usize;
    let mut other = 0usize;
    let mut end = 0usize;
    for (idx, c) in text.char_indices() {
        let next_ascii = ascii + usize::from(c.is_ascii());
        let next_other = other + usize::from(!c.is_ascii());
        if next_ascii.div_ceil(4) + next_other > max_tokens {
            break;
        }
        ascii = next_ascii;
        other = next_other;
        end = idx + c.len_utf8();
    }
    (text[..end].to_string(), true)
}

fn short(sha: &str) -> String {
    sha.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimate_is_ascii_and_cjk_aware() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
        assert_eq!(estimate_tokens("中文"), 2);
        assert_eq!(estimate_tokens("ab中文"), 3);
    }

    #[test]
    fn truncation_respects_the_budget() {
        let text = "a".repeat(100);
        let (out, cut) = truncate_to_tokens(&text, 5);
        assert!(cut);
        assert!(estimate_tokens(&out) <= 5);
        assert_eq!(out.len(), 20);
    }
}
