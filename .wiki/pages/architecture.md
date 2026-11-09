---
id: architecture
title: Architecture
status: verified
generated_at: 2026-11-09T14:37:56Z
commit: 8ce30c67ff3080a3c5cde0325e337dee4b5b2056
evidence:
- path: Cargo.toml
  lines:
  - 2
  - 9
  blob: d1eb7392ec4b86d978166cdb464a9bc0fba4b654
  snippet_sha1: cdb93553223a82b1c268fcf7d31a0b5c0dd928c0
  note: drift-projected L2-L9@a731b14a -> L2-L9 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 439
  - 451
  blob: f53a1400b4d740442891fa8c0e1bed0a433ba83f
  snippet_sha1: 02ad668b7bdf2cb5db082b398c984b1233f151ee
  note: drift-projected L384-L396@c2376300 -> L439-L451 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 30
  - 43
  blob: 0827a471fbe63039bf1155430c66e0c168085ccf
  snippet_sha1: 7e692f17d14072f8df6a7c414f18fe92498379c1
  note: drift-projected L30-L41@04a91928 -> L30-L43 (similarity 1.00)
type: architecture
---
# Architecture

Loredex is a cargo workspace with a strict downward dependency flow: loredex-core (pure data model) <- loredex-git (read-only git wrapper) <- loredex-engine (business logic) <- two entry points, CLI and MCP [[cite:Cargo.toml:L2-L9]].

Storage is plain Markdown: pages live in the repository's `.wiki/pages/*.md`, with machine-readable evidence embedded as YAML frontmatter; `.wiki/.cache/` is a gitignored, disposable directory — there is no authoritative database [[cite:crates/loredex-engine/src/lib.rs:L274-L286]].

The trust model: every evidence item pins a `(path, line range, git blob sha at record time, sha1 of the snippet content)` quadruple [[cite:crates/loredex-core/src/page.rs:L29-L40]]. Staleness is decided by comparing blobs against HEAD cross-checked with a git diff; verification recomputes the content hash of the cited range directly, so trust never rests on commit ids alone.
