//! Integration tests: git-fixture driven wiki lifecycle.
//! Replicates the manually verified dogfood flows so they become regression
//! coverage: put(pins evidence) -> code change(stale) -> drift projection /
//! lost downgrade -> verification outcomes, plus the put_page rejection matrix.

use git2::{Repository, Signature};
use tempfile::TempDir;
use wiki_core::{Status, WikiPage};
use wiki_engine::{
    check_page, discover_modules, find_page, put_page, refresh_page_with_drift,
    verify_page, DriftOutcome,
};
use wiki_git::GitRepo;

struct Fixture {
    tmp: TempDir,
    repo: GitRepo,
}

fn commit_all(repo_path: &std::path::Path, msg: &str) {
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
            git.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&p]).unwrap();
        }
        None => {
            git.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[]).unwrap();
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
        commit_all(tmp.path(), "initial");
        let repo = GitRepo::discover(tmp.path()).expect("discover");
        Self { tmp, repo }
    }

    fn rewrite(&self, path: &str, content: &str) {
        std::fs::write(self.tmp.path().join(path), content).unwrap();
        commit_all(self.tmp.path(), "edit");
    }

    fn remove(&self, path: &str) {
        std::fs::remove_file(self.tmp.path().join(path)).unwrap();
        commit_all(self.tmp.path(), "delete");
    }

    fn mv(&self, from: &str, to: &str) {
        let f = self.tmp.path().join(from);
        let t = self.tmp.path().join(to);
        std::fs::create_dir_all(t.parent().unwrap()).unwrap();
        std::fs::rename(f, t).unwrap();
        commit_all(self.tmp.path(), "mv");
    }

    fn commit_file(&self, path: &str, content: &str) {
        let p = self.tmp.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
        commit_all(self.tmp.path(), "add");
    }

    fn wiki_dir(&self) -> std::path::PathBuf {
        self.repo.wiki_dir()
    }
}

const API_RS: &str = "pub fn serve() {\n    loop {\n        handle();\n    }\n}\n\nfn handle() {}\n";

fn cited_body() -> String {
    "# Entry\n\nserve loops over handle [[cite:src/api.rs:L1-L5]].\n\
     handle is a stub [[cite:src/api.rs:L7-L7]].\n"
        .into()
}

fn make_page(fx: &Fixture) -> WikiPage {
    let out = put_page(
        &fx.repo,
        &fx.wiki_dir(),
        "entry",
        "Entry & API",
        &cited_body(),
        vec![],
    )
    .expect("put_page");
    assert!(out.created);
    assert_eq!(out.citations, 2);
    find_page(&fx.wiki_dir(), "entry").expect("page saved")
}

// ---------- happy path: pin, verify, check ----------

#[test]
fn put_pins_evidence_and_stays_fresh() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    let page = make_page(&fx);
    assert_eq!(page.meta.status, Status::Verified);
    assert!(!check_page(&page, &fx.repo).unwrap().is_stale);
    let verify = verify_page(&page, &fx.repo).unwrap();
    assert!(verify.all_ok, "snippet hashes must match: {verify:?}");
}

// ---------- line insertion: stale -> projection keeps verified ----------

#[test]
fn drift_projection_adopts_shifted_lines() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    make_page(&fx);

    fx.rewrite("src/api.rs", &format!("// hdr1\n// hdr2\n{API_RS}"));
    let stale = find_page(&fx.wiki_dir(), "entry").unwrap();
    let report = check_page(&stale, &fx.repo).unwrap();
    assert!(report.is_stale);
    assert!(report
        .evidence_reports
        .iter()
        .any(|r| !r.ok && r.detail.contains("blob changed")));

    let mut page = stale.clone();
    let outcomes = refresh_page_with_drift(&mut page, &fx.repo).unwrap();
    assert!(outcomes
        .iter()
        .all(|(_, o)| matches!(o, DriftOutcome::Projected { score, .. } if *score == 1.0)));
    assert_eq!(page.meta.status, Status::Verified);

    // persisted: note records the projection, body markers untouched
    let reloaded = find_page(&fx.wiki_dir(), "entry").unwrap();
    assert!(reloaded
        .meta
        .evidence
        .iter()
        .any(|e| e.note.as_deref().unwrap_or("").contains("drift-projected")));
    assert!(reloaded.body.contains("[[cite:src/api.rs:L1-L5]]"));
    assert!(verify_page(&reloaded, &fx.repo).unwrap().all_ok);
    assert!(!check_page(&reloaded, &fx.repo).unwrap().is_stale);

    // exact shift: L1-L5 -> L3-L7
    assert_eq!(reloaded.meta.evidence[0].lines, Some([3, 7]));
}

// ---------- content loss: refresh falls back to draft ----------

#[test]
fn lost_snippet_downgrades_to_draft() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    make_page(&fx);

    fx.rewrite("src/api.rs", "pub fn serve() {}\n");
    let stale = find_page(&fx.wiki_dir(), "entry").unwrap();
    assert!(check_page(&stale, &fx.repo).unwrap().is_stale);
    assert!(
        !verify_page(&stale, &fx.repo).unwrap().all_ok,
        "snippet hash must fail after content loss"
    );

    let mut page = stale;
    let outcomes = refresh_page_with_drift(&mut page, &fx.repo).unwrap();
    assert!(
        outcomes
            .iter()
            .any(|(_, o)| matches!(o, DriftOutcome::Lost)),
        "total rewrite must include Lost, got {outcomes:?}"
    );
    assert_eq!(page.meta.status, Status::Draft);
    assert!(!page.meta.evidence.is_empty());
}

// ---------- deleted evidence file ----------

#[test]
fn deleted_evidence_file_reports_stale() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    make_page(&fx);
    fx.remove("src/api.rs");

    let page = find_page(&fx.wiki_dir(), "entry").unwrap();
    let report = check_page(&page, &fx.repo).unwrap();
    assert!(report.is_stale);
    assert!(report
        .evidence_reports
        .iter()
        .any(|r| !r.ok && r.detail.contains("no longer exists")));
    assert!(refresh_page_with_drift(&mut page.clone(), &fx.repo).is_err());
}

// ---------- unknown generation commit ----------

#[test]
fn unknown_base_commit_is_stale() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    let mut page = make_page(&fx);
    page.meta.commit = "0".repeat(40);
    let report = check_page(&page, &fx.repo).unwrap();
    assert!(report.is_stale);
    assert!(report
        .evidence_reports
        .iter()
        .any(|r| r.detail.contains("unknown commit")));
}

// ---------- module discovery ----------

#[test]
fn modules_dir_fallback_for_non_cargo_repo() {
    let fx = Fixture::new(&[("src/api.rs", API_RS), ("docs/readme.md", "hi\n")]);
    let mods = discover_modules(&fx.repo).unwrap();
    assert!(mods.iter().all(|m| m.kind == "dir"));
    let src = mods.iter().find(|m| m.name == "src").expect("src group");
    assert_eq!(src.files, vec!["src/api.rs".to_string()]);
    assert!(mods.iter().any(|m| m.name == "docs"));
}

#[test]
fn modules_cargo_targets_discovered() {
    let fx = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        ),
        ("src/lib.rs", "pub fn g() {}\n"),
    ]);
    let mods = discover_modules(&fx.repo).unwrap();
    let lib = mods
        .iter()
        .find(|m| m.kind.starts_with("rust-") && m.files.contains(&"src/lib.rs".to_string()))
        .expect("cargo target should cover src/lib.rs");
    assert_eq!(lib.name, "probe/probe");
    assert_eq!(lib.kind, "rust-lib");
}

// ---------- rename recovery ----------

#[test]
fn plain_git_mv_recovered_by_content_addressing() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    make_page(&fx);
    fx.mv("src/api.rs", "src/store/api.rs");

    let stale = find_page(&fx.wiki_dir(), "entry").unwrap();
    let report = check_page(&stale, &fx.repo).unwrap();
    assert!(report.is_stale);
    assert!(report
        .evidence_reports
        .iter()
        .any(|r| r.detail.contains("renamed to src/store/api.rs")
            && r.detail.contains("identical blob")));

    let mut page = stale;
    let outcomes = refresh_page_with_drift(&mut page, &fx.repo).unwrap();
    assert!(outcomes
        .iter()
        .all(|(_, o)| matches!(o, DriftOutcome::Renamed { new_path, exact: true }
            if new_path == "src/store/api.rs")));
    assert_eq!(page.meta.status, Status::Verified);
    assert!(page.meta.evidence.iter().all(|e| e.path == "src/store/api.rs"));
    assert!(verify_page(&find_page(&fx.wiki_dir(), "entry").unwrap(), &fx.repo)
        .unwrap()
        .all_ok);
}

#[test]
fn mv_with_minor_edit_renamed_and_projected() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    make_page(&fx);
    fx.mv("src/api.rs", "moved_api.rs");
    // one-line tweak at the top after the move (below the git rename threshold work)
    fx.rewrite("moved_api.rs", &format!("// touched\n{API_RS}"));

    let stale = find_page(&fx.wiki_dir(), "entry").unwrap();
    assert!(check_page(&stale, &fx.repo).unwrap().is_stale);

    let mut page = stale;
    let outcomes = refresh_page_with_drift(&mut page, &fx.repo).unwrap();
    // tier-1 exact fails (blob changed); similarity rename + projection wins
    assert!(outcomes.iter().all(|(_, o)| matches!(
        o,
        DriftOutcome::Renamed { new_path, exact: false } if new_path == "moved_api.rs"
    )) || outcomes.iter().any(|(_, o)| matches!(o, DriftOutcome::Projected { .. })),
        "{outcomes:?}");
    assert_eq!(page.meta.status, Status::Verified);
    assert!(page.meta.evidence.iter().all(|e| e.path == "moved_api.rs"));
    // first citation shifted down by the inserted header line
    assert_eq!(page.meta.evidence[0].lines, Some([2, 6]));
    assert!(verify_page(&find_page(&fx.wiki_dir(), "entry").unwrap(), &fx.repo)
        .unwrap()
        .all_ok);
}

#[test]
fn mv_with_minor_edit_then_total_rewrite_recovers_at_move_target() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    make_page(&fx);
    // step 1: plain mv -> tier-1 content-addressed recovery re-points path
    fx.mv("src/api.rs", "new/api.rs");
    let mut page = find_page(&fx.wiki_dir(), "entry").unwrap();
    refresh_page_with_drift(&mut page, &fx.repo).unwrap();
    assert_eq!(page.meta.status, Status::Verified);
    assert!(page.meta.evidence.iter().all(|e| e.path == "new/api.rs"));

    // step 2: total rewrite at the new path -> Lost, draft, but path anchors hold
    fx.rewrite("new/api.rs", "pub fn totally_different() {}\n");
    let mut page = find_page(&fx.wiki_dir(), "entry").unwrap();
    let outcomes = refresh_page_with_drift(&mut page, &fx.repo).unwrap();
    assert!(
        outcomes.iter().any(|(_, o)| *o == DriftOutcome::Lost),
        "{outcomes:?}"
    );
    assert_eq!(page.meta.status, Status::Draft);
    assert!(page.meta.evidence.iter().all(|e| e.path == "new/api.rs"));
}

#[test]
fn heavy_rewrite_without_rename_hint_stays_unresolvable() {
    // documented boundary: mv + complete rewrite in ONE commit is neither
    // content-identical nor similar enough for git rename detection; refresh
    // must error rather than silently mis-adopt a path
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    make_page(&fx);
    {
        let f = fx.tmp.path().join("src/api.rs");
        std::fs::remove_file(f).unwrap();
    }
    fx.commit_file("new/api.rs", "pub fn totally_different() {}\n");
    let page = find_page(&fx.wiki_dir(), "entry").unwrap();
    assert!(refresh_page_with_drift(&mut page.clone(), &fx.repo).is_err());
}

// ---------- put rejection matrix ----------

#[test]
fn reject_body_without_citations() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    let err = put_page(&fx.repo, &fx.wiki_dir(), "x", "t", "plain prose only", vec![])
        .expect_err("must reject uncited body");
    assert!(err.to_string().contains("no [[cite:path:Lx-Ly]] markers"));
}

#[test]
fn reject_citation_to_untracked_path() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    std::fs::write(fx.tmp.path().join("draft.rs"), "x\n").unwrap();
    let err = put_page(
        &fx.repo,
        &fx.wiki_dir(),
        "x",
        "t",
        "[[cite:draft.rs:L1-L1]]",
        vec![],
    )
    .expect_err("uncommitted file cannot be cited");
    assert!(err.to_string().contains("not tracked at HEAD"));
}

#[test]
fn reject_malformed_citation_syntax() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    for (body, expect) in [
        ("[[cite:src/api.rs:L0-L3]]", "invalid line range"),
        ("[[cite:src/api.rs:L5-L2]]", "invalid line range"),
        // non-numeric range does not match the range group; path portion becomes
        // the whole token and fails pinning instead
        ("[[cite:src/api.rs:Lx-L2]]", "cannot be pinned"),
    ] {
        let err = put_page(&fx.repo, &fx.wiki_dir(), "x", "t", body, vec![])
            .expect_err("must reject malformed citation");
        assert!(err.to_string().contains(expect), "{body} -> {err}");
    }
}

#[test]
fn reject_invalid_page_ids() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    for id in ["", "with space", "path/../trick", "sl/ash"] {
        let err = put_page(
            &fx.repo,
            &fx.wiki_dir(),
            id,
            "t",
            "[[cite:src/api.rs:L1-L1]]",
            vec![],
        )
        .expect_err("invalid id must be rejected");
        assert!(err.to_string().contains("invalid page id"), "{id}");
    }
}

#[test]
fn put_updates_existing_page_and_dedups_citations() {
    let fx = Fixture::new(&[("src/api.rs", API_RS)]);
    make_page(&fx);
    let body = "# v2\n\nsame thing [[cite:src/api.rs:L1-L5]] and again [[cite:src/api.rs:L1-L5]].\n";
    let out = put_page(&fx.repo, &fx.wiki_dir(), "entry", "Entry v2", body, vec![]).unwrap();
    assert!(!out.created, "second put updates in place");
    assert_eq!(out.citations, 1, "duplicate citations dedup");

    let page = find_page(&fx.wiki_dir(), "entry").unwrap();
    assert_eq!(page.meta.title, "Entry v2");
    assert_eq!(page.meta.evidence.len(), 1);
    assert_eq!(page.meta.status, Status::Verified);
}
