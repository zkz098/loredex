//! Redaction gate + citation draft generation.

use git2::{Repository, Signature};
use loredex_engine::{evidence_candidates, put_page_with, EvidenceCandidates, PutOptions};
use loredex_git::GitRepo;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Fixture {
    _tmp: TempDir,
    repo: GitRepo,
}

fn commit_all(repo_path: &Path) {
    let git = Repository::open(repo_path).expect("open");
    let sig = Signature::now("test", "test@local").expect("sig");
    let mut idx = git.index().unwrap();
    idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    let tree_id = idx.write_tree().unwrap();
    let tree = git.find_tree(tree_id).unwrap();
    let head = git.head().ok().map(|h| h.peel_to_commit().unwrap());
    match head {
        Some(p) => {
            git.commit(Some("HEAD"), &sig, &sig, "c", &tree, &[&p])
                .unwrap();
        }
        None => {
            git.commit(Some("HEAD"), &sig, &sig, "c", &tree, &[])
                .unwrap();
        }
    }
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let tmp = TempDir::new().expect("tempdir");
        Repository::init(tmp.path()).expect("git init");
        for (path, content) in files {
            let p = tmp.path().join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, content).unwrap();
        }
        commit_all(tmp.path());
        let repo = GitRepo::discover(tmp.path()).expect("discover");
        loredex_engine::init_wiki_layout(&repo.wiki_dir()).expect("init layout");
        Self { _tmp: tmp, repo }
    }

    fn workdir(&self) -> &Path {
        self.repo.workdir()
    }

    fn wiki(&self) -> PathBuf {
        self.repo.wiki_dir()
    }
}

const CLEAN: &str = "pub fn serve() {\n    loop {\n        handle();\n    }\n}\n";
const SECRET: &str =
    "fn load() {\n    let aws_key = \"AKIAIOSFODNN7EXAMPLE\";\n    configure(aws_key);\n}\n";

#[test]
fn put_rejects_secret_citation_and_lint_flags_it() {
    let fx = Fixture::new(&[("src/svc.rs", SECRET)]);

    let body = "loader reads the config [[cite:src/svc.rs:L1-L2]].";
    let err = put_page_with(
        &fx.repo.view(),
        &fx.wiki(),
        "svc",
        "Svc",
        body,
        vec![],
        None,
        &PutOptions::default(),
    )
    .expect_err("secret range must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("src/svc.rs:L2"), "{msg}");
    assert!(msg.contains("aws-access-key"), "{msg}");
    // the error message must not echo the secret itself
    assert!(!msg.contains("AKIAIOSFODNN7EXAMPLE"), "{msg}");
    assert!(!fx.workdir().join(".wiki/pages/svc.md").exists());

    // explicit override stores the page
    put_page_with(
        &fx.repo.view(),
        &fx.wiki(),
        "svc",
        "Svc",
        body,
        vec![],
        None,
        &PutOptions {
            allow_secret_citations: true,
        },
    )
    .expect("override puts");

    // lint re-scans the stored page's range and flags it
    let issues = loredex_engine::lint_with(&fx.repo.view(), &fx.wiki()).unwrap();
    let secret_issues: Vec<_> = issues
        .iter()
        .filter(|i| i.kind == "secret-in-evidence")
        .collect();
    assert_eq!(secret_issues.len(), 1, "{issues:?}");
    assert!(secret_issues[0].detail.contains("src/svc.rs:L2"));
}

#[test]
fn put_accepts_clean_citations() {
    let fx = Fixture::new(&[("src/svc.rs", CLEAN)]);
    let body = "serve loops over requests [[cite:src/svc.rs:L1-L5]].";
    let out = put_page_with(
        &fx.repo.view(),
        &fx.wiki(),
        "svc",
        "Svc",
        body,
        vec![],
        None,
        &PutOptions::default(),
    )
    .expect("clean put");
    assert_eq!(out.citations, 1);
    assert!(loredex_engine::lint_with(&fx.repo.view(), &fx.wiki())
        .unwrap()
        .is_empty());
}

#[test]
fn candidates_returns_pasteable_masked_windows() {
    let src = "use db::Pool;\n\npub fn serve() {\n    let pool = Pool::open();\n    loop {\n        handle(&pool);\n    }\n}\n\npub fn shutdown() {\n    let token = \"abc123def456ghi789jkl0\";\n    flush(token);\n}\n";
    let fx = Fixture::new(&[("src/svc.rs", src)]);
    let view = fx.repo.view();
    let EvidenceCandidates {
        windows,
        matched_files,
        ..
    }: EvidenceCandidates = evidence_candidates(&view, ".", &["pool".into()], 5).unwrap();
    assert_eq!(matched_files, 1);
    assert_eq!(windows.len(), 1);
    let w = &windows[0];
    assert_eq!(w.path, "src/svc.rs");
    assert_eq!(
        w.cite,
        format!("[[cite:src/svc.rs:L{}-L{}]]", w.lines[0], w.lines[1])
    );
    // the window covers the `pool` lines (4) plus ±2 context, merged
    assert!(w.lines[0] <= 4 && w.lines[1] >= 4, "{:?}", w.lines);

    // a secret-bearing line can be a candidate, but its preview is masked
    let out = evidence_candidates(&view, ".", &["token".into()], 5).unwrap();
    assert_eq!(out.windows.len(), 1);
    let preview = out.windows[0].masked_preview.join("\n");
    assert!(!preview.contains("abc123def456ghi789jkl0"), "{preview}");
    assert!(preview.contains("[masked]"), "{preview}");
}
