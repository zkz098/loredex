//! `wiki auto`: one-call maintenance orchestration for hooks and CI.
//! Sequence: git check -> algorithmic recovery (drift/rename projection) ->
//! lint -> remaining work as a fresh plan run. The LLM-free part repairs
//! itself; only genuine re-authoring is left for agents, with the worklist
//! already persisted in the run ledger.

use std::path::Path;

use serde::Serialize;
use wiki_git::GitRepo;

use crate::{
    check_page, lint, load_pages, plan, Result, LintIssue, RunRecord,
    refresh_page_with_drift, DriftOutcome, TaskStatus,
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
    let mut recovered = Vec::new();
    let mut needs_rewrite = Vec::new();
    let mut checked = 0usize;

    for p in load_pages(wiki_dir)? {
        if crate::index::is_generated(&p.meta.id) {
            continue;
        }
        checked += 1;
        if !check_page(&p, repo)?.is_stale {
            continue;
        }
        let mut page = p.clone();
        match refresh_page_with_drift(&mut page, repo, wiki_dir) {
            Ok(outcomes) => {
                let lost: Vec<String> = outcomes
                    .iter()
                    .filter(|(_, o)| {
                        matches!(o, DriftOutcome::Lost | DriftOutcome::NoOldBlob)
                    })
                    .map(|(path, _)| path.clone())
                    .collect();
                if lost.is_empty() {
                    recovered.push(RecoveredPage {
                        page_id: p.meta.id.clone(),
                        summary: outcomes
                            .iter()
                            .map(|(_, o)| match o {
                                DriftOutcome::Projected { lines, score } => {
                                    format!("L{}-L{} ({score:.2})", lines[0], lines[1])
                                }
                                DriftOutcome::Renamed { new_path, exact } => {
                                    format!("renamed->{new_path}{}", if *exact { " (exact)" } else { "" })
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

    let lint_issues = lint(repo, wiki_dir)?;
    let run = plan::plan(repo, wiki_dir)?;
    let pending = run
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Pending)
        .count();
    let all_clean = needs_rewrite.is_empty() && lint_issues.is_empty() && pending == 0;

    Ok(AutoReport {
        head: repo.head_short()?,
        checked,
        recovered,
        needs_rewrite,
        lint_issues,
        plan: run,
        all_clean,
    })
}
