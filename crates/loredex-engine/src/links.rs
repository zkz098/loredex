//! Cross-page link recommendation on existing signals, zero new dependencies:
//! shared citations (a rename or extraction should connect the pages that
//! point at the same code), module adjacency, and sparse TF cosine over the
//! same tokenizer as the search index. Directional: a suggestion is dropped
//! only when the source page already carries `[[wiki:target]]`, so adding one
//! link does not silence the reverse one.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use loredex_core::search::tokenize;
use loredex_core::Status;
use loredex_git::GitView;
use serde::Serialize;

use crate::{load_pages, modules, Result};

const WEIGHT_CITATIONS: usize = 3;
const WEIGHT_MODULE: usize = 2;
const WEIGHT_TEXT: usize = 1;

#[derive(Debug, Clone, Serialize)]
pub struct LinkSuggestion {
    pub from: String,
    pub to: String,
    pub score: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkReport {
    pub suggestions: Vec<LinkSuggestion>,
    pub pages_scanned: usize,
    pub hint: String,
}

/// Rank link candidates over all authored pages. `limit` caps suggestions
/// per source page; pairs scoring below `min_score` are not noise-floored out
/// of existence — they simply are not related enough to interrupt a reader.
pub fn suggest_links_with(
    view: &GitView,
    wiki_dir: &std::path::Path,
    limit: usize,
    min_score: usize,
) -> Result<LinkReport> {
    let pages = load_pages(wiki_dir)?;
    let mut idxs: Vec<usize> = Vec::new();
    for (i, p) in pages.iter().enumerate() {
        if !crate::index::is_generated(&p.meta.id) && p.meta.status != Status::Draft {
            idxs.push(i);
        }
    }

    // file -> modules, for the adjacency signal
    let mods = modules::discover_with(view)?;
    let mut file_modules: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for m in &mods {
        for f in &m.files {
            file_modules
                .entry(f.as_str())
                .or_default()
                .push(m.name.as_str());
        }
    }

    // term frequencies per page (title + id + body, same mix as the index)
    let mut tfs: Vec<HashMap<String, usize>> = Vec::with_capacity(idxs.len());
    let mut norms: Vec<f64> = Vec::with_capacity(idxs.len());
    for &i in &idxs {
        let p = &pages[i];
        let mut tf: HashMap<String, usize> = HashMap::new();
        let text = format!("{} {} {}", p.meta.title, p.meta.id, p.body);
        for t in tokenize(&text) {
            *tf.entry(t).or_insert(0) += 1;
        }
        let n: f64 = tf
            .values()
            .map(|v| (*v as f64) * *v as f64)
            .sum::<f64>()
            .sqrt();
        norms.push(if n == 0.0 { 1.0 } else { n });
        tfs.push(tf);
    }
    // document frequency for idf
    let mut df: HashMap<&str, usize> = HashMap::new();
    for tf in &tfs {
        for t in tf.keys() {
            *df.entry(t.as_str()).or_insert(0) += 1;
        }
    }
    let n_docs = idxs.len().max(1) as f64;
    let idf = |t: &str| (n_docs / (1.0 + df.get(t).copied().unwrap_or(0) as f64)).ln() + 1.0;

    // existing links per page, for directional filtering
    let linked: Vec<BTreeSet<String>> = idxs
        .iter()
        .map(|&i| {
            loredex_core::parse_wiki_links(&pages[i].body)
                .into_iter()
                .collect()
        })
        .collect();

    let cites: Vec<BTreeSet<&str>> = idxs
        .iter()
        .map(|&i| {
            pages[i]
                .meta
                .evidence
                .iter()
                .map(|e| e.path.as_str())
                .collect()
        })
        .collect();

    let mut all: Vec<LinkSuggestion> = Vec::new();
    for a in 0..idxs.len() {
        for b in 0..idxs.len() {
            if a == b {
                continue;
            }
            let shared: BTreeSet<&str> = cites[a].intersection(&cites[b]).copied().collect();
            let shared_modules: BTreeSet<&str> = cites[a]
                .iter()
                .flat_map(|f| {
                    file_modules
                        .get(f)
                        .map(|v| v.iter().copied())
                        .unwrap_or_default()
                })
                .filter(|m| {
                    cites[b]
                        .iter()
                        .any(|g| file_modules.get(g).map(|v| v.contains(m)).unwrap_or(false))
                })
                .collect();
            let mut dots = 0.0f64;
            if !shared.is_empty() || !shared_modules.is_empty() {
                // cosine only decides order among graph-connected pairs:
                // text-only pairs bury unrelated prose under the threshold
                for (t, va) in &tfs[a] {
                    if let Some(vb) = tfs[b].get(t) {
                        let w = idf(t) * *va as f64 * *vb as f64;
                        dots += w;
                    }
                }
            }
            let cos = dots / (norms[a] * norms[b]);
            let text_pts = if cos > 0.08 { WEIGHT_TEXT } else { 0 };
            let score =
                shared.len() * WEIGHT_CITATIONS + shared_modules.len() * WEIGHT_MODULE + text_pts;
            if score < min_score || linked[a].contains(pages[idxs[b]].meta.id.as_str()) {
                continue;
            }
            let mut reasons = Vec::new();
            if !shared.is_empty() {
                let files: Vec<&str> = shared.iter().copied().take(3).collect();
                reasons.push(format!("both cite {}", files.join(", ")));
            }
            if !shared_modules.is_empty() {
                reasons.push(format!(
                    "same module {}",
                    shared_modules
                        .iter()
                        .copied()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if text_pts > 0 {
                reasons.push(format!("text similarity {cos:.2}"));
            }
            all.push(LinkSuggestion {
                from: pages[idxs[a]].meta.id.clone(),
                to: pages[idxs[b]].meta.id.clone(),
                score,
                reasons,
            });
        }
    }
    all.sort_by(|x, y| {
        y.score
            .cmp(&x.score)
            .then(x.from.cmp(&y.from))
            .then(x.to.cmp(&y.to))
    });
    let mut per_page: HashMap<String, usize> = HashMap::new();
    let mut suggestions = Vec::new();
    for s in all {
        let c = per_page.entry(s.from.clone()).or_insert(0);
        if *c < limit {
            *c += 1;
            suggestions.push(s);
        }
    }
    Ok(LinkReport {
        pages_scanned: idxs.len(),
        hint: "add [[wiki:<to>]] to `from` to strengthen the reading path; suggestions are advisory, never lint errors".into(),
        suggestions,
    })
}
