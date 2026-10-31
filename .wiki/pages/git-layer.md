---
id: git-layer
title: wiki-git：git 只读封装
status: verified
generated_at: 2026-10-31T12:38:40Z
commit: dd7d0c43dedd0de3296c4118c8b6f5443cb1bfeb
evidence:
- path: crates/loredex-git/src/lib.rs
  lines:
  - 15
  - 26
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: 9c0be376e17c1621cd64d3e4886785fe8cd72034
  note: drift-projected L15-L26@9a997999 -> L15-L26 (similarity 1.00)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 55
  - 67
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: 590b44a09744bf74b0923a315d0d5eeb4c78c925
  note: drift-projected L55-L67@9a997999 -> L55-L67 (similarity 1.00)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 83
  - 95
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: 613cb9d52a71801271ddf3ce00ff039ec602c8f7
  note: drift-projected L83-L95@9a997999 -> L83-L95 (similarity 1.00)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 97
  - 139
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: 91876790eb03369007f7502a8ac02b14ca34399f
  note: drift-projected L97-L141@9a997999 -> L97-L139 (similarity 0.93)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 141
  - 148
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: d99b49e8a6b7350b2c288dab0d7b885af2bec779
  note: drift-projected L143-L150@9a997999 -> L141-L148 (similarity 0.86)
- path: crates/loredex-git/src/lib.rs
  lines:
  - 150
  - 163
  blob: a5b273e75401450c0dcded5eac4480aa98bbf722
  snippet_sha1: d23025297797b9257bddac4e23e2164e48bd9a9c
  note: drift-projected L152-L165@9a997999 -> L150-L163 (similarity 1.00)
type: system
---
# wiki-git：git 只读封装

`GitRepo::discover` 从任意路径向上定位仓库并缓存 workdir [[cite:crates/loredex-git/src/lib.rs:L15-L26]]。证据定位全部基于 HEAD 的提交内容而非工作区：`head_blob_oid` 用 `HEAD:path` revparse 取 blob id [[cite:crates/loredex-git/src/lib.rs:L55-L67]]，`head_lines` 按 1-based 闭区间截取行 [[cite:crates/loredex-git/src/lib.rs:L83-L95]]。

过期交叉验证：`changed_paths_since` 对 (生成时 commit, HEAD) 做 tree diff 并按 evidence 路径设 pathspec [[cite:crates/loredex-git/src/lib.rs:L97-L141]]。`blob_content` 按 oid 直接读对象库，是行号漂移投影能拿到"旧内容"的前提 [[cite:crates/loredex-git/src/lib.rs:L143-L150]]。`tracked_files` 递归遍历 HEAD tree 供模块发现使用 [[cite:crates/loredex-git/src/lib.rs:L152-L165]]。
