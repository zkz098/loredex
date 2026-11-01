//! Range-scoped staleness: `plan`/`check` answer "which pages drifted from
//! their own recorded commit"; this answers "which pages does *your* ref
//! touch" — the question a PR reviewer or release bot asks. Evidence joins
//! against the structured delta set on both sides (current path or the old
//! path of a rename), so a `git mv` shows up on the page that cites it.

use std::path::Path;

use loredex_git::GitView;
use serde::Serialize;

use crate::{load_pages, Result};

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceHit {
    pub path: String,
    pub lines: Option<[u32; 2]>,
    pub change: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed_from: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffPageHit {
    pub id: String,
    pub title: String,
    pub status: String,
    pub evidence: Vec<EvidenceHit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub since: String,
    /// full commit sha the ref resolved to
    pub resolved_ref: String,
    pub head: String,
    /// false when `since` is not an ancestor of HEAD (parallel branch or a
    /// ref ahead of it): the symmetric delta is still exact
    pub since_is_ancestor: bool,
    pub changed_paths: usize,
    pub affected_pages: Vec<DiffPageHit>,
    pub clean_pages: usize,
}

pub fn diff_since_with(view: &GitView, wiki_dir: &Path, since: &str) -> Result<DiffReport> {
    let deltas = view.path_deltas(since)?;
    let resolved = view.resolve_ref_commit(since)?;
    let pages = load_pages(wiki_dir)?;
    let head = view.head_commit()?.to_string();
    let short = |s: &str| s.chars().take(9).collect::<String>();

    let mut affected: Vec<DiffPageHit> = Vec::new();
    let mut clean = 0usize;
    for p in &pages {
        if crate::index::is_generated(&p.meta.id) {
            continue;
        }
        let mut hits = Vec::new();
        for e in &p.meta.evidence {
            // rename pairs match from either side: a page citing the old path
            // is affected because it will go stale; one citing the new path
            // is affected because the content moved under it
            let Some(d) = deltas.iter().find(|d| {
                d.new_path() == Some(e.path.as_str()) || d.old_path() == Some(e.path.as_str())
            }) else {
                continue;
            };
            // a modified/added delta carries old == new; requiring a path
            // difference keeps it out of the rename fields
            let renamed_to = (d.old_path() == Some(e.path.as_str())
                && d.new_path() != Some(e.path.as_str()))
            .then(|| d.new.clone())
            .flatten();
            let renamed_from = (d.new_path() == Some(e.path.as_str())
                && d.old_path() != Some(e.path.as_str()))
            .then(|| d.old.clone())
            .flatten();
            hits.push(EvidenceHit {
                path: e.path.clone(),
                lines: e.lines,
                change: d.change.as_str().to_string(),
                renamed_to,
                renamed_from,
            });
        }
        if hits.is_empty() {
            clean += 1;
        } else {
            affected.push(DiffPageHit {
                id: p.meta.id.clone(),
                title: p.meta.title.clone(),
                status: p.meta.status.as_str().to_string(),
                evidence: hits,
            });
        }
    }

    Ok(DiffReport {
        since: short(&resolved),
        resolved_ref: resolved,
        head: short(&head),
        since_is_ancestor: view.is_ancestor(since, &head).unwrap_or(false),
        changed_paths: deltas.len(),
        affected_pages: affected,
        clean_pages: clean,
    })
}

/// Convenience over the repo-owned view.
pub fn diff_since(
    repo: &loredex_git::GitRepo,
    wiki_dir: &std::path::Path,
    since: &str,
) -> Result<DiffReport> {
    diff_since_with(&repo.view(), wiki_dir, since)
}
