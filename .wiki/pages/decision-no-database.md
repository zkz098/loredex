---
id: decision-no-database
title: The frontmatter is the database
status: verified
generated_at: 2026-11-03T13:20:21Z
commit: 67da1675a709e5972e22a3f53797fe275070907e
evidence:
- path: crates/loredex-core/src/page.rs
  lines:
  - 41
  - 57
  blob: 04a91928fedae8fe609044e59cbd6ec4a564cdc9
  snippet_sha1: e6114a75be2ac5ef7cda901c36235c68c522d569
  note: drift-projected L41-L57@0a536572 -> L41-L57 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 143
  - 153
  blob: 04a91928fedae8fe609044e59cbd6ec4a564cdc9
  snippet_sha1: 669c21e454c64c95a15afd1bf13e20b4b0e2d76c
  note: drift-projected L135-L145@0a536572 -> L143-L153 (similarity 1.00)
- path: crates/loredex-core/src/search.rs
  lines:
  - 116
  - 130
  blob: d761dfe3556d4ca8fb2363ec5b06003e5e1fe9fa
  snippet_sha1: 826251caf0de5de1059b61e0d0e986da16d49c63
  note: drift-projected L106-L120@f7ec83c7 -> L116-L130 (similarity 1.00)
- path: crates/loredex-engine/src/organize.rs
  lines:
  - 54
  - 62
  blob: 6a9c3877c56bf896c092f9b0440c5f3f8dd62427
  snippet_sha1: f72c18e1dac60818ff22d139e6f5856ebb9258fb
- path: crates/loredex-engine/src/lint.rs
  lines:
  - 93
  - 110
  blob: 4800de5f0cd1b0de76d09014bc25ad4f1d2f1f35
  snippet_sha1: d50881deb99637930250c8faad294ab772f17e9d
  note: drift-projected L93-L110@f71fc8f0 -> L93-L110 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 529
  - 537
  blob: c80142e0bcb6707abf0119392942b2c83d283ba3
  snippet_sha1: 29551fb8394878d8509a005b38db3855fb903a23
  note: drift-projected L462-L470@ed2c4338 -> L529-L537 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 112
  - 121
  blob: 4600a9da494272e7d428e28189a2035bdd4f43cc
  snippet_sha1: 283e5dc1033a8132bdfc3741c781d364cf63ca80
  note: drift-projected L99-L108@bc899494 -> L112-L121 (similarity 1.00)
tags:
- policy
type: decision
---
# Decision: the frontmatter *is* the database

**Context and forces.** The producers are coding agents running against arbitrary repositories, and the reader is usually an agent that cannot afford to re-open the source. That means the store must be impossible to corrupt out of sync with what it describes, must survive a human editing a page in an editor, must be reviewable in a pull request, and must work with no setup step beyond `loredex init`.

**Options considered.**
- A persistent index or database as the source of truth, with markdown generated from it — buys query speed, costs a second authority that can drift from the files and a corruption/migration story.
- Forbidding hand-edits outright (the upstream openwiki stance) — keeps the store clean but makes the wiki wrong the moment a maintainer fixes a sentence locally.
- Markdown with YAML frontmatter as the only authoritative state.

**Choice and consequences.** The third: every machine-readable fact — status, the generation commit, and the evidence tuples `(path, lines, blob, snippet_sha1)` — lives in the page's own frontmatter [[cite:crates/loredex-core/src/page.rs:L41-L57]], and the file is the record. Reading the wiki means walking the pages directory [[cite:crates/loredex-core/src/page.rs:L135-L145]], search rebuilds its index from those files on demand [[cite:crates/loredex-core/src/search.rs:L106-L120]], and consolidation clusters are computed purely from recorded evidence — which is why an uncited draft simply has nothing to compare against yet [[cite:crates/loredex-engine/src/organize.rs:L54-L62]]. Hand edits are detected rather than forbidden: `lint` compares body markers against the frontmatter pins and reports `evidence-drift` [[cite:crates/loredex-engine/src/lint.rs:L93-L110]]. The MCP layer caches pages only by file identity (path, length, mtime), so the cache is a performance detail that any external write invalidates [[cite:crates/loredex-mcp/src/lib.rs:L462-L470]].

The cost is honest and bounded: every scan reads and parses all page files, so per-command cost is linear in page count and a wiki of tens of thousands of pages would want a persisted index.

**Reopen when** full-page scans show up in profiles or the README's known-boundaries note becomes live — then persist the *cache*, never the authority, keeping pages the thing a person can diff and fix [[cite:crates/loredex-engine/src/lib.rs:L99-L108]].

Related: [[wiki:architecture]], [[wiki:core-model]], [[wiki:rule-evidence-contract]].
