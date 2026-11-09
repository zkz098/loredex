---
id: entry-mcp-cli
title: 'Entry layer: MCP server and CLI'
status: verified
generated_at: 2026-11-09T14:37:56Z
commit: 8ce30c67ff3080a3c5cde0325e337dee4b5b2056
evidence:
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 23
  - 32
  blob: 6f1ff0c44e3f66f5a16f592dcddc4b54bb61fad1
  snippet_sha1: dc10c3a9193f10dc416ed4b9b3131aacb7a45e76
  note: drift-projected L23-L32@d220dafd -> L23-L32 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 36
  - 39
  blob: 6f1ff0c44e3f66f5a16f592dcddc4b54bb61fad1
  snippet_sha1: 81c5966e09ad7446dd1f3792a23f07cb84f93273
  note: drift-projected L36-L39@d220dafd -> L36-L39 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 44
  - 46
  blob: 6f1ff0c44e3f66f5a16f592dcddc4b54bb61fad1
  snippet_sha1: 2824fc12ebf3eafb69b6eea2cbfad7ba61188f84
  note: drift-projected L44-L46@d220dafd -> L44-L46 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 61
  - 64
  blob: 6f1ff0c44e3f66f5a16f592dcddc4b54bb61fad1
  snippet_sha1: 840f7780ad60f122d8c3fcf25a33c36896d249ff
  note: drift-projected L61-L64@d220dafd -> L61-L64 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 1237
  - 1265
  blob: 6f1ff0c44e3f66f5a16f592dcddc4b54bb61fad1
  snippet_sha1: 578acf07ed012e9ee1b0bd3afa8295d0fa090e92
  note: drift-projected L1237-L1265@d220dafd -> L1237-L1265 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 1295
  - 1325
  blob: 6f1ff0c44e3f66f5a16f592dcddc4b54bb61fad1
  snippet_sha1: 2121dda6cfeb340bd04f4b034e7141e817585c28
  note: drift-projected L1295-L1325@d220dafd -> L1295-L1325 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 932
  - 951
  blob: 6f1ff0c44e3f66f5a16f592dcddc4b54bb61fad1
  snippet_sha1: 2483bbee316979f731c751e7011ff85d7b01f2c4
  note: drift-projected L932-L951@d220dafd -> L932-L951 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 738
  - 754
  blob: 6f1ff0c44e3f66f5a16f592dcddc4b54bb61fad1
  snippet_sha1: edae31610f1a005652a6f5cd7a29a94e50071ba5
  note: drift-projected L738-L754@d220dafd -> L738-L754 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 1035
  - 1051
  blob: 6f1ff0c44e3f66f5a16f592dcddc4b54bb61fad1
  snippet_sha1: 04e09de6b2cf2b73a5ba4ec4641335f113160d10
  note: drift-projected L1035-L1051@d220dafd -> L1035-L1051 (similarity 1.00)
- path: crates/loredex-cli/src/main.rs
  lines:
  - 29
  - 82
  blob: 94f2e96933ee17b58a472f7c0bbb45341b978cf9
  snippet_sha1: a41816cfce0b7a74bceeac51aa3d220d111fa0b5
  note: drift-projected L29-L82@5da7033e -> L29-L82 (similarity 1.00)
- path: crates/loredex-cli/src/main.rs
  lines:
  - 894
  - 896
  blob: 94f2e96933ee17b58a472f7c0bbb45341b978cf9
  snippet_sha1: 98567e54d7ba82b1d391c916db0d975894787331
  note: drift-projected L891-L893@5da7033e -> L894-L896 (similarity 1.00)
type: system
---
# Entry layer: MCP server and CLI

The MCP server (rmcp, stdio; lib+bin split so tests drive the real protocol in-process) advertises a behavioral contract at `initialize`: check freshness before trusting, treat stale as unverified, cite every claim, secret-gated writes [[cite:crates/loredex-mcp/src/lib.rs:L17-L27]]; the contract also tells the agent to capture what it cannot cite yet through `wiki_note` as an explicitly unverified `draft` instead of guessing [[cite:crates/loredex-mcp/src/lib.rs:L28-L31]], and to consolidate duplicated notes/rules with `wiki_link_suggestions` / `wiki_organize` [[cite:crates/loredex-mcp/src/lib.rs:L36-L38]]. The served repository is resolved lazily in four layers — pinned `--repo`, `LOREDEX_REPO`, client MCP roots, process cwd [[cite:crates/loredex-mcp/src/lib.rs:L45-L48]].

All tools return `Json<T>` structured content; key fields include `stale_now`, `matches_head`, `projectable`. `wiki_put_page` rejects an uncited body as `invalid_request` [[cite:crates/loredex-mcp/src/lib.rs:L1096-L1122]]; `wiki_note` accepts one and reports `status: draft` [[cite:crates/loredex-mcp/src/lib.rs:L1151-L1172]]; `wiki_check_freshness` gives per-page staleness with projectable evidence [[cite:crates/loredex-mcp/src/lib.rs:L791-L810]]; `wiki_auto` orchestrates zero-token self-heal, lint and the remaining worklist [[cite:crates/loredex-mcp/src/lib.rs:L604-L620]]; `wiki_get_evidence` expands a citation to the actual file content at HEAD [[cite:crates/loredex-mcp/src/lib.rs:L894-L910]].

Tool methods take `Peer<RoleServer>` via macro injection and borrow the repository from `Mutex<RepoSlot>` with single-level lock discipline, because `git2::Repository` is `!Sync`. The CLI is a thin argument mapping onto the same engine surface — init / put / note / organize / check / plan / index / lint / auto / task [[cite:crates/loredex-cli/src/main.rs:L29-L70]] — and `check`, `lint`, `auto` exit non-zero only on defects, so advisory curation hints never break a build [[cite:crates/loredex-cli/src/main.rs:L724-L726]].

Why the exit-code split exists: [[wiki:rule-advisory-never-fails-ci]]. How the pages themselves are stored: [[wiki:decision-no-database]].
