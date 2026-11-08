use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use loredex_core::search::{IndexedPage, InvertedIndex};
use loredex_core::{list_page_files, Evidence, WikiPage};
use loredex_engine::{self as engine, DriftOutcome};
use loredex_git::{GitRepo, GitView};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{
    GetPromptRequestParams, GetPromptResponse, GetPromptResult, ListPromptsResult,
    ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams, Prompt,
    PromptArgument, PromptMessage, ReadResourceRequestParams, ReadResourceResponse,
    ReadResourceResult, Resource, ResourceContents, ResourceTemplate, Role, ServerCapabilities,
    ServerConfig,
};
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
call wiki_evidence_candidates with keywords to get pasteable [[cite:path:Lx-Ly]] \
windows instead of hunting line numbers, read the cited source, then call \
wiki_put_page with a markdown body where \
every claim carries an inline [[cite:path:Lx-Ly]] marker. Pages without citations \
are rejected; citations whose range matches a secret pattern are rejected too \
(override only when the user asks: allow_secret_citations). wiki_read returns the \
page's content sha1: pass it back as if_match (or set if_absent for a create) so \
two agents cannot silently overwrite each other; on a conflict, re-read and retry.
2b. When you learn something you cannot cite yet (a rule, a failure diagnosis, a \
decision), call wiki_note instead: it stores an uncited body as status=draft, which \
is an explicit 'unverified' flag, and wiki_plan queues it for evidence backfill. \
Never paraphrase a draft's claims as fact, and never put a guess through wiki_put_page.
3. When wiki_check_freshness reports drift, call wiki_refresh_page: unchanged \
snippets are line-projected onto the new revision and stay verified; lost ones \
must be rewritten via wiki_put_page. To scope damage to a branch or commit \
range before merging, call wiki_diff_since with that ref. \
wiki_link_suggestions proposes [[wiki:id]] cross-links worth adding, and \
wiki_organize names the page pairs that pin the same evidence so duplicated notes and \
rules get consolidated instead of accreting.
4. wiki_get_evidence resolves one citation to the file content at current git \
HEAD, so you can audit claims without opening the repo yourself. \
wiki_context_pack builds a token-budgeted bundle of the verified pages for one \
module plus the HEAD snippets they cite; use it instead of reading the whole wiki.
5. Maintenance is one call: wiki_auto projects recoverable drift (zero tokens), lints, \
and opens a plan run; its pending tasks carry suggested_type + playbook instructions. \
When several agents share a run, call wiki_claim_tasks with a stable agent id before \
authoring: a leased task is not handed to another agent until its lease expires \
(wiki_plan shows claimed_by and lease_until).
Wiki pages live in .wiki/pages/*.md (frontmatter = machine-readable evidence) \
and are committed to git.
RESOURCES: Wiki pages are accessible via loredex://pages/{id} (or loredex://quickstart, \
loredex://index, and loredex://modules for discoverable project layout).
PROMPTS: Workflows are pre-configured: author_wiki_page, audit_pr_freshness, heal_wiki.
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
    /// sha1 of the page file; pass as `if_match` on the next write
    page_sha1: String,
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
    /// `draft` when a note landed without citable evidence: its claims are
    /// unverified and wiki_plan queues it for backfill
    status: String,
    citations: usize,
    evidence: Vec<EvidenceOut>,
    /// sha1 of the page as written; pass as `if_match` next time
    page_sha1: String,
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
    /// Store citations even when the cited range matches a secret pattern
    #[serde(default)]
    allow_secret_citations: Option<bool>,
    /// Optimistic concurrency: the page_sha1 returned by wiki_read. The write
    /// fails if another agent changed the page since that read.
    #[serde(default)]
    if_match: Option<String>,
    /// Create only: fail when the page already exists (mutually exclusive with if_match)
    #[serde(default)]
    if_absent: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct NoteParam {
    /// kebab/snake case page id, becomes the filename
    id: String,
    title: String,
    /// markdown body; citations are optional, add them where you can
    body: String,
    #[serde(default)]
    tags: Option<Vec<String>>,
    /// Page kind: rule | troubleshooting | decision | note | system | ...
    #[serde(default, rename = "type")]
    page_type: Option<String>,
    /// Optimistic concurrency: the page_sha1 returned by wiki_read
    #[serde(default)]
    if_match: Option<String>,
    /// Create only: fail when the page already exists
    #[serde(default)]
    if_absent: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct EvidenceCandidatesParam {
    /// Module name (see wiki_list_modules) or '.', or a directory prefix
    #[serde(default = "default_module")]
    module: String,
    /// Case-insensitive keywords searched in HEAD content
    keywords: Vec<String>,
    #[serde(default)]
    limit: Option<usize>,
}

fn default_module() -> String {
    ".".into()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DiffSinceParam {
    /// Base ref: sha, branch, tag, HEAD~n (diffed against HEAD)
    since: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct LinksParam {
    /// Only suggestions authored by this page id
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MarkTaskParam {
    /// page id of the pending plan task
    page_id: String,
    /// done | skipped | failed
    status: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ClaimTasksParam {
    /// Stable agent identifier; leases are keyed by it
    agent: String,
    /// Maximum number of tasks to lease (default 1)
    #[serde(default)]
    limit: Option<usize>,
    /// Lease duration in seconds (default 1800)
    #[serde(default)]
    ttl_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ContextPackParam {
    /// Module name (see wiki_list_modules), '.', or a directory prefix
    #[serde(default = "default_module")]
    module: String,
    /// Approximate token ceiling for the markdown artifact (default 8000)
    #[serde(default)]
    budget_tokens: Option<usize>,
    /// Include stale pages, marked as unverified (default false)
    #[serde(default)]
    include_stale: Option<bool>,
    /// Include draft pages, marked as unverified (default false)
    #[serde(default)]
    include_drafts: Option<bool>,
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

    fn expected_from(
        if_match: Option<String>,
        if_absent: Option<bool>,
    ) -> Result<Option<engine::PageExpectation>, ErrorData> {
        match (if_match, if_absent.unwrap_or(false)) {
            (Some(_), true) => Err(ErrorData::invalid_request(
                "if_match and if_absent are mutually exclusive",
                None,
            )),
            (Some(sha), false) => Ok(Some(engine::PageExpectation::Sha1(sha))),
            (None, true) => Ok(Some(engine::PageExpectation::Absent)),
            (None, false) => Ok(None),
        }
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
        name = "wiki_claim_tasks",
        description = "Lease pending plan tasks to this agent so concurrent agents do not author the same page. Pass a stable agent id and an optional ttl_seconds (default 1800); expired leases become claimable again. wiki_plan shows claimed_by/lease_until. Leasing is advisory: authoring the page with wiki_put_page still resolves the task automatically."
    )]
    async fn wiki_claim_tasks(
        &self,
        Parameters(p): Parameters<ClaimTasksParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.write().await;
        let resolved = self.ensure_repo(&peer).await?;
        let wiki_dir = resolved.path.join(".wiki");
        let agent = p.agent;
        let limit = p.limit.unwrap_or(1);
        let ttl = p.ttl_seconds.unwrap_or(1800);
        let out = tokio::task::spawn_blocking(move || {
            let claimed = engine::claim_tasks(&wiki_dir, &agent, limit, ttl).map_err(Self::err)?;
            serde_json::to_value(serde_json::json!({
                "agent": agent,
                "claimed_count": claimed.len(),
                "claimed": claimed,
            }))
            .map_err(|e| Self::err(e.to_string()))
        })
        .await
        .map_err(|e| Self::err(format!("blocking task failed: {e}")))??;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_context_pack",
        description = "Build a token-budgeted context pack for one module: verified, git-fresh pages selected by cited-path relevance, plus the HEAD source snippets they cite (deduplicated, secret-shaped lines masked). Stale pages and drafts are skipped by default and listed in `skipped`; pass include_stale/include_drafts to include them with an UNVERIFIED warning. Returns structured metadata and a ready-to-paste `markdown` artifact."
    )]
    async fn wiki_context_pack(
        &self,
        Parameters(p): Parameters<ContextPackParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.read().await;
        let out = self
            .with_repo(&peer, move |repo| {
                let pack = engine::context_pack(
                    &repo.view(),
                    &repo.wiki_dir(),
                    &p.module,
                    &engine::PackOptions {
                        budget_tokens: p.budget_tokens.unwrap_or(8_000),
                        include_stale: p.include_stale.unwrap_or(false),
                        include_drafts: p.include_drafts.unwrap_or(false),
                    },
                )
                .map_err(Self::err)?;
                serde_json::to_value(pack).map_err(|e| Self::err(e.to_string()))
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
        description = "Page-type authoring guides (architecture/workflow/system/integration/testing plus the policy types rule/troubleshooting/decision, and repo-local wiki.toml extensions). Use the plan task's `instructions` field, or this registry to pick/define the right type."
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
                let page_sha1 = match &page.source_path {
                    Some(path) => engine::page_sha1(path)
                        .map_err(Self::err)?
                        .unwrap_or_default(),
                    None => String::new(),
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
                    page_sha1,
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
        name = "wiki_evidence_candidates",
        description = "Generate citation drafts before authoring: scans a module's files at HEAD for the given keywords and returns ranked, pasteable [[cite:path:Lx-Ly]] windows with masked preview lines. Saves reading whole files to find line numbers; verify claims against the source before putting the page."
    )]
    async fn wiki_evidence_candidates(
        &self,
        Parameters(p): Parameters<EvidenceCandidatesParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.read().await;
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let cand = engine::evidence_candidates(
                    &view,
                    &p.module,
                    &p.keywords,
                    p.limit.unwrap_or(12),
                )
                .map_err(Self::err)?;
                serde_json::to_value(cand).map_err(|e| Self::err(e.to_string()))
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_diff_since",
        description = "Which wiki pages does the change set between a ref and HEAD touch: per-page evidence joined against a structured tree diff (adds/modifies/deletes, rename-paired). Answers 'what docs did this diff break' for a branch or commit range; independent of each page's recorded commit."
    )]
    async fn wiki_diff_since(
        &self,
        Parameters(p): Parameters<DiffSinceParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.read().await;
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let report = engine::diff_since_with(&view, &repo.wiki_dir(), &p.since)
                    .map_err(Self::err)?;
                serde_json::to_value(report).map_err(|e| Self::err(e.to_string()))
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_link_suggestions",
        description = "Suggest cross-page [[wiki:id]] links ranked by shared citations, module adjacency and text similarity (advisory; adding a link is a put with the marker in the body, never required)."
    )]
    async fn wiki_link_suggestions(
        &self,
        Parameters(p): Parameters<LinksParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.read().await;
        let out = self
            .with_repo(&peer, move |repo| {
                let view = repo.view();
                let mut report = engine::links::suggest_links_with(
                    &view,
                    &repo.wiki_dir(),
                    p.limit.unwrap_or(3),
                    3,
                )
                .map_err(Self::err)?;
                if let Some(id) = &p.id {
                    report.suggestions.retain(|s| s.from == *id);
                }
                serde_json::to_value(report).map_err(|e| Self::err(e.to_string()))
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_organize",
        description = "Consolidation worklist over recorded evidence: page PAIRS that pin substantially the same source files — the drift-duplication candidates that accumulate as notes/rules grow. Overlap is reported per pair, never chained into clusters, because a pair already cross-linking is a declared division of labor. Advisory; draft pages are excluded until wiki_plan backfills their citations."
    )]
    async fn wiki_organize(
        &self,
        peer: Peer<RoleServer>,
    ) -> Result<Json<serde_json::Value>, ErrorData> {
        let _gate = self.gate.read().await;
        let out = self
            .with_repo(&peer, move |repo| {
                let report = engine::organize::consolidation_candidates(&repo.wiki_dir())
                    .map_err(Self::err)?;
                serde_json::to_value(report).map_err(|e| Self::err(e.to_string()))
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_put_page",
        description = "Store agent-authored wiki content. Body must contain [[cite:path:Lx-Ly]] markers; each is pinned to the git blob at HEAD as evidence. A body with no citations is rejected. Pass if_match with the page_sha1 from wiki_read to reject the write when another agent changed the page concurrently; if_absent guards creation."
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
        let expected = Self::expected_from(p.if_match, p.if_absent)?;
        let opts = engine::PutOptions {
            allow_secret_citations: p.allow_secret_citations.unwrap_or(false),
            expected,
            ..Default::default()
        };
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
                    &opts,
                );
                // a failed put may still have written a page
                *cache.lock().unwrap() = PageCache::default();
                let outcome =
                    result.map_err(|e| ErrorData::invalid_request(e.to_string(), None))?;
                Ok(PutOut {
                    id: outcome.page.meta.id.clone(),
                    created: outcome.created,
                    status: outcome.page.meta.status.as_str().to_string(),
                    citations: outcome.citations,
                    evidence: outcome
                        .page
                        .meta
                        .evidence
                        .iter()
                        .map(|e| WikiServer::evidence_out(&view, e))
                        .collect(),
                    page_sha1: outcome.page_sha1,
                })
            })
            .await?;
        Ok(Json(out))
    }

    #[tool(
        name = "wiki_note",
        description = "Low-friction capture of a finding (rule, troubleshooting step, decision, observation) with NO citation requirement: an uncited body is stored as status=draft, so its claims are explicitly unverified and wiki_plan queues it for evidence backfill and typing. If the body does carry [[cite:...]] markers they are pinned like wiki_put_page and the page lands verified. Use this instead of wiki_put_page when you learned something and cannot cite it yet; never store a guess as a verified page. if_match/if_absent protect concurrent captures."
    )]
    async fn wiki_note(
        &self,
        Parameters(p): Parameters<NoteParam>,
        peer: Peer<RoleServer>,
    ) -> Result<Json<PutOut>, ErrorData> {
        let _gate = self.gate.write().await;
        let cache = self.pages.clone();
        let id = p.id;
        let title = p.title;
        let body = p.body;
        let tags = p.tags.unwrap_or_default();
        let page_type = p.page_type;
        let expected = Self::expected_from(p.if_match, p.if_absent)?;
        let out = self
            .with_repo(&peer, move |repo| {
                let result = engine::note_page_with(
                    repo,
                    &repo.wiki_dir(),
                    &id,
                    &title,
                    &body,
                    tags,
                    page_type,
                    expected,
                );
                // a failed note may still have written a page
                *cache.lock().unwrap() = PageCache::default();
                let outcome =
                    result.map_err(|e| ErrorData::invalid_request(e.to_string(), None))?;
                Ok(PutOut {
                    id: outcome.page.meta.id.clone(),
                    created: outcome.created,
                    status: outcome.page.meta.status.as_str().to_string(),
                    citations: outcome.citations,
                    evidence: outcome
                        .page
                        .meta
                        .evidence
                        .iter()
                        .map(|e| WikiServer::evidence_out(&repo.view(), e))
                        .collect(),
                    page_sha1: outcome.page_sha1,
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

#[derive(Debug, PartialEq, Eq)]
enum ResourceTarget {
    Modules,
    Page(String),
}

fn parse_resource_uri(uri: &str) -> Option<ResourceTarget> {
    let stripped = uri.strip_prefix("loredex://")?;
    let path = stripped.trim_start_matches('/');
    if path == "modules" || path == "modules/" {
        Some(ResourceTarget::Modules)
    } else if path == "quickstart" || path == "quickstart/" {
        Some(ResourceTarget::Page("quickstart".to_string()))
    } else if path == "index" || path == "index/" {
        Some(ResourceTarget::Page("index".to_string()))
    } else if let Some(page_id) = path.strip_prefix("pages/") {
        let clean = page_id.trim_end_matches(".md").trim_matches('/');
        if clean.is_empty() {
            None
        } else {
            Some(ResourceTarget::Page(clean.to_string()))
        }
    } else {
        let clean = path.trim_end_matches(".md").trim_matches('/');
        if clean.is_empty() {
            None
        } else {
            Some(ResourceTarget::Page(clean.to_string()))
        }
    }
}

#[tool_handler]
impl rmcp::ServerHandler for WikiServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_instructions(INSTRUCTIONS)
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let _gate = self.gate.read().await;
        let resolved = match self.ensure_repo(&context.peer).await {
            Ok(r) => r,
            Err(_) => return Ok(ListResourcesResult::default()),
        };
        let path = resolved.path;
        let resources = tokio::task::spawn_blocking(move || {
            let mut items = Vec::new();
            items.push(
                Resource::new("loredex://modules", "Modules")
                    .with_title("Codebase Modules")
                    .with_description(
                        "List of discoverable modules in the repository with tracked files",
                    )
                    .with_mime_type("application/json"),
            );

            let repo = match GitRepo::discover(&path) {
                Ok(r) => r,
                Err(_) => return items,
            };
            let pages_dir = repo.wiki_dir().join("pages");
            if let Ok(files) = list_page_files(&pages_dir) {
                for f in files {
                    if let Ok(p) = WikiPage::load(&f) {
                        let id = &p.meta.id;
                        let uri = format!("loredex://pages/{id}");
                        let desc = format!("[{}] {}", p.meta.status.as_str(), p.meta.title);
                        items.push(
                            Resource::new(uri, &p.meta.title)
                                .with_title(&p.meta.title)
                                .with_description(desc)
                                .with_mime_type("text/markdown"),
                        );
                    }
                }
            }
            items
        })
        .await
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        let template = ResourceTemplate::new("loredex://pages/{id}", "wiki-page")
            .with_title("Wiki Page")
            .with_description("Evidence-backed wiki markdown page by ID")
            .with_mime_type("text/markdown");
        Ok(ListResourceTemplatesResult::with_all_items(vec![template]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let _gate = self.gate.read().await;
        let resolved = self.ensure_repo(&context.peer).await?;
        let target = parse_resource_uri(&request.uri).ok_or_else(|| {
            ErrorData::invalid_params(
                format!("Invalid resource URI format: {}", request.uri),
                None,
            )
        })?;
        let uri = request.uri.clone();
        let path = resolved.path;

        let content =
            tokio::task::spawn_blocking(move || -> Result<ResourceContents, ErrorData> {
                let repo = GitRepo::discover(&path).map_err(Self::err)?;
                match target {
                    ResourceTarget::Modules => {
                        let view = repo.view();
                        let modules = engine::discover_with(&view).map_err(Self::err)?;
                        let json = serde_json::to_string_pretty(&modules)
                            .map_err(|e| Self::err(e.to_string()))?;
                        Ok(ResourceContents::text(json, uri).with_mime_type("application/json"))
                    }
                    ResourceTarget::Page(id) => {
                        let page = engine::find_page(&repo.wiki_dir(), &id).map_err(|e| {
                            ErrorData::resource_not_found(
                                format!("Resource '{id}' not found: {e}"),
                                None,
                            )
                        })?;
                        let text = if let Some(source_path) = &page.source_path {
                            std::fs::read_to_string(source_path)
                                .unwrap_or_else(|_| page.to_markdown().unwrap_or_default())
                        } else {
                            page.to_markdown().map_err(Self::err)?
                        };
                        Ok(ResourceContents::text(text, uri).with_mime_type("text/markdown"))
                    }
                }
            })
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))??;

        let result = ReadResourceResult::new(vec![content]);
        Ok(result.into())
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        let prompts = vec![
            Prompt::new(
                "author_wiki_page",
                Some("Guide an agent to author or update a wiki page for a codebase module following the evidence contract."),
                Some(vec![
                    PromptArgument::new("module")
                        .with_title("Module Name")
                        .with_description("Target module name (e.g. from wiki_list_modules or a directory path)")
                        .with_required(true),
                    PromptArgument::new("page_type")
                        .with_title("Page Type")
                        .with_description("Page type playbook (e.g. architecture, workflow, system, rule, troubleshooting, decision)")
                        .with_required(false),
                    PromptArgument::new("keywords")
                        .with_title("Keywords")
                        .with_description("Keywords to discover candidate citations (comma-separated)")
                        .with_required(false),
                ]),
            )
            .with_title("Author Wiki Page"),
            Prompt::new(
                "audit_pr_freshness",
                Some("Check which wiki pages become stale due to code changes between a base ref and HEAD, and formulate refresh tasks."),
                Some(vec![
                    PromptArgument::new("base_ref")
                        .with_title("Base Ref")
                        .with_description("Base ref or branch to diff against HEAD (default: origin/main or main)")
                        .with_required(false),
                ]),
            )
            .with_title("Audit PR Freshness"),
            Prompt::new(
                "heal_wiki",
                Some("Run automated line drift recovery, check lint rules, and inspect remaining authoring plan tasks."),
                None,
            )
            .with_title("Heal Wiki & Plan Maintenance"),
        ];

        Ok(ListPromptsResult::with_all_items(prompts))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, ErrorData> {
        let name = request.name.as_str();
        let args = request.arguments.unwrap_or_default();

        let prompt_text = match name {
            "author_wiki_page" => {
                let module = args
                    .get("module")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ErrorData::invalid_params("Missing required argument: 'module'", None)
                    })?;
                let page_type = args.get("page_type").and_then(|v| v.as_str()).unwrap_or("");
                let keywords = args.get("keywords").and_then(|v| v.as_str()).unwrap_or("");

                let type_clause = if !page_type.is_empty() {
                    format!("Target page type: `{page_type}`. Call `wiki_playbooks` to review structure rules for this type.\n")
                } else {
                    String::new()
                };

                let kw_clause = if !keywords.is_empty() {
                    format!(" with keywords: \"{keywords}\"")
                } else {
                    String::new()
                };

                format!(
                    "You are authoring or updating an evidence-backed wiki page for module: `{module}`.\n\
                     {type_clause}\
                     Follow the Loredex Evidence Contract:\n\
                     1. Discover Citations: Call `wiki_evidence_candidates` for module `{module}`{kw_clause} to get verified [[cite:path:Lx-Ly]] citation candidates.\n\
                     2. Read Code: Read the cited source code to understand its architecture, public interfaces, and key algorithms.\n\
                     3. Concurrency Check: If updating an existing page, call `wiki_read` to retrieve its latest `page_sha1` for optimistic locking (`if_match`).\n\
                     4. Write Content: Compose a concise, high-density Markdown document where every non-trivial claim carries an inline `[[cite:path:Lx-Ly]]` marker.\n\
                     5. Save: Call `wiki_put_page` (or `wiki_note` if capturing unverified findings) to store the page. Pages without citations or with secret-shaped lines will be rejected."
                )
            }
            "audit_pr_freshness" => {
                let base_ref = args
                    .get("base_ref")
                    .and_then(|v| v.as_str())
                    .unwrap_or("origin/main");

                format!(
                    "You are auditing wiki documentation freshness against code changes in this pull request.\n\
                     Base revision: `{base_ref}`.\n\n\
                     Steps:\n\
                     1. Call `wiki_diff_since` with `since=\"{base_ref}\"` to inspect all touched files and their citing wiki pages.\n\
                     2. For each affected page:\n\
                        - Call `wiki_refresh_page` to attempt zero-token line drift projection recovery.\n\
                        - For citations reported as 'lost', examine the code diff and rewrite the affected paragraphs using `wiki_put_page`.\n\
                     3. Call `wiki_check_freshness` to confirm that no verified pages remain stale before submitting/merging."
                )
            }
            "heal_wiki" => {
                "You are performing automated maintenance on the repository wiki.\n\n\
                 Steps:\n\
                 1. Call `wiki_auto` to run zero-token line drift projection recovery, lint checks, and open/update the maintenance plan.\n\
                 2. Inspect the report from `wiki_auto`:\n\
                    - If no lint findings and authoring work is complete, the wiki is healthy and fresh!\n\
                    - If drift was projected, verify the updated citations.\n\
                    - If pending authoring tasks remain, call `wiki_plan` to inspect them, lease a task with `wiki_claim_tasks`, and author the required page using `wiki_put_page`."
                    .to_string()
            }
            other => {
                return Err(ErrorData::invalid_params(
                    format!("Unknown prompt: '{other}'"),
                    None,
                ));
            }
        };

        let message = PromptMessage::new_text(Role::User, prompt_text);
        let result = GetPromptResult::new(vec![message])
            .with_description(format!("Instructions for {name}"));
        Ok(result.into())
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
