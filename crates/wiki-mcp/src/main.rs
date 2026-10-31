use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ServerCapabilities, ServerConfig};
use rmcp::{schemars, tool, tool_handler, tool_router, ErrorData, ServiceExt, transport::stdio};
use serde::{Deserialize, Serialize};
use wiki_core::search::{IndexedPage, InvertedIndex};
use wiki_core::Evidence;
use wiki_engine::{self as engine, DriftOutcome};
use wiki_git::GitRepo;

const INSTRUCTIONS: &str = "\
repowiki: evidence-backed repository wiki. Protocol contract:
1. Call wiki_check_freshness before trusting any wiki content; treat pages \
reported as stale as UNVERIFIED and fall back to reading source files directly.
2. To author or update a page: call wiki_list_modules to find a generation unit, \
read the cited source files, then call wiki_put_page with a markdown body where \
every claim carries an inline [[cite:path:Lx-Ly]] marker. Pages without citations \
are rejected.
3. When wiki_check_freshness reports drift, call wiki_refresh_page: unchanged \
snippets are line-projected onto the new revision and stay verified; lost ones \
must be rewritten via wiki_put_page.
4. wiki_get_evidence resolves one citation to the file content at current git \
HEAD, so you can audit claims without opening the repo yourself.
Wiki pages live in .wiki/pages/*.md (frontmatter = machine-readable evidence) \
and are committed to git.";

// ---------- output types ----------

#[derive(Serialize, schemars::JsonSchema, Clone)]
struct EvidenceOut {
    path: String,
    lines: Option<Vec<u32>>,
    blob: String,
    snippet_sha1: Option<String>,
    note: Option<String>,
    /// false when the blob recorded is not what HEAD has now
    matches_head: bool,
}

#[derive(Serialize, schemars::JsonSchema)]
struct PageSummary {
    id: String,
    title: String,
    /// status recorded in the page file
    status: String,
    /// live git-based freshness verdict
    stale_now: bool,
    evidence_count: usize,
    commit: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ListOut {
    head: String,
    worktree_dirty: bool,
    pages: Vec<PageSummary>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct PageOut {
    id: String,
    title: String,
    status: String,
    stale_now: bool,
    stale_reasons: Vec<String>,
    generated_at: String,
    commit: String,
    tags: Vec<String>,
    evidence: Vec<EvidenceOut>,
    body: String,
    warning: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct PageFreshness {
    id: String,
    stale: bool,
    reasons: Vec<String>,
    /// evidence whose snippet can still be recovered by drift projection
    projectable: Vec<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct FreshnessOut {
    head: String,
    worktree_dirty: bool,
    stale_count: usize,
    pages: Vec<PageFreshness>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct Hit {
    id: String,
    title: String,
    status: String,
    stale_now: bool,
    score: usize,
    matched_lines: Vec<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct SearchOut {
    query: String,
    hits: Vec<Hit>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ModuleOut {
    name: String,
    kind: String,
    file_count: usize,
    files: Vec<String>,
    /// files of this module already cited by existing wiki pages
    already_cited: usize,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ModulesOut {
    modules: Vec<ModuleOut>,
    hint: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct PutOut {
    id: String,
    created: bool,
    citations: usize,
    evidence: Vec<EvidenceOut>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct RefreshOutcomeOut {
    path: String,
    outcome: String,
    new_lines: Option<Vec<u32>>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct RefreshOut {
    id: String,
    new_status: String,
    outcomes: Vec<RefreshOutcomeOut>,
}

// ---------- input types ----------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct IdParam {
    /// Wiki page id (filename under .wiki/pages without .md)
    id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchParam {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct EvidenceParam {
    id: String,
    /// index into the page's evidence array (0-based)
    index: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PutPageParam {
    /// kebab/snake case page id, becomes the filename
    id: String,
    title: String,
    /// markdown body; every claim must carry [[cite:path:Lx-Ly]] (or [[cite:path]])
    body: String,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[derive(Clone)]
struct WikiServer {
    repo: Arc<Mutex<GitRepo>>,
}

impl WikiServer {
    fn evidence_out(repo: &GitRepo, ev: &Evidence) -> EvidenceOut {
        let head_blob = repo.head_blob_oid(&ev.path).ok().flatten();
        EvidenceOut {
            path: ev.path.clone(),
            lines: ev.lines.map(|[a, b]| vec![a, b]),
            blob: ev.blob.clone(),
            snippet_sha1: ev.snippet_sha1.clone(),
            note: ev.note.clone(),
            matches_head: head_blob.as_deref() == Some(ev.blob.as_str()),
        }
    }

    fn err(msg: impl std::fmt::Display) -> ErrorData {
        ErrorData::internal_error(msg.to_string(), None)
    }
}

fn short(sha: &str) -> String {
    sha.chars().take(8).collect()
}

#[tool_router]
impl WikiServer {
    #[tool(
        name = "wiki_list",
        description = "List all wiki pages with recorded status and a live git freshness verdict per page."
    )]
    async fn wiki_list(&self) -> Result<Json<ListOut>, ErrorData> {
        let repo = self.repo.lock().unwrap();
        let pages = engine::load_pages(&repo.wiki_dir()).map_err(Self::err)?;
        let mut out = Vec::new();
        for p in &pages {
            let report = engine::check_page(p, &repo).map_err(Self::err)?;
            out.push(PageSummary {
                id: p.meta.id.clone(),
                title: p.meta.title.clone(),
                status: p.meta.status.as_str().to_string(),
                stale_now: report.is_stale,
                evidence_count: p.meta.evidence.len(),
                commit: short(&p.meta.commit),
            });
        }
        Ok(Json(ListOut {
            head: repo.head_short().map_err(Self::err)?,
            worktree_dirty: repo.is_dirty().map_err(Self::err)?,
            pages: out,
        }))
    }

    #[tool(
        name = "wiki_read",
        description = "Read a wiki page with evidence. A stale page returns stale_now=true, a warning and reasons; do NOT trust its claims, verify sources or regenerate."
    )]
    async fn wiki_read(&self, Parameters(p): Parameters<IdParam>) -> Result<Json<PageOut>, ErrorData> {
        let repo = self.repo.lock().unwrap();
        let page = engine::find_page(&repo.wiki_dir(), &p.id).map_err(|e| Self::err(e.to_string()))?;
        let report = engine::check_page(&page, &repo).map_err(Self::err)?;
        let reasons: Vec<String> = report
            .evidence_reports
            .iter()
            .filter(|r| !r.ok)
            .map(|r| format!("{}: {}", r.path, r.detail))
            .collect();
        let warning = if report.is_stale {
            Some(format!(
                "STALE: evidence no longer matches git (recorded commit {}); claims are unverified. Use wiki_refresh_page or re-author with wiki_put_page.",
                short(&page.meta.commit)
            ))
        } else {
            None
        };
        Ok(Json(PageOut {
            id: page.meta.id.clone(),
            title: page.meta.title.clone(),
            status: page.meta.status.as_str().to_string(),
            stale_now: report.is_stale,
            stale_reasons: reasons,
            generated_at: page.meta.generated_at.clone(),
            commit: short(&page.meta.commit),
            tags: page.meta.tags.clone(),
            evidence: page
                .meta
                .evidence
                .iter()
                .map(|e| WikiServer::evidence_out(&repo, e))
                .collect(),
            body: page.body.clone(),
            warning,
        }))
    }

    #[tool(
        name = "wiki_check_freshness",
        description = "Check every page against git (evidence blob hashes + diff since recorded commit). Call this BEFORE trusting wiki content; marks pages whose snippets are still drift-projectable."
    )]
    async fn wiki_check_freshness(&self) -> Result<Json<FreshnessOut>, ErrorData> {
        let repo = self.repo.lock().unwrap();
        let pages = engine::load_pages(&repo.wiki_dir()).map_err(Self::err)?;
        let mut out = Vec::new();
        let mut stale_count = 0;
        for p in &pages {
            let report = engine::check_page(p, &repo).map_err(Self::err)?;
            let projectable: Vec<String> = if report.is_stale {
                p.meta
                    .evidence
                    .iter()
                    .filter_map(|e| {
                        let (outcome, _) = engine::project_evidence(&repo, e).ok()?;
                        matches!(outcome, DriftOutcome::Projected { .. } | DriftOutcome::Unchanged)
                            .then(|| e.path.clone())
                    })
                    .collect()
            } else {
                Vec::new()
            };
            if report.is_stale {
                stale_count += 1;
            }
            out.push(PageFreshness {
                id: p.meta.id.clone(),
                stale: report.is_stale,
                reasons: report
                    .evidence_reports
                    .iter()
                    .filter(|r| !r.ok)
                    .map(|r| format!("{}: {}", r.path, r.detail))
                    .collect(),
                projectable,
            });
        }
        Ok(Json(FreshnessOut {
            head: repo.head_short().map_err(Self::err)?,
            worktree_dirty: repo.is_dirty().map_err(Self::err)?,
            stale_count,
            pages: out,
        }))
    }

    #[tool(
        name = "wiki_search",
        description = "Tokenized search (camelCase/snake_case split, CJK bigrams, prefix match) over page titles, ids and bodies."
    )]
    async fn wiki_search(&self, Parameters(p): Parameters<SearchParam>) -> Result<Json<SearchOut>, ErrorData> {
        let repo = self.repo.lock().unwrap();
        let pages = engine::load_pages(&repo.wiki_dir()).map_err(Self::err)?;
        let idx = InvertedIndex::build(
            pages
                .iter()
                .map(|pg| IndexedPage {
                    id: pg.meta.id.clone(),
                    title: pg.meta.title.clone(),
                    body: pg.body.clone(),
                })
                .collect(),
        );
        let limit = p.limit.unwrap_or(8);
        let mut hits = Vec::new();
        for h in idx.search(&p.query, limit) {
            let page = pages.iter().find(|pg| pg.meta.id == h.id);
            let stale = match page {
                Some(pg) => engine::check_page(pg, &repo).map(|r| r.is_stale).unwrap_or(false),
                None => false,
            };
            hits.push(Hit {
                id: h.id,
                title: h.title,
                status: page.map(|pg| pg.meta.status.as_str().to_string()).unwrap_or_default(),
                stale_now: stale,
                score: h.score,
                matched_lines: h.matched_lines,
            });
        }
        Ok(Json(SearchOut { query: p.query, hits }))
    }

    #[tool(
        name = "wiki_get_evidence",
        description = "Resolve one evidence citation to the file content at current git HEAD, with whether it still matches the recorded blob."
    )]
    async fn wiki_get_evidence(&self, Parameters(p): Parameters<EvidenceParam>) -> Result<Json<serde_json::Value>, ErrorData> {
        let repo = self.repo.lock().unwrap();
        let page = engine::find_page(&repo.wiki_dir(), &p.id).map_err(|e| Self::err(e.to_string()))?;
        let e = page
            .meta
            .evidence
            .get(p.index as usize)
            .ok_or_else(|| Self::err(format!("page {} has {} evidence item(s)", page.meta.id, page.meta.evidence.len())))?;
        let head = repo.head_blob_oid(&e.path).map_err(Self::err)?;
        let (state, content) = match head.as_deref() {
            None => ("missing".to_string(), None),
            Some(oid) => {
                let state = if oid == e.blob { "unchanged" } else { "changed" };
                let text = match e.lines {
                    Some([lo, hi]) => repo.head_lines(&e.path, lo, hi).map_err(Self::err)?,
                    None => repo.head_file_content(&e.path).map_err(Self::err)?,
                };
                (state.to_string(), text)
            }
        };
        Ok(Json(serde_json::json!({
            "page_id": page.meta.id,
            "index": p.index,
            "path": e.path,
            "recorded_lines": e.lines.map(|[a,b]| vec![a,b]),
            "recorded_blob": e.blob,
            "head_blob": head,
            "state": state,
            "snippet_sha1_matches": e.snippet_sha1.is_some()
                && e.lines.is_some()
                && content.as_deref()
                    .zip(e.snippet_sha1.as_deref())
                    .map(|(c, h)| engine::sha1_hex(c.as_bytes()) == h)
                    .unwrap_or(false),
            "content_at_head": content,
        })))
    }

    #[tool(
        name = "wiki_list_modules",
        description = "Discover generation units (cargo lib/bin/test targets, else directory groups) with their tracked files and how many are already cited by wiki pages. Use to decide which wiki page to author next."
    )]
    async fn wiki_list_modules(&self) -> Result<Json<ModulesOut>, ErrorData> {
        let repo = self.repo.lock().unwrap();
        let modules = engine::discover_modules(&repo).map_err(Self::err)?;
        let pages = engine::load_pages(&repo.wiki_dir()).unwrap_or_default();
        let cited: std::collections::BTreeSet<String> = pages
            .iter()
            .flat_map(|p| p.meta.evidence.iter().map(|e| e.path.clone()))
            .collect();
        Ok(Json(ModulesOut {
            modules: modules
                .into_iter()
                .map(|m| ModuleOut {
                    already_cited: m.files.iter().filter(|f| cited.contains(f.as_str())).count(),
                    name: m.name,
                    kind: m.kind,
                    file_count: m.files.len(),
                    files: m.files,
                })
                .collect(),
            hint: "author wiki pages for modules with already_cited == 0; cite line ranges you actually read".into(),
        }))
    }

    #[tool(
        name = "wiki_put_page",
        description = "Store agent-authored wiki content. Body must contain [[cite:path:Lx-Ly]] markers; each is pinned to the git blob at HEAD as evidence. A body with no citations is rejected."
    )]
    async fn wiki_put_page(&self, Parameters(p): Parameters<PutPageParam>) -> Result<Json<PutOut>, ErrorData> {
        let repo = self.repo.lock().unwrap();
        let outcome = engine::put_page(
            &repo,
            &repo.wiki_dir(),
            &p.id,
            &p.title,
            &p.body,
            p.tags.unwrap_or_default(),
        )
        .map_err(|e| ErrorData::invalid_request(e.to_string(), None))?;
        Ok(Json(PutOut {
            id: outcome.page.meta.id.clone(),
            created: outcome.created,
            citations: outcome.citations,
            evidence: outcome
                .page
                .meta
                .evidence
                .iter()
                .map(|e| WikiServer::evidence_out(&repo, e))
                .collect(),
        }))
    }

    #[tool(
        name = "wiki_refresh_page",
        description = "Refresh a page's evidence against HEAD with line drift projection: unchanged snippets are re-pointed and stay verified; lost ones fall back to a draft for rewrite."
    )]
    async fn wiki_refresh_page(&self, Parameters(p): Parameters<IdParam>) -> Result<Json<RefreshOut>, ErrorData> {
        let repo = self.repo.lock().unwrap();
        let mut page = engine::find_page(&repo.wiki_dir(), &p.id).map_err(|e| Self::err(e.to_string()))?;
        let outcomes = engine::refresh_page_with_drift(&mut page, &repo).map_err(Self::err)?;
        Ok(Json(RefreshOut {
            id: page.meta.id.clone(),
            new_status: page.meta.status.as_str().to_string(),
            outcomes: outcomes
                .into_iter()
                .map(|(path, o)| match o {
                    DriftOutcome::Unchanged => RefreshOutcomeOut { path, outcome: "unchanged".into(), new_lines: None },
                    DriftOutcome::Projected { lines, score } => RefreshOutcomeOut {
                        path,
                        outcome: format!("projected (similarity {score:.2})"),
                        new_lines: Some(vec![lines[0], lines[1]]),
                    },
                    DriftOutcome::Lost => RefreshOutcomeOut { path, outcome: "lost".into(), new_lines: None },
                    DriftOutcome::NoOldBlob => RefreshOutcomeOut { path, outcome: "old-blob-missing".into(), new_lines: None },
                })
                .collect(),
        }))
    }
}

#[tool_handler]
impl rmcp::ServerHandler for WikiServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(INSTRUCTIONS)
    }
}

#[derive(clap::Parser)]
struct Args {
    /// Repository whose wiki to serve (default: current directory)
    #[arg(long, default_value = ".")]
    repo: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = <Args as clap::Parser>::parse();
    let repo = GitRepo::discover(&args.repo)?;
    if !repo.wiki_dir().is_dir() {
        eprintln!("warning: no .wiki directory found; run `repowiki init` in the repo first");
    }
    let server = WikiServer {
        repo: Arc::new(Mutex::new(repo)),
    };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
