---
id: core-model
title: wiki-core：数据模型与解析
status: verified
generated_at: 2026-10-31T10:04:14Z
commit: 2eefe342a01872dd7a037ac697c31468ac07e9ed
evidence:
- path: crates/wiki-core/src/page.rs
  lines:
  - 9
  - 26
  blob: 8a7f7f40314c2e465b9d82e29458ffb45a964f46
  snippet_sha1: 969389b2ae71fbef2971bc79a759298546071e44
- path: crates/wiki-core/src/page.rs
  lines:
  - 65
  - 91
  blob: 8a7f7f40314c2e465b9d82e29458ffb45a964f46
  snippet_sha1: ea5ec5e79cbc20acfa856d317a9c44e02e6e4d68
- path: crates/wiki-core/src/page.rs
  lines:
  - 103
  - 112
  blob: 8a7f7f40314c2e465b9d82e29458ffb45a964f46
  snippet_sha1: b76e51006dde1908b1126ab0200f729263463f4c
- path: crates/wiki-core/src/cite.rs
  lines:
  - 15
  - 44
  blob: 5a9ffc43c0907ffd62ac1196ccc7081393aad952
  snippet_sha1: f98bad59a6e95b3486e684b8249a0671e0983f11
- path: crates/wiki-core/src/cite.rs
  lines:
  - 45
  - 53
  blob: 5a9ffc43c0907ffd62ac1196ccc7081393aad952
  snippet_sha1: 0e1f70fe0f78a4c90ad260cea3378d86be94460a
- path: crates/wiki-core/src/search.rs
  lines:
  - 16
  - 60
  blob: 686ff8abe056e68dc44f4ca7344f49fb49743b52
  snippet_sha1: a16238f7b3915fd95f40938d7114f92cdea69e2b
- path: crates/wiki-core/src/search.rs
  lines:
  - 112
  - 160
  blob: 686ff8abe056e68dc44f4ca7344f49fb49743b52
  snippet_sha1: ee1ddbf56de66e2bcafcd17927e8da845d840ca2
type: system
---
# wiki-core：数据模型与解析

页面状态机 `draft → verified → stale`，定义于 [[cite:crates/wiki-core/src/page.rs:L9-L26]]。`WikiPage::from_markdown` 以首个 `---` 与最近的换行三连横线切分 YAML frontmatter 与正文 [[cite:crates/wiki-core/src/page.rs:L65-L91]]；`save` 采用临时文件 + rename 原子写 [[cite:crates/wiki-core/src/page.rs:L103-L112]]。

引用语法为双中括号 cite 冒号路径，可选 `:Lx-Ly` 行区间后缀；解析器正则拒绝 0 行号或逆序区间 [[cite:crates/wiki-core/src/cite.rs:L15-L44]]，并按 (path, lines) 去重 [[cite:crates/wiki-core/src/cite.rs:L45-L53]]。含此语法的字符串若出现在正文说明文字中，会同样被解析为引用——这是 dogfood 中发现的已知陷阱。

搜索为纯内存倒排索引：分词将驼峰/下划线标识符切开、CJK 取重叠二元组 [[cite:crates/wiki-core/src/search.rs:L16-L60]]；查询按词频饱和计分并支持拉丁前缀匹配 [[cite:crates/wiki-core/src/search.rs:L112-L160]]。索引每次从页面文件重建，不存在可损坏的持久缓存。
