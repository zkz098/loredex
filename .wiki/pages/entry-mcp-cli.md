---
id: entry-mcp-cli
title: 入口层：MCP server 与 CLI
status: verified
generated_at: 2026-10-31T06:11:59Z
commit: 5cd7de119ad6c36ea4d26a6135f5c455a10d6470
evidence:
- path: crates/wiki-mcp/src/main.rs
  lines:
  - 13
  - 27
  blob: 0858bf1657cd9bf1f3b7a0c39b912cda6431e34a
  snippet_sha1: bba4ad020ec9a640906abbe751c7a6d9b776fcf4
- path: crates/wiki-mcp/src/main.rs
  lines:
  - 434
  - 460
  blob: 0858bf1657cd9bf1f3b7a0c39b912cda6431e34a
  snippet_sha1: 8c96cfca935319c2763c8131e2895860cdd457a4
- path: crates/wiki-mcp/src/main.rs
  lines:
  - 281
  - 330
  blob: 0858bf1657cd9bf1f3b7a0c39b912cda6431e34a
  snippet_sha1: a7183fe7539d1e5c27eec9955184634f796ead43
- path: crates/wiki-mcp/src/main.rs
  lines:
  - 463
  - 490
  blob: 0858bf1657cd9bf1f3b7a0c39b912cda6431e34a
  snippet_sha1: 603e23b04c5926ecaaf03b00453d7003f0b75e83
- path: crates/wiki-mcp/src/main.rs
  lines:
  - 365
  - 405
  blob: 0858bf1657cd9bf1f3b7a0c39b912cda6431e34a
  snippet_sha1: 11b9eb57277a8eac0a6837faba30493e548d0205
- path: crates/wiki-cli/src/main.rs
  lines:
  - 21
  - 54
  blob: 48bbff454e7322794d9e0d14c8dc677e845e4623
  snippet_sha1: d3348ac276888a9c4561eb814467b36d39b1b7e9
---
# 入口层：MCP server 与 CLI

MCP server（rmcp 3.8, stdio）在 initialize 时下发 4 条协议契约指令：先查新鲜度、stale 视为未验证、漂移走 refresh、引用可用 get_evidence 审计 [[cite:crates/wiki-mcp/src/main.rs:L13-L27]]。

所有工具返回 `Json<T>` 结构化输出（structuredContent），关键字段：`stale_now`、`matches_head`、`projectable`。写入工具 `wiki_put_page` 将零引用 body 以 invalid_request 拒绝 [[cite:crates/wiki-mcp/src/main.rs:L434-L460]]；`wiki_check_freshness` 对 stale 页逐条试投影并标注可恢复项 [[cite:crates/wiki-mcp/src/main.rs:L281-L330]]；`wiki_refresh_page` 暴露漂移投影结果 [[cite:crates/wiki-mcp/src/main.rs:L463-L490]]。`wiki_get_evidence` 把单条引用展开为 HEAD 当前内容 [[cite:crates/wiki-mcp/src/main.rs:L365-L405]]。

工具方法内部对共享 `Arc<Mutex<GitRepo>>` 只允许单层加锁（曾因嵌套 lock 死锁）。CLI 子命令集合见 [[cite:crates/wiki-cli/src/main.rs:L21-L54]]，`check` 以退出码 1 报告 stale，可直接挂 CI。
