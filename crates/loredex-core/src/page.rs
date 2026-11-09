use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Result, WikiError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Draft,
    Verified,
    Stale,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Draft => "draft",
            Status::Verified => "verified",
            Status::Stale => "stale",
        }
    }
}

/// A verifiable citation: a line range in a file, pinned to the git blob
/// that existed when the evidence was recorded, plus a content hash of the
/// snippet itself so verification does not rely on the commit id alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<[u32; 2]>,
    pub blob: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet_sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageMeta {
    pub id: String,
    pub title: String,
    pub status: Status,
    /// ISO-8601 timestamp when the page (or its evidence) was last recorded.
    pub generated_at: String,
    /// Full commit sha the page was generated against.
    pub commit: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// openwiki-style page kind (architecture | workflow | system | integration | testing | ...)
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub page_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiPage {
    pub meta: PageMeta,
    /// Shared so long-lived caches and the search index can reference the same
    /// body without copying it.
    pub body: Arc<str>,
    pub source_path: Option<PathBuf>,
}

const DELIM: &str = "---";

impl WikiPage {
    pub fn from_markdown(text: &str, source: Option<&Path>) -> Result<Self> {
        let trimmed = text.trim_start_matches('\u{feff}');
        if !trimmed.starts_with(DELIM) {
            return Err(WikiError::MissingFrontmatter(
                source.map(|p| p.display().to_string()).unwrap_or_default(),
            ));
        }
        let rest = &trimmed[DELIM.len()..];
        let end = rest
            .find("\n---")
            .ok_or_else(|| WikiError::MissingFrontmatter(String::from("<memory>")))?;
        let yaml = &rest[..end];
        let body_start = rest[end + 4..]
            .strip_prefix('\n')
            .unwrap_or(&rest[end + 4..]);
        let meta: PageMeta = serde_yaml::from_str(yaml).map_err(|e| {
            WikiError::FrontmatterParse(
                source.map(|p| p.display().to_string()).unwrap_or_default(),
                e.to_string(),
            )
        })?;
        Ok(WikiPage {
            meta,
            body: Arc::from(body_start),
            source_path: source.map(|p| p.to_path_buf()),
        })
    }

    pub fn to_markdown(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(&self.meta)?;
        Ok(format!(
            "{}\n{}\n{}\n{}",
            DELIM,
            yaml.trim_end(),
            DELIM,
            self.body
        ))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_markdown(&text, Some(path))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let md = self.to_markdown()?;
        // Atomic write: temp file + rename. The temp name must be unique per
        // process (and per call) or two concurrent writers would share one
        // staging file and interleave their bytes.
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = path.with_file_name(format!(
            ".{}.{}.{}.tmp",
            path.file_name().unwrap().to_string_lossy(),
            std::process::id(),
            nonce
        ));
        std::fs::write(&tmp, md)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn save_in_place(&self) -> Result<()> {
        let path = self
            .source_path
            .clone()
            .ok_or_else(|| WikiError::Io(std::io::Error::other("page has no source path")))?;
        self.save(&path)
    }
}

/// List page files in a wiki pages directory, recursively (hierarchical ids).
pub fn list_page_files(pages_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if pages_dir.is_dir() {
        walk(pages_dir, &mut out);
    }
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            out.push(path);
        }
    }
}

/// Validate a hierarchical page id: alphanumeric plus '-', '_', '/';
/// no traversal, no leading/trailing slash, no empty segments.
pub fn validate_page_id(id: &str) -> std::result::Result<(), String> {
    if id.is_empty() {
        return Err("empty id".into());
    }
    for seg in id.split('/') {
        if seg.is_empty() {
            return Err(format!("empty path segment in `{id}`"));
        }
        if seg == "." || seg == ".." {
            return Err(format!("traversal segment `{seg}` in `{id}`"));
        }
        if !seg
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!("invalid characters in `{seg}`"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> WikiPage {
        WikiPage {
            meta: PageMeta {
                id: "storage".into(),
                title: "Storage layer".into(),
                status: Status::Verified,
                generated_at: "2026-10-31T00:00:00Z".into(),
                commit: "abc123".into(),
                evidence: vec![Evidence {
                    path: "src/db.rs".into(),
                    symbol: None,
                    lines: Some([10, 88]),
                    blob: "deadbeef".into(),
                    snippet_sha1: Some("cafe".into()),
                    note: None,
                }],
                tags: vec!["core".into()],
                page_type: Some("architecture".into()),
            },
            body: "# overview\n\ntext".into(),
            source_path: None,
        }
    }

    #[test]
    fn roundtrip() {
        let page = sample();
        let md = page.to_markdown().unwrap();
        let parsed = WikiPage::from_markdown(&md, None).unwrap();
        assert_eq!(parsed.meta, page.meta);
        assert_eq!(parsed.body, page.body);
    }
}
