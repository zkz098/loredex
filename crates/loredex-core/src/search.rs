use std::collections::{BTreeMap, HashMap};
use std::ops::Bound;

use foldhash::fast::RandomState as FastState;

/// Local page tokens only: foldhash is fast but not HashDoS-resistant.
type FastMap<K, V> = HashMap<K, V, FastState>;

/// Characters indexed as CJK bigrams instead of being accumulated into
/// latin-style words.
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF   // CJK Unified Ideographs
        | 0x3400..=0x4DBF // Ext A
        | 0x3040..=0x30FF // Hiragana/Katakana
        | 0xAC00..=0xD7AF // Hangul
    )
}

/// Tokenize for the inverted index:
/// - latin words split on punctuation and camelCase boundaries
/// - CJK runs split into overlapping bigrams
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut cjk: Vec<char> = Vec::new();

    let flush_word = |word: &mut String, tokens: &mut Vec<String>| {
        if !word.is_empty() {
            tokens.extend(split_word(word).into_iter().map(|s| s.to_lowercase()));
            word.clear();
        }
    };
    let flush_cjk = |cjk: &mut Vec<char>, tokens: &mut Vec<String>| {
        for pair in cjk.windows(2) {
            tokens.push(pair.iter().collect());
        }
        if cjk.len() == 1 {
            tokens.push(cjk[0].to_string());
        }
        cjk.clear();
    };

    for ch in text.chars() {
        if is_cjk(ch) {
            flush_word(&mut word, &mut tokens);
            cjk.push(ch);
        } else if ch.is_alphanumeric() || ch == '_' {
            flush_cjk(&mut cjk, &mut tokens);
            // camelCase boundary: lower->Upper
            if ch.is_uppercase()
                && !word.is_empty()
                && word
                    .chars()
                    .last()
                    .map(|l| l.is_lowercase())
                    .unwrap_or(false)
            {
                flush_word(&mut word, &mut tokens);
            }
            word.push(ch);
        } else {
            flush_word(&mut word, &mut tokens);
            flush_cjk(&mut cjk, &mut tokens);
        }
    }
    flush_word(&mut word, &mut tokens);
    flush_cjk(&mut cjk, &mut tokens);
    tokens
}

/// Split identifiers like `parse_citations` / `myModule` into parts.
fn split_word(w: &str) -> Vec<&str> {
    let mut parts = vec![w];
    for p in w.split('_').filter(|p| !p.is_empty()) {
        if p != w {
            parts.push(p);
        }
    }
    // also split digit/alpha runs inside long tokens (e.g. L10)
    parts
}

#[derive(Debug, Clone)]
pub struct IndexedPage {
    pub id: String,
    pub title: String,
    /// Shared with the caller's page cache; cloning this is cheap.
    pub body: std::sync::Arc<str>,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Index into the page list handed to `InvertedIndex::build`.
    pub page_idx: usize,
    pub id: String,
    pub title: String,
    /// Ranking score, always >= 1 for a returned hit; hits are sorted by it
    /// descending.
    pub score: usize,
    pub matched_lines: Vec<String>,
}

/// Term frequency stops adding weight beyond this many occurrences per page.
const TF_CAP: usize = 4;
/// Weight a query token gets for matching a page token exactly (a prefix-only
/// match gets 1), so `wiki` ranks a page about `wiki` above one about `wikis`.
const EXACT_BONUS: usize = 2;

/// In-memory inverted index; rebuilt from page files in milliseconds,
/// so there is no persistent cache that can drift from the Markdown source.
pub struct InvertedIndex {
    postings: BTreeMap<String, Vec<(usize, usize)>>, // token -> (page_idx, tf)
    pages: Vec<IndexedPage>,
}

impl InvertedIndex {
    pub fn build(pages: Vec<IndexedPage>) -> Self {
        let mut postings: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
        for (pi, page) in pages.iter().enumerate() {
            let mut text = page.title.clone();
            text.push(' ');
            text.push_str(&page.id);
            text.push(' ');
            text.push_str(&page.body);
            let mut tf: FastMap<String, usize> = FastMap::default();
            for t in tokenize(&text) {
                *tf.entry(t).or_insert(0) += 1;
            }
            for (t, n) in tf {
                postings.entry(t).or_default().push((pi, n));
            }
        }
        Self { postings, pages }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let qtokens: Vec<String> = {
            let mut seen = std::collections::BTreeSet::new();
            tokenize(query)
                .into_iter()
                .filter(|t| seen.insert(t.clone()))
                .collect()
        };
        if qtokens.is_empty() {
            return Vec::new();
        }
        // page -> (summed per-token weight, number of query tokens matched)
        let mut acc: FastMap<usize, (usize, usize)> = FastMap::default();
        let mut best: FastMap<usize, usize> = FastMap::default();
        for t in &qtokens {
            // Each query token contributes at most once per page: the best of
            // its exact posting and its prefix expansions. Summing them would
            // count the exact key twice (it is also its own prefix) and let a
            // page with many `t*` variants outrank one that matches `t` itself.
            best.clear();
            if let Some(list) = self.postings.get(t) {
                for &(pi, tf) in list {
                    let w = best.entry(pi).or_default();
                    *w = (*w).max(EXACT_BONUS + tf.min(TF_CAP));
                }
            }
            if t.chars().next().is_some_and(|c| !is_cjk(c)) {
                // prefix match on latin tokens keeps `wik` -> `wiki` usable.
                // Sorted postings: every key with prefix `t` is contiguous,
                // starting right after `t` itself.
                let after = (Bound::Excluded(t.as_str()), Bound::Unbounded);
                for (key, list) in self.postings.range::<str, _>(after) {
                    if !key.starts_with(t.as_str()) {
                        break;
                    }
                    for &(pi, tf) in list {
                        let w = best.entry(pi).or_default();
                        *w = (*w).max(1 + tf.min(TF_CAP));
                    }
                }
            }
            for (&pi, &w) in &best {
                let e = acc.entry(pi).or_default();
                e.0 += w;
                e.1 += 1;
            }
        }
        // Each unmatched query token costs one point, but a page that matched
        // anything keeps a positive score: the old unsigned subtraction
        // underflowed for long queries (debug panic, release score wrapped to
        // usize::MAX and ranked the weakest hit first).
        let mut hits: Vec<(usize, usize, usize)> = acc
            .into_iter()
            .map(|(pi, (sum, covered))| {
                let missing = qtokens.len().saturating_sub(covered);
                (pi, (sum * 2).saturating_sub(missing).max(1), sum)
            })
            .collect();
        hits.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.0.cmp(&b.0))
        });
        let ql = query.to_lowercase();
        hits.into_iter()
            .take(limit)
            .map(|(pi, score, _)| {
                let page = &self.pages[pi];
                let matched_lines = page
                    .body
                    .lines()
                    .filter(|l| {
                        let ll = l.to_lowercase();
                        qtokens.iter().any(|t| ll.contains(t)) || ll.contains(&ql)
                    })
                    .map(|l| l.trim().to_string())
                    .take(4)
                    .collect();
                SearchHit {
                    page_idx: pi,
                    id: page.id.clone(),
                    title: page.title.clone(),
                    score,
                    matched_lines,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_and_snake_split() {
        let t = tokenize("parse_citations myModuleId");
        assert!(t.contains(&"parse".to_string()));
        assert!(t.contains(&"citations".to_string()));
        assert!(t.contains(&"module".to_string()) || t.contains(&"my".to_string()));
    }

    #[test]
    fn cjk_bigrams() {
        let t = tokenize("存储层");
        assert!(t.contains(&"存储".to_string()));
        assert!(t.contains(&"储层".to_string()));
    }

    #[test]
    fn search_ranks_matching_page() {
        let idx = InvertedIndex::build(vec![
            IndexedPage {
                id: "storage".into(),
                title: "Storage layer".into(),
                body: "opens the db and runs migrations".into(),
            },
            IndexedPage {
                id: "ui".into(),
                title: "前端界面".into(),
                body: "渲染组件树".into(),
            },
        ]);
        let hits = idx.search("migrations", 5);
        assert_eq!(hits[0].id, "storage");
        let hits = idx.search("组件", 5);
        assert_eq!(hits[0].id, "ui");
    }

    fn page(id: &str, body: &str) -> IndexedPage {
        IndexedPage {
            id: id.into(),
            title: String::new(),
            body: body.into(),
        }
    }

    #[test]
    fn long_query_with_one_prefix_hit_does_not_underflow() {
        // Regression: 10 query tokens, the page matches one by prefix only.
        // `s * 2 - missing` used to underflow (debug panic / release wrap to
        // usize::MAX, ranking this weakest hit above everything).
        let idx = InvertedIndex::build(vec![
            page("weak", "wiki notes"),
            page("strong", "alpha beta gamma delta wiki"),
        ]);
        let hits = idx.search("wik alpha beta gamma delta epsilon zeta eta theta iota", 5);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "strong");
        assert_eq!(hits[1].id, "weak");
        assert!(
            hits.iter().all(|h| h.score >= 1 && h.score < 1000),
            "{hits:?}"
        );
    }

    #[test]
    fn exact_token_is_counted_once_and_beats_prefix_variants() {
        let idx = InvertedIndex::build(vec![
            page("variants", "wikis wikipedia wikilink"),
            page("exact", "wiki"),
        ]);
        let hits = idx.search("wiki", 5);
        assert_eq!(hits[0].id, "exact", "{hits:?}");
        // exact: (2 + tf 1) * 2 = 6; variants: best prefix (1 + 1) * 2 = 4
        assert_eq!(hits[0].score, 6);
        assert_eq!(hits[1].score, 4);
    }

    #[test]
    fn matching_more_query_tokens_ranks_higher() {
        let idx = InvertedIndex::build(vec![
            page("one", "storage storage storage storage"),
            page("both", "storage layer"),
        ]);
        let hits = idx.search("storage layer", 5);
        assert_eq!(hits[0].id, "both", "{hits:?}");
    }
}
