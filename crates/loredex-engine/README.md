# loredex-engine

Business layer for [Loredex](https://github.com/zkz098/loredex), an evidence-backed repository wiki for AI coding agents. Everything the CLI and the MCP server do lives here; the crate has no UI of its own.

Organized around the page lifecycle — write, age, verify, heal, plan:

- **Writing** — `put::put_page` stores agent-authored content; bodies with zero resolvable citations are rejected at write time (paths must exist at HEAD, ranges must be well-formed). Optional `PageExpectation` (`if_match` page sha1 / `if_absent`) turns concurrent writes into explicit conflicts, with a cross-process lock around the digest check and rename. `init_wiki_layout` creates `.wiki/pages/`, `.wiki/wiki.toml`, `.wiki/.cache/`.
- **Aging** — `check_page` decides staleness with two independent verdicts (HEAD blob comparison *and* a pathspec tree-diff since the generation commit); `verify_page` re-hashes the cited lines' content, so trust never rests on commit ids alone.
- **Healing** — `drift` projects citations onto the new revision after insertions/deletions (`DriftOutcome::{Unchanged, Projected, Renamed, Lost, NoOldBlob}`); a projected snippet is adopted only at similarity ≥ 0.6, otherwise it honestly reports Lost. `refresh_page_with_drift` re-pins evidence and leaves an audit note; it refuses to save if the page file changed meanwhile. `auto::auto_refresh` runs the whole algorithmic self-heal, then lint, then produces a plan.
- **Planning** — `modules::discover` finds authoring targets (cargo targets / directory groups) with citation coverage; `plan` keeps a persisted run ledger in `.wiki/runs.json` (New + Refresh tasks, skips-with-memory, resume after interruption); `claim_tasks` leases pending tasks to an agent with an expiring deadline; `playbook` supplies per-page-type authoring guides.
- **Context** — `pack::context_pack` selects verified, git-fresh pages for a module by cited-path relevance and appends the deduplicated HEAD snippets they cite, masking secret-shaped lines and trimming to a token budget.
- **Quality** — `lint` flags broken links, stale-but-verified pages, body-evidence drift, unknown page types; `index::generate_index` regenerates `index.md` + `quickstart.md` reading paths.
- **Interoperability** — `config` reads/writes `[wiki] language` (en/zh locale for generated skeleton text), `agents_md` upserts the managed `<!-- BEGIN loredex -->` block in AGENTS.md/CLAUDE.md, `import` migrates openwiki / Google OKF bundles (uncited bodies land as draft for `plan` to backfill).

```rust
let repo = loredex_git::GitRepo::discover(".")?;
let pages = loredex_engine::load_pages(&repo.wiki_dir())?;
for page in &pages {
    let report = loredex_engine::check_page(page, &repo)?;
    if report.is_stale {
        println!("{} stale since {:?}", report.id, report.stale_since);
    }
}
```

Built on [`loredex-core`](https://github.com/zkz098/loredex/blob/main/crates/loredex-core/README.md) (page model) and [`loredex-git`](https://github.com/zkz098/loredex/blob/main/crates/loredex-git/README.md) (evidence); consumed by [`loredex-cli`](https://github.com/zkz098/loredex/blob/main/crates/loredex-cli/README.md) and [`loredex-mcp`](https://github.com/zkz098/loredex/blob/main/crates/loredex-mcp/README.md). License: GPL-3.0-or-later.
