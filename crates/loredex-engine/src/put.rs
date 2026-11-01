use loredex_core::{
    dedup_citations, parse_citations, Evidence, PageMeta, Status, WikiError, WikiPage,
};
use loredex_git::{GitRepo, GitView};

use crate::{now_iso, record_evidence_with, redact, Result};

#[derive(Debug)]
pub struct PutOutcome {
    pub page: WikiPage,
    pub citations: usize,
    pub created: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PutOptions {
    /// Store citations even when the cited range matches a secret pattern.
    /// The scan never echoes snippet content, so opting out is the operator's
    /// explicit call (e.g. a repo that documents test fixtures).
    pub allow_secret_citations: bool,
    /// Capture status. `Draft` relaxes the citation contract: an uncited body
    /// is stored as an unverified draft that `plan` queues for backfill, and
    /// any citations the body happens to carry are still pinned.
    pub status: Status,
}

impl Default for PutOptions {
    fn default() -> Self {
        Self {
            allow_secret_citations: false,
            status: Status::Verified,
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
        let ev = record_evidence_with(view, &c.path, c.lines).map_err(|e| {
            WikiError::Git(format!(
                "citation [{}] cannot be pinned to HEAD: {e}",
                c.path
            ))
        })?;
        evidence.push(ev);
    }

    // secret gate: reject citations that point public wiki readers at secret
    // material; report every hit at once so the author fixes them in one pass
    if !opts.allow_secret_citations {
        let mut hits: Vec<String> = Vec::new();
        for e in &evidence {
            if let Some([lo, hi]) = e.lines {
                if let Some(snippet) = view.head_lines(&e.path, lo, hi)? {
                    let lines: Vec<&str> = snippet.lines().collect();
                    hits.extend(redact::scan_lines(&lines).into_iter().map(|f| {
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
    let created = !path.exists();
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
    page.save(&path)?;
    // authoring a page resolves its plan task; a draft capture is the opposite
    // of resolution — the work (evidence + typing) is only starting
    if status != Status::Draft {
        let _ = crate::plan::mark_task(wiki_dir, id, crate::plan::TaskStatus::Done);
    }
    Ok(PutOutcome {
        page,
        citations: cites.len(),
        created,
    })
}
