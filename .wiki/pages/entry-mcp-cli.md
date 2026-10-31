---
id: entry-mcp-cli
title: 入口层：MCP server 与 CLI
status: verified
generated_at: 2026-10-31T10:04:14Z
commit: 2eefe342a01872dd7a037ac697c31468ac07e9ed
evidence:
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 13
  - 27
  blob: b6c5492cc1da082e66c8601bedd9a78528905982
  snippet_sha1: 7126f3da20da71e3801d1cb78e365b136bd8e30e
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 434
  - 460
  blob: b6c5492cc1da082e66c8601bedd9a78528905982
  snippet_sha1: c469b42316340de0e0a1e08d9e51b9f27344efe6
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 281
  - 330
  blob: b6c5492cc1da082e66c8601bedd9a78528905982
  snippet_sha1: faf2ebae8bad11c17b7b96bc90c2ed385bb71d4e
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 463
  - 490
  blob: b6c5492cc1da082e66c8601bedd9a78528905982
  snippet_sha1: 625cce13af77a3e41240b272cef8ad246898dc24
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 365
  - 405
  blob: b6c5492cc1da082e66c8601bedd9a78528905982
  snippet_sha1: a6aab4d8e489de717e9974fff0b2377eb9d02ddf
- path: crates/wiki-cli/src/main.rs
  lines:
  - 21
  - 54
  blob: 644ba14a39ae8603b8f096bd526b85024dc03261
  snippet_sha1: b5c72adc90ab24918f1cb8569487ca0aaba4b3a3
type: system
---
# 入口层：MCP server 与 CLI

MCP server（rmcp 3.8, stdio）在 initialize 时下发 4 条协议契约指令：先查新鲜度、stale 视为未验证、漂移走 refresh、引用可用 get_evidence 审计 [[cite:crates/wiki-mcp/src/lib.rs:L13-L27]]。

所有工具返回 `Json<T>` 结构化输出（structuredContent），关键字段：`stale_now`、`matches_head`、`projectable`。写入工具 `wiki_put_page` 将零引用 body 以 invalid_request 拒绝 [[cite:crates/wiki-mcp/src/lib.rs:L434-L460]]；`wiki_check_freshness` 对 stale 页逐条试投影并标注可恢复项 [[cite:crates/wiki-mcp/src/lib.rs:L281-L330]]；`wiki_refresh_page` 暴露漂移投影结果 [[cite:crates/wiki-mcp/src/lib.rs:L463-L490]]。`wiki_get_evidence` 把单条引用展开为 HEAD 当前内容 [[cite:crates/wiki-mcp/src/lib.rs:L365-L405]]。

工具方法内部对共享 `Arc<Mutex<GitRepo>>` 只允许单层加锁（曾因嵌套 lock 死锁）。CLI 子命令集合见 [[cite:crates/wiki-cli/src/main.rs:L21-L54]]，`check` 以退出码 1 报告 stale，可直接挂 CI。
