//! Human-readable HTML preview and static site export for `.wiki/`.
//!
//! Zero external web dependencies: compiles Markdown + `[[cite:path:Lx-Ly]]` +
//! `[[wiki:id]]` into self-contained HTML with interactive HEAD source
//! snippets (secret-masked via [`crate::redact::mask_line`]), live git
//! freshness badges, backlinks, and instant search.
//!
//! Two faces:
//! - [`build_html_with`]: write a static site directory (ready for `file://`,
//!   GitHub Pages, or CI artifacts).
//! - [`serve_http`]: local live preview server over `std::net::TcpListener`
//!   that re-reads `.wiki/` and git HEAD on refresh and exposes `/api/search`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use loredex_core::search::{IndexedPage, InvertedIndex};
use loredex_core::{
    dedup_citations, parse_citations, parse_wiki_links, Citation, Evidence, Status, WikiError,
    WikiPage,
};
use loredex_git::{GitRepo, GitView};
use regex::Regex;
use serde::Serialize;

use crate::config::{locale, Locale};
use crate::{check_page_with, discover_with, load_pages, prime_view, redact, Result};

const MAX_WHOLE_FILE_PREVIEW_LINES: usize = 50;

static INLINE_TOKEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"`([^`]+)`|\[\[cite:([^\]\s#]+?)(?:#([^\]\s]+?))?(?::L(\d+)-L(\d+))?\]\]|\[\[wiki:([A-Za-z0-9_\-./]+)\]\]|\[([^\]]+)\]\(([^)\s]+)\)|\*\*([^*]+)\*\*|\*([^*\n]+)\*",
    )
    .expect("static inline markdown regex")
});

#[derive(Debug, Clone, Serialize)]
pub struct HtmlReport {
    pub out_dir: PathBuf,
    pub pages_written: usize,
    pub stale_pages: usize,
    pub index_file: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RenderedSite {
    pub files: BTreeMap<String, String>,
    pub pages_written: usize,
    pub stale_pages: usize,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum EffectiveStatus {
    Verified,
    Stale,
    Draft,
}

impl EffectiveStatus {
    fn as_str(self) -> &'static str {
        match self {
            EffectiveStatus::Verified => "verified",
            EffectiveStatus::Stale => "stale",
            EffectiveStatus::Draft => "draft",
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedEvidence {
    index: usize, // 1-based
    path: String,
    symbol: Option<String>,
    lines: Option<[u32; 2]>,
    blob: String,
    snippet_sha1: Option<String>,
    note: Option<String>,
    ok: bool,
    detail: String,
    snippet_lines: Vec<(u32, String)>,
}

#[derive(Debug, Clone)]
struct PageEntry {
    page: WikiPage,
    effective: EffectiveStatus,
    stale_since: Option<String>,
    evidences: Vec<ResolvedEvidence>,
    backlinks: Vec<(String, String)>, // (id, title)
}

#[derive(Debug, Clone, Serialize)]
struct SearchDoc {
    id: String,
    title: String,
    status: String,
    page_type: String,
    tags: Vec<String>,
    href: String,
    excerpt: String,
}

struct UiLabels {
    overview: &'static str,
    search_placeholder: &'static str,
    pages_label: &'static str,
    verified_label: &'static str,
    stale_label: &'static str,
    draft_label: &'static str,
    modules_label: &'static str,
    evidence_heading: &'static str,
    backlinks_heading: &'static str,
    no_backlinks: &'static str,
    stale_banner_title: &'static str,
    stale_banner_hint: &'static str,
    generated_at_label: &'static str,
    commit_label: &'static str,
    type_label: &'static str,
    catalog_heading: &'static str,
    modules_heading: &'static str,
}

const LABELS_EN: UiLabels = UiLabels {
    overview: "Overview",
    search_placeholder: "Search pages, symbols, citations… (press /)",
    pages_label: "Total Pages",
    verified_label: "Verified",
    stale_label: "Stale",
    draft_label: "Draft",
    modules_label: "Modules",
    evidence_heading: "Pinned Source Evidence (HEAD)",
    backlinks_heading: "Backlinks",
    no_backlinks: "No other wiki pages link to this page yet.",
    stale_banner_title: "This page is stale against current git HEAD.",
    stale_banner_hint: "Run `loredex auto` or `loredex refresh <id>` to project drifted line ranges, or re-verify the source.",
    generated_at_label: "Recorded",
    commit_label: "Base commit",
    type_label: "Type",
    catalog_heading: "Wiki Pages",
    modules_heading: "Repository Modules & Coverage",
};

const LABELS_ZH: UiLabels = UiLabels {
    overview: "总览首页",
    search_placeholder: "搜索页面、符号、引用代码… (按 / 聚焦)",
    pages_label: "页面总数",
    verified_label: "已验证 (Verified)",
    stale_label: "已过期 (Stale)",
    draft_label: "草稿 (Draft)",
    modules_label: "代码模块",
    evidence_heading: "源码证据锚点 (当前 HEAD)",
    backlinks_heading: "反向链接 (Backlinks)",
    no_backlinks: "暂无其他 Wiki 页面链接到本页。",
    stale_banner_title: "当前页面相对于 Git HEAD 已过期 (Stale)。",
    stale_banner_hint:
        "请运行 `loredex auto` 或 `loredex refresh <id>` 自动投影漂移行号，或重新核查源码。",
    generated_at_label: "记录时间",
    commit_label: "基准提交",
    type_label: "类型",
    catalog_heading: "Wiki 页面目录",
    modules_heading: "仓库模块与文档覆盖",
};

fn labels(loc: Locale) -> &'static UiLabels {
    match loc {
        Locale::En => &LABELS_EN,
        Locale::Zh => &LABELS_ZH,
    }
}

/// Write a static HTML preview site into `out_dir`.
pub fn build_html(repo: &GitRepo, wiki_dir: &Path, out_dir: &Path) -> Result<HtmlReport> {
    build_html_with(&repo.view(), wiki_dir, out_dir)
}

/// Write a static HTML preview site into `out_dir` using a shared [`GitView`].
pub fn build_html_with(view: &GitView, wiki_dir: &Path, out_dir: &Path) -> Result<HtmlReport> {
    let site = render_site(view, wiki_dir)?;
    std::fs::create_dir_all(out_dir).map_err(WikiError::Io)?;
    for (rel, content) in &site.files {
        let dest = out_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(WikiError::Io)?;
        }
        std::fs::write(&dest, content).map_err(WikiError::Io)?;
    }
    Ok(HtmlReport {
        out_dir: out_dir.to_path_buf(),
        pages_written: site.pages_written,
        stale_pages: site.stale_pages,
        index_file: out_dir.join("index.html"),
    })
}

/// Render all HTML pages, assets, and search index in memory.
pub fn render_site(view: &GitView, wiki_dir: &Path) -> Result<RenderedSite> {
    let loc = locale(wiki_dir);
    let ui = labels(loc);
    let raw_pages = load_pages(wiki_dir)?;
    prime_view(view, &raw_pages)?;

    let title_by_id: BTreeMap<String, String> = raw_pages
        .iter()
        .map(|p| (p.meta.id.clone(), p.meta.title.clone()))
        .collect();

    // Compute backlinks: target_id -> Vec<(from_id, from_title)>
    let mut backlinks_map: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for p in &raw_pages {
        let mut seen = BTreeSet::new();
        for target in parse_wiki_links(&p.body) {
            if target != p.meta.id && seen.insert(target.clone()) {
                backlinks_map
                    .entry(target)
                    .or_default()
                    .push((p.meta.id.clone(), p.meta.title.clone()));
            }
        }
    }

    let mut entries = Vec::new();
    let mut stale_pages = 0usize;

    for p in raw_pages {
        let report = check_page_with(&p, view)?;
        let is_gen = crate::index::is_generated(&p.meta.id);
        let effective = if is_gen {
            EffectiveStatus::Verified
        } else if report.is_stale || p.meta.status == Status::Stale {
            stale_pages += 1;
            EffectiveStatus::Stale
        } else if p.meta.status == Status::Draft {
            EffectiveStatus::Draft
        } else {
            EffectiveStatus::Verified
        };

        let mut evidences = Vec::new();
        for (idx, ev) in p.meta.evidence.iter().enumerate() {
            let rep = report.evidence_reports.get(idx);
            let ok = rep.map(|r| r.ok).unwrap_or(false);
            let detail = rep
                .map(|r| r.detail.clone())
                .unwrap_or_else(|| "unverified".into());
            let snippet_lines = load_masked_snippet(view, ev);
            evidences.push(ResolvedEvidence {
                index: idx + 1,
                path: ev.path.clone(),
                symbol: ev.symbol.clone(),
                lines: ev.lines,
                blob: ev.blob.clone(),
                snippet_sha1: ev.snippet_sha1.clone(),
                note: ev.note.clone(),
                ok,
                detail,
                snippet_lines,
            });
        }

        let backlinks = backlinks_map.remove(&p.meta.id).unwrap_or_default();

        entries.push(PageEntry {
            page: p,
            effective,
            stale_since: report.stale_since,
            evidences,
            backlinks,
        });
    }

    let status_by_id: BTreeMap<String, EffectiveStatus> = entries
        .iter()
        .map(|e| (e.page.meta.id.clone(), e.effective))
        .collect();

    let repo_name = view
        .workdir()
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".into());
    let head_short = view.head_short().unwrap_or_else(|_| "unknown".into());
    let modules = discover_with(view).unwrap_or_default();

    let mut files = BTreeMap::new();
    files.insert("assets/style.css".to_string(), STYLE_CSS.to_string());
    files.insert("assets/app.js".to_string(), APP_JS.to_string());

    let search_docs: Vec<SearchDoc> = entries
        .iter()
        .map(|e| SearchDoc {
            id: e.page.meta.id.clone(),
            title: e.page.meta.title.clone(),
            status: e.effective.as_str().to_string(),
            page_type: e
                .page
                .meta
                .page_type
                .clone()
                .unwrap_or_else(|| "other".into()),
            tags: e.page.meta.tags.clone(),
            href: format!("pages/{}.html", e.page.meta.id),
            excerpt: make_excerpt(&e.page.body),
        })
        .collect();
    files.insert(
        "search-index.json".to_string(),
        serde_json::to_string(&search_docs).unwrap_or_else(|_| "[]".into()),
    );

    // Render each page into `pages/<id>.html` so `index.html` at root never
    // collides with `.wiki/pages/index.md` (`pages/index.html`).
    for entry in &entries {
        let rel_path = format!("pages/{}.html", entry.page.meta.id);
        let root_prefix = relative_root_prefix(&rel_path);
        let sidebar = render_sidebar(
            &repo_name,
            &head_short,
            &entries,
            Some(&entry.page.meta.id),
            &root_prefix,
            ui,
        );
        let main_html = render_page_main(entry, &title_by_id, &status_by_id, &root_prefix, ui);
        let doc = wrap_document(
            &format!("{} — {} wiki", entry.page.meta.title, repo_name),
            &root_prefix,
            &sidebar,
            &main_html,
        );
        files.insert(rel_path, doc);
    }

    // Render overview dashboard at `index.html`
    let root_sidebar = render_sidebar(&repo_name, &head_short, &entries, None, "", ui);
    let overview_main = render_overview_main(
        &repo_name,
        &head_short,
        &entries,
        &modules,
        &title_by_id,
        &status_by_id,
        ui,
    );
    let index_doc = wrap_document(
        &format!("{repo_name} — Loredex Wiki"),
        "",
        &root_sidebar,
        &overview_main,
    );
    files.insert("index.html".to_string(), index_doc);

    Ok(RenderedSite {
        files,
        pages_written: entries.len(),
        stale_pages,
    })
}

fn load_masked_snippet(view: &GitView, ev: &Evidence) -> Vec<(u32, String)> {
    match ev.lines {
        Some([lo, hi]) => match view.head_lines(&ev.path, lo, hi) {
            Ok(Some(text)) => text
                .lines()
                .enumerate()
                .map(|(i, line)| (lo + i as u32, redact::mask_line(line)))
                .collect(),
            _ => Vec::new(),
        },
        None => match view.head_file_content(&ev.path) {
            Ok(Some(text)) => text
                .lines()
                .take(MAX_WHOLE_FILE_PREVIEW_LINES)
                .enumerate()
                .map(|(i, line)| (i as u32 + 1, redact::mask_line(line)))
                .collect(),
            _ => Vec::new(),
        },
    }
}

fn relative_root_prefix(rel_path: &str) -> String {
    let depth = rel_path.matches('/').count();
    "../".repeat(depth)
}

fn make_excerpt(body: &str) -> String {
    let mut out = String::new();
    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with("```") {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(t);
        if out.len() >= 220 {
            break;
        }
    }
    out.chars().take(220).collect()
}

fn wrap_document(title: &str, root_prefix: &str, sidebar: &str, main_html: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <link rel="stylesheet" href="{root_prefix}assets/style.css">
</head>
<body data-root="{root_prefix}">
  <div class="layout">
    {sidebar}
    <main class="content">
      {main_html}
    </main>
  </div>
  <script src="{root_prefix}assets/app.js"></script>
</body>
</html>
"#,
        title = escape_html(title),
        root_prefix = root_prefix,
        sidebar = sidebar,
        main_html = main_html,
    )
}

fn render_sidebar(
    repo_name: &str,
    head_short: &str,
    entries: &[PageEntry],
    active_id: Option<&str>,
    root_prefix: &str,
    ui: &UiLabels,
) -> String {
    let mut groups: BTreeMap<String, Vec<&PageEntry>> = BTreeMap::new();
    for e in entries {
        let pt = e
            .page
            .meta
            .page_type
            .clone()
            .unwrap_or_else(|| "other".into());
        groups.entry(pt).or_default().push(e);
    }

    let order = [
        "index",
        "architecture",
        "system",
        "workflow",
        "integration",
        "testing",
        "rule",
        "decision",
        "troubleshooting",
        "other",
    ];
    let mut group_keys: Vec<String> = Vec::new();
    for &k in &order {
        if groups.contains_key(k) {
            group_keys.push(k.to_string());
        }
    }
    for k in groups.keys() {
        if !group_keys.contains(k) {
            group_keys.push(k.clone());
        }
    }

    let mut nav_html = String::new();
    let overview_active = if active_id.is_none() { " active" } else { "" };
    nav_html.push_str(&format!(
        r#"<a class="nav-item nav-overview{overview_active}" href="{root_prefix}index.html">
          <span class="status-dot status-verified"></span>
          <span class="nav-title">{overview}</span>
        </a>"#,
        overview_active = overview_active,
        root_prefix = root_prefix,
        overview = escape_html(ui.overview),
    ));

    for group in group_keys {
        let Some(items) = groups.get(&group) else {
            continue;
        };
        nav_html.push_str(&format!(
            r#"<div class="nav-group"><div class="nav-group-title">{}</div>"#,
            escape_html(&group)
        ));
        for item in items {
            let active = if active_id == Some(item.page.meta.id.as_str()) {
                " active"
            } else {
                ""
            };
            let st = item.effective.as_str();
            nav_html.push_str(&format!(
                r#"<a class="nav-item{active}" href="{root_prefix}pages/{id}.html" title="{id} ({st})">
                  <span class="status-dot status-{st}"></span>
                  <span class="nav-title">{title}</span>
                </a>"#,
                active = active,
                root_prefix = root_prefix,
                id = escape_html(&item.page.meta.id),
                st = st,
                title = escape_html(&item.page.meta.title),
            ));
        }
        nav_html.push_str("</div>");
    }

    format!(
        r#"<aside class="sidebar">
      <div class="sidebar-header">
        <div class="brand">
          <a href="{root_prefix}index.html" class="brand-name">{repo_name}</a>
          <span class="head-badge" title="Current git HEAD">@{head_short}</span>
        </div>
        <button id="theme-toggle" class="theme-btn" type="button" title="Toggle light/dark theme">◐</button>
      </div>
      <div class="search-box">
        <input id="wiki-search" type="search" placeholder="{search_placeholder}" autocomplete="off">
        <div id="search-results" class="search-results" hidden></div>
      </div>
      <nav class="sidebar-nav">
        {nav_html}
      </nav>
    </aside>"#,
        root_prefix = root_prefix,
        repo_name = escape_html(repo_name),
        head_short = escape_html(head_short),
        search_placeholder = escape_html(ui.search_placeholder),
        nav_html = nav_html,
    )
}

fn render_overview_main(
    repo_name: &str,
    head_short: &str,
    entries: &[PageEntry],
    modules: &[crate::ModuleInfo],
    title_by_id: &BTreeMap<String, String>,
    status_by_id: &BTreeMap<String, EffectiveStatus>,
    ui: &UiLabels,
) -> String {
    let non_gen: Vec<&PageEntry> = entries
        .iter()
        .filter(|e| !crate::index::is_generated(&e.page.meta.id))
        .collect();
    let total = non_gen.len();
    let verified = non_gen
        .iter()
        .filter(|e| e.effective == EffectiveStatus::Verified)
        .count();
    let stale = non_gen
        .iter()
        .filter(|e| e.effective == EffectiveStatus::Stale)
        .count();
    let draft = non_gen
        .iter()
        .filter(|e| e.effective == EffectiveStatus::Draft)
        .count();

    let mut out = format!(
        r#"<header class="page-header">
        <div class="page-title-row">
          <h1>{repo_name} Wiki</h1>
          <span class="badge status-badge status-verified">HEAD {head_short}</span>
        </div>
      </header>
      <section class="stats-grid">
        <div class="stat-card">
          <div class="stat-value">{total}</div>
          <div class="stat-label">{pages_label}</div>
        </div>
        <div class="stat-card stat-verified">
          <div class="stat-value">{verified}</div>
          <div class="stat-label">{verified_label}</div>
        </div>
        <div class="stat-card stat-stale">
          <div class="stat-value">{stale}</div>
          <div class="stat-label">{stale_label}</div>
        </div>
        <div class="stat-card stat-draft">
          <div class="stat-value">{draft}</div>
          <div class="stat-label">{draft_label}</div>
        </div>
        <div class="stat-card">
          <div class="stat-value">{mod_count}</div>
          <div class="stat-label">{modules_label}</div>
        </div>
      </section>"#,
        repo_name = escape_html(repo_name),
        head_short = escape_html(head_short),
        total = total,
        verified = verified,
        stale = stale,
        draft = draft,
        mod_count = modules.len(),
        pages_label = escape_html(ui.pages_label),
        verified_label = escape_html(ui.verified_label),
        stale_label = escape_html(ui.stale_label),
        draft_label = escape_html(ui.draft_label),
        modules_label = escape_html(ui.modules_label),
    );

    // If quickstart exists, render its body right on the overview page
    if let Some(qs) = entries
        .iter()
        .find(|e| e.page.meta.id == crate::index::QUICKSTART_ID)
    {
        let qs_html = render_markdown(&qs.page.body, &qs.evidences, title_by_id, status_by_id, "");
        out.push_str(&format!(
            r#"<section class="card quickstart-card">{qs_html}</section>"#
        ));
    }

    // Catalog cards
    out.push_str(&format!(
        r#"<section class="catalog-section"><h2>{}</h2><div class="catalog-grid">"#,
        escape_html(ui.catalog_heading)
    ));
    for e in &non_gen {
        let st = e.effective.as_str();
        let pt = e.page.meta.page_type.as_deref().unwrap_or("page");
        let commit_short: String = e.page.meta.commit.chars().take(8).collect();
        out.push_str(&format!(
            r#"<a class="catalog-card" href="pages/{id}.html">
              <div class="catalog-card-top">
                <span class="badge status-badge status-{st}">{st}</span>
                <span class="badge type-badge">{pt}</span>
                <span class="catalog-ev-count">{ev_len} cite(s)</span>
              </div>
              <div class="catalog-title">{title}</div>
              <div class="catalog-id">{id} · @{commit_short}</div>
            </a>"#,
            id = escape_html(&e.page.meta.id),
            st = st,
            pt = escape_html(pt),
            ev_len = e.evidences.len(),
            title = escape_html(&e.page.meta.title),
            commit_short = escape_html(&commit_short),
        ));
    }
    out.push_str("</div></section>");

    // Module coverage table
    if !modules.is_empty() {
        out.push_str(&format!(
            r#"<section class="modules-section"><h2>{}</h2><table class="md-table">
            <thead><tr><th>Module</th><th>Kind</th><th>Files</th><th>Citing Pages</th></tr></thead><tbody>"#,
            escape_html(ui.modules_heading)
        ));
        for m in modules {
            let citing: Vec<String> = non_gen
                .iter()
                .filter(|e| {
                    e.evidences
                        .iter()
                        .any(|ev| crate::evidence::in_module(&ev.path, &m.name))
                })
                .map(|e| {
                    format!(
                        r#"<a class="wiki-link status-{}" href="pages/{}.html">{}</a>"#,
                        e.effective.as_str(),
                        escape_html(&e.page.meta.id),
                        escape_html(&e.page.meta.id)
                    )
                })
                .collect();
            let citing_cell = if citing.is_empty() {
                r#"<span class="muted">uncovered</span>"#.to_string()
            } else {
                citing.join(" ")
            };
            out.push_str(&format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&m.name),
                escape_html(&m.kind),
                m.files.len(),
                citing_cell
            ));
        }
        out.push_str("</tbody></table></section>");
    }

    out
}

fn render_page_main(
    entry: &PageEntry,
    title_by_id: &BTreeMap<String, String>,
    status_by_id: &BTreeMap<String, EffectiveStatus>,
    root_prefix: &str,
    ui: &UiLabels,
) -> String {
    let meta = &entry.page.meta;
    let st = entry.effective.as_str();
    let commit_short: String = meta.commit.chars().take(8).collect();
    let pt = meta.page_type.as_deref().unwrap_or("untyped");

    let tags_html: String = meta
        .tags
        .iter()
        .map(|t| {
            format!(
                r#"<span class="badge tag-badge">#{}</span>"#,
                escape_html(t)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");

    let stale_banner = if entry.effective == EffectiveStatus::Stale {
        let stale_items: Vec<String> = entry
            .evidences
            .iter()
            .filter(|e| !e.ok)
            .map(|e| {
                format!(
                    "<li><code>{}</code>: {}</li>",
                    escape_html(&e.path),
                    escape_html(&e.detail)
                )
            })
            .collect();
        let list = if stale_items.is_empty() {
            String::new()
        } else {
            format!("<ul>{}</ul>", stale_items.join(""))
        };
        format!(
            r#"<div class="stale-banner">
              <strong>⚠ {title}</strong>
              <p>{hint}</p>
              {list}
            </div>"#,
            title = escape_html(ui.stale_banner_title),
            hint = escape_html(ui.stale_banner_hint),
            list = list,
        )
    } else {
        String::new()
    };

    let body_html = render_markdown(
        &entry.page.body,
        &entry.evidences,
        title_by_id,
        status_by_id,
        root_prefix,
    );

    let mut ev_section = String::new();
    if !entry.evidences.is_empty() {
        ev_section.push_str(&format!(
            r#"<section id="evidence" class="evidence-section">
            <h2>{} ({})</h2>"#,
            escape_html(ui.evidence_heading),
            entry.evidences.len()
        ));
        for ev in &entry.evidences {
            let sym_label = match &ev.symbol {
                Some(s) => format!("#{s}"),
                None => String::new(),
            };
            let range_label = match ev.lines {
                Some([lo, hi]) => format!(":L{lo}-L{hi}"),
                None => String::new(),
            };
            let ok_class = if ev.ok { "ev-ok" } else { "ev-stale" };
            let blob_short: String = ev.blob.chars().take(8).collect();
            let sha_short: String = ev
                .snippet_sha1
                .as_deref()
                .unwrap_or("-")
                .chars()
                .take(8)
                .collect();
            let note_html = match &ev.note {
                Some(n) => format!(r#"<div class="ev-note">↻ {}</div>"#, escape_html(n)),
                None => String::new(),
            };
            let mut code_rows = String::new();
            if ev.snippet_lines.is_empty() {
                code_rows.push_str(
                    r#"<div class="ev-empty">(source snippet unavailable at current HEAD)</div>"#,
                );
            } else {
                code_rows.push_str(r#"<pre class="ev-code"><code>"#);
                for (ln, text) in &ev.snippet_lines {
                    code_rows.push_str(&format!(
                        r#"<span class="ev-line"><span class="ev-ln">{ln:>4}</span><span class="ev-src">{src}</span></span>
"#,
                        ln = ln,
                        src = escape_html(text)
                    ));
                }
                code_rows.push_str("</code></pre>");
            }

            ev_section.push_str(&format!(
                r#"<details id="ev-{idx}" class="ev-card {ok_class}" open>
                  <summary class="ev-summary">
                    <span class="ev-idx">#{idx}</span>
                    <code class="ev-path">{path}{sym}{range}</code>
                    <span class="ev-verdict">{detail}</span>
                    <span class="ev-hashes">blob <code>{blob_short}</code> · sha1 <code>{sha_short}</code></span>
                  </summary>
                  {note_html}
                  {code_rows}
                </details>"#,
                idx = ev.index,
                ok_class = ok_class,
                path = escape_html(&ev.path),
                sym = escape_html(&sym_label),
                range = escape_html(&range_label),
                detail = escape_html(&ev.detail),
                blob_short = escape_html(&blob_short),
                sha_short = escape_html(&sha_short),
                note_html = note_html,
                code_rows = code_rows,
            ));
        }
        ev_section.push_str("</section>");
    }

    let mut backlinks_section = format!(
        r#"<section id="backlinks" class="backlinks-section"><h2>{}</h2>"#,
        escape_html(ui.backlinks_heading)
    );
    if entry.backlinks.is_empty() {
        backlinks_section.push_str(&format!(
            r#"<p class="muted">{}</p>"#,
            escape_html(ui.no_backlinks)
        ));
    } else {
        backlinks_section.push_str(r#"<ul class="backlinks-list">"#);
        for (from_id, from_title) in &entry.backlinks {
            let from_st = status_by_id
                .get(from_id)
                .copied()
                .unwrap_or(EffectiveStatus::Verified)
                .as_str();
            backlinks_section.push_str(&format!(
                r#"<li><a class="wiki-link status-{from_st}" href="{root_prefix}pages/{from_id}.html">{from_id}</a> — {from_title}</li>"#,
                from_st = from_st,
                root_prefix = root_prefix,
                from_id = escape_html(from_id),
                from_title = escape_html(from_title),
            ));
        }
        backlinks_section.push_str("</ul>");
    }
    backlinks_section.push_str("</section>");

    let _ = entry.stale_since.as_deref();

    format!(
        r##"<article class="wiki-article">
          <header class="page-header">
            <div class="page-meta-top">
              <span class="badge status-badge status-{st}">{st}</span>
              <span class="badge type-badge">{type_label}: {pt}</span>
              {tags_html}
            </div>
            <h1 class="page-title">{title}</h1>
            <div class="page-meta-sub">
              <span><code>{id}</code></span>
              <span>· {commit_label}: <code>{commit_short}</code></span>
              <span>· {gen_label}: <code>{gen_at}</code></span>
              <span>· <a href="#evidence">{ev_count} evidence</a></span>
            </div>
          </header>
          {stale_banner}
          <div class="markdown-body">
            {body_html}
          </div>
          {ev_section}
          {backlinks_section}
        </article>"##,
        st = st,
        type_label = escape_html(ui.type_label),
        pt = escape_html(pt),
        tags_html = tags_html,
        title = escape_html(&meta.title),
        id = escape_html(&meta.id),
        commit_label = escape_html(ui.commit_label),
        commit_short = escape_html(&commit_short),
        gen_label = escape_html(ui.generated_at_label),
        gen_at = escape_html(&meta.generated_at),
        ev_count = entry.evidences.len(),
        stale_banner = stale_banner,
        body_html = body_html,
        ev_section = ev_section,
        backlinks_section = backlinks_section,
    )
}

/// Match an inline `[[cite:path:Lx-Ly]]` marker in the body to its
/// corresponding [`ResolvedEvidence`] entry, accounting for drift projection
/// and rename recovery notes recorded in frontmatter.
fn match_citation<'a>(
    cite: &Citation,
    ordinal: Option<usize>,
    evidences: &'a [ResolvedEvidence],
) -> Option<&'a ResolvedEvidence> {
    // 1. Exact path + symbol + line range match
    if let Some(ev) = evidences
        .iter()
        .find(|e| e.path == cite.path && e.symbol == cite.symbol && e.lines == cite.lines)
    {
        return Some(ev);
    }
    // 1b. Symbol match on the same path (e.g. [[cite:path#symbol]] resolved to lines at write time)
    if let Some(sym) = &cite.symbol {
        if let Some(ev) = evidences.iter().find(|e| {
            e.path == cite.path
                && e.symbol
                    .as_deref()
                    .map(|es| es == sym || es.ends_with(&format!("::{sym}")))
                    .unwrap_or(false)
        }) {
            return Some(ev);
        }
    }
    // 2. Same path, note records drift projection from original L{lo}-L{hi}
    if let Some([lo, hi]) = cite.lines {
        let orig_tag = format!("L{lo}-L{hi}");
        if let Some(ev) = evidences.iter().find(|e| {
            e.path == cite.path
                && e.note
                    .as_deref()
                    .map(|n| n.contains(&orig_tag))
                    .unwrap_or(false)
        }) {
            return Some(ev);
        }
    }
    // 3. Ordinal match when deduped body citations align with frontmatter evidence
    if let Some(idx) = ordinal {
        if let Some(ev) = evidences.get(idx) {
            if ev.path == cite.path
                || ev
                    .note
                    .as_deref()
                    .map(|n| n.contains(&cite.path))
                    .unwrap_or(false)
            {
                return Some(ev);
            }
        }
    }
    // 4. Fallback to first evidence with the same path
    evidences.iter().find(|e| e.path == cite.path)
}

/// Convert wiki Markdown into HTML, transforming `[[cite:...]]` into
/// interactive evidence chips and `[[wiki:...]]` into page links.
fn render_markdown(
    body: &str,
    evidences: &[ResolvedEvidence],
    title_by_id: &BTreeMap<String, String>,
    status_by_id: &BTreeMap<String, EffectiveStatus>,
    root_prefix: &str,
) -> String {
    let deduped = parse_citations(body)
        .map(dedup_citations)
        .unwrap_or_default();
    let cite_ordinal =
        |path: &str, symbol: Option<&str>, lines: Option<[u32; 2]>| -> Option<usize> {
            deduped
                .iter()
                .position(|c| c.path == path && c.symbol.as_deref() == symbol && c.lines == lines)
        };

    let lines: Vec<&str> = body.lines().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Fenced code blocks: ``` or ~~~
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let fence = if trimmed.starts_with("```") {
                "```"
            } else {
                "~~~"
            };
            let lang = trimmed.trim_start_matches(fence).trim();
            i += 1;
            let mut code_buf = String::new();
            while i < lines.len() && !lines[i].trim().starts_with(fence) {
                code_buf.push_str(lines[i]);
                code_buf.push('\n');
                i += 1;
            }
            if i < lines.len() {
                i += 1; // consume closing fence
            }
            let lang_attr = if lang.is_empty() {
                String::new()
            } else {
                format!(r#" class="language-{}""#, escape_html(lang))
            };
            out.push_str(&format!(
                "<pre><code{}>{}</code></pre>\n",
                lang_attr,
                escape_html(&code_buf)
            ));
            continue;
        }

        // Indented code block (4 spaces)
        if line.starts_with("    ") && !trimmed.starts_with('-') && !trimmed.starts_with('*') {
            let mut code_buf = String::new();
            while i < lines.len() && (lines[i].starts_with("    ") || lines[i].trim().is_empty()) {
                if let Some(stripped) = lines[i].strip_prefix("    ") {
                    code_buf.push_str(stripped);
                }
                code_buf.push('\n');
                i += 1;
            }
            out.push_str(&format!(
                "<pre><code>{}</code></pre>\n",
                escape_html(code_buf.trim_end())
            ));
            continue;
        }

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" {
            out.push_str("<hr>\n");
            i += 1;
            continue;
        }

        // Headings (# .. ######)
        if let Some((level, heading_text)) = parse_heading(trimmed) {
            let slug = slugify(heading_text);
            let inner = render_inline(
                heading_text,
                evidences,
                &cite_ordinal,
                title_by_id,
                status_by_id,
                root_prefix,
            );
            out.push_str(&format!("<h{level} id=\"{slug}\">{inner}</h{level}>\n"));
            i += 1;
            continue;
        }

        // GFM Pipe table
        if trimmed.starts_with('|')
            && i + 1 < lines.len()
            && is_table_separator(lines[i + 1].trim())
        {
            let headers = split_table_row(trimmed);
            i += 2; // skip header + separator
            out.push_str("<table class=\"md-table\"><thead><tr>");
            for h in headers {
                let inner = render_inline(
                    h,
                    evidences,
                    &cite_ordinal,
                    title_by_id,
                    status_by_id,
                    root_prefix,
                );
                out.push_str(&format!("<th>{inner}</th>"));
            }
            out.push_str("</tr></thead><tbody>");
            while i < lines.len() && lines[i].trim().starts_with('|') {
                let cells = split_table_row(lines[i].trim());
                out.push_str("<tr>");
                for c in cells {
                    let inner = render_inline(
                        c,
                        evidences,
                        &cite_ordinal,
                        title_by_id,
                        status_by_id,
                        root_prefix,
                    );
                    out.push_str(&format!("<td>{inner}</td>"));
                }
                out.push_str("</tr>");
                i += 1;
            }
            out.push_str("</tbody></table>\n");
            continue;
        }

        // Blockquote
        if trimmed.starts_with('>') {
            let mut quote_lines = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('>') {
                let q = lines[i].trim().strip_prefix('>').unwrap_or("").trim_start();
                quote_lines.push(render_inline(
                    q,
                    evidences,
                    &cite_ordinal,
                    title_by_id,
                    status_by_id,
                    root_prefix,
                ));
                i += 1;
            }
            out.push_str(&format!(
                "<blockquote><p>{}</p></blockquote>\n",
                quote_lines.join(" ")
            ));
            continue;
        }

        // Unordered list
        if is_ul_item(trimmed) {
            out.push_str("<ul>\n");
            while i < lines.len() && is_ul_item(lines[i].trim()) {
                let item_text = lines[i].trim()[2..].trim();
                let inner = render_inline(
                    item_text,
                    evidences,
                    &cite_ordinal,
                    title_by_id,
                    status_by_id,
                    root_prefix,
                );
                out.push_str(&format!("  <li>{inner}</li>\n"));
                i += 1;
            }
            out.push_str("</ul>\n");
            continue;
        }

        // Ordered list
        if let Some(first_item) = strip_ol_prefix(trimmed) {
            let _ = first_item;
            out.push_str("<ol>\n");
            while i < lines.len() {
                let Some(item_text) = strip_ol_prefix(lines[i].trim()) else {
                    break;
                };
                let inner = render_inline(
                    item_text,
                    evidences,
                    &cite_ordinal,
                    title_by_id,
                    status_by_id,
                    root_prefix,
                );
                out.push_str(&format!("  <li>{inner}</li>\n"));
                i += 1;
            }
            out.push_str("</ol>\n");
            continue;
        }

        // Paragraph
        let mut para = Vec::new();
        while i < lines.len() {
            let l = lines[i];
            let t = l.trim();
            if t.is_empty()
                || t.starts_with("```")
                || t.starts_with("~~~")
                || t == "---"
                || t == "***"
                || parse_heading(t).is_some()
                || t.starts_with('>')
                || is_ul_item(t)
                || strip_ol_prefix(t).is_some()
                || (t.starts_with('|')
                    && i + 1 < lines.len()
                    && is_table_separator(lines[i + 1].trim()))
            {
                break;
            }
            para.push(render_inline(
                t,
                evidences,
                &cite_ordinal,
                title_by_id,
                status_by_id,
                root_prefix,
            ));
            i += 1;
        }
        out.push_str(&format!("<p>{}</p>\n", para.join(" ")));
    }

    out
}

fn parse_heading(trimmed: &str) -> Option<(usize, &str)> {
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ') {
        Some((hashes, trimmed[hashes + 1..].trim()))
    } else {
        None
    }
}

fn is_ul_item(trimmed: &str) -> bool {
    trimmed.starts_with("- ") || trimmed.starts_with("* ")
}

fn strip_ol_prefix(trimmed: &str) -> Option<&str> {
    let digits = trimmed.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 && trimmed[digits..].starts_with(". ") {
        Some(trimmed[digits + 2..].trim())
    } else {
        None
    }
}

fn is_table_separator(trimmed: &str) -> bool {
    trimmed.starts_with('|')
        && trimmed
            .bytes()
            .all(|b| matches!(b, b'|' | b'-' | b':' | b' ' | b'\t'))
        && trimmed.contains('-')
}

fn split_table_row(trimmed: &str) -> Vec<&str> {
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner.split('|').map(str::trim).collect()
}

fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            for lc in c.to_lowercase() {
                slug.push(lc);
            }
        } else if (c == ' ' || c == '-' || c == '_') && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

fn render_inline<F>(
    text: &str,
    evidences: &[ResolvedEvidence],
    cite_ordinal: &F,
    title_by_id: &BTreeMap<String, String>,
    status_by_id: &BTreeMap<String, EffectiveStatus>,
    root_prefix: &str,
) -> String
where
    F: Fn(&str, Option<&str>, Option<[u32; 2]>) -> Option<usize>,
{
    let mut out = String::with_capacity(text.len() + 32);
    let mut last = 0;

    for caps in INLINE_TOKEN_RE.captures_iter(text) {
        let m = caps.get(0).unwrap();
        out.push_str(&escape_html(&text[last..m.start()]));

        if let Some(code) = caps.get(1) {
            out.push_str(&format!("<code>{}</code>", escape_html(code.as_str())));
        } else if let Some(cite_path) = caps.get(2) {
            let path = cite_path.as_str().to_string();
            let symbol = caps.get(3).map(|s| s.as_str().to_string());
            let lines = match (caps.get(4), caps.get(5)) {
                (Some(lo), Some(hi)) => {
                    let lo = lo.as_str().parse::<u32>().unwrap_or(1);
                    let hi = hi.as_str().parse::<u32>().unwrap_or(lo);
                    Some([lo, hi])
                }
                _ => None,
            };
            let c = Citation {
                path: path.clone(),
                symbol: symbol.clone(),
                lines,
                span: (m.start(), m.end()),
            };
            let ord = cite_ordinal(&path, symbol.as_deref(), lines);
            if let Some(ev) = match_citation(&c, ord, evidences) {
                let effective_sym = match &ev.symbol {
                    Some(s) => format!("#{s}"),
                    None => String::new(),
                };
                let effective_range = match ev.lines {
                    Some([lo, hi]) => format!(":L{lo}-L{hi}"),
                    None => String::new(),
                };
                let drifted = (lines.is_some() && ev.lines != lines) || ev.path != path;
                let drift_mark = if drifted { " ↻" } else { "" };
                let ok_cls = if ev.ok { "cite-ok" } else { "cite-stale" };
                let tooltip = match &ev.note {
                    Some(n) => format!("{} ({})", ev.detail, n),
                    None => ev.detail.clone(),
                };
                out.push_str(&format!(
                    r##"<a class="cite-chip {ok_cls}" href="#ev-{idx}" data-ev="ev-{idx}" title="{tooltip}">⟦#{idx} {ev_path}{effective_sym}{effective_range}{drift_mark}⟧</a>"##,
                    ok_cls = ok_cls,
                    idx = ev.index,
                    tooltip = escape_html(&tooltip),
                    ev_path = escape_html(&ev.path),
                    effective_sym = escape_html(&effective_sym),
                    effective_range = escape_html(&effective_range),
                    drift_mark = drift_mark,
                ));
            } else {
                let raw_sym = match &symbol {
                    Some(s) => format!("#{s}"),
                    None => String::new(),
                };
                let raw_range = match lines {
                    Some([lo, hi]) => format!(":L{lo}-L{hi}"),
                    None => String::new(),
                };
                out.push_str(&format!(
                    r#"<span class="cite-chip cite-unpinned" title="Unpinned citation">⟦{}{}{}⟧</span>"#,
                    escape_html(&path),
                    escape_html(&raw_sym),
                    escape_html(&raw_range),
                ));
            }
        } else if let Some(wiki_id) = caps.get(6) {
            let target = wiki_id.as_str();
            if !target.contains("..") && title_by_id.contains_key(target) {
                let st = status_by_id
                    .get(target)
                    .copied()
                    .unwrap_or(EffectiveStatus::Verified)
                    .as_str();
                let title = title_by_id
                    .get(target)
                    .map(String::as_str)
                    .unwrap_or(target);
                out.push_str(&format!(
                    r#"<a class="wiki-link status-{st}" href="{root_prefix}pages/{target}.html" title="{title}">{target}</a>"#,
                    st = st,
                    root_prefix = root_prefix,
                    target = escape_html(target),
                    title = escape_html(title),
                ));
            } else {
                out.push_str(&format!(
                    r#"<span class="wiki-link broken" title="Page not found">{target}</span>"#,
                    target = escape_html(target)
                ));
            }
        } else if let (Some(label), Some(url)) = (caps.get(7), caps.get(8)) {
            let u = url.as_str();
            if u.to_ascii_lowercase().starts_with("javascript:") {
                out.push_str(&escape_html(label.as_str()));
            } else {
                out.push_str(&format!(
                    r#"<a href="{}" target="_blank" rel="noopener">{}</a>"#,
                    escape_html(u),
                    escape_html(label.as_str())
                ));
            }
        } else if let Some(bold) = caps.get(9) {
            out.push_str(&format!("<strong>{}</strong>", escape_html(bold.as_str())));
        } else if let Some(italic) = caps.get(10) {
            out.push_str(&format!("<em>{}</em>", escape_html(italic.as_str())));
        }

        last = m.end();
    }
    out.push_str(&escape_html(&text[last..]));
    out
}

pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Handle a single HTTP request path (e.g. `/`, `/pages/core-model.html`,
/// `/api/search?q=drift`) against the live repository at `repo_root`.
pub fn handle_http_path(repo_root: &Path, request_target: &str) -> Result<HttpResponse> {
    let repo = GitRepo::discover(repo_root)?;
    let wiki_dir = repo.wiki_dir();
    let (raw_path, query) = match request_target.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (request_target, None),
    };

    if raw_path == "/api/search" {
        let q = query
            .and_then(|qs| {
                qs.split('&').find_map(|pair| {
                    let (k, v) = pair.split_once('=')?;
                    (k == "q").then(|| percent_decode(v))
                })
            })
            .unwrap_or_default();
        let pages = load_pages(&wiki_dir)?;
        let idx = InvertedIndex::build(
            pages
                .iter()
                .map(|p| IndexedPage {
                    id: p.meta.id.clone(),
                    title: p.meta.title.clone(),
                    body: p.body.clone(),
                })
                .collect(),
        );
        let hits = idx.search(&q, 15);
        #[derive(Serialize)]
        struct ApiHit {
            id: String,
            title: String,
            score: usize,
            href: String,
            matched_lines: Vec<String>,
        }
        let payload: Vec<ApiHit> = hits
            .into_iter()
            .map(|h| ApiHit {
                href: format!("pages/{}.html", h.id),
                id: h.id,
                title: h.title,
                score: h.score,
                matched_lines: h.matched_lines,
            })
            .collect();
        let body = serde_json::to_string(&payload).unwrap_or_else(|_| "[]".into());
        return Ok(HttpResponse {
            status: 200,
            content_type: "application/json; charset=utf-8",
            body,
        });
    }

    let site = render_site(&repo.view(), &wiki_dir)?;
    let key = match raw_path.trim_start_matches('/') {
        "" | "index.html" => "index.html".to_string(),
        other if site.files.contains_key(other) => other.to_string(),
        other if site.files.contains_key(&format!("pages/{other}")) => {
            format!("pages/{other}")
        }
        other if site.files.contains_key(&format!("pages/{other}.html")) => {
            format!("pages/{other}.html")
        }
        other => other.to_string(),
    };

    if let Some(body) = site.files.get(&key) {
        let content_type = if key.ends_with(".css") {
            "text/css; charset=utf-8"
        } else if key.ends_with(".js") {
            "application/javascript; charset=utf-8"
        } else if key.ends_with(".json") {
            "application/json; charset=utf-8"
        } else {
            "text/html; charset=utf-8"
        };
        Ok(HttpResponse {
            status: 200,
            content_type,
            body: body.clone(),
        })
    } else {
        Ok(HttpResponse {
            status: 404,
            content_type: "text/plain; charset=utf-8",
            body: format!("404 not found: {raw_path}"),
        })
    }
}

/// Serve the live HTML wiki preview over HTTP until interrupted (`max_requests`
/// is `None` in CLI use, or `Some(n)` in tests).
pub fn serve_http(
    listener: TcpListener,
    repo_root: &Path,
    max_requests: Option<usize>,
) -> Result<()> {
    let mut served = 0usize;
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        if n == 0 {
            continue;
        }
        let req = String::from_utf8_lossy(&buf[..n]);
        let target = req
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let resp = handle_http_path(repo_root, target).unwrap_or_else(|e| HttpResponse {
            status: 500,
            content_type: "text/plain; charset=utf-8",
            body: format!("500 internal error: {e}"),
        });
        let status_text = match resp.status {
            200 => "OK",
            404 => "Not Found",
            _ => "Internal Server Error",
        };
        let header = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            resp.status,
            status_text,
            resp.content_type,
            resp.body.len()
        );
        let _ = stream.write_all(header.as_bytes());
        let _ = stream.write_all(resp.body.as_bytes());
        let _ = stream.flush();

        served += 1;
        if let Some(limit) = max_requests {
            if served >= limit {
                break;
            }
        }
    }
    Ok(())
}

fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let raw = input.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'+' {
            bytes.push(b' ');
            i += 1;
        } else if raw[i] == b'%' && i + 2 < raw.len() {
            if let Ok(hex_str) = std::str::from_utf8(&raw[i + 1..i + 3]) {
                if let Ok(b) = u8::from_str_radix(hex_str, 16) {
                    bytes.push(b);
                    i += 3;
                    continue;
                }
            }
            bytes.push(raw[i]);
            i += 1;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

const STYLE_CSS: &str = r#":root {
  --bg: #f8fafc;
  --surface: #ffffff;
  --surface-alt: #f1f5f9;
  --border: #e2e8f0;
  --text: #0f172a;
  --muted: #64748b;
  --accent: #2563eb;
  --accent-soft: #dbeafe;
  --verified: #16a34a;
  --verified-bg: #dcfce7;
  --stale: #d97706;
  --stale-bg: #fef3c7;
  --draft: #64748b;
  --draft-bg: #e2e8f0;
  --code-bg: #0f172a;
  --code-text: #e2e8f0;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #0b1120;
    --surface: #111827;
    --surface-alt: #1e293b;
    --border: #1f2937;
    --text: #f1f5f9;
    --muted: #94a3b8;
    --accent: #60a5fa;
    --accent-soft: #1e3a8a;
    --verified: #4ade80;
    --verified-bg: #064e3b;
    --stale: #fbbf24;
    --stale-bg: #78350f;
    --draft: #94a3b8;
    --draft-bg: #334155;
    --code-bg: #090d16;
    --code-text: #e2e8f0;
  }
}
:root[data-theme="dark"] {
  --bg: #0b1120;
  --surface: #111827;
  --surface-alt: #1e293b;
  --border: #1f2937;
  --text: #f1f5f9;
  --muted: #94a3b8;
  --accent: #60a5fa;
  --accent-soft: #1e3a8a;
  --verified: #4ade80;
  --verified-bg: #064e3b;
  --stale: #fbbf24;
  --stale-bg: #78350f;
  --draft: #94a3b8;
  --draft-bg: #334155;
  --code-bg: #090d16;
  --code-text: #e2e8f0;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Inter, sans-serif;
  background: var(--bg);
  color: var(--text);
  line-height: 1.65;
}
a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }
code, pre {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 0.88em;
}
.layout {
  display: grid;
  grid-template-columns: 280px 1fr;
  min-height: 100vh;
}
.sidebar {
  background: var(--surface);
  border-right: 1px solid var(--border);
  padding: 1rem;
  position: sticky;
  top: 0;
  height: 100vh;
  overflow-y: auto;
}
.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.75rem;
}
.brand { display: flex; align-items: center; gap: 0.5rem; font-weight: 700; }
.brand-name { color: var(--text); font-size: 1.05rem; }
.head-badge {
  font-family: ui-monospace, monospace;
  font-size: 0.75rem;
  background: var(--surface-alt);
  padding: 0.1rem 0.4rem;
  border-radius: 4px;
  color: var(--muted);
}
.theme-btn {
  background: var(--surface-alt);
  border: 1px solid var(--border);
  color: var(--text);
  border-radius: 6px;
  cursor: pointer;
  padding: 0.25rem 0.5rem;
}
.search-box { position: relative; margin-bottom: 1rem; }
.search-box input {
  width: 100%;
  padding: 0.5rem 0.65rem;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  color: var(--text);
  font-size: 0.85rem;
}
.search-results {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: 0 10px 25px rgba(0,0,0,0.18);
  max-height: 360px;
  overflow-y: auto;
  z-index: 20;
}
.search-hit {
  display: block;
  padding: 0.55rem 0.75rem;
  border-bottom: 1px solid var(--border);
  color: var(--text);
}
.search-hit:hover { background: var(--surface-alt); text-decoration: none; }
.search-hit-title { font-weight: 600; font-size: 0.88rem; }
.search-hit-sub { font-size: 0.75rem; color: var(--muted); }
.nav-group { margin-top: 1rem; }
.nav-group-title {
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--muted);
  margin-bottom: 0.35rem;
  font-weight: 600;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.35rem 0.5rem;
  border-radius: 6px;
  color: var(--text);
  font-size: 0.88rem;
}
.nav-item:hover, .nav-item.active {
  background: var(--surface-alt);
  text-decoration: none;
}
.nav-item.active { font-weight: 600; color: var(--accent); }
.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.status-dot.status-verified { background: var(--verified); }
.status-dot.status-stale { background: var(--stale); }
.status-dot.status-draft { background: var(--draft); }
.content {
  max-width: 960px;
  padding: 2rem 2.5rem 4rem;
}
.page-header {
  border-bottom: 1px solid var(--border);
  padding-bottom: 1rem;
  margin-bottom: 1.5rem;
}
.page-title-row, .page-meta-top {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}
.page-title { margin: 0.5rem 0; font-size: 1.85rem; }
.page-meta-sub { font-size: 0.83rem; color: var(--muted); display: flex; gap: 0.5rem; flex-wrap: wrap; }
.badge {
  display: inline-block;
  font-size: 0.73rem;
  font-weight: 600;
  padding: 0.15rem 0.55rem;
  border-radius: 999px;
  text-transform: uppercase;
}
.status-badge.status-verified { background: var(--verified-bg); color: var(--verified); }
.status-badge.status-stale { background: var(--stale-bg); color: var(--stale); }
.status-badge.status-draft { background: var(--draft-bg); color: var(--draft); }
.type-badge, .tag-badge { background: var(--surface-alt); color: var(--muted); text-transform: none; }
.stale-banner {
  background: var(--stale-bg);
  border: 1px solid var(--stale);
  border-radius: 8px;
  padding: 0.9rem 1.1rem;
  margin-bottom: 1.5rem;
}
.cite-chip {
  display: inline-block;
  font-family: ui-monospace, monospace;
  font-size: 0.78rem;
  padding: 0.08rem 0.45rem;
  border-radius: 5px;
  background: var(--accent-soft);
  color: var(--accent);
  border: 1px solid transparent;
  margin: 0 0.1rem;
}
.cite-chip.cite-stale {
  background: var(--stale-bg);
  color: var(--stale);
}
.wiki-link {
  font-weight: 500;
  padding: 0.05rem 0.35rem;
  border-radius: 4px;
  background: var(--surface-alt);
}
.wiki-link.broken { color: #dc2626; text-decoration: line-through; }
.markdown-body pre {
  background: var(--code-bg);
  color: var(--code-text);
  padding: 0.9rem 1.1rem;
  border-radius: 8px;
  overflow-x: auto;
}
.markdown-body code {
  background: var(--surface-alt);
  padding: 0.12rem 0.32rem;
  border-radius: 4px;
}
.markdown-body pre code { background: none; padding: 0; }
.md-table {
  width: 100%;
  border-collapse: collapse;
  margin: 1rem 0;
  font-size: 0.9rem;
}
.md-table th, .md-table td {
  border: 1px solid var(--border);
  padding: 0.5rem 0.75rem;
  text-align: left;
}
.md-table th { background: var(--surface-alt); }
.evidence-section, .backlinks-section {
  margin-top: 2.5rem;
  border-top: 1px solid var(--border);
  padding-top: 1.25rem;
}
.ev-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  margin-bottom: 0.85rem;
  overflow: hidden;
}
.ev-card.ev-highlight {
  outline: 2px solid var(--accent);
}
.ev-summary {
  cursor: pointer;
  padding: 0.6rem 0.9rem;
  background: var(--surface-alt);
  display: flex;
  align-items: center;
  gap: 0.65rem;
  flex-wrap: wrap;
  font-size: 0.84rem;
}
.ev-idx { font-weight: 700; color: var(--accent); }
.ev-verdict { color: var(--muted); }
.ev-hashes { margin-left: auto; color: var(--muted); font-size: 0.76rem; }
.ev-note {
  padding: 0.4rem 0.9rem;
  font-size: 0.78rem;
  background: var(--accent-soft);
  color: var(--accent);
}
.ev-code {
  margin: 0;
  padding: 0.65rem 0;
  background: var(--code-bg);
  color: var(--code-text);
  overflow-x: auto;
}
.ev-line { display: flex; padding: 0 0.9rem; }
.ev-ln {
  user-select: none;
  color: #64748b;
  margin-right: 1rem;
  min-width: 2.5rem;
  text-align: right;
}
.ev-src { white-space: pre; }
.stats-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(135px, 1fr));
  gap: 0.85rem;
  margin-bottom: 1.75rem;
}
.stat-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 0.9rem 1rem;
}
.stat-value { font-size: 1.6rem; font-weight: 700; }
.stat-label { font-size: 0.78rem; color: var(--muted); }
.catalog-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 0.9rem;
}
.catalog-card {
  display: block;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 0.9rem 1rem;
  color: var(--text);
}
.catalog-card:hover {
  border-color: var(--accent);
  text-decoration: none;
}
.catalog-card-top {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  margin-bottom: 0.5rem;
}
.catalog-ev-count { margin-left: auto; font-size: 0.75rem; color: var(--muted); }
.catalog-title { font-weight: 600; margin-bottom: 0.25rem; }
.catalog-id { font-family: ui-monospace, monospace; font-size: 0.75rem; color: var(--muted); }
.quickstart-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 1rem 1.4rem;
  margin-bottom: 2rem;
}
.muted { color: var(--muted); }
@media (max-width: 820px) {
  .layout { grid-template-columns: 1fr; }
  .sidebar { position: static; height: auto; }
}
"#;

const APP_JS: &str = r#"(function () {
  var rootPrefix = document.body.getAttribute("data-root") || "";
  var themeBtn = document.getElementById("theme-toggle");
  var savedTheme = localStorage.getItem("loredex-theme");
  if (savedTheme) {
    document.documentElement.setAttribute("data-theme", savedTheme);
  }
  if (themeBtn) {
    themeBtn.addEventListener("click", function () {
      var cur = document.documentElement.getAttribute("data-theme");
      var next = cur === "dark" ? "light" : "dark";
      document.documentElement.setAttribute("data-theme", next);
      localStorage.setItem("loredex-theme", next);
    });
  }

  // Highlight and open target evidence card when clicking a [[cite:...]] chip
  document.querySelectorAll(".cite-chip[data-ev]").forEach(function (chip) {
    chip.addEventListener("click", function () {
      var id = chip.getAttribute("data-ev");
      var target = document.getElementById(id);
      if (target) {
        target.open = true;
        document.querySelectorAll(".ev-card.ev-highlight").forEach(function (el) {
          el.classList.remove("ev-highlight");
        });
        target.classList.add("ev-highlight");
      }
    });
  });

  // Client-side instant search (backed by search-index.json)
  var searchInput = document.getElementById("wiki-search");
  var resultsBox = document.getElementById("search-results");
  var indexCache = null;

  function loadIndex() {
    if (indexCache) return Promise.resolve(indexCache);
    return fetch(rootPrefix + "search-index.json")
      .then(function (r) { return r.json(); })
      .then(function (data) { indexCache = data; return data; })
      .catch(function () { return []; });
  }

  function renderResults(items) {
    if (!resultsBox) return;
    if (!items.length) {
      resultsBox.hidden = true;
      resultsBox.innerHTML = "";
      return;
    }
    resultsBox.innerHTML = items.slice(0, 10).map(function (it) {
      return '<a class="search-hit" href="' + rootPrefix + it.href + '">' +
        '<div class="search-hit-title">' + escapeHtml(it.title) + '</div>' +
        '<div class="search-hit-sub">' + escapeHtml(it.id) + ' · ' + escapeHtml(it.status) + '</div>' +
        '</a>';
    }).join("");
    resultsBox.hidden = false;
  }

  function escapeHtml(s) {
    return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  }

  if (searchInput && resultsBox) {
    searchInput.addEventListener("input", function () {
      var q = searchInput.value.trim().toLowerCase();
      if (!q) {
        resultsBox.hidden = true;
        return;
      }
      loadIndex().then(function (docs) {
        var matched = docs.filter(function (d) {
          return d.id.toLowerCase().indexOf(q) !== -1 ||
            d.title.toLowerCase().indexOf(q) !== -1 ||
            d.excerpt.toLowerCase().indexOf(q) !== -1 ||
            (d.tags || []).some(function (t) { return t.toLowerCase().indexOf(q) !== -1; });
        });
        renderResults(matched);
      });
    });

    document.addEventListener("keydown", function (ev) {
      if (ev.key === "/" && document.activeElement !== searchInput) {
        ev.preventDefault();
        searchInput.focus();
      } else if (ev.key === "Escape") {
        resultsBox.hidden = true;
      }
    });
  }
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_renders_headings_lists_tables_and_links() {
        let mut titles = BTreeMap::new();
        titles.insert("core-model".to_string(), "Core Model".to_string());
        let mut statuses = BTreeMap::new();
        statuses.insert("core-model".to_string(), EffectiveStatus::Verified);

        let ev = vec![ResolvedEvidence {
            index: 1,
            path: "src/lib.rs".into(),
            symbol: None,
            lines: Some([10, 20]),
            blob: "abcdef1234567890".into(),
            snippet_sha1: Some("1122334455667788".into()),
            note: Some("drift-projected L5-L15@00112233 -> L10-L20 (similarity 1.00)".into()),
            ok: true,
            detail: "blob matches HEAD".into(),
            snippet_lines: vec![(10, "pub fn run() {}".into())],
        }];

        let md = "\
# Heading One

Paragraph with **bold**, *italic*, `[[cite:literal]]`, [[wiki:core-model]], [[wiki:missing]], and [[cite:src/lib.rs:L5-L15]].

- item a
- item b

1. first
2. second

| Col1 | Col2 |
| --- | --- |
| val1 | val2 |

```rust
fn main() {}
```
";
        let html = render_markdown(md, &ev, &titles, &statuses, "../");
        assert!(html.contains("<h1 id=\"heading-one\">Heading One</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        // literal marker inside backticks stays code
        assert!(html.contains("<code>[[cite:literal]]</code>"));
        // real citation resolves to #ev-1 with projected range L10-L20 and drift indicator
        assert!(html.contains("href=\"#ev-1\""));
        assert!(html.contains("src/lib.rs:L10-L20 ↻"));
        // wiki link resolves relative to root_prefix
        assert!(html.contains("href=\"../pages/core-model.html\""));
        assert!(html.contains("class=\"wiki-link broken\""));
        assert!(html.contains("<table class=\"md-table\">"));
        assert!(html.contains("<pre><code class=\"language-rust\">"));
    }
}
