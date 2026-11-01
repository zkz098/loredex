//! Tests for the generation plan, run ledger resume, hierarchical pages,
//! index generation and lint.

use git2::{Repository, Signature};
use loredex_core::Status;
use loredex_engine::{
    find_page, generate_index, lint, load_ledger, mark_task, mark_tasks, open_plan,
    playbook_registry, put_page, refresh_page_with_drift, TaskKind, TaskStatus,
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
    let head = git.head().ok().map(|h| h.peel_to_commit().unwrap());
    match head {
        Some(p) => {
            git.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&p])
                .unwrap();
        }
        None => {
            git.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[])
                .unwrap();
        }
    }
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
        Self { tmp, repo }
    }
    fn wiki(&self) -> std::path::PathBuf {
        self.repo.wiki_dir()
    }
    fn edit(&self, path: &str, content: &str) {
        std::fs::write(self.tmp.path().join(path), content).unwrap();
        commit_all(self.tmp.path(), "edit");
    }
    fn remove(&self, path: &str) {
        std::fs::remove_file(self.tmp.path().join(path)).unwrap();
        commit_all(self.tmp.path(), "remove");
    }
}

const SRC: &str = "fn a() {\n  b();\n}\n\nfn b() {}\n";

fn put(fx: &Fx, id: &str, title: &str, ptype: Option<&str>) {
    put_page(
        &fx.repo,
        &fx.wiki(),
        id,
        title,
        &format!("# {title}\n\nsee [[cite:src.rs:L1-L5]]\n"),
        vec![],
        ptype.map(|s| s.to_string()),
    )
    .unwrap();
}

#[test]
fn plan_emits_new_task_for_uncovered_module_and_put_resolves_it() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    let run = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let new = run
        .tasks
        .iter()
        .find(|t| t.kind == TaskKind::New)
        .expect("uncovered src.rs must produce a New task");
    assert!(new.seeds.contains(&"src.rs".to_string()));

    // authoring with the suggested id auto-completes the task
    put(&fx, &new.page_id, "Root module", None);
    let ledger = load_ledger(&fx.wiki());
    let active = ledger.active_run().expect("run still active");
    let task = active
        .tasks
        .iter()
        .find(|t| t.page_id == new.page_id)
        .unwrap();
    assert_eq!(task.status, TaskStatus::Done);
}

#[test]
fn plan_emits_refresh_task_after_stale() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    put(&fx, "root", "Root", None);
    fx.edit(
        "src.rs",
        "// expanded docs\nfn a() {\n  b();\n}\n\nfn b() {}\n\nfn c() {}\n",
    );
    let run = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let refresh = run
        .tasks
        .iter()
        .find(|t| t.kind == TaskKind::Refresh && t.page_id == "root")
        .expect("stale page must produce Refresh task");
    assert!(refresh.purpose.contains("stale"));

    // resolving via refresh hook
    let mut page = find_page(&fx.wiki(), "root").unwrap();
    refresh_page_with_drift(&mut page, &fx.repo, &fx.wiki()).unwrap();
    let ledger = load_ledger(&fx.wiki());
    let task = ledger
        .active_run()
        .unwrap()
        .tasks
        .iter()
        .find(|t| t.page_id == "root")
        .unwrap();
    assert_eq!(task.status, TaskStatus::Done);
}

#[test]
fn interrupted_pending_tasks_carry_over_to_next_plan() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    let run1 = open_plan(&fx.repo, &fx.wiki()).unwrap();
    assert!(!run1.tasks.is_empty());
    // "interruption": nothing resolved, agent vanishes (openwiki's failure)

    let run2 = open_plan(&fx.repo, &fx.wiki()).unwrap();
    for t1 in &run1.tasks {
        assert!(
            run2.tasks
                .iter()
                .any(|t| t.page_id == t1.page_id && t.status == TaskStatus::Pending),
            "pending task {} must survive into next run",
            t1.page_id
        );
    }
    assert_eq!(run2.base_commit, run1.target_commit);
}

#[test]
fn mark_task_and_close_run_lifecycle() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    let run = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let pid = run.tasks[0].page_id.clone();
    mark_task(&fx.wiki(), &pid, TaskStatus::Skipped).unwrap();
    assert_eq!(
        load_ledger(&fx.wiki())
            .active_run()
            .unwrap()
            .tasks
            .iter()
            .find(|t| t.page_id == pid)
            .unwrap()
            .status,
        TaskStatus::Skipped
    );
    let closed = loredex_engine::close_run(&fx.wiki()).unwrap();
    assert_eq!(closed.as_deref(), Some(run.id.as_str()));
    assert!(load_ledger(&fx.wiki()).active_run().is_none());
}

#[test]
fn hierarchical_pages_and_index_generation() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    put(&fx, "architecture/core", "Core", Some("architecture"));
    assert!(fx.wiki().join("pages/architecture/core.md").is_file());

    let gen = generate_index(&fx.repo, &fx.wiki()).unwrap();
    let idx = gen.iter().find(|p| p.meta.id == "index").unwrap();
    assert!(idx.body.contains("[[wiki:architecture/core]]"));
    assert!(idx.body.contains("## architecture"));
    assert!(idx.body.contains("architecture")); // type shown
    let qs = gen.iter().find(|p| p.meta.id == "quickstart").unwrap();
    assert!(qs.body.contains("Recommended reading path"));
    assert!(qs.body.contains("### architecture"));
    assert!(qs.body.contains("[[wiki:architecture/core]]"));
    assert!(qs.body.contains("agent protocol"));

    // nested id traversal must be rejected
    assert!(put_page(
        &fx.repo,
        &fx.wiki(),
        "evil/../../../etc",
        "x",
        "[[cite:src.rs:L1-L1]]",
        vec![],
        None
    )
    .is_err());
    assert!(find_page(&fx.wiki(), "../outside").is_err());
}

#[test]
fn lint_finds_broken_links_stale_verified_and_drift() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    // page with broken wiki link + valid cite
    put_page(
        &fx.repo,
        &fx.wiki(),
        "root",
        "Root",
        "see [[cite:src.rs:L1-L5]] plus [[wiki:does/not-exist]]",
        vec![],
        None,
    )
    .unwrap();

    let issues = lint(&fx.repo, &fx.wiki()).unwrap();
    assert!(issues.iter().any(|i| i.kind == "broken-link"));

    // make it stale without marking -> stale-but-verified
    fx.edit(
        "src.rs",
        "fn zz() {}\n// completely rewritten content now\n",
    );
    let issues = lint(&fx.repo, &fx.wiki()).unwrap();
    assert!(issues.iter().any(|i| i.kind == "stale-but-verified"));

    // hand-edit body adding a marker with no evidence entry -> drift
    let page_file = fx.wiki().join("pages/root.md");
    let text = std::fs::read_to_string(&page_file).unwrap();
    std::fs::write(
        &page_file,
        text.replace("plus", "plus [[cite:ghost.rs:L1-L2]] and"),
    )
    .unwrap();
    // refresh state so live-stale check dominates; drift must still fire
    let mut page = find_page(&fx.wiki(), "root").unwrap();
    page.meta.status = Status::Verified;
    refresh_page_with_drift(&mut page, &fx.repo, &fx.wiki())
        // may go Draft (rewrite lost content) - that's fine, drift check is path based
        .ok();
    let issues = lint(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        issues.iter().any(|i| i.kind == "evidence-drift"),
        "{issues:?}"
    );
}

#[test]
fn plan_tasks_carry_playbook_instructions() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    let run = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let t = &run.tasks[0];
    assert!(!t.instructions.is_empty());
    assert!(!t.suggested_type.is_empty());
    // root dir group suggests architecture
    assert!(["architecture", "system"].contains(&t.suggested_type.as_str()));
    assert!(t.instructions.contains("Sections:"));
}

#[test]
fn wiki_toml_extends_playbook_registry() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    std::fs::create_dir_all(fx.wiki()).unwrap();
    std::fs::write(
        fx.wiki().join("wiki.toml"),
        "[page_types.runbook]\ndescription = \"ops runbook\"\nsections = [\"trigger\", \"steps\"]\nchecklist = [\"how to run?\"]\n",
    )
    .unwrap();
    let reg = playbook_registry(&fx.wiki());
    let pb = reg.get("runbook").expect("custom type from wiki.toml");
    assert_eq!(
        pb.sections,
        vec!["trigger".to_string(), "steps".to_string()]
    );
    assert!(reg.contains_key("architecture")); // builtins survive

    // unknown declared type now lints clean, known custom too
    put_page(
        &fx.repo,
        &fx.wiki(),
        "ops",
        "Ops",
        "[[cite:src.rs:L1-L2]]",
        vec![],
        Some("runbook".into()),
    )
    .unwrap();
    let issues = lint(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        !issues.iter().any(|i| i.kind == "unknown-type"),
        "{issues:?}"
    );

    // bogus type flagged
    put_page(
        &fx.repo,
        &fx.wiki(),
        "weird",
        "W",
        "[[cite:src.rs:L1-L2]]",
        vec![],
        Some("nonsense".into()),
    )
    .unwrap();
    let issues = lint(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        issues
            .iter()
            .any(|i| i.kind == "unknown-type" && i.page == "weird"),
        "{issues:?}"
    );
}

#[test]
fn generated_pages_excluded_from_plans() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    put(&fx, "root", "Root", Some("system"));
    generate_index(&fx.repo, &fx.wiki()).unwrap();
    let run = open_plan(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        !run.tasks
            .iter()
            .any(|t| t.page_id == "index" || t.page_id == "quickstart"),
        "generated pages must not spawn tasks: {:?}",
        run.tasks.iter().map(|t| &t.page_id).collect::<Vec<_>>()
    );
    // and lint must not complain about their uncited bodies
    let issues = lint(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        !issues
            .iter()
            .any(|i| i.page == "index" || i.page == "quickstart"),
        "{issues:?}"
    );
}

#[test]
fn auto_recovers_drift_and_reports_clean() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    put_page(
        &fx.repo,
        &fx.wiki(),
        "root",
        "Root",
        "behavior [[cite:src.rs:L2-L4]]",
        vec![],
        Some("system".into()),
    )
    .unwrap();
    fx.edit("src.rs", &format!("// inserted\n{SRC}"));
    let report = loredex_engine::auto_refresh(&fx.repo, &fx.wiki()).unwrap();
    assert_eq!(report.recovered.len(), 1, "{report:?}");
    assert!(report.needs_rewrite.is_empty());
    assert!(report.lint_issues.is_empty());
    assert!(
        report.all_clean,
        "projection should close everything: {:?}",
        report.plan.tasks
    );
}

#[test]
fn auto_routes_lost_content_to_rewrite_and_plan() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    put_page(
        &fx.repo,
        &fx.wiki(),
        "root",
        "Root",
        "behavior [[cite:src.rs:L2-L4]]",
        vec![],
        Some("system".into()),
    )
    .unwrap();
    fx.edit("src.rs", "fn nothing_like_before() {}\n");
    let report = loredex_engine::auto_refresh(&fx.repo, &fx.wiki()).unwrap();
    assert_eq!(report.needs_rewrite.len(), 1);
    assert!(!report.all_clean);
    // draft-aware planning: the now-draft page appears as a pending task
    assert!(report
        .plan
        .tasks
        .iter()
        .any(|t| t.page_id == "root" && t.status == TaskStatus::Pending));
    // and the run ledger persisted the same
    assert!(load_ledger(&fx.wiki()).active_run().is_some());
}

#[test]
fn auto_downgrades_deleted_citation_to_draft() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    put(&fx, "root", "Root", Some("system"));
    fx.remove("src.rs");

    let report = loredex_engine::auto_refresh(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        report
            .needs_rewrite
            .iter()
            .any(|item| item.page_id == "root"),
        "{report:?}"
    );
    assert!(!report.all_clean);

    // the page is no longer "verified + stale": it is persisted as draft with
    // its last-known evidence pin, so lint stops flagging stale-but-verified
    let page = find_page(&fx.wiki(), "root").unwrap();
    assert_eq!(page.meta.status, Status::Draft);
    assert!(!page.meta.evidence.is_empty());
    assert!(!report
        .lint_issues
        .iter()
        .any(|i| i.page == "root" && i.kind == "stale-but-verified"));
    assert!(report
        .plan
        .tasks
        .iter()
        .any(|t| t.page_id == "root" && t.status == TaskStatus::Pending));
}

#[test]
fn skipped_new_task_stays_suppressed_until_reset() {
    let fx = Fx::new(&[("src.rs", SRC), ("extra/helper.rs", "// helper\n")]);
    put(&fx, "root", "Root", Some("system")); // cite src.rs only
    let run1 = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let noise = run1
        .tasks
        .iter()
        .find(|t| t.kind == TaskKind::New && t.page_id != "root")
        .expect("second module uncovered");
    let noise_id = noise.page_id.clone();
    mark_task(&fx.wiki(), &noise_id, TaskStatus::Skipped).unwrap();

    let run2 = open_plan(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        !run2.tasks.iter().any(|t| t.page_id == noise_id),
        "skipped New task must not regrow: {:?}",
        run2.tasks.iter().map(|t| &t.page_id).collect::<Vec<_>>()
    );
    // reset re-exposes it
    let run3 = loredex_engine::plan_with(&fx.repo, &fx.wiki(), true).unwrap();
    assert!(run3.tasks.iter().any(|t| t.page_id == noise_id));
}

#[test]
fn batch_mark_resolves_multiple_pending_tasks() {
    let fx = Fx::new(&[("src.rs", SRC), ("docs/readme.md", "hello\n")]);
    let run = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let ids: Vec<String> = run.tasks.iter().map(|t| t.page_id.clone()).collect();
    assert!(ids.len() >= 2, "expected several modules: {ids:?}");

    mark_tasks(
        &fx.wiki(),
        ids.iter().map(String::as_str),
        TaskStatus::Skipped,
    )
    .unwrap();
    let ledger = load_ledger(&fx.wiki());
    let active = ledger.active_run().expect("run stays active");
    for id in &ids {
        assert_eq!(
            active
                .tasks
                .iter()
                .find(|t| &t.page_id == id)
                .expect("task present")
                .status,
            TaskStatus::Skipped
        );
    }
    // an empty batch is a no-op
    mark_tasks(&fx.wiki(), std::iter::empty(), TaskStatus::Done).unwrap();
}

#[test]
fn superseded_runs_keep_resolution_but_drop_payload() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    let run1 = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let new = run1
        .tasks
        .iter()
        .find(|t| t.kind == TaskKind::New)
        .expect("uncovered module");
    let pid = new.page_id.clone();
    assert!(!new.instructions.is_empty());
    put(&fx, &pid, "Root", None); // resolves the New task

    let _run2 = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let ledger = load_ledger(&fx.wiki());
    let superseded = ledger
        .runs
        .iter()
        .find(|r| r.id == run1.id)
        .expect("previous run kept");
    let t = superseded
        .tasks
        .iter()
        .find(|t| t.page_id == pid)
        .expect("task kept");
    assert_eq!(t.status, TaskStatus::Done);
    assert_eq!(t.kind, TaskKind::New);
    assert!(
        t.instructions.is_empty() && t.seeds.is_empty() && t.purpose.is_empty(),
        "resolved payload should be compacted: {t:?}"
    );
}

#[test]
fn auto_resolves_recovered_refresh_tasks() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    put_page(
        &fx.repo,
        &fx.wiki(),
        "root",
        "Root",
        "behavior [[cite:src.rs:L2-L4]]",
        vec![],
        Some("system".into()),
    )
    .unwrap();
    fx.edit("src.rs", &format!("// inserted\n{SRC}"));
    open_plan(&fx.repo, &fx.wiki()).unwrap(); // pending Refresh task

    let report = loredex_engine::auto_refresh(&fx.repo, &fx.wiki()).unwrap();
    assert!(report.recovered.iter().any(|r| r.page_id == "root"));
    let ledger = load_ledger(&fx.wiki());
    assert!(
        ledger
            .runs
            .iter()
            .flat_map(|r| r.tasks.iter())
            .any(|t| t.page_id == "root"
                && t.kind == TaskKind::Refresh
                && t.status == TaskStatus::Done),
        "auto must resolve the recovered page's task in its batch mark"
    );
}

#[test]
fn carried_new_task_retires_when_another_page_covers_its_seeds() {
    // A New task claims "this module is undocumented", not "a page with the
    // suggested id must exist": authoring under any id closes it, otherwise the
    // worklist resurrects on every run and `auto` can never report clean.
    let fx = Fx::new(&[("src.rs", SRC)]);
    let run1 = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let new = run1
        .tasks
        .iter()
        .find(|t| t.kind == TaskKind::New)
        .expect("uncovered src.rs");
    let suggested = new.page_id.clone();
    assert!(new.seeds.contains(&"src.rs".to_string()));

    put_page(
        &fx.repo,
        &fx.wiki(),
        "notes-on-src",
        "Notes on src",
        "src.rs holds two functions [[cite:src.rs:L1-L5]]",
        vec![],
        Some("system".into()),
    )
    .unwrap();

    let run2 = open_plan(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        !run2.tasks.iter().any(|t| t.page_id == suggested),
        "covered module must not carry over: {:?}",
        run2.tasks.iter().map(|t| &t.page_id).collect::<Vec<_>>()
    );
}

#[test]
fn carried_new_task_retires_on_the_ratio_that_created_it() {
    // seeds are the uncovered files at creation time; documenting most of them
    // drops the module below the emit threshold, so the carried task must go too
    let fx = Fx::new(&[
        ("pkg/a.rs", "fn a() {}\n"),
        ("pkg/b.rs", "fn b() {}\n"),
        ("pkg/c.rs", "fn c() {}\n"),
        ("pkg/d.rs", "fn d() {}\n"),
    ]);
    let run1 = open_plan(&fx.repo, &fx.wiki()).unwrap();
    let new = run1
        .tasks
        .iter()
        .find(|t| t.kind == TaskKind::New && t.page_id == "pkg")
        .expect("four uncovered files must plan one module task");
    assert_eq!(new.seeds.len(), 4);

    put_page(
        &fx.repo,
        &fx.wiki(),
        "pkg-notes",
        "Package notes",
        "[[cite:pkg/a.rs:L1-L1]] [[cite:pkg/b.rs:L1-L1]] [[cite:pkg/c.rs:L1-L1]]",
        vec![],
        Some("system".into()),
    )
    .unwrap();

    let run2 = open_plan(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        !run2.tasks.iter().any(|t| t.page_id == "pkg"),
        "1 of 4 uncovered is below the threshold: {:?}",
        run2.tasks.iter().map(|t| &t.page_id).collect::<Vec<_>>()
    );
}
