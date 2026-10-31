//! Repo-local wiki configuration: `.wiki/wiki.toml`.
//!
//! `[wiki] language` selects the locale for tool-generated skeleton text
//! (index/quickstart pages, playbook instructions, AGENTS.md block).
//! Page bodies authored by agents are never translated.
//! `[page_types.*]` extensions are parsed by `playbook::registry` from the
//! same file.

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

#[derive(Debug, Deserialize)]
struct WikiSection {
    #[serde(default)]
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FileConfig {
    #[serde(default)]
    wiki: Option<WikiSection>,
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

fn parse_locale(text: &str) -> Locale {
    match toml::from_str::<FileConfig>(text) {
        Ok(cfg) => cfg
            .wiki
            .and_then(|w| w.language)
            .map(|t| Locale::from_tag(&t))
            .unwrap_or(Locale::En),
        Err(_) => Locale::En,
    }
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
}
