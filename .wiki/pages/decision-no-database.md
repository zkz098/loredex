---
id: decision-no-database
title: The frontmatter is the database
status: verified
generated_at: 2026-11-09T14:37:56Z
commit: 8ce30c67ff3080a3c5cde0325e337dee4b5b2056
evidence:
- path: crates/loredex-core/src/page.rs
  lines:
  - 43
  - 59
  blob: 0827a471fbe63039bf1155430c66e0c168085ccf
  snippet_sha1: e6114a75be2ac5ef7cda901c36235c68c522d569
  note: drift-projected L41-L57@04a91928 -> L43-L59 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 145
  - 155
  blob: 0827a471fbe63039bf1155430c66e0c168085ccf
  snippet_sha1: 669c21e454c64c95a15afd1bf13e20b4b0e2d76c
  note: drift-projected L143-L153@04a91928 -> L145-L155 (similarity 1.00)
- path: crates/loredex-core/src/search.rs
  lines:
  - 138
  - 163
  blob: cd125703ab5b2d5d5abba89b07270a2b279ca8bc
  snippet_sha1: fecd880252719dbc5cd7ed4eaf69aa62f665d4c0
  note: drift-projected L116-L130@d761dfe3 -> L138-L163 (similarity 0.73)
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
  - 538
  - 546
  blob: 6f1ff0c44e3f66f5a16f592dcddc4b54bb61fad1
  snippet_sha1: 29551fb8394878d8509a005b38db3855fb903a23
  note: drift-projected L538-L546@d220dafd -> L538-L546 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 122
  - 131
  blob: f53a1400b4d740442891fa8c0e1bed0a433ba83f
  snippet_sha1: 283e5dc1033a8132bdfc3741c781d364cf63ca80
  note: drift-projected L117-L126@c2376300 -> L122-L131 (similarity 1.00)
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
