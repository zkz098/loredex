---
id: architecture
title: 总体架构
status: verified
generated_at: 2026-10-31T12:29:04Z
commit: 1d51e1c5f143342205c6040d13ad0bb8ae955f1d
evidence:
- path: Cargo.toml
  lines:
  - 2
  - 9
  blob: d90ecb196ee1a92ffee0dd63ab4b77f2ae37cea7
  snippet_sha1: cdb93553223a82b1c268fcf7d31a0b5c0dd928c0
- path: crates/loredex-engine/src/lib.rs
  lines:
  - 264
  - 276
  blob: fcac5a00a2f8473e23d77eab839a4ea40aa76d8d
  snippet_sha1: 02ad668b7bdf2cb5db082b398c984b1233f151ee
- path: crates/loredex-core/src/page.rs
  lines:
  - 29
  - 40
  blob: 8a7f7f40314c2e465b9d82e29458ffb45a964f46
  snippet_sha1: c29606ed08aec635918031cfd27810613803de0c
type: architecture
---
# 总体架构

repowiki 是 cargo workspace，依赖方向自上而下：wiki-core（纯数据模型）← wiki-git（git 只读封装）← wiki-engine（业务逻辑）← 两个入口 CLI/MCP [[cite:Cargo.toml:L2-L9]]。

存储为纯 Markdown：wiki 页面存放在仓库的 `.wiki/pages/*.md`，正文 frontmatter 内嵌机器可读的 evidence；`.wiki/.cache/` 为 gitignore 的可丢弃目录，无权威数据库 [[cite:crates/loredex-engine/src/lib.rs:L264-L276]]。

可信性模型：每条 evidence 钉住 `(路径, 行区间, 生成时的 git blob sha, snippet 内容 sha1)` [[cite:crates/loredex-core/src/page.rs:L29-L40]]。过期判定不信任 commit 号本身，而是对比 blob 并用 git diff 交叉验证；校验则直接重算行区间内容哈希。
