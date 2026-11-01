//! Consolidation analysis over recorded evidence: which pages describe the
//! same thing twice. Computed from the frontmatter pins rather than prose
//! heuristics, and consumed by two surfaces — `loredex organize` lists the
//! pairs as work items, `loredex lint` names them as advisory findings.
//!
//! Two judgements keep this useful instead of noisy. The interesting threshold
//! is *how much* of two pages coincides: two pages that both touch a hot core
//! file are normally describing different aspects of it, so only an overlap
//! covering most of the smaller page reads as duplication. And the unit is the
//! *pair*, never a transitive cluster — overlap is not transitive, so chaining
//! pairs collapses a whole wiki into one fake task.

use std::collections::BTreeSet;
use std::path::Path;

use loredex_core::{Status, WikiPage};
use serde::Serialize;

use crate::{config::Locale, load_pages, Result};

/// `shared * OVERLAP_RATIO >= smaller` i.e. the overlap must cover at least
/// half of the smaller page's distinct evidence files.
const OVERLAP_RATIO: usize = 2;

#[derive(Debug, Clone)]
pub struct Overlap {
    /// the two page ids, in load order (sorted by id)
    pub a: String,
    pub b: String,
    pub shared: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsolidationCandidate {
    /// the two page ids, sorted
    pub pages: Vec<String>,
    /// evidence files the pair pins in common
    pub shared_files: Vec<String>,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrganizeReport {
    pub pages_scanned: usize,
    pub candidates: Vec<ConsolidationCandidate>,
    pub hint: String,
}

/// Pairs whose evidence sets substantially coincide. Generated navigation
/// pages carry no pins and drafts have none yet (`plan` queues them for
/// backfill), so neither participates. A pair that already cross-links is a
/// *declared* division of labor — the readers can find each other — so it is
/// not an open consolidation item.
pub fn overlap_pairs(pages: &[WikiPage]) -> Vec<Overlap> {
    let mut sets: Vec<(&str, BTreeSet<&str>)> = Vec::new();
    let mut links: Vec<BTreeSet<String>> = Vec::new();
    for p in pages {
        if crate::index::is_generated(&p.meta.id) || p.meta.status == Status::Draft {
            continue;
        }
        sets.push((
            p.meta.id.as_str(),
            p.meta.evidence.iter().map(|e| e.path.as_str()).collect(),
        ));
        links.push(
            loredex_core::parse_wiki_links(&p.body)
                .into_iter()
                .collect(),
        );
    }
    let mut out = Vec::new();
    for i in 0..sets.len() {
        for j in i + 1..sets.len() {
            let shared: Vec<&str> = sets[i].1.intersection(&sets[j].1).copied().collect();
            let smaller = sets[i].1.len().min(sets[j].1.len()).max(1);
            if shared.is_empty() || shared.len() * OVERLAP_RATIO < smaller {
                continue;
            }
            let (a, b) = (sets[i].0, sets[j].0);
            if links[i].contains(b) || links[j].contains(a) {
                continue;
            }
            out.push(Overlap {
                a: a.to_string(),
                b: b.to_string(),
                shared: shared.into_iter().map(str::to_string).collect(),
            });
        }
    }
    out
}

/// The same pairs as a worklist, strongest overlap first.
pub fn consolidation_candidates(wiki_dir: &Path) -> Result<OrganizeReport> {
    let pages = load_pages(wiki_dir)?;
    let locale = crate::config::locale(wiki_dir);
    let scanned = pages
        .iter()
        .filter(|p| !crate::index::is_generated(&p.meta.id) && p.meta.status != Status::Draft)
        .count();

    let mut candidates: Vec<ConsolidationCandidate> = overlap_pairs(&pages)
        .into_iter()
        .map(|o| {
            let mut ids = vec![o.a, o.b];
            ids.sort();
            let shared_files = o.shared;
            let count = shared_files.len();
            ConsolidationCandidate {
                suggestion: suggestion(&ids, count, locale),
                pages: ids,
                shared_files,
            }
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.shared_files
            .len()
            .cmp(&a.shared_files.len())
            .then_with(|| a.pages.cmp(&b.pages))
    });

    Ok(OrganizeReport {
        pages_scanned: scanned,
        candidates,
        hint: match locale {
            Locale::En => "one owner per claim: merge the pair into a single page, or split their responsibilities and cross-link them".into(),
            Locale::Zh => "同一条论断只留一个出处:要么合并成一页,要么明确分工后互链".into(),
        },
    })
}

fn suggestion(pages: &[String], shared: usize, locale: Locale) -> String {
    match locale {
        Locale::En => format!(
            "{} pin the same {shared} file(s); merge them or link them to declare the split",
            pages.join(" and ")
        ),
        Locale::Zh => format!(
            "{} 覆盖了相同的 {shared} 个证据文件;合并为一页,或明确分工后互链",
            pages.join(" 与 ")
        ),
    }
}
