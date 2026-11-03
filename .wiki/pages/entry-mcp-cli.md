---
id: entry-mcp-cli
title: 'Entry layer: MCP server and CLI'
status: verified
generated_at: 2026-11-03T13:20:21Z
commit: 67da1675a709e5972e22a3f53797fe275070907e
evidence:
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 17
  - 26
  blob: c80142e0bcb6707abf0119392942b2c83d283ba3
  snippet_sha1: dc10c3a9193f10dc416ed4b9b3131aacb7a45e76
  note: drift-projected L17-L27@ed2c4338 -> L17-L26 (similarity 0.82)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 30
  - 33
  blob: c80142e0bcb6707abf0119392942b2c83d283ba3
  snippet_sha1: 81c5966e09ad7446dd1f3792a23f07cb84f93273
  note: drift-projected L28-L31@ed2c4338 -> L30-L33 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 38
  - 40
  blob: c80142e0bcb6707abf0119392942b2c83d283ba3
  snippet_sha1: 2824fc12ebf3eafb69b6eea2cbfad7ba61188f84
  note: drift-projected L36-L38@ed2c4338 -> L38-L40 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 52
  - 55
  blob: c80142e0bcb6707abf0119392942b2c83d283ba3
  snippet_sha1: 840f7780ad60f122d8c3fcf25a33c36896d249ff
  note: drift-projected L45-L48@ed2c4338 -> L52-L55 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 1228
  - 1256
  blob: c80142e0bcb6707abf0119392942b2c83d283ba3
  snippet_sha1: 578acf07ed012e9ee1b0bd3afa8295d0fa090e92
  note: drift-projected L1096-L1122@ed2c4338 -> L1228-L1256 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 1286
  - 1316
  blob: c80142e0bcb6707abf0119392942b2c83d283ba3
  snippet_sha1: 2121dda6cfeb340bd04f4b034e7141e817585c28
  note: drift-projected L1151-L1172@ed2c4338 -> L1286-L1316 (similarity 0.91)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 923
  - 942
  blob: c80142e0bcb6707abf0119392942b2c83d283ba3
  snippet_sha1: 2483bbee316979f731c751e7011ff85d7b01f2c4
  note: drift-projected L791-L810@ed2c4338 -> L923-L942 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 729
  - 745
  blob: c80142e0bcb6707abf0119392942b2c83d283ba3
  snippet_sha1: edae31610f1a005652a6f5cd7a29a94e50071ba5
  note: drift-projected L604-L620@ed2c4338 -> L729-L745 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 1026
  - 1042
  blob: c80142e0bcb6707abf0119392942b2c83d283ba3
  snippet_sha1: 04e09de6b2cf2b73a5ba4ec4641335f113160d10
  note: drift-projected L894-L910@ed2c4338 -> L1026-L1042 (similarity 1.00)
- path: crates/loredex-cli/src/main.rs
  lines:
  - 29
  - 82
  blob: 39e2b9b14b2d9aa74ee28b53f1f6db44056d8675
  snippet_sha1: a41816cfce0b7a74bceeac51aa3d220d111fa0b5
  note: drift-projected L29-L70@ee39b2bb -> L29-L82 (similarity 1.00)
- path: crates/loredex-cli/src/main.rs
  lines:
  - 877
  - 879
  blob: 39e2b9b14b2d9aa74ee28b53f1f6db44056d8675
  snippet_sha1: 98567e54d7ba82b1d391c916db0d975894787331
  note: drift-projected L724-L726@ee39b2bb -> L877-L879 (similarity 1.00)
type: system
---
# Entry layer: MCP server and CLI

The MCP server (rmcp, stdio; lib+bin split so tests drive the real protocol in-process) advertises a behavioral contract at `initialize`: check freshness before trusting, treat stale as unverified, cite every claim, secret-gated writes [[cite:crates/loredex-mcp/src/lib.rs:L17-L27]]; the contract also tells the agent to capture what it cannot cite yet through `wiki_note` as an explicitly unverified `draft` instead of guessing [[cite:crates/loredex-mcp/src/lib.rs:L28-L31]], and to consolidate duplicated notes/rules with `wiki_link_suggestions` / `wiki_organize` [[cite:crates/loredex-mcp/src/lib.rs:L36-L38]]. The served repository is resolved lazily in four layers — pinned `--repo`, `LOREDEX_REPO`, client MCP roots, process cwd [[cite:crates/loredex-mcp/src/lib.rs:L45-L48]].

All tools return `Json<T>` structured content; key fields include `stale_now`, `matches_head`, `projectable`. `wiki_put_page` rejects an uncited body as `invalid_request` [[cite:crates/loredex-mcp/src/lib.rs:L1096-L1122]]; `wiki_note` accepts one and reports `status: draft` [[cite:crates/loredex-mcp/src/lib.rs:L1151-L1172]]; `wiki_check_freshness` gives per-page staleness with projectable evidence [[cite:crates/loredex-mcp/src/lib.rs:L791-L810]]; `wiki_auto` orchestrates zero-token self-heal, lint and the remaining worklist [[cite:crates/loredex-mcp/src/lib.rs:L604-L620]]; `wiki_get_evidence` expands a citation to the actual file content at HEAD [[cite:crates/loredex-mcp/src/lib.rs:L894-L910]].

Tool methods take `Peer<RoleServer>` via macro injection and borrow the repository from `Mutex<RepoSlot>` with single-level lock discipline, because `git2::Repository` is `!Sync`. The CLI is a thin argument mapping onto the same engine surface — init / put / note / organize / check / plan / index / lint / auto / task [[cite:crates/loredex-cli/src/main.rs:L29-L70]] — and `check`, `lint`, `auto` exit non-zero only on defects, so advisory curation hints never break a build [[cite:crates/loredex-cli/src/main.rs:L724-L726]].

Why the exit-code split exists: [[wiki:rule-advisory-never-fails-ci]]. How the pages themselves are stored: [[wiki:decision-no-database]].
