---
id: rule-advisory-never-fails-ci
title: Curation hints are advisory and never break CI
status: verified
generated_at: 2026-11-03T13:20:21Z
commit: 67da1675a709e5972e22a3f53797fe275070907e
evidence:
- path: crates/loredex-engine/src/organize.rs
  lines:
  - 21
  - 23
  blob: 6a9c3877c56bf896c092f9b0440c5f3f8dd62427
  snippet_sha1: 2fa8ecbdf6935348ba6100f2e1aa79a4263a316f
- path: crates/loredex-engine/src/organize.rs
  lines:
  - 74
  - 82
  blob: 6a9c3877c56bf896c092f9b0440c5f3f8dd62427
  snippet_sha1: 2c73c861ae254bf04cbee3170e6828920f457f58
- path: crates/loredex-engine/src/organize.rs
  lines:
  - 79
  - 81
  blob: 6a9c3877c56bf896c092f9b0440c5f3f8dd62427
  snippet_sha1: 58d2a6e9d0c0ba6d5d2bb75b204f015d415d5704
- path: crates/loredex-engine/src/lint.rs
  lines:
  - 17
  - 26
  blob: 4800de5f0cd1b0de76d09014bc25ad4f1d2f1f35
  snippet_sha1: da600c830af88a1a362d701e28653e394a051891
  note: drift-projected L17-L26@f71fc8f0 -> L17-L26 (similarity 1.00)
- path: crates/loredex-engine/src/lint.rs
  lines:
  - 151
  - 161
  blob: 4800de5f0cd1b0de76d09014bc25ad4f1d2f1f35
  snippet_sha1: a0b584142e3f263cf4341f4fd28366363438fd71
  note: drift-projected L152-L162@f71fc8f0 -> L151-L161 (similarity 1.00)
- path: crates/loredex-cli/src/main.rs
  lines:
  - 858
  - 879
  blob: 39e2b9b14b2d9aa74ee28b53f1f6db44056d8675
  snippet_sha1: b8545f73e379d3dac9a07fb8969e138fdba41e8e
  note: drift-projected L705-L726@ee39b2bb -> L858-L879 (similarity 1.00)
- path: crates/loredex-engine/src/auto.rs
  lines:
  - 113
  - 124
  blob: b4a94af752dbaae368c1b4a90f764601191baf18
  snippet_sha1: a5fe4fe2aff3959e61bc519870fbe1b6c9fd0e12
- path: crates/loredex-engine/src/organize.rs
  lines:
  - 92
  - 98
  blob: 6a9c3877c56bf896c092f9b0440c5f3f8dd62427
  snippet_sha1: 5762ee3145bede9cf9ff41e26ab6d652a15ba363
- path: crates/loredex-engine/tests/knowledge.rs
  lines:
  - 232
  - 250
  blob: 66d62f35f3827e7e03d39b2fe41e542787945024
  snippet_sha1: 5602dd8de02da6b264dff2fb5792b7de75b564bd
  note: drift-projected L232-L250@b7b556bf -> L232-L250 (similarity 1.00)
- path: crates/loredex-engine/tests/knowledge.rs
  lines:
  - 380
  - 398
  blob: 66d62f35f3827e7e03d39b2fe41e542787945024
  snippet_sha1: f8577e806cff6c8a0cfc7f93bbcc4778d5fef3e0
  note: drift-projected L380-L398@b7b556bf -> L380-L398 (similarity 1.00)
tags:
- policy
type: rule
---
# Rule: curation hints are advisory and never break CI

**A maintainer suggestion must never turn a build red.** Structural defects (broken links, `stale-but-verified`, body/frontmatter evidence drift, unknown page type, secret-shaped citations) fail `loredex lint` and `loredex auto`; judgement-call findings carry `advisory: true` and only inform.

Why it exists: duplicate coverage is not a defect. Two pages citing the same file is often the correct shape — one describes the mechanism, one records the failure. If overlap failed CI, every refactor touching a widely-cited file would flood the log with failures the author cannot act on, and the team would learn to ignore the wiki gate, losing the defect checks that matter.

**Which findings qualify as advisory, and how loose the match may be:** the pair must have *substantial* evidence overlap before it is named at all — the shared files have to cover at least half of the smaller page's distinct evidence [[cite:crates/loredex-engine/src/organize.rs:L21-L23]]. Anything looser means two pages are describing different aspects of the same hot file, which is normal structure, not duplication [[cite:crates/loredex-engine/src/organize.rs:L74-L82]]. And a pair that already carries a cross-page wiki link is *settled*: someone read both pages and split their responsibilities, so the finding closes itself [[cite:crates/loredex-engine/src/organize.rs:L79-L81]].

The rule in code: the finding type carries the flag [[cite:crates/loredex-engine/src/lint.rs:L17-L26]], the overlap pass sets it to `true` while every defect pass sets `false` [[cite:crates/loredex-engine/src/lint.rs:L152-L162]], `loredex lint` exits 1 only when a non-advisory issue exists [[cite:crates/loredex-cli/src/main.rs:L705-L726]], and `auto` counts defects rather than raw findings for `all_clean` — the flag post-merge hooks and the GitHub Action gate on [[cite:crates/loredex-engine/src/auto.rs:L113-L124]].

Violation and its shape:
- **Bad** — wiring `loredex lint`'s overlap finding, or `loredex organize` itself, into a merge gate. It converts a curation backlog into a blocker, and the first large refactor makes people suppress the whole check.
- **Good** — print advisories under an `[advisory:...]` tag, keep exit 0, and let `loredex organize` report clusters as work items for whoever owns the area [[cite:crates/loredex-engine/src/organize.rs:L92-L98]].

Confirm the rule holds: `loredex lint` on a wiki whose only finding is overlap exits 0, and `auto` stays clean of non-advisory issues — both asserted by tests, along with the overlap-ratio floor [[cite:crates/loredex-engine/tests/knowledge.rs:L232-L250]] [[cite:crates/loredex-engine/tests/knowledge.rs:L380-L398]].

Related: [[wiki:rule-evidence-contract]], [[wiki:entry-mcp-cli]], [[wiki:decision-no-database]].
