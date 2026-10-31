use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use loredex_core::search::{IndexedPage, InvertedIndex};
use loredex_core::{list_page_files, Evidence, WikiPage};
use loredex_engine::{self as engine, DriftOutcome};
use loredex_git::{GitRepo, GitView};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ServerCapabilities, ServerConfig};
use rmcp::{
    schemars, tool, tool_handler, tool_router, transport::stdio, ErrorData, Peer, RoleServer,
    ServiceExt,
};
use serde::{Deserialize, Serialize};

const INSTRUCTIONS: &str = "\
Loredex: evidence-backed repository wiki. Protocol contract:
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
5. Maintenance is one call: wiki_auto projects recoverable drift (zero tokens), lints, \
and opens a plan run; its pending tasks carry suggested_type + playbook instructions.
Wiki pages live in .wiki/pages/*.md (frontmatter = machine-readable evidence) \
and are committed to git.
REPOSITORY RESOLUTION: the server resolves its target repo lazily in this order \
-- --repo flag (pinned), LOREDEX_REPO env var, client MCP roots, then the \
process working directory. If a tool answers 'no repository resolved', call \
wiki_attach_repo with the workspace path (verify first with wiki_current_repo).";

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
    page_type: Option<String>,
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
    /// Page kind: architecture | workflow | system | integration | testing | ...
    #[serde(default, rename = "type")]
    page_type: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MarkTaskParam {
    /// page id of the pending plan task
    page_id: String,
    /// done | skipped | failed
    status: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AttachParam {
    /// Absolute (or cwd-relative) path inside the git repository to serve
    path: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct CurrentOut {
    resolved: bool,
    /// how it was resolved: cli | env | roots | cwd | attach
    source: Option<String>,
    repo_root: Option<String>,
    head: Option<String>,
    wiki_present: bool,
    pages: usize,
}

/// Lazy, layered repository resolution.
/// Order: --repo flag (pinned at startup) → LOREDEX_REPO env → client MCP
/// roots (attempted once) → process cwd. The slot stores only the resolved
/// workdir; each tool call opens its own GitRepo on a blocking thread, so
/// requests no longer serialize behind one !Sync Repository.
#[derive(Clone)]
pub struct WikiServer {
    slot: Arc<Mutex<RepoSlot>>,
    pages: Arc<Mutex<PageCache>>,
    /// Writers (put/refresh/auto/plan/attach) take the write side; readers can
    /// run concurrently. Blocking scans are moved off the async runtime.
    gate: Arc<tokio::sync::RwLock<()>>,
}

struct RepoSlot {
    fixed: bool,
    current: Option<PathBuf>,
    source: Option<String>,
    roots_attempted: bool,
}

/// Long-lived page/search cache. The key is HEAD + every page file's
/// path/len/mtime, so any write (ours or an external editor) invalidates it.
struct PageCache {
    key: Option<String>,
    pages: Arc<Vec<WikiPage>>,
    index: Option<Arc<InvertedIndex>>,
}

impl Default for PageCache {
    fn default() -> Self {
        Self {
            key: None,
            pages: Arc::new(Vec::new()),
            index: None,
        }
    }
}

struct Resolved {
    path: PathBuf,
    source: String,
}

impl WikiServer {
    fn set_current(&self, path: PathBuf, source: &str) {
        {
            let mut slot = self.slot.lock().unwrap();
            slot.current = Some(path);
            slot.source = Some(source.to_string());
        }
        self.invalidate_pages();
    }

    fn invalidate_pages(&self) {
        *self.pages.lock().unwrap() = PageCache::default();
    }

    async fn ensure_repo(&self, peer: &Peer<RoleServer>) -> Result<Resolved, ErrorData> {
        {
            let slot = self.slot.lock().unwrap();
            if let Some(path) = &slot.current {
                return Ok(Resolved {
                    path: path.clone(),
                    source: slot.source.clone().unwrap_or_default(),
                });
            }
            if slot.fixed {
                return Err(Self::err("repository pinned at startup but is unavailable"));
            }
        }
        // env layer
        if let Ok(p) = std::env::var("LOREDEX_REPO") {
            if let Ok(r) = GitRepo::discover(Path::new(&p)) {
                let path = r.workdir().to_path_buf();
                self.set_current(path.clone(), "env");
                return Ok(Resolved {
                    path,
                    source: "env".into(),
                });
            }
        }
        // roots layer: only clients that declare the roots capability may
        // answer roots/list; querying others would hang the session
        let client_has_roots = peer
            .peer_info()
            .map(|info| info.capabilities.roots.is_some())
            .unwrap_or(false);
        let try_roots = client_has_roots && {
            let mut slot = self.slot.lock().unwrap();
            if slot.roots_attempted {
                false
            } else {
                slot.roots_attempted = true;
                true
            }
        };
        if try_roots {
            #[allow(deprecated)] // SEP-2577 deprecates roots, but clients still send them
            // A non-responsive client must never wedge the server: 3s budget, then fall through.
            if let Ok(Ok(res)) =
                tokio::time::timeout(std::time::Duration::from_secs(3), peer.list_roots()).await
            {
                for root in res.roots {
                    if let Some(p) = uri_to_path(&root.uri) {
                        if let Ok(r) = GitRepo::discover(&p) {
                            let path = r.workdir().to_path_buf();
                            self.set_current(path.clone(), "roots");
                            return Ok(Resolved {
                                path,
                                source: "roots".into(),
                            });
                        }
                    }
                }
            }
        }
        // cwd last resort
        if let Ok(cwd) = std::env::current_dir() {
            if let Ok(r) = GitRepo::discover(&cwd) {
                let path = r.workdir().to_path_buf();
                self.set_current(path.clone(), "cwd");
                return Ok(Resolved {
                    path,
                    source: "cwd".into(),
                });
            }
        }
        Err(ErrorData::invalid_request(
            "no repository resolved (checked --repo, LOREDEX_REPO, client roots, cwd). \
             Call wiki_attach_repo with the workspace path, e.g. /home/user/project.",
            None,
        ))
    }

    /// Run a synchronous git/engine operation on a blocking thread with a
    /// fresh Repository. No global repository lock is held while it runs.
    async fn with_repo<T, F>(&self, peer: &Peer<RoleServer>, f: F) -> Result<T, ErrorData>
    where
        T: Send + 'static,
        F: FnOnce(&GitRepo) -> Result<T, ErrorData> + Send + 'static,
    {
        let resolved = self.ensure_repo(peer).await?;
        let path = resolved.path;
        tokio::task::spawn_blocking(move || {
            let repo = GitRepo::discover(&path).map_err(Self::err)?;
            f(&repo)
        })
        .await
        .map_err(|e| Self::err(format!("blocking task failed: {e}")))?
    }

    fn current_out(repo: &GitRepo, source: &str) -> CurrentOut {
        let wiki_dir = repo.wiki_dir();
        let pages = engine::load_pages(&wiki_dir).map(|p| p.len()).unwrap_or(0);
        CurrentOut {
            resolved: true,
            source: Some(source.to_string()),
            repo_root: Some(repo.workdir().display().to_string()),
            head: repo.head_short().ok(),
            wiki_present: wiki_dir.is_dir(),
            pages,
        }
    }

    fn evidence_out(view: &GitView, ev: &Evidence) -> EvidenceOut {
        let head_blob = view.head_blob_oid(&ev.path).ok().flatten();
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

/// Page-file metadata: cheap compared with YAML parsing/tokenization and
/// sensitive to any page write (loredex writes use rename). HEAD is
/// deliberately not part of the key: freshness is always computed live, and
/// page content does not change when a source-only commit lands.
fn page_cache_key(repo: &GitRepo) -> String {
    let mut material = format!("{}\0", repo.workdir().display());
    if let Ok(files) = list_page_files(&engine::pages_dir(&repo.wiki_dir())) {
        for path in files {
            material.push_str(&path.display().to_string());
            material.push('\0');
            if let Ok(meta) = std::fs::metadata(&path) {
                material.push_str(&meta.len().to_string());
                material.push('\0');
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                material.push_str(&mtime.to_string());
                material.push('\0');
            }
        }
    }
    engine::sha1_hex(material.as_bytes())
}

fn cached_pages(repo: &GitRepo, cache: &Mutex<PageCache>) -> Result<Arc<Vec<WikiPage>>, ErrorData> {
    let key = page_cache_key(repo);
    let mut c = cache.lock().unwrap();
    if c.key.as_deref() != Some(key.as_str()) {
        let pages = engine::load_pages(&repo.wiki_dir()).map_err(WikiServer::err)?;
        *c = PageCache {
            key: Some(key),
            pages: Arc::new(pages),
            index: None,
        };
    }
    Ok(c.pages.clone())
}

fn cached_pages_and_index(
    repo: &GitRepo,
    cache: &Mutex<PageCache>,
) -> Result<(Arc<Vec<WikiPage>>, Arc<InvertedIndex>), ErrorData> {
    let key = page_cache_key(repo);
    let mut c = cache.lock().unwrap();
    if c.key.as_deref() != Some(key.as_str()) {
        let pages = engine::load_pages(&repo.wiki_dir()).map_err(WikiServer::err)?;
        *c = PageCache {
            key: Some(key),
            pages: Arc::new(pages),
            index: None,
        };
    }
    if c.index.is_none() {
        let index = InvertedIndex::build(
            c.pages
                .iter()
                .map(|page| IndexedPage {
                    id: page.meta.id.clone(),
                    title: page.meta.title.clone(),
                    body: page.body.clone(),
                })
                .collect(),
        );
        c.index = Some(Arc::new(index));
    }
    Ok((c.pages.clone(), c.index.clone().expect("index built above")))
}

fn short(sha: &str) -> String {
    sha.chars().take(8).collect()
}

/// MCP root URIs are file:// URLs; decode to a local path.
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri)
        .ok()
        .and_then(|u| u.to_file_path().ok())
}

#[tool_router]
impl WikiServer {
    #[tool(
        name = "wiki_plan",
        description = "Open/resume the wiki generation run: lists concrete work items (New = uncovered module with seed files, Refresh = stale page) tracked in .wiki/runs.json. Pending tasks carry over between runs, so interrupted generation resumes exactly. Authoring via wiki_put_page or refreshing a page auto-completes its task."
    )]
    async fn wiki_plan(
        &self,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.write().await;
        let out = self
            .with_repo(&peer, |repo| {
                let view = repo.view();
                let run =
                    engine::plan_with_view(&view, &repo.wiki_dir(), false).map_err(Self::err)?;
                serde_json::to_value(run).map_err(|e| Self::err(format!("plan serialization: {e}")))
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_mark_task",
        description = "Explicitly resolve a wiki_plan task when you decide to skip or cannot complete it (status: skipped | failed). done is set automatically by wiki_put_page / wiki_refresh_page."
    )]
    async fn wiki_mark_task(
        &self,
        Parameters(p): Parameters<MarkTaskParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.write().await;
        let status = match p.status.as_str() {
            "done" => engine::TaskStatus::Done,
            "skipped" => engine::TaskStatus::Skipped,
            "failed" => engine::TaskStatus::Failed,
            other => {
                return Err(ErrorData::invalid_request(
                    format!("unknown task status {other}"),
                    None,
                ))
            }
        };
        let resolved = self.ensure_repo(&peer).await?;
        let wiki_dir = resolved.path.join(".wiki");
        let page_id = p.page_id;
        let out = tokio::task::spawn_blocking(move || {
            engine::mark_task(&wiki_dir, &page_id, status).map_err(Self::err)?;
            let ledger = engine::load_ledger(&wiki_dir);
            serde_json::to_value(ledger.active_run()).map_err(|e| Self::err(e.to_string()))
        })
        .await
        .map_err(|e| Self::err(format!("blocking task failed: {e}")))??;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_auto",
        description = "One-call wiki maintenance: recovers every drift/rename that projection can fix (zero model tokens), lints structure, then opens a plan run for the remaining authoring work. Returns the full report; pending tasks are persisted in .wiki/runs.json."
    )]
    async fn wiki_auto(
        &self,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.write().await;
        let cache = self.pages.clone();
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let report = engine::auto_refresh_with(&view, &repo.wiki_dir());
                // auto may have rewritten pages even when it reports an error
                *cache.lock().unwrap() = PageCache::default();
                let report = report.map_err(Self::err)?;
                serde_json::to_value(report).map_err(|e| Self::err(e.to_string()))
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_playbooks",
        description = "Page-type authoring guides (architecture/workflow/system/integration/testing + repo-local wiki.toml extensions). Use the plan task's `instructions` field, or this registry to pick/define the right type."
    )]
    async fn wiki_playbooks(
        &self,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.read().await;
        let resolved = self.ensure_repo(&peer).await?;
        let wiki_dir = resolved.path.join(".wiki");
        let out = tokio::task::spawn_blocking(move || {
            let reg = engine::playbook_registry(&wiki_dir);
            serde_json::to_value(reg).map_err(|e| Self::err(e.to_string()))
        })
        .await
        .map_err(|e| Self::err(format!("blocking task failed: {e}")))??;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_attach_repo",
        description = "Attach the server to a git repository (path inside the repo you want to serve). Use when automatic resolution fails or you need to switch repositories mid-session. Refused when the server was started with a pinned --repo."
    )]
    async fn wiki_attach_repo(
        &self,
        Parameters(p): Parameters<AttachParam>,
    ) -> Result<Json<CurrentOut>, ErrorData> {
        let _gate = self.gate.write().await;
        {
            let slot = self.slot.lock().unwrap();
            if slot.fixed {
                return Err(Self::err(
                    "server is pinned to --repo at startup; attach is disabled",
                ));
            }
        }
        let attach_path = PathBuf::from(&p.path);
        let (out, root) = tokio::task::spawn_blocking(move || {
            let repo = GitRepo::discover(&attach_path).map_err(|e| {
                ErrorData::invalid_request(
                    format!("cannot open git repo at {}: {e}", attach_path.display()),
                    None,
                )
            })?;
            let out = Self::current_out(&repo, "attach");
            Ok::<_, ErrorData>((out, repo.workdir().to_path_buf()))
        })
        .await
        .map_err(|e| Self::err(format!("blocking task failed: {e}")))??;
        self.set_current(root, "attach");
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_current_repo",
        description = "Report which repository the server resolved and its wiki state (source: cli/env/roots/cwd/attach, head, page count)."
    )]
    async fn wiki_current_repo(
        &self,
        peer: Peer<RoleServer>,
    ) -> Result<Json<CurrentOut>, ErrorData> {
        let _gate = self.gate.read().await;
        let resolved = self.ensure_repo(&peer).await?;
        let path = resolved.path;
        let source = resolved.source;
        let out = tokio::task::spawn_blocking(move || {
            let repo = GitRepo::discover(&path).map_err(Self::err)?;
            Ok::<_, ErrorData>(Self::current_out(&repo, &source))
        })
        .await
        .map_err(|e| Self::err(format!("blocking task failed: {e}")))??;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_list",
        description = "List all wiki pages with recorded status and a live git freshness verdict per page."
    )]
    async fn wiki_list(&self, peer: Peer<RoleServer>) -> Result<Json<ListOut>, ErrorData> {
        let _gate = self.gate.read().await;
        let cache = self.pages.clone();
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let pages = cached_pages(repo, &cache)?;
                engine::prime_view(&view, &pages).map_err(Self::err)?;
                let mut out = Vec::with_capacity(pages.len());
                for p in pages.iter() {
                    let report = engine::check_page_with(p, &view).map_err(Self::err)?;
                    out.push(PageSummary {
                        id: p.meta.id.clone(),
                        title: p.meta.title.clone(),
                        status: p.meta.status.as_str().to_string(),
                        stale_now: report.is_stale,
                        evidence_count: p.meta.evidence.len(),
                        commit: short(&p.meta.commit),
                    });
                }
                Ok(ListOut {
                    head: view.head_short().map_err(Self::err)?,
                    worktree_dirty: view.is_dirty().map_err(Self::err)?,
                    pages: out,
                })
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_read",
        description = "Read a wiki page with evidence. A stale page returns stale_now=true, a warning and reasons; do NOT trust its claims, verify sources or regenerate."
    )]
    async fn wiki_read(
        &self,
        Parameters(p): Parameters<IdParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<PageOut>, ErrorData> {
        let _gate = self.gate.read().await;
        let id = p.id;
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let page = engine::find_page(&repo.wiki_dir(), &id)
                    .map_err(|e| Self::err(e.to_string()))?;
                let report = engine::check_page_with(&page, &view).map_err(Self::err)?;
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
                Ok(PageOut {
                    id: page.meta.id.clone(),
                    title: page.meta.title.clone(),
                    page_type: page.meta.page_type.clone(),
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
                        .map(|e| WikiServer::evidence_out(&view, e))
                        .collect(),
                    body: page.body.to_string(),
                    warning,
                })
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_check_freshness",
        description = "Check every page against git (evidence blob hashes + diff since recorded commit). Call this BEFORE trusting wiki content; marks pages whose snippets are still drift-projectable."
    )]
    async fn wiki_check_freshness(
        &self,
        peer: Peer<RoleServer>,
    ) -> Result<Json<FreshnessOut>, ErrorData> {
        let _gate = self.gate.read().await;
        let cache = self.pages.clone();
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let pages = cached_pages(repo, &cache)?;
                engine::prime_view(&view, &pages).map_err(Self::err)?;
                let mut out = Vec::with_capacity(pages.len());
                let mut stale_count = 0;
                for p in pages.iter() {
                    let report = engine::check_page_with(p, &view).map_err(Self::err)?;
                    let projectable: Vec<String> = if report.is_stale {
                        let renames = view.rename_map(&p.meta.commit);
                        p.meta
                            .evidence
                            .iter()
                            .filter_map(|e| {
                                let (outcome, _) =
                                    engine::project_evidence_with(&view, e, &renames).ok()?;
                                matches!(
                                    outcome,
                                    DriftOutcome::Projected { .. }
                                        | DriftOutcome::Unchanged
                                        | DriftOutcome::Renamed { .. }
                                )
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
                Ok(FreshnessOut {
                    head: view.head_short().map_err(Self::err)?,
                    worktree_dirty: view.is_dirty().map_err(Self::err)?,
                    stale_count,
                    pages: out,
                })
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_search",
        description = "Tokenized search (camelCase/snake_case split, CJK bigrams, prefix match) over page titles, ids and bodies."
    )]
    async fn wiki_search(
        &self,
        Parameters(p): Parameters<SearchParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<SearchOut>, ErrorData> {
        let _gate = self.gate.read().await;
        let cache = self.pages.clone();
        let query = p.query;
        let limit = p.limit.unwrap_or(8);
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let (pages, index) = cached_pages_and_index(repo, &cache)?;
                let mut hits = Vec::new();
                for h in index.search(&query, limit) {
                    let page = &pages[h.page_idx];
                    let stale = engine::check_page_with(page, &view)
                        .map(|r| r.is_stale)
                        .unwrap_or(false);
                    hits.push(Hit {
                        id: h.id,
                        title: h.title,
                        status: page.meta.status.as_str().to_string(),
                        stale_now: stale,
                        score: h.score,
                        matched_lines: h.matched_lines,
                    });
                }
                Ok(SearchOut { query, hits })
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_get_evidence",
        description = "Resolve one evidence citation to the file content at current git HEAD, with whether it still matches the recorded blob."
    )]
    async fn wiki_get_evidence(
        &self,
        Parameters(p): Parameters<EvidenceParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.read().await;
        let id = p.id;
        let index = p.index;
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let page = engine::find_page(&repo.wiki_dir(), &id)
                    .map_err(|e| Self::err(e.to_string()))?;
                let e = page.meta.evidence.get(index as usize).ok_or_else(|| {
                    Self::err(format!(
                        "page {} has {} evidence item(s)",
                        page.meta.id,
                        page.meta.evidence.len()
                    ))
                })?;
                let head = view.head_blob_oid(&e.path).map_err(Self::err)?;
                let (state, content) = match head.as_deref() {
                    None => ("missing".to_string(), None),
                    Some(oid) => {
                        let state = if oid == e.blob {
                            "unchanged"
                        } else {
                            "changed"
                        };
                        let text = match e.lines {
                            Some([lo, hi]) => {
                                view.head_lines(&e.path, lo, hi).map_err(Self::err)?
                            }
                            None => view
                                .head_file_content(&e.path)
                                .map_err(Self::err)?
                                .map(|content| content.as_ref().clone()),
                        };
                        (state.to_string(), text)
                    }
                };
                Ok(serde_json::json!({
                    "page_id": page.meta.id,
                    "index": index,
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
                }))
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_list_modules",
        description = "Discover generation units (cargo lib/bin/test targets, else directory groups) with their tracked files and how many are already cited by wiki pages. Use to decide which wiki page to author next."
    )]
    async fn wiki_list_modules(
        &self,
        peer: Peer<RoleServer>,
    ) -> Result<Json<ModulesOut>, ErrorData> {
        let _gate = self.gate.read().await;
        let cache = self.pages.clone();
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let modules = engine::discover_with(&view).map_err(Self::err)?;
                let pages = cached_pages(repo, &cache)?;
                let cited: BTreeSet<String> = pages
                    .iter()
                    .flat_map(|p| p.meta.evidence.iter().map(|e| e.path.clone()))
                    .collect();
                Ok(ModulesOut {
                    modules: modules
                        .into_iter()
                        .map(|m| ModuleOut {
                            already_cited: m
                                .files
                                .iter()
                                .filter(|f| cited.contains(f.as_str()))
                                .count(),
                            name: m.name,
                            kind: m.kind,
                            file_count: m.files.len(),
                            files: m.files,
                        })
                        .collect(),
                    hint: "author wiki pages for modules with already_cited == 0; cite line ranges you actually read".into(),
                })
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_put_page",
        description = "Store agent-authored wiki content. Body must contain [[cite:path:Lx-Ly]] markers; each is pinned to the git blob at HEAD as evidence. A body with no citations is rejected."
    )]
    async fn wiki_put_page(
        &self,
        Parameters(p): Parameters<PutPageParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<PutOut>, ErrorData> {
        let _gate = self.gate.write().await;
        let cache = self.pages.clone();
        let id = p.id;
        let title = p.title;
        let body = p.body;
        let tags = p.tags.unwrap_or_default();
        let page_type = p.page_type;
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let result = engine::put_page_with(
                    &view,
                    &repo.wiki_dir(),
                    &id,
                    &title,
                    &body,
                    tags,
                    page_type,
                );
                // a failed put may still have written a page
                *cache.lock().unwrap() = PageCache::default();
                let outcome =
                    result.map_err(|e| ErrorData::invalid_request(e.to_string(), None))?;
                Ok(PutOut {
                    id: outcome.page.meta.id.clone(),
                    created: outcome.created,
                    citations: outcome.citations,
                    evidence: outcome
                        .page
                        .meta
                        .evidence
                        .iter()
                        .map(|e| WikiServer::evidence_out(&view, e))
                        .collect(),
                })
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_refresh_page",
        description = "Refresh a page's evidence against HEAD with line drift projection: unchanged snippets are re-pointed and stay verified; lost ones fall back to a draft for rewrite."
    )]
    async fn wiki_refresh_page(
        &self,
        Parameters(p): Parameters<IdParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<RefreshOut>, ErrorData> {
        let _gate = self.gate.write().await;
        let cache = self.pages.clone();
        let id = p.id;
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let mut page = engine::find_page(&repo.wiki_dir(), &id)
                    .map_err(|e| Self::err(e.to_string()))?;
                let result =
                    engine::refresh_page_with_drift_view(&mut page, &view, &repo.wiki_dir());
                // a failed refresh may still have written a page
                *cache.lock().unwrap() = PageCache::default();
                let outcomes = result.map_err(Self::err)?;
                Ok(RefreshOut {
                    id: page.meta.id.clone(),
                    new_status: page.meta.status.as_str().to_string(),
                    outcomes: outcomes
                        .into_iter()
                        .map(|(path, o)| match o {
                            DriftOutcome::Unchanged => RefreshOutcomeOut {
                                path,
                                outcome: "unchanged".into(),
                                new_lines: None,
                            },
                            DriftOutcome::Projected { lines, score } => RefreshOutcomeOut {
                                path,
                                outcome: format!("projected (similarity {score:.2})"),
                                new_lines: Some(vec![lines[0], lines[1]]),
                            },
                            DriftOutcome::Renamed { new_path, exact } => RefreshOutcomeOut {
                                path,
                                outcome: if exact {
                                    format!("renamed -> {new_path} (identical blob)")
                                } else {
                                    format!("renamed -> {new_path} + projected")
                                },
                                new_lines: None,
                            },
                            DriftOutcome::Lost => RefreshOutcomeOut {
                                path,
                                outcome: "lost".into(),
                                new_lines: None,
                            },
                            DriftOutcome::NoOldBlob => RefreshOutcomeOut {
                                path,
                                outcome: "old-blob-missing".into(),
                                new_lines: None,
                            },
                        })
                        .collect(),
                })
            })
            .await?;
        Ok(Json(out))
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
#[command(name = "loredex-mcp", version)]
pub struct Args {
    /// Pin the served repository (skips env/roots/cwd resolution and disables attach)
    #[arg(long)]
    pub repo: Option<PathBuf>,
}

impl WikiServer {
    /// Lazy resolution: env -> roots -> cwd -> wiki_attach_repo.
    pub fn new_open() -> Self {
        WikiServer {
            slot: Arc::new(Mutex::new(RepoSlot {
                fixed: false,
                current: None,
                source: None,
                roots_attempted: false,
            })),
            pages: Arc::new(Mutex::new(PageCache::default())),
            gate: Arc::new(tokio::sync::RwLock::new(())),
        }
    }

    /// --repo style pinning: startup must resolve the repo; attach disabled.
    pub fn pinned(repo: GitRepo) -> Self {
        WikiServer {
            slot: Arc::new(Mutex::new(RepoSlot {
                fixed: true,
                current: Some(repo.workdir().to_path_buf()),
                source: Some("cli".into()),
                roots_attempted: true,
            })),
            pages: Arc::new(Mutex::new(PageCache::default())),
            gate: Arc::new(tokio::sync::RwLock::new(())),
        }
    }
}

pub async fn run(args: Args) -> anyhow::Result<()> {
    let server = if let Some(p) = &args.repo {
        // pinned layer: fail fast at startup
        let repo = GitRepo::discover(p).map_err(|e| {
            anyhow::anyhow!("--repo {}: cannot open git repository: {e}", p.display())
        })?;
        eprintln!(
            "loredex-mcp: serving {} (source: cli, pinned)",
            repo.workdir().display()
        );
        WikiServer::pinned(repo)
    } else {
        eprintln!(
            "loredex-mcp: repository resolves lazily on first tool call \
             (LOREDEX_REPO env -> client roots -> cwd; or wiki_attach_repo)"
        );
        WikiServer::new_open()
    };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
