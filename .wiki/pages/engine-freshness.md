---
id: engine-freshness
title: 'loredex-engine: staleness, verification, drift projection'
status: verified
generated_at: 2026-11-03T13:20:21Z
commit: 67da1675a709e5972e22a3f53797fe275070907e
evidence:
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 169
  - 191
  blob: 4600a9da494272e7d428e28189a2035bdd4f43cc
  snippet_sha1: 283e3285b868e8f97890cce8658fa81d64361966
  note: drift-projected L146-L168@bc899494 -> L169-L191 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 279
  - 285
  blob: 4600a9da494272e7d428e28189a2035bdd4f43cc
  snippet_sha1: 8d7aa3de63ced0667e657ecb3b92ab4ff66ca116
  note: drift-projected L256-L262@bc899494 -> L279-L285 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 339
  - 353
  blob: 4600a9da494272e7d428e28189a2035bdd4f43cc
  snippet_sha1: 58819359d859568299c50d43a98675ffd8e53d9c
  note: drift-projected L316-L330@bc899494 -> L339-L353 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 32
  - 52
  blob: f40c756f8c4738472d9f66c9c24f81fbc8f2ff79
  snippet_sha1: ead352d9b8107af3806ed4482344fa7275ec9548
  note: drift-projected L32-L52@81c113ab -> L32-L52 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 28
  - 30
  blob: f40c756f8c4738472d9f66c9c24f81fbc8f2ff79
  snippet_sha1: 4ca069b9ca1c9df03f402f88117ac2bebaa11fd7
  note: drift-projected L28-L30@81c113ab -> L28-L30 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 85
  - 96
  blob: f40c756f8c4738472d9f66c9c24f81fbc8f2ff79
  snippet_sha1: a01e6c0297db68a7399add05bf04847673acf2d8
  note: drift-projected L85-L96@81c113ab -> L85-L96 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 406
  - 417
  blob: f40c756f8c4738472d9f66c9c24f81fbc8f2ff79
  snippet_sha1: 53e778d40b1e2e279e316a500be9f11f5696d899
  note: drift-projected L399-L411@81c113ab -> L406-L417 (similarity 0.85)
- path: crates/loredex-engine/src/put.rs
  lines:
  - 123
  - 153
  blob: 53f8b6e86e53d713a66c0bf4102f750cbdb0fba5
  snippet_sha1: ea7190bfa177bcdb5afdc9d6fe6bb1aa8764805a
  note: drift-projected L90-L120@a9842770 -> L123-L153 (similarity 1.00)
- path: crates/loredex-engine/src/put.rs
  lines:
  - 53
  - 95
  blob: 53f8b6e86e53d713a66c0bf4102f750cbdb0fba5
  snippet_sha1: a513c03b85b8d58947168a8f631e92c33c87f24a
  note: drift-projected L36-L62@a9842770 -> L53-L95 (similarity 1.00)
type: system
---
# loredex-engine: staleness, verification, drift projection

`record_evidence` pins a citation to HEAD: it stores the blob oid plus the sha1 of the cited line range, so trust does not rest on the commit id alone [[cite:crates/loredex-engine/src/lib.rs:L146-L168]].

`check_page` applies a dual verdict: a page is stale when a recorded blob differs from HEAD *or* the pathspec diff from the generation commit touches an evidence path [[cite:crates/loredex-engine/src/lib.rs:L256-L262]]; an unresolvable base commit is stale on its own. `verify_page` goes further and recomputes each snippet's sha1 against HEAD content — content-level trust independent of commit ids [[cite:crates/loredex-engine/src/lib.rs:L316-L330]].

Line drift projection: `project_range` builds an old-line to new-line map from a line-level diff, then scores how well the projected snippet matches the recorded one [[cite:crates/loredex-engine/src/drift.rs:L32-L52]]; the projection is adopted only above the 0.6 similarity threshold, so one edited line inside a multi-line citation still survives [[cite:crates/loredex-engine/src/drift.rs:L28-L30]]. Renamed paths are recovered first by identical blob oid (a plain `git mv` keeps the oid), then by git similarity rename [[cite:crates/loredex-engine/src/drift.rs:L85-L96]]; genuinely lost citations downgrade the page to `draft` for rewrite rather than silently re-pinning [[cite:crates/loredex-engine/src/drift.rs:L399-L411]].

The write path `put_page` requires every submitted body to carry citation markers, each converted into pinned evidence; uncited bodies are rejected [[cite:crates/loredex-engine/src/put.rs:L90-L120]] — the entry guarantee that no claim enters the wiki without a resolvable source pointer. The one legal exception is the capture path `note_page`, which stores an uncited body as `draft` (never `verified`) while still pinning any markers it happens to carry [[cite:crates/loredex-engine/src/put.rs:L36-L62]].

Related: [[wiki:git-layer]] for the primitives, [[wiki:rule-evidence-contract]] for the write-path rule.
