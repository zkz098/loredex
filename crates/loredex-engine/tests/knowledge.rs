//! Tests for the knowledge-maintenance pipeline: policy page types, the
//! uncited `note` capture path that lands as draft, consolidation clustering
//! and the advisory overlapping-evidence lint.

use git2::{Repository, Signature};
use loredex_core::Status;
use loredex_engine::{
    consolidation_clusters, find_page, lint_with, note_page, open_plan, playbook_registry,
    put_page, TaskKind, TaskStatus,
};
use loredex_git::GitRepo;
use tempfile::TempDir;

struct Fx {
    _tmp: TempDir,
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
        Self { _tmp: tmp, repo }
    }
    fn wiki(&self) -> std::path::PathBuf {
        self.repo.wiki_dir()
    }
}

const SRC: &str = "fn a() {\n  b();\n}\n\nfn b() {}\n";

#[test]
fn policy_types_are_first_class_playbooks_in_both_locales() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    loredex_engine::init_wiki_layout(&fx.wiki()).unwrap();
    for (lang, marker) in [
        (
            "en",
            [
                ("rule", "enforcement point"),
                ("troubleshooting", "symptom & error text"),
                ("decision", "options considered"),
            ],
        ),
        (
            "zh",
            [
                ("rule", "强制点"),
                ("troubleshooting", "症状与报错原文"),
                ("decision", "备选方案"),
            ],
        ),
    ] {
        std::fs::write(
            fx.wiki().join("wiki.toml"),
            format!("[wiki]\npages = \"pages\"\nlanguage = \"{lang}\"\n"),
        )
        .unwrap();
        let reg = playbook_registry(&fx.wiki());
        for (t, section) in marker {
            let pb = reg.get(t).unwrap_or_else(|| panic!("{lang}: {t} missing"));
            assert!(
                pb.sections.iter().any(|s| s.contains(section)),
                "{lang}: {t} sections {:?}",
                pb.sections
            );
            assert!(!pb.checklist.is_empty(), "{lang}: {t} checklist empty");
        }
    }

    // a declared policy type passes lint (would be `unknown-type` otherwise)
    put_page(
        &fx.repo,
        &fx.wiki(),
        "no-unwrap",
        "No unwrap in src/",
        "Never unwrap in `src` [[cite:src.rs:L1-L5]].",
        vec![],
        Some("rule".into()),
    )
    .unwrap();
    let issues = lint_with(&fx.repo.view(), &fx.wiki()).unwrap();
    assert!(
        !issues.iter().any(|i| i.kind == "unknown-type"),
        "{issues:?}"
    );
}

#[test]
fn uncited_note_lands_as_draft_and_plan_queues_backfill() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    let out = note_page(
        &fx.repo,
        &fx.wiki(),
        "notes/retry-bug",
        "Retry hangs on reconnect",
        "Symptom: `reconnect` spins forever. Root cause suspected in the backoff loop.\n",
        vec!["triage".into()],
        Some("troubleshooting".into()),
    )
    .unwrap();
    assert_eq!(out.page.meta.status, Status::Draft);
    assert_eq!(out.citations, 0);
    assert!(out.page.meta.evidence.is_empty());

    let run = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let task = run
        .tasks
        .iter()
        .find(|t| t.page_id == "notes/retry-bug")
        .expect("draft must appear as authoring work");
    assert_eq!(task.kind, TaskKind::Refresh);
    assert!(task.purpose.contains("draft"), "{}", task.purpose);
    assert_eq!(task.suggested_type, "troubleshooting");

    // a draft capture never resolves work: New tasks for the same id stay pending
    let pid = run
        .tasks
        .iter()
        .find(|t| t.kind == TaskKind::New)
        .expect("uncovered src.rs")
        .page_id
        .clone();
    note_page(
        &fx.repo,
        &fx.wiki(),
        &pid,
        "Uncited stub",
        "placeholder observation, no evidence\n",
        vec![],
        None,
    )
    .unwrap();
    let ledger = loredex_engine::load_ledger(&fx.wiki());
    let task = ledger
        .active_run()
        .unwrap()
        .tasks
        .iter()
        .find(|t| t.page_id == pid)
        .unwrap();
    assert_eq!(task.status, TaskStatus::Pending);
    assert_eq!(
        find_page(&fx.wiki(), &pid).unwrap().meta.status,
        Status::Draft
    );

    // promotion: the same id through put (cited) rewrites it verified and
    // resolves the pending task
    put_page(
        &fx.repo,
        &fx.wiki(),
        "notes/retry-bug",
        "Retry hangs on reconnect",
        "The backoff loop never yields [[cite:src.rs:L1-L3]].",
        vec!["triage".into()],
        Some("troubleshooting".into()),
    )
    .unwrap();
    let page = find_page(&fx.wiki(), "notes/retry-bug").unwrap();
    assert_eq!(page.meta.status, Status::Verified);
    assert_eq!(page.meta.evidence.len(), 1);
    let ledger = loredex_engine::load_ledger(&fx.wiki());
    let task = ledger
        .active_run()
        .unwrap()
        .tasks
        .iter()
        .find(|t| t.page_id == "notes/retry-bug")
        .unwrap();
    assert_eq!(task.status, TaskStatus::Done);
}

#[test]
fn note_with_citations_is_pinned_and_stays_verified() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    let out = note_page(
        &fx.repo,
        &fx.wiki(),
        "rule-no-unwrap",
        "No unwrap in src",
        "`a` delegates to `b` [[cite:src.rs:L1-L2]].",
        vec![],
        Some("rule".into()),
    )
    .unwrap();
    assert_eq!(out.page.meta.status, Status::Verified);
    assert_eq!(out.citations, 1);
    assert_eq!(out.page.meta.evidence[0].path, "src.rs");
}

#[test]
fn put_still_rejects_uncited_bodies() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    let err = put_page(
        &fx.repo,
        &fx.wiki(),
        "guess",
        "Guess",
        "no markers here",
        vec![],
        None,
    )
    .expect_err("the trust guarantee must not loosen for put");
    assert!(err.to_string().contains("no [[cite:path:Lx-Ly]] markers"));
}

#[test]
fn overlapping_evidence_is_advisory_and_linking_silences_it() {
    let fx = Fx::new(&[("src.rs", SRC), ("other.rs", "fn c() {}\n")]);
    loredex_engine::init_wiki_layout(&fx.wiki()).unwrap();
    // register the `note` type so the only finding left is the overlap
    std::fs::write(
        fx.wiki().join("wiki.toml"),
        "[wiki]\npages = \"pages\"\n\n[page_types.note]\nsections = [\"claim\"]\nchecklist = [\"what?\"]\n",
    )
    .unwrap();
    for id in ["note-a", "note-b"] {
        put_page(
            &fx.repo,
            &fx.wiki(),
            id,
            id,
            &format!("{id} describes the same code [[cite:src.rs:L1-L3]]."),
            vec![],
            Some("note".into()),
        )
        .unwrap();
    }
    let issues = lint_with(&fx.repo.view(), &fx.wiki()).unwrap();
    let overlap: Vec<_> = issues
        .iter()
        .filter(|i| i.kind == "overlapping-evidence")
        .collect();
    assert_eq!(overlap.len(), 1, "{issues:?}");
    assert!(overlap[0].advisory);
    assert!(overlap[0].detail.contains("note-a"));
    assert!(overlap[0].detail.contains("note-b"));

    // draft pages have nothing pinned to overlap with
    note_page(
        &fx.repo,
        &fx.wiki(),
        "note-c",
        "Uncited note",
        "wild guess about src.rs\n",
        vec![],
        Some("note".into()),
    )
    .unwrap();
    let after_draft = lint_with(&fx.repo.view(), &fx.wiki()).unwrap();
    assert_eq!(
        after_draft
            .iter()
            .filter(|i| i.kind == "overlapping-evidence")
            .count(),
        1
    );

    // a cross-link from either side declares the split intentional
    put_page(
        &fx.repo,
        &fx.wiki(),
        "note-b",
        "Note B",
        "note-b sees the same code [[cite:src.rs:L1-L3]], see [[wiki:note-a]]",
        vec![],
        Some("note".into()),
    )
    .unwrap();
    let issues = lint_with(&fx.repo.view(), &fx.wiki()).unwrap();
    let overlap: Vec<_> = issues
        .iter()
        .filter(|i| i.kind == "overlapping-evidence")
        .collect();
    assert!(overlap.is_empty(), "{overlap:?}");
    // advisory findings alone must not make `auto` report the wiki unclean;
    // note-c is a draft, so it stays as the single pending authoring task
    let auto = loredex_engine::auto_refresh(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        auto.lint_issues.iter().all(|i| i.advisory),
        "{:?}",
        auto.lint_issues
    );
}

#[test]
fn organize_clusters_pages_sharing_evidence() {
    let fx = Fx::new(&[("src.rs", SRC), ("other.rs", "fn c() {}\n")]);
    for (id, path) in [("core", "src.rs"), ("notes-on-core", "src.rs")] {
        put_page(
            &fx.repo,
            &fx.wiki(),
            id,
            id,
            &format!("claim about {id} [[cite:{path}:L1-L2]]."),
            vec![],
            Some("system".into()),
        )
        .unwrap();
    }
    put_page(
        &fx.repo,
        &fx.wiki(),
        "unrelated",
        "unrelated",
        "different file [[cite:other.rs:L1-L1]].",
        vec![],
        Some("system".into()),
    )
    .unwrap();

    let report = consolidation_clusters(&fx.wiki()).unwrap();
    assert_eq!(report.clusters.len(), 1, "{:?}", report.clusters);
    let c = &report.clusters[0];
    assert_eq!(c.pages, vec!["core", "notes-on-core"]);
    assert_eq!(c.shared_files, vec!["src.rs"]);
    assert!(c.suggestion.contains("core"));
    assert_eq!(report.pages_scanned, 3);
}

#[test]
fn organize_chains_pages_sharing_different_files() {
    // a(src) - b(src,other) - c(other): no file is cited by all three, but the
    // bridge makes them one maintenance cluster
    let fx = Fx::new(&[("src.rs", SRC), ("other.rs", "fn c() {}\n")]);
    for (id, cites) in [
        ("a", ["src.rs:L1-L2"].to_vec()),
        ("b", ["src.rs:L3-L4", "other.rs:L1-L1"].to_vec()),
        ("c", ["other.rs:L1-L1"].to_vec()),
    ] {
        let body: String = cites
            .iter()
            .map(|c| format!("{id} cites [[cite:{c}]]. "))
            .collect();
        put_page(&fx.repo, &fx.wiki(), id, id, &body, vec![], None).unwrap();
    }
    let report = consolidation_clusters(&fx.wiki()).unwrap();
    assert_eq!(report.clusters.len(), 1, "{:?}", report.clusters);
    let c = &report.clusters[0];
    assert_eq!(c.pages, vec!["a", "b", "c"]);
    assert_eq!(c.shared_files, vec!["other.rs", "src.rs"]);
}
