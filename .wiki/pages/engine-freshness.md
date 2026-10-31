---
id: engine-freshness
title: wiki-engine：过期检测、内容校验与漂移投影
status: verified
generated_at: 2026-10-31T06:11:59Z
commit: 5cd7de119ad6c36ea4d26a6135f5c455a10d6470
evidence:
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 76
  - 95
  blob: 4fc0060eaf8d8f508bd1bc9740871f091420daab
  snippet_sha1: b3929d9f21ad47cf8f9df27bf649e8fb24f42200
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 115
  - 178
  blob: 4fc0060eaf8d8f508bd1bc9740871f091420daab
  snippet_sha1: cfb80d566b7057d11ac2a2f75a26ae9514077106
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 201
  - 215
  blob: 4fc0060eaf8d8f508bd1bc9740871f091420daab
  snippet_sha1: 2e0cf9a049d9041bbefaa4517ce90d31a1489ea7
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 31
  - 70
  blob: 50c4749bdf9c7b096e16b1a50587bba5a42444ab
  snippet_sha1: ad6c8ce1ba654f5491dbe3c72c6063d1a187ce02
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 20
  - 27
  blob: 50c4749bdf9c7b096e16b1a50587bba5a42444ab
  snippet_sha1: cdcc975c5d3700ad16e50c1a12dc27e1dd0c44a7
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 148
  - 190
  blob: 50c4749bdf9c7b096e16b1a50587bba5a42444ab
  snippet_sha1: fb3404b5ffad92101af3e5f95e91ce91b7abd7d8
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
