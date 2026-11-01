//! Range diff (`diff --since`) and cross-page link recommendations.

use git2::{Repository, Signature};
use loredex_engine::links::suggest_links_with;
use loredex_engine::{diff_since_with, put_page_with, DiffReport, LinkReport, PutOptions};
use loredex_git::GitRepo;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Fixture {
    _tmp: TempDir,
    repo: GitRepo,
}

fn commit_all(repo_path: &Path) -> String {
    let git = Repository::open(repo_path).expect("open");
    let sig = Signature::now("test", "test@local").expect("sig");
    let mut idx = git.index().unwrap();
    idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    let tree_id = idx.write_tree().unwrap();
    let tree = git.find_tree(tree_id).unwrap();
    let head = git.head().ok().map(|h| h.peel_to_commit().unwrap());
    let oid = match head {
        Some(p) => git
            .commit(Some("HEAD"), &sig, &sig, "c", &tree, &[&p])
            .unwrap(),
        None => git
            .commit(Some("HEAD"), &sig, &sig, "c", &tree, &[])
            .unwrap(),
    };
    oid.to_string()
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let tmp = TempDir::new().expect("tempdir");
        Repository::init(tmp.path()).expect("git init");
        write(tmp.path(), files);
        commit_all(tmp.path());
        let repo = GitRepo::discover(tmp.path()).expect("discover");
        loredex_engine::init_wiki_layout(&repo.wiki_dir()).expect("init layout");
        Self { _tmp: tmp, repo }
    }

    fn write_and_commit(&self, files: &[(&str, &str)]) -> String {
        write(self.workdir(), files);
        commit_all(self.workdir())
    }

    fn workdir(&self) -> &Path {
        self.repo.workdir()
    }

    fn wiki(&self) -> PathBuf {
        self.repo.wiki_dir()
    }

    fn put(&self, id: &str, body: &str) {
        put_page_with(
            &self.repo.view(),
            &self.wiki(),
            id,
            id,
            body,
            vec![],
            None,
            &PutOptions::default(),
        )
        .expect("put");
    }
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (path, content) in files {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
}

// ---------- diff --since ----------

#[test]
fn diff_since_reports_modify_and_rename_for_citing_pages() {
    let fx = Fixture::new(&[
        ("src/db.rs", "fn open() {}\nfn query() {}\nfn close() {}\n"),
        ("src/api.rs", "fn serve() {}\n"),
        ("src/ui.rs", "fn render() {}\n"),
    ]);
    let base = fx.repo.head_commit().unwrap();
    fx.put("db", "opens and queries [[cite:src/db.rs:L1-L2]]");
    fx.put("api", "serves [[cite:src/api.rs:L1-L1]]");
    fx.put("ui", "renders [[cite:src/ui.rs:L1-L1]]");

    fx.write_and_commit(&[(
        "src/db.rs",
        "fn open() {}\nfn query_v2() {}\nfn close() {}\n",
    )]);
    std::fs::rename(
        fx.workdir().join("src/ui.rs"),
        fx.workdir().join("src/frontend.rs"),
    )
    .unwrap();
    fx.write_and_commit(&[]);

    let report: DiffReport = diff_since_with(&fx.repo.view(), &fx.wiki(), &base).unwrap();
    assert_eq!(report.since, base[..9]);
    assert!(report.since_is_ancestor);
    assert_eq!(report.changed_paths, 2);

    let by_id: std::collections::BTreeMap<_, _> = report
        .affected_pages
        .iter()
        .map(|p| (p.id.as_str(), p))
        .collect();
    assert_eq!(report.affected_pages.len(), 2, "{report:?}");
    let db = by_id["db"];
    assert_eq!(db.evidence[0].change, "modified");
    assert_eq!(db.evidence[0].lines, Some([1, 2]));
    let ui = by_id["ui"];
    assert_eq!(ui.evidence[0].change, "renamed");
    assert_eq!(
        ui.evidence[0].renamed_to.as_deref(),
        Some("src/frontend.rs")
    );
    assert_eq!(report.clean_pages, 1); // api
}

#[test]
fn diff_since_surfaces_rename_pairs_and_bad_refs() {
    let fx = Fixture::new(&[("src/old.rs", "fn thing() {}\nfn more() {}\n")]);
    let base = fx.repo.head_commit().unwrap();
    fx.put("old", "cited [[cite:src/old.rs:L1-L2]]");

    std::fs::rename(
        fx.workdir().join("src/old.rs"),
        fx.workdir().join("src/new.rs"),
    )
    .unwrap();
    fx.write_and_commit(&[]);

    let report = diff_since_with(&fx.repo.view(), &fx.wiki(), &base).unwrap();
    assert_eq!(report.affected_pages.len(), 1);
    let hit = &report.affected_pages[0].evidence[0];
    assert_eq!(hit.change, "renamed");
    assert_eq!(hit.renamed_to.as_deref(), Some("src/new.rs"));

    let err = diff_since_with(&fx.repo.view(), &fx.wiki(), "definitely-not-a-ref");
    assert!(err.is_err());
}

#[test]
fn diff_since_on_head_is_empty() {
    let fx = Fixture::new(&[("src/a.rs", "fn a() {}\n")]);
    fx.put("core", "a does [[cite:src/a.rs:L1-L1]]");
    let report = diff_since_with(&fx.repo.view(), &fx.wiki(), "HEAD").unwrap();
    assert_eq!(report.changed_paths, 0);
    assert!(report.affected_pages.is_empty());
}

// ---------- links ----------

#[test]
fn links_recommends_shared_citation_pairs_both_ways() {
    let fx = Fixture::new(&[
        ("src/db.rs", "fn open() {}\nfn query() {}\nfn close() {}\n"),
        ("src/api.rs", "fn serve() {}\n"),
    ]);
    fx.put("db", "storage opens and queries [[cite:src/db.rs:L1-L2]]");
    fx.put(
        "db-usages",
        "the api calls open [[cite:src/db.rs:L1-L1]] and serve [[cite:src/api.rs:L1-L1]]",
    );

    let LinkReport {
        suggestions,
        pages_scanned,
        ..
    } = suggest_links_with(&fx.repo.view(), &fx.wiki(), 3, 3).unwrap();
    assert_eq!(pages_scanned, 2);
    let pairs: Vec<(&str, &str)> = suggestions
        .iter()
        .map(|s| (s.from.as_str(), s.to.as_str()))
        .collect();
    assert!(pairs.contains(&("db", "db-usages")), "{pairs:?}");
    assert!(pairs.contains(&("db-usages", "db")), "{pairs:?}");
    let s = suggestions.iter().find(|s| s.from == "db").unwrap();
    assert!(
        s.reasons.iter().any(|r| r.contains("src/db.rs")),
        "{:?}",
        s.reasons
    );
}

#[test]
fn links_skips_directions_already_linked() {
    let fx = Fixture::new(&[("src/db.rs", "fn open() {}\nfn query() {}\n")]);
    fx.put("db", "opens [[cite:src/db.rs:L1-L1]]");
    fx.put(
        "db-usages",
        "calls open [[cite:src/db.rs:L1-L1]] plus [[wiki:db]]",
    );

    let report = suggest_links_with(&fx.repo.view(), &fx.wiki(), 3, 3).unwrap();
    let froms: Vec<&str> = report.suggestions.iter().map(|s| s.from.as_str()).collect();
    assert_eq!(froms, vec!["db"], "{:?}", report.suggestions); // db->usages still open
}

#[test]
fn links_uses_module_adjacency_alone() {
    let fx = Fixture::new(&[
        ("src/db.rs", "fn open() {}\n"),
        ("src/api.rs", "fn serve() {}\n"),
    ]);
    fx.put("db", "opens [[cite:src/db.rs:L1-L1]]");
    fx.put("api", "serves [[cite:src/api.rs:L1-L1]]");

    let report = suggest_links_with(&fx.repo.view(), &fx.wiki(), 3, 3).unwrap();
    // distinct files: score = same-module adjacency (2) + text bonus (1) = 3
    let s = report
        .suggestions
        .iter()
        .find(|s| s.from == "db" && s.to == "api")
        .unwrap_or_else(|| panic!("no db->api suggestion in {:?}", report.suggestions));
    assert_eq!(s.score, 3);
    assert!(
        s.reasons.iter().any(|r| r.contains("same module")),
        "{:?}",
        s.reasons
    );
}
