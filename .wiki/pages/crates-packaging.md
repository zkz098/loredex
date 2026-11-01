---
id: crates-packaging
title: 'Crate packaging: one release train, five units'
status: verified
generated_at: 2026-11-01T03:31:30Z
commit: d43a1b59b393a2bf4ccd67984e1bbf430d256db0
evidence:
- path: Cargo.toml
  lines:
  - 1
  - 16
  blob: abf859da79ebc5ef0865f2c4b0342bc9cee29cca
  snippet_sha1: a5da00a1b7b63ebbc6e7a2067ceb6e9da004b86e
- path: crates/loredex-core/Cargo.toml
  lines:
  - 1
  - 15
  blob: 1c565aeef8f3f77893fcaada27bdbef2021664be
  snippet_sha1: 77f389761a7d351698fa837caedb15376e7eb09c
- path: crates/loredex-git/Cargo.toml
  lines:
  - 1
  - 16
  blob: 25086f1d8b1e3f31b245efbb0a3738b515afaa12
  snippet_sha1: 0eccea42a28fa90a10ad2cddac0ab70049b2fe4d
- path: crates/loredex-engine/Cargo.toml
  lines:
  - 1
  - 20
  blob: c1808bc2a1d03998bbed1aaf4e14f168201b1e24
  snippet_sha1: e5d719801c8c972d24b1268927854a25ff529d92
- path: crates/loredex-cli/Cargo.toml
  lines:
  - 1
  - 21
  blob: 2cdf6694c6114ed0f9551cec0705db15dbc4be8e
  snippet_sha1: 50fe96955d9141952ee19408b9fd9d2a8ca8b7f2
- path: crates/loredex-core/README.md
  lines:
  - 1
  - 4
  blob: fb42ff8b19adfec06bb8de948bcba866783835ea
  snippet_sha1: e6aaea3e3bc51b206c2e6d4de0394da6e08fb570
- path: crates/loredex-git/README.md
  lines:
  - 1
  - 4
  blob: ec5571df5ffea883cd8b458d37f9fe1ed0eb6349
  snippet_sha1: f003db60b1516184dbf3565df65c7e65db7e20b2
- path: crates/loredex-engine/README.md
  lines:
  - 1
  - 4
  blob: 7b9bf513d6b710cf61282966828227e9667b5b52
  snippet_sha1: 4c2958d2ecb57f637d83cba770fedcf0b09a9a0b
- path: crates/loredex-cli/Cargo.toml
  lines:
  - 10
  - 21
  blob: 2cdf6694c6114ed0f9551cec0705db15dbc4be8e
  snippet_sha1: 0840fec66c6d7dc107800da11839e70e2d556b22
- path: crates/loredex-cli/README.md
  lines:
  - 1
  - 4
  blob: 7ba82f3d3914d7e3567dd7907360889412356208
  snippet_sha1: 07fd47f78dbb5191fce67eac53acd4ce6ad9f948
- path: crates/loredex-mcp/Cargo.toml
  lines:
  - 10
  - 16
  blob: 686677fb0bafcaa0ec8256bc70de6421ea8a467a
  snippet_sha1: dd43c03404ad817965d7394f802438711f0863f6
- path: crates/loredex-mcp/README.md
  lines:
  - 1
  - 4
  blob: 5ed4487018bf5cb770b30bcd1adcc24ae589b396
  snippet_sha1: a05cf669be8d4f48c71fc4e1e666801541206057
- path: crates/loredex-engine/Cargo.toml
  lines:
  - 22
  - 29
  blob: c1808bc2a1d03998bbed1aaf4e14f168201b1e24
  snippet_sha1: 95a2a02e460b7e837c08703daf6661517e9a8ed5
- path: crates/loredex-cli/src/main.rs
  lines:
  - 671
  - 675
  blob: ee39b2bb45a936c44a4cb13ce936e990cebdc605
  snippet_sha1: e187a2a82afad82d21785360a8b1bbc44abc69f6
- path: crates/loredex-cli/templates/loredex-refresh.yml
  lines:
  - 1
  - 13
  blob: 039a23c4776de624dd4dceb566febc6caeceb471
  snippet_sha1: 52501d01389c043dde417b1e9ec94f9fae546230
- path: crates/loredex-cli/templates/loredex-post-merge
  lines:
  - 1
  - 9
  blob: 197ac3cf62ee7d7d3cdf44c528417a57546e512f
  snippet_sha1: fe1b6fa14528f84b41a6dcf4ebc9f916aec0213e
tags:
- policy
type: architecture
---
# Crate packaging: one release train, five publishable units

The workspace publishes five independent crates rather than one monolith, and every one of them inherits its identity from the root package so a release bumps them together [[cite:Cargo.toml:L1-L16]]. Per-crate metadata therefore carries only a `description` and its dependency edge, never a version [[cite:crates/loredex-core/Cargo.toml:L1-L15]] — which is what makes `cargo install loredex-cli loredex-mcp` and the cargo-dist archives describe the same state of the code.

**Dependency direction is enforced by the manifests**, not by convention: core depends on nothing in the workspace; the git layer depends on core [[cite:crates/loredex-git/Cargo.toml:L1-L16]]; the engine depends on core and git and owns every business rule [[cite:crates/loredex-engine/Cargo.toml:L1-L20]]; the CLI and the MCP server are the only two faces and both depend on the engine [[cite:crates/loredex-cli/Cargo.toml:L1-L21]]. Each crate's README states that position in the chain, so a contributor reading the directory sees the layering before the code [[cite:crates/loredex-core/README.md:L1-L4]] [[cite:crates/loredex-git/README.md:L1-L4]] [[cite:crates/loredex-engine/README.md:L1-L4]].

**Two entry points, deliberately split.** `loredex-cli` builds the single `loredex` binary — argument mapping and printing only [[cite:crates/loredex-cli/Cargo.toml:L10-L21]] [[cite:crates/loredex-cli/README.md:L1-L4]]. `loredex-mcp` declares a lib *and* a bin, and the lib exists so the integration tests can drive the real protocol in-process instead of spawning a child [[cite:crates/loredex-mcp/Cargo.toml:L10-L16]] [[cite:crates/loredex-mcp/README.md:L1-L4]].

**Benchmarks are a packaged concern too.** The engine declares a `scan` bench with `harness = false`, so the regression budget runs as a cargo bench target rather than a test [[cite:crates/loredex-engine/Cargo.toml:L22-L29]]; see [[wiki:testing-gates]] for the CI gate that consumes it.

**Automation artifacts ship inside the CLI.** `loredex templates --out contrib` writes the scheduled-maintenance workflow and the post-merge hook out of files compiled into the binary [[cite:crates/loredex-cli/src/main.rs:L671-L675]]; the workflow template documents the setup contract it expects a user to adjust, including why `contents: write` is optional [[cite:crates/loredex-cli/templates/loredex-refresh.yml:L1-L13]], and the hook stays silent when loredex or `.wiki` is absent so it is safe to install everywhere [[cite:crates/loredex-cli/templates/loredex-post-merge:L1-L9]].

Related: [[wiki:architecture]] for the runtime partitioning, [[wiki:entry-mcp-cli]] for the two faces' behavior, [[wiki:decision-no-database]] for the storage choice.
