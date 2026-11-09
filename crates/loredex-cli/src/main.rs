use std::io::Read;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use loredex_core::{Status, WikiPage};
use loredex_engine::{self as engine};
use loredex_git::GitRepo;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Parser)]
#[command(
    name = "loredex",
    version,
    about = "Loredex: evidence-backed repository wiki for coding agents"
)]
struct Cli {
    /// Repository root (defaults to discovery from current directory)
    #[arg(long, global = true)]
    repo: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create .wiki/ layout in the repository
    Init,
    /// Store wiki content authored by a coding agent; body must carry
    /// [[cite:path:Lx-Ly]] markers that become git-pinned evidence
    Put {
        id: String,
        #[arg(long)]
        title: String,
        /// Body from a file, or `-` for stdin
        #[arg(short, long)]
        body: String,
        #[arg(long)]
        tag: Vec<String>,
        /// Page kind, e.g. architecture | workflow | system | testing
        #[arg(long = "type")]
        page_type: Option<String>,
        /// Store citations even when the cited range matches a secret pattern
        #[arg(long)]
        no_redact: bool,
        /// Only write if the page still has this content sha1 (from `show`/MCP read)
        #[arg(long, conflicts_with = "if_absent")]
        if_match: Option<String>,
        /// Create only: fail when the page already exists
        #[arg(long)]
        if_absent: bool,
    },
    /// Capture an agent finding without citations: stored as an unverified
    /// draft page (with its markers pinned if it happens to carry them),
    /// queued by `loredex plan` for citation backfill and typing
    Note {
        id: String,
        #[arg(long)]
        title: String,
        /// Body from a file, or `-` for stdin
        #[arg(short, long)]
        body: String,
        #[arg(long)]
        tag: Vec<String>,
        /// Page kind, e.g. rule | troubleshooting | decision | note
        #[arg(long = "type")]
        page_type: Option<String>,
        /// Only write if the page still has this content sha1 (from `show`/MCP read)
        #[arg(long, conflicts_with = "if_absent")]
        if_match: Option<String>,
        /// Create only: fail when the page already exists
        #[arg(long)]
        if_absent: bool,
    },
    /// Consolidation worklist: page pairs whose evidence substantially
    /// overlaps, to merge or cross-link (advisory; never fails)
    Organize {
        #[arg(long)]
        json: bool,
    },
    /// List pages with status
    List,
    /// Print a page (or just its content sha1 for `--if-match`)
    Show {
        id: String,
        /// Print only the page file sha1, the token for `put --if-match`
        #[arg(long)]
        sha1: bool,
    },
    /// Detect stale pages by comparing evidence blobs against git history
    Check {
        /// Write `status: stale` into affected page files
        #[arg(long)]
        mark: bool,
    },
    /// Report which pages' evidence is touched by changes between a ref and
    /// HEAD (the PR-review question: "what docs did this diff break?")
    Diff {
        /// Base ref: sha, branch, tag, HEAD~n (diffed against HEAD)
        #[arg(long)]
        since: String,
        #[arg(long)]
        json: bool,
        /// Exit 1 when any verified page is affected (CI/hook friendly)
        #[arg(long)]
        fail_on_affected: bool,
    },
    /// Suggest cross-page [[wiki:id]] links from shared citations, module
    /// adjacency and text similarity (advisory; never a lint error)
    Links {
        /// Only suggestions authored by this page
        #[arg(long)]
        id: Option<String>,
        #[arg(long, default_value_t = 3)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Verify a page's evidence at snippet level (content hashes, not just commits)
    Verify { id: String },
    /// Re-point a stale page's evidence at HEAD using line drift projection
    Refresh { id: String },
    /// Tokenized search over wiki pages
    Search { query: String },
    /// List discoverable modules (cargo targets / directory groups) for wiki authoring
    Modules,
    /// Generate citation drafts: pasteable [[cite:path:Lx-Ly]] windows for a
    /// module, ranked by keyword matches against HEAD content
    Evidence {
        /// Module name (see `loredex modules`) or '.', or a directory prefix
        #[arg(default_value = ".")]
        module: String,
        /// Case-insensitive keywords to look for in the source
        #[arg(long = "kw")]
        keyword: Vec<String>,
        #[arg(long, default_value_t = 12)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Compute the generation plan (new-coverage + stale-refresh tasks) and record a run
    Plan {
        #[arg(long)]
        json: bool,
        /// Close the active run instead of opening/resuming one
        #[arg(long)]
        close: bool,
        /// Re-expose New tasks previously skipped/failed
        #[arg(long)]
        reset: bool,
    },
    /// Lease pending plan tasks to one agent (multi-agent coordination)
    Claim {
        /// Stable agent identifier, e.g. codex-1
        #[arg(long)]
        agent: String,
        #[arg(long, default_value_t = 1)]
        limit: usize,
        /// Lease duration in seconds; expired leases are reclaimable
        #[arg(long, default_value_t = 1800)]
        ttl_seconds: u64,
        #[arg(long)]
        json: bool,
    },
    /// Build a token-budgeted context pack: verified pages for a module plus
    /// the source snippets they cite
    Pack {
        /// Module name (see `loredex modules`) or '.', or a directory prefix
        #[arg(default_value = ".")]
        module: String,
        #[arg(long, default_value_t = 8000)]
        budget_tokens: usize,
        /// Include stale pages (marked as unverified)
        #[arg(long)]
        include_stale: bool,
        /// Include draft pages (marked as unverified)
        #[arg(long)]
        include_drafts: bool,
        #[arg(long)]
        json: bool,
    },
    /// Regenerate the navigation index page (.wiki/pages/index.md)
    Index,
    /// One-call maintenance: project recoverable drifts, lint, open plan for the rest.
    /// Exit 1 when authoring work remains (CI/hook friendly).
    Auto {
        #[arg(long)]
        json: bool,
    },
    /// Write automation templates (GitHub Action + post-merge hook) into a directory
    Templates {
        #[arg(long, default_value = "contrib")]
        out: PathBuf,
    },
    /// Resolve a pending plan task from the CLI (mirrors wiki_mark_task)
    Task {
        page_id: String,
        /// done | skipped | failed
        #[arg(default_value = "skipped")]
        status: String,
    },
    /// List page-type playbooks (built-in + wiki.toml extensions)
    Playbooks,
    /// Structural checks: broken wiki links, stale-but-verified, evidence drift
    Lint {
        #[arg(long)]
        json: bool,
    },
    /// Show or set the skeleton text language for generated pages ([wiki] language in .wiki/wiki.toml)
    Config {
        /// Set the language (en | zh); omit to just show the current one
        #[arg(long)]
        language: Option<String>,
    },
    /// Insert or refresh the loredex managed block in AGENTS.md
    AgentsMd {
        /// Remove the managed block instead of writing it
        #[arg(long)]
        remove: bool,
        /// Only report state (exit 1 when missing or drifted; CI-friendly)
        #[arg(long)]
        check: bool,
        /// Target file, relative to the repo root (e.g. CLAUDE.md)
        #[arg(long, default_value = "AGENTS.md")]
        file: PathBuf,
    },
    /// Import an existing wiki from another format (currently: openwiki / OKF bundle)
    Import {
        /// Directory containing the bundle (e.g. ./openwiki)
        dir: PathBuf,
        #[arg(long, default_value = "openwiki")]
        format: String,
        /// Namespace imported ids under this page-id prefix
        #[arg(long)]
        prefix: Option<String>,
        /// Report what would happen without touching .wiki/
        #[arg(long)]
        dry_run: bool,
        /// Overwrite existing pages with the same id
        #[arg(long)]
        force: bool,
    },
    /// Build a human-readable static HTML preview site with interactive source evidence
    Html {
        /// Output directory (defaults to .wiki/.cache/html)
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Serve a live human-readable HTML preview of the wiki over HTTP
    Serve {
        /// Address to bind (e.g. 127.0.0.1:3000)
        #[arg(long, default_value = "127.0.0.1:3000")]
        bind: String,
    },
}

fn open_repo(cli: &Cli) -> anyhow::Result<GitRepo> {
    let root = match &cli.repo {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };
    Ok(GitRepo::discover(&root)?)
}

fn expected_page(if_match: Option<String>, if_absent: bool) -> Option<engine::PageExpectation> {
    match (if_match, if_absent) {
        (Some(sha), _) => Some(engine::PageExpectation::Sha1(sha)),
        (None, true) => Some(engine::PageExpectation::Absent),
        (None, false) => None,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let repo = open_repo(&cli)?;
    let wiki_dir = repo.wiki_dir();

    match cli.command {
        Command::Init => {
            engine::init_wiki_layout(&wiki_dir)?;
            let gitignore = repo.workdir().join(".gitignore");
            let entry = ".wiki/.cache/";
            let existing = std::fs::read_to_string(&gitignore).unwrap_or_default();
            if !existing.lines().any(|l| l.trim() == entry) {
                let mut content = existing;
                if !content.is_empty() && !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(entry);
                content.push('\n');
                std::fs::write(&gitignore, content)?;
            }
            println!(
                "initialized wiki at {} (pages are committed; .wiki/.cache/ is gitignored)",
                wiki_dir.display()
            );
            println!(
                "tip: run `loredex agents-md` to register the wiki in AGENTS.md for coding agents"
            );
        }
        Command::Put {
            id,
            title,
            body,
            tag,
            page_type,
            no_redact,
            if_match,
            if_absent,
        } => {
            let body_text = if body == "-" {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                std::fs::read_to_string(&body)?
            };
            let opts = engine::PutOptions {
                allow_secret_citations: no_redact,
                expected: expected_page(if_match, if_absent),
                ..Default::default()
            };
            let outcome = engine::put_page_with(
                &repo.view(),
                &wiki_dir,
                &id,
                &title,
                &body_text,
                tag,
                page_type,
                &opts,
            )?;
            println!(
                "{} {}: {} evidence citation(s), pinned to HEAD {} (page sha1 {})",
                if outcome.created {
                    "created"
                } else {
                    "updated"
                },
                outcome.page.meta.id,
                outcome.citations,
                repo.head_short()?,
                outcome.page_sha1
            );
        }
        Command::Note {
            id,
            title,
            body,
            tag,
            page_type,
            if_match,
            if_absent,
        } => {
            let body_text = if body == "-" {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                std::fs::read_to_string(&body)?
            };
            let outcome = engine::note_page_with(
                &repo,
                &wiki_dir,
                &id,
                &title,
                &body_text,
                tag,
                page_type,
                expected_page(if_match, if_absent),
            )?;
            println!(
                "{} {} as '{}' with {} citation(s) (page sha1 {}){}",
                if outcome.created {
                    "captured"
                } else {
                    "updated"
                },
                outcome.page.meta.id,
                outcome.page.meta.status.as_str(),
                outcome.citations,
                outcome.page_sha1,
                if outcome.page.meta.status == Status::Draft {
                    " — `loredex plan` queues it for citation backfill"
                } else {
                    ""
                }
            );
        }
        Command::Organize { json } => {
            let report = engine::organize::consolidation_candidates(&wiki_dir)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(|e| anyhow::anyhow!(e))?
                );
            } else if report.candidates.is_empty() {
                println!(
                    "no consolidation candidates ({} page(s) scanned)",
                    report.pages_scanned
                );
            } else {
                for c in &report.candidates {
                    println!("{} and {}", c.pages[0], c.pages[1]);
                    println!("   shared evidence: {}", c.shared_files.join(", "));
                    println!("   {}", c.suggestion);
                }
                println!("hint: {}", report.hint);
            }
        }
        Command::List => {
            let pages = engine::load_pages(&wiki_dir)?;
            if pages.is_empty() {
                println!("no pages yet; run `loredex put <id> --title ... --body <file|->` with [[cite:path:Lx-Ly]] markers");
            }
            for p in &pages {
                println!(
                    "{:<8} {:<20} {:<10} {} ({} evidence, commit {})",
                    p.meta.status.as_str(),
                    p.meta.id,
                    "",
                    p.meta.title,
                    p.meta.evidence.len(),
                    p.meta.commit.chars().take(8).collect::<String>(),
                );
            }
        }
        Command::Show { id, sha1 } => {
            let page = engine::find_page(&wiki_dir, &id)?;
            if sha1 {
                let digest = match &page.source_path {
                    Some(path) => engine::page_sha1(path)?,
                    None => None,
                };
                match digest {
                    Some(d) => println!("{d}"),
                    None => anyhow::bail!("page `{id}` has no source file"),
                }
            } else {
                print!("{}", page.to_markdown()?);
            }
        }
        Command::Check { mark } => {
            let pages = engine::load_pages(&wiki_dir)?;
            let view = repo.view();
            engine::prime_view(&view, &pages)?;
            let mut stale_count = 0;
            for page in &pages {
                let report = engine::check_page_with(page, &view)?;
                if report.is_stale {
                    stale_count += 1;
                    println!("STALE  {} — {}", report.id, report.title);
                    for r in &report.evidence_reports {
                        if !r.ok {
                            println!("       - {}: {}", r.path, r.detail);
                        }
                    }
                    if page.meta.status != Status::Stale {
                        if mark {
                            let mut p = page.clone();
                            p.meta.status = Status::Stale;
                            p.save_in_place()?;
                            println!("       marked stale");
                        } else {
                            println!(
                                "       (status still '{}'; use --mark)",
                                page.meta.status.as_str()
                            );
                        }
                    }
                } else if report.status_before == Status::Stale {
                    println!(
                        "FIXED? {} — evidence matches HEAD again; run `loredex refresh {}`",
                        report.id, report.id
                    );
                }
            }
            let total = pages.len();
            println!(
                "\n{total} page(s), {stale_count} stale; repo HEAD {}{}",
                view.head_short()?,
                if view.is_dirty()? {
                    " (working tree dirty)"
                } else {
                    ""
                }
            );
            if stale_count > 0 {
                std::process::exit(1);
            }
        }
        Command::Diff {
            since,
            json,
            fail_on_affected,
        } => {
            let view = repo.view();
            let report = engine::diff_since_with(&view, &wiki_dir, &since)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(|e| anyhow::anyhow!(e))?
                );
            } else if report.affected_pages.is_empty() {
                println!(
                    "{} changed path(s) between {} and {}; no wiki evidence touched",
                    report.changed_paths, report.since, report.head
                );
            } else {
                if !report.since_is_ancestor {
                    println!(
                        "note: {} is not an ancestor of HEAD; reporting the symmetric tree diff",
                        report.since
                    );
                }
                println!(
                    "{} page(s) affected by {} changed path(s) between {} and {}:",
                    report.affected_pages.len(),
                    report.changed_paths,
                    report.since,
                    report.head
                );
                for p in &report.affected_pages {
                    println!("  [{}] {} — {}", p.status, p.id, p.title);
                    for e in &p.evidence {
                        let range = e
                            .lines
                            .map(|[a, b]| format!("L{a}-L{b}"))
                            .unwrap_or_else(|| "whole file".into());
                        let extra = match (&e.renamed_to, &e.renamed_from) {
                            (Some(t), _) => format!(" -> {t}"),
                            (_, Some(f)) => format!(" (was {f})"),
                            _ => String::new(),
                        };
                        println!("       - {} {range} {}{extra}", e.path, e.change);
                    }
                }
            }
            if fail_on_affected && report.affected_pages.iter().any(|p| p.status == "verified") {
                std::process::exit(1);
            }
        }
        Command::Links { id, limit, json } => {
            let view = repo.view();
            let mut report = engine::links::suggest_links_with(&view, &wiki_dir, limit, 3)?;
            if let Some(page) = &id {
                report.suggestions.retain(|s| s.from == *page);
            }
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(|e| anyhow::anyhow!(e))?
                );
            } else if report.suggestions.is_empty() {
                println!(
                    "no link suggestions ({} page(s) scanned)",
                    report.pages_scanned
                );
            } else {
                let mut cur = "";
                for s in &report.suggestions {
                    if s.from != cur {
                        cur = &s.from;
                        println!("{}:", s.from);
                    }
                    println!(
                        "   [[wiki:{}]] (score {}) — {}",
                        s.to,
                        s.score,
                        s.reasons.join("; ")
                    );
                }
                println!("hint: {}", report.hint);
            }
        }
        Command::Verify { id } => {
            let page = engine::find_page(&wiki_dir, &id)?;
            let report = engine::verify_page(&page, &repo)?;
            for r in &report.results {
                println!(
                    "[{}] {}: {}",
                    if r.ok { "ok" } else { "FAIL" },
                    r.path,
                    r.detail
                );
            }
            if !report.all_ok {
                println!("\npage `{id}` failed snippet verification");
                std::process::exit(1);
            }
            println!("\npage `{id}` verified against content hashes");
        }
        Command::Refresh { id } => {
            let mut page = engine::find_page(&wiki_dir, &id)?;
            let outcomes = engine::refresh_page_with_drift(&mut page, &repo, &wiki_dir)?;
            for (path, outcome) in &outcomes {
                println!("  {path}: {outcome:?}");
            }
            println!(
                "refreshed {} -> status '{}' against HEAD {}",
                page.meta.id,
                page.meta.status.as_str(),
                repo.head_short()?
            );
        }
        Command::Search { query } => {
            use loredex_core::search::{IndexedPage, InvertedIndex};
            let pages = engine::load_pages(&wiki_dir)?;
            let idx = InvertedIndex::build(
                pages
                    .iter()
                    .map(|p| IndexedPage {
                        id: p.meta.id.clone(),
                        title: p.meta.title.clone(),
                        body: p.body.clone(),
                    })
                    .collect(),
            );
            let hits = idx.search(&query, 10);
            if hits.is_empty() {
                println!("no matches");
            }
            for h in hits {
                println!(
                    "[{}] {} — {} (score {})",
                    page_status(&pages, &h.id),
                    h.id,
                    h.title,
                    h.score
                );
                for l in &h.matched_lines {
                    println!("   {l}");
                }
            }
        }
        Command::Evidence {
            module,
            keyword,
            limit,
            json,
        } => {
            let view = repo.view();
            let out = engine::evidence_candidates(&view, &module, &keyword, limit)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&out).map_err(|e| anyhow::anyhow!(e))?
                );
            } else {
                if out.windows.is_empty() {
                    println!(
                        "no matching windows in module `{}`; check `loredex modules` or loosen keywords",
                        out.module
                    );
                }
                println!(
                    "{} candidate window(s) in {} file(s) at HEAD {}:",
                    out.windows.len(),
                    out.matched_files,
                    view.head_short()?
                );
                for w in &out.windows {
                    match &w.symbol_cite {
                        Some(sc) => println!("  {}  (or {})", w.cite, sc),
                        None => println!("  {}", w.cite),
                    }
                    for p in &w.masked_preview {
                        println!("     | {p}");
                    }
                }
                println!("hint: {}", out.hint);
            }
        }
        Command::Plan { json, close, reset } => {
            if close {
                match engine::close_run(&wiki_dir)? {
                    Some(id) => println!("closed run {id}"),
                    None => println!("no active run"),
                }
                return Ok(());
            }
            let run = engine::plan_with_view(&repo.view(), &wiki_dir, reset)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&run).map_err(|e| anyhow::anyhow!(e))?
                );
            } else {
                println!(
                    "run {} (base {} -> target {}, {} task(s))",
                    run.id,
                    &run.base_commit[..8.min(run.base_commit.len())],
                    &run.target_commit[..8.min(run.target_commit.len())],
                    run.tasks.len()
                );
                for t in &run.tasks {
                    let claim = t
                        .claimed_by
                        .as_deref()
                        .map(|a| {
                            format!(
                                " [claimed by {a} until {}]",
                                t.lease_until.as_deref().unwrap_or("?")
                            )
                        })
                        .unwrap_or_default();
                    println!(
                        "  [{:?}] {:<24} {:?}{claim} — {}",
                        t.status, t.page_id, t.kind, t.purpose
                    );
                    for seed in t.seeds.iter().take(4) {
                        println!("         seed: {seed}");
                    }
                }
                if run.tasks.is_empty() {
                    println!("  wiki fully covered and fresh");
                }
            }
        }
        Command::Claim {
            agent,
            limit,
            ttl_seconds,
            json,
        } => {
            let claimed = engine::claim_tasks(&wiki_dir, &agent, limit, ttl_seconds)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&claimed).map_err(|e| anyhow::anyhow!(e))?
                );
            } else if claimed.is_empty() {
                println!("no claimable task for agent `{agent}`");
            } else {
                for t in &claimed {
                    println!("claimed [{:?}] {} — {}", t.kind, t.page_id, t.purpose);
                }
                println!(
                    "lease until {}; authoring the page with `loredex put` resolves it automatically",
                    claimed
                        .first()
                        .and_then(|t| t.lease_until.as_deref())
                        .unwrap_or("?")
                );
            }
        }
        Command::Pack {
            module,
            budget_tokens,
            include_stale,
            include_drafts,
            json,
        } => {
            let pack = engine::context_pack(
                &repo.view(),
                &wiki_dir,
                &module,
                &engine::PackOptions {
                    budget_tokens,
                    include_stale,
                    include_drafts,
                },
            )?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&pack).map_err(|e| anyhow::anyhow!(e))?
                );
            } else {
                print!("{}", pack.markdown);
                if pack.truncated {
                    eprintln!(
                        "note: context pack truncated to ~{} tokens ({} page(s), {} snippet(s) included, {} skipped)",
                        pack.budget_tokens,
                        pack.pages.len(),
                        pack.evidence.len(),
                        pack.skipped.len()
                    );
                }
            }
        }
        Command::Index => {
            for page in engine::generate_index_with(&repo.view(), &wiki_dir)? {
                println!("wrote {}", page.source_path.unwrap().display());
            }
        }
        Command::Auto { json } => {
            let report = engine::auto_refresh_with(&repo.view(), &wiki_dir)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(|e| anyhow::anyhow!(e))?
                );
            } else {
                println!(
                    "checked {} page(s) at HEAD {} — {} recovered by projection, {} need rewriting",
                    report.checked,
                    report.head,
                    report.recovered.len(),
                    report.needs_rewrite.len()
                );
                for r in &report.recovered {
                    println!("  [recovered] {}: {}", r.page_id, r.summary);
                }
                for n in &report.needs_rewrite {
                    println!("  [rewrite]   {}: {}", n.page_id, n.reason);
                }
                for i in &report.lint_issues {
                    let kind = if i.advisory {
                        format!("advisory:{}", i.kind)
                    } else {
                        i.kind.clone()
                    };
                    println!("  [lint:{kind}] {}: {}", i.page, i.detail);
                }
                let pending: Vec<_> = report
                    .plan
                    .tasks
                    .iter()
                    .filter(|t| t.status == loredex_engine::TaskStatus::Pending)
                    .collect();
                if !pending.is_empty() {
                    println!("  [plan] {} pending task(s):", pending.len());
                    for t in pending {
                        println!(
                            "     [{:?}] {} [{:?}] — {}",
                            t.status, t.page_id, t.kind, t.purpose
                        );
                    }
                }
            }
            if !report.all_clean {
                std::process::exit(1);
            }
        }
        Command::Templates { out } => {
            std::fs::create_dir_all(&out)?;
            let action = include_str!("../templates/loredex-refresh.yml");
            let hook = include_str!("../templates/loredex-post-merge");
            let a = out.join("loredex-refresh.yml");
            let h = out.join("loredex-post-merge");
            std::fs::write(&a, action)?;
            std::fs::write(&h, hook)?;
            println!(
                "wrote {} and {} (read the header comments for install)",
                a.display(),
                h.display()
            );
        }
        Command::Task { page_id, status } => {
            let st = match status.as_str() {
                "done" => loredex_engine::TaskStatus::Done,
                "skipped" => loredex_engine::TaskStatus::Skipped,
                "failed" => loredex_engine::TaskStatus::Failed,
                other => anyhow::bail!("unknown status {other}"),
            };
            engine::mark_task(&wiki_dir, &page_id, st)?;
            println!("marked {page_id} as {status}");
        }
        Command::Playbooks => {
            let reg = engine::playbook_registry(&wiki_dir);
            for pb in reg.values() {
                println!("{} — {}", pb.name, pb.description);
                println!("   sections: {}", pb.sections.join(" -> "));
                for c in &pb.checklist {
                    println!("   - {c}");
                }
            }
        }
        Command::Lint { json } => {
            let issues = engine::lint_with(&repo.view(), &wiki_dir)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&issues).map_err(|e| anyhow::anyhow!(e))?
                );
            } else if issues.is_empty() {
                println!("lint clean");
            } else {
                for i in &issues {
                    let tag = if i.advisory {
                        format!("advisory:{}", i.kind)
                    } else {
                        i.kind.clone()
                    };
                    println!("[{tag}] {}: {}", i.page, i.detail);
                }
            }
            if issues.iter().any(|i| !i.advisory) {
                std::process::exit(1);
            }
        }
        Command::Modules => {
            let modules = engine::discover_with(&repo.view())?;
            for m in &modules {
                println!("{} ({}, {} files)", m.name, m.kind, m.files.len());
                for f in m.files.iter().take(12) {
                    println!("   {f}");
                }
                if m.files.len() > 12 {
                    println!("   ... {} more", m.files.len() - 12);
                }
            }
        }
        Command::Config { language } => match language {
            None => {
                let loc = engine::wiki_locale(&wiki_dir);
                println!(
                    "wiki language: {} (set with `loredex config --language en|zh`)",
                    loc.as_str()
                );
            }
            Some(tag) => {
                let loc = match tag.to_lowercase().as_str() {
                    t if t.starts_with("en") => engine::Locale::En,
                    t if t.starts_with("zh") => engine::Locale::Zh,
                    other => anyhow::bail!("unknown language `{other}` (supported: en, zh)"),
                };
                engine::set_language(&wiki_dir, loc.as_str())?;
                println!(
                    "wiki language set to {}; run `loredex index` to regenerate skeleton pages",
                    loc.as_str()
                );
            }
        },
        Command::AgentsMd {
            remove,
            check,
            file,
        } => {
            let path = if file.is_absolute() {
                file.clone()
            } else {
                repo.workdir().join(file)
            };
            if remove {
                if engine::agents_block_remove(&path)? {
                    println!("removed loredex block from {}", path.display());
                } else {
                    println!("no loredex block in {}", path.display());
                }
                return Ok(());
            }
            let locale = engine::wiki_locale(&wiki_dir);
            if check {
                use engine::AgentsMdStatus as S;
                match engine::agents_block_check(&path, locale)? {
                    S::InPlace => println!("in place: {}", path.display()),
                    S::Drifted => {
                        println!("drifted: {} (re-run `loredex agents-md`)", path.display());
                        std::process::exit(1);
                    }
                    S::Missing => {
                        println!("no loredex block in {}", path.display());
                        std::process::exit(1);
                    }
                    S::FileMissing => {
                        println!("{} does not exist", path.display());
                        std::process::exit(1);
                    }
                }
                return Ok(());
            }
            use engine::UpsertAction as A;
            match engine::agents_block_upsert(&path, locale)? {
                A::Created => println!("created {} with the loredex block", path.display()),
                A::Updated => println!("refreshed loredex block in {}", path.display()),
                A::Appended => println!("appended loredex block to {}", path.display()),
                A::Unchanged => println!("loredex block already current in {}", path.display()),
            }
        }
        Command::Import {
            dir,
            format,
            prefix,
            dry_run,
            force,
        } => {
            let Some(fmt) = engine::ImportFormat::parse(&format) else {
                anyhow::bail!("unknown import format `{format}` (supported: openwiki)");
            };
            let report = engine::import_dir(
                &repo,
                &wiki_dir,
                &dir,
                fmt,
                prefix.as_deref(),
                dry_run,
                force,
            )?;
            let verb = if dry_run { "would import" } else { "imported" };
            println!(
                "{verb} {} page(s) from {}{}",
                report.imported.len(),
                dir.display(),
                if report.registered_types.is_empty() {
                    String::new()
                } else {
                    format!(", registered types: {}", report.registered_types.join(", "))
                }
            );
            for p in &report.imported {
                println!(
                    "  [{}] {} — {} ({} evidence{})",
                    p.status.as_str(),
                    p.id,
                    p.title,
                    p.evidence,
                    if p.overwritten { ", overwritten" } else { "" }
                );
            }
            for s in &report.skipped {
                println!("  [skipped] {}: {}", s.path, s.reason);
            }
            if report.imported.iter().any(|p| p.status == Status::Draft) && !dry_run {
                println!("draft pages await citation backfill — see `loredex plan`");
            }
        }
        Command::Html { out, json } => {
            let out_dir = out.unwrap_or_else(|| wiki_dir.join(".cache").join("html"));
            let report = engine::build_html_with(&repo.view(), &wiki_dir, &out_dir)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(|e| anyhow::anyhow!(e))?
                );
            } else {
                println!(
                    "built HTML preview ({} page(s), {} stale) -> {}",
                    report.pages_written,
                    report.stale_pages,
                    report.index_file.display()
                );
            }
        }
        Command::Serve { bind } => {
            let listener = std::net::TcpListener::bind(&bind)?;
            let local = listener.local_addr()?;
            println!(
                "serving live wiki preview for {} at http://{local} (Ctrl+C to stop)",
                repo.workdir().display()
            );
            engine::serve_http(listener, repo.workdir(), None)?;
        }
    }
    Ok(())
}

fn page_status(pages: &[WikiPage], id: &str) -> &'static str {
    pages
        .iter()
        .find(|p| p.meta.id == id)
        .map(|p| p.meta.status.as_str())
        .unwrap_or("?")
}
