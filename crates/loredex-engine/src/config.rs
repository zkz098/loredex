//! Repo-local wiki configuration: `.wiki/wiki.toml` and `.wikiignore`.
//!
//! `[wiki] language` selects the locale for tool-generated skeleton text
//! (index/quickstart pages, playbook instructions, AGENTS.md block).
//! Page bodies authored by agents are never translated.
//! `[wiki] ignore_modules` (plus optional `.wikiignore` files) excludes
//! non-documentation directories/targets (fuzz corpora, regression seeds,
//! helper scripts) from module discovery and `loredex plan`.
//! `[page_types.*]` extensions are parsed by `playbook::registry` from the
//! same file.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Zh,
}

impl Locale {
    pub fn as_str(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Zh => "zh",
        }
    }

    fn from_tag(tag: &str) -> Locale {
        let t = tag.trim().to_lowercase();
        if t.starts_with("zh") {
            Locale::Zh
        } else {
            Locale::En
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct WikiSection {
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    ignore_modules: Vec<String>,
    #[serde(default)]
    ignore: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    wiki: Option<WikiSection>,
    #[serde(default)]
    ignore_modules: Vec<String>,
}

/// Compiled ignore rules from `.wiki/wiki.toml` (`ignore_modules`) and
/// `.wikiignore` (in `.wiki/.wikiignore` or `<workdir>/.wikiignore`).
#[derive(Debug, Clone, Default)]
pub struct IgnoreRules {
    patterns: Vec<String>,
}

impl IgnoreRules {
    /// Load ignore rules from `wiki_dir/wiki.toml`, `wiki_dir/.wikiignore`,
    /// and `workdir/.wikiignore`. Missing or malformed files are tolerated.
    pub fn load(wiki_dir: &Path, workdir: &Path) -> Self {
        let mut seen = BTreeSet::new();
        let mut patterns = Vec::new();
        let mut push = |raw: &str| {
            let clean = normalize_pattern(raw);
            if !clean.is_empty() && seen.insert(clean.clone()) {
                patterns.push(clean);
            }
        };

        if let Ok(text) = std::fs::read_to_string(wiki_dir.join("wiki.toml")) {
            for p in parse_ignore_modules(&text) {
                push(&p);
            }
        }
        for path in [wiki_dir.join(".wikiignore"), workdir.join(".wikiignore")] {
            if let Ok(text) = std::fs::read_to_string(path) {
                for p in parse_wikiignore(&text) {
                    push(&p);
                }
            }
        }
        Self { patterns }
    }

    pub fn from_patterns<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut seen = BTreeSet::new();
        let mut patterns = Vec::new();
        for raw in iter {
            let clean = normalize_pattern(raw.as_ref());
            if !clean.is_empty() && seen.insert(clean.clone()) {
                patterns.push(clean);
            }
        }
        Self { patterns }
    }

    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// True if a tracked repo-relative file path should be excluded from
    /// module discovery and coverage planning.
    pub fn is_ignored_file(&self, rel_path: &str) -> bool {
        let path = normalize_pattern(rel_path);
        if path.is_empty() {
            return false;
        }
        self.patterns.iter().any(|pat| matches_rule(pat, &path))
    }

    /// True if a discovered module name (e.g. `fuzz/corpus`, `pkg/target`) or
    /// its suggested page id (e.g. `fuzz-corpus`) should be excluded.
    pub fn is_ignored_module(&self, module_name: &str) -> bool {
        let name = normalize_pattern(module_name);
        if name.is_empty() {
            return false;
        }
        let page_id = crate::plan::suggest_page_id(&name);
        self.patterns.iter().any(|pat| {
            matches_rule(pat, &name)
                || pat == &page_id
                || crate::plan::suggest_page_id(pat) == page_id
        })
    }
}

fn normalize_pattern(raw: &str) -> String {
    let replaced = raw.trim().replace('\\', "/");
    let stripped = replaced.strip_prefix("./").unwrap_or(&replaced);
    stripped.trim_matches('/').to_string()
}

pub(crate) fn parse_ignore_modules(text: &str) -> Vec<String> {
    let Ok(cfg) = toml::from_str::<FileConfig>(text) else {
        return Vec::new();
    };
    let mut out = cfg.ignore_modules;
    if let Some(w) = cfg.wiki {
        out.extend(w.ignore_modules);
        out.extend(w.ignore);
        out.extend(w.exclude);
    }
    out.into_iter()
        .map(|s| normalize_pattern(&s))
        .filter(|s| !s.is_empty())
        .collect()
}

pub(crate) fn parse_wikiignore(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(normalize_pattern)
        .filter(|s| !s.is_empty())
        .collect()
}

fn matches_rule(pat: &str, candidate: &str) -> bool {
    if pat == candidate {
        return true;
    }
    // Directory prefix match: pattern `fuzz/corpus` matches `fuzz/corpus/a`
    if candidate
        .strip_prefix(pat)
        .is_some_and(|rest| rest.starts_with('/'))
    {
        return true;
    }
    // Single segment without `/` or `**/segment`: match any path component
    // (e.g. `proptest-regressions` matches `crates/foo/proptest-regressions/bar.txt`)
    let seg_pat = pat.strip_prefix("**/").unwrap_or(pat);
    let seg_pat = seg_pat.strip_suffix("/**").unwrap_or(seg_pat);
    if !seg_pat.contains('/') && !seg_pat.contains('*') && !seg_pat.contains('?') {
        if candidate.split('/').any(|seg| seg == seg_pat) {
            return true;
        }
    } else if pat.starts_with("**/")
        && !seg_pat.contains('*')
        && !seg_pat.contains('?')
        && (candidate == seg_pat
            || candidate.ends_with(&format!("/{seg_pat}"))
            || candidate.contains(&format!("/{seg_pat}/")))
    {
        return true;
    }
    if pat.contains('*') || pat.contains('?') {
        if glob_match(pat, candidate) {
            return true;
        }
        // A directory glob like `fuzz/*` also covers files inside matched dirs
        if let Some(dir_pat) = pat.strip_suffix("/**") {
            if glob_match(dir_pat, candidate) {
                return true;
            }
        }
    }
    false
}

fn glob_match(pat: &str, text: &str) -> bool {
    let p_segs: Vec<&str> = pat.split('/').collect();
    let t_segs: Vec<&str> = text.split('/').collect();
    glob_segments(&p_segs, &t_segs)
}

fn glob_segments(p: &[&str], t: &[&str]) -> bool {
    if p.is_empty() {
        return t.is_empty();
    }
    if p[0] == "**" {
        if glob_segments(&p[1..], t) {
            return true;
        }
        if !t.is_empty() && glob_segments(p, &t[1..]) {
            return true;
        }
        return false;
    }
    if t.is_empty() {
        return false;
    }
    if glob_segment(p[0].as_bytes(), t[0].as_bytes()) {
        if p.len() == 1 && !t[1..].is_empty() {
            // Prefix directory match when pattern matched a parent directory
            return true;
        }
        glob_segments(&p[1..], &t[1..])
    } else {
        false
    }
}

fn glob_segment(p: &[u8], t: &[u8]) -> bool {
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_pi, mut star_ti) = (None, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_pi = Some(pi);
            pi += 1;
            star_ti = ti;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Parse the `[wiki]` section, tolerating a missing or broken file:
/// config is decoration, never a hard dependency (same policy as the
/// playbook registry).
pub fn locale(wiki_dir: &Path) -> Locale {
    let text = match std::fs::read_to_string(wiki_dir.join("wiki.toml")) {
        Ok(t) => t,
        Err(_) => return Locale::En,
    };
    parse_locale(&text)
}

/// Return configured `ignore_modules` patterns from `.wiki/wiki.toml` and `.wikiignore`.
pub fn ignore_rules(wiki_dir: &Path, workdir: &Path) -> IgnoreRules {
    IgnoreRules::load(wiki_dir, workdir)
}

pub(crate) fn parse_locale(text: &str) -> Locale {
    match toml::from_str::<FileConfig>(text) {
        Ok(cfg) => cfg
            .wiki
            .and_then(|w| w.language)
            .map(|t| Locale::from_tag(&t))
            .unwrap_or(Locale::En),
        Err(_) => Locale::En,
    }
}

/// Append one or more patterns to `[wiki] ignore_modules` in `.wiki/wiki.toml`,
/// preserving existing settings and returning the updated list.
pub fn add_ignore_modules(wiki_dir: &Path, additions: &[String]) -> crate::Result<Vec<String>> {
    std::fs::create_dir_all(wiki_dir).map_err(loredex_core::WikiError::Io)?;
    let path = wiki_dir.join("wiki.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut current = parse_ignore_modules(&text);
    for a in additions {
        let clean = normalize_pattern(a);
        if !clean.is_empty() && !current.contains(&clean) {
            current.push(clean);
        }
    }
    let formatted_items: Vec<String> = current
        .iter()
        .map(|s| format!("\"{}\"", s.replace('\\', "/").replace('"', "\\\"")))
        .collect();
    let line = format!("ignore_modules = [{}]", formatted_items.join(", "));

    let mut out: Vec<String> = Vec::new();
    let mut in_wiki = false;
    let mut replaced = false;
    let mut skipping_multiline = false;
    for l in text.lines() {
        let t = l.trim();
        if skipping_multiline {
            if t.contains(']') {
                skipping_multiline = false;
            }
            continue;
        }
        if t == "[wiki]" {
            in_wiki = true;
        } else if t.starts_with('[') {
            in_wiki = false;
        }
        if in_wiki
            && !replaced
            && (t.starts_with("ignore_modules")
                || t.starts_with("ignore ")
                || t.starts_with("ignore="))
        {
            out.push(line.clone());
            replaced = true;
            if !t.contains(']') {
                skipping_multiline = true;
            }
            continue;
        }
        out.push(l.to_string());
    }
    if !replaced {
        if let Some(at) = out.iter().position(|l| l.trim() == "[wiki]") {
            out.insert(at + 1, line);
        } else {
            out.push(String::new());
            out.push("[wiki]".into());
            out.push(line);
        }
    }
    let mut content = out.join("\n");
    if !content.ends_with('\n') {
        content.push('\n');
    }
    std::fs::write(&path, content).map_err(loredex_core::WikiError::Io)?;
    Ok(current)
}

/// Set (or replace) `[wiki] language` in `.wiki/wiki.toml`, creating the
/// file if needed. All other content is left untouched.
pub fn set_language(wiki_dir: &Path, language: &str) -> crate::Result<()> {
    std::fs::create_dir_all(wiki_dir).map_err(loredex_core::WikiError::Io)?;
    let path = wiki_dir.join("wiki.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let line = format!("language = \"{language}\"");
    let mut out: Vec<String> = Vec::new();
    let mut in_wiki = false;
    let mut replaced = false;
    for l in text.lines() {
        let t = l.trim();
        if t == "[wiki]" {
            in_wiki = true;
        } else if t.starts_with('[') {
            in_wiki = false;
        }
        if in_wiki && !replaced && t.starts_with("language") {
            out.push(line.clone());
            replaced = true;
            continue;
        }
        out.push(l.to_string());
    }
    if !replaced {
        if let Some(at) = out.iter().position(|l| l.trim() == "[wiki]") {
            out.insert(at + 1, line);
        } else {
            out.push(String::new());
            out.push("[wiki]".into());
            out.push(line);
        }
    }
    let mut content = out.join("\n");
    if !content.ends_with('\n') {
        content.push('\n');
    }
    std::fs::write(&path, content).map_err(loredex_core::WikiError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_language() {
        assert_eq!(parse_locale("[wiki]\nlanguage = \"zh\"\n"), Locale::Zh);
        assert_eq!(parse_locale("[wiki]\nlanguage = \"zh-CN\"\n"), Locale::Zh);
        assert_eq!(parse_locale("[wiki]\nlanguage = \"en\"\n"), Locale::En);
        assert_eq!(parse_locale("[wiki]\npages = \"pages\"\n"), Locale::En);
        assert_eq!(parse_locale("not toml at all ["), Locale::En);
    }

    #[test]
    fn from_tag_variants() {
        assert_eq!(Locale::from_tag("ZH_CN"), Locale::Zh);
        assert_eq!(Locale::from_tag(" english "), Locale::En);
    }

    #[test]
    fn ignore_rules_match_prefixes_segments_globs_and_page_ids() {
        let rules = IgnoreRules::from_patterns([
            "fuzz/corpus",
            "website/scripts/",
            "**/proptest-regressions",
            "crates/*/fixtures/**",
        ]);
        assert!(rules.is_ignored_file("fuzz/corpus/seed1"));
        assert!(rules.is_ignored_module("fuzz/corpus"));
        assert!(rules.is_ignored_module("fuzz-corpus"));
        assert!(rules.is_ignored_file("website/scripts/build.mjs"));
        assert!(rules.is_ignored_file("crates/limedl-types/proptest-regressions/lib.txt"));
        assert!(rules.is_ignored_file("crates/foo/fixtures/sample.json"));
        assert!(!rules.is_ignored_file("crates/limedl-types/src/lib.rs"));
        assert!(!rules.is_ignored_module("crates/limedl-types"));
    }
}
