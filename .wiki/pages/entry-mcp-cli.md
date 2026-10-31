---
id: entry-mcp-cli
title: 'Entry layer: MCP server and CLI'
status: verified
generated_at: 2026-10-31T12:43:23Z
commit: 1d2863aaea11c84a442cdbf282fb7da8211225cd
evidence:
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 16
  - 37
  blob: 144a9b3398f5a958b27e4699ed7332c3e9bff3e3
  snippet_sha1: 18d2537d2cd010407b2fb9f375a35c9f99192abe
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 463
  - 500
  blob: 144a9b3398f5a958b27e4699ed7332c3e9bff3e3
  snippet_sha1: dcad0e03b6d42de21c5adbc0454da6ac46edd828
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 575
  - 629
  blob: 144a9b3398f5a958b27e4699ed7332c3e9bff3e3
  snippet_sha1: eed71446943545c1e9db446ff18484bbe23657e0
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 503
  - 538
  blob: 144a9b3398f5a958b27e4699ed7332c3e9bff3e3
  snippet_sha1: 1af8dbde32efea57e4df3d685780f6367e4e2f81
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 373
  - 422
  blob: 144a9b3398f5a958b27e4699ed7332c3e9bff3e3
  snippet_sha1: fea225fdd09429e69d02d171ce7277f166baa666
- path: crates/loredex-cli/src/main.rs
  lines:
  - 25
  - 75
  blob: 7800424e8e44e00edc78fc7265aedf01f4a33ecd
  snippet_sha1: 3aacd3c3ea97884790fb1c66af586fbdc403a1a6
type: system
---
# Entry layer: MCP server and CLI

The MCP server (rmcp 3.8, stdio; lib+bin split for in-process tests) advertises a behavioral contract at initialize: check freshness first, treat stale as unverified, project drift through refresh, audit via citations, and the four-layer repository resolution order [[cite:crates/loredex-mcp/src/lib.rs:L16-L37]].

All tools return `Json<T>` structured content; key fields include `stale_now`, `matches_head`, `projectable`. The write tool `wiki_put_page` rejects uncited bodies as invalid_request [[cite:crates/loredex-mcp/src/lib.rs:L463-L500]]; `wiki_check_freshness` reports per-page staleness with projectable evidence and playbook-carrying plan tasks [[cite:crates/loredex-mcp/src/lib.rs:L575-L629]]; `wiki_auto` orchestrates zero-token self-heal, lint and the remaining worklist [[cite:crates/loredex-mcp/src/lib.rs:L503-L538]]; `wiki_get_evidence` expands a citation to HEAD content [[cite:crates/loredex-mcp/src/lib.rs:L373-L422]].

Tool methods take `Peer<RoleServer>` via macro injection and borrow the repo from `Mutex<RepoSlot>` with single-level lock discipline (git2::Repository is `!Sync`). The CLI's subcommand surface — init/put/check/plan/index/lint/auto/task — is a thin mapping onto loredex-engine [[cite:crates/loredex-cli/src/main.rs:L25-L75]]; `check` and `auto` exit non-zero on findings so hooks and CI can gate on them.
