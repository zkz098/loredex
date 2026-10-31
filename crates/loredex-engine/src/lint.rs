//! Structural wiki checks (openwiki's missing CI guard):
//! - broken [[wiki:id]] links
//! - pages marked verified that git says are stale
//! - body/frontmatter drift: hand-edited markers no longer matching evidence
//!   (openwiki forbids hand-edits outright; Loredex detects instead)

use std::collections::BTreeSet;
use std::path::Path;

use loredex_core::{dedup_citations, parse_citations, parse_wiki_links, Status};
use loredex_git::{GitRepo, GitView};
use serde::Serialize;

use crate::{check_page_with, load_pages, Result};

#[derive(Debug, Clone, Serialize)]
pub struct LintIssue {
    pub page: String,
    pub kind: String, // broken-link | stale-but-verified | evidence-drift
    pub detail: String,
}

pub fn lint(repo: &GitRepo, wiki_dir: &Path) -> Result<Vec<LintIssue>> {
    lint_with(&repo.view(), wiki_dir)
}

pub fn lint_with(view: &GitView, wiki_dir: &Path) -> Result<Vec<LintIssue>> {
    let pages = load_pages(wiki_dir)?;
    crate::prime_view(view, &pages)?;
    let ids: BTreeSet<String> = pages.iter().map(|p| p.meta.id.clone()).collect();
    // Config-derived data is constant for the whole run; read it once instead
    // of re-parsing wiki.toml for every page.
    let playbooks = crate::playbook::registry(wiki_dir);
    let mut issues = Vec::new();

    for p in &pages {
        // 1. broken cross-page links (one pass over the body)
        for target in parse_wiki_links(&p.body) {
            if !ids.contains(&target) {
                issues.push(LintIssue {
                    page: p.meta.id.clone(),
                    kind: "broken-link".into(),
                    detail: format!("[[wiki:{target}]] has no such page"),
                });
            }
        }

        if crate::index::is_generated(&p.meta.id) {
            continue; // generated, no evidence
        }
        // 0. declared type must exist in the playbook registry
        if let Some(t) = &p.meta.page_type {
            if !playbooks.contains_key(t) {
                issues.push(LintIssue {
                    page: p.meta.id.clone(),
                    kind: "unknown-type".into(),
                    detail: format!(
                        "type `{t}` has no playbook; known: {:?} (extend via .wiki/wiki.toml [page_types.{t}])",
                        playbooks.keys().collect::<Vec<_>>()
                    ),
                });
            }
        }

        // 2. status field contradicts live git state
        if p.meta.status != Status::Stale && p.meta.status != Status::Draft {
            if let Ok(r) = check_page_with(p, view) {
                if r.is_stale && p.meta.status == Status::Verified {
                    let why = r
                        .evidence_reports
                        .iter()
                        .filter(|e| !e.ok)
                        .map(|e| format!("{}: {}", e.path, e.detail))
                        .collect::<Vec<_>>()
                        .join("; ");
                    issues.push(LintIssue {
                        page: p.meta.id.clone(),
                        kind: "stale-but-verified".into(),
                        detail: why,
                    });
                }
            }
        }

        // 3. body evidence markers vs frontmatter evidence drift.
        //    frontmatter is authoritative (it holds blob pins); body markers
        //    are narrative. put_page derives one from the other, so a
        //    mismatch means a hand edit happened after the fact.
        if let Ok(cites) = parse_citations(&p.body) {
            let body_set: BTreeSet<(String, Option<[u32; 2]>)> = dedup_citations(cites)
                .into_iter()
                .map(|c| (c.path, c.lines))
                .collect();
            let fm_set: BTreeSet<(String, Option<[u32; 2]>)> = p
                .meta
                .evidence
                .iter()
                .map(|e| (e.path.clone(), e.lines))
                .collect();
            // projected/renamed evidence legitimately differs from original markers;
            // flag only missing paths (marker citing a path with no evidence entry)
            let fm_paths: BTreeSet<String> = fm_set.iter().map(|(path, _)| path.clone()).collect();
            for (path, _) in &body_set {
                if !fm_paths.contains(path) {
                    issues.push(LintIssue {
                        page: p.meta.id.clone(),
                        kind: "evidence-drift".into(),
                        detail: format!(
                            "body cites {path} but frontmatter has no evidence for it (hand edit? re-put the page)"
                        ),
                    });
                }
            }
        }
    }
    Ok(issues)
}
