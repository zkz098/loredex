---
id: troubleshooting-lost-citations
title: '"citation content lost" flips pages to draft'
status: verified
generated_at: 2026-11-10T15:16:08Z
commit: 6802401ccb4cc357a9c5f870b52adaab11715e4b
evidence:
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 28
  - 30
  blob: 0d54875ecfa420ea80b08633b8dcd314b3c9c15c
  snippet_sha1: 4ca069b9ca1c9df03f402f88117ac2bebaa11fd7
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 399
  - 411
  blob: 0d54875ecfa420ea80b08633b8dcd314b3c9c15c
  snippet_sha1: 327d32745e5c5249b723fa7c232aad13e0f13498
- path: crates/loredex-engine/src/auto.rs
  lines:
  - 64
  - 99
  blob: ba5de45fb822b8efb953c1498ce55def0f751063
  snippet_sha1: 54099b938cc7ebc3cd79999521b24d4e130c2b92
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 85
  - 96
  blob: 0d54875ecfa420ea80b08633b8dcd314b3c9c15c
  snippet_sha1: 58d96aa0f5018ab781dcd053912493423e69500f
- path: crates/loredex-engine/src/put.rs
  lines:
  - 163
  - 192
  blob: 70af27d9389e0533a046d32aa3ab22838f3931ed
  snippet_sha1: f5eba9af9cd6e28d8e17839599906136c4933dc8
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 316
  - 330
  blob: 97ac2a10ec698dc5c7c31964bc053faff69eae61
  snippet_sha1: 08d9b743e61eb18dcda98fe2e41bb18e6a40dcfe
tags:
- policy
type: troubleshooting
---
# Troubleshooting: pages flip to draft with "citation content lost"

**Symptom.** After a refactor or a rebase, `loredex auto` prints a rewrite line naming the page and the file whose evidence could not be recovered, e.g.

    [rewrite]   git-layer: citation content lost for: crates/loredex-engine/src/drift.rs

The page now carries `status: draft`, its evidence entries read `content lost at HEAD; re-pinned at recorded range (draft)`, `loredex check` exits 1, and `loredex plan` lists the page as a pending Refresh task.

**Root cause.** Projection recovers a citation by mapping the old line range onto the new revision with a line-level diff and adopting the result only when the snippet still matches above the 0.6 similarity threshold [[cite:crates/loredex-engine/src/drift.rs:L28-L30]]. When the cited code was *rewritten* rather than moved, the score falls short, and refresh downgrades the page instead of re-pointing a claim at different text [[cite:crates/loredex-engine/src/drift.rs:L399-L411]]; `auto` collects exactly those pages as `needs_rewrite` while everything projection could fix is repaired with zero model tokens [[cite:crates/loredex-engine/src/auto.rs:L64-L99]]. The downgrade is the point: a `verified` page whose evidence no longer says what the claim says is the failure mode this tool exists to prevent.

Watch for the false alarm after a rename: a plain `git mv` keeps the blob oid and is recovered by content-addressed lookup on its own [[cite:crates/loredex-engine/src/drift.rs:L85-L96]]. mv-plus-edits needs similarity detection *and* projection, and only fails when the snippet content truly changed.

**Fix.** Read the file at HEAD and re-put the same page id with corrected ranges — `put` re-pins every marker, stores the page as `verified` and auto-resolves its plan task [[cite:crates/loredex-engine/src/put.rs:L163-L192]]. If the described behavior is gone, rewrite or delete the claim rather than pointing it at the nearest surviving line; if you cannot cite it yet, re-capture it with `loredex note` so it stays explicitly unverified.

**Confirm resolved.** `loredex check` exits 0 with the page listed as verified, and `loredex verify <id>` recomputes every snippet's content hash against HEAD — all entries print `ok` when the page is trustworthy again [[cite:crates/loredex-engine/src/lib.rs:L316-L330]].

Related: [[wiki:engine-freshness]], [[wiki:git-layer]], [[wiki:rule-evidence-contract]].
