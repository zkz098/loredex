---
id: core-model
title: 'loredex-core: data model and parsing'
status: verified
generated_at: 2026-11-03T13:23:09Z
commit: 67da1675a709e5972e22a3f53797fe275070907e
evidence:
- path: crates/loredex-core/src/page.rs
  lines:
  - 8
  - 25
  blob: 04a91928fedae8fe609044e59cbd6ec4a564cdc9
  snippet_sha1: 75a6d6537d642a933867315487a89e909474298f
- path: crates/loredex-core/src/page.rs
  lines:
  - 71
  - 97
  blob: 04a91928fedae8fe609044e59cbd6ec4a564cdc9
  snippet_sha1: 798aedcddab6ba41b856879b0f7ec746cacc7be7
- path: crates/loredex-core/src/page.rs
  lines:
  - 115
  - 135
  blob: 04a91928fedae8fe609044e59cbd6ec4a564cdc9
  snippet_sha1: ef6249dddeca4ce47c183c6d5fdb8d56f37698f3
- path: crates/loredex-core/src/cite.rs
  lines:
  - 27
  - 56
  blob: d34aacbf434a2801d21e2ef956f22ba442b29d2b
  snippet_sha1: f2ed3fe3000d1e17a032f489c5758b820cb64f71
- path: crates/loredex-core/src/cite.rs
  lines:
  - 58
  - 64
  blob: d34aacbf434a2801d21e2ef956f22ba442b29d2b
  snippet_sha1: f1a8cf598d322be8c0fe85f4a03dc1dcfee6ea38
- path: crates/loredex-core/src/search.rs
  lines:
  - 23
  - 68
  blob: d761dfe3556d4ca8fb2363ec5b06003e5e1fe9fa
  snippet_sha1: 4adf67c89aa7c7c0e63b7cc91bf51a35ebbc2b84
- path: crates/loredex-core/src/search.rs
  lines:
  - 137
  - 200
  blob: d761dfe3556d4ca8fb2363ec5b06003e5e1fe9fa
  snippet_sha1: 0182ccb2f19165f67f2d0b77109183c2523dac87
type: system
---
Page state machine `draft -> verified -> stale`, defined in [[cite:crates/loredex-core/src/page.rs:L8-L25]]. `WikiPage::from_markdown` splits YAML frontmatter from the body at the first `---` and the nearest newline-delimited closing fence [[cite:crates/loredex-core/src/page.rs:L71-L97]]; `save` writes atomically via temp-file + rename [[cite:crates/loredex-core/src/page.rs:L115-L135]].

Citation syntax is a double-bracket cite marker with an optional `:Lx-Ly` line range suffix; the parser rejects zero-based or inverted ranges [[cite:crates/loredex-core/src/cite.rs:L27-L56]] and deduplicates by (path, lines) [[cite:crates/loredex-core/src/cite.rs:L58-L64]]. A literal marker appearing in prose is parsed as a real citation — a pitfall found while dogfooding.

Search is a pure in-memory inverted index: tokenization splits camelCase/snake_case identifiers and emits overlapping bigrams for CJK [[cite:crates/loredex-core/src/search.rs:L23-L68]]; queries score with saturated term frequency and support latin prefix matching with overflow-safe subtraction [[cite:crates/loredex-core/src/search.rs:L137-L200]]. The index is rebuilt from page files on every load, so no persistent cache can drift from the Markdown source of truth.
