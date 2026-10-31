---
id: entry-mcp-cli
title: 入口层：MCP server 与 CLI
status: verified
generated_at: 2026-10-31T10:10:53Z
commit: 6fbcd9636a4d01a534e61920acc5758c98aef7b4
evidence:
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 13
  - 29
  blob: 262cddf2dec4e291ba4d8cd9914ad7f9f03136fb
  snippet_sha1: d9fa80b3e38253764133e0334673e31b33a2b78b
  note: drift-projected L13-L27@f9e3ae53 -> L13-L29 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 464
  - 490
  blob: 262cddf2dec4e291ba4d8cd9914ad7f9f03136fb
  snippet_sha1: c469b42316340de0e0a1e08d9e51b9f27344efe6
  note: drift-projected L462-L488@f9e3ae53 -> L464-L490 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 283
  - 332
  blob: 262cddf2dec4e291ba4d8cd9914ad7f9f03136fb
  snippet_sha1: faf2ebae8bad11c17b7b96bc90c2ed385bb71d4e
  note: drift-projected L281-L330@f9e3ae53 -> L283-L332 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 493
  - 520
  blob: 262cddf2dec4e291ba4d8cd9914ad7f9f03136fb
  snippet_sha1: 625cce13af77a3e41240b272cef8ad246898dc24
  note: drift-projected L491-L518@f9e3ae53 -> L493-L520 (similarity 1.00)
- path: crates/wiki-mcp/src/lib.rs
  lines:
  - 367
  - 435
  blob: 262cddf2dec4e291ba4d8cd9914ad7f9f03136fb
  snippet_sha1: 9208eb5aa08e41fa88ca42e4eb4fc1efa78aee75
  note: drift-projected L365-L433@f9e3ae53 -> L367-L435 (similarity 1.00)
- path: crates/wiki-cli/src/main.rs
  lines:
  - 21
  - 54
  blob: ce076e7325300e9274ad6cfe5c2865fab23e4b73
  snippet_sha1: b5c72adc90ab24918f1cb8569487ca0aaba4b3a3
  note: drift-projected L21-L54@8d14929a -> L21-L54 (similarity 1.00)
type: system
---
# 入口层：MCP server 与 CLI

MCP server（rmcp 3.8, stdio）在 initialize 时下发 4 条协议契约指令：先查新鲜度、stale 视为未验证、漂移走 refresh、引用可用 get_evidence 审计 [[cite:crates/wiki-mcp/src/lib.rs:L13-L27]]。

所有工具返回 `Json<T>` 结构化输出（structuredContent），关键字段：`stale_now`、`matches_head`、`projectable`。写入工具 `wiki_put_page` 将零引用 body 以 invalid_request 拒绝 [[cite:crates/wiki-mcp/src/lib.rs:L434-L460]]；`wiki_check_freshness` 对 stale 页逐条试投影并标注可恢复项 [[cite:crates/wiki-mcp/src/lib.rs:L281-L330]]；`wiki_refresh_page` 暴露漂移投影结果 [[cite:crates/wiki-mcp/src/lib.rs:L463-L490]]。`wiki_get_evidence` 把单条引用展开为 HEAD 当前内容 [[cite:crates/wiki-mcp/src/lib.rs:L365-L405]]。

工具方法内部对共享 `Arc<Mutex<GitRepo>>` 只允许单层加锁（曾因嵌套 lock 死锁）。CLI 子命令集合见 [[cite:crates/wiki-cli/src/main.rs:L21-L54]]，`check` 以退出码 1 报告 stale，可直接挂 CI。
