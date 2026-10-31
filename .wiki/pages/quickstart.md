---
id: quickstart
title: Wiki quickstart
status: verified
generated_at: 2026-10-31T12:43:35Z
commit: 1d2863aaea11c84a442cdbf282fb7da8211225cd
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
1. [[wiki:repo-root]] — Repository root and engineering

### system

1. [[wiki:core-model]] — loredex-core: data model and parsing
1. [[wiki:engine-freshness]] — loredex-engine: staleness, verification, drift projection
1. [[wiki:entry-mcp-cli]] — Entry layer: MCP server and CLI
1. [[wiki:git-layer]] — loredex-git: read-only git wrapper


## How to use this wiki (agent protocol)

1. Call `wiki_check_freshness` (or `loredex check`) before trusting any page; treat stale pages as unverified and read the source.
2. Audit claims with `wiki_get_evidence` — every citation resolves to the exact file content at current git HEAD.
3. Keep the wiki fresh: `wiki_plan` lists what needs writing or repair; author with inline `[[cite:path:Lx-Ly]]` markers, one per claim; `wiki_refresh_page` auto-heals drifted citations.
