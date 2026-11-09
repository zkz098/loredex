---
id: core-model
title: 'loredex-core: data model and parsing'
status: verified
generated_at: 2026-11-09T14:37:56Z
commit: 8ce30c67ff3080a3c5cde0325e337dee4b5b2056
evidence:
- path: crates/loredex-core/src/page.rs
  lines:
  - 8
  - 25
  blob: 0827a471fbe63039bf1155430c66e0c168085ccf
  snippet_sha1: 75a6d6537d642a933867315487a89e909474298f
  note: drift-projected L8-L25@04a91928 -> L8-L25 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 73
  - 99
  blob: 0827a471fbe63039bf1155430c66e0c168085ccf
  snippet_sha1: 798aedcddab6ba41b856879b0f7ec746cacc7be7
  note: drift-projected L71-L97@04a91928 -> L73-L99 (similarity 1.00)
- path: crates/loredex-core/src/page.rs
  lines:
  - 117
  - 137
  blob: 0827a471fbe63039bf1155430c66e0c168085ccf
  snippet_sha1: ef6249dddeca4ce47c183c6d5fdb8d56f37698f3
  note: drift-projected L115-L135@04a91928 -> L117-L137 (similarity 1.00)
- path: crates/loredex-core/src/cite.rs
  lines:
  - 30
  - 61
  blob: 670c12d1f22e8f4f552f4321b8c0ca250d539263
  snippet_sha1: fe36178d028fca7c4915c39e63aa0b3a09245058
  note: drift-projected L27-L56@d34aacbf -> L30-L61 (similarity 0.97)
- path: crates/loredex-core/src/cite.rs
  lines:
  - 63
  - 69
  blob: 670c12d1f22e8f4f552f4321b8c0ca250d539263
  snippet_sha1: 2b3f7a770bac7e83908d84968117aa382f574b63
  note: drift-projected L58-L64@d34aacbf -> L63-L69 (similarity 0.86)
- path: crates/loredex-core/src/search.rs
  lines:
  - 23
  - 68
  blob: cd125703ab5b2d5d5abba89b07270a2b279ca8bc
  snippet_sha1: 4adf67c89aa7c7c0e63b7cc91bf51a35ebbc2b84
  note: drift-projected L23-L68@d761dfe3 -> L23-L68 (similarity 1.00)
- path: crates/loredex-core/src/search.rs
  lines:
  - 182
  - 257
  blob: cd125703ab5b2d5d5abba89b07270a2b279ca8bc
  snippet_sha1: 1e76f197b19a886fa5031189b1568427117b6ca7
  note: drift-projected L137-L200@d761dfe3 -> L182-L257 (similarity 0.66)
type: system
---
Page state machine `draft -> verified -> stale`, defined in [[cite:crates/loredex-core/src/page.rs:L8-L25]]. `WikiPage::from_markdown` splits YAML frontmatter from the body at the first `---` and the nearest newline-delimited closing fence [[cite:crates/loredex-core/src/page.rs:L71-L97]]; `save` writes atomically via temp-file + rename [[cite:crates/loredex-core/src/page.rs:L115-L135]].

Citation syntax is a double-bracket cite marker with an optional `:Lx-Ly` line range suffix; the parser rejects zero-based or inverted ranges [[cite:crates/loredex-core/src/cite.rs:L27-L56]] and deduplicates by (path, lines) [[cite:crates/loredex-core/src/cite.rs:L58-L64]]. A literal marker appearing in prose is parsed as a real citation — a pitfall found while dogfooding.

Search is a pure in-memory inverted index: tokenization splits camelCase/snake_case identifiers and emits overlapping bigrams for CJK [[cite:crates/loredex-core/src/search.rs:L23-L68]]; queries score with saturated term frequency and support latin prefix matching with overflow-safe subtraction [[cite:crates/loredex-core/src/search.rs:L137-L200]]. The index is rebuilt from page files on every load, so no persistent cache can drift from the Markdown source of truth.
