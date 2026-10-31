use loredex_core::{
    dedup_citations, parse_citations, Evidence, PageMeta, Status, WikiError, WikiPage,
};
use loredex_git::{GitRepo, GitView};

use crate::{now_iso, record_evidence_with, Result};

#[derive(Debug)]
pub struct PutOutcome {
    pub page: WikiPage,
    pub citations: usize,
    pub created: bool,
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
    put_page_with(&repo.view(), wiki_dir, id, title, body, tags, page_type)
}

pub fn put_page_with(
    view: &GitView,
    wiki_dir: &std::path::Path,
    id: &str,
    title: &str,
    body: &str,
    tags: Vec<String>,
    page_type: Option<String>,
) -> Result<PutOutcome> {
    loredex_core::validate_page_id(id).map_err(|m| {
        WikiError::Git(format!(
            "invalid page id `{id}` ({m}); use alphanumeric, '-', '_' or '/'"
        ))
    })?;
    let cites = dedup_citations(parse_citations(body).map_err(|e| {
        WikiError::FrontmatterParse(id.into(), format!("citation parse error: {e}"))
    })?);
    if cites.is_empty() {
        return Err(WikiError::FrontmatterParse(
            id.into(),
            "body contains no [[cite:path:Lx-Ly]] markers; every wiki page must cite its sources"
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

    let path = crate::pages_dir(wiki_dir).join(format!("{id}.md"));
    std::fs::create_dir_all(path.parent().expect("pages dir")).map_err(WikiError::Io)?;
    let created = !path.exists();
    let page = WikiPage {
        meta: PageMeta {
            id: id.to_string(),
            title: title.to_string(),
            status: Status::Verified,
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
    // authoring a page resolves its plan task, if one is pending
    let _ = crate::plan::mark_task(wiki_dir, id, crate::plan::TaskStatus::Done);
    Ok(PutOutcome {
        page,
        citations: cites.len(),
        created,
    })
}
