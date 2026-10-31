---
id: engine-freshness
title: wiki-engine：过期检测、内容校验与漂移投影
status: verified
generated_at: 2026-10-31T09:20:20Z
commit: 5fe5c2dc04b8fc113e1131e941939b6d327704cf
evidence:
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 86
  - 105
  blob: f2a1f0d8da6d83757e2ef7dd594a64f84837e1fe
  snippet_sha1: b3929d9f21ad47cf8f9df27bf649e8fb24f42200
  note: drift-projected L76-L95@ebb3f7e5 -> L86-L105 (similarity 1.00)
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 125
  - 208
  blob: f2a1f0d8da6d83757e2ef7dd594a64f84837e1fe
  snippet_sha1: 82adea9c441275e383a930c410b267c1820348af
  note: drift-projected L115-L198@ebb3f7e5 -> L125-L208 (similarity 1.00)
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 231
  - 245
  blob: f2a1f0d8da6d83757e2ef7dd594a64f84837e1fe
  snippet_sha1: 2e0cf9a049d9041bbefaa4517ce90d31a1489ea7
  note: drift-projected L221-L235@ebb3f7e5 -> L231-L245 (similarity 1.00)
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 40
  - 79
  blob: 6e32729050d9b6119de47c04813a45825ed45cbf
  snippet_sha1: ad6c8ce1ba654f5491dbe3c72c6063d1a187ce02
  note: drift-projected L40-L79@c57febdf -> L40-L79 (similarity 1.00)
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 29
  - 36
  blob: 6e32729050d9b6119de47c04813a45825ed45cbf
  snippet_sha1: cdcc975c5d3700ad16e50c1a12dc27e1dd0c44a7
  note: drift-projected L29-L36@c57febdf -> L29-L36 (similarity 1.00)
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 217
  - 269
  blob: 6e32729050d9b6119de47c04813a45825ed45cbf
  snippet_sha1: 1d0ae10a29f08fa1ac2ddc4884e094ff6c8a1298
  note: drift-projected L217-L264@c57febdf -> L217-L269 (similarity 1.00)
- path: crates/wiki-engine/src/put.rs
  lines:
  - 18
  - 59
  blob: 7620e01ae59ef0a21e7880d0de5516f4b6682e33
  snippet_sha1: 2c283b898c8909ad858149c3b5e70bf94dbcbf4d
  note: drift-projected L18-L60@f3a81528 -> L18-L59 (similarity 0.88)
---
# wiki-engine：过期检测、内容校验与漂移投影

`record_evidence` 将引用钉到 HEAD：记录 blob oid 并对行区间内容做 sha1 [[cite:crates/wiki-engine/src/lib.rs:L76-L95]]。

`check_page` 的过期判定是双重的：blob 与 HEAD 不一致，或生成 commit→HEAD 的 diff 命中了 evidence 路径，任一成立即 stale；生成 commit 本身不可解析也判 stale [[cite:crates/wiki-engine/src/lib.rs:L115-L178]]。`verify_page` 进一步重算每个行区间的 snippet sha1，与记录值比对，实现"不依赖 commit 号的内容级可信校验" [[cite:crates/wiki-engine/src/lib.rs:L201-L215]]。

行号漂移投影：`project_range` 对旧 blob 与 HEAD 内容做行级 diff，构建旧行号→新行号映射（删除行锚定到最近的存活行，区间坍缩时保持原宽度）[[cite:crates/wiki-engine/src/drift.rs:L31-L70]]；投影后新旧 snippet 相似度低于阈值 0.6 才判 Lost [[cite:crates/wiki-engine/src/drift.rs:L20-L27]]。`refresh_page_with_drift` 逐条投影：可恢复的更新 lines/blob/snippet_sha1 并在 note 留痕，Lost 的降级为 draft 强制重写 [[cite:crates/wiki-engine/src/drift.rs:L148-L190]]。

入库路径 `put_page`：agent 提交的 body 必须含引用标记，逐条转 evidence；零引用直接拒绝 [[cite:crates/wiki-engine/src/put.rs:L18-L60]]。
