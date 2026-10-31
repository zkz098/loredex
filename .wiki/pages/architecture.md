---
id: architecture
title: Architecture
status: verified
generated_at: 2026-10-31T12:43:23Z
commit: 1d2863aaea11c84a442cdbf282fb7da8211225cd
evidence:
- path: Cargo.toml
  lines:
  - 2
  - 9
  blob: c548aeb1cd615b0910f518e0a0fcd1b5d747b566
  snippet_sha1: cdb93553223a82b1c268fcf7d31a0b5c0dd928c0
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 274
  - 286
  blob: f716c3c7fc54d6c9bfa7dea6970eaddf0b4eadf7
  snippet_sha1: 02ad668b7bdf2cb5db082b398c984b1233f151ee
- path: crates/loredex-core/src/page.rs
  lines:
  - 29
  - 40
  blob: e814a40a848673def3eb54918a306230323ab57e
  snippet_sha1: c29606ed08aec635918031cfd27810613803de0c
type: architecture
---
# Architecture

Loredex is a cargo workspace with a strict downward dependency flow: loredex-core (pure data model) <- loredex-git (read-only git wrapper) <- loredex-engine (business logic) <- two entry points, CLI and MCP [[cite:Cargo.toml:L2-L9]].

Storage is plain Markdown: pages live in the repository's `.wiki/pages/*.md`, with machine-readable evidence embedded as YAML frontmatter; `.wiki/.cache/` is a gitignored, disposable directory — there is no authoritative database [[cite:crates/loredex-engine/src/lib.rs:L274-L286]].

The trust model: every evidence item pins a `(path, line range, git blob sha at record time, sha1 of the snippet content)` quadruple [[cite:crates/loredex-core/src/page.rs:L29-L40]]. Staleness is decided by comparing blobs against HEAD cross-checked with a git diff; verification recomputes the content hash of the cited range directly, so trust never rests on commit ids alone.
