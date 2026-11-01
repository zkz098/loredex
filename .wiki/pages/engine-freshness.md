---
id: engine-freshness
title: 'loredex-engine: staleness, verification, drift projection'
status: verified
generated_at: 2026-11-01T03:14:48Z
commit: d0155cf4e48bdf51a9efb9b1cfbaf21320314f23
evidence:
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 146
  - 168
  blob: bc899494cd898a0e2bbe24cf06a4fe1da946508d
  snippet_sha1: 283e3285b868e8f97890cce8658fa81d64361966
  note: drift-projected L146-L168@babdf319 -> L146-L168 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 256
  - 262
  blob: bc899494cd898a0e2bbe24cf06a4fe1da946508d
  snippet_sha1: 8d7aa3de63ced0667e657ecb3b92ab4ff66ca116
  note: drift-projected L256-L262@babdf319 -> L256-L262 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 316
  - 330
  blob: bc899494cd898a0e2bbe24cf06a4fe1da946508d
  snippet_sha1: 58819359d859568299c50d43a98675ffd8e53d9c
  note: drift-projected L316-L330@babdf319 -> L316-L330 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 32
  - 52
  blob: 81c113ab88942e4388af5b2d1c8710bbf435e1f3
  snippet_sha1: ead352d9b8107af3806ed4482344fa7275ec9548
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 28
  - 30
  blob: 81c113ab88942e4388af5b2d1c8710bbf435e1f3
  snippet_sha1: 4ca069b9ca1c9df03f402f88117ac2bebaa11fd7
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 85
  - 96
  blob: 81c113ab88942e4388af5b2d1c8710bbf435e1f3
  snippet_sha1: a01e6c0297db68a7399add05bf04847673acf2d8
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 399
  - 411
  blob: 81c113ab88942e4388af5b2d1c8710bbf435e1f3
  snippet_sha1: 53e11eae038acb7e67caafa24e6eec54add05bb8
- path: crates/loredex-engine/src/put.rs
  lines:
  - 90
  - 120
  blob: a9842770ec28a68164352211a73f07e39bcbf2d0
  snippet_sha1: ea7190bfa177bcdb5afdc9d6fe6bb1aa8764805a
- path: crates/loredex-engine/src/put.rs
  lines:
  - 36
  - 62
  blob: a9842770ec28a68164352211a73f07e39bcbf2d0
  snippet_sha1: 95d89fc33e29c18426c9c8bcb04301eb8a9b027a
type: system
---
# loredex-engine: staleness, verification, drift projection

`record_evidence` pins a citation to HEAD: it stores the blob oid plus the sha1 of the cited line range, so trust does not rest on the commit id alone [[cite:crates/loredex-engine/src/lib.rs:L146-L168]].

`check_page` applies a dual verdict: a page is stale when a recorded blob differs from HEAD *or* the pathspec diff from the generation commit touches an evidence path [[cite:crates/loredex-engine/src/lib.rs:L256-L262]]; an unresolvable base commit is stale on its own. `verify_page` goes further and recomputes each snippet's sha1 against HEAD content — content-level trust independent of commit ids [[cite:crates/loredex-engine/src/lib.rs:L316-L330]].

Line drift projection: `project_range` builds an old-line to new-line map from a line-level diff, then scores how well the projected snippet matches the recorded one [[cite:crates/loredex-engine/src/drift.rs:L32-L52]]; the projection is adopted only above the 0.6 similarity threshold, so one edited line inside a multi-line citation still survives [[cite:crates/loredex-engine/src/drift.rs:L28-L30]]. Renamed paths are recovered first by identical blob oid (a plain `git mv` keeps the oid), then by git similarity rename [[cite:crates/loredex-engine/src/drift.rs:L85-L96]]; genuinely lost citations downgrade the page to `draft` for rewrite rather than silently re-pinning [[cite:crates/loredex-engine/src/drift.rs:L399-L411]].

The write path `put_page` requires every submitted body to carry citation markers, each converted into pinned evidence; uncited bodies are rejected [[cite:crates/loredex-engine/src/put.rs:L90-L120]] — the entry guarantee that no claim enters the wiki without a resolvable source pointer. The one legal exception is the capture path `note_page`, which stores an uncited body as `draft` (never `verified`) while still pinning any markers it happens to carry [[cite:crates/loredex-engine/src/put.rs:L36-L62]].

Related: [[wiki:git-layer]] for the primitives, [[wiki:rule-evidence-contract]] for the write-path rule.
