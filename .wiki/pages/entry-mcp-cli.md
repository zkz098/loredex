---
id: entry-mcp-cli
title: 'Entry layer: MCP server and CLI'
status: verified
generated_at: 2026-11-01T03:15:36Z
commit: d0155cf4e48bdf51a9efb9b1cfbaf21320314f23
evidence:
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 17
  - 27
  blob: ed2c43380383c1d1aee829a667556ede7d3a8657
  snippet_sha1: 053299532073ef61304df7650834b11d98a02ac6
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 28
  - 31
  blob: ed2c43380383c1d1aee829a667556ede7d3a8657
  snippet_sha1: 81c5966e09ad7446dd1f3792a23f07cb84f93273
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 36
  - 38
  blob: ed2c43380383c1d1aee829a667556ede7d3a8657
  snippet_sha1: 2824fc12ebf3eafb69b6eea2cbfad7ba61188f84
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 45
  - 48
  blob: ed2c43380383c1d1aee829a667556ede7d3a8657
  snippet_sha1: 840f7780ad60f122d8c3fcf25a33c36896d249ff
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 1096
  - 1122
  blob: ed2c43380383c1d1aee829a667556ede7d3a8657
  snippet_sha1: dea8830caf9b6416689046b766f257fd5127a744
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 1151
  - 1172
  blob: ed2c43380383c1d1aee829a667556ede7d3a8657
  snippet_sha1: f40a93d1b1f5b60a16c5fe2a1c28f2ebe4166b2d
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 791
  - 810
  blob: ed2c43380383c1d1aee829a667556ede7d3a8657
  snippet_sha1: 2483bbee316979f731c751e7011ff85d7b01f2c4
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 604
  - 620
  blob: ed2c43380383c1d1aee829a667556ede7d3a8657
  snippet_sha1: edae31610f1a005652a6f5cd7a29a94e50071ba5
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 894
  - 910
  blob: ed2c43380383c1d1aee829a667556ede7d3a8657
  snippet_sha1: 04e09de6b2cf2b73a5ba4ec4641335f113160d10
- path: crates/loredex-cli/src/main.rs
  lines:
  - 29
  - 70
  blob: ee39b2bb45a936c44a4cb13ce936e990cebdc605
  snippet_sha1: 5e818760eaacafafe4a191cae1fc4267331f7a49
- path: crates/loredex-cli/src/main.rs
  lines:
  - 724
  - 726
  blob: ee39b2bb45a936c44a4cb13ce936e990cebdc605
  snippet_sha1: 98567e54d7ba82b1d391c916db0d975894787331
type: system
---
# Entry layer: MCP server and CLI

The MCP server (rmcp, stdio; lib+bin split so tests drive the real protocol in-process) advertises a behavioral contract at `initialize`: check freshness before trusting, treat stale as unverified, cite every claim, secret-gated writes [[cite:crates/loredex-mcp/src/lib.rs:L17-L27]]; the contract also tells the agent to capture what it cannot cite yet through `wiki_note` as an explicitly unverified `draft` instead of guessing [[cite:crates/loredex-mcp/src/lib.rs:L28-L31]], and to consolidate duplicated notes/rules with `wiki_link_suggestions` / `wiki_organize` [[cite:crates/loredex-mcp/src/lib.rs:L36-L38]]. The served repository is resolved lazily in four layers — pinned `--repo`, `LOREDEX_REPO`, client MCP roots, process cwd [[cite:crates/loredex-mcp/src/lib.rs:L45-L48]].

All tools return `Json<T>` structured content; key fields include `stale_now`, `matches_head`, `projectable`. `wiki_put_page` rejects an uncited body as `invalid_request` [[cite:crates/loredex-mcp/src/lib.rs:L1096-L1122]]; `wiki_note` accepts one and reports `status: draft` [[cite:crates/loredex-mcp/src/lib.rs:L1151-L1172]]; `wiki_check_freshness` gives per-page staleness with projectable evidence [[cite:crates/loredex-mcp/src/lib.rs:L791-L810]]; `wiki_auto` orchestrates zero-token self-heal, lint and the remaining worklist [[cite:crates/loredex-mcp/src/lib.rs:L604-L620]]; `wiki_get_evidence` expands a citation to the actual file content at HEAD [[cite:crates/loredex-mcp/src/lib.rs:L894-L910]].

Tool methods take `Peer<RoleServer>` via macro injection and borrow the repository from `Mutex<RepoSlot>` with single-level lock discipline, because `git2::Repository` is `!Sync`. The CLI is a thin argument mapping onto the same engine surface — init / put / note / organize / check / plan / index / lint / auto / task [[cite:crates/loredex-cli/src/main.rs:L29-L70]] — and `check`, `lint`, `auto` exit non-zero only on defects, so advisory curation hints never break a build [[cite:crates/loredex-cli/src/main.rs:L724-L726]].

Why the exit-code split exists: [[wiki:rule-advisory-never-fails-ci]]. How the pages themselves are stored: [[wiki:decision-no-database]].
