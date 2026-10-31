---
id: engine-freshness
title: 'loredex-engine: staleness, verification, drift projection'
status: verified
generated_at: 2026-10-31T12:48:00Z
commit: 36830a4b7c71fb5e8d12b7287d7db0358971805f
evidence:
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 90
  - 108
  blob: f716c3c7fc54d6c9bfa7dea6970eaddf0b4eadf7
  snippet_sha1: 611eb1281befdd587e8e67de5f8673de82b3e9c7
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 129
  - 185
  blob: f716c3c7fc54d6c9bfa7dea6970eaddf0b4eadf7
  snippet_sha1: 776d2500376bf85400f3fd2357513f7222fb07b7
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 234
  - 252
  blob: f716c3c7fc54d6c9bfa7dea6970eaddf0b4eadf7
  snippet_sha1: 570e40c2c3b468f39090df5953e52c26670831d1
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 34
  - 74
  blob: 84ca622439429999d5bfdc99cae091f508fdd7cd
  snippet_sha1: 77a85a350863b8b01c39ec3a7af91f2e8d358783
  note: drift-projected L34-L74@75afc4ab -> L34-L74 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 25
  - 30
  blob: 84ca622439429999d5bfdc99cae091f508fdd7cd
  snippet_sha1: a06223e23808aa9b99a600f1bde0da2d4c045389
  note: drift-projected L25-L30@75afc4ab -> L25-L30 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 233
  - 274
  blob: 84ca622439429999d5bfdc99cae091f508fdd7cd
  snippet_sha1: eb3acd65ccd4efbabafaa6da7bbd0611c6f0d9d6
  note: drift-projected L233-L274@75afc4ab -> L233-L274 (similarity 1.00)
- path: crates/loredex-engine/src/put.rs
  lines:
  - 20
  - 67
  blob: af719f7b8477da77230d4ce23c8d91b7c7b3b3a7
  snippet_sha1: 0bb2dd68705ede6062736d48fb709dc80ff3c84d
type: system
---
# loredex-engine: staleness, verification, drift projection

`record_evidence` pins a citation to HEAD: it stores the blob oid and the sha1 of the cited line range [[cite:crates/loredex-engine/src/lib.rs:L90-L108]].

`check_page` applies a dual verdict: a page is stale when a recorded blob differs from HEAD *or* the pathspec diff from the generation commit touches an evidence path; an unresolvable base commit is stale on its own [[cite:crates/loredex-engine/src/lib.rs:L129-L185]]. `verify_page` goes further and recomputes each snippet's sha1 against HEAD content — content-level trust independent of commit ids [[cite:crates/loredex-engine/src/lib.rs:L234-L252]].

Line drift projection: `project_range` builds an old-line to new-line map from a line-level diff (deleted lines anchor to the nearest surviving line, collapsed ranges keep width) [[cite:crates/loredex-engine/src/drift.rs:L34-L74]]; the projected snippet is adopted only at similarity above the 0.6 threshold [[cite:crates/loredex-engine/src/drift.rs:L25-L30]]. `refresh_page_with_drift` also recovers renamed paths — first by identical blob oid, then by git similarity rename — and downgrades genuinely lost citations to `draft` for rewrite [[cite:crates/loredex-engine/src/drift.rs:L233-L274]].

The write path `put_page` requires every submitted body to carry citation markers, each converted into pinned evidence; uncited bodies are rejected [[cite:crates/loredex-engine/src/put.rs:L20-L67]] — the entry guarantee that no claim enters the wiki without a resolvable source pointer.
