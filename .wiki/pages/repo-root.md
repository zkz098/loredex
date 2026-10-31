---
id: repo-root
title: 仓库根与工程化
status: verified
generated_at: 2026-10-31T12:38:40Z
commit: dd7d0c43dedd0de3296c4118c8b6f5443cb1bfeb
evidence:
- path: Cargo.toml
  lines:
  - 1
  - 9
  blob: c548aeb1cd615b0910f518e0a0fcd1b5d747b566
  snippet_sha1: 5313a611bf4562bc9dd637aa2e8cfecb6eed6074
  note: drift-projected L1-L9@d90ecb19 -> L1-L9 (similarity 1.00)
- path: README.md
  lines:
  - 1
  - 9
  blob: d43556aa7c539171753d6368aeb5564bdc9528e1
  snippet_sha1: 20f47e4407be332a12e1ea438df09aec009c8f38
  note: drift-projected L1-L6@fdf0643c -> L1-L9 (similarity 1.00)
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
