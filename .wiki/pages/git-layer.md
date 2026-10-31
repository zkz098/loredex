---
id: git-layer
title: wiki-git：git 只读封装
status: verified
generated_at: 2026-10-31T12:27:05Z
commit: 1d51e1c5f143342205c6040d13ad0bb8ae955f1d
evidence:
- path: crates/loredex-git/src/lib.rs
  lines:
  - 15
  - 26
  blob: 9a997999dbccceba8051d0e1fb340596b48576cb
  snippet_sha1: 9c0be376e17c1621cd64d3e4886785fe8cd72034
- path: crates/loredex-git/src/lib.rs
  lines:
  - 55
  - 67
  blob: 9a997999dbccceba8051d0e1fb340596b48576cb
  snippet_sha1: 590b44a09744bf74b0923a315d0d5eeb4c78c925
- path: crates/loredex-git/src/lib.rs
  lines:
  - 83
  - 95
  blob: 9a997999dbccceba8051d0e1fb340596b48576cb
  snippet_sha1: 613cb9d52a71801271ddf3ce00ff039ec602c8f7
- path: crates/loredex-git/src/lib.rs
  lines:
  - 97
  - 141
  blob: 9a997999dbccceba8051d0e1fb340596b48576cb
  snippet_sha1: 79b4b74bfa0f2e5a7f9c31f3ea42f30b05434ddb
- path: crates/loredex-git/src/lib.rs
  lines:
  - 143
  - 150
  blob: 9a997999dbccceba8051d0e1fb340596b48576cb
  snippet_sha1: ba4296b0f42f2e2e02783fe41440734e6f98f245
- path: crates/loredex-git/src/lib.rs
  lines:
  - 152
  - 165
  blob: 9a997999dbccceba8051d0e1fb340596b48576cb
  snippet_sha1: d23025297797b9257bddac4e23e2164e48bd9a9c
type: system
---
# wiki-git：git 只读封装

`GitRepo::discover` 从任意路径向上定位仓库并缓存 workdir [[cite:crates/loredex-git/src/lib.rs:L15-L26]]。证据定位全部基于 HEAD 的提交内容而非工作区：`head_blob_oid` 用 `HEAD:path` revparse 取 blob id [[cite:crates/loredex-git/src/lib.rs:L55-L67]]，`head_lines` 按 1-based 闭区间截取行 [[cite:crates/loredex-git/src/lib.rs:L83-L95]]。

过期交叉验证：`changed_paths_since` 对 (生成时 commit, HEAD) 做 tree diff 并按 evidence 路径设 pathspec [[cite:crates/loredex-git/src/lib.rs:L97-L141]]。`blob_content` 按 oid 直接读对象库，是行号漂移投影能拿到"旧内容"的前提 [[cite:crates/loredex-git/src/lib.rs:L143-L150]]。`tracked_files` 递归遍历 HEAD tree 供模块发现使用 [[cite:crates/loredex-git/src/lib.rs:L152-L165]]。
