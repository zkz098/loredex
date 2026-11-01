# AGENTS.md

Conventions, known failure modes and design decisions for this repository live in
the evidence-backed wiki under `.wiki/`, **not** in this file: reading path at
`.wiki/pages/quickstart.md`, build and test gates at
`.wiki/pages/testing-gates.md`. Nothing outside the managed block below is
maintained by tooling; add repo-specific notes as wiki pages instead.

<!-- BEGIN loredex -->
## Repository wiki (loredex)

This repo keeps an evidence-backed wiki under `.wiki/` (pages in `.wiki/pages/*.md`; every claim carries a `[[cite:path:Lx-Ly]]` source marker pinned to git content hashes).

Agent protocol:

1. Look before reading code: `loredex search "<query>"` / `loredex show <page>` (or MCP `wiki_search` / `wiki_read`).
2. Verify freshness before trusting a page: `loredex check` (or MCP `wiki_check_freshness`); STALE pages are unverified — read the source.
3. After changing code: `loredex auto` heals drifted citations, `loredex plan` lists remaining authoring work.
4. When writing pages, one inline `[[cite:path:Lx-Ly]]` per claim; bodies without citations are rejected.
5. Project policy lives in wiki pages, not in this file: type `rule` / `troubleshooting` / `decision` pages hold the conventions, known failures and design choices — `loredex search "<policy>"` reads them, `loredex note` captures a finding you cannot cite yet (stored as draft, backfilled by `loredex plan`), `loredex organize` reports which pages duplicate each other.

Do not hand-edit this block; regenerate with `loredex agents-md` (remove it with `loredex agents-md --remove`).
<!-- END loredex -->
