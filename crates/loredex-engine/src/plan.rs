//! Generation plan + run ledger: the openwiki `.run.json` replacement, but
//! producer-agnostic — any coding agent consumes the plan, wiki state itself
//! marks tasks done.
//!
//! Ledger: `.wiki/runs.json`, append-only run history. A run is opened when
//! `plan` is executed; its pending tasks carry over to the next `plan`
//! invocation (resume) instead of being discarded — interrupted generation
//! (openwiki's actual failure mode) never loses the worklist. Resolved tasks
//! keep their resolution state; their authoring payload is compacted once the
//! run is superseded so the committed ledger stays small.

use std::collections::BTreeSet;
use std::path::Path;

use loredex_core::WikiError;
use loredex_git::{GitRepo, GitView};
use serde::{Deserialize, Serialize};

use crate::{
    check_page_with, discover_with, index::is_generated, load_pages, now_iso, playbook, Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    /// module has no wiki coverage yet
    New,
    /// existing page whose evidence went stale
    Refresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Done,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
    pub page_id: String,
    pub kind: TaskKind,
    // The fields below are defaults so superseded runs can drop the payload
    // of resolved tasks while keeping the resolution itself (see
    // `compact_resolved_tasks`). Pending tasks always carry the full payload.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub purpose: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seeds: Vec<String>,
    pub status: TaskStatus,
    /// suggested page `type` frontmatter value (see playbook registry)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub suggested_type: String,
    /// playbook-derived authoring instructions for producer agents
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub started_at: String,
    pub base_commit: String,
    pub target_commit: String,
    pub status: String, // active | closed
    pub tasks: Vec<PlanTask>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RunLedger {
    pub schema: u32,
    pub runs: Vec<RunRecord>,
}

pub fn runs_path(wiki_dir: &Path) -> std::path::PathBuf {
    wiki_dir.join("runs.json")
}

pub fn load_ledger(wiki_dir: &Path) -> RunLedger {
    match std::fs::read_to_string(runs_path(wiki_dir)) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => RunLedger {
            schema: 1,
            runs: Vec::new(),
        },
    }
}

fn save_ledger(wiki_dir: &Path, ledger: &RunLedger) -> Result<()> {
    std::fs::create_dir_all(wiki_dir).map_err(WikiError::Io)?;
    // Compact JSON: the ledger is committed to git and rewritten on every
    // put/refresh/plan, so pretty-printing only adds I/O and review noise.
    let text = serde_json::to_string(ledger).map_err(|e| WikiError::Git(e.to_string()))?;
    std::fs::write(runs_path(wiki_dir), text).map_err(WikiError::Io)
}

/// Resolved tasks only contribute resolution state to future plans; their
/// authoring payload is regenerable from the playbook registry. Dropping it
/// keeps a long-lived ledger small instead of re-parsing megabytes per write.
fn compact_resolved_tasks(run: &mut RunRecord) {
    for t in &mut run.tasks {
        if t.status != TaskStatus::Pending {
            t.purpose.clear();
            t.seeds.clear();
            t.suggested_type.clear();
            t.instructions.clear();
        }
    }
}

/// Which pages/modules need work right now: stale pages (Refresh) and
/// modules with files nobody cites (New). Excludes the generated index page.
pub fn compute_tasks(repo: &GitRepo, wiki_dir: &Path) -> Result<Vec<PlanTask>> {
    compute_tasks_with(&repo.view(), wiki_dir)
}

pub fn compute_tasks_with(view: &GitView, wiki_dir: &Path) -> Result<Vec<PlanTask>> {
    let mut tasks = Vec::new();
    let reg = playbook::registry(wiki_dir);
    let locale = crate::config::locale(wiki_dir);
    let pages = load_pages(wiki_dir)?;
    crate::prime_view(view, &pages)?;
    let mut cited: BTreeSet<String> = BTreeSet::new();

    let instr = |t: &str, subject: &str| {
        reg.get(t)
            .map(|pb| pb.instructions_for(subject, locale))
            .unwrap_or_else(|| match locale {
                crate::config::Locale::Zh => {
                    format!("为 `{subject}` 撰写带引用的页面(未知类型 `{t}`)。")
                }
                _ => format!("Write page `{subject}` with cited claims (unknown type `{t}`)."),
            })
    };

    for p in &pages {
        if is_generated(&p.meta.id) {
            continue;
        }
        for e in &p.meta.evidence {
            cited.insert(e.path.clone());
        }
        let report = check_page_with(p, view)?;
        if !report.is_stale && p.meta.status != loredex_core::Status::Verified {
            // draft/stale-flagged pages are authoring work even when git-clean
            tasks.push(PlanTask {
                suggested_type: p.meta.page_type.clone().unwrap_or_else(|| "system".into()),
                instructions: instr(p.meta.page_type.as_deref().unwrap_or("system"), &p.meta.id),
                page_id: p.meta.id.clone(),
                kind: TaskKind::Refresh,
                purpose: format!("page status is `{}`", p.meta.status.as_str()),
                seeds: p.meta.evidence.iter().map(|e| e.path.clone()).collect(),
                status: TaskStatus::Pending,
            });
            continue;
        }
        if report.is_stale {
            let why = report
                .evidence_reports
                .iter()
                .filter(|r| !r.ok)
                .map(|r| format!("{}: {}", r.path, r.detail))
                .collect::<Vec<_>>()
                .join("; ");
            let stype = p.meta.page_type.clone().unwrap_or_else(|| "system".into());
            let subject = p.meta.id.clone();
            tasks.push(PlanTask {
                suggested_type: stype.clone(),
                instructions: instr(&stype, &subject),
                page_id: p.meta.id.clone(),
                kind: TaskKind::Refresh,
                purpose: format!("evidence stale — {why}"),
                seeds: p.meta.evidence.iter().map(|e| e.path.clone()).collect(),
                status: TaskStatus::Pending,
            });
        }
    }

    for m in discover_with(view)? {
        if m.name.starts_with(".wiki") || (m.name != "." && m.name.starts_with('.')) {
            continue; // dot-directories are tooling config, not doc subjects
        }
        let pid_check = suggest_page_id(&m.name);
        if pages.iter().any(|p| p.meta.id == pid_check) {
            continue; // root/uncertainty pages already authored
        }
        let uncovered: Vec<String> = m
            .files
            .iter()
            .filter(|f| !cited.contains(*f) && !f.starts_with(".wiki/"))
            .cloned()
            .collect();
        let config_only = uncovered.iter().all(|f| {
            matches!(
                f.rsplit('/').next().unwrap_or(f),
                "Cargo.toml"
                    | "Cargo.lock"
                    | ".gitignore"
                    | "rust-toolchain.toml"
                    | "package-lock.json"
            )
        });
        if !config_only
            && uncovered.len() as f64 >= m.files.len() as f64 * 0.5
            && !uncovered.is_empty()
        {
            let mut seeds = uncovered;
            seeds.truncate(12);
            let pid = suggest_page_id(&m.name);
            let stype = playbook::suggest_for_module(&m.name).to_string();
            tasks.push(PlanTask {
                suggested_type: stype.clone(),
                instructions: instr(&stype, &pid),
                page_id: pid,
                kind: TaskKind::New,
                purpose: format!(
                    "module `{}` ({} files) has no wiki coverage",
                    m.name,
                    m.files.len()
                ),
                seeds,
                status: TaskStatus::Pending,
            });
        }
    }
    Ok(tasks)
}

/// Derive a valid page id suggestion from a module/group name.
pub fn suggest_page_id(module_name: &str) -> String {
    let base = if module_name == "." {
        "repo-root".to_string()
    } else {
        module_name.replace('/', "-")
    };
    let cleaned: String = base
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "uncategorized".to_string()
    } else {
        trimmed
    }
}

/// Open (or resume) a run (see `plan_with`).
pub fn plan(repo: &GitRepo, wiki_dir: &Path) -> Result<RunRecord> {
    plan_with(repo, wiki_dir, false)
}

/// Open (or resume) a run. Semantics:
/// - pending tasks carry over from the previous run (interrupted work survives)
/// - New tasks already resolved (done/skipped/failed) in the previous run stay
///   suppressed, so `skipped` means a decision, not noise that regrows each run
/// - Refresh tasks are never suppressed: they are git-truth, not judgement
/// - `reset` re-exposes previously resolved New tasks
pub fn plan_with(repo: &GitRepo, wiki_dir: &Path, reset: bool) -> Result<RunRecord> {
    plan_with_view(&repo.view(), wiki_dir, reset)
}

pub fn plan_with_view(view: &GitView, wiki_dir: &Path, reset: bool) -> Result<RunRecord> {
    let mut ledger = load_ledger(wiki_dir);
    let fresh = compute_tasks_with(view, wiki_dir)?;

    let prev = ledger.runs.last();
    // resolutions are remembered across the whole ledger, not just the last run
    let resolved_new: BTreeSet<String> = if reset {
        BTreeSet::new()
    } else {
        ledger
            .runs
            .iter()
            .flat_map(|r| r.tasks.iter())
            .filter(|t| t.kind == TaskKind::New && t.status != TaskStatus::Pending)
            .map(|t| t.page_id.clone())
            .collect()
    };
    let carried: Vec<PlanTask> = prev
        .map(|r| {
            r.tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Pending)
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut tasks = Vec::new();
    for t in fresh.iter().chain(carried.iter()) {
        if resolved_new.contains(&t.page_id) {
            continue;
        }
        if seen.insert(t.page_id.clone()) {
            tasks.push(t.clone());
        }
    }

    let head = view.head_commit()?.to_string();
    let (base_commit, next_id) = match ledger.runs.last() {
        Some(last) => (last.target_commit.clone(), ledger.runs.len() + 1),
        None => (head.clone(), 1),
    };
    let run = RunRecord {
        id: format!("run-{next_id}-{}", head.chars().take(8).collect::<String>()),
        started_at: now_iso(),
        base_commit,
        target_commit: head,
        status: "active".into(),
        tasks,
    };
    // close superseded runs: the new run carries their pending work
    for r in &mut ledger.runs {
        if r.status == "active" {
            r.status = "superseded".into();
            compact_resolved_tasks(r);
        }
    }
    // keep at most 32 runs
    if ledger.runs.len() >= 32 {
        ledger.runs.remove(0);
    }
    ledger.runs.push(run.clone());
    save_ledger(wiki_dir, &ledger)?;
    Ok(run)
}

impl RunLedger {
    pub fn active_run(&self) -> Option<&RunRecord> {
        self.runs.iter().rev().find(|r| r.status == "active")
    }
}

/// Mark a task in the active run as resolved. Called automatically from
/// put_page / refresh hooks and explicitly from wiki_mark_task.
pub fn mark_task(wiki_dir: &Path, page_id: &str, status: TaskStatus) -> Result<()> {
    mark_tasks(wiki_dir, std::iter::once(page_id), status)
}

/// Batch form of `mark_task`: resolve many page ids with one ledger read and
/// one write. Batch maintenance (auto) must use this instead of marking per
/// page, which is quadratic in ledger size.
pub fn mark_tasks<'a, I>(wiki_dir: &Path, page_ids: I, status: TaskStatus) -> Result<()>
where
    I: IntoIterator<Item = &'a str>,
{
    let ids: BTreeSet<&str> = page_ids.into_iter().collect();
    if ids.is_empty() {
        return Ok(());
    }
    let mut ledger = load_ledger(wiki_dir);
    let mut changed = false;
    if let Some(run) = ledger.runs.iter_mut().rev().find(|r| r.status == "active") {
        for t in &mut run.tasks {
            if t.status == TaskStatus::Pending && ids.contains(t.page_id.as_str()) {
                t.status = status;
                changed = true;
            }
        }
    }
    if changed {
        save_ledger(wiki_dir, &ledger)?;
    }
    Ok(())
}

/// Close the active run (nothing more will be carried).
pub fn close_run(wiki_dir: &Path) -> Result<Option<String>> {
    let mut ledger = load_ledger(wiki_dir);
    if let Some(run) = ledger.runs.iter_mut().rev().find(|r| r.status == "active") {
        run.status = "closed".into();
        let id = run.id.clone();
        save_ledger(wiki_dir, &ledger)?;
        return Ok(Some(id));
    }
    Ok(None)
}

/// Number of pending tasks for a page id (used by hooks).
pub fn has_pending(wiki_dir: &Path, page_id: &str) -> bool {
    load_ledger(wiki_dir)
        .active_run()
        .map(|r| {
            r.tasks
                .iter()
                .any(|t| t.page_id == page_id && t.status == TaskStatus::Pending)
        })
        .unwrap_or(false)
}
