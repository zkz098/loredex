//! `wiki auto`: one-call maintenance orchestration for hooks and CI.
//! Sequence: git check -> algorithmic recovery (drift/rename projection) ->
//! lint -> remaining work as a fresh plan run. The LLM-free part repairs
//! itself; only genuine re-authoring is left for agents, with the worklist
//! already persisted in the run ledger.

use std::path::Path;

use loredex_git::{GitRepo, GitView};
use serde::Serialize;

use crate::drift::refresh_page_with_drift_view_deferred;
use crate::{
    check_page_with, lint_with, load_pages, plan::mark_tasks, plan::plan_with_view, DriftOutcome,
    LintIssue, Result, RunRecord, TaskStatus,
};

#[derive(Debug, Serialize)]
pub struct RecoveredPage {
    pub page_id: String,
    pub summary: String,
}

#[derive(Debug, Serialize)]
pub struct NeedsRewrite {
    pub page_id: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct AutoReport {
    pub head: String,
    pub checked: usize,
    /// pages whose evidence was repaired by projection/rename recovery (zero tokens)
    pub recovered: Vec<RecoveredPage>,
    /// pages still requiring author attention (draft after Lost, or unrecoverable)
    pub needs_rewrite: Vec<NeedsRewrite>,
    pub lint_issues: Vec<LintIssue>,
    /// fresh plan opened for whatever remains
    pub plan: RunRecord,
    pub all_clean: bool,
}

pub fn auto_refresh(repo: &GitRepo, wiki_dir: &Path) -> Result<AutoReport> {
    auto_refresh_with(&repo.view(), wiki_dir)
}

pub fn auto_refresh_with(view: &GitView, wiki_dir: &Path) -> Result<AutoReport> {
    let mut recovered = Vec::new();
    let mut needs_rewrite = Vec::new();
    let mut checked = 0usize;
    let mut completed: Vec<String> = Vec::new();

    let pages = load_pages(wiki_dir)?;
    crate::prime_view(view, &pages)?;
    for p in pages {
        if crate::index::is_generated(&p.meta.id) {
            continue;
        }
        checked += 1;
        if !check_page_with(&p, view)?.is_stale {
            continue;
        }
        let mut page = p.clone();
        match refresh_page_with_drift_view_deferred(&mut page, view, wiki_dir) {
            Ok(outcomes) => {
                let lost: Vec<String> = outcomes
                    .iter()
                    .filter(|(_, o)| matches!(o, DriftOutcome::Lost | DriftOutcome::NoOldBlob))
                    .map(|(path, _)| path.clone())
                    .collect();
                if lost.is_empty() {
                    completed.push(p.meta.id.clone());
                    recovered.push(RecoveredPage {
                        page_id: p.meta.id.clone(),
                        summary: outcomes
                            .iter()
                            .map(|(_, o)| match o {
                                DriftOutcome::Projected { lines, score } => {
                                    format!("L{}-L{} ({score:.2})", lines[0], lines[1])
                                }
                                DriftOutcome::Renamed { new_path, exact } => {
                                    format!(
                                        "renamed->{new_path}{}",
                                        if *exact { " (exact)" } else { "" }
                                    )
                                }
                                _ => "ok".to_string(),
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                    });
                } else {
                    needs_rewrite.push(NeedsRewrite {
                        page_id: p.meta.id.clone(),
                        reason: format!("citation content lost for: {}", lost.join(", ")),
                    });
                }
            }
            Err(e) => needs_rewrite.push(NeedsRewrite {
                page_id: p.meta.id.clone(),
                reason: e.to_string(),
            }),
        }
    }
    // One ledger read+write for the whole batch: marking per page is quadratic
    // in ledger size and dominated auto on large wikis.
    let _ = mark_tasks(
        wiki_dir,
        completed.iter().map(String::as_str),
        TaskStatus::Done,
    );

    let lint_issues = lint_with(view, wiki_dir)?;
    let run = plan_with_view(view, wiki_dir, false)?;
    let pending = run
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Pending)
        .count();
    // advisory findings (overlap hints) are curation suggestions, not defects:
    // they must not make a hook/CI run report the wiki as unclean
    let defects = lint_issues.iter().filter(|i| !i.advisory).count();
    let all_clean = needs_rewrite.is_empty() && defects == 0 && pending == 0;

    Ok(AutoReport {
        head: view.head_short()?,
        checked,
        recovered,
        needs_rewrite,
        lint_issues,
        plan: run,
        all_clean,
    })
}
