//! Managed block in AGENTS.md (the "let agents discover the wiki" hook).
//!
//! Convention shared by agent-doc tools (ai-memory, openwiki): a pair of
//! HTML-comment markers with the tool name; the region between them belongs
//! to loredex, everything outside is the user's. Updates replace the whole
//! block idempotently; removal deletes only our own block.
//!
//! Fallback when the file exists but carries no markers: we append at the
//! end (never rewrite user prose).

use std::path::Path;

use loredex_core::WikiError;

use crate::config::Locale;
use crate::Result;

pub const BEGIN: &str = "<!-- BEGIN loredex -->";
pub const END: &str = "<!-- END loredex -->";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertAction {
    /// AGENTS.md did not exist; created with the block
    Created,
    /// block present but drifted; replaced in place
    Updated,
    /// file present without a block; block appended at the end
    Appended,
    /// block already matches the desired content
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentsMdStatus {
    InPlace,
    Drifted,
    /// AGENTS.md exists but has no loredex markers
    Missing,
    /// AGENTS.md does not exist
    FileMissing,
}

fn block_body(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "\
## Repository wiki (loredex)

This repo keeps an evidence-backed wiki under `.wiki/` (pages in `.wiki/pages/*.md`; every claim carries a `[[cite:path:Lx-Ly]]` source marker pinned to git content hashes).

Agent protocol:

1. Look before reading code: `loredex search \"<query>\"` / `loredex show <page>` (or MCP `wiki_search` / `wiki_get_page`).
2. Verify freshness before trusting a page: `loredex check` (or MCP `wiki_check_freshness`); STALE pages are unverified — read the source.
3. After changing code: `loredex auto` heals drifted citations, `loredex plan` lists remaining authoring work.
4. When writing pages, one inline `[[cite:path:Lx-Ly]]` per claim; bodies without citations are rejected.

Do not hand-edit this block; regenerate with `loredex agents-md` (remove it with `loredex agents-md --remove`).
",
        Locale::Zh => "\
## 仓库 Wiki(loredex)

本仓库在 `.wiki/` 下维护证据可回溯的 wiki(页面在 `.wiki/pages/*.md`;每条论断都带 `[[cite:path:Lx-Ly]]` 源码标记,并 pin 到 git 内容哈希)。

Agent 协议:

1. 先查 wiki 再读代码:`loredex search \"<关键词>\"` / `loredex show <页面>`(或 MCP `wiki_search` / `wiki_get_page`)。
2. 信任页面前先验证新鲜度:`loredex check`(或 MCP `wiki_check_freshness`);STALE 页面视为未验证,去读源码。
3. 修改代码后:`loredex auto` 自愈漂移引用,`loredex plan` 列出剩余写作任务。
4. 写页面时每条论断配一个行内 `[[cite:path:Lx-Ly]]`;无引用的正文会被拒绝。

请勿手改本区块;用 `loredex agents-md` 重新生成(`loredex agents-md --remove` 移除)。
",
    }
}

/// The full desired block, markers included, ends with a newline.
pub fn desired_block(locale: Locale) -> String {
    format!("{BEGIN}\n{}{END}\n", block_body(locale))
}

/// Locate the managed region: (start offset of BEGIN, end offset after END line).
fn find_block(text: &str) -> Option<(usize, usize)> {
    let start = text.find(BEGIN)?;
    let rest = &text[start..];
    let end_rel = rest.find(END)?;
    let after_end = start + end_rel + END.len();
    // include the trailing newline of the END line
    let end = if text[after_end..].starts_with('\n') {
        after_end + 1
    } else {
        after_end
    };
    Some((start, end))
}

/// Current state of the managed block against the desired content.
pub fn agents_block_check(path: &Path, locale: Locale) -> Result<AgentsMdStatus> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AgentsMdStatus::FileMissing)
        }
        Err(e) => return Err(WikiError::Io(e)),
    };
    match find_block(&text) {
        None => Ok(AgentsMdStatus::Missing),
        Some((s, e)) => Ok(if text[s..e] == desired_block(locale) {
            AgentsMdStatus::InPlace
        } else {
            AgentsMdStatus::Drifted
        }),
    }
}

/// Create-or-replace the managed block; user content outside markers is
/// preserved byte-for-byte.
pub fn agents_block_upsert(path: &Path, locale: Locale) -> Result<UpsertAction> {
    let block = desired_block(locale);
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::write(path, format!("# AGENTS.md\n\n{block}")).map_err(WikiError::Io)?;
            return Ok(UpsertAction::Created);
        }
        Err(e) => return Err(WikiError::Io(e)),
    };
    if let Some((s, e)) = find_block(&text) {
        if text[s..e] == *block {
            return Ok(UpsertAction::Unchanged);
        }
        let mut out = String::new();
        out.push_str(&text[..s]);
        out.push_str(&block);
        out.push_str(&text[e..]);
        std::fs::write(path, out).map_err(WikiError::Io)?;
        return Ok(UpsertAction::Updated);
    }
    // no markers anywhere: append, never rewrite existing prose
    let mut out = text;
    if !out.ends_with("\n\n") {
        if out.ends_with('\n') {
            out.push('\n');
        } else {
            out.push_str("\n\n");
        }
    }
    out.push_str(&block);
    std::fs::write(path, out).map_err(WikiError::Io)?;
    Ok(UpsertAction::Appended)
}

/// Remove the managed block (and the whole file if nothing else remains).
pub fn agents_block_remove(path: &Path) -> Result<bool> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(WikiError::Io(e)),
    };
    let Some((s, e)) = find_block(&text) else {
        return Ok(false);
    };
    let mut out = String::new();
    out.push_str(&text[..s]);
    let tail = text[e..].trim_start_matches('\n');
    let head = out.trim_end();
    out = head.to_string();
    if !tail.is_empty() {
        out.push_str("\n\n");
        out.push_str(tail);
    }
    if out.trim().is_empty() {
        std::fs::remove_file(path).map_err(WikiError::Io)?;
    } else {
        out.push('\n');
        std::fs::write(path, out).map_err(WikiError::Io)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("loredex-agents-md-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn create_check_update_remove() {
        let p = tmp_file("AGENTS.md.t1");
        let _ = std::fs::remove_file(&p);
        assert_eq!(
            agents_block_check(&p, Locale::En).unwrap(),
            AgentsMdStatus::FileMissing
        );
        assert_eq!(
            agents_block_upsert(&p, Locale::En).unwrap(),
            UpsertAction::Created
        );
        assert_eq!(
            agents_block_check(&p, Locale::En).unwrap(),
            AgentsMdStatus::InPlace
        );
        // idempotent
        assert_eq!(
            agents_block_upsert(&p, Locale::En).unwrap(),
            UpsertAction::Unchanged
        );
        // locale switch replaces the block in place
        assert_eq!(
            agents_block_upsert(&p, Locale::Zh).unwrap(),
            UpsertAction::Updated
        );
        assert_eq!(
            agents_block_check(&p, Locale::Zh).unwrap(),
            AgentsMdStatus::InPlace
        );
        assert!(agents_block_remove(&p).unwrap());
        // created files keep the non-managed "# AGENTS.md" header
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "# AGENTS.md\n");
        // a block-only file is removed entirely
        std::fs::write(&p, desired_block(Locale::En)).unwrap();
        assert!(agents_block_remove(&p).unwrap());
        assert!(!p.exists());
    }

    #[test]
    fn append_preserves_user_content() {
        let p = tmp_file("AGENTS.md.t2");
        std::fs::write(&p, "# My rules\n\nBe kind.\n").unwrap();
        assert_eq!(
            agents_block_check(&p, Locale::En).unwrap(),
            AgentsMdStatus::Missing
        );
        assert_eq!(
            agents_block_upsert(&p, Locale::En).unwrap(),
            UpsertAction::Appended
        );
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.starts_with("# My rules\n\nBe kind.\n"));
        assert!(text.contains(BEGIN) && text.contains(END));
        assert!(agents_block_remove(&p).unwrap());
        let back = std::fs::read_to_string(&p).unwrap();
        assert_eq!(back, "# My rules\n\nBe kind.\n");
    }
}
