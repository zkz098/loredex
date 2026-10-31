use std::io::Read;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json;
use loredex_core::{Status, WikiPage};
use loredex_engine::{self as engine};
use loredex_git::GitRepo;

#[derive(Parser)]
#[command(name = "loredex", version, about = "Loredex: evidence-backed repository wiki for coding agents")]
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
    },
    /// List pages with status
    List,
    /// Print a page
    Show { id: String },
    /// Detect stale pages by comparing evidence blobs against git history
    Check {
        /// Write `status: stale` into affected page files
        #[arg(long)]
        mark: bool,
    },
    /// Verify a page's evidence at snippet level (content hashes, not just commits)
    Verify { id: String },
    /// Re-point a stale page's evidence at HEAD using line drift projection
    Refresh { id: String },
    /// Tokenized search over wiki pages
    Search { query: String },
    /// List discoverable modules (cargo targets / directory groups) for wiki authoring
    Modules,
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
}

fn open_repo(cli: &Cli) -> anyhow::Result<GitRepo> {
    let root = match &cli.repo {
        Some(p) => p.clone(),
        None => std::env::current_dir()?,
    };
    Ok(GitRepo::discover(&root)?)
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
        }
        Command::Put { id, title, body, tag, page_type } => {
            let body_text = if body == "-" {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                std::fs::read_to_string(&body)?
            };
            let outcome = engine::put_page(&repo, &wiki_dir, &id, &title, &body_text, tag, page_type)?;
            println!(
                "{} {}: {} evidence citation(s), pinned to HEAD {}",
                if outcome.created { "created" } else { "updated" },
                outcome.page.meta.id,
                outcome.citations,
                repo.head_short()?
            );
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
        Command::Show { id } => {
            let page = engine::find_page(&wiki_dir, &id)?;
            print!("{}", page.to_markdown()?);
        }
        Command::Check { mark } => {
            let pages = engine::load_pages(&wiki_dir)?;
            let mut stale_count = 0;
            for page in &pages {
                let report = engine::check_page(page, &repo)?;
                if report.is_stale {
                    stale_count += 1;
                    println!(
                        "STALE  {} — {}",
                        report.id, report.title
                    );
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
                            println!("       (status still '{}'; use --mark)", page.meta.status.as_str());
                        }
                    }
                } else if report.status_before == Status::Stale {
                    println!("FIXED? {} — evidence matches HEAD again; run `loredex refresh {}`", report.id, report.id);
                }
            }
            let total = pages.len();
            println!(
                "\n{total} page(s), {stale_count} stale; repo HEAD {}{}",
                repo.head_short()?,
                if repo.is_dirty()? { " (working tree dirty)" } else { "" }
            );
            if stale_count > 0 {
                std::process::exit(1);
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
                println!("[{}] {} — {} (score {})", page_status(&pages, &h.id), h.id, h.title, h.score);
                for l in &h.matched_lines {
                    println!("   {l}");
                }
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
            let run = engine::plan_with(&repo, &wiki_dir, reset)?;
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
                    println!(
                        "  [{:?}] {:<24} {:?} — {}",
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
        Command::Index => {
            for page in engine::generate_index(&repo, &wiki_dir)? {
                println!("wrote {}", page.source_path.unwrap().display());
            }
        }
        Command::Auto { json } => {
            let report = engine::auto_refresh(&repo, &wiki_dir)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(|e| anyhow::anyhow!(e))?
                );
            } else {
                println!(
                    "checked {} page(s) at HEAD {} — {} recovered by projection, {} need rewriting",
                    report.checked, report.head,
                    report.recovered.len(), report.needs_rewrite.len()
                );
                for r in &report.recovered {
                    println!("  [recovered] {}: {}", r.page_id, r.summary);
                }
                for n in &report.needs_rewrite {
                    println!("  [rewrite]   {}: {}", n.page_id, n.reason);
                }
                for i in &report.lint_issues {
                    println!("  [lint:{}] {}: {}", i.kind, i.page, i.detail);
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
                        println!("     [{:?}] {} [{:?}] — {}", t.status, t.page_id, t.kind, t.purpose);
                    }
                }
            }
            if !report.all_clean {
                std::process::exit(1);
            }
        }
        Command::Templates { out } => {
            std::fs::create_dir_all(&out)?;
            let action = include_str!("../../../templates/loredex-refresh.yml");
            let hook = include_str!("../../../templates/loredex-post-merge");
            let a = out.join("loredex-refresh.yml");
            let h = out.join("loredex-post-merge");
            std::fs::write(&a, action)?;
            std::fs::write(&h, hook)?;
            println!("wrote {} and {} (read the header comments for install)", a.display(), h.display());
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
            for (_, pb) in &reg {
                println!(
                    "{} — {}",
                    pb.name, pb.description
                );
                println!("   sections: {}", pb.sections.join(" -> "));
                for c in &pb.checklist {
                    println!("   - {c}");
                }
            }
        }
        Command::Lint { json } => {
            let issues = engine::lint(&repo, &wiki_dir)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&issues).map_err(|e| anyhow::anyhow!(e))?
                );
            } else if issues.is_empty() {
                println!("lint clean");
            } else {
                for i in &issues {
                    println!("[{}] {}: {}", i.kind, i.page, i.detail);
                }
            }
            if !issues.is_empty() {
                std::process::exit(1);
            }
        }
        Command::Modules => {
            let modules = engine::discover_modules(&repo)?;
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
