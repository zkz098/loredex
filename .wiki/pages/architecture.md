---
id: architecture
title: 总体架构
status: verified
generated_at: 2026-10-31T10:10:53Z
commit: 6fbcd9636a4d01a534e61920acc5758c98aef7b4
evidence:
- path: Cargo.toml
  lines:
  - 2
  - 9
  blob: f345004db21c29c565c558dec023cff808cc7436
  snippet_sha1: 31ef060985310bbefb6844dddd9c673008bc9091
  note: drift-projected L2-L9@f5c9e850 -> L2-L9 (similarity 1.00)
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 268
  - 280
  blob: c4ecfd9459348311c218185ba0056ff9c391bf3e
  snippet_sha1: 14d520db4237840502828874b31ebf6f071e18c8
  note: drift-projected L268-L280@9bd86f9a -> L268-L280 (similarity 1.00)
- path: crates/wiki-core/src/page.rs
  lines:
  - 29
  - 40
  blob: 8a7f7f40314c2e465b9d82e29458ffb45a964f46
  snippet_sha1: c29606ed08aec635918031cfd27810613803de0c
type: architecture
---
# 总体架构

repowiki 是 cargo workspace，依赖方向自上而下：wiki-core（纯数据模型）← wiki-git（git 只读封装）← wiki-engine（业务逻辑）← 两个入口 CLI/MCP [[cite:Cargo.toml:L2-L9]]。

存储为纯 Markdown：wiki 页面存放在仓库的 `.wiki/pages/*.md`，正文 frontmatter 内嵌机器可读的 evidence；`.wiki/.cache/` 为 gitignore 的可丢弃目录，无权威数据库 [[cite:crates/wiki-engine/src/lib.rs:L264-L276]]。

可信性模型：每条 evidence 钉住 `(路径, 行区间, 生成时的 git blob sha, snippet 内容 sha1)` [[cite:crates/wiki-core/src/page.rs:L29-L40]]。过期判定不信任 commit 号本身，而是对比 blob 并用 git diff 交叉验证；校验则直接重算行区间内容哈希。
