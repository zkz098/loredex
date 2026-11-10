use std::collections::BTreeMap;
use std::rc::Rc;

use loredex_core::{Evidence, WikiPage};
use loredex_git::{GitRepo, GitView};
use similar::{ChangeTag, TextDiff};

use crate::{now_iso, record_evidence_with, sha1_hex, Result, WikiError};

/// Outcome of trying to keep an evidence citation alive across edits.
#[derive(Debug, Clone, PartialEq)]
pub enum DriftOutcome {
    /// Blob at HEAD still equals the recorded one.
    Unchanged,
    /// Lines were re-projected onto new content; snippet survived (score 1.0)
    /// or was accepted above the similarity threshold.
    Projected { lines: [u32; 2], score: f64 },
    /// The cited path is gone but its content was found elsewhere in HEAD.
    /// `exact` = same blob oid (plain `git mv`); otherwise mv + edit, adopted
    /// only after line projection.
    Renamed { new_path: String, exact: bool },
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
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    if old_lines.is_empty()
        || new_lines.is_empty()
        || lo == 0
        || hi < lo
        || (lo as usize) > old_lines.len()
    {
        return ([lo, hi], 0.0);
    }
    let mut map = vec![0usize; old_lines.len()];
    let (mut oi, mut ni) = (0usize, 0usize);
    let diff = TextDiff::from_lines(old_content, new_content);
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if oi < old_lines.len() {
                    map[oi] = ni.min(new_lines.len() - 1);
                }
                oi += 1;
                ni += 1;
            }
            ChangeTag::Delete => {
                // deleted old line: anchor it to the last surviving new line
                if oi < old_lines.len() {
                    map[oi] = ni.saturating_sub(1).min(new_lines.len() - 1);
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
    let mut nlo = map[lo].min(new_lines.len() - 1);
    let mut nhi = map[hi].min(new_lines.len() - 1);
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
    view: &GitView,
    ev: &Evidence,
    renames: &BTreeMap<String, String>,
) -> Result<(DriftOutcome, Option<Evidence>)> {
    if let Some(found) = view.path_of_blob_in_head(&ev.blob)? {
        let mut adopted = ev.clone();
        adopted.path = found.clone();
        adopted.note = Some(format!(
            "rename-detected {} -> {} (identical blob)",
            ev.path, found
        ));
        return Ok((
            DriftOutcome::Renamed {
                new_path: found,
                exact: true,
            },
            Some(adopted),
        ));
    }
    if let Some(new_path) = renames.get(&ev.path) {
        let old_content = view.blob_content(&ev.blob)?;
        let new_content = view.head_file_content(new_path)?;
        if let (Some(o), Some(n)) = (old_content, new_content) {
            if let Some([lo, hi]) = ev.lines {
                let (range, score) = match view.line_map(&ev.blob, new_path)? {
                    Some(map) => project_with_map(&map, &o, &n, lo, hi),
                    None => project_range(&o, &n, lo, hi),
                };
                if score >= DRIFT_THRESHOLD {
                    let blob = view
                        .head_blob_oid(new_path)?
                        .ok_or_else(|| WikiError::Git(format!("{new_path} vanished")))?;
                    let snippet = head_lines_string(view, new_path, range[0], range[1])?;
                    let adopted = Evidence {
                        path: new_path.clone(),
                        symbol: ev.symbol.clone(),
                        lines: Some(range),
                        blob,
                        snippet_sha1: Some(sha1_hex(snippet.as_bytes())),
                        note: Some(format!(
                            "renamed {} -> {}, drift-projected L{lo}-L{hi} -> L{}-L{} (similarity {score:.2})",
                            ev.path, new_path, range[0], range[1]
                        )),
                    };
                    return Ok((
                        DriftOutcome::Renamed {
                            new_path: new_path.clone(),
                            exact: false,
                        },
                        Some(adopted),
                    ));
                }
                if let Some((sym, ast_range, ast_score)) =
                    try_ast_relocate(new_path, ev.symbol.as_deref(), &o, &n, lo, hi)
                {
                    let blob = view
                        .head_blob_oid(new_path)?
                        .ok_or_else(|| WikiError::Git(format!("{new_path} vanished")))?;
                    let snippet = head_lines_string(view, new_path, ast_range[0], ast_range[1])?;
                    let adopted = Evidence {
                        path: new_path.clone(),
                        symbol: Some(sym.clone()),
                        lines: Some(ast_range),
                        blob,
                        snippet_sha1: Some(sha1_hex(snippet.as_bytes())),
                        note: Some(format!(
                            "renamed {} -> {}, ast-relocated #{sym} L{lo}-L{hi} -> L{}-L{} (similarity {ast_score:.2})",
                            ev.path, new_path, ast_range[0], ast_range[1]
                        )),
                    };
                    return Ok((
                        DriftOutcome::Renamed {
                            new_path: new_path.clone(),
                            exact: false,
                        },
                        Some(adopted),
                    ));
                }
            }
            // moved but the snippet itself is gone: re-pin at the new location so
            // the page stays editable; caller still sees Lost -> draft rewrite
            let mut repinned = record_evidence_with(view, new_path, ev.lines)?;
            repinned.note = Some(format!(
                "renamed {} -> {}; cited content lost, re-pinned for rewrite (draft)",
                ev.path, new_path
            ));
            return Ok((DriftOutcome::Lost, Some(repinned)));
        }
    }
    // The path is gone and no rename leads to it: an honest Lost, not an
    // error that would abort the whole page refresh.
    Ok((DriftOutcome::Lost, None))
}

/// Fast projection over a git xdiff line map: same range/score semantics as
/// `project_range`, but the whole-file diff was computed once per file pair.
fn project_with_map(
    map: &loredex_git::LineMap,
    old_content: &str,
    new_content: &str,
    lo: u32,
    hi: u32,
) -> ([u32; 2], f64) {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    if old_lines.is_empty()
        || new_lines.is_empty()
        || lo == 0
        || hi < lo
        || (lo as usize) > old_lines.len()
    {
        return ([lo, hi], 0.0);
    }
    let lo0 = (lo.max(1) as usize - 1).min(old_lines.len() - 1);
    let hi0 = (hi as usize).saturating_sub(1).min(old_lines.len() - 1);
    let (nlo, nhi) = map.project(lo0, hi0);
    let nlo = nlo.min(new_lines.len() - 1);
    let nhi = nhi.min(new_lines.len() - 1);
    let old_snippet = old_lines[lo0..=hi0].join("\n");
    let new_snippet = new_lines[nlo..=nhi].join("\n");
    let score = similarity(&old_snippet, &new_snippet);
    ([nlo as u32 + 1, nhi as u32 + 1], score)
}

fn similarity(a: &str, b: &str) -> f64 {
    if a.trim().is_empty() || b.trim().is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let diff = TextDiff::from_lines(a, b);
    let total = diff
        .iter_all_changes()
        .filter(|c| c.tag() != ChangeTag::Insert)
        .count();
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
    project_evidence_with(&repo.view(), ev, renames)
}

pub fn project_evidence_with(
    view: &GitView,
    ev: &Evidence,
    renames: &BTreeMap<String, String>,
) -> Result<(DriftOutcome, Option<Evidence>)> {
    let current = view.head_blob_oid(&ev.path)?;
    let current = match current {
        Some(oid) => oid,
        None => return recover_renamed(view, ev, renames),
    };
    if current == ev.blob {
        return Ok((DriftOutcome::Unchanged, None));
    }
    let [lo, hi] = match ev.lines {
        Some(r) => r,
        None => {
            // whole-file evidence: no line projection needed, just re-pin
            return Ok((
                DriftOutcome::Projected {
                    lines: [0, 0],
                    score: 1.0,
                },
                None,
            ));
        }
    };
    let old_content = match view.blob_content(&ev.blob)? {
        Some(c) => c,
        None => return Ok((DriftOutcome::NoOldBlob, None)),
    };
    let new_content = match view.head_file_content(&ev.path)? {
        Some(c) => c,
        None => return Ok((DriftOutcome::Lost, None)),
    };
    let (new_range, score) = match view.line_map(&ev.blob, &ev.path)? {
        Some(map) => project_with_map(&map, &old_content, &new_content, lo, hi),
        None => project_range(&old_content, &new_content, lo, hi),
    };
    if score < DRIFT_THRESHOLD {
        if let Some((sym, ast_range, ast_score)) = try_ast_relocate(
            &ev.path,
            ev.symbol.as_deref(),
            &old_content,
            &new_content,
            lo,
            hi,
        ) {
            let snippet = head_lines_string(view, &ev.path, ast_range[0], ast_range[1])?;
            let adopted = Evidence {
                path: ev.path.clone(),
                symbol: Some(sym.clone()),
                lines: Some(ast_range),
                blob: current.clone(),
                snippet_sha1: Some(sha1_hex(snippet.as_bytes())),
                note: Some(format!(
                    "ast-relocated #{sym} L{lo}-L{hi}@{} -> L{}-L{} (similarity {ast_score:.2})",
                    &ev.blob[..8.min(ev.blob.len())],
                    ast_range[0],
                    ast_range[1]
                )),
            };
            return Ok((
                DriftOutcome::Projected {
                    lines: ast_range,
                    score: ast_score,
                },
                Some(adopted),
            ));
        }
        return Ok((DriftOutcome::Lost, None));
    }
    let snippet = head_lines_string(view, &ev.path, new_range[0], new_range[1])?;
    let adopted = Evidence {
        path: ev.path.clone(),
        symbol: ev.symbol.clone(),
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
    Ok((
        DriftOutcome::Projected {
            lines: new_range,
            score,
        },
        Some(adopted),
    ))
}

/// Fallback when monotonic line-diff projection fails on a `.rs` file (e.g.
/// functions or `impl` blocks were reordered within the file): use
/// `tree-sitter-rust` to locate the cited symbol in `new_content` and adopt its
/// new span if snippet similarity still passes [`DRIFT_THRESHOLD`].
fn try_ast_relocate(
    path: &str,
    explicit_symbol: Option<&str>,
    old_content: &str,
    new_content: &str,
    lo: u32,
    hi: u32,
) -> Option<(String, [u32; 2], f64)> {
    if !crate::ast::is_rust_path(path) {
        return None;
    }
    let sym_name = match explicit_symbol {
        Some(s) => s.to_string(),
        None => crate::ast::symbol_matching_range(path, old_content, lo, hi)?.qualified_name,
    };
    let found = crate::ast::resolve_symbol(path, new_content, &sym_name)?;
    let old_lines: Vec<&str> = old_content.split('\n').collect();
    let new_lines: Vec<&str> = new_content.split('\n').collect();
    if old_lines.is_empty() || new_lines.is_empty() {
        return None;
    }
    let lo0 = (lo.max(1) as usize - 1).min(old_lines.len() - 1);
    let hi0 = (hi as usize).saturating_sub(1).min(old_lines.len() - 1);
    let nlo0 = (found.lines[0].max(1) as usize - 1).min(new_lines.len() - 1);
    let nhi0 = (found.lines[1] as usize)
        .saturating_sub(1)
        .min(new_lines.len() - 1);
    let old_snippet = old_lines[lo0..=hi0].join("\n");
    let new_snippet = new_lines[nlo0..=nhi0].join("\n");
    let score = similarity(&old_snippet, &new_snippet);
    if score >= DRIFT_THRESHOLD {
        Some((found.qualified_name, found.lines, score))
    } else {
        None
    }
}

fn head_lines_string(view: &GitView, path: &str, lo: u32, hi: u32) -> Result<String> {
    Ok(view.head_lines(path, lo, hi)?.unwrap_or_default())
}

/// Refresh one page's evidence with drift projection. Unrecoverable
/// citations keep their last-known pin and downgrade the page to draft; one
/// bad citation never aborts the rest of the page.
pub fn refresh_page_with_drift(
    page: &mut WikiPage,
    repo: &GitRepo,
    wiki_dir: &std::path::Path,
) -> Result<Vec<(String, DriftOutcome)>> {
    refresh_page_with_drift_view(page, &repo.view(), wiki_dir)
}

fn kept_with_note(ev: &Evidence, note: String) -> Evidence {
    let mut kept = ev.clone();
    kept.note = Some(note);
    kept
}

/// Lost content at a still-existing path can be re-pinned at the recorded
/// range to keep the page editable; a vanished path keeps its last-known pin.
fn lost_evidence(view: &GitView, ev: &Evidence) -> Evidence {
    match view.head_blob_oid(&ev.path) {
        Ok(Some(_)) => match record_evidence_with(view, &ev.path, ev.lines) {
            Ok(mut repinned) => {
                repinned.note =
                    Some("content lost at HEAD; re-pinned at recorded range (draft)".into());
                repinned
            }
            Err(err) => kept_with_note(
                ev,
                format!("refresh error: {err}; last-known pin kept (draft)"),
            ),
        },
        Ok(None) => kept_with_note(
            ev,
            "source vanished at HEAD; last-known pin kept (draft)".into(),
        ),
        Err(err) => kept_with_note(
            ev,
            format!("refresh error: {err}; last-known pin kept (draft)"),
        ),
    }
}

pub fn refresh_page_with_drift_view(
    page: &mut WikiPage,
    view: &GitView,
    wiki_dir: &std::path::Path,
) -> Result<Vec<(String, DriftOutcome)>> {
    refresh_page_with_drift_view_inner(page, view, wiki_dir, true, false)
}

/// Refresh a page's evidence with drift projection and optional inline body
/// line-range synchronization (`sync_lines`). Renamed citation paths are
/// always synchronized into `page.body` so body markers and frontmatter never
/// split on file moves.
pub fn refresh_page_with_drift_options(
    page: &mut WikiPage,
    view: &GitView,
    wiki_dir: &std::path::Path,
    mark_done: bool,
    sync_lines: bool,
) -> Result<Vec<(String, DriftOutcome)>> {
    refresh_page_with_drift_view_inner(page, view, wiki_dir, mark_done, sync_lines)
}

/// Batch variant for `auto`: the caller resolves every recovered page with a
/// single ledger write instead of one read+write per page.
pub(crate) fn refresh_page_with_drift_view_deferred(
    page: &mut WikiPage,
    view: &GitView,
    wiki_dir: &std::path::Path,
    sync_lines: bool,
) -> Result<Vec<(String, DriftOutcome)>> {
    refresh_page_with_drift_view_inner(page, view, wiki_dir, false, sync_lines)
}

/// Synchronize inline `[[cite:...]]` markers in `body` when evidence paths are
/// renamed (always) or line ranges shift (`sync_lines = true`).
pub fn sync_body_citations(
    body: &str,
    old_evidence: &[Evidence],
    new_evidence: &[Evidence],
    outcomes: &[(String, DriftOutcome)],
    sync_lines: bool,
) -> String {
    let Ok(cites) = loredex_core::parse_citations(body) else {
        return body.to_string();
    };
    if cites.is_empty() {
        return body.to_string();
    }
    let triples: Vec<(&Evidence, &Evidence, &DriftOutcome)> = old_evidence
        .iter()
        .zip(new_evidence.iter())
        .zip(outcomes.iter())
        .map(|((o, n), (_, out))| (o, n, out))
        .collect();

    let mut out = body.to_string();
    for c in cites.iter().rev() {
        let matched = triples
            .iter()
            .find(|(o, _, _)| o.path == c.path && o.symbol == c.symbol && o.lines == c.lines)
            .or_else(|| {
                triples
                    .iter()
                    .find(|(o, _, _)| o.path == c.path && o.lines == c.lines)
            })
            .or_else(|| {
                triples.iter().find(|(o, _, _)| {
                    o.path == c.path && o.symbol.is_some() && o.symbol == c.symbol
                })
            })
            .or_else(|| {
                triples
                    .iter()
                    .find(|(o, n, _)| o.path == c.path && o.path != n.path)
            });

        let Some((old_ev, new_ev, outcome)) = matched else {
            continue;
        };
        let target_path = if old_ev.path != new_ev.path {
            new_ev.path.as_str()
        } else {
            c.path.as_str()
        };
        let target_lines = if sync_lines
            && c.lines.is_some()
            && new_ev.lines.is_some()
            && matches!(
                outcome,
                DriftOutcome::Projected { .. } | DriftOutcome::Renamed { .. }
            ) {
            new_ev.lines
        } else {
            c.lines
        };
        if target_path == c.path && target_lines == c.lines {
            continue;
        }
        let mut marker = format!("[[cite:{target_path}");
        if let Some(sym) = &c.symbol {
            marker.push('#');
            marker.push_str(sym);
        }
        if let Some([lo, hi]) = target_lines {
            marker.push_str(&format!(":L{lo}-L{hi}"));
        }
        marker.push_str("]]");
        out.replace_range(c.span.0..c.span.1, &marker);
    }
    out
}

fn refresh_page_with_drift_view_inner(
    page: &mut WikiPage,
    view: &GitView,
    wiki_dir: &std::path::Path,
    mark_done: bool,
    sync_lines: bool,
) -> Result<Vec<(String, DriftOutcome)>> {
    // Remember what the file looked like when the caller loaded it: refresh is
    // a read-modify-write too, and another agent may have put a new body while
    // projection was running.
    let before = match page.source_path.as_deref() {
        Some(path) => crate::page_sha1(path)?,
        None => None,
    };
    let mut outcomes = Vec::new();
    let mut evidence = Vec::new();
    let renames: Rc<BTreeMap<String, String>> = if view.commit_exists(&page.meta.commit) {
        view.rename_map(&page.meta.commit)
    } else {
        Rc::new(BTreeMap::new())
    };
    for ev in &page.meta.evidence {
        let (outcome, new_ev) = match project_evidence_with(view, ev, &renames) {
            Ok((outcome, Some(adopted))) => (outcome, adopted),
            Ok((mut outcome, None)) => {
                let new_ev = match &outcome {
                    DriftOutcome::Unchanged => ev.clone(),
                    DriftOutcome::Projected { lines, .. } if lines == &[0, 0] => {
                        // whole-file evidence re-pinned
                        match record_evidence_with(view, &ev.path, None) {
                            Ok(new_ev) => new_ev,
                            Err(err) => {
                                outcome = DriftOutcome::Lost;
                                kept_with_note(
                                    ev,
                                    format!("refresh error: {err}; last-known pin kept (draft)"),
                                )
                            }
                        }
                    }
                    DriftOutcome::Lost => lost_evidence(view, ev),
                    DriftOutcome::NoOldBlob => {
                        match record_evidence_with(view, &ev.path, ev.lines) {
                            Ok(mut repinned) => {
                                repinned.note =
                                    Some("old blob unavailable; re-pinned at HEAD (draft)".into());
                                repinned
                            }
                            Err(err) => kept_with_note(
                                ev,
                                format!("refresh error: {err}; last-known pin kept (draft)"),
                            ),
                        }
                    }
                    _ => ev.clone(),
                };
                (outcome, new_ev)
            }
            Err(err) => (
                DriftOutcome::Lost,
                kept_with_note(
                    ev,
                    format!("refresh error: {err}; last-known pin kept (draft)"),
                ),
            ),
        };
        outcomes.push((ev.path.clone(), outcome));
        evidence.push(new_ev);
    }
    let any_lost = outcomes
        .iter()
        .any(|(_, o)| matches!(o, DriftOutcome::Lost | DriftOutcome::NoOldBlob));

    page.body = sync_body_citations(
        &page.body,
        &page.meta.evidence,
        &evidence,
        &outcomes,
        sync_lines,
    )
    .into();
    page.meta.evidence = evidence;
    page.meta.commit = view.head_commit()?.to_string();
    page.meta.generated_at = now_iso();
    page.meta.status = if any_lost {
        loredex_core::Status::Draft
    } else {
        loredex_core::Status::Verified
    };
    {
        let _lock = crate::lock::FileLock::page(wiki_dir, &page.meta.id)?;
        if let Some(path) = page.source_path.as_deref() {
            let current = crate::page_sha1(path)?;
            if current != before {
                return Err(WikiError::Conflict(format!(
                    "page `{}` changed while it was being refreshed; re-read and retry",
                    page.meta.id
                )));
            }
        }
        page.save_in_place()?;
    }
    // a successful refresh resolves a pending Refresh task
    if mark_done && !any_lost {
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

    #[test]
    fn out_of_bounds_or_empty_range_never_projects_to_trailing_eof_with_score_one() {
        let old = (1..=502)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let new = (1..=200)
            .map(|i| format!("replaced {i}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let (_, score_oob) = project_range(&old, &new, 503, 510);
        assert_eq!(
            score_oob, 0.0,
            "out-of-bounds range must not match trailing newline"
        );

        let old_blank = "fn a() {}\n\n\nfn b() {}\n";
        let new_blank = "fn c() {}\n\n\nfn d() {}\n";
        let (_, score_blank) = project_range(old_blank, new_blank, 2, 3);
        assert_eq!(
            score_blank, 0.0,
            "whitespace-only snippet must not score 1.0"
        );
    }
}

#[cfg(test)]
mod research {
    use super::project_range;

    /// Research experiment (#9 line anchors): when a cited copy of a
    /// duplicated block is deleted, anchor hashes (firstLine/lastLine/context)
    /// cannot distinguish the copies either (identical context) — the honest
    /// outcome is Lost, never a silent mis-adopt.
    #[test]
    fn duplicate_copies_fail_honestly() {
        let old = "A\nB\nC\nA\nB\nC\nD\n";
        let new = "A\nB\nC\nD\n";
        let (_, score) = project_range(old, new, 5, 6);
        assert!(
            score < 0.6,
            "must not silently adopt the look-alike copy: {score}"
        );
    }

    /// With distinguishable context, the existing diff projection already
    /// locates correctly:
    #[test]
    fn distinct_context_projection_works() {
        let old = "x1\nx2\nB\nC\ny1\ny2\n";
        let new = "zz\nx1\nx2\nB\nC\ny1\n";
        let (range, score) = project_range(old, new, 3, 4);
        assert_eq!(range, [4, 5]);
        assert_eq!(score, 1.0);
    }
}
