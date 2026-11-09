//! Feature-level integration tests for the three additions:
//! skeleton language setting, AGENTS.md managed block, openwiki import.

use git2::{Repository, Signature};
use loredex_core::Status;
use loredex_engine::{
    agents_block_check, agents_block_remove, agents_block_upsert, build_html, compute_tasks,
    evidence_candidates, generate_index, handle_http_path, import_openwiki, load_pages, note_page,
    playbook_registry, put_page, refresh_page_with_drift, serve_http, set_language, verify_page,
    wiki_locale, AgentsMdStatus, DriftOutcome, UpsertAction,
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

// ---------- HTML preview & HTTP server ----------

#[test]
fn html_preview_exports_site_with_citations_backlinks_and_http_routes() {
    let fx = Fixture::new(&[(
        "src/api.rs",
        "pub fn serve() {\n    let sample_id = \"a1b2c3d4e5f6g7h8i9j0\";\n    handle(sample_id);\n}\n",
    )]);

    put_page(
        &fx.repo,
        &fx.wiki(),
        "api/overview",
        "API Overview",
        "# API Layer\n\nEntry point is `serve` [[cite:src/api.rs:L1-L4]].\n",
        vec!["backend".into()],
        Some("system".into()),
    )
    .unwrap();

    note_page(
        &fx.repo,
        &fx.wiki(),
        "caller",
        "Caller notes",
        "Calls into [[wiki:api/overview]] during startup.\n",
        vec![],
        Some("workflow".into()),
    )
    .unwrap();

    generate_index(&fx.repo, &fx.wiki()).unwrap();

    let out_dir = fx.tmp.path().join("site");
    let report = build_html(&fx.repo, &fx.wiki(), &out_dir).unwrap();
    assert_eq!(report.pages_written, 4); // api/overview, caller, index, quickstart
    assert_eq!(report.stale_pages, 0);
    assert!(report.index_file.is_file());

    // Hierarchical page sits at pages/api/overview.html with `../../` relative root prefix
    let api_html =
        std::fs::read_to_string(out_dir.join("pages/api/overview.html")).expect("page html");
    assert!(api_html.contains("API Overview"));
    assert!(api_html.contains("href=\"#ev-1\""));
    assert!(api_html.contains("src/api.rs:L1-L4"));
    // Secret-shaped mixed-class token in source is masked in the HTML snippet
    assert!(api_html.contains("a1b2…[masked]"));
    assert!(!api_html.contains("a1b2c3d4e5f6g7h8i9j0"));
    // Backlink from `caller` is rendered on `api/overview`
    assert!(api_html.contains("pages/caller.html"));

    // Modify source file to trigger drift & staleness
    std::fs::write(
        fx.tmp.path().join("src/api.rs"),
        "// header1\n// header2\npub fn serve() {\n    let sample_id = \"a1b2c3d4e5f6g7h8i9j0\";\n    handle(sample_id);\n}\n",
    )
    .unwrap();
    commit_all(fx.tmp.path());

    // Live HTTP handler reflects staleness immediately
    let stale_resp = handle_http_path(fx.tmp.path(), "/pages/api/overview.html").unwrap();
    assert_eq!(stale_resp.status, 200);
    assert!(stale_resp.body.contains("stale-banner"));

    // Refresh projects L1-L4 -> L3-L6, and HTTP handler shows the projected range + ↻
    let mut page = loredex_engine::find_page(&fx.wiki(), "api/overview").unwrap();
    refresh_page_with_drift(&mut page, &fx.repo, &fx.wiki()).unwrap();

    let fresh_resp = handle_http_path(fx.tmp.path(), "/api/overview").unwrap();
    assert_eq!(fresh_resp.status, 200);
    assert!(fresh_resp.body.contains("src/api.rs:L3-L6 ↻"));

    // Search API endpoint works over HTTP
    let search_resp = handle_http_path(fx.tmp.path(), "/api/search?q=Entry+point").unwrap();
    assert_eq!(search_resp.status, 200);
    assert!(search_resp.body.contains("api/overview"));

    // 404 on unknown route
    let not_found = handle_http_path(fx.tmp.path(), "/no-such-page").unwrap();
    assert_eq!(not_found.status, 404);

    // Exercise TcpListener loop for 1 request
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let root = fx.tmp.path().to_path_buf();
    let handle = std::thread::spawn(move || {
        serve_http(listener, &root, Some(1)).unwrap();
    });
    let mut client = std::net::TcpStream::connect(addr).unwrap();
    use std::io::{Read, Write};
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut buf = String::new();
    client.read_to_string(&mut buf).unwrap();
    assert!(buf.starts_with("HTTP/1.1 200 OK"));
    assert!(buf.contains("API Overview"));
    handle.join().unwrap();
}

// ---------- Experimental Rust tree-sitter AST citations & relocation ----------

#[test]
fn rust_tree_sitter_symbol_citations_and_ast_relocation_across_reorder() {
    let initial_rs = "\
pub struct Pool {
    pub cap: usize,
}

impl Pool {
    pub fn connect(url: &str) -> Self {
        let _ = url;
        Self { cap: 16 }
    }
}

pub fn serve_requests(pool: &Pool) -> usize {
    let mut handled = 0;
    if pool.cap > 0 {
        handled += 1;
    }
    handled
}
";
    let fx = Fixture::new(&[("src/svc.rs", initial_rs)]);

    // 1. evidence_candidates annotates enclosing Rust AST symbol + symbol_cite
    let cands = evidence_candidates(&fx.repo.view(), ".", &["handled".into()], 5).unwrap();
    assert_eq!(cands.windows.len(), 1);
    assert_eq!(cands.windows[0].symbol.as_deref(), Some("serve_requests"));
    assert_eq!(
        cands.windows[0].symbol_cite.as_deref(),
        Some("[[cite:src/svc.rs#serve_requests]]")
    );

    // 2. Unknown Rust symbol is rejected at write time with available symbol hints
    let bad = put_page(
        &fx.repo,
        &fx.wiki(),
        "svc",
        "Service",
        "Missing symbol [[cite:src/svc.rs#nonexistent_fn]].\n",
        vec![],
        Some("system".into()),
    );
    let err_msg = format!("{}", bad.unwrap_err());
    assert!(err_msg.contains("nonexistent_fn"), "{err_msg}");
    assert!(err_msg.contains("Pool::connect"), "{err_msg}");

    // 3. Symbol-only citations resolve to exact AST line ranges at write time
    let outcome = put_page(
        &fx.repo,
        &fx.wiki(),
        "svc",
        "Service",
        "Pool connects [[cite:src/svc.rs#Pool::connect]] and serves [[cite:src/svc.rs#serve_requests]].\n",
        vec![],
        Some("system".into()),
    )
    .unwrap();
    assert_eq!(outcome.page.meta.status, Status::Verified);
    assert_eq!(outcome.page.meta.evidence.len(), 2);
    assert_eq!(
        outcome.page.meta.evidence[0].symbol.as_deref(),
        Some("Pool::connect")
    );
    assert_eq!(outcome.page.meta.evidence[0].lines, Some([6, 9]));
    assert_eq!(
        outcome.page.meta.evidence[1].symbol.as_deref(),
        Some("serve_requests")
    );
    assert_eq!(outcome.page.meta.evidence[1].lines, Some([12, 18]));
    assert!(verify_page(&outcome.page, &fx.repo).unwrap().all_ok);

    // 4. Reorder the file completely: move `serve_requests` to the top and push
    //    `impl Pool` below a large new block so line-diff projection fails on
    //    the moved function, triggering tree-sitter AST relocation.
    let reordered_rs = "\
pub fn serve_requests(pool: &Pool) -> usize {
    let mut handled = 0;
    if pool.cap > 0 {
        handled += 1;
    }
    handled
}

pub fn unrelated_alpha() -> u32 {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    a + b + c + d
}

pub fn unrelated_beta() -> u32 {
    let x = 10;
    let y = 20;
    let z = 30;
    x * y * z
}

pub struct Pool {
    pub cap: usize,
}

impl Pool {
    pub fn connect(url: &str) -> Self {
        let _ = url;
        Self { cap: 16 }
    }
}
";
    std::fs::write(fx.tmp.path().join("src/svc.rs"), reordered_rs).unwrap();
    commit_all(fx.tmp.path());

    let mut page = loredex_engine::find_page(&fx.wiki(), "svc").unwrap();
    let outcomes = refresh_page_with_drift(&mut page, &fx.repo, &fx.wiki()).unwrap();
    assert_eq!(page.meta.status, Status::Verified, "{outcomes:?}");
    assert!(outcomes
        .iter()
        .all(|(_, o)| matches!(o, DriftOutcome::Projected { .. })));
    // Pool::connect moved down to lines 29-32; serve_requests moved up to lines 1-7
    assert_eq!(page.meta.evidence[0].lines, Some([29, 32]));
    assert_eq!(page.meta.evidence[1].lines, Some([1, 7]));
    assert!(verify_page(&page, &fx.repo).unwrap().all_ok);
}
