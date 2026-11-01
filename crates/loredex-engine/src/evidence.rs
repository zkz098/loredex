//! Citation draft generation: locating line ranges is the most expensive
//! part of authoring a page. This scans the HEAD content of a module's files
//! for keyword matches and returns mergeable line windows as ready-to-paste
//! `[[cite:path:Lx-Ly]]` markers, so an agent only has to pick windows and
//! write prose. Preview lines are masked (`redact::mask_line`) so the output
//! never carries secret-shaped strings; the write-time gate in `put` still
//! rejects citations that land on real secrets.

use std::sync::LazyLock;

use loredex_git::GitView;
use regex::Regex;
use serde::Serialize;

use crate::redact;
use crate::Result;

const MERGE_GAP: u32 = 6;
const MAX_WINDOW: u32 = 60;
const MAX_FILE_BYTES: usize = 512 * 1024;
const MAX_PREVIEW_LINES: usize = 3;

static NOISE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|/)(package[-_.]lock\.json|pnpm-lock\.yaml|yarn\.lock|Cargo\.lock|go\.sum|composer\.lock|Gemfile\.lock|poetry\.lock|uv\.lock)$")
        .expect("static noise regex")
});

#[derive(Debug, Clone, Serialize)]
pub struct Window {
    pub path: String,
    pub lines: [u32; 2],
    /// pasteable marker for the body
    pub cite: String,
    pub matched_lines: usize,
    pub masked_preview: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceCandidates {
    pub module: String,
    pub matched_files: usize,
    pub windows: Vec<Window>,
    pub hint: String,
}

/// Candidate citation windows for `module` (as named by `discover`, or a
/// directory prefix) filtered by case-insensitive keywords. Windows are
/// ranked by match density; `limit` caps the returned count.
pub fn candidates(
    view: &GitView,
    module: &str,
    keywords: &[String],
    limit: usize,
) -> Result<EvidenceCandidates> {
    let name = module.to_string();
    view.prime_head_index()?;
    let files: Vec<String> = view
        .tracked_files()?
        .into_iter()
        .filter(|f| in_module(f, &name) && !NOISE.is_match(f) && !f.starts_with(".wiki/"))
        .collect();

    let kws: Vec<String> = keywords
        .iter()
        .map(|k| k.to_lowercase())
        .filter(|k| !k.is_empty())
        .collect();
    let mut windows = Vec::new();
    let mut matched_files = 0;

    for path in &files {
        let Some(content) = view.head_file_content(path)? else {
            continue;
        };
        if content.len() > MAX_FILE_BYTES {
            continue;
        }
        let hits: Vec<u32> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| {
                let l = line.to_lowercase();
                kws.iter().any(|k| l.contains(k.as_str()))
            })
            .map(|(i, _)| i as u32 + 1)
            .collect();
        if hits.is_empty() {
            continue;
        }
        matched_files += 1;
        let ranges = merge_hits(&hits);
        let file_windows: Vec<Window> = ranges
            .into_iter()
            .map(|(lo, hi)| {
                let lines: Vec<&str> = content
                    .lines()
                    .take(hi as usize)
                    .skip(lo as usize - 1)
                    .collect();
                let matched = hits.iter().filter(|h| **h >= lo && **h <= hi).count();
                let preview = lines
                    .iter()
                    .take(MAX_PREVIEW_LINES)
                    .map(|l| redact::mask_line(l.trim()))
                    .collect();
                Window {
                    path: path.clone(),
                    lines: [lo, hi],
                    cite: format!("[[cite:{path}:L{lo}-L{hi}]]"),
                    matched_lines: matched,
                    masked_preview: preview,
                }
            })
            .collect();
        windows.extend(file_windows);
    }

    windows.sort_by(|a, b| {
        b.matched_lines
            .cmp(&a.matched_lines)
            .then(a.path.cmp(&b.path))
            .then(a.lines[0].cmp(&b.lines[0]))
    });
    windows.truncate(limit.max(1));

    Ok(EvidenceCandidates {
        module: name,
        matched_files,
        hint: if kws.is_empty() {
            "no keywords: every window matches trivially; pass keywords to rank real evidence"
                .into()
        } else {
            "paste `cite` markers into the body; verify claims against the source before putting the page".into()
        },
        windows,
    })
}

pub(crate) fn in_module(path: &str, module: &str) -> bool {
    if module == "." {
        return true;
    }
    // cargo target names ("loredex-engine") and directory groups are both
    // accepted: exact path, directory prefix, or a `/<name>/` path component.
    path == module
        || path.starts_with(&format!("{module}/"))
        || path.contains(&format!("/{module}/"))
}

/// Merge hit lines that are close together, cap window length, and snap
/// single-line hits to a small window so surrounding context is citable.
fn merge_hits(hits: &[u32]) -> Vec<(u32, u32)> {
    const CONTEXT: u32 = 2;
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &h in hits {
        let lo = h.saturating_sub(CONTEXT).max(1);
        let hi = h + CONTEXT;
        match out.last_mut() {
            Some(last) if lo <= last.1 + MERGE_GAP => last.1 = last.1.max(hi),
            _ => out.push((lo, hi)),
        }
    }
    for w in &mut out {
        if w.1 - w.0 + 1 > MAX_WINDOW {
            w.1 = w.0 + MAX_WINDOW - 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_nearby_and_caps_far_windows() {
        let w = merge_hits(&[5, 8, 30, 200]);
        assert_eq!(w[0], (3, 10)); // 5-2 .. 8+2 merged
        assert_eq!(w[1], (28, 32));
        assert!(w.last().unwrap().1 - w.last().unwrap().0 < MAX_WINDOW);
    }

    #[test]
    fn module_matching_accepts_paths_and_names() {
        assert!(in_module("crates/foo/src/lib.rs", "crates/foo"));
        assert!(in_module("src/lib.rs", "."));
        assert!(in_module("packages/foo/index.ts", "foo"));
        assert!(!in_module("src/other.rs", "foo"));
    }

    #[test]
    fn mask_applied_to_preview() {
        let masked = redact::mask_line("token = \"abc123def456ghi789jkl0\"");
        assert!(masked.contains("[masked]"));
    }
}
