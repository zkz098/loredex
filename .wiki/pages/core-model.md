---
id: core-model
title: wiki-core：数据模型与解析
status: verified
generated_at: 2026-10-31T12:38:40Z
commit: dd7d0c43dedd0de3296c4118c8b6f5443cb1bfeb
evidence:
- path: crates/loredex-core/src/page.rs
  lines:
  - 9
  - 26
  blob: e814a40a848673def3eb54918a306230323ab57e
  snippet_sha1: 969389b2ae71fbef2971bc79a759298546071e44
  note: drift-projected L9-L26@8a7f7f40 -> L9-L26 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 68
  - 94
  blob: e814a40a848673def3eb54918a306230323ab57e
  snippet_sha1: 293aece1073df13d3589bcd028ff5c64d9f62413
  note: drift-projected L68-L94@8a7f7f40 -> L68-L94 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 112
  - 124
  blob: e814a40a848673def3eb54918a306230323ab57e
  snippet_sha1: c2851e6ee8582e5996db4fabb92712f011c35a8a
  note: drift-projected L106-L115@8a7f7f40 -> L112-L124 (similarity 0.90)
- path: crates/loredex-core/src/cite.rs
  lines:
  - 17
  - 48
  blob: 7c13345cd8f6b37e43dfc9feab20fdeb141d73f0
  snippet_sha1: 7feaf9a2dc20ac97b290b999ad7ad50df5190ec4
  note: drift-projected L17-L45@5a9ffc43 -> L17-L48 (similarity 0.96)
- path: crates/loredex-core/src/cite.rs
  lines:
  - 50
  - 58
  blob: 7c13345cd8f6b37e43dfc9feab20fdeb141d73f0
  snippet_sha1: 9f49ae32804dc6f0860f79e80e8d796286e2ddf0
  note: drift-projected L47-L55@5a9ffc43 -> L50-L58 (similarity 1.00)
- path: crates/loredex-core/src/search.rs
  lines:
  - 16
  - 67
  blob: 43bd12e11aaec651533de243ed002ed88ef5bf18
  snippet_sha1: 2ee041598080a40998d83778745f2e2b3a0ee9e5
  note: drift-projected L16-L60@686ff8ab -> L16-L67 (similarity 0.98)
- path: crates/loredex-core/src/search.rs
  lines:
  - 119
  - 177
  blob: 43bd12e11aaec651533de243ed002ed88ef5bf18
  snippet_sha1: 77d85e50ee5c84ac7814c8f6949b621bee30b3bc
  note: drift-projected L112-L160@686ff8ab -> L119-L177 (similarity 0.92)
type: system
---
# wiki-core：数据模型与解析

页面状态机 `draft → verified → stale`，定义于 [[cite:crates/loredex-core/src/page.rs:L9-L26]]。`WikiPage::from_markdown` 以首个 `---` 与最近的换行三连横线切分 YAML frontmatter 与正文 [[cite:crates/loredex-core/src/page.rs:L68-L94]]；`save` 采用临时文件 + rename 原子写 [[cite:crates/loredex-core/src/page.rs:L106-L115]]。

引用语法为双中括号 cite 冒号路径，可选 `:Lx-Ly` 行区间后缀；解析器正则拒绝 0 行号或逆序区间 [[cite:crates/loredex-core/src/cite.rs:L17-L45]]，并按 (path, lines) 去重 [[cite:crates/loredex-core/src/cite.rs:L47-L55]]。含此语法的字符串若出现在正文说明文字中，会同样被解析为引用——这是 dogfood 中发现的已知陷阱。

搜索为纯内存倒排索引：分词将驼峰/下划线标识符切开、CJK 取重叠二元组 [[cite:crates/loredex-core/src/search.rs:L16-L60]]；查询按词频饱和计分并支持拉丁前缀匹配 [[cite:crates/loredex-core/src/search.rs:L112-L160]]。索引每次从页面文件重建，不存在可损坏的持久缓存。
