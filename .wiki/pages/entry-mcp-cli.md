---
id: entry-mcp-cli
title: 入口层：MCP server 与 CLI
status: verified
generated_at: 2026-10-31T12:28:41Z
commit: 1d51e1c5f143342205c6040d13ad0bb8ae955f1d
evidence:
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 13
  - 34
  blob: b8d56815a8a962e476ff2883f6d9f58a5252081d
  snippet_sha1: c4d6803623c6856b89ae65bac3d1bad6e40f9657
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 434
  - 460
  blob: b8d56815a8a962e476ff2883f6d9f58a5252081d
  snippet_sha1: 10ee0efee1ae460459724456b972aacdf2235b39
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 528
  - 575
  blob: b8d56815a8a962e476ff2883f6d9f58a5252081d
  snippet_sha1: 364b96e7c95b07e38a909b54d481887533b09eec
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 463
  - 490
  blob: b8d56815a8a962e476ff2883f6d9f58a5252081d
  snippet_sha1: 57456b4a2ca33ff3d209e3160692a8eeb48375c3
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 365
  - 405
  blob: b8d56815a8a962e476ff2883f6d9f58a5252081d
  snippet_sha1: 60e52a81813bbd2a06c26ebbfb1ea97afd69b245
- path: crates/loredex-cli/src/main.rs
  lines:
  - 22
  - 72
  blob: 1ec52da9ff3f777a1add6b314e31812c9a81587d
  snippet_sha1: 3aacd3c3ea97884790fb1c66af586fbdc403a1a6
type: system
---
# 入口层：MCP server 与 CLI

MCP server（rmcp 3.8, stdio）在 initialize 时下发 4 条协议契约指令：先查新鲜度、stale 视为未验证、漂移走 refresh、引用可用 get_evidence 审计 [[cite:crates/loredex-mcp/src/lib.rs:L13-L34]]。

所有工具返回 `Json<T>` 结构化输出（structuredContent），关键字段：`stale_now`、`matches_head`、`projectable`。写入工具 `wiki_put_page` 将零引用 body 以 invalid_request 拒绝 [[cite:crates/loredex-mcp/src/lib.rs:L434-L460]]；`wiki_check_freshness` 对 stale 页逐条试投影并标注可恢复项 [[cite:crates/loredex-mcp/src/lib.rs:L528-L575]]；`wiki_refresh_page` 暴露漂移投影结果 [[cite:crates/loredex-mcp/src/lib.rs:L463-L490]]。`wiki_get_evidence` 把单条引用展开为 HEAD 当前内容 [[cite:crates/loredex-mcp/src/lib.rs:L365-L405]]。

工具方法内部对共享 `Arc<Mutex<GitRepo>>` 只允许单层加锁（曾因嵌套 lock 死锁）。CLI 子命令集合见 [[cite:crates/loredex-cli/src/main.rs:L22-L72]]，`check` 以退出码 1 报告 stale，可直接挂 CI。
