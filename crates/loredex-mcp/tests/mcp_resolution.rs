//! In-process MCP integration tests for the 4-layer repository resolution
//! (cli pin > env > client roots > cwd > wiki_attach_repo).
//!
//! Uses rmcp's duplex transport so real `Peer<RoleServer>` injection, the
//! roots capability gate, tool dispatch and structured output are all
//! exercised through the actual protocol stack.
//!
//! Env-var dependent tests share `ENV_LOCK`; nextest runs every test in its
//! own process, but plain `cargo test` threads need the serialization.

use std::future::Future;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use loredex_mcp::WikiServer;
#[allow(deprecated)]
use rmcp::model::{ClientCapabilities, ClientConfig, Implementation, ListRootsResult, Root};
use rmcp::{service::RunningService, ClientHandler, RoleClient, ServiceExt};
use tempfile::TempDir;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// ---------- git fixture (minimal) ----------

struct Fixture {
    tmp: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        let git = git2::Repository::init(tmp.path()).expect("git init");
        std::fs::write(tmp.path().join("lib.rs"), "pub fn f() {}\n").unwrap();
        let sig = git2::Signature::now("test", "test@local").unwrap();
        let mut idx = git.index().unwrap();
        idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let tree = git.find_tree(idx.write_tree().unwrap()).unwrap();
        git.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        Self { tmp }
    }

    fn path(&self) -> &std::path::Path {
        self.tmp.path()
    }
}

// ---------- test client ----------

#[derive(Clone)]
struct TestClient {
    /// whether the client DECLARES the roots capability
    declares_roots: bool,
    /// roots to answer with (file paths, converted to file:// URIs)
    answer_roots: Vec<String>,
    /// how many times the server actually asked us for roots
    asked: Arc<AtomicUsize>,
}

impl TestClient {
    fn no_roots() -> Self {
        Self {
            declares_roots: false,
            answer_roots: vec![],
            asked: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn with_roots(paths: Vec<String>) -> Self {
        Self {
            declares_roots: true,
            answer_roots: paths,
            asked: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl ClientHandler for TestClient {
    #[allow(deprecated)]
    fn get_info(&self) -> ClientConfig {
        let caps = if self.declares_roots {
            ClientCapabilities::builder().enable_roots().build()
        } else {
            ClientCapabilities::builder().build()
        };
        ClientConfig::new(caps, Implementation::new("loredex-test", "0"))
    }

    #[allow(deprecated)]
    fn list_roots(
        &self,
        _context: rmcp::service::RequestContext<rmcp::RoleClient>,
    ) -> impl Future<Output = Result<ListRootsResult, rmcp::ErrorData>> + Send + '_ {
        self.asked.fetch_add(1, Ordering::SeqCst);
        let roots: Vec<Root> = self
            .answer_roots
            .iter()
            .map(|p| Root::new(format!("file://{p}")))
            .collect();
        std::future::ready(Ok(ListRootsResult::new(roots)))
    }
}

// ---------- harness ----------

type Client = RunningService<RoleClient, TestClient>;

async fn pair(
    server: WikiServer,
    client_handler: TestClient,
) -> (Client, tokio::task::JoinHandle<()>) {
    let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
    let server_task = tokio::spawn(async move {
        let service = server.serve(server_transport).await.expect("server serve");
        let _ = service.waiting().await;
    });
    let client = client_handler
        .serve(client_transport)
        .await
        .expect("client handshake");
    (client, server_task)
}

async fn call(client: &Client, tool: &str, args: serde_json::Value) -> rmcp::model::CallToolResult {
    call_try(client, tool, args).await.expect("call_tool ok")
}

async fn call_try(
    client: &Client,
    tool: &str,
    args: serde_json::Value,
) -> Result<rmcp::model::CallToolResult, rmcp::ServiceError> {
    use rmcp::model::CallToolRequestParams;
    let params = match args.as_object() {
        Some(map) if !map.is_empty() => {
            CallToolRequestParams::new(tool.to_string()).with_arguments(map.clone())
        }
        _ => CallToolRequestParams::new(tool.to_string()),
    };
    client.call_tool(params).await
}

fn structured(res: &rmcp::model::CallToolResult) -> serde_json::Value {
    serde_json::to_value(res.structured_content.as_ref().expect("structuredContent")).unwrap()
}

// ---------- tests ----------

#[tokio::test]
async fn env_layer_resolves_repo() {
    let _g = ENV_LOCK.lock().await;
    let fx = Fixture::new();
    std::env::set_var("LOREDEX_REPO", fx.path());
    let (client, _s) = pair(WikiServer::new_open(), TestClient::no_roots()).await;
    let sc = structured(&call(&client, "wiki_current_repo", serde_json::json!({})).await);
    std::env::remove_var("LOREDEX_REPO");
    assert_eq!(sc["source"], "env");
    assert!(sc["repo_root"]
        .as_str()
        .unwrap()
        .starts_with(&fx.tmp.path().to_string_lossy().to_string()));
}

#[tokio::test]
async fn roots_layer_resolves_repo_when_client_declares_roots() {
    let _g = ENV_LOCK.lock().await;
    let fx = Fixture::new();
    std::env::remove_var("LOREDEX_REPO");
    let client_handler = TestClient::with_roots(vec![fx.path().to_string_lossy().into()]);
    let asked = client_handler.asked.clone();
    let (client, _s) = pair(WikiServer::new_open(), client_handler).await;
    let sc = structured(&call(&client, "wiki_current_repo", serde_json::json!({})).await);
    assert_eq!(sc["source"], "roots", "must resolve via advertised roots");
    assert_eq!(asked.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn roots_capability_gate_prevents_query() {
    let _g = ENV_LOCK.lock().await;
    // client does NOT declare roots: server must not send roots/list;
    // resolution falls through to cwd (test process runs inside a git repo)
    let handler = TestClient::no_roots();
    let asked = handler.asked.clone();
    let (client, _s) = pair(WikiServer::new_open(), handler).await;
    let sc = structured(&call(&client, "wiki_current_repo", serde_json::json!({})).await);
    assert_eq!(
        asked.load(Ordering::SeqCst),
        0,
        "server must not query absent roots"
    );
    assert_eq!(sc["source"], "cwd");
    assert_eq!(sc["resolved"], serde_json::json!(true));
}

#[tokio::test]
async fn pinned_server_refuses_attach() {
    let fx = Fixture::new();
    let repo = loredex_git::GitRepo::discover(fx.path()).unwrap();
    let (client, _s) = pair(WikiServer::pinned(repo), TestClient::no_roots()).await;
    let err = call_try(
        &client,
        "wiki_attach_repo",
        serde_json::json!({ "path": fx.path().to_string_lossy() }),
    )
    .await
    .expect_err("attach must be refused on a pinned server");
    assert!(format!("{err}").contains("pinned"), "{err}");
}

#[tokio::test]
async fn attach_tool_switches_repo() {
    let _g = ENV_LOCK.lock().await;
    let fx = Fixture::new();
    std::env::remove_var("LOREDEX_REPO");
    let (client, _s) = pair(WikiServer::new_open(), TestClient::no_roots()).await;
    // first resolve via cwd (the workspace repo itself)
    let before = structured(&call(&client, "wiki_current_repo", serde_json::json!({})).await);
    assert_eq!(before["source"], "cwd");
    // attach explicitly moves the session to the fixture repo
    let after = structured(
        &call(
            &client,
            "wiki_attach_repo",
            serde_json::json!({ "path": fx.path().to_string_lossy() }),
        )
        .await,
    );
    assert_eq!(after["source"], "attach");
    assert!(after["repo_root"]
        .as_str()
        .unwrap()
        .starts_with(&fx.tmp.path().to_string_lossy().to_string()));
    // sticky: subsequent calls keep the attached repo
    let again = structured(&call(&client, "wiki_current_repo", serde_json::json!({})).await);
    assert_eq!(again["source"], "attach");
}

#[tokio::test]
async fn tools_run_end_to_end_after_attach() {
    let _g = ENV_LOCK.lock().await;
    let fx = Fixture::new();
    std::env::remove_var("LOREDEX_REPO");
    let (client, _s) = pair(WikiServer::new_open(), TestClient::no_roots()).await;
    call(
        &client,
        "wiki_attach_repo",
        serde_json::json!({ "path": fx.path().to_string_lossy() }),
    )
    .await;
    // put_page over the real protocol stack: citations pinned, structured out
    let res = structured(
        &call(
            &client,
            "wiki_put_page",
            serde_json::json!({
                "id": "lib",
                "title": "Fixture lib",
                "body": "f is a stub [[cite:lib.rs:L1-L1]]"
            }),
        )
        .await,
    );
    assert_eq!(res["citations"], serde_json::json!(1));
    assert_eq!(res["evidence"][0]["matches_head"], serde_json::json!(true));

    // uncited body -> protocol-level rejection
    let err = call_try(
        &client,
        "wiki_put_page",
        serde_json::json!({"id": "bad", "title": "t", "body": "no cites"}),
    )
    .await
    .expect_err("uncited body must be rejected");
    assert!(format!("{err}").contains("must cite"), "{err}");

    let list = structured(&call(&client, "wiki_list", serde_json::json!({})).await);
    assert_eq!(list["pages"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn search_cache_invalidates_on_write_and_external_edit() {
    let _g = ENV_LOCK.lock().await;
    let fx = Fixture::new();
    std::env::remove_var("LOREDEX_REPO");
    let (client, _s) = pair(WikiServer::new_open(), TestClient::no_roots()).await;
    call(
        &client,
        "wiki_attach_repo",
        serde_json::json!({ "path": fx.path().to_string_lossy() }),
    )
    .await;

    call(
        &client,
        "wiki_put_page",
        serde_json::json!({
            "id": "a", "title": "A",
            "body": "alpha needle [[cite:lib.rs:L1-L1]]"
        }),
    )
    .await;
    let hits = structured(
        &call(
            &client,
            "wiki_search",
            serde_json::json!({"query": "alpha"}),
        )
        .await,
    );
    assert_eq!(hits["hits"].as_array().unwrap().len(), 1);

    // a second put must invalidate the cached index immediately
    call(
        &client,
        "wiki_put_page",
        serde_json::json!({
            "id": "b", "title": "B",
            "body": "alpha second [[cite:lib.rs:L1-L1]]"
        }),
    )
    .await;
    let hits = structured(
        &call(
            &client,
            "wiki_search",
            serde_json::json!({"query": "alpha"}),
        )
        .await,
    );
    assert_eq!(hits["hits"].as_array().unwrap().len(), 2);

    // an external edit (same byte length) must invalidate via the mtime key
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let page = fx.path().join(".wiki/pages/a.md");
    let text = std::fs::read_to_string(&page)
        .unwrap()
        .replace("alpha", "omega");
    std::fs::write(&page, text).unwrap();

    let omega = structured(
        &call(
            &client,
            "wiki_search",
            serde_json::json!({"query": "omega"}),
        )
        .await,
    );
    assert_eq!(omega["hits"].as_array().unwrap().len(), 1);
    assert_eq!(omega["hits"][0]["id"], serde_json::json!("a"));
    let alpha = structured(
        &call(
            &client,
            "wiki_search",
            serde_json::json!({"query": "alpha"}),
        )
        .await,
    );
    assert_eq!(alpha["hits"].as_array().unwrap().len(), 1);
    assert_eq!(alpha["hits"][0]["id"], serde_json::json!("b"));
}

#[tokio::test]
async fn full_lifecycle_over_protocol_stale_refresh_evidence_search() {
    let _g = ENV_LOCK.lock().await;
    let fx = Fixture::new();
    std::env::remove_var("LOREDEX_REPO");
    let (client, _s) = pair(WikiServer::new_open(), TestClient::no_roots()).await;
    call(
        &client,
        "wiki_attach_repo",
        serde_json::json!({ "path": fx.path().to_string_lossy() }),
    )
    .await;

    // author
    call(
        &client,
        "wiki_put_page",
        serde_json::json!({
            "id": "lib", "title": "Fixture lib",
            "body": "f is a stub [[cite:lib.rs:L1-L1]]"
        }),
    )
    .await;

    // evolve the cited file: prepend lines so citations drift
    std::fs::write(
        fx.path().join("lib.rs"),
        "// a\n// b\npub fn f() {}\npub fn g() {}\n",
    )
    .unwrap();
    {
        let git = git2::Repository::open(fx.path()).unwrap();
        let sig = git2::Signature::now("t", "t@l").unwrap();
        let mut idx = git.index().unwrap();
        idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let tree = git.find_tree(idx.write_tree().unwrap()).unwrap();
        let parent = git.head().unwrap().peel_to_commit().unwrap();
        git.commit(Some("HEAD"), &sig, &sig, "drift", &tree, &[&parent])
            .unwrap();
    }

    // freshness reports stale + projectable
    let fresh = structured(&call(&client, "wiki_check_freshness", serde_json::json!({})).await);
    assert_eq!(fresh["stale_count"], serde_json::json!(1));
    assert_eq!(fresh["pages"][0]["stale"], serde_json::json!(true));
    assert_eq!(
        fresh["pages"][0]["projectable"].as_array().unwrap().len(),
        1
    );

    // read surfaces the stale warning
    let read = structured(&call(&client, "wiki_read", serde_json::json!({"id": "lib"})).await);
    assert_eq!(read["stale_now"], serde_json::json!(true));
    assert!(read["warning"].as_str().unwrap().contains("STALE"));

    // evidence expansion at HEAD: recorded line now points at inserted text
    let ev = structured(
        &call(
            &client,
            "wiki_get_evidence",
            serde_json::json!({"id": "lib", "index": 0}),
        )
        .await,
    );
    assert_eq!(ev["state"], "changed");
    assert_eq!(ev["content_at_head"].as_str().unwrap(), "// a");

    // refresh projects the citation onto the new revision
    let refreshed = structured(
        &call(
            &client,
            "wiki_refresh_page",
            serde_json::json!({"id": "lib"}),
        )
        .await,
    );
    assert_eq!(refreshed["new_status"], "verified");
    assert!(refreshed["outcomes"][0]["outcome"]
        .as_str()
        .unwrap()
        .starts_with("projected"));

    // after projection, evidence expansion follows the snippet to its new line
    let ev = structured(
        &call(
            &client,
            "wiki_get_evidence",
            serde_json::json!({"id": "lib", "index": 0}),
        )
        .await,
    );
    assert_eq!(ev["state"], "unchanged");
    assert_eq!(ev["content_at_head"].as_str().unwrap(), "pub fn f() {}");

    // search finds it
    let hits =
        structured(&call(&client, "wiki_search", serde_json::json!({"query": "stub"})).await);
    assert_eq!(hits["hits"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn initialize_advertises_instructions_and_tools() {
    let fx = Fixture::new();
    let repo = loredex_git::GitRepo::discover(fx.path()).unwrap();
    let (client, _s) = pair(WikiServer::pinned(repo), TestClient::no_roots()).await;

    // server -> client direction is what the client stored: check we can list tools
    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in [
        "wiki_list",
        "wiki_read",
        "wiki_check_freshness",
        "wiki_put_page",
        "wiki_repin_page",
        "wiki_refresh_page",
        "wiki_get_evidence",
        "wiki_search",
        "wiki_list_modules",
        "wiki_attach_repo",
        "wiki_current_repo",
        "wiki_note",
        "wiki_organize",
        "wiki_claim_tasks",
        "wiki_context_pack",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
}

#[tokio::test]
async fn wiki_plan_and_mark_over_protocol() {
    let _g = ENV_LOCK.lock().await;
    let fx = Fixture::new();
    std::env::remove_var("LOREDEX_REPO");
    let (client, _s) = pair(WikiServer::new_open(), TestClient::no_roots()).await;
    call(
        &client,
        "wiki_attach_repo",
        serde_json::json!({ "path": fx.path().to_string_lossy() }),
    )
    .await;

    // plan: fixture has uncovered lib.rs -> at least one New task
    let plan = structured(&call(&client, "wiki_plan", serde_json::json!({})).await);
    let tasks = plan["tasks"].as_array().unwrap();
    assert!(!tasks.is_empty(), "uncovered module must plan a task");
    let page_id = tasks[0]["page_id"].as_str().unwrap().to_string();

    // authoring with that id resolves the task automatically
    call(
        &client,
        "wiki_put_page",
        serde_json::json!({
            "id": page_id,
            "title": "Lib",
            "body": "g is stub [[cite:lib.rs:L1-L1]]",
            "type": "system"
        }),
    )
    .await;
    let plan2 = structured(&call(&client, "wiki_plan", serde_json::json!({})).await);
    let done = plan2["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["page_id"] == serde_json::json!(page_id));
    // task dropped from fresh computation (covered) — pending carryover is empty
    assert!(done
        .map(|t| t["status"].as_str() == Some("done"))
        .unwrap_or(true));

    // explicit mark on the next pending task
    if let Some(t) = plan2["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["status"] == serde_json::json!("pending"))
    {
        let pid = t["page_id"].as_str().unwrap().to_string();
        let res = structured(
            &call(
                &client,
                "wiki_mark_task",
                serde_json::json!({ "page_id": pid, "status": "skipped" }),
            )
            .await,
        );
        assert!(res["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["page_id"] == serde_json::json!(pid)
                && x["status"] == serde_json::json!("skipped")));
    }

    // read the authored page back: type field present
    let read = structured(&call(&client, "wiki_read", serde_json::json!({"id": page_id})).await);
    assert_eq!(read["page_type"], serde_json::json!("system"));
}

#[tokio::test]
async fn wiki_auto_over_protocol() {
    let _g = ENV_LOCK.lock().await;
    let fx = Fixture::new();
    std::env::remove_var("LOREDEX_REPO");
    let (client, _s) = pair(WikiServer::new_open(), TestClient::no_roots()).await;
    call(
        &client,
        "wiki_attach_repo",
        serde_json::json!({ "path": fx.path().to_string_lossy() }),
    )
    .await;
    let rep = structured(&call(&client, "wiki_auto", serde_json::json!({})).await);
    assert_eq!(rep["checked"], serde_json::json!(0));
    assert_eq!(rep["recovered"].as_array().unwrap().len(), 0);
    // no pages yet -> the uncovered module is pending authoring work, not clean
    assert_eq!(rep["all_clean"], serde_json::json!(false));
    let pending = rep["plan"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["status"] == serde_json::json!("pending"))
        .count();
    assert!(pending >= 1, "{rep}");
}

#[tokio::test]
async fn wiki_note_capture_promotes_and_organize_reports() {
    let _g = ENV_LOCK.lock().await;
    let fx = Fixture::new();
    std::env::remove_var("LOREDEX_REPO");
    let (client, _s) = pair(WikiServer::new_open(), TestClient::no_roots()).await;
    call(
        &client,
        "wiki_attach_repo",
        serde_json::json!({ "path": fx.path().to_string_lossy() }),
    )
    .await;

    // an uncited finding lands as an explicitly unverified draft
    let note = structured(
        &call(
            &client,
            "wiki_note",
            serde_json::json!({
                "id": "notes/backoff",
                "title": "Backoff spins",
                "body": "Suspected: the retry loop never yields.\n",
                "type": "troubleshooting"
            }),
        )
        .await,
    );
    assert_eq!(note["status"], serde_json::json!("draft"));
    assert_eq!(note["citations"], serde_json::json!(0));

    // plan queues it for evidence backfill, carrying the type's instructions
    let plan = structured(&call(&client, "wiki_plan", serde_json::json!({})).await);
    let task = plan["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["page_id"] == serde_json::json!("notes/backoff"))
        .expect("draft must be planned for backfill");
    assert_eq!(task["status"], serde_json::json!("pending"));
    assert_eq!(task["suggested_type"], serde_json::json!("troubleshooting"));

    // citing it promotes to verified and closes the task
    let promoted = structured(
        &call(
            &client,
            "wiki_put_page",
            serde_json::json!({
                "id": "notes/backoff",
                "title": "Backoff spins",
                "body": "The loop body is `f` [[cite:lib.rs:L1-L1]].",
                "type": "troubleshooting"
            }),
        )
        .await,
    );
    assert_eq!(promoted["status"], serde_json::json!("verified"));

    // two verified pages over the same evidence become a consolidation item
    call(
        &client,
        "wiki_put_page",
        serde_json::json!({
            "id": "core",
            "title": "Core",
            "body": "f is a stub [[cite:lib.rs:L1-L1]]",
            "type": "system"
        }),
    )
    .await;
    let org = structured(&call(&client, "wiki_organize", serde_json::json!({})).await);
    let candidates = org["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1, "{org}");
    assert_eq!(candidates[0]["pages"].as_array().unwrap().len(), 2);
    assert_eq!(
        candidates[0]["shared_files"][0],
        serde_json::json!("lib.rs")
    );
}

#[tokio::test]
async fn put_if_match_and_context_pack_over_protocol() {
    let fx = Fixture::new();
    let repo = loredex_git::GitRepo::discover(fx.path()).unwrap();
    let (client, _s) = pair(WikiServer::pinned(repo), TestClient::no_roots()).await;

    let first = structured(
        &call(
            &client,
            "wiki_put_page",
            serde_json::json!({
                "id": "lib", "title": "Fixture lib",
                "body": "# lib\n\nf is a stub [[cite:lib.rs:L1-L1]]\n"
            }),
        )
        .await,
    );
    let first_sha = first["page_sha1"].as_str().unwrap().to_string();
    assert_eq!(first_sha.len(), 40);

    // The token from the read is accepted...
    let second = structured(
        &call(
            &client,
            "wiki_put_page",
            serde_json::json!({
                "id": "lib", "title": "Fixture lib v2",
                "body": "# lib v2\n\nf is a stub [[cite:lib.rs:L1-L1]]\n",
                "if_match": first_sha
            }),
        )
        .await,
    );
    let second_sha = second["page_sha1"].as_str().unwrap().to_string();
    assert_ne!(second_sha, first_sha);

    // ...but a stale one is a conflict, not a silent overwrite.
    let err = call_try(
        &client,
        "wiki_put_page",
        serde_json::json!({
            "id": "lib", "title": "Fixture lib v3",
            "body": "# lib v3\n\nf is a stub [[cite:lib.rs:L1-L1]]\n",
            "if_match": first_sha
        }),
    )
    .await
    .expect_err("stale if_match must fail");
    assert!(
        format!("{err}").contains("changed since you read it"),
        "{err}"
    );

    let read = structured(&call(&client, "wiki_read", serde_json::json!({"id": "lib"})).await);
    assert_eq!(read["page_sha1"], serde_json::json!(second_sha));

    // A module pack carries the page and its deduplicated snippet.
    let pack = structured(
        &call(
            &client,
            "wiki_context_pack",
            serde_json::json!({"module": "."}),
        )
        .await,
    );
    assert_eq!(pack["pages"].as_array().unwrap().len(), 1, "{pack}");
    assert_eq!(pack["evidence"].as_array().unwrap().len(), 1, "{pack}");
    assert!(pack["markdown"].as_str().unwrap().contains("lib.rs"));

    // Leasing is protocol-visible; with no active run it is a guided error.
    let err = call_try(
        &client,
        "wiki_claim_tasks",
        serde_json::json!({"agent": "test-agent"}),
    )
    .await
    .expect_err("claim without a plan run must fail");
    assert!(format!("{err}").contains("no active plan run"), "{err}");
}

#[tokio::test]
async fn test_resources_lifecycle_over_protocol() {
    let fx = Fixture::new();
    let repo = loredex_git::GitRepo::discover(fx.path()).unwrap();
    let (client, _s) = pair(WikiServer::pinned(repo), TestClient::no_roots()).await;

    // 1. Initial list: should have loredex://modules
    let initial_res = client.list_resources(None).await.unwrap();
    let uris: Vec<&str> = initial_res
        .resources
        .iter()
        .map(|r| r.uri.as_str())
        .collect();
    assert!(
        uris.contains(&"loredex://modules"),
        "must expose modules resource"
    );

    // 2. Resource templates
    let templates = client.list_resource_templates(None).await.unwrap();
    let tmpl_uris: Vec<&str> = templates
        .resource_templates
        .iter()
        .map(|t| t.uri_template.as_str())
        .collect();
    assert!(
        tmpl_uris.contains(&"loredex://pages/{id}"),
        "must advertise pages template"
    );

    // 3. Put a verified page via tool call
    let put_res = call(
        &client,
        "wiki_put_page",
        serde_json::json!({
            "id": "intro",
            "title": "Introduction",
            "body": "Intro [[cite:lib.rs:L1-L1]]",
        }),
    )
    .await;
    assert!(!put_res.is_error.unwrap_or(false));

    // 4. List resources again: should now include loredex://pages/intro
    let updated_res = client.list_resources(None).await.unwrap();
    let updated_uris: Vec<&str> = updated_res
        .resources
        .iter()
        .map(|r| r.uri.as_str())
        .collect();
    assert!(
        updated_uris.contains(&"loredex://pages/intro"),
        "must list created page resource"
    );

    // 5. Read resource: loredex://pages/intro
    let page_content = client
        .read_resource(rmcp::model::ReadResourceRequestParams::new(
            "loredex://pages/intro",
        ))
        .await
        .unwrap();
    match &page_content.contents[0] {
        rmcp::model::ResourceContents::TextResourceContents {
            text, mime_type, ..
        } => {
            assert!(text.contains("id: intro"));
            assert!(text.contains("Intro [[cite:lib.rs:L1-L1]]"));
            assert_eq!(mime_type.as_deref(), Some("text/markdown"));
        }
        _ => panic!("expected text resource contents"),
    }

    // 6. Read resource: loredex://modules
    let modules_content = client
        .read_resource(rmcp::model::ReadResourceRequestParams::new(
            "loredex://modules",
        ))
        .await
        .unwrap();
    match &modules_content.contents[0] {
        rmcp::model::ResourceContents::TextResourceContents {
            text, mime_type, ..
        } => {
            assert_eq!(mime_type.as_deref(), Some("application/json"));
            assert!(text.contains("lib.rs"));
        }
        _ => panic!("expected text resource contents"),
    }

    // 7. Non-existent resource returns error
    let err = client
        .read_resource(rmcp::model::ReadResourceRequestParams::new(
            "loredex://pages/nonexistent",
        ))
        .await
        .expect_err("nonexistent resource must return error");
    assert!(format!("{err}").contains("not found"), "{err}");
}

#[tokio::test]
async fn test_prompts_lifecycle_over_protocol() {
    let fx = Fixture::new();
    let repo = loredex_git::GitRepo::discover(fx.path()).unwrap();
    let (client, _s) = pair(WikiServer::pinned(repo), TestClient::no_roots()).await;

    // 1. List prompts
    let list = client.list_prompts(None).await.unwrap();
    let names: Vec<&str> = list.prompts.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"author_wiki_page"));
    assert!(names.contains(&"audit_pr_freshness"));
    assert!(names.contains(&"heal_wiki"));

    // Check argument schema of author_wiki_page
    let author_prompt = list
        .prompts
        .iter()
        .find(|p| p.name == "author_wiki_page")
        .unwrap();
    let args = author_prompt.arguments.as_ref().unwrap();
    assert!(args
        .iter()
        .any(|a| a.name == "module" && a.required == Some(true)));

    // 2. Get prompt: author_wiki_page with arguments
    let mut author_args = serde_json::Map::new();
    author_args.insert(
        "module".to_string(),
        serde_json::Value::String("crates/foo".to_string()),
    );
    author_args.insert(
        "page_type".to_string(),
        serde_json::Value::String("architecture".to_string()),
    );
    author_args.insert(
        "keywords".to_string(),
        serde_json::Value::String("engine,parse".to_string()),
    );

    let res = client
        .get_prompt(
            rmcp::model::GetPromptRequestParams::new("author_wiki_page")
                .with_arguments(author_args),
        )
        .await
        .unwrap();
    assert_eq!(res.messages.len(), 1);
    match &res.messages[0].content {
        rmcp::model::ContentBlock::Text(t) => {
            assert!(t.text.contains("crates/foo"));
            assert!(t.text.contains("architecture"));
            assert!(t.text.contains("engine,parse"));
            assert!(t.text.contains("wiki_evidence_candidates"));
            assert!(t.text.contains("wiki_put_page"));
        }
        _ => panic!("expected text content block"),
    }

    // Missing required argument 'module' should fail
    let err = client
        .get_prompt(rmcp::model::GetPromptRequestParams::new("author_wiki_page"))
        .await
        .expect_err("missing required param must fail");
    assert!(format!("{err}").contains("module"), "{err}");

    // 3. Get prompt: audit_pr_freshness
    let mut audit_args = serde_json::Map::new();
    audit_args.insert(
        "base_ref".to_string(),
        serde_json::Value::String("main".to_string()),
    );
    let res = client
        .get_prompt(
            rmcp::model::GetPromptRequestParams::new("audit_pr_freshness")
                .with_arguments(audit_args),
        )
        .await
        .unwrap();
    match &res.messages[0].content {
        rmcp::model::ContentBlock::Text(t) => {
            assert!(t.text.contains("main"));
            assert!(t.text.contains("wiki_diff_since"));
            assert!(t.text.contains("wiki_refresh_page"));
        }
        _ => panic!("expected text content block"),
    }

    // 4. Get prompt: heal_wiki
    let res = client
        .get_prompt(rmcp::model::GetPromptRequestParams::new("heal_wiki"))
        .await
        .unwrap();
    match &res.messages[0].content {
        rmcp::model::ContentBlock::Text(t) => {
            assert!(t.text.contains("wiki_auto"));
            assert!(t.text.contains("wiki_plan"));
        }
        _ => panic!("expected text content block"),
    }

    // 5. Unknown prompt
    let err = client
        .get_prompt(rmcp::model::GetPromptRequestParams::new("unknown_prompt"))
        .await
        .expect_err("unknown prompt must fail");
    assert!(format!("{err}").contains("Unknown prompt"), "{err}");
}

#[tokio::test]
async fn wiki_repin_page_over_protocol() {
    let fx = Fixture::new();
    std::fs::write(fx.path().join("lib.rs"), "pub fn f() {}\npub fn g() {}\n").unwrap();
    {
        let git = git2::Repository::open(fx.path()).unwrap();
        let sig = git2::Signature::now("t", "t@l").unwrap();
        let mut idx = git.index().unwrap();
        idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let tree = git.find_tree(idx.write_tree().unwrap()).unwrap();
        let parent = git.head().unwrap().peel_to_commit().unwrap();
        git.commit(Some("HEAD"), &sig, &sig, "two-fns", &tree, &[&parent])
            .unwrap();
    }
    let repo = loredex_git::GitRepo::discover(fx.path()).unwrap();
    let (client, _s) = pair(WikiServer::pinned(repo), TestClient::no_roots()).await;

    call(
        &client,
        "wiki_put_page",
        serde_json::json!({
            "id": "lib",
            "title": "Fixture lib",
            "body": "f [[cite:lib.rs:L1-L1]]",
            "tags": ["core"],
            "type": "system"
        }),
    )
    .await;

    // Edit the page body on disk to cite L2-L2 and set status: draft
    let md_path = fx.path().join(".wiki/pages/lib.md");
    let raw = std::fs::read_to_string(&md_path).unwrap();
    std::fs::write(
        &md_path,
        raw.replace("[[cite:lib.rs:L1-L1]]", "[[cite:lib.rs:L2-L2]]")
            .replace("status: verified", "status: draft"),
    )
    .unwrap();

    let repinned = structured(
        &call(
            &client,
            "wiki_repin_page",
            serde_json::json!({ "id": "lib" }),
        )
        .await,
    );
    assert_eq!(repinned["status"], serde_json::json!("verified"));
    assert_eq!(repinned["citations"], serde_json::json!(1));
    assert_eq!(repinned["evidence"][0]["lines"], serde_json::json!([2, 2]));
}
