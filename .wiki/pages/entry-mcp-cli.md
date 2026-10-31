---
id: entry-mcp-cli
title: 入口层：MCP server 与 CLI
status: verified
generated_at: 2026-10-31T12:38:40Z
commit: dd7d0c43dedd0de3296c4118c8b6f5443cb1bfeb
evidence:
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 16
  - 37
  blob: 372678cfc21e34760f235e041b79fc0b05dd3815
  snippet_sha1: c4d6803623c6856b89ae65bac3d1bad6e40f9657
  note: drift-projected L13-L34@b8d56815 -> L16-L37 (similarity 1.00)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 463
  - 500
  blob: 372678cfc21e34760f235e041b79fc0b05dd3815
  snippet_sha1: dcad0e03b6d42de21c5adbc0454da6ac46edd828
  note: drift-projected L434-L460@b8d56815 -> L463-L500 (similarity 0.85)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 575
  - 629
  blob: 372678cfc21e34760f235e041b79fc0b05dd3815
  snippet_sha1: eed71446943545c1e9db446ff18484bbe23657e0
  note: drift-projected L528-L575@b8d56815 -> L575-L629 (similarity 0.92)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 503
  - 538
  blob: 372678cfc21e34760f235e041b79fc0b05dd3815
  snippet_sha1: 1af8dbde32efea57e4df3d685780f6367e4e2f81
  note: drift-projected L463-L490@b8d56815 -> L503-L538 (similarity 0.86)
- path: crates/loredex-mcp/src/lib.rs
  lines:
  - 373
  - 422
  blob: 372678cfc21e34760f235e041b79fc0b05dd3815
  snippet_sha1: fea225fdd09429e69d02d171ce7277f166baa666
  note: drift-projected L365-L405@b8d56815 -> L373-L422 (similarity 0.93)
- path: crates/loredex-cli/src/main.rs
  lines:
  - 25
  - 75
  blob: 7800424e8e44e00edc78fc7265aedf01f4a33ecd
  snippet_sha1: 3aacd3c3ea97884790fb1c66af586fbdc403a1a6
  note: drift-projected L22-L72@1ec52da9 -> L25-L75 (similarity 1.00)
type: system
---
# 入口层：MCP server 与 CLI

MCP server（rmcp 3.8, stdio）在 initialize 时下发 4 条协议契约指令：先查新鲜度、stale 视为未验证、漂移走 refresh、引用可用 get_evidence 审计 [[cite:crates/loredex-mcp/src/lib.rs:L13-L34]]。

所有工具返回 `Json<T>` 结构化输出（structuredContent），关键字段：`stale_now`、`matches_head`、`projectable`。写入工具 `wiki_put_page` 将零引用 body 以 invalid_request 拒绝 [[cite:crates/loredex-mcp/src/lib.rs:L434-L460]]；`wiki_check_freshness` 对 stale 页逐条试投影并标注可恢复项 [[cite:crates/loredex-mcp/src/lib.rs:L528-L575]]；`wiki_refresh_page` 暴露漂移投影结果 [[cite:crates/loredex-mcp/src/lib.rs:L463-L490]]。`wiki_get_evidence` 把单条引用展开为 HEAD 当前内容 [[cite:crates/loredex-mcp/src/lib.rs:L365-L405]]。

工具方法内部对共享 `Arc<Mutex<GitRepo>>` 只允许单层加锁（曾因嵌套 lock 死锁）。CLI 子命令集合见 [[cite:crates/loredex-cli/src/main.rs:L22-L72]]，`check` 以退出码 1 报告 stale，可直接挂 CI。
