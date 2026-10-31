# Loredex

[![CI](https://github.com/zkz098/loredex/actions/workflows/ci.yml/badge.svg)](https://github.com/zkz098/loredex/actions/workflows/ci.yml)
[![GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)

[中文文档](README_cn.md)

A **trustworthy repository wiki for AI coding agents**: plain-Markdown storage, git-driven freshness tracking, and every claim pinned to verifiable source evidence. Written in Rust, two entry points (CLI + MCP server).

## Why

Agents reading a repository face two costs: expensive context and misleading stale docs. Loredex answers with:

- **Pinned evidence** — every claim in a page body carries a `[[cite:path:Lx-Ly]]` marker, resolved at write time into a `(path, line range, git blob sha, snippet content sha1)` tuple. Bodies without citations are rejected.
- **Git-driven staleness** — commit ids are never trusted alone: HEAD blob comparison cross-checked against a tree diff, and verification recomputes the content hash of the cited lines. Change the code and the citing pages go stale.
- **Line drift projection** — after insertions/deletions, citations are remapped onto the new revision via line-level diff; accepted above a similarity threshold with an audit note, keeping the page `verified`. Only genuinely lost content is downgraded to `draft` for rewrite.
- **Rename recovery** — a plain `git mv` keeps the same blob oid, so the new path is found by content-addressed lookup (more precise than rename heuristics); mv + small edits use git similarity detection plus line projection; everything else falls back to a rewrite.
- **No database** — the frontmatter *is* the database. `.wiki/pages/*.md` is committed to git, reviewable, hand-editable; there is no authoritative index that can corrupt, and the search index rebuilds from files in milliseconds.

## Install

**From crates.io (cargo)** — requires Rust 1.85+ (git2 vendors and compiles libgit2; takes a few minutes):

```bash
cargo install loredex-cli loredex-mcp
# or from a checkout:
cargo build --release
# binaries: target/release/loredex (CLI), target/release/loredex-mcp (MCP server)
```

**Prebuilt binaries**: GitHub Releases carry `loredex-cli-<target-triple>.tar.xz` / `loredex-mcp-<target-triple>.tar.xz` archives + shell/PowerShell installers, produced by cargo-dist for 5 targets. `cargo binstall loredex-cli` installs in seconds.

**CPU baseline**: release binaries and checkout builds compile x86-64 at the `x86-64-v3` level (`-C target-cpu=x86-64-v3` in `.cargo/config.toml`) — an AVX2-era CPU (2013+) is required; delete those tables to build for older machines. aarch64 targets and `cargo install` from crates.io (the config is workspace-local, not packaged) are unaffected.

## Quick start (CLI)

```bash
cd /path/to/your-repo          # must have at least one commit
loredex init                   # creates .wiki/pages/ + wiki.toml (language setting); gitignores .wiki/.cache/

loredex modules                # module discovery (cargo targets / directory groups)
loredex put core --title "Core layer" --body notes.md --type system
loredex list / show core / search "keyword"
loredex check --mark           # staleness scan; non-zero exit = stale found (CI-friendly)
loredex plan [--json]          # generation worklist: uncovered modules (New) + stale pages (Refresh), persisted to .wiki/runs.json
loredex index                  # regenerate pages/index.md + pages/quickstart.md (typed reading path + agent protocol)
loredex playbooks              # page-type authoring guides (5 built-in + wiki.toml extensions)
loredex lint                   # broken links / stale-but-verified / body-evidence drift / unknown type; exit 1 on findings
loredex auto [--json]          # one-call maintenance: algorithmic self-heal -> lint -> plan for the rest (exit 1 = authoring work remains)
loredex task <id> skipped      # resolve a plan task by hand (skips are remembered; `plan --reset` re-exposes)
loredex templates --out contrib    # export GitHub Action + post-merge hook templates
loredex verify core            # content-hash verification (does not rely on commit ids)
loredex refresh core           # drift projection refresh
loredex config --language zh   # locale for tool-generated skeleton text (index/quickstart, playbook instructions, AGENTS.md block); agent-authored bodies are never translated
loredex agents-md              # write the managed discovery block into AGENTS.md (`--check` for CI, `--remove`, `--file CLAUDE.md`)
loredex import ./openwiki      # migrate an openwiki / Google OKF bundle; bodies without resolvable citations land as draft for `plan` to backfill (`--dry-run` / `--force` / `--prefix`)
```

`--body -` reads from stdin. A page body looks like:

```markdown
serve() handles requests in a loop [[cite:src/api.rs:L1-L5]]; main initializes the db first [[cite:src/main.rs:L3-L6]].
```

## Agent integration (MCP)

**Zero-config (recommended)**: one global entry serves any repository. The server resolves its target lazily in four layers — `--repo` flag (pinned) → `LOREDEX_REPO` env → client MCP roots → process working directory (mainstream clients spawn stdio servers with the workspace as cwd):

```json
{
  "mcpServers": {
    "loredex": { "command": "loredex-mcp" }
  }
}
```

If every layer fails, tools return a guided error and the agent self-heals via `wiki_attach_repo`; `wiki_current_repo` reports the resolved repository and its source at any time. To pin a single repository:

```json
{
  "mcpServers": {
    "loredex": {
      "command": "loredex-mcp",
      "args": ["--repo", "/path/to/your-repo"]
    }
  }
}
```

The server advertises a behavioral contract at `initialize` (check freshness before trusting; stale means unverified; drift goes through refresh; citations are auditable). All tools return structured `structuredContent` (`stale_now`, `matches_head`, `projectable`, ...).

| Tool | Purpose |
|---|---|
| `wiki_check_freshness` | Full git staleness scan + projectable-item annotation |
| `wiki_list` / `wiki_read` | Page inventory / read a page (stale pages carry a warning) |
| `wiki_search` | Tokenized search (camelCase, snake_case, CJK bigrams, prefix) |
| `wiki_get_evidence` | Expand one citation to the actual file content at HEAD |
| `wiki_list_modules` | Discover authoring targets with citation coverage |
| `wiki_put_page` | Store agent-authored content (zero citations = rejected) |
| `wiki_refresh_page` | Drift projection refresh with per-citation outcomes |
| `wiki_attach_repo` / `wiki_current_repo` | Attach/switch repository, introspect resolution |
| `wiki_plan` / `wiki_mark_task` | Open/resume the generation run, resolve tasks (auto-completed by put/refresh) |
| `wiki_playbooks` | Page-type guides (source of each plan task's `instructions`) |
| `wiki_auto` | Maintenance orchestration: zero-token self-heal + lint + plan |

## Workflow

```
discover modules -> read source -> put_page (with citations) -> [code evolves] -> check / auto / refresh
        ├─ blob unchanged     -> stays verified
        ├─ lines drifted      -> projected, verified + audit note
        ├─ git mv             -> new path found by blob content addressing, verified
        ├─ mv + small edit    -> similarity rename + projection, verified
        └─ content lost       -> draft; the agent rewrites
```

- Pages and `.wiki/` are committed together, shared with the team, reviewable in PRs; `runs.json` lives in git too, so interrupted generation resumes automatically via `wiki_plan`
- Plan tasks carry `suggested_type` and playbook `instructions` (which sections a page of that type needs, which questions it must answer), so any producer agent writes structurally consistent pages without hidden prompts; extend types via `.wiki/wiki.toml [page_types.<name>]`; unregistered types are flagged by `lint`
- `loredex lint` belongs in CI; page ids support hierarchy (`architecture/overview`), `--type` labels the kind
- `loredex agents-md` keeps a marked block (`<!-- BEGIN loredex -->`) in AGENTS.md/CLAUDE.md so agents entering the repo discover the wiki and its freshness protocol — only the marked region is ever rewritten; `loredex config --language zh` switches the locale of all generated skeleton text
- `loredex import` migrates an existing openwiki / Google OKF bundle; the trust model survives the move — pages without resolvable citations land as `draft` and get backfilled via `plan`

## Architecture

```
crates/
├── loredex-core    data model (Status/Evidence/PageMeta), Markdown parsing,
│                   citation parser, tokenizer + inverted index (no IO)
├── loredex-git     read-only git2 wrapper: HEAD blobs, line ranges, tree diffs,
│                   direct object-db access, rename detection
├── loredex-engine  business layer: check_page (dual staleness verdicts),
│                   verify_page (sha1), drift projection (similar), put_page,
│                   modules discovery, plan/run ledger, index, lint, auto,
│                   wiki.toml config ([wiki] language), AGENTS.md managed
│                   block, openwiki/OKF importer
├── loredex-cli     clap entry (loredex)
└── loredex-mcp     rmcp 3.8 stdio server (loredex-mcp), lib+bin for in-process tests
```

Key implementation details:

- Staleness = blob comparison **and** pathspec diff from the generation commit to HEAD; an unresolvable base commit is itself a stale verdict
- Drift projection maps old lines to new (deleted lines anchor to the nearest surviving line, collapsed ranges keep width); a projected snippet is adopted only at similarity ≥ 0.6 — otherwise it honestly reports Lost rather than mis-adopting look-alikes
- Citation validation happens at write time: the path must exist at HEAD, malformed ranges are rejected
- Repository resolution is capability-gated (roots are only requested from clients declaring them) with a 3 s timeout — no client can wedge a session
- `git2::Repository` is `!Sync`, so the repo instance lives inside the mutex; async resolution only *ensures* the slot and tools borrow it synchronously (single-level lock discipline)

## Known boundaries

- Content quality depends on the authoring agent — the tooling guarantees "cited and uncorrupted citations", not that an explanation is correct
- check/verify/search load every page; fine at thousands of pages, a persistent index (not built) would be needed at tens of thousands
- Inline body markers are not rewritten by refresh projection (frontmatter is authoritative; `lint` detects the drift)
- For non-ASCII paths, set `core.quotePath=false`

## Roadmap

- Incremental page rebuilds per commit range; `post-commit` hook integration
- Rewriting inline body markers during projection
- Persistent search index in `.wiki/.cache/` for very large wikis

## License

GNU GPL v3.0 or later (see [LICENSE](LICENSE)). Wiki data under `.wiki/` is unaffected — it is just Markdown in your repository.

## Development

```bash
cargo nextest run --workspace          # 65 tests (unit + git-fixture integration + in-process MCP protocol)
cargo llvm-cov nextest --workspace --summary-only   # coverage baseline (CI gates at 70% lines)
```

- Toolchain: cargo-nextest (one process per test, so env manipulation is safe; `.config/nextest.toml` sets a 15 s slow-timeout so deadlocks fail instead of hanging) + cargo-llvm-cov
- Current coverage: ~75% regions / ~78% lines overall; core layers 80–98%, `loredex-mcp` 77% (in-process duplex client↔server exercises the real protocol stack: four-layer resolution, roots capability gate, full lifecycle), CLI kept as a thin argument mapping
- Releases: cargo-dist (`dist-workspace.toml`) builds 5 targets + shell/PowerShell installers on tag `v*`
