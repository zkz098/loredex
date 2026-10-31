---
id: architecture
title: 总体架构
status: verified
generated_at: 2026-10-31T12:38:40Z
commit: dd7d0c43dedd0de3296c4118c8b6f5443cb1bfeb
evidence:
- path: Cargo.toml
  lines:
  - 2
  - 9
  blob: c548aeb1cd615b0910f518e0a0fcd1b5d747b566
  snippet_sha1: cdb93553223a82b1c268fcf7d31a0b5c0dd928c0
  note: drift-projected L2-L9@d90ecb19 -> L2-L9 (similarity 1.00)
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 274
  - 286
  blob: f716c3c7fc54d6c9bfa7dea6970eaddf0b4eadf7
  snippet_sha1: 02ad668b7bdf2cb5db082b398c984b1233f151ee
  note: drift-projected L264-L276@fcac5a00 -> L274-L286 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 29
  - 40
  blob: e814a40a848673def3eb54918a306230323ab57e
  snippet_sha1: c29606ed08aec635918031cfd27810613803de0c
  note: drift-projected L29-L40@8a7f7f40 -> L29-L40 (similarity 1.00)
type: architecture
---
# 总体架构

repowiki 是 cargo workspace，依赖方向自上而下：wiki-core（纯数据模型）← wiki-git（git 只读封装）← wiki-engine（业务逻辑）← 两个入口 CLI/MCP [[cite:Cargo.toml:L2-L9]]。

存储为纯 Markdown：wiki 页面存放在仓库的 `.wiki/pages/*.md`，正文 frontmatter 内嵌机器可读的 evidence；`.wiki/.cache/` 为 gitignore 的可丢弃目录，无权威数据库 [[cite:crates/loredex-engine/src/lib.rs:L264-L276]]。

可信性模型：每条 evidence 钉住 `(路径, 行区间, 生成时的 git blob sha, snippet 内容 sha1)` [[cite:crates/loredex-core/src/page.rs:L29-L40]]。过期判定不信任 commit 号本身，而是对比 blob 并用 git diff 交叉验证；校验则直接重算行区间内容哈希。
