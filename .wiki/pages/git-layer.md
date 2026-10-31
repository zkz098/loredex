---
id: git-layer
title: 'loredex-git: read-only git wrapper'
status: verified
generated_at: 2026-10-31T12:43:23Z
commit: 1d2863aaea11c84a442cdbf282fb7da8211225cd
evidence:
- path: crates/loredex-git/src/lib.rs
  lines:
  - 15
  - 26
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: 9c0be376e17c1621cd64d3e4886785fe8cd72034
- path: crates/loredex-git/src/lib.rs
  lines:
  - 55
  - 67
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: 590b44a09744bf74b0923a315d0d5eeb4c78c925
- path: crates/loredex-git/src/lib.rs
  lines:
  - 83
  - 95
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: 613cb9d52a71801271ddf3ce00ff039ec602c8f7
- path: crates/loredex-git/src/lib.rs
  lines:
  - 97
  - 139
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: 91876790eb03369007f7502a8ac02b14ca34399f
- path: crates/loredex-git/src/lib.rs
  lines:
  - 141
  - 148
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: d99b49e8a6b7350b2c288dab0d7b885af2bec779
- path: crates/loredex-git/src/lib.rs
  lines:
  - 150
  - 163
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: d23025297797b9257bddac4e23e2164e48bd9a9c
type: system
---
# loredex-git: read-only git wrapper

`GitRepo::discover` locates the repository from any path (walking up like git) and caches the workdir [[cite:crates/loredex-git/src/lib.rs:L15-L26]]. All evidence pinning reads committed content at HEAD, never the working tree: `head_blob_oid` resolves `HEAD:path` via revparse [[cite:crates/loredex-git/src/lib.rs:L55-L67]] and `head_lines` slices 1-based inclusive ranges [[cite:crates/loredex-git/src/lib.rs:L83-L95]].

Staleness cross-validation: `changed_paths_since` runs a tree diff between the generation commit and HEAD restricted to evidence pathspecs [[cite:crates/loredex-git/src/lib.rs:L97-L139]]. `blob_content` reads any object by oid straight from the object db — the precondition that lets drift projection recover the *old* text [[cite:crates/loredex-git/src/lib.rs:L141-L148]]. `tracked_files` walks the HEAD tree recursively for module discovery [[cite:crates/loredex-git/src/lib.rs:L150-L163]].
