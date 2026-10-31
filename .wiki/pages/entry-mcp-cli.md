---
id: entry-mcp-cli
title: 入口层：MCP server 与 CLI
status: verified
generated_at: 2026-10-31T09:20:20Z
commit: 5fe5c2dc04b8fc113e1131e941939b6d327704cf
evidence:
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 13
  - 27
  blob: b6c5492cc1da082e66c8601bedd9a78528905982
  snippet_sha1: 7126f3da20da71e3801d1cb78e365b136bd8e30e
  note: drift-projected L13-L27@bfa09fef -> L13-L27 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 493
  - 526
  blob: b6c5492cc1da082e66c8601bedd9a78528905982
  snippet_sha1: 31f245ff3e87dcca02ea51a48a3045753a05e0a4
  note: drift-projected L434-L467@bfa09fef -> L493-L526 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 293
  - 342
  blob: b6c5492cc1da082e66c8601bedd9a78528905982
  snippet_sha1: e047f0e999076e9190db745ff4c9312f7dd2b0c5
  note: drift-projected L281-L330@bfa09fef -> L293-L342 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 529
  - 556
  blob: b6c5492cc1da082e66c8601bedd9a78528905982
  snippet_sha1: cfbfd4ce826b8c4a48ce70de3e7c18f9c05ba032
  note: drift-projected L470-L497@bfa09fef -> L529-L556 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 423
  - 463
  blob: b6c5492cc1da082e66c8601bedd9a78528905982
  snippet_sha1: 89afa4e23e1ab7723774678b501c35ff13e49061
  note: drift-projected L365-L405@bfa09fef -> L423-L463 (similarity 1.00)
- path: crates/wiki-cli/src/main.rs
  lines:
  - 22
  - 73
  blob: 644ba14a39ae8603b8f096bd526b85024dc03261
  snippet_sha1: 0d7ff3225c75749c42a992ac693c6cbd91f9bf90
  note: drift-projected L21-L54@48bbff45 -> L22-L73 (similarity 1.00)
---
# 入口层：MCP server 与 CLI

MCP server（rmcp 3.8, stdio）在 initialize 时下发 4 条协议契约指令：先查新鲜度、stale 视为未验证、漂移走 refresh、引用可用 get_evidence 审计 [[cite:crates/wiki-mcp/src/lib.rs:L13-L27]]。

所有工具返回 `Json<T>` 结构化输出（structuredContent），关键字段：`stale_now`、`matches_head`、`projectable`。写入工具 `wiki_put_page` 将零引用 body 以 invalid_request 拒绝 [[cite:crates/wiki-mcp/src/lib.rs:L434-L460]]；`wiki_check_freshness` 对 stale 页逐条试投影并标注可恢复项 [[cite:crates/wiki-mcp/src/lib.rs:L281-L330]]；`wiki_refresh_page` 暴露漂移投影结果 [[cite:crates/wiki-mcp/src/lib.rs:L463-L490]]。`wiki_get_evidence` 把单条引用展开为 HEAD 当前内容 [[cite:crates/wiki-mcp/src/lib.rs:L365-L405]]。

工具方法内部对共享 `Arc<Mutex<GitRepo>>` 只允许单层加锁（曾因嵌套 lock 死锁）。CLI 子命令集合见 [[cite:crates/wiki-cli/src/main.rs:L21-L54]]，`check` 以退出码 1 报告 stale，可直接挂 CI。
