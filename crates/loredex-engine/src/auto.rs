//! `wiki auto`: one-call maintenance orchestration for hooks and CI.
//! Sequence: git check -> algorithmic recovery (drift/rename projection) ->
//! lint -> remaining work as a fresh plan run. The LLM-free part repairs
//! itself; only genuine re-authoring is left for agents, with the worklist
//! already persisted in the run ledger.

use std::collections::BTreeSet;
use std::path::Path;

use loredex_core::{dedup_citations, parse_citations, Status, WikiPage};
use loredex_git::{GitRepo, GitView};
use serde::Serialize;

use crate::drift::refresh_page_with_drift_view_deferred;
use crate::put::{repin_page_with, PutOptions};
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
    /// true when any page failed recovery (`needs_rewrite`) or has a non-advisory lint defect
    pub has_errors: bool,
    pub all_clean: bool,
}

pub fn auto_refresh(repo: &GitRepo, wiki_dir: &Path) -> Result<AutoReport> {
    auto_refresh_with(&repo.view(), wiki_dir)
}

pub fn auto_refresh_with(view: &GitView, wiki_dir: &Path) -> Result<AutoReport> {
    auto_refresh_with_options(view, wiki_dir, false)
}

/// Run `auto` maintenance with optional `--fix` progressive refactoring mode:
/// - always syncs renamed file paths into both YAML frontmatter and Markdown body citations
/// - when `fix == true`, also rewrites shifted `:Lx-Ly` line numbers in Markdown bodies,
///   re-pins pages whose body `[[cite:...]]` markers were hand-edited, and promotes
///   repaired `draft`/`stale` pages whose citations resolve cleanly at HEAD to `verified`.
pub fn auto_refresh_with_options(view: &GitView, wiki_dir: &Path, fix: bool) -> Result<AutoReport> {
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
        let is_stale = check_page_with(&p, view)?.is_stale;

        // If the agent hand-edited `[[cite:...]]` markers in the body (so body
        // citations no longer match frontmatter), `--fix` re-pins frontmatter
        // directly from the body at HEAD.
        if fix && body_citations_edited(&p) {
            if let Ok(outcome) = repin_page_with(view, wiki_dir, &p.meta.id, &PutOptions::default())
            {
                completed.push(p.meta.id.clone());
                recovered.push(RecoveredPage {
                    page_id: p.meta.id.clone(),
                    summary: format!(
                        "repinned {} citation(s) from body -> verified",
                        outcome.citations
                    ),
                });
                continue;
            }
        }

        if !is_stale {
            // Even when git-fresh, heal any split rename where frontmatter already
            // recorded `renamed old -> new` but the body still cited `old`.
            if body_has_missing_cite_path(view, &p) {
                if let Ok(outcome) =
                    repin_page_with(view, wiki_dir, &p.meta.id, &PutOptions::default())
                {
                    completed.push(p.meta.id.clone());
                    recovered.push(RecoveredPage {
                        page_id: p.meta.id.clone(),
                        summary: format!(
                            "synced renamed body citations ({} citation(s))",
                            outcome.citations
                        ),
                    });
                    continue;
                }
            }
            // In `--fix` mode, a previously draft/stale page that is already at HEAD
            // with valid resolvable citations in its body is promoted to `verified`.
            if fix && p.meta.status != Status::Verified && has_citations(&p.body) {
                if let Ok(outcome) =
                    repin_page_with(view, wiki_dir, &p.meta.id, &PutOptions::default())
                {
                    completed.push(p.meta.id.clone());
                    recovered.push(RecoveredPage {
                        page_id: p.meta.id.clone(),
                        summary: format!(
                            "promoted {} -> verified ({} citation(s))",
                            p.meta.status.as_str(),
                            outcome.citations
                        ),
                    });
                }
            }
            continue;
        }

        let mut page = p.clone();
        match refresh_page_with_drift_view_deferred(&mut page, view, wiki_dir, fix) {
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
    let has_errors = !needs_rewrite.is_empty() || defects > 0;
    let all_clean = !has_errors && pending == 0;

    Ok(AutoReport {
        head: view.head_short()?,
        checked,
        recovered,
        needs_rewrite,
        lint_issues,
        plan: run,
        has_errors,
        all_clean,
    })
}

fn has_citations(body: &str) -> bool {
    parse_citations(body)
        .map(|c| !c.is_empty())
        .unwrap_or(false)
}

fn body_has_missing_cite_path(view: &GitView, page: &WikiPage) -> bool {
    let Ok(cites) = parse_citations(&page.body) else {
        return false;
    };
    let fm_paths: BTreeSet<&str> = page.meta.evidence.iter().map(|e| e.path.as_str()).collect();
    cites.iter().any(|c| {
        !fm_paths.contains(c.path.as_str()) || view.head_blob_oid(&c.path).ok().flatten().is_none()
    })
}

fn body_citations_edited(page: &WikiPage) -> bool {
    let Ok(cites) = parse_citations(&page.body) else {
        return false;
    };
    let deduped = dedup_citations(cites);
    if deduped.is_empty() {
        return false;
    }
    if deduped.len() != page.meta.evidence.len() {
        return true;
    }
    for (c, ev) in deduped.iter().zip(page.meta.evidence.iter()) {
        if c.path != ev.path {
            return true;
        }
        if c.lines != ev.lines {
            // Check whether `ev.note` merely reflects an earlier projection from `c.lines`
            let from_projection = match (c.lines, ev.note.as_deref()) {
                (Some([lo, hi]), Some(note)) => note.contains(&format!("L{lo}-L{hi}")),
                _ => false,
            };
            if !from_projection {
                return true;
            }
        }
    }
    false
}
