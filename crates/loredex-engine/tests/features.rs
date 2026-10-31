//! Feature-level integration tests for the three additions:
//! skeleton language setting, AGENTS.md managed block, openwiki import.

use git2::{Repository, Signature};
use loredex_core::Status;
use loredex_engine::{
    agents_block_check, agents_block_remove, agents_block_upsert, compute_tasks, generate_index,
    import_openwiki, load_pages, playbook_registry, set_language, wiki_locale, AgentsMdStatus,
    UpsertAction,
};
use loredex_git::GitRepo;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Fixture {
    tmp: TempDir,
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
        Self { tmp, repo }
    }

    fn wiki(&self) -> PathBuf {
        self.repo.wiki_dir()
    }
}

// ---------- language setting ----------

#[test]
fn language_setting_switches_skeleton_text() {
    let fx = Fixture::new(&[("src/a.rs", "fn a() {}\nfn b() {}\n")]);
    assert_eq!(wiki_locale(&fx.wiki()), loredex_engine::Locale::En);

    set_language(&fx.wiki(), "zh").unwrap();
    assert_eq!(wiki_locale(&fx.wiki()), loredex_engine::Locale::Zh);

    generate_index(&fx.repo, &fx.wiki()).unwrap();
    let pages = load_pages(&fx.wiki()).unwrap();
    let qs = pages.iter().find(|p| p.meta.id == "quickstart").unwrap();
    assert!(qs.body.contains("推荐阅读路径"), "quickstart: {}", qs.body);
    assert_eq!(qs.meta.title, "Wiki 快速入口");
    let idx = pages.iter().find(|p| p.meta.id == "index").unwrap();
    assert!(idx.body.contains("仓库 Wiki 索引"));

    // playbooks follow the locale too
    let reg = playbook_registry(&fx.wiki());
    assert!(reg["system"].description.contains("运行时"));
    let instr = reg["system"].instructions_for("core", loredex_engine::Locale::Zh);
    assert!(instr.contains("撰写"));

    // set_language is idempotent and preserves other sections
    std::fs::write(
        fx.wiki().join("wiki.toml"),
        "# keep me\n[wiki]\nlanguage = \"en\"\n[page_types.devlog]\ndescription = \"x\"\n",
    )
    .unwrap();
    set_language(&fx.wiki(), "zh").unwrap();
    let text = std::fs::read_to_string(fx.wiki().join("wiki.toml")).unwrap();
    assert!(text.contains("# keep me"));
    assert!(text.contains("[page_types.devlog]"));
    assert_eq!(text.matches("language =").count(), 1, "{text}");
    assert_eq!(wiki_locale(&fx.wiki()), loredex_engine::Locale::Zh);
}

// ---------- AGENTS.md managed block ----------

#[test]
fn agents_block_respects_language() {
    let fx = Fixture::new(&[("src/a.rs", "fn a() {}\n")]);
    let agents = fx.tmp.path().join("AGENTS.md");
    set_language(&fx.wiki(), "zh").unwrap();
    let locale = wiki_locale(&fx.wiki());
    assert_eq!(
        agents_block_upsert(&agents, locale).unwrap(),
        UpsertAction::Created
    );
    let text = std::fs::read_to_string(&agents).unwrap();
    assert!(text.contains("## 仓库 Wiki(loredex)"));
    assert_eq!(
        agents_block_check(&agents, locale).unwrap(),
        AgentsMdStatus::InPlace
    );
    assert!(agents_block_remove(&agents).unwrap());
}

// ---------- openwiki import ----------

const ARCH_MD: &str = "---\ntype: architecture\ntitle: System architecture\ntags: [core]\n---\n# System architecture\n\nThe API layer is covered in [the core module](modules/core.md).\n";
const CORE_MD: &str =
    "---\ntype: devlog\ntitle: Core module\n---\nCore serves requests [[cite:src/api.rs:L1-L5]].\n";
const UNCITED_MD: &str = "# Notes\n\nProse without any evidence markers.\n";

fn bundle(dir: &Path) {
    for (rel, content) in [
        ("architecture.md", ARCH_MD),
        ("modules/core.md", CORE_MD),
        ("notes.md", UNCITED_MD),
        ("index.md", "# bundle index\n"),
        ("log.md", "# changelog\n"),
        ("INSTRUCTIONS.md", "build instructions\n"),
        (".claims/claim1.json", "{}"),
    ] {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
}

#[test]
fn import_openwiki_lands_drafts_pins_cites_and_rewrites_links() {
    let fx = Fixture::new(&[(
        "src/api.rs",
        "pub fn serve() {\n    loop {\n        handle();\n    }\n}\n",
    )]);
    let src = TempDir::new().unwrap();
    bundle(src.path());

    let rep = import_openwiki(&fx.repo, &fx.wiki(), src.path(), None, false, false).unwrap();
    assert_eq!(rep.imported.len(), 3, "{:?}", rep.imported);
    assert!(rep.registered_types.contains(&"devlog".to_string()));

    let pages = load_pages(&fx.wiki()).unwrap();
    let get = |id: &str| pages.iter().find(|p| p.meta.id == id).unwrap();

    // cited page pins evidence and becomes verified
    let core = get("modules/core");
    assert_eq!(core.meta.status, Status::Verified);
    assert_eq!(core.meta.evidence.len(), 1);
    assert_eq!(core.meta.page_type.as_deref(), Some("devlog"));

    // uncited pages land as drafts, tagged imported
    let arch = get("architecture");
    assert_eq!(arch.meta.status, Status::Draft);
    assert!(arch.meta.tags.contains(&"imported".to_string()));
    // cross-page markdown link rewritten to a wiki link
    assert!(arch.body.contains("[[wiki:modules/core]]"), "{}", arch.body);
    let notes = get("notes");
    assert_eq!(notes.meta.status, Status::Draft);
    assert_eq!(notes.meta.title, "Notes"); // from the leading H1

    // reserved files skipped
    assert!(load_pages(&fx.wiki())
        .unwrap()
        .iter()
        .all(|p| p.meta.id != "index" || p.meta.tags.contains(&"generated".to_string())));
    assert_eq!(rep.skipped.len(), 2, "{:?}", rep.skipped);

    // imported types are lint-clean (registered via wiki.toml stubs)
    let reg = playbook_registry(&fx.wiki());
    assert!(reg.contains_key("devlog"));

    // drafts become plan work for citation backfill
    let tasks = compute_tasks(&fx.repo, &fx.wiki()).unwrap();
    let t = tasks.iter().find(|t| t.page_id == "architecture").unwrap();
    assert!(t.purpose.contains("draft"));

    // re-import without --force skips existing pages
    let rep2 = import_openwiki(&fx.repo, &fx.wiki(), src.path(), None, false, false).unwrap();
    assert_eq!(rep2.imported.len(), 0);
    assert_eq!(rep2.skipped.len(), 5); // 3 existing + 2 reserved

    // --force overwrites, --dry_run writes nothing
    let rep3 = import_openwiki(&fx.repo, &fx.wiki(), src.path(), None, false, true).unwrap();
    assert_eq!(rep3.imported.len(), 3);
    assert!(rep3.imported.iter().all(|p| p.overwritten));
}

#[test]
fn import_prefix_and_dry_run() {
    let fx = Fixture::new(&[(
        "src/api.rs",
        "pub fn serve() {\n    loop {\n        handle();\n    }\n}\n",
    )]);
    let src = TempDir::new().unwrap();
    bundle(src.path());

    let rep = import_openwiki(
        &fx.repo,
        &fx.wiki(),
        src.path(),
        Some("openwiki"),
        true,
        false,
    )
    .unwrap();
    assert_eq!(rep.imported.len(), 3);
    assert!(rep.imported.iter().any(|p| p.id == "openwiki/modules/core"));
    // dry run: nothing written, wiki.toml untouched
    assert!(load_pages(&fx.wiki()).unwrap().is_empty());
    assert!(!playbook_registry(&fx.wiki()).contains_key("devlog"));

    // broken source path surfaces as an error, not a panic
    let err = import_openwiki(
        &fx.repo,
        &fx.wiki(),
        &fx.tmp.path().join("nowhere"),
        None,
        false,
        false,
    );
    assert!(err.is_err());
}
