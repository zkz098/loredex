---
id: entry-mcp-cli
title: 入口层：MCP server 与 CLI
status: verified
generated_at: 2026-10-31T08:50:59Z
commit: 11fb67f355bd5b29a6b5a6187382ac22bff485ef
evidence:
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 13
  - 27
  blob: 60348614a0a09cbcd050cd0aa3255d27ede64ece
  snippet_sha1: 7126f3da20da71e3801d1cb78e365b136bd8e30e
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 434
  - 460
  blob: 60348614a0a09cbcd050cd0aa3255d27ede64ece
  snippet_sha1: 11be1fe73a971bfcedd160e45c32918ceea9868a
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 281
  - 330
  blob: 60348614a0a09cbcd050cd0aa3255d27ede64ece
  snippet_sha1: e047f0e999076e9190db745ff4c9312f7dd2b0c5
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 463
  - 490
  blob: 60348614a0a09cbcd050cd0aa3255d27ede64ece
  snippet_sha1: cfbfd4ce826b8c4a48ce70de3e7c18f9c05ba032
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 365
  - 405
  blob: 60348614a0a09cbcd050cd0aa3255d27ede64ece
  snippet_sha1: 89afa4e23e1ab7723774678b501c35ff13e49061
- path: crates/wiki-cli/src/main.rs
  lines:
  - 21
  - 54
  blob: 48bbff454e7322794d9e0d14c8dc677e845e4623
  snippet_sha1: d3348ac276888a9c4561eb814467b36d39b1b7e9
---
# 入口层：MCP server 与 CLI

MCP server（rmcp 3.8, stdio）在 initialize 时下发 4 条协议契约指令：先查新鲜度、stale 视为未验证、漂移走 refresh、引用可用 get_evidence 审计 [[cite:crates/wiki-mcp/src/lib.rs:L13-L27]]。

所有工具返回 `Json<T>` 结构化输出（structuredContent），关键字段：`stale_now`、`matches_head`、`projectable`。写入工具 `wiki_put_page` 将零引用 body 以 invalid_request 拒绝 [[cite:crates/wiki-mcp/src/lib.rs:L434-L460]]；`wiki_check_freshness` 对 stale 页逐条试投影并标注可恢复项 [[cite:crates/wiki-mcp/src/lib.rs:L281-L330]]；`wiki_refresh_page` 暴露漂移投影结果 [[cite:crates/wiki-mcp/src/lib.rs:L463-L490]]。`wiki_get_evidence` 把单条引用展开为 HEAD 当前内容 [[cite:crates/wiki-mcp/src/lib.rs:L365-L405]]。

工具方法内部对共享 `Arc<Mutex<GitRepo>>` 只允许单层加锁（曾因嵌套 lock 死锁）。CLI 子命令集合见 [[cite:crates/wiki-cli/src/main.rs:L21-L54]]，`check` 以退出码 1 报告 stale，可直接挂 CI。
