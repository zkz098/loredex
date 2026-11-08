---
id: git-layer
title: 'loredex-git: read-only git wrapper'
status: draft
generated_at: 2026-11-08T15:19:47Z
commit: 6218b0933acfd281aa30842652167f82a491d069
evidence:
- path: crates/loredex-git/src/lib.rs
  lines:
  - 23
  - 34
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: 2329e1ff614c18c94955bfe55bda582529aac8da
  note: drift-projected L23-L34@e093b50d -> L23-L34 (similarity 0.92)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 69
  - 85
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: 8f5d28518426d2754f84a4c37a62bf924db6b1e7
  note: content lost at HEAD; re-pinned at recorded range (draft)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 99
  - 105
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: 4b843c1347bfce05e06a9aa26d4044423f43413b
  note: content lost at HEAD; re-pinned at recorded range (draft)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 391
  - 399
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: 8f749694fee40b81410069b456bf73f57948f6e2
  note: drift-projected L416-L424@e093b50d -> L391-L399 (similarity 1.00)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 112
  - 119
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: f90187d7ab7fd6b567c33b35a9a3f126c81be62f
  note: content lost at HEAD; re-pinned at recorded range (draft)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 98
  - 101
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: 119edcf75aaa5d353174659486d6c77d3d3fa7a7
  note: drift-projected L121-L124@e093b50d -> L98-L101 (similarity 1.00)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 401
  - 408
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: d6c71d19049d56e78fe3a9b64243c2241cd6d9dc
  note: drift-projected L426-L435@e093b50d -> L401-L408 (similarity 0.70)
type: system
---
# loredex-git: read-only git wrapper

`GitRepo::discover` locates the repository from any path (walking up like git) and caches the workdir [[cite:crates/loredex-git/src/lib.rs:L23-L34]]. All evidence pinning reads committed content at HEAD, never the working tree: `head_blob_oid` resolves the path in the HEAD tree without a full tree walk [[cite:crates/loredex-git/src/lib.rs:L69-L85]], and `head_lines` slices 1-based inclusive ranges out of the committed blob [[cite:crates/loredex-git/src/lib.rs:L99-L105]].

Staleness cross-validation: `changed_paths_since` diffs the generation commit against HEAD restricted to the evidence pathspecs, so a path only counts as changed when git itself says it did [[cite:crates/loredex-git/src/lib.rs:L416-L424]]. `blob_content` reads any object by oid straight from the object db — the precondition that lets drift projection recover the *old* text [[cite:crates/loredex-git/src/lib.rs:L112-L119]]. `tracked_files` walks the HEAD tree recursively for module discovery [[cite:crates/loredex-git/src/lib.rs:L121-L124]].

`path_deltas` is the un-pathspaced sibling of `changed_paths_since`: a full ref->HEAD change list with similarity rename pairing, which `loredex diff` joins against page evidence to answer "what docs did this branch break" [[cite:crates/loredex-git/src/lib.rs:L426-L435]].

See [[wiki:engine-freshness]] for how these primitives compose into staleness verdicts and projection.
