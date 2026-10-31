use wiki_core::{dedup_citations, parse_citations, Evidence, PageMeta, Status, WikiPage, WikiError};
use wiki_git::GitRepo;

use crate::{now_iso, record_evidence, Result};

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
) -> Result<PutOutcome> {
    if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(WikiError::Git(format!(
            "invalid page id `{id}` (letters, digits, '-' or '_')"
        )));
    }
    let cites = dedup_citations(parse_citations(body).map_err(|e| {
        WikiError::FrontmatterParse(id.into(), format!("citation parse error: {e}"))
    })?);
    if cites.is_empty() {
        return Err(WikiError::FrontmatterParse(
            id.into(),
            "body contains no [[cite:path:Lx-Ly]] markers; every wiki page must cite its sources".into(),
        ));
    }

    let mut evidence: Vec<Evidence> = Vec::new();
    for c in &cites {
        let ev = record_evidence(repo, &c.path, c.lines).map_err(|e| {
            WikiError::Git(format!("citation [{}] cannot be pinned to HEAD: {e}", c.path))
        })?;
        evidence.push(ev);
    }

    let path = crate::pages_dir(wiki_dir).join(format!("{id}.md"));
    std::fs::create_dir_all(path.parent().expect("pages dir"))
        .map_err(WikiError::Io)?;
    let created = !path.exists();
    let page = WikiPage {
        meta: PageMeta {
            id: id.to_string(),
            title: title.to_string(),
            status: Status::Verified,
            generated_at: now_iso(),
            commit: repo.head_commit()?,
            evidence,
            tags,
        },
        body: body.to_string(),
        source_path: Some(path.clone()),
    };
    page.save(&path)?;
    Ok(PutOutcome {
        page,
        citations: cites.len(),
        created,
    })
}
