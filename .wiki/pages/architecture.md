---
id: architecture
title: Architecture
status: verified
generated_at: 2026-11-08T15:19:47Z
commit: 6218b0933acfd281aa30842652167f82a491d069
evidence:
- path: Cargo.toml
  lines:
  - 2
  - 9
  blob: a731b14a83819e681bdb8da6a4daa4fd4f62f934
  snippet_sha1: cdb93553223a82b1c268fcf7d31a0b5c0dd928c0
  note: drift-projected L2-L9@5c79748e -> L2-L9 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 384
  - 396
  blob: c2376300ae68670e70a6cf650587fa7d50421535
  snippet_sha1: 02ad668b7bdf2cb5db082b398c984b1233f151ee
  note: drift-projected L379-L391@4600a9da -> L384-L396 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 30
  - 41
  blob: 04a91928fedae8fe609044e59cbd6ec4a564cdc9
  snippet_sha1: c29606ed08aec635918031cfd27810613803de0c
  note: drift-projected L30-L41@0a536572 -> L30-L41 (similarity 1.00)
type: architecture
---
# Architecture

Loredex is a cargo workspace with a strict downward dependency flow: loredex-core (pure data model) <- loredex-git (read-only git wrapper) <- loredex-engine (business logic) <- two entry points, CLI and MCP [[cite:Cargo.toml:L2-L9]].

Storage is plain Markdown: pages live in the repository's `.wiki/pages/*.md`, with machine-readable evidence embedded as YAML frontmatter; `.wiki/.cache/` is a gitignored, disposable directory — there is no authoritative database [[cite:crates/loredex-engine/src/lib.rs:L274-L286]].

The trust model: every evidence item pins a `(path, line range, git blob sha at record time, sha1 of the snippet content)` quadruple [[cite:crates/loredex-core/src/page.rs:L29-L40]]. Staleness is decided by comparing blobs against HEAD cross-checked with a git diff; verification recomputes the content hash of the cited range directly, so trust never rests on commit ids alone.
