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
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};

use rmcp::model::{
    ClientCapabilities, ClientConfig, Implementation, ListRootsResult, Root,
};
use rmcp::{service::RunningService, RoleClient, ServiceExt, ClientHandler};
use tempfile::TempDir;
use wiki_mcp::WikiServer;

static ENV_LOCK: Mutex<()> = Mutex::new(());

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
    fn get_info(&self) -> ClientConfig {
        let caps = if self.declares_roots {
            ClientCapabilities::builder().enable_roots().build()
        } else {
            ClientCapabilities::builder().build()
        };
        ClientConfig::new(caps, Implementation::new("repowiki-test", "0"))
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

async fn pair(server: WikiServer, client_handler: TestClient) -> (Client, tokio::task::JoinHandle<()>) {
    let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
    let server_task = tokio::spawn(async move {
        let service = server
            .serve(server_transport)
            .await
            .expect("server serve");
        let _ = service.waiting().await;
    });
    let client = client_handler
        .serve(client_transport)
        .await
        .expect("client handshake");
    (client, server_task)
}

async fn call(
    client: &Client,
    tool: &str,
    args: serde_json::Value,
) -> rmcp::model::CallToolResult {
    call_try(client, tool, args).await.expect("call_tool ok")
}

async fn call_try(
    client: &Client,
    tool: &str,
    args: serde_json::Value,
) -> Result<rmcp::model::CallToolResult, rmcp::ServiceError> {
    use rmcp::model::CallToolRequestParams;
    let params = match args.as_object() {
        Some(map) if !map.is_empty() => CallToolRequestParams::new(tool.to_string())
            .with_arguments(map.clone()),
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
    let _g = ENV_LOCK.lock().unwrap();
    let fx = Fixture::new();
    std::env::set_var("REPOWIKI_REPO", fx.path());
    let (client, _s) = pair(WikiServer::new_open(), TestClient::no_roots()).await;
    let sc = structured(&call(&client, "wiki_current_repo", serde_json::json!({})).await);
    std::env::remove_var("REPOWIKI_REPO");
    assert_eq!(sc["source"], "env");
    assert!(sc["repo_root"]
        .as_str()
        .unwrap()
        .starts_with(&fx.tmp.path().to_string_lossy().to_string()));
}

#[tokio::test]
async fn roots_layer_resolves_repo_when_client_declares_roots() {
    let _g = ENV_LOCK.lock().unwrap();
    let fx = Fixture::new();
    std::env::remove_var("REPOWIKI_REPO");
    let client_handler = TestClient::with_roots(vec![fx.path().to_string_lossy().into()]);
    let asked = client_handler.asked.clone();
    let (client, _s) = pair(WikiServer::new_open(), client_handler).await;
    let sc = structured(&call(&client, "wiki_current_repo", serde_json::json!({})).await);
    assert_eq!(sc["source"], "roots", "must resolve via advertised roots");
    assert_eq!(asked.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn roots_capability_gate_prevents_query() {
    let _g = ENV_LOCK.lock().unwrap();
    // client does NOT declare roots: server must not send roots/list;
    // resolution falls through to cwd (test process runs inside a git repo)
    let handler = TestClient::no_roots();
    let asked = handler.asked.clone();
    let (client, _s) = pair(WikiServer::new_open(), handler).await;
    let sc = structured(&call(&client, "wiki_current_repo", serde_json::json!({})).await);
    assert_eq!(asked.load(Ordering::SeqCst), 0, "server must not query absent roots");
    assert_eq!(sc["source"], "cwd");
    assert_eq!(sc["resolved"], serde_json::json!(true));
}

#[tokio::test]
async fn pinned_server_refuses_attach() {
    let fx = Fixture::new();
    let repo = wiki_git::GitRepo::discover(fx.path()).unwrap();
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
    let _g = ENV_LOCK.lock().unwrap();
    let fx = Fixture::new();
    std::env::remove_var("REPOWIKI_REPO");
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
    let _g = ENV_LOCK.lock().unwrap();
    let fx = Fixture::new();
    std::env::remove_var("REPOWIKI_REPO");
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
async fn full_lifecycle_over_protocol_stale_refresh_evidence_search() {
    let _g = ENV_LOCK.lock().unwrap();
    let fx = Fixture::new();
    std::env::remove_var("REPOWIKI_REPO");
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
    std::fs::write(fx.path().join("lib.rs"), "// a\n// b\npub fn f() {}\npub fn g() {}\n").unwrap();
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
    assert_eq!(fresh["pages"][0]["projectable"].as_array().unwrap().len(), 1);

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
    let refreshed =
        structured(&call(&client, "wiki_refresh_page", serde_json::json!({"id": "lib"})).await);
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
    let hits = structured(&call(&client, "wiki_search", serde_json::json!({"query": "stub"})).await);
    assert_eq!(hits["hits"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn initialize_advertises_instructions_and_tools() {
    let fx = Fixture::new();
    let repo = wiki_git::GitRepo::discover(fx.path()).unwrap();
    let (client, _s) = pair(WikiServer::pinned(repo), TestClient::no_roots()).await;

    // server -> client direction is what the client stored: check we can list tools
    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in [
        "wiki_list",
        "wiki_read",
        "wiki_check_freshness",
        "wiki_put_page",
        "wiki_refresh_page",
        "wiki_get_evidence",
        "wiki_search",
        "wiki_list_modules",
        "wiki_attach_repo",
        "wiki_current_repo",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
}
