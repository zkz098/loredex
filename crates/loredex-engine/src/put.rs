use loredex_core::{
    dedup_citations, parse_citations, Evidence, PageMeta, Status, WikiError, WikiPage,
};
use loredex_git::{GitRepo, GitView};

use crate::{now_iso, record_evidence_symbol_with, redact, Result};

#[derive(Debug)]
pub struct PutOutcome {
    pub page: WikiPage,
    pub citations: usize,
    pub created: bool,
    /// sha1 of the page file as written; pass it back as `expected` on the
    /// next write to detect a concurrent edit.
    pub page_sha1: String,
}

/// Optimistic-concurrency precondition for a write. Reads return the page's
/// content sha1 (`page_sha1`); a writer that supplies it only succeeds while
/// the file is still exactly what it read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageExpectation {
    /// Create only: fail when a page with this id already exists.
    Absent,
    /// Update only: fail when the page is missing or its sha1 differs.
    Sha1(String),
}

#[derive(Debug, Clone)]
pub struct PutOptions {
    /// Store citations even when the cited range matches a secret pattern.
    /// The scan never echoes snippet content, so opting out is the operator's
    /// explicit call (e.g. a repo that documents test fixtures).
    pub allow_secret_citations: bool,
    /// Capture status. `Draft` relaxes the citation contract: an uncited body
    /// is stored as an unverified draft that `plan` queues for backfill, and
    /// any citations the body happens to carry are still pinned.
    pub status: Status,
    /// Reject the write when the page file changed since this token was read.
    pub expected: Option<PageExpectation>,
}

impl Default for PutOptions {
    fn default() -> Self {
        Self {
            allow_secret_citations: false,
            status: Status::Verified,
            expected: None,
        }
    }
}

/// A citation-free capture for notes/rules/troubleshooting found mid-task:
/// uncited bodies land as `draft` (never surfaced as verified claims);
/// a body that does carry citations is stored `verified` like `put_page`.
pub fn note_page(
    repo: &GitRepo,
    wiki_dir: &std::path::Path,
    id: &str,
    title: &str,
    body: &str,
    tags: Vec<String>,
    page_type: Option<String>,
) -> Result<PutOutcome> {
    note_page_with(repo, wiki_dir, id, title, body, tags, page_type, None)
}

/// `note_page` with an optional optimistic-concurrency precondition.
#[allow(clippy::too_many_arguments)]
pub fn note_page_with(
    repo: &GitRepo,
    wiki_dir: &std::path::Path,
    id: &str,
    title: &str,
    body: &str,
    tags: Vec<String>,
    page_type: Option<String>,
    expected: Option<PageExpectation>,
) -> Result<PutOutcome> {
    put_page_with(
        &repo.view(),
        wiki_dir,
        id,
        title,
        body,
        tags,
        page_type,
        &PutOptions {
            allow_secret_citations: false,
            status: Status::Draft,
            expected,
        },
    )
}

/// Store wiki content authored by a coding agent: the body must carry
/// `[[cite:path:Lx-Ly]]` markers; every marker becomes a git-pinned
/// evidence entry. A body without citations is rejected — that is the
/// trust guarantee: no claim enters the wiki without a resolvable source
/// pointer at HEAD.
pub fn put_page(
    repo: &GitRepo,
    wiki_dir: &std::path::Path,
    id: &str,
    title: &str,
    body: &str,
    tags: Vec<String>,
    page_type: Option<String>,
) -> Result<PutOutcome> {
    put_page_with(
        &repo.view(),
        wiki_dir,
        id,
        title,
        body,
        tags,
        page_type,
        &PutOptions::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn put_page_with(
    view: &GitView,
    wiki_dir: &std::path::Path,
    id: &str,
    title: &str,
    body: &str,
    tags: Vec<String>,
    page_type: Option<String>,
    opts: &PutOptions,
) -> Result<PutOutcome> {
    loredex_core::validate_page_id(id).map_err(|m| {
        WikiError::Git(format!(
            "invalid page id `{id}` ({m}); use alphanumeric, '-', '_' or '/'"
        ))
    })?;
    if crate::index::is_generated(id) {
        return Err(WikiError::Git(format!(
            "page id `{id}` is reserved for generated navigation; choose another id"
        )));
    }
    let cites = dedup_citations(parse_citations(body).map_err(|e| {
        WikiError::FrontmatterParse(id.into(), format!("citation parse error: {e}"))
    })?);
    if cites.is_empty() && opts.status != Status::Draft {
        return Err(WikiError::FrontmatterParse(
            id.into(),
            "body contains no [[cite:path:Lx-Ly]] markers; every wiki page must cite its sources \
             (use `note` / status Draft for an uncited capture)"
                .into(),
        ));
    }

    let mut evidence: Vec<Evidence> = Vec::new();
    for c in &cites {
        let ev = record_evidence_symbol_with(view, &c.path, c.symbol.as_deref(), c.lines).map_err(
            |e| {
                WikiError::Git(format!(
                    "citation [{}] cannot be pinned to HEAD: {e}",
                    c.path
                ))
            },
        )?;
        evidence.push(ev);
    }

    // secret gate: reject citations that point public wiki readers at secret
    // material; report every hit at once so the author fixes them in one pass.
    // The whole file is scanned for context so a range inside a multi-line
    // secret (PEM body) is caught even without the block's header line.
    if !opts.allow_secret_citations {
        let mut hits: Vec<String> = Vec::new();
        for e in &evidence {
            if let Some([lo, hi]) = e.lines {
                if let Some(content) = view.head_file_content(&e.path)? {
                    hits.extend(redact::scan_range(&content, lo, hi).into_iter().map(|f| {
                        format!(
                            "{}:L{} [{rule}]",
                            e.path,
                            lo + f.line as u32 - 1,
                            rule = f.rule
                        )
                    }));
                }
            }
        }
        if !hits.is_empty() {
            return Err(WikiError::FrontmatterParse(
                id.into(),
                format!(
                    "citations point at secret-shaped content: {}; cite different lines or pass --no-redact / allow_secret_citations to override",
                    hits.join(", ")
                ),
            ));
        }
    }

    let path = crate::pages_dir(wiki_dir).join(format!("{id}.md"));
    std::fs::create_dir_all(path.parent().expect("pages dir")).map_err(WikiError::Io)?;
    let status = if opts.status == Status::Draft && cites.is_empty() {
        Status::Draft
    } else {
        Status::Verified
    };
    let page = WikiPage {
        meta: PageMeta {
            id: id.to_string(),
            title: title.to_string(),
            status,
            generated_at: now_iso(),
            commit: view.head_commit()?.to_string(),
            evidence,
            tags,
            page_type,
        },
        body: body.into(),
        source_path: Some(path.clone()),
    };

    // Evidence pinning is done; only the digest check + rename need the page
    // lock, so a slow citation read never blocks another writer for long.
    let (created, page_sha1) = {
        let _lock = crate::lock::FileLock::page(wiki_dir, id)?;
        let current = crate::page_sha1(&path)?;
        check_expectation(id, opts.expected.as_ref(), current.as_deref())?;
        let created = current.is_none();
        page.save(&path)?;
        let digest = crate::page_sha1(&path)?.unwrap_or_default();
        (created, digest)
    };

    // authoring a page resolves its plan task; a draft capture is the opposite
    // of resolution — the work (evidence + typing) is only starting
    if status != Status::Draft {
        let _ = crate::plan::mark_task(wiki_dir, id, crate::plan::TaskStatus::Done);
    }
    Ok(PutOutcome {
        page,
        citations: cites.len(),
        created,
        page_sha1,
    })
}

fn check_expectation(
    id: &str,
    expected: Option<&PageExpectation>,
    current: Option<&str>,
) -> Result<()> {
    match expected {
        None => Ok(()),
        Some(PageExpectation::Absent) => match current {
            None => Ok(()),
            Some(actual) => Err(WikiError::Conflict(format!(
                "page `{id}` already exists (sha1 {actual}); re-read it, or write without --if-absent"
            ))),
        },
        Some(PageExpectation::Sha1(want)) => {
            let want = want.to_ascii_lowercase();
            match current {
                Some(actual) if actual == want => Ok(()),
                Some(actual) => Err(WikiError::Conflict(format!(
                    "page `{id}` changed since you read it (expected sha1 {want}, current {actual}); \
                     re-read the page and retry with the new page_sha1"
                ))),
                None => Err(WikiError::Conflict(format!(
                    "page `{id}` does not exist (expected sha1 {want}); create it without --if-match"
                ))),
            }
        }
    }
}
