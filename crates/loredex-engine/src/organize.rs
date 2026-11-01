//! Consolidation analysis over recorded evidence: which pages describe the
//! same thing twice. Computed from the frontmatter pins rather than prose
//! heuristics, and consumed by two surfaces — `loredex organize` groups the
//! pairs into clusters, `loredex lint` names them as advisory findings.
//!
//! The interesting threshold is *how much* of two pages coincides. Two pages
//! that both touch a hot core file are normally describing different aspects
//! of it; only when the shared files make up most of the smaller page does
//! duplication become the likelier reading.

use std::collections::{BTreeMap, BTreeSet};
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
    /// page ids in the cluster, sorted
    pub pages: Vec<String>,
    /// evidence files binding at least one qualifying pair of the cluster
    pub shared_files: Vec<String>,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrganizeReport {
    pub pages_scanned: usize,
    pub clusters: Vec<ConsolidationCandidate>,
    pub hint: String,
}

/// Pairs whose evidence sets substantially coincide. Generated navigation
/// pages carry no pins and drafts have none yet (`plan` queues them for
/// backfill), so neither participates.
pub fn overlap_pairs(pages: &[WikiPage]) -> Vec<Overlap> {
    let mut sets: Vec<(&str, BTreeSet<&str>)> = Vec::new();
    for p in pages {
        if crate::index::is_generated(&p.meta.id) || p.meta.status == Status::Draft {
            continue;
        }
        sets.push((
            p.meta.id.as_str(),
            p.meta.evidence.iter().map(|e| e.path.as_str()).collect(),
        ));
    }
    let mut out = Vec::new();
    for i in 0..sets.len() {
        for j in i + 1..sets.len() {
            let shared: Vec<&str> = sets[i].1.intersection(&sets[j].1).copied().collect();
            let smaller = sets[i].1.len().min(sets[j].1.len()).max(1);
            if shared.is_empty() || shared.len() * OVERLAP_RATIO < smaller {
                continue;
            }
            out.push(Overlap {
                a: sets[i].0.to_string(),
                b: sets[j].0.to_string(),
                shared: shared.into_iter().map(str::to_string).collect(),
            });
        }
    }
    out
}

/// Connected components of the overlap graph: a page pair is a duplicate
/// signal, and pages chained by such pairs are one consolidation job.
pub fn consolidation_clusters(wiki_dir: &Path) -> Result<OrganizeReport> {
    let pages = load_pages(wiki_dir)?;
    let locale = crate::config::locale(wiki_dir);
    let pairs = overlap_pairs(&pages);
    let scanned = pages
        .iter()
        .filter(|p| !crate::index::is_generated(&p.meta.id) && p.meta.status != Status::Draft)
        .count();

    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for o in &pairs {
        adj.entry(o.a.as_str()).or_default().push(o.b.as_str());
        adj.entry(o.b.as_str()).or_default().push(o.a.as_str());
    }

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for start in adj.keys() {
        if !seen.insert(start) {
            continue;
        }
        let mut stack = vec![*start];
        let mut comp: BTreeSet<&str> = BTreeSet::new();
        while let Some(x) = stack.pop() {
            comp.insert(x);
            for n in adj.get(x).map(Vec::as_slice).unwrap_or_default() {
                if seen.insert(n) {
                    stack.push(n);
                }
            }
        }
        if comp.len() < 2 {
            continue;
        }
        let mut shared_files: BTreeSet<&str> = BTreeSet::new();
        for o in &pairs {
            if comp.contains(o.a.as_str()) && comp.contains(o.b.as_str()) {
                shared_files.extend(o.shared.iter().map(String::as_str));
            }
        }
        let ids: Vec<String> = comp.iter().map(|s| s.to_string()).collect();
        out.push(ConsolidationCandidate {
            suggestion: suggestion(&ids, locale),
            pages: ids,
            shared_files: shared_files.iter().map(|s| s.to_string()).collect(),
        });
    }
    out.sort_by(|a, b| {
        b.pages
            .len()
            .cmp(&a.pages.len())
            .then(a.pages.join(",").cmp(&b.pages.join(",")))
    });

    Ok(OrganizeReport {
        pages_scanned: scanned,
        clusters: out,
        hint: match locale {
            Locale::En => "one owner per claim: merge the cluster into a single page and cross-link the survivors with [[wiki:id]]; `loredex links` ranks the links worth adding".into(),
            Locale::Zh => "同一条论断只留一个出处:把簇合并成单页,幸存页面用 [[wiki:id]] 互链;该链哪些由 `loredex links` 排序给出".into(),
        },
    })
}

fn suggestion(pages: &[String], locale: Locale) -> String {
    match locale {
        Locale::En => format!(
            "{} describe the same evidence; consolidate into one page or link them",
            pages.join(", ")
        ),
        Locale::Zh => format!(
            "{} 覆盖了相同证据;合并为一页,或明确分工后互链",
            pages.join("、")
        ),
    }
}
