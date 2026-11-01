---
id: testing-gates
title: Test and CI layers
status: verified
generated_at: 2026-11-01T03:19:47Z
commit: 28096161c173a87ea8e68061fb3994b1937f1847
evidence:
- path: crates/loredex-mcp/tests/mcp_resolution.rs
  lines:
  - 1
  - 6
  blob: c6ab107ca0957652d0811e439e98210b86fb2772
  snippet_sha1: 0c87247e0a7694c9e7857121a373cd081db676fb
- path: .github/workflows/ci.yml
  lines:
  - 19
  - 22
  blob: b38152f1dc6ff2ebf907e499fa04653ecdcfc701
  snippet_sha1: 5edf5908f6af0b86bf40171a278baa6cb7c5319b
- path: .github/workflows/ci.yml
  lines:
  - 27
  - 28
  blob: b38152f1dc6ff2ebf907e499fa04653ecdcfc701
  snippet_sha1: 121304982946f9a55c391d324d91bc89b82ea3c5
- path: .github/workflows/ci.yml
  lines:
  - 38
  - 39
  blob: b38152f1dc6ff2ebf907e499fa04653ecdcfc701
  snippet_sha1: 9802eeb2ed9062715ecb3df1ec2c7659b679dc8f
- path: .github/workflows/ci.yml
  lines:
  - 47
  - 48
  blob: b38152f1dc6ff2ebf907e499fa04653ecdcfc701
  snippet_sha1: 4485bd35e473ae9e143f85e37494d2f348e44f17
- path: .github/workflows/ci.yml
  lines:
  - 66
  - 68
  blob: b38152f1dc6ff2ebf907e499fa04653ecdcfc701
  snippet_sha1: a257e989009413de9728abc2726e44ed13113b63
- path: crates/loredex-engine/tests/knowledge.rs
  lines:
  - 37
  - 45
  blob: 66d62f35f3827e7e03d39b2fe41e542787945024
  snippet_sha1: ab56e1edd1bb8cf41540f1038f21673bf36ed1d3
- path: crates/loredex-mcp/tests/mcp_resolution.rs
  lines:
  - 8
  - 12
  blob: c6ab107ca0957652d0811e439e98210b86fb2772
  snippet_sha1: 6c63d4529ab644a369bfaeb78dd4c6f3cd19d28c
- path: crates/loredex-engine/tests/knowledge.rs
  lines:
  - 215
  - 232
  blob: 66d62f35f3827e7e03d39b2fe41e542787945024
  snippet_sha1: 26a4b6f62d729ad57f49715531d5bdb6cbaa9cf4
- path: crates/loredex-engine/tests/knowledge.rs
  lines:
  - 140
  - 166
  blob: 66d62f35f3827e7e03d39b2fe41e542787945024
  snippet_sha1: ca8899fc27648bd1b3fbaa3c695f4300b206ca1e
- path: crates/loredex-engine/tests/knowledge.rs
  lines:
  - 374
  - 378
  blob: 66d62f35f3827e7e03d39b2fe41e542787945024
  snippet_sha1: b898efe5fb0fd3df79c3cf921330a75b23a52bc6
- path: crates/loredex-engine/tests/knowledge.rs
  lines:
  - 232
  - 250
  blob: 66d62f35f3827e7e03d39b2fe41e542787945024
  snippet_sha1: 5602dd8de02da6b264dff2fb5792b7de75b564bd
- path: crates/loredex-engine/tests/redact_evidence.rs
  lines:
  - 64
  - 80
  blob: 27413517b7cb091439ed6f2ce589a8b3053c7762
  snippet_sha1: 69451bfa625efa8f0dad30bf3179b768cec3041b
- path: .github/workflows/loredex-refresh.yml
  lines:
  - 1
  - 8
  blob: 1375de9f9ed3255fed18fe13af0e5dcfe87ce252
  snippet_sha1: 624c525e2a4250629e8698f20da707df84ad646d
- path: Cargo.toml
  lines:
  - 1
  - 16
  blob: abf859da79ebc5ef0865f2c4b0342bc9cee29cca
  snippet_sha1: a5da00a1b7b63ebbc6e7a2067ceb6e9da004b86e
- path: .cargo/config.toml
  lines:
  - 1
  - 6
  blob: 13e2d106c85aafedf9d10653a5fe19415eadff04
  snippet_sha1: 461eac94d33022a3ddecd3287f398fca42a00469
tags:
- policy
type: testing
---
# Test and CI layers

**Layers.** Unit tests sit next to the code; behaviour is asserted at integration level under `crates/*/tests/`. `loredex-engine` covers the wiki lifecycle end to end (e2e staleness/drift, plan+ledger, features, diff+links, redaction, the knowledge pipeline), `loredex-core` covers page and citation parsing, `loredex-git` covers diff and path classification, and `loredex-mcp` drives the real MCP protocol in-process over a duplex transport, so tool dispatch, the roots capability gate and structured output are exercised the way a client sees them [[cite:crates/loredex-mcp/tests/mcp_resolution.rs:L1-L6]].

**How to run, in the order CI enforces.** Locally, before pushing:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
```

clippy runs with warnings denied [[cite:.github/workflows/ci.yml:L19-L22]] and an unused-dependency sweep gates the same job [[cite:.github/workflows/ci.yml:L27-L28]]; the test job is plain nextest over the workspace [[cite:.github/workflows/ci.yml:L38-L39]]. Two extra gates exist because this is a whole-repo scanner: a hard regression budget on the page-scan benchmark [[cite:.github/workflows/ci.yml:L47-L48]] and a coverage baseline of 70% lines [[cite:.github/workflows/ci.yml:L66-L68]]. While iterating, run one suite: `cargo nextest run -p loredex-engine --test knowledge`.

**Fixtures and harness.** Integration suites build a throwaway git repository in a tempdir, commit the fixture files, then drive the engine against real blobs — freshness and drift projection cannot be rehearsed against a mock [[cite:crates/loredex-engine/tests/knowledge.rs:L37-L45]]. Tests that touch `LOREDEX_REPO` share a serialization lock: nextest isolates every test in its own process, plain `cargo test` threads them [[cite:crates/loredex-mcp/tests/mcp_resolution.rs:L8-L12]].

**Regressions the suite encodes** — each is a bug that shipped or nearly shipped:

- an uncited body must never enter as `verified` [[cite:crates/loredex-engine/tests/knowledge.rs:L215-L232]], and a draft capture must not silently resolve its plan task [[cite:crates/loredex-engine/tests/knowledge.rs:L140-L166]];
- evidence overlap is a *pair* judgement above a coincidence threshold, never a transitive cluster that swallows unrelated pages [[cite:crates/loredex-engine/tests/knowledge.rs:L374-L378]];
- advisory findings cannot redden CI: only non-advisory issues keep `auto` from calling the wiki clean [[cite:crates/loredex-engine/tests/knowledge.rs:L232-L250]];
- a secret-shaped citation is refused at write time, naming the offending range [[cite:crates/loredex-engine/tests/redact_evidence.rs:L64-L80]] — the gate also refused *this* page the first time it was written, because the range I had picked included a fixture's fake access key.

Wiki maintenance is exercised too: this repository's own `.wiki/` is refreshed on a schedule, committing what projection recovers and opening an issue only for genuine re-authoring [[cite:.github/workflows/loredex-refresh.yml:L1-L8]].

**Workspace facts.** Five crates under `crates/` (core / git / engine / cli / mcp), edition 2021, Rust 1.85 [[cite:Cargo.toml:L1-L16]]. Release builds default to an `x86-64-v3` CPU baseline, so the CPU you test on is not necessarily the one a user has — cross-platform CI jobs matter more here than for a portable CLI [[cite:.cargo/config.toml:L1-L6]].

Related: [[wiki:rule-advisory-never-fails-ci]], [[wiki:entry-mcp-cli]], [[wiki:repo-root]].
