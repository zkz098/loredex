---
id: rule-evidence-contract
title: Every verified claim cites resolvable evidence
status: verified
generated_at: 2026-11-09T11:46:50Z
commit: d9e2dbdd9a229e90e1611b012914a6c2e20bf824
evidence:
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
- path: crates/loredex-engine/src/auto.rs
  lines:
  - 113
  - 124
  blob: b4a94af752dbaae368c1b4a90f764601191baf18
  snippet_sha1: a5fe4fe2aff3959e61bc519870fbe1b6c9fd0e12
- path: crates/loredex-engine/src/lint.rs
  lines:
  - 93
  - 110
  blob: 4800de5f0cd1b0de76d09014bc25ad4f1d2f1f35
  snippet_sha1: d50881deb99637930250c8faad294ab772f17e9d
  note: drift-projected L93-L110@f71fc8f0 -> L93-L110 (similarity 1.00)
- path: crates/loredex-engine/tests/knowledge.rs
  lines:
  - 215
  - 232
  blob: 66d62f35f3827e7e03d39b2fe41e542787945024
  snippet_sha1: 26a4b6f62d729ad57f49715531d5bdb6cbaa9cf4
  note: drift-projected L215-L232@b7b556bf -> L215-L232 (similarity 1.00)
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
