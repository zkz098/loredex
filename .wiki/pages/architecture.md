---
id: architecture
title: 总体架构
status: verified
generated_at: 2026-10-31T08:50:59Z
commit: 11fb67f355bd5b29a6b5a6187382ac22bff485ef
evidence:
- path: Cargo.toml
  lines:
  - 2
  - 9
  blob: f5c9e850d708484b64bf96e354c09bdb29ec4ac3
  snippet_sha1: 31ef060985310bbefb6844dddd9c673008bc9091
  note: drift-projected L2-L9@3feb9be7 -> L2-L9 (similarity 1.00)
- path: crates/wiki-engine/src/lib.rs
  lines:
  - 264
  - 276
  blob: 4fc0060eaf8d8f508bd1bc9740871f091420daab
  snippet_sha1: 9dbe75483ba6a7831324b4204d4136d61c920987
- path: crates/wiki-core/src/page.rs
  lines:
  - 29
  - 40
  blob: 823417dc2c265162e3fbfcb8fa0fc453b42c3855
  snippet_sha1: c29606ed08aec635918031cfd27810613803de0c
---
# 总体架构

repowiki 是 cargo workspace，依赖方向自上而下：wiki-core（纯数据模型）← wiki-git（git 只读封装）← wiki-engine（业务逻辑）← 两个入口 CLI/MCP [[cite:Cargo.toml:L2-L9]]。

存储为纯 Markdown：wiki 页面存放在仓库的 `.wiki/pages/*.md`，正文 frontmatter 内嵌机器可读的 evidence；`.wiki/.cache/` 为 gitignore 的可丢弃目录，无权威数据库 [[cite:crates/wiki-engine/src/lib.rs:L264-L276]]。

可信性模型：每条 evidence 钉住 `(路径, 行区间, 生成时的 git blob sha, snippet 内容 sha1)` [[cite:crates/wiki-core/src/page.rs:L29-L40]]。过期判定不信任 commit 号本身，而是对比 blob 并用 git diff 交叉验证；校验则直接重算行区间内容哈希。
