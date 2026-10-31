# loredex-cli

The `loredex` command: maintain an [evidence-backed repository wiki](https://github.com/zkz098/loredex) for humans and AI coding agents. Thin clap argument mapping over [`loredex-engine`](https://github.com/zkz098/loredex/blob/master/crates/loredex-engine/README.md) — all logic lives there.

```bash
cargo install loredex-cli
# or grab a prebuilt binary from GitHub Releases (cargo binstall loredex-cli)
```

```bash
cd /path/to/your-repo    # must have at least one commit
loredex init             # creates .wiki/pages/ + wiki.toml
loredex put core --title "Core layer" --body notes.md
loredex check --mark     # staleness scan; non-zero exit = stale found (CI-friendly)
```

Subcommands:

| Command | Purpose |
|---|---|
| `init` | Create the `.wiki/` layout and `wiki.toml` |
| `put <id> --title --body <file\|->` | Store a page; bodies without resolvable citations are rejected |
| `list` / `show <id>` / `search <query>` | Inventory, read, tokenized search (camel/snake/CJK) |
| `check [--mark]` | Git-driven staleness scan (exit 1 on stale) |
| `verify <id>` / `refresh <id>` | Content-hash verification / drift-projection refresh |
| `modules` | Discover authoring targets with citation coverage |
| `plan [--json --reset --close]` / `task <id> <status>` | Generation worklist (New + Refresh) persisted to `.wiki/runs.json` |
| `index` | Regenerate `index.md` + `quickstart.md` reading paths |
| `auto [--json]` | One-call maintenance: self-heal → lint → plan for the rest |
| `lint [--json]` | Broken links / stale-but-verified / evidence drift / unknown type (exit 1 on findings) |
| `playbooks` | Page-type authoring guides |
| `config [--language en\|zh]` | Locale for tool-generated skeleton text |
| `agents-md [--check --remove --file]` | Managed discovery block for AGENTS.md/CLAUDE.md |
| `import <dir> [--format openwiki]` | Migrate an openwiki / Google OKF bundle |
| `templates [--out contrib]` | Export GitHub Action + post-merge hook templates |

A global `--repo` flag targets any repository. Page bodies pin every claim to source lines:

```markdown
serve() handles requests in a loop [[cite:src/api.rs:L1-L5]]; main initializes the db first [[cite:src/main.rs:L3-L6]].
```

License: GPL-3.0-or-later. Wiki data under `.wiki/` is unaffected — it is just Markdown in your repository.
