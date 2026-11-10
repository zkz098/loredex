---
id: engine-freshness
title: 'loredex-engine: staleness, verification, drift projection'
status: verified
generated_at: 2026-11-10T15:16:08Z
commit: 6802401ccb4cc357a9c5f870b52adaab11715e4b
evidence:
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 146
  - 168
  blob: 97ac2a10ec698dc5c7c31964bc053faff69eae61
  snippet_sha1: 35a7c0f5ba4eca97860be4cf4a49a1cce4d26bb1
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 256
  - 262
  blob: 97ac2a10ec698dc5c7c31964bc053faff69eae61
  snippet_sha1: 986c45e0fe23222e10896182ce92c4a29865216d
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 316
  - 330
  blob: 97ac2a10ec698dc5c7c31964bc053faff69eae61
  snippet_sha1: 08d9b743e61eb18dcda98fe2e41bb18e6a40dcfe
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 32
  - 52
  blob: 0d54875ecfa420ea80b08633b8dcd314b3c9c15c
  snippet_sha1: edf7813626f991ace596e7844ba169bbad0d4573
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 28
  - 30
  blob: 0d54875ecfa420ea80b08633b8dcd314b3c9c15c
  snippet_sha1: 4ca069b9ca1c9df03f402f88117ac2bebaa11fd7
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 85
  - 96
  blob: 0d54875ecfa420ea80b08633b8dcd314b3c9c15c
  snippet_sha1: 58d96aa0f5018ab781dcd053912493423e69500f
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 399
  - 411
  blob: 0d54875ecfa420ea80b08633b8dcd314b3c9c15c
  snippet_sha1: 327d32745e5c5249b723fa7c232aad13e0f13498
- path: crates/loredex-engine/src/put.rs
  lines:
  - 90
  - 120
  blob: 70af27d9389e0533a046d32aa3ab22838f3931ed
  snippet_sha1: 1f3dbd195d2b3741f67e3ac91742ff5a89693481
- path: crates/loredex-engine/src/put.rs
  lines:
  - 36
  - 62
  blob: 70af27d9389e0533a046d32aa3ab22838f3931ed
  snippet_sha1: dd2d3d7a19ccbac6aa0715c691db030d1f496987
type: system
---
# loredex-engine: staleness, verification, drift projection

`record_evidence` pins a citation to HEAD: it stores the blob oid plus the sha1 of the cited line range, so trust does not rest on the commit id alone [[cite:crates/loredex-engine/src/lib.rs:L146-L168]].

`check_page` applies a dual verdict: a page is stale when a recorded blob differs from HEAD *or* the pathspec diff from the generation commit touches an evidence path [[cite:crates/loredex-engine/src/lib.rs:L256-L262]]; an unresolvable base commit is stale on its own. `verify_page` goes further and recomputes each snippet's sha1 against HEAD content — content-level trust independent of commit ids [[cite:crates/loredex-engine/src/lib.rs:L316-L330]].

Line drift projection: `project_range` builds an old-line to new-line map from a line-level diff, then scores how well the projected snippet matches the recorded one [[cite:crates/loredex-engine/src/drift.rs:L32-L52]]; the projection is adopted only above the 0.6 similarity threshold, so one edited line inside a multi-line citation still survives [[cite:crates/loredex-engine/src/drift.rs:L28-L30]]. Renamed paths are recovered first by identical blob oid (a plain `git mv` keeps the oid), then by git similarity rename [[cite:crates/loredex-engine/src/drift.rs:L85-L96]]; genuinely lost citations downgrade the page to `draft` for rewrite rather than silently re-pinning [[cite:crates/loredex-engine/src/drift.rs:L399-L411]].

The write path `put_page` requires every submitted body to carry citation markers, each converted into pinned evidence; uncited bodies are rejected [[cite:crates/loredex-engine/src/put.rs:L90-L120]] — the entry guarantee that no claim enters the wiki without a resolvable source pointer. The one legal exception is the capture path `note_page`, which stores an uncited body as `draft` (never `verified`) while still pinning any markers it happens to carry [[cite:crates/loredex-engine/src/put.rs:L36-L62]].

Related: [[wiki:git-layer]] for the primitives, [[wiki:rule-evidence-contract]] for the write-path rule.
