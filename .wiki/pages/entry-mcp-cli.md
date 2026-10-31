---
id: entry-mcp-cli
title: 入口层：MCP server 与 CLI
status: verified
generated_at: 2026-10-31T10:08:06Z
commit: aa235362bbd98253ac936f90f5959a1599fdaebc
evidence:
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 13
  - 27
  blob: f9e3ae53ebb6cf0b89b7a18125b95fcc80afa55a
  snippet_sha1: 7126f3da20da71e3801d1cb78e365b136bd8e30e
  note: drift-projected L13-L27@9af0a740 -> L13-L27 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 462
  - 488
  blob: f9e3ae53ebb6cf0b89b7a18125b95fcc80afa55a
  snippet_sha1: c469b42316340de0e0a1e08d9e51b9f27344efe6
  note: drift-projected L448-L474@9af0a740 -> L462-L488 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 281
  - 330
  blob: f9e3ae53ebb6cf0b89b7a18125b95fcc80afa55a
  snippet_sha1: faf2ebae8bad11c17b7b96bc90c2ed385bb71d4e
  note: drift-projected L281-L330@9af0a740 -> L281-L330 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 491
  - 518
  blob: f9e3ae53ebb6cf0b89b7a18125b95fcc80afa55a
  snippet_sha1: 625cce13af77a3e41240b272cef8ad246898dc24
  note: drift-projected L477-L504@9af0a740 -> L491-L518 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 365
  - 433
  blob: f9e3ae53ebb6cf0b89b7a18125b95fcc80afa55a
  snippet_sha1: 9208eb5aa08e41fa88ca42e4eb4fc1efa78aee75
  note: drift-projected L365-L419@9af0a740 -> L365-L433 (similarity 1.00)
- path: crates/wiki-cli/src/main.rs
  lines:
  - 21
  - 54
  blob: 8d14929a02579d0bc771fcdc9ce938bb0645f3e3
  snippet_sha1: b5c72adc90ab24918f1cb8569487ca0aaba4b3a3
  note: drift-projected L21-L54@e0b29841 -> L21-L54 (similarity 1.00)
type: system
---
# 入口层：MCP server 与 CLI

MCP server（rmcp 3.8, stdio）在 initialize 时下发 4 条协议契约指令：先查新鲜度、stale 视为未验证、漂移走 refresh、引用可用 get_evidence 审计 [[cite:crates/wiki-mcp/src/lib.rs:L13-L27]]。

所有工具返回 `Json<T>` 结构化输出（structuredContent），关键字段：`stale_now`、`matches_head`、`projectable`。写入工具 `wiki_put_page` 将零引用 body 以 invalid_request 拒绝 [[cite:crates/wiki-mcp/src/lib.rs:L434-L460]]；`wiki_check_freshness` 对 stale 页逐条试投影并标注可恢复项 [[cite:crates/wiki-mcp/src/lib.rs:L281-L330]]；`wiki_refresh_page` 暴露漂移投影结果 [[cite:crates/wiki-mcp/src/lib.rs:L463-L490]]。`wiki_get_evidence` 把单条引用展开为 HEAD 当前内容 [[cite:crates/wiki-mcp/src/lib.rs:L365-L405]]。

工具方法内部对共享 `Arc<Mutex<GitRepo>>` 只允许单层加锁（曾因嵌套 lock 死锁）。CLI 子命令集合见 [[cite:crates/wiki-cli/src/main.rs:L21-L54]]，`check` 以退出码 1 报告 stale，可直接挂 CI。
