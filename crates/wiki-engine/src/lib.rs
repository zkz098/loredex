use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};
use wiki_core::{list_page_files, Evidence, Status, WikiPage, WikiError};
use wiki_git::GitRepo;

pub mod auto;
pub mod drift;
pub mod index;
pub mod lint;
pub mod modules;
pub mod plan;
pub mod playbook;
pub mod put;

pub use auto::{auto_refresh, AutoReport};
pub use drift::{project_evidence, project_range, refresh_page_with_drift, DriftOutcome};
pub use index::{all_wiki_links, generate_index, is_generated, INDEX_ID, QUICKSTART_ID};
pub use lint::{lint, LintIssue};
pub use modules::{discover as discover_modules, ModuleInfo};
pub use playbook::{registry as playbook_registry, TypePlaybook};
pub use plan::{
    close_run, compute_tasks, load_ledger, mark_task, plan as open_plan, plan_with, PlanTask, RunLedger,
    RunRecord, TaskKind, TaskStatus,
};
pub use put::{put_page, PutOutcome};

pub type Result<T> = std::result::Result<T, WikiError>;

pub fn sha1_hex(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // ISO-8601 UTC without pulling in a datetime crate: days -> y/m/d via civil algorithm.
    let (y, mo, d, h, mi, s) = civil_from_days(secs as i64 / 86400, secs % 86400);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn civil_from_days(days: i64, secs_of_day: u64) -> (i64, u64, u64, u64, u64, u64) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        m,
        d,
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60,
    )
}

pub fn pages_dir(wiki_dir: &Path) -> PathBuf {
    wiki_dir.join("pages")
}

pub fn load_pages(wiki_dir: &Path) -> Result<Vec<WikiPage>> {
    let mut pages = Vec::new();
    for f in list_page_files(&pages_dir(wiki_dir))? {
        pages.push(WikiPage::load(&f)?);
    }
    Ok(pages)
}

pub fn find_page(wiki_dir: &Path, id: &str) -> Result<WikiPage> {
    wiki_core::validate_page_id(id).map_err(WikiError::Git)?;
    let path = pages_dir(wiki_dir).join(format!("{id}.md"));
    if path.is_file() {
        WikiPage::load(&path)
    } else {
        Err(WikiError::PageNotFound(id.to_string()))
    }
}

/// Record evidence pointers against the content at HEAD.
pub fn record_evidence(repo: &GitRepo, path: &str, lines: Option<[u32; 2]>) -> Result<Evidence> {
    let blob = repo
        .head_blob_oid(path)?
        .ok_or_else(|| WikiError::Git(format!("{path} is not tracked at HEAD; commit it first")))?;
    let snippet_sha1 = match lines {
        Some([lo, hi]) => repo
            .head_lines(path, lo, hi)?
            .map(|s| sha1_hex(s.as_bytes())),
        None => None,
    };
    Ok(Evidence {
        path: path.to_string(),
        lines,
        blob,
        snippet_sha1,
        note: None,
    })
}

#[derive(Debug)]
pub struct EvidenceReport {
    pub path: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug)]
pub struct PageReport {
    pub id: String,
    pub title: String,
    pub status_before: Status,
    pub is_stale: bool,
    pub stale_since: Option<String>,
    pub evidence_reports: Vec<EvidenceReport>,
}

/// Git-based freshness check: compares recorded blobs against HEAD and,
/// when they differ, confirms via diff whether the evidence paths changed
/// in commits between generation and HEAD.
pub fn check_page(page: &WikiPage, repo: &GitRepo) -> Result<PageReport> {
    let mut reports = Vec::new();
    let mut stale = false;

    if !repo.commit_exists(&page.meta.commit) {
        stale = true;
        reports.push(EvidenceReport {
            path: "<commit>".into(),
            ok: false,
            detail: format!(
                "generated against unknown commit {}",
                page.meta.commit
            ),
        });
    }

    let mut changed_paths: Vec<String> = Vec::new();
    if !page.meta.evidence.is_empty() && repo.commit_exists(&page.meta.commit) {
        let refs: Vec<&str> = page.meta.evidence.iter().map(|e| e.path.as_str()).collect();
        changed_paths = repo.changed_paths_since(&page.meta.commit, &refs)?;
    }

    let mut renames: Option<std::collections::BTreeMap<String, String>> = None;
    for e in &page.meta.evidence {
        let current = repo.head_blob_oid(&e.path)?;
        let in_diff = changed_paths.iter().any(|p| p == &e.path);
        match current {
            None => {
                stale = true;
                if renames.is_none() {
                    renames = Some(
                        if repo.commit_exists(&page.meta.commit) {
                            repo.renames_since(&page.meta.commit).into_iter().collect()
                        } else {
                            Default::default()
                        },
                    );
                }
                let map = renames.as_ref().unwrap();
                let detail = match (repo.path_of_blob_in_head(&e.blob)?, map.get(&e.path)) {
                    (Some(found), _) => {
                        format!("renamed to {found} (identical blob; recoverable via refresh)")
                    }
                    (None, Some(new_path)) => {
                        format!("renamed to {new_path} (recoverable via refresh)")
                    }
                    _ => "file no longer exists at HEAD".to_string(),
                };
                reports.push(EvidenceReport {
                    path: e.path.clone(),
                    ok: false,
                    detail,
                });
            }
            Some(oid) if oid != e.blob || in_diff => {
                stale = true;
                reports.push(EvidenceReport {
                    path: e.path.clone(),
                    ok: false,
                    detail: if oid != e.blob {
                        format!(
                            "blob changed {} -> {} (changed in commits since {})",
                            short(&e.blob),
                            short(&oid),
                            short(&page.meta.commit)
                        )
                    } else {
                        format!(
                            "path {} appears in git diff since {}",
                            e.path,
                            short(&page.meta.commit)
                        )
                    },
                });
            }
            Some(_) => reports.push(EvidenceReport {
                path: e.path.clone(),
                ok: true,
                detail: "blob matches HEAD".into(),
            }),
        }
    }

    Ok(PageReport {
        id: page.meta.id.clone(),
        title: page.meta.title.clone(),
        status_before: page.meta.status,
        is_stale: stale,
        stale_since: if stale { Some(repo.head_short()?) } else { None },
        evidence_reports: reports,
    })
}

fn short(sha: &str) -> String {
    sha.chars().take(8).collect()
}

#[derive(Debug)]
pub struct VerifyReport {
    pub id: String,
    pub results: Vec<EvidenceReport>,
    pub all_ok: bool,
}

/// Snippet-level verification: recompute sha1 of the recorded line range at
/// HEAD and compare, so trust does not rest on commit ids alone.
pub fn verify_page(page: &WikiPage, repo: &GitRepo) -> Result<VerifyReport> {
    let mut results = Vec::new();
    let mut all_ok = true;
    for e in &page.meta.evidence {
        let current_blob = repo.head_blob_oid(&e.path)?;
        let detail = match (&e.lines, &e.snippet_sha1, current_blob.as_deref()) {
            (Some([lo, hi]), Some(expected), Some(_)) => {
                match repo.head_lines(&e.path, *lo, *hi)? {
                    None => (false, format!("{} not found at HEAD", e.path)),
                    Some(snippet) => {
                        let actual = sha1_hex(snippet.as_bytes());
                        if actual == *expected {
                            (true, format!("snippet {lo}-{hi} matches recorded hash"))
                        } else {
                            (false, format!(
                                "snippet {lo}-{hi} hash mismatch (recorded {}, HEAD has {actual})",
                                &expected[..8.min(expected.len())]
                            ))
                        }
                    }
                }
            }
            (None, _, Some(oid)) => {
                let ok = oid == e.blob;
                (
                    ok,
                    if ok { "whole-file blob matches".into() } else { format!("whole-file blob changed: {} -> {}", short(&e.blob), short(&oid)) },
                )
            }
            (None, Some(_), _) => (false, "no lines recorded for snippet hash".into()),
            (Some(_), None, _) => (true, "no snippet hash recorded (blob pin only)".into()),
            (_, _, None) => (false, format!("{} not found at HEAD", e.path)),
        };
        if !detail.0 {
            all_ok = false;
        }
        results.push(EvidenceReport {
            path: e.path.clone(),
            ok: detail.0,
            detail: detail.1,
        });
    }
    Ok(VerifyReport {
        id: page.meta.id.clone(),
        results,
        all_ok,
    })
}

/// Re-pin the page's evidence to current HEAD content and mark verified
/// (blind re-pin; prefer `refresh_page_with_drift` for changed files).
pub fn refresh_page(page: &mut WikiPage, repo: &GitRepo) -> Result<()> {
    let mut fresh = Vec::new();
    for e in &page.meta.evidence {
        fresh.push(record_evidence(repo, &e.path, e.lines)?);
    }
    page.meta.evidence = fresh;
    page.meta.commit = repo.head_commit()?;
    page.meta.generated_at = now_iso();
    page.meta.status = Status::Verified;
    page.save_in_place()
}

pub fn init_wiki_layout(wiki_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(pages_dir(wiki_dir))?;
    let config = wiki_dir.join("wiki.toml");
    if !config.exists() {
        std::fs::write(
            &config,
            "# repowiki configuration\n[wiki]\npages = \"pages\"\n# cache directory is gitignored; rebuildable, never committed\n",
        )?;
    }
    std::fs::create_dir_all(wiki_dir.join(".cache"))?;
    Ok(())
}
