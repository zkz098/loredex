---
id: core-model
title: 'loredex-core: data model and parsing'
status: verified
generated_at: 2026-11-02T11:24:23Z
commit: fe5974431c3ea5d7d6b228f66457cfdc2182befd
evidence:
- path: crates/loredex-core/src/page.rs
  lines:
  - 10
  - 27
  blob: 04a91928fedae8fe609044e59cbd6ec4a564cdc9
  snippet_sha1: 969389b2ae71fbef2971bc79a759298546071e44
  note: drift-projected L10-L27@0a536572 -> L10-L27 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 71
  - 97
  blob: 04a91928fedae8fe609044e59cbd6ec4a564cdc9
  snippet_sha1: 798aedcddab6ba41b856879b0f7ec746cacc7be7
  note: drift-projected L71-L97@0a536572 -> L71-L97 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 115
  - 135
  blob: 04a91928fedae8fe609044e59cbd6ec4a564cdc9
  snippet_sha1: ef6249dddeca4ce47c183c6d5fdb8d56f37698f3
  note: drift-projected L115-L127@0a536572 -> L115-L135 (similarity 0.77)
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
