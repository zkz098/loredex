//! Consolidation report over evidence overlap: pages citing the same files
//! are prime drift-duplication candidates — the long-term maintenance
//! question "which notes/rules describe the same thing twice" answered from
//! the frontmatter rather than prose heuristics. Advisory like `links`:
//! a cluster is a work item, never a lint error.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use loredex_core::Status;
use serde::Serialize;

use crate::{config::Locale, load_pages, Result};

#[derive(Debug, Clone, Serialize)]
pub struct ConsolidationCandidate {
    /// page ids in the cluster, sorted
    pub pages: Vec<String>,
    /// evidence paths cited by two or more pages of the cluster
    pub shared_files: Vec<String>,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrganizeReport {
    pub pages_scanned: usize,
    pub clusters: Vec<ConsolidationCandidate>,
    pub hint: String,
}

/// Merge pages whose recorded evidence points at the same file, then report
/// clusters of size >= 2. Draft pages have no pins to compare, so they are
/// skipped — `plan` queues them for citation backfill first.
pub fn consolidation_clusters(wiki_dir: &Path) -> Result<OrganizeReport> {
    let pages = load_pages(wiki_dir)?;
    let locale = crate::config::locale(wiki_dir);

    // page index list per evidence path, for the union pass
    let mut ids: Vec<&str> = Vec::new();
    let mut by_file: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for p in pages.iter() {
        if crate::index::is_generated(&p.meta.id) || p.meta.status == Status::Draft {
            continue;
        }
        let i = ids.len();
        let files: BTreeSet<&str> = p.meta.evidence.iter().map(|e| e.path.as_str()).collect();
        for f in files {
            by_file.entry(f).or_default().push(i);
        }
        ids.push(p.meta.id.as_str());
    }

    let mut parent: Vec<usize> = (0..ids.len()).collect();
    let find = |parent: &mut Vec<usize>, mut x: usize| {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    };
    for group in by_file.values() {
        for pair in group.windows(2) {
            let (a, b) = (find(&mut parent, pair[0]), find(&mut parent, pair[1]));
            if a != b {
                parent[b.max(a)] = b.min(a);
            }
        }
    }

    let mut clusters: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..ids.len() {
        clusters.entry(find(&mut parent, i)).or_default().push(i);
    }

    let mut out = Vec::new();
    for members in clusters.values() {
        if members.len() < 2 {
            continue;
        }
        // a chained cluster (a-b share x, b-c share y) has no file every member
        // cites, so report the files that bind at least two of them
        let member_set: BTreeSet<usize> = members.iter().copied().collect();
        let mut shared_files: Vec<String> = by_file
            .iter()
            .filter(|(_, group)| group.iter().filter(|m| member_set.contains(*m)).count() > 1)
            .map(|(path, _)| path.to_string())
            .collect();
        shared_files.sort();
        let mut pages: Vec<String> = members.iter().map(|m| ids[*m].to_string()).collect();
        pages.sort();
        out.push(ConsolidationCandidate {
            suggestion: suggestion(&pages, locale),
            pages,
            shared_files,
        });
    }
    out.sort_by(|a, b| {
        b.pages
            .len()
            .cmp(&a.pages.len())
            .then(a.pages.join(",").cmp(&b.pages.join(",")))
    });

    Ok(OrganizeReport {
        pages_scanned: ids.len(),
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
