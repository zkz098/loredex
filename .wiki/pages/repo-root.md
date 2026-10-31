---
id: repo-root
title: 仓库根与工程化
status: verified
generated_at: 2026-10-31T10:11:09Z
commit: 6fbcd9636a4d01a534e61920acc5758c98aef7b4
evidence:
- path: Cargo.toml
  lines:
  - 1
  - 9
  blob: f345004db21c29c565c558dec023cff808cc7436
  snippet_sha1: 24b7cb3e46654cbf7679c8f53c5803b39cd8eb51
- path: README.md
  lines:
  - 1
  - 6
  blob: b879739b29dda2a50bed67338af8bdc1327b48fb
  snippet_sha1: a0ffa35fba631d36346e64ee44cfa64a9ac68752
- path: .config/nextest.toml
  lines:
  - 1
  - 3
  blob: 41b3bd688d70750d9e1237655d972fc96f146128
  snippet_sha1: 71f568085da5f2c960ee661a2c6ba7a8ba262ace
type: architecture
---
# 仓库根与工程化

本仓库是 cargo workspace，resolver v2，五个 crate 按依赖方向分层 [[cite:Cargo.toml:L1-L9]]。用户文档入口是 README，它同时是对外卖点的唯一叙述面 [[cite:README.md:L1-L6]]。

发布前工程化基线：`cargo nextest run --workspace`（每测试独立进程，env 操纵安全）与 `cargo llvm-cov nextest --workspace` 覆盖率门槛；nextest 的 slow-timeout 配置让锁死锁类故障以失败而非挂起呈现 [[cite:.config/nextest.toml:L1-L3]]。

自动化分发物是 `templates/` 下两个模板：GitHub Action（定时投影+开 issue 工作单）与 post-merge hook（本地静默自愈+提醒），由 `repowiki templates` 导出到用户仓库 [[wiki:architecture]]。
