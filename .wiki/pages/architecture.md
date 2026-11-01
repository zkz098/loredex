---
id: architecture
title: Architecture
status: verified
generated_at: 2026-11-01T03:14:48Z
commit: d0155cf4e48bdf51a9efb9b1cfbaf21320314f23
evidence:
- path: Cargo.toml
  lines:
  - 2
  - 9
  blob: abf859da79ebc5ef0865f2c4b0342bc9cee29cca
  snippet_sha1: cdb93553223a82b1c268fcf7d31a0b5c0dd928c0
  note: drift-projected L2-L9@c548aeb1 -> L2-L9 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 356
  - 368
  blob: bc899494cd898a0e2bbe24cf06a4fe1da946508d
  snippet_sha1: 02ad668b7bdf2cb5db082b398c984b1233f151ee
  note: drift-projected L356-L368@babdf319 -> L356-L368 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 30
  - 41
  blob: 0a5365724b11730690a2a857196929b865e7eb43
  snippet_sha1: c29606ed08aec635918031cfd27810613803de0c
  note: drift-projected L29-L40@e814a40a -> L30-L41 (similarity 1.00)
type: architecture
---
# Architecture

Loredex is a cargo workspace with a strict downward dependency flow: loredex-core (pure data model) <- loredex-git (read-only git wrapper) <- loredex-engine (business logic) <- two entry points, CLI and MCP [[cite:Cargo.toml:L2-L9]].

Storage is plain Markdown: pages live in the repository's `.wiki/pages/*.md`, with machine-readable evidence embedded as YAML frontmatter; `.wiki/.cache/` is a gitignored, disposable directory — there is no authoritative database [[cite:crates/loredex-engine/src/lib.rs:L274-L286]].

The trust model: every evidence item pins a `(path, line range, git blob sha at record time, sha1 of the snippet content)` quadruple [[cite:crates/loredex-core/src/page.rs:L29-L40]]. Staleness is decided by comparing blobs against HEAD cross-checked with a git diff; verification recomputes the content hash of the cited range directly, so trust never rests on commit ids alone.
