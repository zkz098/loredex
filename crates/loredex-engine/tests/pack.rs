//! Context packs: module relevance, trust filtering, evidence dedup, token
//! budgeting and secret masking.

use git2::{Repository, Signature};
use loredex_core::{PageMeta, Status, WikiPage};
use loredex_engine::{
    context_pack, init_wiki_layout, put_page_with, record_evidence, PackOptions, PutOptions,
};
use loredex_git::GitRepo;
use tempfile::TempDir;

struct Fx {
    tmp: TempDir,
    repo: GitRepo,
}

fn commit_all(path: &std::path::Path, msg: &str) {
    let git = Repository::open(path).unwrap();
    let sig = Signature::now("t", "t@l").unwrap();
    let mut idx = git.index().unwrap();
    idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    let tree = git.find_tree(idx.write_tree().unwrap()).unwrap();
    let parents: Vec<git2::Commit> = git
        .head()
        .ok()
        .map(|h| vec![h.peel_to_commit().unwrap()])
        .unwrap_or_default();
    let refs: Vec<&git2::Commit> = parents.iter().collect();
    git.commit(Some("HEAD"), &sig, &sig, msg, &tree, &refs)
        .unwrap();
}

impl Fx {
    fn new(files: &[(&str, &str)]) -> Self {
        let tmp = TempDir::new().unwrap();
        Repository::init(tmp.path()).unwrap();
        for (p, c) in files {
            let f = tmp.path().join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, c).unwrap();
        }
        commit_all(tmp.path(), "init");
        let repo = GitRepo::discover(tmp.path()).unwrap();
        init_wiki_layout(&repo.wiki_dir()).unwrap();
        Self { tmp, repo }
    }
    fn wiki(&self) -> std::path::PathBuf {
        self.repo.wiki_dir()
    }
    fn put(&self, id: &str, body: &str) {
        put_page_with(
            &self.repo.view(),
            &self.wiki(),
            id,
            id,
            body,
            vec![],
            None,
            &PutOptions::default(),
        )
        .unwrap();
    }
    fn edit(&self, path: &str, content: &str) {
        std::fs::write(self.tmp.path().join(path), content).unwrap();
        commit_all(self.tmp.path(), "edit");
    }
}

fn write_draft(fx: &Fx, id: &str, path: &str, lines: [u32; 2]) {
    let evidence = record_evidence(&fx.repo, path, Some(lines)).unwrap();
    let file = fx.wiki().join(format!("pages/{id}.md"));
    let page = WikiPage {
        meta: PageMeta {
            id: id.to_string(),
            title: id.to_string(),
            status: Status::Draft,
            generated_at: "2026-01-01T00:00:00Z".into(),
            commit: fx.repo.head_commit().unwrap(),
            evidence: vec![evidence],
            tags: vec![],
            page_type: None,
        },
        body: "an unverified capture\n".into(),
        source_path: Some(file.clone()),
    };
    page.save(&file).unwrap();
}

fn fixture() -> Fx {
    let fx = Fx::new(&[
        ("alpha/fresh.rs", "pub fn alpha() {\n    helper();\n}\n"),
        ("alpha/moving.rs", "pub fn moving() {\n}\n"),
        ("beta/b.rs", "pub fn beta() {}\n"),
    ]);
    fx.put("alpha-one", "# one\n\n[[cite:alpha/fresh.rs:L1-L2]]\n");
    fx.put("alpha-two", "# two\n\n[[cite:alpha/fresh.rs:L1-L2]]\n");
    fx.put("alpha-stale", "# stale\n\n[[cite:alpha/moving.rs:L1-L2]]\n");
    fx.put("beta-page", "# beta\n\n[[cite:beta/b.rs:L1-L1]]\n");
    write_draft(&fx, "alpha-draft", "alpha/fresh.rs", [1, 1]);
    // The stale page's evidence changes after authoring.
    fx.edit("alpha/moving.rs", "pub fn moving() {\n    changed();\n}\n");
    fx
}

#[test]
fn pack_selects_module_pages_and_dedups_evidence() {
    let fx = fixture();
    let options = PackOptions {
        budget_tokens: 8_000,
        ..Default::default()
    };
    let pack = context_pack(&fx.repo.view(), &fx.wiki(), "alpha", &options).unwrap();

    let ids: Vec<&str> = pack.pages.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["alpha-one", "alpha-two"]);
    assert_eq!(pack.evidence.len(), 1, "{:?}", pack.evidence);
    assert_eq!(pack.evidence[0].path, "alpha/fresh.rs");
    assert_eq!(pack.evidence[0].pages, vec!["alpha-one", "alpha-two"]);
    assert!(pack.skipped.iter().any(|s| s.id == "alpha-stale"));
    assert!(pack.skipped.iter().any(|s| s.id == "alpha-draft"));
    assert!(!pack.truncated);
    assert!(pack.estimated_tokens <= 8_000);
    assert_eq!(pack.head.len(), 40);
    assert!(pack.markdown.contains("## Evidence"));
    // beta has no evidence under alpha and must not leak in.
    assert!(!pack.markdown.contains("beta/b.rs"));
}

#[test]
fn pack_budget_truncates_without_overshooting() {
    let fx = fixture();
    for budget in [16usize, 40] {
        let pack = context_pack(
            &fx.repo.view(),
            &fx.wiki(),
            "alpha",
            &PackOptions {
                budget_tokens: budget,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            pack.estimated_tokens <= pack.budget_tokens,
            "budget {} rendered {}",
            pack.budget_tokens,
            pack.estimated_tokens
        );
        assert!(
            pack.truncated,
            "budget {budget} unexpectedly fit everything"
        );
    }
}

#[test]
fn pack_masks_secret_shaped_snippet_lines() {
    let fx = Fx::new(&[(
        "secret/c.rs",
        "const KEY: &str = \"AKIAIOSFODNN7EXAMPLE\";\n",
    )]);
    put_page_with(
        &fx.repo.view(),
        &fx.wiki(),
        "secret-page",
        "secret",
        "# secret\n\n[[cite:secret/c.rs:L1-L1]]\n",
        vec![],
        None,
        &PutOptions {
            allow_secret_citations: true,
            ..Default::default()
        },
    )
    .unwrap();

    let pack = context_pack(
        &fx.repo.view(),
        &fx.wiki(),
        "secret",
        &PackOptions::default(),
    )
    .unwrap();
    assert_eq!(pack.pages.len(), 1);
    assert_eq!(pack.evidence.len(), 1);
    assert_eq!(pack.evidence[0].masked_lines, 1);
    assert!(pack.markdown.contains("[masked]"));
    assert!(!pack.markdown.contains("AKIAIOSFODNN7EXAMPLE"));
}
