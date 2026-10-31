// Citation syntax is enforced at page-write time: a body
// without markers is rejected by wiki-engine::put_page.
use regex::Regex;

use crate::error::{WikiError, Result};

/// A citation marker parsed out of wiki body text:
/// `[[cite:src/db.rs:L10-L88]]` or `[[cite:src/db.rs]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    pub path: String,
    pub lines: Option<[u32; 2]>,
    /// byte range of the full marker within the body, for diagnostics
    pub span: (usize, usize),
}

pub fn parse_citations(body: &str) -> Result<Vec<Citation>> {
    let re = Regex::new(r"\[\[cite:([^\]\s]+?)(?::L(\d+)-L(\d+))?\]\]")
        .map_err(|e| WikiError::FrontmatterParse("cite-regex".into(), e.to_string()))?;
    let mut out = Vec::new();
    for caps in re.captures_iter(body) {
        let m = caps.get(0).unwrap();
        let path = caps[1].to_string();
        let lines = match (caps.get(2), caps.get(3)) {
            (Some(lo), Some(hi)) => {
                let (lo, hi) = (lo.as_str().parse::<u32>().unwrap(), hi.as_str().parse::<u32>().unwrap());
                if lo == 0 || lo > hi {
                    return Err(WikiError::FrontmatterParse(
                        path,
                        format!("invalid line range L{lo}-L{hi} (lines are 1-based, lo<=hi)"),
                    ));
                }
                Some([lo, hi])
            }
            _ => None,
        };
        out.push(Citation {
            path,
            lines,
            span: (m.start(), m.end()),
        });
    }
    Ok(out)
}

/// Validate parsed citations and return them deduplicated by (path, lines).
pub fn dedup_citations(cites: Vec<Citation>) -> Vec<Citation> {
    let mut seen = std::collections::BTreeSet::new();
    cites
        .into_iter()
        .filter(|c| seen.insert((c.path.clone(), c.lines)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_range_and_plain() {
        let body = "See [[cite:src/db.rs:L10-L88]] and also [[cite:README.md]].";
        let mut cites = parse_citations(body).unwrap();
        assert_eq!(cites.len(), 2);
        assert_eq!(cites[0].path, "src/db.rs");
        assert_eq!(cites[0].lines, Some([10, 88]));
        assert_eq!(cites[1].path, "README.md");
        assert_eq!(cites[1].lines, None);
    }

    #[test]
    fn rejects_invalid_range() {
        let err = parse_citations("[[cite:y.rs:L5-L1]]");
        assert!(err.is_err());
    }

    #[test]
    fn dedups() {
        let body = "[[cite:a.rs:L1-L2]] x [[cite:a.rs:L1-L2]] y [[cite:a.rs]]";
        let cites = dedup_citations(parse_citations(body).unwrap());
        assert_eq!(cites.len(), 2);
    }
}

/// Extract `[[wiki:<page-id>]]` cross-page link targets from body text.
pub fn parse_wiki_links(body: &str) -> Vec<String> {
    let Ok(re) = Regex::new(r"\[\[wiki:([A-Za-z0-9_\-./]+)\]\]") else {
        return Vec::new();
    };
    re.captures_iter(body)
        .filter_map(|c| {
            let t = &c[1];
            (!t.contains("..")).then(|| t.to_string())
        })
        .collect()
}

#[cfg(test)]
mod link_tests {
    use super::parse_wiki_links;
    #[test]
    fn extracts_clean_links() {
        assert_eq!(
            parse_wiki_links("a [[wiki:x/y-1]] b [[wiki:index]] c [[wiki:bad..x]]"),
            vec!["x/y-1", "index"]
        );
    }
}
