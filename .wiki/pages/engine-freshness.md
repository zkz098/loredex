---
id: engine-freshness
title: wiki-engine：过期检测、内容校验与漂移投影
status: verified
generated_at: 2026-10-31T10:04:14Z
commit: 2eefe342a01872dd7a037ac697c31468ac07e9ed
evidence:
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 76
  - 95
  blob: f2a1f0d8da6d83757e2ef7dd594a64f84837e1fe
  snippet_sha1: f76a6e2717d20841af87dda437988cda828eadda
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 115
  - 178
  blob: f2a1f0d8da6d83757e2ef7dd594a64f84837e1fe
  snippet_sha1: 6bf3ce89311733fadeec8bc6e287be190c3b2deb
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 201
  - 215
  blob: f2a1f0d8da6d83757e2ef7dd594a64f84837e1fe
  snippet_sha1: ba6b3906fa868db06a16382dfa1fc9d704460794
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 31
  - 70
  blob: f1a630c647995bec2a9118d75f35629cd973f22c
  snippet_sha1: 45a5a0d72dedebfc6e92927dadb22183404f6532
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 20
  - 27
  blob: f1a630c647995bec2a9118d75f35629cd973f22c
  snippet_sha1: 958c222a24b6afdd7765e663da83a87969b30d12
- path: crates/wiki-engine/src/drift.rs
  lines:
  - 148
  - 190
  blob: f1a630c647995bec2a9118d75f35629cd973f22c
  snippet_sha1: b50a901988db441a42e7aad3d0c52b3e5416ab49
- path: crates/wiki-engine/src/put.rs
  lines:
  - 18
  - 60
  blob: 7620e01ae59ef0a21e7880d0de5516f4b6682e33
  snippet_sha1: e7c4a2f77b6e9ae2f188278b0f7ad86f90c38950
type: system
---
# wiki-engine：过期检测、内容校验与漂移投影

`record_evidence` 将引用钉到 HEAD：记录 blob oid 并对行区间内容做 sha1 [[cite:crates/wiki-engine/src/lib.rs:L76-L95]]。

`check_page` 的过期判定是双重的：blob 与 HEAD 不一致，或生成 commit→HEAD 的 diff 命中了 evidence 路径，任一成立即 stale；生成 commit 本身不可解析也判 stale [[cite:crates/wiki-engine/src/lib.rs:L115-L178]]。`verify_page` 进一步重算每个行区间的 snippet sha1，与记录值比对，实现"不依赖 commit 号的内容级可信校验" [[cite:crates/wiki-engine/src/lib.rs:L201-L215]]。

行号漂移投影：`project_range` 对旧 blob 与 HEAD 内容做行级 diff，构建旧行号→新行号映射（删除行锚定到最近的存活行，区间坍缩时保持原宽度）[[cite:crates/wiki-engine/src/drift.rs:L31-L70]]；投影后新旧 snippet 相似度低于阈值 0.6 才判 Lost [[cite:crates/wiki-engine/src/drift.rs:L20-L27]]。`refresh_page_with_drift` 逐条投影：可恢复的更新 lines/blob/snippet_sha1 并在 note 留痕，Lost 的降级为 draft 强制重写 [[cite:crates/wiki-engine/src/drift.rs:L148-L190]]。

入库路径 `put_page`：agent 提交的 body 必须含引用标记，逐条转 evidence；零引用直接拒绝 [[cite:crates/wiki-engine/src/put.rs:L18-L60]]。
