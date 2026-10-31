---
id: core-model
title: wiki-core：数据模型与解析
status: verified
generated_at: 2026-10-31T06:25:08Z
commit: ab925999364ec18ebd81e65c0fccaa6af60aecfd
evidence:
- path: crates/wiki-core/src/page.rs
  lines:
  - 9
  - 26
  blob: 823417dc2c265162e3fbfcb8fa0fc453b42c3855
  snippet_sha1: 969389b2ae71fbef2971bc79a759298546071e44
- path: crates/wiki-core/src/page.rs
  lines:
  - 65
  - 91
  blob: 823417dc2c265162e3fbfcb8fa0fc453b42c3855
  snippet_sha1: 293aece1073df13d3589bcd028ff5c64d9f62413
- path: crates/wiki-core/src/page.rs
  lines:
  - 103
  - 112
  blob: 823417dc2c265162e3fbfcb8fa0fc453b42c3855
  snippet_sha1: 5eef3fa63d88110703d4d0e7e36bb9fe699292c5
- path: crates/wiki-core/src/cite.rs
  lines:
  - 16
  - 46
  blob: 3cfb4e6899991afb98fe3d93c06ddbcc71205f3e
  snippet_sha1: 099e32768fe19812faf60a2ea1bda7ee2dfa2f4c
  note: drift-projected L16-L46@37a4ef22 -> L16-L46 (similarity 0.97)
- path: crates/wiki-core/src/cite.rs
  lines:
  - 47
  - 55
  blob: 3cfb4e6899991afb98fe3d93c06ddbcc71205f3e
  snippet_sha1: 9f49ae32804dc6f0860f79e80e8d796286e2ddf0
  note: drift-projected L47-L55@37a4ef22 -> L47-L55 (similarity 1.00)
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
---
# wiki-core：数据模型与解析

页面状态机 `draft → verified → stale`，定义于 [[cite:crates/wiki-core/src/page.rs:L9-L26]]。`WikiPage::from_markdown` 以首个 `---` 与最近的换行三连横线切分 YAML frontmatter 与正文 [[cite:crates/wiki-core/src/page.rs:L65-L91]]；`save` 采用临时文件 + rename 原子写 [[cite:crates/wiki-core/src/page.rs:L103-L112]]。

引用语法为双中括号 cite 冒号路径，可选 `:Lx-Ly` 行区间后缀；解析器正则拒绝 0 行号或逆序区间 [[cite:crates/wiki-core/src/cite.rs:L15-L44]]，并按 (path, lines) 去重 [[cite:crates/wiki-core/src/cite.rs:L45-L53]]。含此语法的字符串若出现在正文说明文字中，会同样被解析为引用——这是 dogfood 中发现的已知陷阱。

搜索为纯内存倒排索引：分词将驼峰/下划线标识符切开、CJK 取重叠二元组 [[cite:crates/wiki-core/src/search.rs:L16-L60]]；查询按词频饱和计分并支持拉丁前缀匹配 [[cite:crates/wiki-core/src/search.rs:L112-L160]]。索引每次从页面文件重建，不存在可损坏的持久缓存。
