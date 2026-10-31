use std::collections::BTreeMap;

use similar::{ChangeTag, TextDiff};
use wiki_core::{Evidence, WikiPage};
use wiki_git::GitRepo;

use crate::{now_iso, record_evidence, sha1_hex, Result, WikiError};

/// Outcome of trying to keep an evidence citation alive across edits.
#[derive(Debug, Clone, PartialEq)]
pub enum DriftOutcome {
    /// Blob at HEAD still equals the recorded one.
    Unchanged,
    /// Lines were re-projected onto new content; snippet survived (score 1.0)
    /// or was accepted above the similarity threshold.
    Projected {
        lines: [u32; 2],
        score: f64,
    },
    /// The cited path is gone but its content was found elsewhere in HEAD.
    /// `exact` = same blob oid (plain `git mv`); otherwise mv + edit, adopted
    /// only after line projection.
    Renamed {
        new_path: String,
        exact: bool,
    },
    /// Could not find the cited content in the new revision.
    Lost,
    /// Old blob object is missing from the object db; fell back to raw re-pin.
    NoOldBlob,
}

/// Similarity needed to adopt a projected line range. Snippets are usually
/// several lines, so one edited line inside a short citation still passes.
const DRIFT_THRESHOLD: f64 = 0.6;

/// Pure projection: map a 1-based inclusive [lo, hi] line range from
/// `old_content` onto `new_content` via a line diff, then score how well
/// the projected snippet matches the old one.
pub fn project_range(old_content: &str, new_content: &str, lo: u32, hi: u32) -> ([u32; 2], f64) {
    let old_lines: Vec<&str> = old_content.split('\n').collect();
    let new_lines: Vec<&str> = new_content.split('\n').collect();
    if old_lines.is_empty() || new_lines.is_empty() {
        return ([lo, hi], 0.0);
    }
    let mut map = vec![0usize; old_lines.len()];
    let (mut oi, mut ni) = (0usize, 0usize);
    let diff = TextDiff::from_lines(old_content, new_content);
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if oi < old_lines.len() {
                    map[oi] = ni;
                }
                oi += 1;
                ni += 1;
            }
            ChangeTag::Delete => {
                // deleted old line: anchor it to the last surviving new line
                if oi < old_lines.len() {
                    map[oi] = ni.saturating_sub(1);
                }
                oi += 1;
            }
            ChangeTag::Insert => {
                ni += 1;
            }
        }
    }

    let lo = lo.max(1) as usize - 1;
    let hi = (hi as usize).saturating_sub(1).min(old_lines.len() - 1);
    let mut nlo = map[lo];
    let mut nhi = map[hi];
    if nhi < nlo {
        std::mem::swap(&mut nlo, &mut nhi);
    }
    // preserve width when both endpoints collapse onto one line
    if nlo == nhi && hi > lo {
        nhi = (nlo + (hi - lo)).min(new_lines.len() - 1);
    }
    let projected: Vec<&str> = new_lines[nlo..=nhi].to_vec();
    let old_snippet = old_lines[lo..=hi].join("\n");
    let new_snippet = projected.join("\n");
    let score = similarity(&old_snippet, &new_snippet);
    ([nlo as u32 + 1, nhi as u32 + 1], score)
}

/// Recover evidence whose recorded path vanished from HEAD.
/// Tier 1: content-addressed exact match (plain `git mv`).
/// Tier 2: git similarity rename + line projection (mv + edit).
fn recover_renamed(
    repo: &GitRepo,
    ev: &Evidence,
    renames: &BTreeMap<String, String>,
) -> Result<(DriftOutcome, Option<Evidence>)> {
    if let Some(found) = repo.path_of_blob_in_head(&ev.blob)? {
        let mut adopted = ev.clone();
        adopted.path = found.clone();
        adopted.note = Some(format!(
            "rename-detected {} -> {} (identical blob)",
            ev.path, found
        ));
        return Ok((DriftOutcome::Renamed { new_path: found, exact: true }, Some(adopted)));
    }
    if let Some(new_path) = renames.get(&ev.path) {
        let old_content = repo.blob_content(&ev.blob)?;
        let new_content = repo.head_file_content(new_path)?;
        if let (Some(o), Some(n)) = (old_content, new_content) {
            if let Some([lo, hi]) = ev.lines {
                let (range, score) = project_range(&o, &n, lo, hi);
                if score >= DRIFT_THRESHOLD {
                    let blob = repo
                        .head_blob_oid(new_path)?
                        .ok_or_else(|| WikiError::Git(format!("{new_path} vanished")))?;
                    let snippet = head_lines_string(repo, new_path, range[0], range[1])?;
                    let adopted = Evidence {
                        path: new_path.clone(),
                        lines: Some(range),
                        blob,
                        snippet_sha1: Some(sha1_hex(snippet.as_bytes())),
                        note: Some(format!(
                            "renamed {} -> {}, drift-projected L{lo}-L{hi} -> L{}-L{} (similarity {score:.2})",
                            ev.path, new_path, range[0], range[1]
                        )),
                    };
                    return Ok((
                        DriftOutcome::Renamed { new_path: new_path.clone(), exact: false },
                        Some(adopted),
                    ));
                }
            }
            // moved but the snippet itself is gone: re-pin at the new location so
            // the page stays editable; caller still sees Lost -> draft rewrite
            let repinned = record_evidence(repo, new_path, ev.lines)?;
            return Ok((DriftOutcome::Lost, Some(repinned)));
        }
    }
    Err(WikiError::Git(format!("{} missing at HEAD", ev.path)))
}

fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let diff = TextDiff::from_lines(a, b);
    let total = diff.iter_all_changes().filter(|c| c.tag() != ChangeTag::Insert).count();
    let equal = diff
        .iter_all_changes()
        .filter(|c| c.tag() == ChangeTag::Equal)
        .count();
    if total == 0 {
        0.0
    } else {
        equal as f64 / total as f64
    }
}

/// Attempt to re-point one evidence item at HEAD without re-pinning blindly.
/// `renames` maps old->new paths detected between the page's commit and HEAD.
pub fn project_evidence(
    repo: &GitRepo,
    ev: &Evidence,
    renames: &BTreeMap<String, String>,
) -> Result<(DriftOutcome, Option<Evidence>)> {
    let current = repo.head_blob_oid(&ev.path)?;
    let current = match current {
        Some(oid) => oid,
        None => return recover_renamed(repo, ev, renames),
    };
    if current == ev.blob {
        return Ok((DriftOutcome::Unchanged, None));
    }
    let [lo, hi] = match ev.lines {
        Some(r) => r,
        None => {
            // whole-file evidence: no line projection needed, just re-pin
            return Ok((DriftOutcome::Projected { lines: [0, 0], score: 1.0 }, None));
        }
    };
    let old_content = match repo.blob_content(&ev.blob)? {
        Some(c) => c,
        None => return Ok((DriftOutcome::NoOldBlob, None)),
    };
    let new_content = match repo.head_file_content(&ev.path)? {
        Some(c) => c,
        None => return Ok((DriftOutcome::Lost, None)),
    };
    let (new_range, score) = project_range(&old_content, &new_content, lo, hi);
    if score < DRIFT_THRESHOLD {
        return Ok((DriftOutcome::Lost, None));
    }
    let snippet = head_lines_string(repo, &ev.path, new_range[0], new_range[1])?;
    let adopted = Evidence {
        path: ev.path.clone(),
        lines: Some(new_range),
        blob: current.clone(),
        snippet_sha1: Some(sha1_hex(snippet.as_bytes())),
        note: Some(format!(
            "drift-projected L{lo}-L{hi}@{} -> L{}-L{} (similarity {score:.2})",
            &ev.blob[..8.min(ev.blob.len())],
            new_range[0],
            new_range[1]
        )),
    };
    Ok((DriftOutcome::Projected { lines: new_range, score }, Some(adopted)))
}

pub fn head_lines_string(repo: &GitRepo, path: &str, lo: u32, hi: u32) -> Result<String> {
    Ok(repo
        .head_lines(path, lo, hi)?
        .unwrap_or_default())
}

/// Refresh one page's evidence with drift projection, falling back to a
/// plain re-pin only when the snippet cannot be recovered.
pub fn refresh_page_with_drift(
    page: &mut WikiPage,
    repo: &GitRepo,
    wiki_dir: &std::path::Path,
) -> Result<Vec<(String, DriftOutcome)>> {
    let mut outcomes = Vec::new();
    let mut evidence = Vec::new();
    let renames: BTreeMap<String, String> = if repo.commit_exists(&page.meta.commit) {
        repo.renames_since(&page.meta.commit).into_iter().collect()
    } else {
        BTreeMap::new()
    };
    for ev in &page.meta.evidence {
        let (outcome, adopted) = project_evidence(repo, ev, &renames)?;
        outcomes.push((ev.path.clone(), outcome.clone()));
        match adopted {
            Some(new_ev) => evidence.push(new_ev),
            None => match &outcome {
                DriftOutcome::Unchanged => evidence.push(ev.clone()),
                DriftOutcome::Projected { lines, .. } if lines == &[0, 0] => {
                    // whole-file evidence re-pinned
                    evidence.push(record_evidence(repo, &ev.path, None)?);
                }
                _ => {
                    // Lost / NoOldBlob: re-pin at recorded range only if the file exists
                    evidence.push(record_evidence(repo, &ev.path, ev.lines)?);
                }
            },
        }
    }
    let any_lost = outcomes
        .iter()
        .any(|(_, o)| matches!(o, DriftOutcome::Lost | DriftOutcome::NoOldBlob));

    page.meta.evidence = evidence;
    page.meta.commit = repo.head_commit()?;
    page.meta.generated_at = now_iso();
    page.meta.status = if any_lost {
        wiki_core::Status::Draft
    } else {
        wiki_core::Status::Verified
    };
    page.save_in_place()?;
    // a successful refresh resolves a pending Refresh task
    if !any_lost {
        let _ = crate::plan::mark_task(wiki_dir, &page.meta.id, crate::plan::TaskStatus::Done);
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::project_range;

    #[test]
    fn shifts_range_after_insertion() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "x\ny\na\nb\nc\nd\ne\n";
        let (range, score) = project_range(old, new, 2, 4);
        assert_eq!(range, [4, 6]);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn tolerant_of_edits_inside_snippet() {
        let old = "fn a() {\n  keep1();\n  gone();\n  keep2();\n}\n";
        let new = "fn a() {\n  keep1();\n  replaced();\n  keep2();\n  extra();\n}\n";
        let (range, score) = project_range(old, new, 2, 4);
        assert!(score > 0.6, "score {score}");
        assert!(range[0] <= 2 && range[1] >= 4);
    }

    #[test]
    fn reports_loss_when_content_deleted() {
        let old = "a\nb\nc\n";
        let new = "x\ny\nz\n";
        let (_, score) = project_range(old, new, 1, 3);
        assert!(score < 0.6);
    }
}

#[cfg(test)]
mod research {
    use super::project_range;

    /// 调研#9 关键实验：重复代码块中引用第二副本，第二副本被删后，
    /// 纯锚点哈希（firstLine/lastLine/context）同样无法区分两个副本
    /// （上下文行也相同）——此时正确行为是诚实判 Lost，而非误adopt。
    #[test]
    fn duplicate_copies_fail_honestly() {
        let old = "A\nB\nC\nA\nB\nC\nD\n";
        let new = "A\nB\nC\nD\n";
        let (_, score) = project_range(old, new, 5, 6);
        assert!(score < 0.6, "must not silently adopt the look-alike copy: {score}");
    }

    /// 而区分度足够时（上下文不同），现有 diff 投影已经做对：
    #[test]
    fn distinct_context_projection_works() {
        let old = "x1\nx2\nB\nC\ny1\ny2\n";
        let new = "zz\nx1\nx2\nB\nC\ny1\n";
        let (range, score) = project_range(old, new, 3, 4);
        assert_eq!(range, [4, 5]);
        assert_eq!(score, 1.0);
    }
}
