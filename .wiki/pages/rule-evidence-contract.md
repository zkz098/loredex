---
id: rule-evidence-contract
title: Every verified claim cites resolvable evidence
status: verified
generated_at: 2026-11-10T15:16:08Z
commit: 6802401ccb4cc357a9c5f870b52adaab11715e4b
evidence:
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
- path: crates/loredex-engine/src/auto.rs
  lines:
  - 113
  - 124
  blob: ba5de45fb822b8efb953c1498ce55def0f751063
  snippet_sha1: 92698ffc477dd476b207065ea7b0fca30af76c0e
- path: crates/loredex-engine/src/lint.rs
  lines:
  - 93
  - 110
  blob: 4800de5f0cd1b0de76d09014bc25ad4f1d2f1f35
  snippet_sha1: d50881deb99637930250c8faad294ab772f17e9d
- path: crates/loredex-engine/tests/knowledge.rs
  lines:
  - 215
  - 232
  blob: 66d62f35f3827e7e03d39b2fe41e542787945024
  snippet_sha1: 26a4b6f62d729ad57f49715531d5bdb6cbaa9cf4
tags:
- policy
type: rule
---
# Rule: every verified claim cites resolvable source evidence

**Never store an uncited claim as `verified`.** A body with no inline citation marker is rejected at write time; the only legal way in without citations is the capture path, which must land as `draft`.

Why it exists: the wiki's value is that a reader (usually an agent that cannot afford to re-read the repo) can trust a sentence without re-verifying it. A paraphrase with no pointer is exactly the stale, misleading doc that makes agents read the source anyway — and worse, it *looks* verified in `loredex list`.

The rule is enforced on the write path, not by convention: `put_page_with` parses markers, refuses an empty set, and pins every accepted marker to the HEAD blob plus a snippet content hash [[cite:crates/loredex-engine/src/put.rs:L90-L120]]. The capture path is allowed to relax the *citation* requirement but never the *status* one: an uncited note is written as `draft`, and only becomes `verified` if it happens to carry pinnable markers [[cite:crates/loredex-engine/src/put.rs:L36-L62]].

Violation and its shape:
- **Bad** — calling `wiki_put_page` with a guess ("the retry loop probably never yields") to make it disappear from the worklist. `loredex check` will later report the page verified-but-uncited by nothing, and no evidence can be audited.
- **Good** — `wiki_note` the same sentence: it lands `draft`, `loredex plan` carries it as pending authoring work with the type's playbook instructions, and `loredex auto` refuses to call the wiki clean while it stays pending [[cite:crates/loredex-engine/src/auto.rs:L113-L124]].

Enforcement points: the rejection above, `loredex lint`'s `evidence-drift` check for hand-edited bodies that no longer match their frontmatter pins [[cite:crates/loredex-engine/src/lint.rs:L93-L110]], and the tests that pin this matrix [[cite:crates/loredex-engine/tests/knowledge.rs:L215-L232]].

Related: [[wiki:engine-freshness]], [[wiki:rule-advisory-never-fails-ci]].
