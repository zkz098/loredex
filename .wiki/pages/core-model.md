---
id: core-model
title: 'loredex-core: data model and parsing'
status: verified
generated_at: 2026-11-01T02:58:00Z
commit: 5fe6004cfe49890d5d14150d96e7200277613764
evidence:
- path: crates/loredex-core/src/page.rs
  lines:
  - 10
  - 27
  blob: 0a5365724b11730690a2a857196929b865e7eb43
  snippet_sha1: 969389b2ae71fbef2971bc79a759298546071e44
  note: drift-projected L9-L26@e814a40a -> L10-L27 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 71
  - 97
  blob: 0a5365724b11730690a2a857196929b865e7eb43
  snippet_sha1: 798aedcddab6ba41b856879b0f7ec746cacc7be7
  note: drift-projected L68-L94@e814a40a -> L71-L97 (similarity 0.96)
- path: crates/loredex-core/src/page.rs
  lines:
  - 115
  - 127
  blob: 0a5365724b11730690a2a857196929b865e7eb43
  snippet_sha1: c2851e6ee8582e5996db4fabb92712f011c35a8a
  note: drift-projected L112-L124@e814a40a -> L115-L127 (similarity 1.00)
- path: crates/loredex-core/src/cite.rs
  lines:
  - 27
  - 56
  blob: d34aacbf434a2801d21e2ef956f22ba442b29d2b
  snippet_sha1: f2ed3fe3000d1e17a032f489c5758b820cb64f71
  note: drift-projected L17-L48@7c13345c -> L27-L56 (similarity 0.90)
- path: crates/loredex-core/src/cite.rs
  lines:
  - 58
  - 66
  blob: d34aacbf434a2801d21e2ef956f22ba442b29d2b
  snippet_sha1: 9f49ae32804dc6f0860f79e80e8d796286e2ddf0
  note: drift-projected L50-L58@7c13345c -> L58-L66 (similarity 1.00)
- path: crates/loredex-core/src/search.rs
  lines:
  - 21
  - 72
  blob: f7ec83c7acd245116769c9889a0780e63c56638e
  snippet_sha1: 2ee041598080a40998d83778745f2e2b3a0ee9e5
  note: drift-projected L16-L67@43bd12e1 -> L21-L72 (similarity 1.00)
- path: crates/loredex-core/src/search.rs
  lines:
  - 127
  - 187
  blob: f7ec83c7acd245116769c9889a0780e63c56638e
  snippet_sha1: 0b5aff9b99c910f59969139ebf1c890f414001d6
  note: drift-projected L119-L177@43bd12e1 -> L127-L187 (similarity 0.92)
type: system
---
# loredex-core: data model and parsing

Page state machine `draft -> verified -> stale`, defined in [[cite:crates/loredex-core/src/page.rs:L9-L26]]. `WikiPage::from_markdown` splits YAML frontmatter from the body at the first `---` and the nearest newline-delimited closing fence [[cite:crates/loredex-core/src/page.rs:L68-L94]]; `save` writes atomically via temp-file + rename [[cite:crates/loredex-core/src/page.rs:L112-L124]].

Citation syntax is a double-bracket cite marker with an optional `:Lx-Ly` line range suffix; the parser rejects zero-based or inverted ranges [[cite:crates/loredex-core/src/cite.rs:L17-L48]] and deduplicates by (path, lines) [[cite:crates/loredex-core/src/cite.rs:L50-L58]]. A literal marker appearing in prose is parsed as a real citation — a pitfall found while dogfooding.

Search is a pure in-memory inverted index: tokenization splits camelCase/snake_case identifiers and emits overlapping bigrams for CJK [[cite:crates/loredex-core/src/search.rs:L16-L67]]; queries score with saturated term frequency and support latin prefix matching [[cite:crates/loredex-core/src/search.rs:L119-L177]]. The index is rebuilt from page files on every load, so no persistent cache can drift from the Markdown source of truth.
