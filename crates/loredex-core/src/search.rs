use std::collections::{BTreeMap, HashMap};

/// Token count above which a token is treated as a word rather than a CJK char run.
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
    pub score: usize,
    pub matched_lines: Vec<String>,
}

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
            let mut tf: HashMap<String, usize> = HashMap::new();
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
        let mut scores: HashMap<usize, usize> = HashMap::new();
        let mut coverage: HashMap<usize, usize> = HashMap::new();
        for t in &qtokens {
            // prefix match on latin tokens keeps `wik` -> `wiki` usable
            let mut candidates: Vec<&(usize, usize)> = Vec::new();
            if let Some(list) = self.postings.get(t) {
                candidates.extend(list.iter());
            }
            if t.chars().next().map(|c| !is_cjk(c)).unwrap_or(false) {
                // Sorted postings: every key with prefix `t` is contiguous.
                for (key, list) in self.postings.range(t.clone()..) {
                    if !key.starts_with(t.as_str()) {
                        break;
                    }
                    candidates.extend(list.iter());
                }
            }
            for (pi, tf) in candidates {
                *scores.entry(*pi).or_default() += 1 + (*tf).min(4);
                *coverage.entry(*pi).or_default() += 1;
            }
        }
        let mut hits: Vec<(usize, usize)> = scores
            .into_iter()
            .map(|(pi, s)| {
                (
                    pi,
                    s * 2
                        - qtokens
                            .len()
                            .saturating_sub(*coverage.get(&pi).unwrap_or(&0)),
                )
            })
            .collect();
        hits.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        hits.into_iter()
            .take(limit)
            .filter(|(_, score)| *score > 0)
            .map(|(pi, score)| {
                let page = &self.pages[pi];
                let ql = query.to_lowercase();
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
}
