---
id: engine-freshness
title: wiki-engine：过期检测、内容校验与漂移投影
status: verified
generated_at: 2026-10-31T12:28:41Z
commit: 1d51e1c5f143342205c6040d13ad0bb8ae955f1d
evidence:
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 90
  - 108
  blob: fcac5a00a2f8473e23d77eab839a4ea40aa76d8d
  snippet_sha1: 611eb1281befdd587e8e67de5f8673de82b3e9c7
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 129
  - 190
  blob: fcac5a00a2f8473e23d77eab839a4ea40aa76d8d
  snippet_sha1: 2c6be3290ba1c8e387ca1d56199a03340d82c226
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 235
  - 252
  blob: fcac5a00a2f8473e23d77eab839a4ea40aa76d8d
  snippet_sha1: 67bdfc83faf9ba748bc09c605e3bef850d2f3755
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 40
  - 80
  blob: 8d709d899792ad41e4f6026d2f0cb995c8cbdad0
  snippet_sha1: 77a85a350863b8b01c39ec3a7af91f2e8d358783
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 31
  - 36
  blob: 8d709d899792ad41e4f6026d2f0cb995c8cbdad0
  snippet_sha1: a06223e23808aa9b99a600f1bde0da2d4c045389
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 217
  - 258
  blob: 8d709d899792ad41e4f6026d2f0cb995c8cbdad0
  snippet_sha1: eb3acd65ccd4efbabafaa6da7bbd0611c6f0d9d6
- path: crates/loredex-engine/src/put.rs
  lines:
  - 18
  - 60
  blob: 37b6dc5f6921521af332d42e1e39157a3dc05294
  snippet_sha1: ebdb070d1e6a99f131422d0895701d32db662a6b
type: system
---
# wiki-engine：过期检测、内容校验与漂移投影

`record_evidence` 将引用钉到 HEAD：记录 blob oid 并对行区间内容做 sha1 [[cite:crates/loredex-engine/src/lib.rs:L90-L108]]。

`check_page` 的过期判定是双重的：blob 与 HEAD 不一致，或生成 commit→HEAD 的 diff 命中了 evidence 路径，任一成立即 stale；生成 commit 本身不可解析也判 stale [[cite:crates/loredex-engine/src/lib.rs:L129-L190]]。`verify_page` 进一步重算每个行区间的 snippet sha1，与记录值比对，实现"不依赖 commit 号的内容级可信校验" [[cite:crates/loredex-engine/src/lib.rs:L235-L252]]。

行号漂移投影：`project_range` 对旧 blob 与 HEAD 内容做行级 diff，构建旧行号→新行号映射（删除行锚定到最近的存活行，区间坍缩时保持原宽度）[[cite:crates/loredex-engine/src/drift.rs:L40-L80]]；投影后新旧 snippet 相似度低于阈值 0.6 才判 Lost [[cite:crates/loredex-engine/src/drift.rs:L31-L36]]。`refresh_page_with_drift` 逐条投影：可恢复的更新 lines/blob/snippet_sha1 并在 note 留痕，Lost 的降级为 draft 强制重写 [[cite:crates/loredex-engine/src/drift.rs:L217-L258]]。

入库路径 `put_page`：agent 提交的 body 必须含引用标记，逐条转 evidence；零引用直接拒绝 [[cite:crates/loredex-engine/src/put.rs:L18-L60]]。
