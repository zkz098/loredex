---
id: decision-no-database
title: The frontmatter is the database
status: verified
generated_at: 2026-11-10T15:16:08Z
commit: 6802401ccb4cc357a9c5f870b52adaab11715e4b
evidence:
- path: crates/loredex-core/src/page.rs
  lines:
  - 41
  - 57
  blob: 0827a471fbe63039bf1155430c66e0c168085ccf
  snippet_sha1: b84591c9fd3689c3149782907fa84cee18978c98
- path: crates/loredex-core/src/page.rs
  lines:
  - 135
  - 145
  blob: 0827a471fbe63039bf1155430c66e0c168085ccf
  snippet_sha1: de1236c41fdb48d4e914ba8836a64b3ed28d6813
- path: crates/loredex-core/src/search.rs
  lines:
  - 106
  - 120
  blob: cd125703ab5b2d5d5abba89b07270a2b279ca8bc
  snippet_sha1: 072592b0c4490633c494519a04515cf0a624158d
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
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 462
  - 470
  blob: ef81cb0099e3e22883362d265b3cda16e809e506
  snippet_sha1: 9cdbdc30bf4b9aecc040eceab5a65a0b9492321f
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 99
  - 108
  blob: 97ac2a10ec698dc5c7c31964bc053faff69eae61
  snippet_sha1: 581e4e90dc283b3eaf982d26a223e933c24c5ce8
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
