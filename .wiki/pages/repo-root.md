---
id: repo-root
title: Repository root and engineering
status: verified
generated_at: 2026-11-16T11:40:24Z
commit: 6aa0c0c82b2e5edbf551321b1c26add6f8d9f63a
evidence:
- path: Cargo.toml
  lines:
  - 1
  - 9
  blob: fe252926d15f1d8a894c2bc8f41ed2223b7efafa
  snippet_sha1: 5313a611bf4562bc9dd637aa2e8cfecb6eed6074
  note: drift-projected L1-L9@7d094478 -> L1-L9 (similarity 1.00)
- path: README.md
  lines:
  - 1
  - 9
  blob: a35d4653338648f27bb08e4aac265a95c8f71d99
  snippet_sha1: 2cffae28322fc373ba6a5d601fd9469a90f1aa11
  note: drift-projected L1-L9@4bcac374 -> L1-L9 (similarity 1.00)
- path: .config/nextest.toml
  lines:
  - 1
  - 3
  blob: 41b3bd688d70750d9e1237655d972fc96f146128
  snippet_sha1: 71f568085da5f2c960ee661a2c6ba7a8ba262ace
type: architecture
---
# Repository root and engineering

The repository is a cargo workspace (resolver v2) whose five crates layer strictly by dependency direction [[cite:Cargo.toml:L1-L9]]. README.md is the English entry point (Chinese at README_cn.md) and the single narrative surface for positioning, install and protocol [[cite:README.md:L1-L9]].

The pre-commit/CI baseline: `cargo nextest run --workspace` (one process per test, so env manipulation is safe) and `cargo llvm-cov nextest` with a 70%-lines coverage gate in CI; nextest's slow-timeout turns lock deadlocks into failures instead of hangs [[cite:.config/nextest.toml:L1-L3]].

Automation is distributed as two templates exported by `loredex templates`: a GitHub Action (scheduled projection + plan-issue) and a post-merge hook (local silent self-heal with a reminder) — and this repository runs its own action on the `.wiki/` it eats, see [[wiki:architecture]].
