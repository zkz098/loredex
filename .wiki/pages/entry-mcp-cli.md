---
id: entry-mcp-cli
title: 入口层：MCP server 与 CLI
status: verified
generated_at: 2026-10-31T06:37:04Z
commit: 9300a9e46e346049071d99ec7b5433db1d158189
evidence:
- path: crates/wiki-mcp/src/main.rs
  lines:
  - 13
  - 26
  blob: 96f3586f9003759ec3c801e6e70569a6e61f237e
  snippet_sha1: 4e5d13878e567c64c7faf162d26c8124c6cddd3b
  note: drift-projected L13-L27@0858bf16 -> L13-L26 (similarity 0.87)
- path: crates/wiki-mcp/src/main.rs
  lines:
  - 599
  - 628
  blob: 96f3586f9003759ec3c801e6e70569a6e61f237e
  snippet_sha1: 02c857a3fa10bbd39c13ca2ae633a29b7129308a
  note: drift-projected L434-L460@0858bf16 -> L599-L628 (similarity 0.93)
- path: crates/wiki-mcp/src/main.rs
  lines:
  - 438
  - 492
  blob: 96f3586f9003759ec3c801e6e70569a6e61f237e
  snippet_sha1: ebecfd1d6b0a7fe5327b349dd772b9230ce67937
  note: drift-projected L281-L330@0858bf16 -> L438-L492 (similarity 0.92)
- path: crates/wiki-mcp/src/main.rs
  lines:
  - 630
  - 660
  blob: 96f3586f9003759ec3c801e6e70569a6e61f237e
  snippet_sha1: d09252da0f8b077f027807042e55d6f4353ca157
  note: drift-projected L463-L490@0858bf16 -> L630-L660 (similarity 0.93)
- path: crates/wiki-mcp/src/main.rs
  lines:
  - 526
  - 569
  blob: 96f3586f9003759ec3c801e6e70569a6e61f237e
  snippet_sha1: a90fffd59738d29694297da772e81fb9e5626cc0
  note: drift-projected L365-L405@0858bf16 -> L526-L569 (similarity 0.95)
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
