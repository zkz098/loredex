---
id: troubleshooting-lost-citations
title: '"citation content lost" flips pages to draft'
status: verified
generated_at: 2026-11-08T15:19:47Z
commit: 6218b0933acfd281aa30842652167f82a491d069
evidence:
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 28
  - 30
  blob: f40c756f8c4738472d9f66c9c24f81fbc8f2ff79
  snippet_sha1: 4ca069b9ca1c9df03f402f88117ac2bebaa11fd7
  note: drift-projected L28-L30@81c113ab -> L28-L30 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 406
  - 417
  blob: f40c756f8c4738472d9f66c9c24f81fbc8f2ff79
  snippet_sha1: 53e778d40b1e2e279e316a500be9f11f5696d899
  note: drift-projected L399-L411@81c113ab -> L406-L417 (similarity 0.85)
- path: crates/loredex-engine/src/auto.rs
  lines:
  - 64
  - 99
  blob: b4a94af752dbaae368c1b4a90f764601191baf18
  snippet_sha1: bcd733e77b4e4c53e3b5481d62cdcf3370e4e7d7
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 85
  - 96
  blob: f40c756f8c4738472d9f66c9c24f81fbc8f2ff79
  snippet_sha1: a01e6c0297db68a7399add05bf04847673acf2d8
  note: drift-projected L85-L96@81c113ab -> L85-L96 (similarity 1.00)
- path: crates/loredex-engine/src/put.rs
  lines:
  - 197
  - 237
  blob: 53f8b6e86e53d713a66c0bf4102f750cbdb0fba5
  snippet_sha1: 692fa1cfb1dc8b0f15e9db2f0dbc3ac9767f4437
  note: drift-projected L163-L192@a9842770 -> L197-L237 (similarity 0.93)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 344
  - 358
  blob: c2376300ae68670e70a6cf650587fa7d50421535
  snippet_sha1: 58819359d859568299c50d43a98675ffd8e53d9c
  note: drift-projected L339-L353@4600a9da -> L344-L358 (similarity 1.00)
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
