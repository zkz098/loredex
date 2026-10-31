---
id: engine-freshness
title: wiki-engine：过期检测、内容校验与漂移投影
status: verified
generated_at: 2026-10-31T12:38:40Z
commit: dd7d0c43dedd0de3296c4118c8b6f5443cb1bfeb
evidence:
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 90
  - 108
  blob: f716c3c7fc54d6c9bfa7dea6970eaddf0b4eadf7
  snippet_sha1: 611eb1281befdd587e8e67de5f8673de82b3e9c7
  note: drift-projected L90-L108@fcac5a00 -> L90-L108 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 129
  - 185
  blob: f716c3c7fc54d6c9bfa7dea6970eaddf0b4eadf7
  snippet_sha1: 776d2500376bf85400f3fd2357513f7222fb07b7
  note: drift-projected L129-L190@fcac5a00 -> L129-L185 (similarity 0.82)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 234
  - 252
  blob: f716c3c7fc54d6c9bfa7dea6970eaddf0b4eadf7
  snippet_sha1: 570e40c2c3b468f39090df5953e52c26670831d1
  note: drift-projected L235-L252@fcac5a00 -> L234-L252 (similarity 0.83)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 34
  - 74
  blob: 75afc4ab4a246e1c9305e2fe7751b5c676b8d41c
  snippet_sha1: 77a85a350863b8b01c39ec3a7af91f2e8d358783
  note: drift-projected L40-L80@8d709d89 -> L34-L74 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 25
  - 30
  blob: 75afc4ab4a246e1c9305e2fe7751b5c676b8d41c
  snippet_sha1: a06223e23808aa9b99a600f1bde0da2d4c045389
  note: drift-projected L31-L36@8d709d89 -> L25-L30 (similarity 1.00)
- path: crates/loredex-engine/src/drift.rs
  lines:
  - 233
  - 274
  blob: 75afc4ab4a246e1c9305e2fe7751b5c676b8d41c
  snippet_sha1: eb3acd65ccd4efbabafaa6da7bbd0611c6f0d9d6
  note: drift-projected L217-L258@8d709d89 -> L233-L274 (similarity 1.00)
- path: crates/loredex-engine/src/put.rs
  lines:
  - 20
  - 67
  blob: af719f7b8477da77230d4ce23c8d91b7c7b3b3a7
  snippet_sha1: 0bb2dd68705ede6062736d48fb709dc80ff3c84d
  note: drift-projected L18-L60@37b6dc5f -> L20-L67 (similarity 0.88)
type: system
---
# wiki-engine：过期检测、内容校验与漂移投影

`record_evidence` 将引用钉到 HEAD：记录 blob oid 并对行区间内容做 sha1 [[cite:crates/loredex-engine/src/lib.rs:L90-L108]]。

`check_page` 的过期判定是双重的：blob 与 HEAD 不一致，或生成 commit→HEAD 的 diff 命中了 evidence 路径，任一成立即 stale；生成 commit 本身不可解析也判 stale [[cite:crates/loredex-engine/src/lib.rs:L129-L190]]。`verify_page` 进一步重算每个行区间的 snippet sha1，与记录值比对，实现"不依赖 commit 号的内容级可信校验" [[cite:crates/loredex-engine/src/lib.rs:L235-L252]]。

行号漂移投影：`project_range` 对旧 blob 与 HEAD 内容做行级 diff，构建旧行号→新行号映射（删除行锚定到最近的存活行，区间坍缩时保持原宽度）[[cite:crates/loredex-engine/src/drift.rs:L40-L80]]；投影后新旧 snippet 相似度低于阈值 0.6 才判 Lost [[cite:crates/loredex-engine/src/drift.rs:L31-L36]]。`refresh_page_with_drift` 逐条投影：可恢复的更新 lines/blob/snippet_sha1 并在 note 留痕，Lost 的降级为 draft 强制重写 [[cite:crates/loredex-engine/src/drift.rs:L217-L258]]。

入库路径 `put_page`：agent 提交的 body 必须含引用标记，逐条转 evidence；零引用直接拒绝 [[cite:crates/loredex-engine/src/put.rs:L18-L60]]。
