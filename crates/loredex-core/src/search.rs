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
    /// Ranking score (scaled BM25), always >= 1 for a returned hit; hits are
    /// sorted by it descending.
    pub score: usize,
    pub matched_lines: Vec<String>,
}

/// Okapi BM25 parameters.
const BM25_K1: f32 = 1.2;
const BM25_B: f32 = 0.75;
/// Discount applied to prefix-expanded tokens compared to exact token matches.
const PREFIX_DISCOUNT: f32 = 0.6;
/// Bonus factor for an exact token match.
const EXACT_MATCH_BONUS: f32 = 1.25;

/// Calculate non-negative smoothed IDF (Okapi BM25 / Lucene variant):
/// ln(1.0 + (N - n + 0.5) / (n + 0.5))
#[inline]
fn idf(total_docs: usize, doc_freq: usize) -> f32 {
    let n = doc_freq as f32;
    let total = total_docs as f32;
    ((1.0 + (total - n + 0.5) / (n + 0.5)).max(1.0)).ln()
}

/// Calculate BM25 term saturation with document length normalization:
/// (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * (|D| / avgdl)))
#[inline]
fn bm25_tf(tf: u32, doc_len: u32, avgdl: f32) -> f32 {
    let tf = tf as f32;
    let len_norm = (1.0 - BM25_B) + BM25_B * (doc_len as f32 / avgdl.max(1.0));
    (tf * (BM25_K1 + 1.0)) / (tf + BM25_K1 * len_norm)
}

/// In-memory inverted index; rebuilt from page files in milliseconds,
/// so there is no persistent cache that can drift from the Markdown source.
pub struct InvertedIndex {
    postings: BTreeMap<String, Vec<(usize, u32)>>, // token -> (page_idx, tf)
    doc_lens: Vec<u32>,
    avgdl: f32,
    pages: Vec<IndexedPage>,
}

impl InvertedIndex {
    pub fn build(pages: Vec<IndexedPage>) -> Self {
        let mut postings: BTreeMap<String, Vec<(usize, u32)>> = BTreeMap::new();
        let mut doc_lens = Vec::with_capacity(pages.len());
        let mut total_len = 0usize;

        for (pi, page) in pages.iter().enumerate() {
            let mut text =
                String::with_capacity(page.title.len() + page.id.len() + page.body.len() + 2);
            text.push_str(&page.title);
            text.push(' ');
            text.push_str(&page.id);
            text.push(' ');
            text.push_str(&page.body);

            let tokens = tokenize(&text);
            let d_len = tokens.len() as u32;
            doc_lens.push(d_len);
            total_len += d_len as usize;

            let mut tf: FastMap<String, u32> = FastMap::default();
            for t in tokens {
                *tf.entry(t).or_insert(0) += 1;
            }
            for (t, n) in tf {
                postings.entry(t).or_default().push((pi, n));
            }
        }

        let avgdl = if pages.is_empty() {
            1.0
        } else {
            (total_len as f32) / (pages.len() as f32)
        };

        Self {
            postings,
            doc_lens,
            avgdl,
            pages,
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let qtokens: Vec<String> = {
            let mut seen = std::collections::BTreeSet::new();
            tokenize(query)
                .into_iter()
                .filter(|t| seen.insert(t.clone()))
                .collect()
        };
        if qtokens.is_empty() || self.pages.is_empty() {
            return Vec::new();
        }

        let total_docs = self.pages.len();
        // page -> (accumulated BM25 score, number of distinct query tokens matched)
        let mut acc: FastMap<usize, (f32, usize)> = FastMap::default();
        let mut best: FastMap<usize, f32> = FastMap::default();

        for t in &qtokens {
            best.clear();

            // 1. Exact token match with BM25 weighting
            if let Some(list) = self.postings.get(t) {
                let term_idf = idf(total_docs, list.len());
                for &(pi, tf) in list {
                    let d_len = self.doc_lens[pi];
                    let score = term_idf * bm25_tf(tf, d_len, self.avgdl) * EXACT_MATCH_BONUS;
                    let w = best.entry(pi).or_default();
                    *w = (*w).max(score);
                }
            }

            // 2. Prefix expansion for latin tokens (skip CJK)
            if t.chars().next().is_some_and(|c| !is_cjk(c)) {
                let after = (Bound::Excluded(t.as_str()), Bound::Unbounded);
                for (key, list) in self.postings.range::<str, _>(after) {
                    if !key.starts_with(t.as_str()) {
                        break;
                    }
                    let term_idf = idf(total_docs, list.len());
                    for &(pi, tf) in list {
                        let d_len = self.doc_lens[pi];
                        let score = term_idf * bm25_tf(tf, d_len, self.avgdl) * PREFIX_DISCOUNT;
                        let w = best.entry(pi).or_default();
                        *w = (*w).max(score);
                    }
                }
            }

            // Each query token contributes at most once per page
            for (&pi, &w) in &best {
                let e = acc.entry(pi).or_default();
                e.0 += w;
                e.1 += 1;
            }
        }

        // Rank documents:
        // Combined score = sum(BM25) * coverage_multiplier
        // where coverage rewards documents matching more of the query tokens.
        let mut hits: Vec<(usize, usize, f32)> = acc
            .into_iter()
            .map(|(pi, (raw_score, covered))| {
                let coverage_boost = 1.0 + 0.5 * (covered as f32 / qtokens.len() as f32);
                let final_score = raw_score * coverage_boost;
                let int_score = ((final_score * 100.0).round() as usize).max(1);
                (pi, int_score, final_score)
            })
            .collect();

        hits.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
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
        assert!(
            hits[0].score > hits[1].score,
            "exact ({}) should beat variants ({})",
            hits[0].score,
            hits[1].score
        );
    }

    #[test]
    fn idf_favors_rare_keywords_over_generic_words() {
        // "wiki" appears in all 3 pages; "flock" appears in only 1.
        // Searching for "wiki flock" should rank the page with "flock" highest,
        // even if another page repeats "wiki" repeatedly.
        let idx = InvertedIndex::build(vec![
            page("generic", "wiki wiki wiki wiki wiki wiki"),
            page("both", "wiki system uses flock for process coordination"),
            page("other", "wiki documentation page"),
        ]);
        let hits = idx.search("wiki flock", 5);
        assert_eq!(hits[0].id, "both", "page with rare term must rank first");
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn length_normalization_favors_concise_exact_matches() {
        // Both pages have 1 occurrence of "deadlock", but "short" is 5 tokens
        // while "verbose" is a long document with 100 filler words.
        let verbose_body = format!("deadlock {}", "filler ".repeat(100));
        let idx = InvertedIndex::build(vec![
            page("verbose", &verbose_body),
            page("concise", "avoid deadlock in file locking"),
        ]);
        let hits = idx.search("deadlock", 5);
        assert_eq!(
            hits[0].id, "concise",
            "concise document should get length-norm boost"
        );
        assert!(hits[0].score > hits[1].score);
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
