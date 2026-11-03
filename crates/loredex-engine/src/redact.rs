//! Secret sniffing for cited evidence: the trust model pins wiki claims to
//! source line ranges, but a citation is also a public pointer into wherever
//! secrets live. Snippet plaintext is never persisted (frontmatter stores
//! hashes only), so the gate runs at write time on the cited range, and lint
//! re-scans existing pages' ranges against HEAD.
//!
//! Two layers: per-line shape rules (`scan_lines`), and file context
//! (`scan_range`) for secrets that span lines — a citation landing inside the
//! base64 body of a PEM private key is flagged even when it leaves out the
//! `-----BEGIN` header line that the per-line rule keys on.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::{Captures, Regex};

/// Post-match filter: `true` means the match is a real finding. Used where a
/// shape is common in documentation with placeholder values.
type Guard = fn(&Captures) -> bool;

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let spec: Vec<(&str, String, Option<Guard>)> = vec![
        ("private-key", PEM_BEGIN_PAT.to_string(), None),
        // the workspace regex build has no unicode-case, so `(?i)` is out:
        // ASCII word cases are enumerated instead
        (
            "aws-access-key",
            format!(
                "(?:{alt})[0-9A-Z]{{16}}",
                alt = alt_words(&["akia", "asia", "abia", "acca"])
            ),
            None,
        ),
        (
            "github-token",
            format!(
                "(?:{alt})[0-9A-Za-z_]{{30,}}",
                alt = alt_words(&["ghp_", "ghu_", "gho_", "ghr_", "ghs_", "ghc_"])
            ),
            None,
        ),
        (
            "github-fine-grained-token",
            r"\bgithub_pat_[0-9A-Za-z_]{22,}".to_string(),
            None,
        ),
        (
            "gitlab-token",
            format!("(?:{alt})[0-9A-Za-z_-]{{30}}", alt = alt_words(&["glp-"])),
            None,
        ),
        (
            "slack-token",
            r"xox[baprs]-[0-9A-Za-z-]{10,}".to_string(),
            None,
        ),
        (
            "slack-webhook",
            r"hooks\.slack\.com/services/T[0-9A-Za-z_]+/B[0-9A-Za-z_]+/[0-9A-Za-z_]{16,}"
                .to_string(),
            None,
        ),
        (
            "google-api-key",
            r"AIza[0-9A-Za-z_\-]{35}".to_string(),
            None,
        ),
        // before the OpenAI rules: `sk-ant-…` would otherwise be named after
        // whichever `sk-` rule happens to match first
        (
            "anthropic-key",
            r"\bsk-ant-[0-9A-Za-z_\-]{20,}".to_string(),
            None,
        ),
        // current OpenAI keys carry a hyphenated kind prefix that the legacy
        // `sk-<alnum>` shape below cannot cross
        (
            "openai-key",
            r"\bsk-(?:proj|svcacct|admin)-[0-9A-Za-z_\-]{20,}".to_string(),
            None,
        ),
        // `\b` keeps identifiers such as `task-…` / `disk-…` from matching
        ("openai-key", r"\bsk-[0-9A-Za-z_]{40,}".to_string(), None),
        (
            "stripe-key",
            r"\b[rs]k_(?:live|test|prod)_[0-9A-Za-z]{16,}".to_string(),
            None,
        ),
        (
            "huggingface-token",
            r"\bhf_[A-Za-z]{34}\b".to_string(),
            None,
        ),
        ("npm-token", r"\bnpm_[0-9A-Za-z]{36}\b".to_string(), None),
        (
            "pypi-token",
            r"pypi-AgEIcHlwaS5vcmc[0-9A-Za-z_\-]{50,}".to_string(),
            None,
        ),
        (
            "jwt",
            r"eyJ[0-9A-Za-z_]{8,}\.[0-9A-Za-z_]{8,}\.[0-9A-Za-z_\-]{8,}".to_string(),
            None,
        ),
        (
            "url-credentials",
            r#"\b[A-Za-z][A-Za-z0-9+.\-]*://([^\s:/?#@'"]+):([^\s/?#@'"]+)@"#.to_string(),
            Some(real_url_password),
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
            None,
        ),
        (
            "bearer-token",
            format!(
                "(?:{alt})[ \\t]+[0-9A-Za-z+/=_-]{{16,}}",
                alt = alt_words(&["basic", "bearer"])
            ),
            None,
        ),
    ];
    spec.into_iter()
        .map(|(name, pat, guard)| Rule {
            name,
            re: Regex::new(&pat).expect("static secret rule"),
            guard,
        })
        .collect()
});

const PEM_BEGIN_PAT: &str = r"-----BEGIN (RSA |EC |DSA |OPENSSH |PGP |ENCRYPTED )?PRIVATE KEY";

static PEM_BEGIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(PEM_BEGIN_PAT).expect("static pem begin regex"));

static PEM_END: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-----END (RSA |EC |DSA |OPENSSH |PGP |ENCRYPTED )?PRIVATE KEY")
        .expect("static pem end regex")
});

/// `scheme://user:password@host` with a literal password, not a documented
/// placeholder or an interpolated variable.
fn real_url_password(caps: &Captures) -> bool {
    let Some(pw) = caps.get(2).map(|m| m.as_str()) else {
        return false;
    };
    if pw.starts_with(['$', '<', '{', '%', '*', '[']) {
        return false; // ${VAR}, <password>, {{pw}}, %(pw)s, ***, [redacted]
    }
    let first = pw.chars().next().unwrap_or('x');
    if pw.chars().all(|c| c == first) {
        return false; // xxxx, ****, ....
    }
    const PLACEHOLDERS: &[&str] = &[
        "pass",
        "passwd",
        "password",
        "pwd",
        "secret",
        "changeme",
        "example",
        "placeholder",
        "redacted",
        "dummy",
        "test",
        "user",
        "mypassword",
        "yourpassword",
        "your_password",
        "your-password",
    ];
    !PLACEHOLDERS.contains(&pw.to_ascii_lowercase().as_str())
}

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
    name: &'static str,
    re: Regex,
    guard: Option<Guard>,
}

impl Rule {
    fn hits(&self, line: &str) -> bool {
        match self.guard {
            None => self.re.is_match(line),
            Some(guard) => self.re.captures_iter(line).any(|c| guard(&c)),
        }
    }
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
        if let Some(r) = RULES.iter().find(|r| r.hits(line)) {
            // one rule per line is enough to reject
            out.push(Finding {
                rule: r.name.to_string(),
                line: i + 1,
            });
        }
    }
    out
}

/// Scan lines `lo..=hi` (1-based, inclusive) of a whole file: every per-line
/// rule of [`scan_lines`], plus multi-line context — each line of the range
/// that lies inside a PEM private-key block is flagged as
/// `private-key-block`, wherever the block's header sits. `Finding::line` is
/// relative to `lo`, sorted, at most one finding per line.
pub fn scan_range(content: &str, lo: u32, hi: u32) -> Vec<Finding> {
    let lines: Vec<&str> = content.split('\n').collect();
    let lo = lo.max(1) as usize;
    let hi = (hi as usize).min(lines.len());
    if hi < lo {
        return Vec::new();
    }
    let window: Vec<&str> = lines[lo - 1..hi]
        .iter()
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut out = scan_lines(&window);
    let flagged: BTreeSet<usize> = out.iter().map(|f| f.line).collect();
    for (start, end) in pem_blocks(&lines) {
        let (a, b) = (start.max(lo - 1), end.min(hi - 1));
        if a > b {
            continue; // block lies outside the cited range
        }
        for i in a..=b {
            let line = i - (lo - 1) + 1;
            if !flagged.contains(&line) {
                out.push(Finding {
                    rule: "private-key-block".into(),
                    line,
                });
            }
        }
    }
    out.sort_by_key(|f| f.line);
    out
}

/// 0-based inclusive line spans of PEM private-key blocks: from the BEGIN
/// line to the matching END line. A header without an END line extends only
/// over the base64-looking lines that follow it, so a stray header in a test
/// string cannot flag the rest of the file.
fn pem_blocks(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !PEM_BEGIN.is_match(lines[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i;
        if !PEM_END.is_match(lines[i]) {
            for (j, line) in lines.iter().enumerate().skip(i + 1) {
                if PEM_END.is_match(line) {
                    end = j;
                    break;
                }
                if !pem_body_like(line) {
                    break;
                }
                end = j;
            }
        }
        out.push((start, end));
        i = end + 1;
    }
    out
}

/// A PEM body line, possibly wrapped as a string literal (`"MIIE…\n" +`),
/// or an RFC 1421 header (`Proc-Type: …`) / blank separator.
fn pem_body_like(line: &str) -> bool {
    let t = line
        .trim()
        .trim_start_matches(['"', '\'', '`'])
        .trim_end_matches([',', ';', '+', ')', ' '])
        .trim_end_matches(['"', '\'', '`']);
    let t = t.strip_suffix("\\n").unwrap_or(t);
    if t.is_empty() {
        return true;
    }
    if let Some((key, _)) = t.split_once(':') {
        return matches!(key, "Proc-Type" | "DEK-Info" | "Comment");
    }
    t.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
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

    // Token fixtures are assembled at runtime so this source file never holds
    // a literal secret shape (a wiki page citing these lines stays clean).
    fn fake(prefix: &str, body: char, n: usize) -> String {
        format!("{prefix}{}", body.to_string().repeat(n))
    }

    fn rule_of(line: &str) -> Option<String> {
        scan_lines(&[line]).into_iter().next().map(|f| f.rule)
    }

    #[test]
    fn detects_current_vendor_token_shapes() {
        let cases = [
            (
                format!("k = \"{}\"", fake("sk-proj-", 'A', 48)),
                "openai-key",
            ),
            (fake("sk-svcacct-", 'b', 40), "openai-key"),
            (fake("sk-ant-api03-", 'c', 40), "anthropic-key"),
            (fake("github_pat_", 'D', 40), "github-fine-grained-token"),
            (fake(&format!("{}_live_", "sk"), 'e', 24), "stripe-key"),
            (fake("hf_", 'f', 34), "huggingface-token"),
            (fake("npm_", 'G', 36), "npm-token"),
            (fake("pypi-AgEIcHlwaS5vcmc", 'h', 60), "pypi-token"),
        ];
        for (line, want) in &cases {
            assert_eq!(rule_of(line).as_deref(), Some(*want), "{line}");
        }
    }

    #[test]
    fn word_boundary_keeps_identifiers_clean() {
        let line = format!("let {}{} = 1;", "task-", "a".repeat(44));
        assert_eq!(rule_of(&line), None, "{line}");
        let line = fake("disk_sk-", 'q', 44);
        assert_eq!(rule_of(&line), None, "{line}");
    }

    #[test]
    fn url_credentials_skip_placeholders() {
        let real = format!("DATABASE_URL=postgres://admin:{}@db:5432/app", "s3cr3tP4ss");
        assert_eq!(rule_of(&real).as_deref(), Some("url-credentials"));
        for doc in [
            "postgres://user:password@localhost/db",
            "https://bot:${GH_TOKEN}@github.com/o/r.git",
            "redis://default:<password>@cache:6379",
            "amqp://guest:****@mq",
            "see http://localhost:8080/path for the dashboard",
        ] {
            assert_eq!(rule_of(doc), None, "{doc}");
        }
    }

    fn pem_file() -> String {
        let begin = format!("-----BEGIN {}PRIVATE KEY-----", "RSA ");
        let end = format!("-----END {}PRIVATE KEY-----", "RSA ");
        let body = "MIIEowIBAAKCAQEAu1SU1LfVLPHCozMxH2Mo4lgOEePzNm0tRgeLezV6ffAt0gun";
        format!("fn main() {{}}\n{begin}\n{body}\n{body}\n{end}\nfn tail() {{}}\n")
    }

    #[test]
    fn range_inside_pem_body_is_flagged_without_its_header() {
        let file = pem_file();
        // lines 3-4 are pure base64: no per-line rule matches them
        let f = scan_range(&file, 3, 4);
        assert_eq!(f.len(), 2, "{f:?}");
        assert!(f.iter().all(|x| x.rule == "private-key-block"));
        assert_eq!(f[0].line, 1);
        // the END line and the code after it
        let f = scan_range(&file, 5, 6);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].line, 1);
        // header line keeps its specific rule name, no duplicate per line
        let f = scan_range(&file, 1, 3);
        let rules: Vec<_> = f.iter().map(|x| (x.line, x.rule.as_str())).collect();
        assert_eq!(rules, [(2, "private-key"), (3, "private-key-block")]);
        // code outside the block stays clean
        assert!(scan_range(&file, 1, 1).is_empty());
        assert!(scan_range(&file, 6, 99).is_empty());
    }

    #[test]
    fn unterminated_pem_header_does_not_swallow_the_file() {
        let header = format!("-----BEGIN {}PRIVATE KEY-----", "");
        let file = format!("const H: &str = \"{header}\";\nfn serve() {{\n    loop {{}}\n}}\n");
        let f = scan_range(&file, 2, 4);
        assert!(f.is_empty(), "{f:?}");
    }
}
