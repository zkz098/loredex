//! Extra unit tests for core model changes (type field, hierarchy, ids).

use loredex_core::{parse_wiki_links, validate_page_id, PageMeta, Status, WikiPage};

#[test]
fn type_field_roundtrips() {
    let mut meta = PageMeta {
        id: "x".into(),
        title: "t".into(),
        status: Status::Verified,
        generated_at: "2026-01-01T00:00:00Z".into(),
        commit: "abc".into(),
        evidence: vec![],
        tags: vec![],
        page_type: Some("architecture".into()),
    };
    let page = WikiPage {
        meta: meta.clone(),
        body: "b".into(),
        source_path: None,
    };
    let md = page.to_markdown().unwrap();
    assert!(md.contains("type: architecture"), "{md}");
    let parsed = WikiPage::from_markdown(&md, None).unwrap();
    assert_eq!(parsed.meta.page_type.as_deref(), Some("architecture"));
    meta.page_type = None;
    let md2 = WikiPage {
        meta,
        body: "b".into(),
        source_path: None,
    }
    .to_markdown()
    .unwrap();
    assert!(!md2.contains("type:"), "{md2}");
}

#[test]
fn id_validation() {
    assert!(validate_page_id("architecture/overview").is_ok());
    assert!(validate_page_id("a/b/c-d_e2").is_ok());
    assert!(validate_page_id("../evil").is_err());
    assert!(validate_page_id("a/../../b").is_err());
    assert!(validate_page_id("/abs").is_err());
    assert!(validate_page_id("trail/").is_err());
    assert!(validate_page_id("with space").is_err());
    assert!(validate_page_id("").is_err());
}

#[test]
fn wiki_links_parsed() {
    let body = "see [[wiki:architecture/overview]] and [[wiki:index]]. broken [[wiki:nope/..x]]";
    let links = parse_wiki_links(body);
    assert_eq!(links, vec!["architecture/overview", "index"]);
}
