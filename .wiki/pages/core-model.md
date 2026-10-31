---
id: core-model
title: 'loredex-core: data model and parsing'
status: verified
generated_at: 2026-10-31T12:43:23Z
commit: 1d2863aaea11c84a442cdbf282fb7da8211225cd
evidence:
- path: crates/loredex-core/src/page.rs
  lines:
  - 9
  - 26
  blob: e814a40a848673def3eb54918a306230323ab57e
  snippet_sha1: 969389b2ae71fbef2971bc79a759298546071e44
- path: crates/loredex-core/src/page.rs
  lines:
  - 68
  - 94
  blob: e814a40a848673def3eb54918a306230323ab57e
  snippet_sha1: 293aece1073df13d3589bcd028ff5c64d9f62413
- path: crates/loredex-core/src/page.rs
  lines:
  - 112
  - 124
  blob: e814a40a848673def3eb54918a306230323ab57e
  snippet_sha1: c2851e6ee8582e5996db4fabb92712f011c35a8a
- path: crates/loredex-core/src/cite.rs
  lines:
  - 17
  - 48
  blob: 7c13345cd8f6b37e43dfc9feab20fdeb141d73f0
  snippet_sha1: 7feaf9a2dc20ac97b290b999ad7ad50df5190ec4
- path: crates/loredex-core/src/cite.rs
  lines:
  - 50
  - 58
  blob: 7c13345cd8f6b37e43dfc9feab20fdeb141d73f0
  snippet_sha1: 9f49ae32804dc6f0860f79e80e8d796286e2ddf0
- path: crates/loredex-core/src/search.rs
  lines:
  - 16
  - 67
  blob: 43bd12e11aaec651533de243ed002ed88ef5bf18
  snippet_sha1: 2ee041598080a40998d83778745f2e2b3a0ee9e5
- path: crates/loredex-core/src/search.rs
  lines:
  - 119
  - 177
  blob: 43bd12e11aaec651533de243ed002ed88ef5bf18
  snippet_sha1: 77d85e50ee5c84ac7814c8f6949b621bee30b3bc
type: system
---
# loredex-core: data model and parsing

Page state machine `draft -> verified -> stale`, defined in [[cite:crates/loredex-core/src/page.rs:L9-L26]]. `WikiPage::from_markdown` splits YAML frontmatter from the body at the first `---` and the nearest newline-delimited closing fence [[cite:crates/loredex-core/src/page.rs:L68-L94]]; `save` writes atomically via temp-file + rename [[cite:crates/loredex-core/src/page.rs:L112-L124]].

Citation syntax is a double-bracket cite marker with an optional `:Lx-Ly` line range suffix; the parser rejects zero-based or inverted ranges [[cite:crates/loredex-core/src/cite.rs:L17-L48]] and deduplicates by (path, lines) [[cite:crates/loredex-core/src/cite.rs:L50-L58]]. A literal marker appearing in prose is parsed as a real citation — a pitfall found while dogfooding.

Search is a pure in-memory inverted index: tokenization splits camelCase/snake_case identifiers and emits overlapping bigrams for CJK [[cite:crates/loredex-core/src/search.rs:L16-L67]]; queries score with saturated term frequency and support latin prefix matching [[cite:crates/loredex-core/src/search.rs:L119-L177]]. The index is rebuilt from page files on every load, so no persistent cache can drift from the Markdown source of truth.
