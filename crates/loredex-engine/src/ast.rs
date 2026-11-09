//! Experimental `tree-sitter` AST integration for Rust source files (`.rs`).
//!
//! Keeps Loredex's storage and verification contract language-agnostic —
//! frontmatter still pins `(path, lines, blob, snippet_sha1)` — while adding
//! three AST-powered capabilities for Rust code:
//! 1. **Symbol citation resolution**: `[[cite:crates/foo/src/lib.rs#bar]]` or
//!    `[[cite:src/page.rs#WikiPage::from_markdown]]` resolves at write time to
//!    the AST item's exact 1-based `[lo, hi]` line range at HEAD.
//! 2. **AST-aware evidence windows**: `loredex evidence` snaps keyword hits in
//!    `.rs` files to the enclosing function/struct/enum/trait item and emits a
//!    symbol-annotated `[[cite:path#symbol:Llo-Lhi]]` marker.
//! 3. **AST-assisted drift relocation**: when a cited Rust function or type is
//!    moved to a different position in the file (breaking monotonic line-diff
//!    projection), `refresh` / `auto` locates the symbol in the new AST and
//!    adopts its new line range if content similarity still meets the
//!    [`crate::drift`] threshold.

use serde::Serialize;
use tree_sitter::{Node, Parser};

/// A citable Rust AST item extracted by `tree-sitter-rust`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AstSymbol {
    /// Unqualified identifier, e.g. `from_markdown` or `project_range`.
    pub name: String,
    /// Scope-qualified identifier when nested inside an `impl` or `mod`, e.g.
    /// `WikiPage::from_markdown`; equals `name` for top-level items.
    pub qualified_name: String,
    /// Item kind (`fn`, `struct`, `enum`, `trait`, `impl`, `const`, `static`, `type`, `macro`, `mod`).
    pub kind: &'static str,
    /// 1-based inclusive `[start_line, end_line]`.
    pub lines: [u32; 2],
}

impl AstSymbol {
    /// Best citation anchor name: uses `qualified_name` when inside an `impl`
    /// block or nested module so methods like `WikiPage::from_markdown` are
    /// unambiguous.
    pub fn cite_name(&self) -> &str {
        &self.qualified_name
    }
}

/// True if `path` is a Rust source file supported by the experimental AST layer.
pub fn is_rust_path(path: &str) -> bool {
    path.ends_with(".rs")
}

/// Parse a Rust source file with `tree-sitter-rust` and return all citable
/// item spans in source order. Returns an empty Vec for non-`.rs` paths.
pub fn parse_rust_symbols(path: &str, source: &str) -> Vec<AstSymbol> {
    if !is_rust_path(path) {
        return Vec::new();
    }
    let mut parser = Parser::new();
    let lang = tree_sitter_rust::LANGUAGE.into();
    if parser.set_language(&lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_items(tree.root_node(), source.as_bytes(), None, &mut out);
    out
}

/// Resolve a symbol query (`"project_range"` or `"WikiPage::from_markdown"`)
/// in a `.rs` file to its [`AstSymbol`].
///
/// Matching priority:
/// 1. Exact match on `qualified_name` (e.g. `WikiPage::from_markdown` or top-level `project_range`).
/// 2. Exact match on unqualified `name` (e.g. `from_markdown`).
/// 3. Suffix match on `::symbol` for deeply nested module items.
pub fn resolve_symbol(path: &str, source: &str, symbol: &str) -> Option<AstSymbol> {
    let items = parse_rust_symbols(path, source);
    if items.is_empty() {
        return None;
    }
    if let Some(exact_q) = items.iter().find(|s| s.qualified_name == symbol) {
        return Some(exact_q.clone());
    }
    if let Some(exact_n) = items.iter().find(|s| s.name == symbol) {
        return Some(exact_n.clone());
    }
    let suffix = format!("::{symbol}");
    items
        .into_iter()
        .find(|s| s.qualified_name.ends_with(&suffix))
}

/// Find the tightest non-module, non-impl-container Rust item enclosing `line`
/// (1-based), falling back to an enclosing `impl` or `mod` only if no inner
/// item covers `line`.
pub fn enclosing_symbol(path: &str, source: &str, line: u32) -> Option<AstSymbol> {
    let items = parse_rust_symbols(path, source);
    enclosing_from_list(&items, line)
}

pub(crate) fn enclosing_from_list(items: &[AstSymbol], line: u32) -> Option<AstSymbol> {
    items
        .iter()
        .filter(|s| s.lines[0] <= line && line <= s.lines[1] && !matches!(s.kind, "mod" | "impl"))
        .min_by_key(|s| s.lines[1].saturating_sub(s.lines[0]))
        .cloned()
        .or_else(|| {
            items
                .iter()
                .filter(|s| s.lines[0] <= line && line <= s.lines[1])
                .min_by_key(|s| s.lines[1].saturating_sub(s.lines[0]))
                .cloned()
        })
}

/// Find a Rust symbol whose `[lo, hi]` span substantially matches `[lo, hi]`
/// (either exact, or overlapping by >= 70% of the cited range). Used during
/// drift recovery to infer which AST item an older line-only citation pointed at.
pub fn symbol_matching_range(path: &str, source: &str, lo: u32, hi: u32) -> Option<AstSymbol> {
    let items = parse_rust_symbols(path, source);
    if items.is_empty() || hi < lo {
        return None;
    }
    if let Some(exact) = items
        .iter()
        .find(|s| s.lines == [lo, hi] && !matches!(s.kind, "mod" | "impl"))
    {
        return Some(exact.clone());
    }
    let target_len = (hi - lo + 1) as f64;
    let mut best: Option<(f64, &AstSymbol)> = None;
    for s in items.iter().filter(|s| !matches!(s.kind, "mod" | "impl")) {
        let ov_lo = s.lines[0].max(lo);
        let ov_hi = s.lines[1].min(hi);
        if ov_hi < ov_lo {
            continue;
        }
        let overlap = (ov_hi - ov_lo + 1) as f64;
        let sym_len = (s.lines[1] - s.lines[0] + 1).max(1) as f64;
        let iou = overlap / target_len.max(sym_len);
        if iou >= 0.65 && best.map(|(b, _)| iou > b).unwrap_or(true) {
            best = Some((iou, s));
        }
    }
    best.map(|(_, s)| s.clone())
}

fn collect_items(node: Node<'_>, src: &[u8], prefix: Option<&str>, out: &mut Vec<AstSymbol>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_item" | "function_signature_item" => {
                if let Some(sym) = make_named_symbol(child, src, prefix, "fn") {
                    out.push(sym);
                }
            }
            "struct_item" => {
                if let Some(sym) = make_named_symbol(child, src, prefix, "struct") {
                    out.push(sym);
                }
            }
            "enum_item" => {
                if let Some(sym) = make_named_symbol(child, src, prefix, "enum") {
                    out.push(sym);
                }
            }
            "trait_item" => {
                if let Some(sym) = make_named_symbol(child, src, prefix, "trait") {
                    let next_prefix = sym.qualified_name.clone();
                    out.push(sym);
                    if let Some(body) = child.child_by_field_name("body") {
                        collect_items(body, src, Some(&next_prefix), out);
                    }
                }
            }
            "type_item" => {
                if let Some(sym) = make_named_symbol(child, src, prefix, "type") {
                    out.push(sym);
                }
            }
            "const_item" => {
                if let Some(sym) = make_named_symbol(child, src, prefix, "const") {
                    out.push(sym);
                }
            }
            "static_item" => {
                if let Some(sym) = make_named_symbol(child, src, prefix, "static") {
                    out.push(sym);
                }
            }
            "macro_definition" => {
                if let Some(sym) = make_named_symbol(child, src, prefix, "macro") {
                    out.push(sym);
                }
            }
            "mod_item" => {
                if let Some(sym) = make_named_symbol(child, src, prefix, "mod") {
                    let next_prefix = sym.qualified_name.clone();
                    out.push(sym);
                    if let Some(body) = child.child_by_field_name("body") {
                        collect_items(body, src, Some(&next_prefix), out);
                    }
                }
            }
            "impl_item" => {
                if let Some(type_node) = child.child_by_field_name("type") {
                    let raw_ty = node_text(type_node, src);
                    let clean_ty = strip_generics(raw_ty);
                    if !clean_ty.is_empty() {
                        let impl_qual = match prefix {
                            Some(p) => format!("{p}::{clean_ty}"),
                            None => clean_ty.to_string(),
                        };
                        out.push(AstSymbol {
                            name: clean_ty.to_string(),
                            qualified_name: format!("impl {impl_qual}"),
                            kind: "impl",
                            lines: node_lines_with_attrs(child, src),
                        });
                        if let Some(body) = child.child_by_field_name("body") {
                            collect_items(body, src, Some(&impl_qual), out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn make_named_symbol(
    node: Node<'_>,
    src: &[u8],
    prefix: Option<&str>,
    kind: &'static str,
) -> Option<AstSymbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = node_text(name_node, src).trim();
    if name.is_empty() {
        return None;
    }
    let qualified_name = match prefix {
        Some(p) => format!("{p}::{name}"),
        None => name.to_string(),
    };
    Some(AstSymbol {
        name: name.to_string(),
        qualified_name,
        kind,
        lines: node_lines_with_attrs(node, src),
    })
}

/// Include immediately preceding `#[...]` attributes (not doc comments, so line
/// ranges stay tight around the declaration and its attributes) when computing
/// the 1-based `[lo, hi]` span of a Rust AST item.
fn node_lines_with_attrs(node: Node<'_>, _src: &[u8]) -> [u32; 2] {
    let mut start_row = node.start_position().row as u32;
    let end_row = node.end_position().row as u32;
    let mut cur = node.prev_sibling();
    while let Some(prev) = cur {
        if prev.kind() == "attribute_item" {
            let prev_end = prev.end_position().row as u32;
            if prev_end + 1 >= start_row {
                start_row = prev.start_position().row as u32;
                cur = prev.prev_sibling();
                continue;
            }
        }
        break;
    }
    [start_row + 1, end_row + 1]
}

fn node_text<'a>(node: Node<'_>, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

fn strip_generics(ty: &str) -> &str {
    let t = ty.trim().trim_start_matches('&').trim();
    match t.find('<') {
        Some(idx) => t[..idx].trim(),
        None => t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RS: &str = "\
use std::path::Path;

pub struct Config {
    pub language: String,
}

impl Config {
    pub fn load(path: &Path) -> Self {
        let _ = path;
        Self { language: String::new() }
    }
}

#[inline]
pub fn top_helper(x: u32) -> u32 {
    x + 1
}

mod inner {
    pub fn nested_fn() {}
}
";

    #[test]
    fn extracts_structs_impl_methods_attrs_and_mods() {
        let syms = parse_rust_symbols("src/config.rs", SAMPLE_RS);
        let names: Vec<(&str, &str, [u32; 2])> = syms
            .iter()
            .map(|s| (s.kind, s.qualified_name.as_str(), s.lines))
            .collect();
        assert!(names.contains(&("struct", "Config", [3, 5])), "{names:?}");
        assert!(
            names.contains(&("fn", "Config::load", [8, 11])),
            "{names:?}"
        );
        // #[inline] on line 14 is included in top_helper's span [14, 17]
        assert!(names.contains(&("fn", "top_helper", [14, 17])), "{names:?}");
        assert!(
            names.contains(&("fn", "inner::nested_fn", [20, 20])),
            "{names:?}"
        );
    }

    #[test]
    fn resolves_qualified_and_unqualified_symbols() {
        let load = resolve_symbol("src/config.rs", SAMPLE_RS, "Config::load").unwrap();
        assert_eq!(load.lines, [8, 11]);
        let load_short = resolve_symbol("src/config.rs", SAMPLE_RS, "load").unwrap();
        assert_eq!(load_short.qualified_name, "Config::load");
        let nested = resolve_symbol("src/config.rs", SAMPLE_RS, "nested_fn").unwrap();
        assert_eq!(nested.qualified_name, "inner::nested_fn");
        assert!(resolve_symbol("README.md", SAMPLE_RS, "Config").is_none());
    }

    #[test]
    fn finds_enclosing_symbol_for_line() {
        let sym = enclosing_symbol("src/config.rs", SAMPLE_RS, 9).unwrap();
        assert_eq!(sym.qualified_name, "Config::load");
        assert_eq!(sym.lines, [8, 11]);
    }
}
