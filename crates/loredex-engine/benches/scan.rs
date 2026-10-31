//! Scan/index regression benchmarks on a synthetic 2000-page wiki.
//!
//! Each scan iteration builds a fresh GitView (the same lifetime as one MCP
//! tool call) and walks every page, so the numbers cover the full per-command
//! git cache setup rather than just a warmed inner loop.

use std::hint::black_box;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use criterion::Criterion;
use git2::{Repository, Signature};
use loredex_core::search::{IndexedPage, InvertedIndex};
use loredex_core::{Evidence, PageMeta, Status, WikiPage};
use loredex_engine::{check_page_with, load_pages, prime_view, sha1_hex};
use loredex_git::GitRepo;
use tempfile::TempDir;

const FILES: usize = 200;
const LINES: usize = 60;
const PAGES: usize = 2000;
const COMMON_PAGES: usize = 300;

#[derive(Clone, Copy)]
enum Scenario {
    Fresh,
    Edited,
    DeletedCommon,
}

struct Fixture {
    _tmp: TempDir,
    repo: GitRepo,
    pages: Vec<WikiPage>,
}

fn commit_all(root: &Path, message: &str) {
    let repo = Repository::open(root).expect("open repo");
    let sig = Signature::now("bench", "bench@example.com").expect("signature");
    let mut index = repo.index().expect("index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("add");
    let tree_id = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");
    let head = repo.head().ok().map(|h| h.peel_to_commit().expect("head"));
    match head {
        Some(parent) => repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
            .expect("commit"),
        None => repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
            .expect("commit"),
    };
}

fn build(scenario: Scenario) -> Fixture {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    std::fs::create_dir_all(root.join("src")).expect("src");
    for i in 0..FILES {
        let mut content = String::new();
        for j in 1..=LINES {
            content.push_str(&format!("fn f{i}_line{j}() {{}} // payload {i}-{j}\n"));
        }
        std::fs::write(root.join(format!("src/f{i:03}.rs")), content).expect("source");
    }
    Repository::init(root).expect("git init");
    commit_all(root, "base");
    let repo = GitRepo::discover(root).expect("discover");
    std::fs::create_dir_all(root.join(".wiki/pages")).expect("pages dir");

    let mut pages = Vec::with_capacity(PAGES);
    {
        let view = repo.view();
        let base = view.head_commit().expect("head").to_string();
        let mut cursor = 0usize;
        for p in 0..PAGES {
            let files: Vec<usize> = match scenario {
                Scenario::DeletedCommon if p < COMMON_PAGES => vec![1, 1, 1],
                _ => {
                    let a = cursor % FILES;
                    cursor += 7;
                    vec![a, (a + 13) % FILES, (a + 29) % FILES]
                }
            };
            let mut evidence = Vec::new();
            let mut body = format!("# Page {p:04}\n\n");
            for (n, f) in files.iter().enumerate() {
                let rel = format!("src/f{f:03}.rs");
                let lo = 1 + (n as u32 * 10);
                let hi = lo + 9;
                let blob = view
                    .head_blob_oid(&rel)
                    .expect("blob lookup")
                    .expect("tracked file");
                let snippet = view
                    .head_lines(&rel, lo, hi)
                    .expect("lines")
                    .expect("snippet");
                evidence.push(Evidence {
                    path: rel.clone(),
                    lines: Some([lo, hi]),
                    blob,
                    snippet_sha1: Some(sha1_hex(snippet.as_bytes())),
                    note: None,
                });
                body.push_str(&format!("claim {n} [[cite:{rel}:L{lo}-L{hi}]]. "));
            }
            let page = WikiPage {
                meta: PageMeta {
                    id: format!("page-{p:04}"),
                    title: format!("Page {p:04}"),
                    status: Status::Verified,
                    generated_at: "2026-10-31T00:00:00Z".into(),
                    commit: base.clone(),
                    evidence,
                    tags: vec![],
                    page_type: Some("system".into()),
                },
                body: body.into(),
                source_path: Some(root.join(format!(".wiki/pages/page-{p:04}.md"))),
            };
            page.save_in_place().expect("save page");
            pages.push(page);
        }
    }

    // Wiki pages are committed like in a real repository; pages record the
    // commit they were generated at, which is this pages commit.
    commit_all(root, "wiki pages");
    let pages_commit = repo.head_commit().expect("pages commit");
    for page in &mut pages {
        page.meta.commit = pages_commit.clone();
        page.save_in_place().expect("re-save page");
    }

    match scenario {
        Scenario::Fresh => {}
        Scenario::Edited => {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(root.join("src/f000.rs"))
                .expect("open source");
            writeln!(f, "fn appended() {{}}").expect("append");
            commit_all(root, "edit");
        }
        Scenario::DeletedCommon => {
            std::fs::remove_file(root.join("src/f001.rs")).expect("delete");
            commit_all(root, "delete");
        }
    }

    Fixture {
        _tmp: tmp,
        repo,
        pages,
    }
}

fn scan_fixture(c: &mut Criterion, name: &str, fixture: &Fixture) {
    let pages = &fixture.pages;
    c.bench_function(name, |b| {
        b.iter(|| {
            let view = fixture.repo.view();
            prime_view(&view, pages).expect("prime");
            let mut stale = 0usize;
            for page in pages {
                if check_page_with(page, &view).expect("check").is_stale {
                    stale += 1;
                }
            }
            black_box(stale)
        })
    });
}

fn criterion_benches(c: &mut Criterion) {
    let fresh = build(Scenario::Fresh);
    scan_fixture(c, "scan_2000_fresh", &fresh);

    let edited = build(Scenario::Edited);
    scan_fixture(c, "scan_2000_after_edit", &edited);

    let deleted = build(Scenario::DeletedCommon);
    scan_fixture(c, "scan_300_cite_deleted_file", &deleted);

    let pages = &fresh.pages;
    c.bench_function("index_build_2000", |b| {
        b.iter(|| {
            let index = InvertedIndex::build(
                pages
                    .iter()
                    .map(|page| IndexedPage {
                        id: page.meta.id.clone(),
                        title: page.meta.title.clone(),
                        body: page.body.clone(),
                    })
                    .collect(),
            );
            black_box(index)
        })
    });

    let wiki_dir = fresh.repo.wiki_dir();
    c.bench_function("pages_load_2000", |b| {
        b.iter(|| black_box(load_pages(&wiki_dir).expect("load pages")))
    });
}

fn time_scan(fixture: &Fixture) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..3 {
        let start = Instant::now();
        let view = fixture.repo.view();
        prime_view(&view, &fixture.pages).expect("prime");
        let mut stale = 0usize;
        for page in &fixture.pages {
            if check_page_with(page, &view).expect("check").is_stale {
                stale += 1;
            }
        }
        black_box(stale);
        best = best.min(start.elapsed());
    }
    best
}

fn assert_budget(name: &str, elapsed: Duration, budget: Duration, failed: &mut bool) {
    let ok = elapsed <= budget;
    println!(
        "{name}: {:?} (budget {:?}) {}",
        elapsed,
        budget,
        if ok { "ok" } else { "REGRESSION" }
    );
    if !ok {
        *failed = true;
    }
}

/// Machine-independent safety net with generous headroom (local numbers are
/// 1-2 orders of magnitude below these); catches re-introducing per-page git
/// work without making CI flaky on shared runners.
fn budget_check() {
    let mut failed = false;
    let fresh = build(Scenario::Fresh);
    assert_budget(
        "scan_2000_fresh",
        time_scan(&fresh),
        Duration::from_millis(100),
        &mut failed,
    );

    let edited = build(Scenario::Edited);
    assert_budget(
        "scan_2000_after_edit",
        time_scan(&edited),
        Duration::from_millis(100),
        &mut failed,
    );

    let deleted = build(Scenario::DeletedCommon);
    assert_budget(
        "scan_300_cite_deleted_file",
        time_scan(&deleted),
        Duration::from_millis(500),
        &mut failed,
    );

    let start = Instant::now();
    let index = InvertedIndex::build(
        fresh
            .pages
            .iter()
            .map(|page| IndexedPage {
                id: page.meta.id.clone(),
                title: page.meta.title.clone(),
                body: page.body.clone(),
            })
            .collect(),
    );
    black_box(index);
    assert_budget(
        "index_build_2000",
        start.elapsed(),
        Duration::from_millis(200),
        &mut failed,
    );

    let start = Instant::now();
    black_box(load_pages(&fresh.repo.wiki_dir()).expect("load pages"));
    assert_budget(
        "pages_load_2000",
        start.elapsed(),
        Duration::from_millis(500),
        &mut failed,
    );

    if failed {
        std::process::exit(1);
    }
}

fn main() {
    if std::env::args().any(|arg| arg == "--budget") {
        budget_check();
        return;
    }
    let mut criterion = Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .configure_from_args();
    criterion_benches(&mut criterion);
    criterion.final_summary();
}
