---
id: entry-mcp-cli
title: 'Entry layer: MCP server and CLI'
status: verified
generated_at: 2026-11-10T15:16:08Z
commit: 6802401ccb4cc357a9c5f870b52adaab11715e4b
evidence:
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 17
  - 27
  blob: ef81cb0099e3e22883362d265b3cda16e809e506
  snippet_sha1: 46189913e01bbbf9f63d23ac1ff4f3e0bc45327f
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 28
  - 31
  blob: ef81cb0099e3e22883362d265b3cda16e809e506
  snippet_sha1: 9d5857d91e98e8adaf77d8b1838c8908cb831255
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 36
  - 38
  blob: ef81cb0099e3e22883362d265b3cda16e809e506
  snippet_sha1: aabba286c4cab5de591ffd4ef97cc7b70b9485a2
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 45
  - 48
  blob: ef81cb0099e3e22883362d265b3cda16e809e506
  snippet_sha1: 30040ffbcd3c1d92fc87484d89161c53a00e0f90
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 1096
  - 1122
  blob: ef81cb0099e3e22883362d265b3cda16e809e506
  snippet_sha1: 293da79ca9111e3cc1290500fd983b23556474ae
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 1151
  - 1172
  blob: ef81cb0099e3e22883362d265b3cda16e809e506
  snippet_sha1: 7271b4dcfad5851e2d4b01d1631963c09e9b48f4
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 791
  - 810
  blob: ef81cb0099e3e22883362d265b3cda16e809e506
  snippet_sha1: 696aa5c34978d5e236627d380770e727d02676d0
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 604
  - 620
  blob: ef81cb0099e3e22883362d265b3cda16e809e506
  snippet_sha1: cee916abfdb1106f04b628eeef4ad2279050d991
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 894
  - 910
  blob: ef81cb0099e3e22883362d265b3cda16e809e506
  snippet_sha1: c0c31858a4c001f3321c322ad31813bd054f5b2f
- path: crates/loredex-cli/src/main.rs
  lines:
  - 29
  - 70
  blob: 42b915e5dbe33c3ad32c286d6e4609556b6dcb8e
  snippet_sha1: 383c48f4d66d06e35e7f7b9a99e717039875971e
- path: crates/loredex-cli/src/main.rs
  lines:
  - 724
  - 726
  blob: 42b915e5dbe33c3ad32c286d6e4609556b6dcb8e
  snippet_sha1: 351d842403979bd7bf0138dc663bf63746804db0
type: system
---
# Entry layer: MCP server and CLI

The MCP server (rmcp, stdio; lib+bin split so tests drive the real protocol in-process) advertises a behavioral contract at `initialize`: check freshness before trusting, treat stale as unverified, cite every claim, secret-gated writes [[cite:crates/loredex-mcp/src/lib.rs:L17-L27]]; the contract also tells the agent to capture what it cannot cite yet through `wiki_note` as an explicitly unverified `draft` instead of guessing [[cite:crates/loredex-mcp/src/lib.rs:L28-L31]], and to consolidate duplicated notes/rules with `wiki_link_suggestions` / `wiki_organize` [[cite:crates/loredex-mcp/src/lib.rs:L36-L38]]. The served repository is resolved lazily in four layers — pinned `--repo`, `LOREDEX_REPO`, client MCP roots, process cwd [[cite:crates/loredex-mcp/src/lib.rs:L45-L48]].

All tools return `Json<T>` structured content; key fields include `stale_now`, `matches_head`, `projectable`. `wiki_put_page` rejects an uncited body as `invalid_request` [[cite:crates/loredex-mcp/src/lib.rs:L1096-L1122]]; `wiki_note` accepts one and reports `status: draft` [[cite:crates/loredex-mcp/src/lib.rs:L1151-L1172]]; `wiki_check_freshness` gives per-page staleness with projectable evidence [[cite:crates/loredex-mcp/src/lib.rs:L791-L810]]; `wiki_auto` orchestrates zero-token self-heal, lint and the remaining worklist [[cite:crates/loredex-mcp/src/lib.rs:L604-L620]]; `wiki_get_evidence` expands a citation to the actual file content at HEAD [[cite:crates/loredex-mcp/src/lib.rs:L894-L910]].

Tool methods take `Peer<RoleServer>` via macro injection and borrow the repository from `Mutex<RepoSlot>` with single-level lock discipline, because `git2::Repository` is `!Sync`. The CLI is a thin argument mapping onto the same engine surface — init / put / note / organize / check / plan / index / lint / auto / task [[cite:crates/loredex-cli/src/main.rs:L29-L70]] — and `check`, `lint`, `auto` exit non-zero only on defects, so advisory curation hints never break a build [[cite:crates/loredex-cli/src/main.rs:L724-L726]].

Why the exit-code split exists: [[wiki:rule-advisory-never-fails-ci]]. How the pages themselves are stored: [[wiki:decision-no-database]].
