//! Multi-agent concurrency: optimistic page writes (`if_match` / `if_absent`)
//! and leased plan tasks.

use std::collections::BTreeSet;

use git2::{Repository, Signature};
use loredex_core::WikiError;
use loredex_engine::{
    claim_tasks, init_wiki_layout, load_ledger, mark_task, open_plan, page_sha1, put_page,
    put_page_with, PageExpectation, PutOptions, TaskStatus,
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
        init_wiki_layout(&repo.wiki_dir()).unwrap();
        Self { _tmp: tmp, repo }
    }
    fn wiki(&self) -> std::path::PathBuf {
        self.repo.wiki_dir()
    }
}

const SRC: &str = "fn a() {\n  b();\n}\n\nfn b() {}\n";
const BODY: &str = "# page\n\nsee [[cite:src.rs:L1-L5]]\n";

fn put(
    fx: &Fx,
    id: &str,
    body: &str,
    expected: Option<PageExpectation>,
) -> loredex_engine::Result<()> {
    let view = fx.repo.view();
    let opts = PutOptions {
        expected,
        ..Default::default()
    };
    put_page_with(&view, &fx.wiki(), id, id, body, vec![], None, &opts)?;
    Ok(())
}

#[test]
fn if_match_rejects_a_lost_update() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    let first = put_page(&fx.repo, &fx.wiki(), "a", "A", BODY, vec![], None).unwrap();
    assert!(first.created);
    assert_eq!(first.page_sha1.len(), 40);
    let stale = first.page_sha1.clone();

    // Another agent (or a human) wins the race with different content.
    let second = put_page(&fx.repo, &fx.wiki(), "a", "A (agent 2)", BODY, vec![], None).unwrap();
    assert!(!second.created);
    assert_ne!(second.page_sha1, stale);

    let err = put(&fx, "a", BODY, Some(PageExpectation::Sha1(stale))).unwrap_err();
    assert!(
        matches!(&err, WikiError::Conflict(msg) if msg.contains("changed since you read it")),
        "{err}"
    );

    let current = page_sha1(&fx.wiki().join("pages/a.md")).unwrap().unwrap();
    let third = put_page_with(
        &fx.repo.view(),
        &fx.wiki(),
        "a",
        "A",
        BODY,
        vec![],
        None,
        &PutOptions {
            expected: Some(PageExpectation::Sha1(current.clone())),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!third.created);
    assert_ne!(third.page_sha1, current);
}

#[test]
fn if_absent_guards_creation() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    put(&fx, "a", BODY, Some(PageExpectation::Absent)).unwrap();
    let err = put(&fx, "a", BODY, Some(PageExpectation::Absent)).unwrap_err();
    assert!(
        matches!(&err, WikiError::Conflict(msg) if msg.contains("already exists")),
        "{err}"
    );
}

#[test]
fn claims_are_exclusive_and_expire() {
    let fx = Fx::new(&[
        ("alpha/a.rs", "fn a() {}\n"),
        ("beta/b.rs", "fn b() {}\n"),
        ("gamma/c.rs", "fn c() {}\n"),
        ("delta/d.rs", "fn d() {}\n"),
    ]);
    let run = open_plan(&fx.repo, &fx.wiki()).unwrap();
    assert!(run.tasks.len() >= 2, "{:?}", run.tasks);

    let a = claim_tasks(&fx.wiki(), "agent-a", 1, 1800).unwrap();
    assert_eq!(a.len(), 1);
    let rest = claim_tasks(&fx.wiki(), "agent-b", 64, 1800).unwrap();
    assert!(
        rest.iter().all(|t| t.page_id != a[0].page_id),
        "agent-b also received {}",
        a[0].page_id
    );
    assert_eq!(a[0].claimed_by.as_deref(), Some("agent-a"));
    assert!(rest
        .iter()
        .all(|t| t.claimed_by.as_deref() == Some("agent-b")));

    // Resolving the task releases its lease.
    mark_task(&fx.wiki(), &a[0].page_id, TaskStatus::Skipped).unwrap();
    let ledger = load_ledger(&fx.wiki());
    let task = ledger
        .active_run()
        .unwrap()
        .tasks
        .iter()
        .find(|t| t.page_id == a[0].page_id)
        .unwrap();
    assert!(task.claimed_by.is_none());
    assert!(task.lease_until.is_none());
}

#[test]
fn an_expired_lease_can_be_reclaimed() {
    let fx = Fx::new(&[("alpha/a.rs", "fn a() {}\n"), ("beta/b.rs", "fn b() {}\n")]);
    open_plan(&fx.repo, &fx.wiki()).unwrap();
    let first = claim_tasks(&fx.wiki(), "agent-a", 1, 0).unwrap();
    assert_eq!(first.len(), 1);
    let second = claim_tasks(&fx.wiki(), "agent-b", 1, 1800).unwrap();
    assert_eq!(second[0].page_id, first[0].page_id);
    assert_eq!(second[0].claimed_by.as_deref(), Some("agent-b"));
}

#[test]
fn re_planning_preserves_existing_claims() {
    let fx = Fx::new(&[("alpha/a.rs", "fn a() {}\n"), ("beta/b.rs", "fn b() {}\n")]);
    open_plan(&fx.repo, &fx.wiki()).unwrap();
    let claimed = claim_tasks(&fx.wiki(), "agent-a", 1, 1800).unwrap();
    let page_id = claimed[0].page_id.clone();

    // A second agent opens/refreshes the plan while the lease is live.
    open_plan(&fx.repo, &fx.wiki()).unwrap();
    let ledger = load_ledger(&fx.wiki());
    let task = ledger
        .active_run()
        .unwrap()
        .tasks
        .iter()
        .find(|t| t.page_id == page_id)
        .unwrap();
    assert_eq!(task.claimed_by.as_deref(), Some("agent-a"));
    assert!(task.lease_until.is_some());
}

#[test]
fn concurrent_claims_are_serialized_by_the_ledger_lock() {
    let fx = Fx::new(&[
        ("alpha/a.rs", "fn a() {}\n"),
        ("beta/b.rs", "fn b() {}\n"),
        ("gamma/c.rs", "fn c() {}\n"),
        ("delta/d.rs", "fn d() {}\n"),
        ("epsilon/e.rs", "fn e() {}\n"),
        ("zeta/f.rs", "fn f() {}\n"),
    ]);
    open_plan(&fx.repo, &fx.wiki()).unwrap();
    let handles: Vec<_> = (0..6)
        .map(|i| {
            let wiki = fx.wiki();
            std::thread::spawn(move || claim_tasks(&wiki, &format!("agent-{i}"), 1, 60).unwrap())
        })
        .collect();
    let mut ids = BTreeSet::new();
    for h in handles {
        for task in h.join().unwrap() {
            assert!(
                ids.insert(task.page_id.clone()),
                "duplicate claim: {}",
                task.page_id
            );
        }
    }
    assert_eq!(ids.len(), 6);
}
