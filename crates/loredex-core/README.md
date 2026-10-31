# loredex-core

Data model for [Loredex](https://github.com/zkz098/loredex), an evidence-backed repository wiki for AI coding agents.

This crate owns the page format and everything that can be expressed without touching git:

- **`PageMeta` / `Status` / `Evidence` / `WikiPage`** — the wiki page model. A page is one Markdown file whose YAML frontmatter *is* the database: `status` (`draft` | `verified` | `stale`), the generation commit, and the evidence list. `WikiPage::from_markdown` / `to_markdown` parse and serialize; `load` / `save` are atomic (temp file + rename).
- **Citation syntax** — `parse_citations` extracts `[[cite:path:Lx-Ly]]` markers from a body into `Citation` values, `dedup_citations` normalizes them, and `parse_wiki_links` reads page-to-page `[[wiki:id]]` links.
- **Search** — `search::tokenize` splits camelCase, snake_case, and CJK text (bigrams) into tokens; `search::InvertedIndex` builds over all pages and answers `search` with ranked `SearchHit`s. Pure in-memory, rebuilt from files in milliseconds.
- **Page-id rules** — `validate_page_id` and `list_page_files` encode the `.wiki/pages/*.md` layout (ids may be hierarchical, e.g. `architecture/overview`).
- **`WikiError`** — thiserror enum shared by the whole workspace (frontmatter, YAML, IO, git, page-not-found).

```rust
use loredex_core::WikiPage;

let page = WikiPage::load(std::path::Path::new(".wiki/pages/core.md"))?;
let citations = loredex_core::parse_citations(&page.body)?;
assert!(!citations.is_empty(), "uncited bodies are rejected at write time by the engine");
```

Depends only on serde/serde_yaml/regex/thiserror — no git, no async. Higher layers: [`loredex-git`](https://crates.io/crates/loredex-git) (evidence resolution against HEAD), [`loredex-engine`](https://crates.io/crates/loredex-engine) (business logic), and the [`loredex` CLI / `loredex-mcp` server](https://github.com/zkz098/loredex) built on top.

License: GPL-3.0-or-later. Wiki data written by Loredex is unaffected — it is just Markdown in your repository.
