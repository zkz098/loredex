---
id: repo-root
title: 仓库根与工程化
status: draft
generated_at: 2026-10-31T12:26:49Z
commit: 1d51e1c5f143342205c6040d13ad0bb8ae955f1d
evidence:
- path: Cargo.toml
  lines:
  - 1
  - 9
  blob: d90ecb196ee1a92ffee0dd63ab4b77f2ae37cea7
  snippet_sha1: 5313a611bf4562bc9dd637aa2e8cfecb6eed6074
- path: README.md
  lines:
  - 1
  - 6
  blob: fdf0643c206dae527fb472c355f7f2911b6e6c65
  snippet_sha1: ff5363b1b6524e4b425282d134d3369155d7d5de
  note: drift-projected L1-L6@b879739b -> L1-L6 (similarity 0.80)
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
