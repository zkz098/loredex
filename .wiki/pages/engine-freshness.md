---
id: engine-freshness
title: 'loredex-engine: staleness, verification, drift projection'
status: verified
generated_at: 2026-11-09T14:37:56Z
commit: 8ce30c67ff3080a3c5cde0325e337dee4b5b2056
evidence:
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 179
  - 251
  blob: f53a1400b4d740442891fa8c0e1bed0a433ba83f
  snippet_sha1: d34bb4484dc69d6f63c9697340a686d222792652
  note: drift-projected L174-L196@c2376300 -> L179-L251 (similarity 0.91)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 339
  - 345
  blob: f53a1400b4d740442891fa8c0e1bed0a433ba83f
  snippet_sha1: 8d7aa3de63ced0667e657ecb3b92ab4ff66ca116
  note: drift-projected L284-L290@c2376300 -> L339-L345 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 399
  - 413
  blob: f53a1400b4d740442891fa8c0e1bed0a433ba83f
  snippet_sha1: 58819359d859568299c50d43a98675ffd8e53d9c
  note: drift-projected L344-L358@c2376300 -> L399-L413 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 32
  - 52
  blob: 9062b2d49788c15508c3e148f71423fe07df4fe5
  snippet_sha1: ead352d9b8107af3806ed4482344fa7275ec9548
  note: drift-projected L32-L52@f40c756f -> L32-L52 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 28
  - 30
  blob: 9062b2d49788c15508c3e148f71423fe07df4fe5
  snippet_sha1: 4ca069b9ca1c9df03f402f88117ac2bebaa11fd7
  note: drift-projected L28-L30@f40c756f -> L28-L30 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 85
  - 96
  blob: 9062b2d49788c15508c3e148f71423fe07df4fe5
  snippet_sha1: a01e6c0297db68a7399add05bf04847673acf2d8
  note: drift-projected L85-L96@f40c756f -> L85-L96 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 505
  - 516
  blob: 9062b2d49788c15508c3e148f71423fe07df4fe5
  snippet_sha1: 53e778d40b1e2e279e316a500be9f11f5696d899
  note: drift-projected L406-L417@f40c756f -> L505-L516 (similarity 1.00)
- path: crates/loredex-engine/src/put.rs
  lines:
  - 123
  - 153
  blob: a19f4d1d27850d30359e72caa2dfc38033b53398
  snippet_sha1: ea7190bfa177bcdb5afdc9d6fe6bb1aa8764805a
  note: drift-projected L123-L153@53f8b6e8 -> L123-L153 (similarity 1.00)
- path: crates/loredex-engine/src/put.rs
  lines:
  - 53
  - 95
  blob: a19f4d1d27850d30359e72caa2dfc38033b53398
  snippet_sha1: a513c03b85b8d58947168a8f631e92c33c87f24a
  note: drift-projected L53-L95@53f8b6e8 -> L53-L95 (similarity 1.00)
type: system
---
# loredex-engine: staleness, verification, drift projection

`record_evidence` pins a citation to HEAD: it stores the blob oid plus the sha1 of the cited line range, so trust does not rest on the commit id alone [[cite:crates/loredex-engine/src/lib.rs:L146-L168]].

`check_page` applies a dual verdict: a page is stale when a recorded blob differs from HEAD *or* the pathspec diff from the generation commit touches an evidence path [[cite:crates/loredex-engine/src/lib.rs:L256-L262]]; an unresolvable base commit is stale on its own. `verify_page` goes further and recomputes each snippet's sha1 against HEAD content — content-level trust independent of commit ids [[cite:crates/loredex-engine/src/lib.rs:L316-L330]].

Line drift projection: `project_range` builds an old-line to new-line map from a line-level diff, then scores how well the projected snippet matches the recorded one [[cite:crates/loredex-engine/src/drift.rs:L32-L52]]; the projection is adopted only above the 0.6 similarity threshold, so one edited line inside a multi-line citation still survives [[cite:crates/loredex-engine/src/drift.rs:L28-L30]]. Renamed paths are recovered first by identical blob oid (a plain `git mv` keeps the oid), then by git similarity rename [[cite:crates/loredex-engine/src/drift.rs:L85-L96]]; genuinely lost citations downgrade the page to `draft` for rewrite rather than silently re-pinning [[cite:crates/loredex-engine/src/drift.rs:L399-L411]].

The write path `put_page` requires every submitted body to carry citation markers, each converted into pinned evidence; uncited bodies are rejected [[cite:crates/loredex-engine/src/put.rs:L90-L120]] — the entry guarantee that no claim enters the wiki without a resolvable source pointer. The one legal exception is the capture path `note_page`, which stores an uncited body as `draft` (never `verified`) while still pinning any markers it happens to carry [[cite:crates/loredex-engine/src/put.rs:L36-L62]].

Related: [[wiki:git-layer]] for the primitives, [[wiki:rule-evidence-contract]] for the write-path rule.
