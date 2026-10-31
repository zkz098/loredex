//! Generation plan + run ledger: the openwiki `.run.json` replacement, but
//! producer-agnostic — any coding agent consumes the plan, wiki state itself
//! marks tasks done.
//!
//! Ledger: `.wiki/runs.json`, append-only run history. A run is opened when
//! `plan` is executed; its pending tasks carry over to the next `plan`
//! invocation (resume) instead of being discarded — interrupted generation
//! (openwiki's actual failure mode) never loses the worklist.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use wiki_core::WikiError;
use wiki_git::GitRepo;

use crate::{discover_modules, load_pages, now_iso, check_page, Result};

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
    pub purpose: String,
    pub seeds: Vec<String>,
    pub status: TaskStatus,
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
        Err(_) => RunLedger { schema: 1, runs: Vec::new() },
    }
}

fn save_ledger(wiki_dir: &Path, ledger: &RunLedger) -> Result<()> {
    std::fs::create_dir_all(wiki_dir).map_err(WikiError::Io)?;
    let text = serde_json::to_string_pretty(ledger)
        .map_err(|e| WikiError::Git(e.to_string()))?;
    std::fs::write(runs_path(wiki_dir), text).map_err(WikiError::Io)
}

/// Which pages/modules need work right now: stale pages (Refresh) and
/// modules with files nobody cites (New). Excludes the generated index page.
pub fn compute_tasks(repo: &GitRepo, wiki_dir: &Path) -> Result<Vec<PlanTask>> {
    let mut tasks = Vec::new();
    let pages = load_pages(wiki_dir)?;
    let mut cited: BTreeSet<String> = BTreeSet::new();

    for p in &pages {
        if p.meta.id == "index" {
            continue;
        }
        for e in &p.meta.evidence {
            cited.insert(e.path.clone());
        }
        let report = check_page(p, repo)?;
        if report.is_stale {
            let why = report
                .evidence_reports
                .iter()
                .filter(|r| !r.ok)
                .map(|r| format!("{}: {}", r.path, r.detail))
                .collect::<Vec<_>>()
                .join("; ");
            tasks.push(PlanTask {
                page_id: p.meta.id.clone(),
                kind: TaskKind::Refresh,
                purpose: format!("evidence stale — {why}"),
                seeds: p.meta.evidence.iter().map(|e| e.path.clone()).collect(),
                status: TaskStatus::Pending,
            });
        }
    }

    for m in discover_modules(repo)? {
        if m.name == ".wiki" || m.name.starts_with(".wiki/") {
            continue;
        }
        let uncovered: Vec<String> = m
            .files
            .iter()
            .filter(|f| !cited.contains(*f) && !f.starts_with(".wiki/"))
            .cloned()
            .collect();
        if uncovered.len() as f64 >= m.files.len() as f64 * 0.5 && !uncovered.is_empty() {
            let mut seeds = uncovered;
            seeds.truncate(12);
            tasks.push(PlanTask {
                page_id: suggest_page_id(&m.name),
                kind: TaskKind::New,
                purpose: format!("module `{}` ({} files) has no wiki coverage", m.name, m.files.len()),
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

/// Open (or resume) a run: carry pending tasks from the latest active run,
/// merge in freshly computed ones, dedupe by page_id (fresh computation wins
/// over stale pending of kind New that a put already resolved).
pub fn plan(repo: &GitRepo, wiki_dir: &Path) -> Result<RunRecord> {
    let mut ledger = load_ledger(wiki_dir);
    let fresh = compute_tasks(repo, wiki_dir)?;

    let carried: Vec<PlanTask> = ledger
        .active_run()
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
        if seen.insert(t.page_id.clone()) {
            tasks.push(t.clone());
        }
    }

    let head = repo.head_commit()?;
    let (base_commit, next_id) = match ledger.runs.last() {
        Some(last) => (last.target_commit.clone(), ledger.runs.len() + 1),
        None => (head.clone(), 1),
    };
    let run = RunRecord {
        id: format!("run-{next_id}-{}", &head.chars().take(8).collect::<String>()),
        started_at: now_iso(),
        base_commit,
        target_commit: head,
        status: "active".into(),
        tasks,
    };
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
    let mut ledger = load_ledger(wiki_dir);
    let mut changed = false;
    if let Some(run) = ledger.runs.iter_mut().rev().find(|r| r.status == "active") {
        for t in &mut run.tasks {
            if t.page_id == page_id && t.status == TaskStatus::Pending {
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
