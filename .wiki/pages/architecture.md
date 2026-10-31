---
id: architecture
title: 总体架构
status: verified
generated_at: 2026-10-31T09:20:20Z
commit: 5fe5c2dc04b8fc113e1131e941939b6d327704cf
evidence:
- path: Cargo.toml
  lines:
  - 2
  - 9
  blob: f5c9e850d708484b64bf96e354c09bdb29ec4ac3
  snippet_sha1: 31ef060985310bbefb6844dddd9c673008bc9091
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 274
  - 286
  blob: f2a1f0d8da6d83757e2ef7dd594a64f84837e1fe
  snippet_sha1: 6868d15af20453a2d80cbe56de6950153900bc3f
  note: drift-projected L264-L276@ebb3f7e5 -> L274-L286 (similarity 1.00)
- path: crates/wiki-core/src/page.rs
  lines:
  - 29
  - 40
  blob: 8a7f7f40314c2e465b9d82e29458ffb45a964f46
  snippet_sha1: c29606ed08aec635918031cfd27810613803de0c
  note: drift-projected L29-L40@823417dc -> L29-L40 (similarity 1.00)
---
# 总体架构

repowiki 是 cargo workspace，依赖方向自上而下：wiki-core（纯数据模型）← wiki-git（git 只读封装）← wiki-engine（业务逻辑）← 两个入口 CLI/MCP [[cite:Cargo.toml:L2-L9]]。

存储为纯 Markdown：wiki 页面存放在仓库的 `.wiki/pages/*.md`，正文 frontmatter 内嵌机器可读的 evidence；`.wiki/.cache/` 为 gitignore 的可丢弃目录，无权威数据库 [[cite:crates/wiki-engine/src/lib.rs:L264-L276]]。

可信性模型：每条 evidence 钉住 `(路径, 行区间, 生成时的 git blob sha, snippet 内容 sha1)` [[cite:crates/wiki-core/src/page.rs:L29-L40]]。过期判定不信任 commit 号本身，而是对比 blob 并用 git diff 交叉验证；校验则直接重算行区间内容哈希。
