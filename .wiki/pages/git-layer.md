---
id: git-layer
title: 'loredex-git: read-only git wrapper'
status: verified
generated_at: 2026-11-10T15:16:08Z
commit: 6802401ccb4cc357a9c5f870b52adaab11715e4b
evidence:
- path: crates/loredex-git/src/lib.rs
  lines:
  - 23
  - 34
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: 2329e1ff614c18c94955bfe55bda582529aac8da
- path: crates/loredex-git/src/lib.rs
  lines:
  - 69
  - 85
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: 8f5d28518426d2754f84a4c37a62bf924db6b1e7
- path: crates/loredex-git/src/lib.rs
  lines:
  - 99
  - 105
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: 4b843c1347bfce05e06a9aa26d4044423f43413b
- path: crates/loredex-git/src/lib.rs
  lines:
  - 416
  - 424
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: 2266bac86260abf2010c60e01ee91f7ed0d16302
- path: crates/loredex-git/src/lib.rs
  lines:
  - 112
  - 119
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: f90187d7ab7fd6b567c33b35a9a3f126c81be62f
- path: crates/loredex-git/src/lib.rs
  lines:
  - 121
  - 124
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: f8405b5eff56c528565d92f26519a45a24d76a7a
- path: crates/loredex-git/src/lib.rs
  lines:
  - 426
  - 435
  blob: e4ba64b2462a23c737ddc98d65349f9e0b66b7cc
  snippet_sha1: defb81be681b5c12dbeb4c5a5ca54cdb0b38b6f0
type: system
---
# loredex-git: read-only git wrapper

`GitRepo::discover` locates the repository from any path (walking up like git) and caches the workdir [[cite:crates/loredex-git/src/lib.rs:L23-L34]]. All evidence pinning reads committed content at HEAD, never the working tree: `head_blob_oid` resolves the path in the HEAD tree without a full tree walk [[cite:crates/loredex-git/src/lib.rs:L69-L85]], and `head_lines` slices 1-based inclusive ranges out of the committed blob [[cite:crates/loredex-git/src/lib.rs:L99-L105]].

Staleness cross-validation: `changed_paths_since` diffs the generation commit against HEAD restricted to the evidence pathspecs, so a path only counts as changed when git itself says it did [[cite:crates/loredex-git/src/lib.rs:L416-L424]]. `blob_content` reads any object by oid straight from the object db — the precondition that lets drift projection recover the *old* text [[cite:crates/loredex-git/src/lib.rs:L112-L119]]. `tracked_files` walks the HEAD tree recursively for module discovery [[cite:crates/loredex-git/src/lib.rs:L121-L124]].

`path_deltas` is the un-pathspaced sibling of `changed_paths_since`: a full ref->HEAD change list with similarity rename pairing, which `loredex diff` joins against page evidence to answer "what docs did this branch break" [[cite:crates/loredex-git/src/lib.rs:L426-L435]].

See [[wiki:engine-freshness]] for how these primitives compose into staleness verdicts and projection.
