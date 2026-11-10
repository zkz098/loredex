---
id: architecture
title: Architecture
status: verified
generated_at: 2026-11-10T15:16:07Z
commit: 6802401ccb4cc357a9c5f870b52adaab11715e4b
evidence:
- path: Cargo.toml
  lines:
  - 2
  - 9
  blob: 7d094478474dc090a6ae2236019b200073f2f3ed
  snippet_sha1: cdb93553223a82b1c268fcf7d31a0b5c0dd928c0
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 274
  - 286
  blob: 97ac2a10ec698dc5c7c31964bc053faff69eae61
  snippet_sha1: 2095a8e88454748d1635300b9c2fb59aa0e455ed
- path: crates/loredex-core/src/page.rs
  lines:
  - 29
  - 40
  blob: 0827a471fbe63039bf1155430c66e0c168085ccf
  snippet_sha1: 282e1108d9327561c48e0d3322d153cd1a81fa99
type: architecture
---
# Architecture

Loredex is a cargo workspace with a strict downward dependency flow: loredex-core (pure data model) <- loredex-git (read-only git wrapper) <- loredex-engine (business logic) <- two entry points, CLI and MCP [[cite:Cargo.toml:L2-L9]].

Storage is plain Markdown: pages live in the repository's `.wiki/pages/*.md`, with machine-readable evidence embedded as YAML frontmatter; `.wiki/.cache/` is a gitignored, disposable directory — there is no authoritative database [[cite:crates/loredex-engine/src/lib.rs:L274-L286]].

The trust model: every evidence item pins a `(path, line range, git blob sha at record time, sha1 of the snippet content)` quadruple [[cite:crates/loredex-core/src/page.rs:L29-L40]]. Staleness is decided by comparing blobs against HEAD cross-checked with a git diff; verification recomputes the content hash of the cited range directly, so trust never rests on commit ids alone.
