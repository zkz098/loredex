---
id: core-model
title: wiki-core：数据模型与解析
status: verified
generated_at: 2026-10-31T12:28:41Z
commit: 1d51e1c5f143342205c6040d13ad0bb8ae955f1d
evidence:
- path: crates/loredex-core/src/page.rs
  lines:
  - 9
  - 26
  blob: 8a7f7f40314c2e465b9d82e29458ffb45a964f46
  snippet_sha1: 969389b2ae71fbef2971bc79a759298546071e44
- path: crates/loredex-core/src/page.rs
  lines:
  - 68
  - 94
  blob: 8a7f7f40314c2e465b9d82e29458ffb45a964f46
  snippet_sha1: 293aece1073df13d3589bcd028ff5c64d9f62413
- path: crates/loredex-core/src/page.rs
  lines:
  - 106
  - 115
  blob: 8a7f7f40314c2e465b9d82e29458ffb45a964f46
  snippet_sha1: 55f7cd120f99578059c718ab7a1721c04430ae3c
- path: crates/loredex-core/src/cite.rs
  lines:
  - 17
  - 45
  blob: 5a9ffc43c0907ffd62ac1196ccc7081393aad952
  snippet_sha1: e017a162a4931f12fafbeeb47044aa9fb7e7a907
- path: crates/loredex-core/src/cite.rs
  lines:
  - 47
  - 55
  blob: 5a9ffc43c0907ffd62ac1196ccc7081393aad952
  snippet_sha1: 9f49ae32804dc6f0860f79e80e8d796286e2ddf0
- path: crates/loredex-core/src/search.rs
  lines:
  - 16
  - 60
  blob: 686ff8abe056e68dc44f4ca7344f49fb49743b52
  snippet_sha1: a16238f7b3915fd95f40938d7114f92cdea69e2b
- path: crates/loredex-core/src/search.rs
  lines:
  - 112
  - 160
  blob: 686ff8abe056e68dc44f4ca7344f49fb49743b52
  snippet_sha1: ee1ddbf56de66e2bcafcd17927e8da845d840ca2
type: system
---
# wiki-core：数据模型与解析

页面状态机 `draft → verified → stale`，定义于 [[cite:crates/loredex-core/src/page.rs:L9-L26]]。`WikiPage::from_markdown` 以首个 `---` 与最近的换行三连横线切分 YAML frontmatter 与正文 [[cite:crates/loredex-core/src/page.rs:L68-L94]]；`save` 采用临时文件 + rename 原子写 [[cite:crates/loredex-core/src/page.rs:L106-L115]]。

引用语法为双中括号 cite 冒号路径，可选 `:Lx-Ly` 行区间后缀；解析器正则拒绝 0 行号或逆序区间 [[cite:crates/loredex-core/src/cite.rs:L17-L45]]，并按 (path, lines) 去重 [[cite:crates/loredex-core/src/cite.rs:L47-L55]]。含此语法的字符串若出现在正文说明文字中，会同样被解析为引用——这是 dogfood 中发现的已知陷阱。

搜索为纯内存倒排索引：分词将驼峰/下划线标识符切开、CJK 取重叠二元组 [[cite:crates/loredex-core/src/search.rs:L16-L60]]；查询按词频饱和计分并支持拉丁前缀匹配 [[cite:crates/loredex-core/src/search.rs:L112-L160]]。索引每次从页面文件重建，不存在可损坏的持久缓存。
