//! Secret sniffing for cited evidence: the trust model pins wiki claims to
//! source line ranges, but a citation is also a public pointer into wherever
//! secrets live. Snippet plaintext is never persisted (frontmatter stores
//! hashes only), so the gate runs at write time on the cited range, and lint
//! re-scans existing pages' ranges against HEAD.

use std::sync::LazyLock;

use regex::Regex;

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let spec: &[(&str, String)] = &[
        (
            "private-key",
            r"-----BEGIN (RSA |EC |DSA |OPENSSH |PGP |ENCRYPTED )?PRIVATE KEY".to_string(),
        ),
        // the workspace regex build has no unicode-case, so `(?i)` is out:
        // ASCII word cases are enumerated instead
        (
            "aws-access-key",
            format!(
                "(?:{alt})[0-9A-Z]{{16}}",
                alt = alt_words(&["akia", "asia", "abia", "acca"])
            ),
        ),
        (
            "github-token",
            format!(
                "(?:{alt})[0-9A-Za-z_]{{30,}}",
                alt = alt_words(&["ghp_", "ghu_", "gho_", "ghr_", "ghs_", "ghc_"])
            ),
        ),
        (
            "gitlab-token",
            format!("(?:{alt})[0-9A-Za-z_-]{{30}}", alt = alt_words(&["glp-"])),
        ),
        ("slack-token", r"xox[baprs]-[0-9A-Za-z-]{10,}".to_string()),
        ("google-api-key", r"AIza[0-9A-Za-z_\-]{35}".to_string()),
        ("openai-key", r"sk-[0-9A-Za-z_]{40,}".to_string()),
        ("anthropic-key", r"sk-ant-[0-9A-Za-z_\-]{20,}".to_string()),
        (
            "jwt",
            r"eyJ[0-9A-Za-z_]{8,}\.[0-9A-Za-z_]{8,}\.[0-9A-Za-z_\-]{8,}".to_string(),
        ),
        (
            "secret-assignment",
            format!(
                "(?:{alt})[\\x22']?[\\s:=]+[\\x22']?[0-9A-Za-z_\\-./+]{{16,}}",
                alt = alt_words(&[
                    "api_key",
                    "api-key",
                    "apikey",
                    "secret",
                    "token",
                    "passwd",
                    "password",
                    "access_key",
                    "access-key",
                    "accesskey"
                ])
            ),
        ),
        (
            "bearer-token",
            format!(
                "(?:{alt})[ \\t]+[0-9A-Za-z+/=_-]{{16,}}",
                alt = alt_words(&["basic", "bearer"])
            ),
        ),
    ];
    spec.iter()
        .map(|(name, pat)| Rule {
            name: (*name).to_string(),
            re: Regex::new(pat).expect("static secret rule"),
        })
        .collect()
});

/// ASCII-case-insensitive literal as a character-class chain (`[aA][kK]…`),
/// so the alternation stays linear for the regex engine.
fn alt_words(words: &[&str]) -> String {
    words
        .iter()
        .map(|w| {
            w.chars()
                .map(|c| {
                    if c.is_ascii_alphabetic() {
                        format!("[{}{}]", c.to_ascii_lowercase(), c.to_ascii_uppercase())
                    } else {
                        regex::escape(&c.to_string())
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("|")
}

struct Rule {
    name: String,
    re: Regex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: String,
    /// 1-based offset within the scanned snippet
    pub line: usize,
}

pub fn scan_lines(lines: &[&str]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        for r in RULES.iter() {
            if r.re.is_match(line) {
                out.push(Finding {
                    rule: r.name.clone(),
                    line: i + 1,
                });
                break; // one rule per line is enough to reject
            }
        }
    }
    out
}

/// Non-secret identifier words are safe to echo; long mixed-class strings are
/// not. Used to mask preview lines before they reach agent/tool output.
static SUSPECT_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[0-9A-Za-z_\-./+]{16,}").expect("static mask regex"));

pub fn mask_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut last = 0;
    for m in SUSPECT_WORD.find_iter(line) {
        out.push_str(&line[last..m.start()]);
        let tok = m.as_str();
        let has_digit = tok.bytes().any(|b| b.is_ascii_digit());
        let has_alpha = tok.bytes().any(|b| b.is_ascii_alphabetic());
        if has_digit && has_alpha {
            out.push_str(&tok[..4]);
            out.push_str("…[masked]");
        } else {
            out.push_str(tok);
        }
        last = m.end();
    }
    out.push_str(&line[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_shapes() {
        let lines: Vec<&str> = vec![
            "aws_key = \"AKIAIOSFODNN7EXAMPLE\"",
            "github: ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "    bearer qwertyuiopasdfghjklzxcvbnm",
            "pub fn serve() {",
            "-----BEGIN PRIVATE KEY-----",
        ];
        let f = scan_lines(&lines);
        let rules: Vec<&str> = f.iter().map(|x| x.rule.as_str()).collect();
        assert!(rules.contains(&"aws-access-key"));
        assert!(rules.contains(&"github-token"));
        assert!(rules.contains(&"bearer-token"));
        assert!(rules.contains(&"private-key"));
        assert_eq!(
            f.iter().find(|x| x.rule == "aws-access-key").unwrap().line,
            1
        );
    }

    #[test]
    fn ignores_plain_code_and_short_assignments() {
        let lines: Vec<&str> = vec![
            "let port = 8080;",
            "const MAX_RETRIES: u32 = 3;",
            "# API-KEY rotation is documented in runbook.md",
            "fn handle_request(req: Request) -> Response {",
        ];
        let f = scan_lines(&lines);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn masks_only_secret_shaped_words() {
        let m = mask_line("key = \"a1b2c3d4e5f6g7h8i9j0\" // config");
        assert!(m.contains("a1b2…[masked]"), "{m}");
        assert!(m.contains("config"), "{m}");
        let m2 = mask_line("fn handle_request(req: Request) {}");
        assert_eq!(m2, "fn handle_request(req: Request) {}");
    }
}
