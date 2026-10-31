//! Tests for the generation plan, run ledger resume, hierarchical pages,
//! index generation and lint.

use git2::{Repository, Signature};
use tempfile::TempDir;
use wiki_core::Status;
use wiki_engine::{
    find_page, generate_index, lint, load_ledger, mark_task, open_plan, playbook_registry,
    put_page, refresh_page_with_drift, TaskKind, TaskStatus,
};
use wiki_git::GitRepo;

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
            git.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&p]).unwrap();
        }
        None => {
            git.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[]).unwrap();
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
    let task = active.tasks.iter().find(|t| t.page_id == new.page_id).unwrap();
    assert_eq!(task.status, TaskStatus::Done);
}

#[test]
fn plan_emits_refresh_task_after_stale() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    put(&fx, "root", "Root", None);
    fx.edit("src.rs", "// expanded docs\nfn a() {\n  b();\n}\n\nfn b() {}\n\nfn c() {}\n");
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
            run2
                .tasks
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
    let closed = wiki_engine::close_run(&fx.wiki()).unwrap();
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
    fx.edit("src.rs", "fn zz() {}\n// completely rewritten content now\n");
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
    assert_eq!(pb.sections, vec!["trigger".to_string(), "steps".to_string()]);
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
    assert!(!issues.iter().any(|i| i.kind == "unknown-type"), "{issues:?}");

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
    assert!(issues.iter().any(|i| i.kind == "unknown-type" && i.page == "weird"), "{issues:?}");
}

#[test]
fn generated_pages_excluded_from_plans() {
    let fx = Fx::new(&[("src.rs", SRC)]);
    put(&fx, "root", "Root", Some("system"));
    generate_index(&fx.repo, &fx.wiki()).unwrap();
    let run = open_plan(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        !run.tasks.iter().any(|t| t.page_id == "index" || t.page_id == "quickstart"),
        "generated pages must not spawn tasks: {:?}",
        run.tasks.iter().map(|t| &t.page_id).collect::<Vec<_>>()
    );
    // and lint must not complain about their uncited bodies
    let issues = lint(&fx.repo, &fx.wiki()).unwrap();
    assert!(
        !issues.iter().any(|i| (i.page == "index" || i.page == "quickstart")),
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
    let report = wiki_engine::auto_refresh(&fx.repo, &fx.wiki()).unwrap();
    assert_eq!(report.recovered.len(), 1, "{report:?}");
    assert!(report.needs_rewrite.is_empty());
    assert!(report.lint_issues.is_empty());
    assert!(report.all_clean, "projection should close everything: {:?}", report.plan.tasks);
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
    let report = wiki_engine::auto_refresh(&fx.repo, &fx.wiki()).unwrap();
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
