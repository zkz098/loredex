---
id: git-layer
title: 'loredex-git: read-only git wrapper'
status: verified
generated_at: 2026-11-01T03:02:46Z
commit: 5fe6004cfe49890d5d14150d96e7200277613764
evidence:
- path: crates/loredex-git/src/lib.rs
  lines:
  - 23
  - 34
  blob: e093b50dbe48d34aa53eeea8a451bb79555b58e7
  snippet_sha1: 9c0be376e17c1621cd64d3e4886785fe8cd72034
- path: crates/loredex-git/src/lib.rs
  lines:
  - 69
  - 85
  blob: e093b50dbe48d34aa53eeea8a451bb79555b58e7
  snippet_sha1: 40fed49ef13297375ba972ef14f2357b752bcad4
- path: crates/loredex-git/src/lib.rs
  lines:
  - 99
  - 105
  blob: e093b50dbe48d34aa53eeea8a451bb79555b58e7
  snippet_sha1: 250e9dfe04fc8add367927d76cda669943f9a7a4
- path: crates/loredex-git/src/lib.rs
  lines:
  - 416
  - 424
  blob: e093b50dbe48d34aa53eeea8a451bb79555b58e7
  snippet_sha1: 8f749694fee40b81410069b456bf73f57948f6e2
- path: crates/loredex-git/src/lib.rs
  lines:
  - 112
  - 119
  blob: e093b50dbe48d34aa53eeea8a451bb79555b58e7
  snippet_sha1: 98e16ca62a408b78cfe9069791e02497d04a1591
- path: crates/loredex-git/src/lib.rs
  lines:
  - 121
  - 124
  blob: e093b50dbe48d34aa53eeea8a451bb79555b58e7
  snippet_sha1: 119edcf75aaa5d353174659486d6c77d3d3fa7a7
- path: crates/loredex-git/src/lib.rs
  lines:
  - 426
  - 435
  blob: e093b50dbe48d34aa53eeea8a451bb79555b58e7
  snippet_sha1: e4d93b726f189ca7ac21f83b00ef3cba62b28a3e
type: system
---
# loredex-git: read-only git wrapper

`GitRepo::discover` locates the repository from any path (walking up like git) and caches the workdir [[cite:crates/loredex-git/src/lib.rs:L23-L34]]. All evidence pinning reads committed content at HEAD, never the working tree: `head_blob_oid` resolves the path in the HEAD tree without a full tree walk [[cite:crates/loredex-git/src/lib.rs:L69-L85]], and `head_lines` slices 1-based inclusive ranges out of the committed blob [[cite:crates/loredex-git/src/lib.rs:L99-L105]].

Staleness cross-validation: `changed_paths_since` diffs the generation commit against HEAD restricted to the evidence pathspecs, so a path only counts as changed when git itself says it did [[cite:crates/loredex-git/src/lib.rs:L416-L424]]. `blob_content` reads any object by oid straight from the object db — the precondition that lets drift projection recover the *old* text [[cite:crates/loredex-git/src/lib.rs:L112-L119]]. `tracked_files` walks the HEAD tree recursively for module discovery [[cite:crates/loredex-git/src/lib.rs:L121-L124]].

`path_deltas` is the un-pathspaced sibling of `changed_paths_since`: a full ref->HEAD change list with similarity rename pairing, which `loredex diff` joins against page evidence to answer "what docs did this branch break" [[cite:crates/loredex-git/src/lib.rs:L426-L435]].

See [[wiki:engine-freshness]] for how these primitives compose into staleness verdicts and projection.
