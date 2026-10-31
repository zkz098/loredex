---
id: quickstart
title: Wiki quickstart
status: verified
generated_at: 2026-10-31T12:29:04Z
commit: 1d51e1c5f143342205c6040d13ad0bb8ae955f1d
evidence: []
tags:
- generated
type: index
---
# repowiki wiki — quickstart

Generated navigation, not evidence-backed. Full catalog: [[wiki:index]].

## Recommended reading path

### architecture

1. [[wiki:architecture]] — 总体架构
1. [[wiki:repo-root]] — 仓库根与工程化

### system

1. [[wiki:core-model]] — wiki-core：数据模型与解析
1. [[wiki:engine-freshness]] — wiki-engine：过期检测、内容校验与漂移投影
1. [[wiki:entry-mcp-cli]] — 入口层：MCP server 与 CLI
1. [[wiki:git-layer]] — wiki-git：git 只读封装


## How to use this wiki (agent protocol)

1. Call `wiki_check_freshness` (or `loredex check`) before trusting any page; treat stale pages as unverified and read the source.
2. Audit claims with `wiki_get_evidence` — every citation resolves to the exact file content at current git HEAD.
3. Keep the wiki fresh: `wiki_plan` lists what needs writing or repair; author with inline `[[cite:path:Lx-Ly]]` markers, one per claim; `wiki_refresh_page` auto-heals drifted citations.
