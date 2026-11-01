---
id: quickstart
title: Wiki quickstart
status: verified
generated_at: 2026-11-01T03:31:30Z
commit: d43a1b59b393a2bf4ccd67984e1bbf430d256db0
evidence: []
tags:
- generated
type: index
---
# repowiki wiki — quickstart

Generated navigation, not evidence-backed. Full catalog: [[wiki:index]].

## Recommended reading path

### architecture

1. [[wiki:architecture]] — Architecture
1. [[wiki:crates-packaging]] — Crate packaging: one release train, five units
1. [[wiki:repo-root]] — Repository root and engineering

### system

1. [[wiki:core-model]] — loredex-core: data model and parsing
1. [[wiki:engine-freshness]] — loredex-engine: staleness, verification, drift projection
1. [[wiki:entry-mcp-cli]] — Entry layer: MCP server and CLI
1. [[wiki:git-layer]] — loredex-git: read-only git wrapper

### testing

1. [[wiki:testing-gates]] — Test and CI layers

### rule

1. [[wiki:rule-advisory-never-fails-ci]] — Curation hints are advisory and never break CI
1. [[wiki:rule-evidence-contract]] — Every verified claim cites resolvable evidence

### troubleshooting

1. [[wiki:troubleshooting-lost-citations]] — "citation content lost" flips pages to draft

### decision

1. [[wiki:decision-no-database]] — The frontmatter is the database


## How to use this wiki (agent protocol)

1. Call `wiki_check_freshness` (or `loredex check`) before trusting any page; treat stale pages as unverified and read the source.
2. Audit claims with `wiki_get_evidence` — every citation resolves to the exact file content at current git HEAD.
3. Keep the wiki fresh: `wiki_plan` lists what needs writing or repair; author with inline `[[cite:path:Lx-Ly]]` markers, one per claim; `wiki_refresh_page` auto-heals drifted citations.
4. Policy lives in `rule` / `troubleshooting` / `decision` pages: consult them before coding against the repo, capture findings with `loredex note` (uncited bodies land as drafts for `wiki_plan` to backfill), and fold duplicates together with `loredex organize`.
