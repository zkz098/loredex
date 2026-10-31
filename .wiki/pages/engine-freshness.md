---
id: engine-freshness
title: wiki-engine：过期检测、内容校验与漂移投影
status: verified
generated_at: 2026-10-31T08:59:53Z
commit: 0f60abeec631b68b376d721d411064df074d5950
evidence:
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 76
  - 95
  blob: ebb3f7e5e419e62bd5f4d73789b8a330f7df41f8
  snippet_sha1: b3929d9f21ad47cf8f9df27bf649e8fb24f42200
  note: drift-projected L76-L95@4fc0060e -> L76-L95 (similarity 1.00)
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 115
  - 198
  blob: ebb3f7e5e419e62bd5f4d73789b8a330f7df41f8
  snippet_sha1: 82adea9c441275e383a930c410b267c1820348af
  note: drift-projected L115-L178@4fc0060e -> L115-L198 (similarity 0.98)
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 221
  - 235
  blob: ebb3f7e5e419e62bd5f4d73789b8a330f7df41f8
  snippet_sha1: 2e0cf9a049d9041bbefaa4517ce90d31a1489ea7
  note: drift-projected L201-L215@4fc0060e -> L221-L235 (similarity 1.00)
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 40
  - 79
  blob: c57febdf0b90675dd0a0f4dd2f85e1190604516a
  snippet_sha1: ad6c8ce1ba654f5491dbe3c72c6063d1a187ce02
  note: drift-projected L31-L70@50c4749b -> L40-L79 (similarity 1.00)
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 29
  - 36
  blob: c57febdf0b90675dd0a0f4dd2f85e1190604516a
  snippet_sha1: cdcc975c5d3700ad16e50c1a12dc27e1dd0c44a7
  note: drift-projected L20-L27@50c4749b -> L29-L36 (similarity 1.00)
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 217
  - 264
  blob: c57febdf0b90675dd0a0f4dd2f85e1190604516a
  snippet_sha1: af0059c058b23109622aa13ddae55913da069b28
  note: drift-projected L148-L190@50c4749b -> L217-L264 (similarity 0.98)
- path: crates/wiki-engine/src/put.rs
  lines:
  - 18
  - 60
  blob: f3a81528bc7d063cfe51209a5057dc35a23957a9
  snippet_sha1: 5454865c617ae541d39724e13756562d1a609f67
---
# wiki-engine：过期检测、内容校验与漂移投影

`record_evidence` 将引用钉到 HEAD：记录 blob oid 并对行区间内容做 sha1 [[cite:crates/wiki-engine/src/lib.rs:L76-L95]]。

`check_page` 的过期判定是双重的：blob 与 HEAD 不一致，或生成 commit→HEAD 的 diff 命中了 evidence 路径，任一成立即 stale；生成 commit 本身不可解析也判 stale [[cite:crates/wiki-engine/src/lib.rs:L115-L178]]。`verify_page` 进一步重算每个行区间的 snippet sha1，与记录值比对，实现"不依赖 commit 号的内容级可信校验" [[cite:crates/wiki-engine/src/lib.rs:L201-L215]]。

行号漂移投影：`project_range` 对旧 blob 与 HEAD 内容做行级 diff，构建旧行号→新行号映射（删除行锚定到最近的存活行，区间坍缩时保持原宽度）[[cite:crates/wiki-engine/src/drift.rs:L31-L70]]；投影后新旧 snippet 相似度低于阈值 0.6 才判 Lost [[cite:crates/wiki-engine/src/drift.rs:L20-L27]]。`refresh_page_with_drift` 逐条投影：可恢复的更新 lines/blob/snippet_sha1 并在 note 留痕，Lost 的降级为 draft 强制重写 [[cite:crates/wiki-engine/src/drift.rs:L148-L190]]。

入库路径 `put_page`：agent 提交的 body 必须含引用标记，逐条转 evidence；零引用直接拒绝 [[cite:crates/wiki-engine/src/put.rs:L18-L60]]。
