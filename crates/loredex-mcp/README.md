# loredex-mcp

The `loredex-mcp` server: serves an [evidence-backed repository wiki](https://github.com/zkz098/loredex) to AI coding agents over the MCP stdio transport ([rmcp](https://crates.io/crates/rmcp) v3).

**Zero-config**: one global entry serves any repository. The target resolves lazily in four layers — `--repo` flag (pinned) → `LOREDEX_REPO` env → client MCP roots (capability-gated, 3 s timeout) → process working directory; if every layer fails, tools return a guided error and the agent self-heals via `wiki_attach_repo`.

```json
{
  "mcpServers": {
    "loredex": { "command": "loredex-mcp" }
  }
}
```

Tools (all return structured `structuredContent`, e.g. `stale_now`, `matches_head`, `projectable`):

| Tool | Purpose |
|---|---|
| `wiki_check_freshness` | Full git staleness scan + projectable-item annotation |
| `wiki_list` / `wiki_read` | Page inventory / read a page (stale pages carry a warning) |
| `wiki_search` | Tokenized search (camelCase, snake_case, CJK bigrams, prefix) |
| `wiki_get_evidence` | Expand one citation to the actual file content at HEAD |
| `wiki_list_modules` | Discover authoring targets with citation coverage |
| `wiki_put_page` | Store agent-authored content (zero citations = rejected) |
| `wiki_refresh_page` | Drift-projection refresh with per-citation outcomes |
| `wiki_attach_repo` / `wiki_current_repo` | Attach/switch repository, introspect resolution |
| `wiki_plan` / `wiki_mark_task` | Open/resume the generation run, resolve tasks |
| `wiki_playbooks` | Page-type guides (instructions carried by each plan task) |
| `wiki_auto` | Maintenance orchestration: zero-token self-heal + lint + plan |

The server advertises a behavioral contract at `initialize`: check freshness before trusting, stale means unverified, drift goes through refresh, citations are auditable.

Split lib + bin: `WikiServer::new_open()` (lazy resolution) and `WikiServer::pinned(GitRepo)` let integration tests drive the real protocol stack in-process. All business logic comes from [`loredex-engine`](https://github.com/zkz098/loredex/blob/master/crates/loredex-engine/README.md).

License: GPL-3.0-or-later.
