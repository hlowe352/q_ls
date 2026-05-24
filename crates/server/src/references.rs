//! `textDocument/references` — find all references to the symbol under
//! the cursor.
//!
//! "Same symbol" here means: same name, same scope. Multiple rebindings of
//! a name within one scope (`a:1; a:2; a`) are treated as one symbol with
//! several def sites — they all get returned, and rename rewrites all of
//! them. A name with the same spelling but a different scope (an outer
//! global vs. a lambda parameter) is a *different* symbol and is excluded.

use std::collections::HashSet;

use tower_lsp_server::ls_types::*;
use q_parser::SyntaxKind;

use crate::document::Document;

pub fn find_references(
    doc: &Document,
    pos: Position,
    include_declaration: bool,
    uri: &Uri,
) -> Vec<Location> {
    let cursor = doc.offset_of(pos);
    let table = doc.sym_table();
    let Some((name, _, _)) = doc.ident_at(cursor) else { return Vec::new() };
    // Copy the borrowed name so we can read the syntax tree without
    // holding a reference into `doc.text`.
    let name = name.to_string();

    // All def sites of `name` in the scope the cursor lives in. If `name`
    // isn't bound anywhere visible, bail.
    let def_offsets: HashSet<usize> =
        table.def_offsets_for(cursor, &name).into_iter().collect();
    if def_offsets.is_empty() {
        return Vec::new();
    }

    // Qualified form of `name` (e.g. `.cache.cache` when name is `cache`
    // inside `\d .cache`).  Used to match backtick symbol tokens.
    let qualified_name = table.qualified_for(cursor, &name)
        .map(|q| q.to_string())
        .unwrap_or_else(|| name.clone());

    let root = doc.parse().syntax();
    let mut out = Vec::new();
    for token in root
        .descendants_with_tokens()
        .filter_map(|el| el.into_token())
    {
        let tk = token.kind();
        let off: usize = token.text_range().start().into();

        if matches!(tk, SyntaxKind::Ident | SyntaxKind::DottedIdent) {
            if token.text() != name {
                continue;
            }

            let parent_kind = token.parent().map(|p| p.kind());
            let in_param_list = is_in_kind(&token, SyntaxKind::ParamList);

            // Inclusion rule:
            //   - the token IS one of the def sites (param token, list-pattern
            //     element, assignment LHS), or
            //   - the token is a reference that resolves to one of those defs.
            let is_decl = def_offsets.contains(&off);
            let resolves_to_def = is_decl
                || (!in_param_list
                    && matches!(parent_kind, Some(SyntaxKind::IdentExpr | SyntaxKind::Namespace))
                    && table
                        .resolve(off, &name)
                        .is_some_and(|d| def_offsets.contains(&d)));

            if !resolves_to_def {
                continue;
            }
            if is_decl && !include_declaration {
                continue;
            }

            let start = doc.position_of(off);
            let end = doc.position_of(off + name.len());
            out.push(Location { uri: uri.clone(), range: Range::new(start, end) });

        } else if tk == SyntaxKind::Symbol {
            // `` `.cache.cache `` — backtick-prefixed symbol used as table ref
            // (e.g. in upsert / insert / in-place qSQL).
            // Match against bare name OR its fully qualified form.
            let sym_name = token.text().strip_prefix('`').unwrap_or("");
            if sym_name != name && sym_name != qualified_name {
                continue;
            }
            // Symbol usage is always a reference, never a declaration.
            let start = doc.position_of(off);
            let end = doc.position_of(off + token.text().len());
            out.push(Location { uri: uri.clone(), range: Range::new(start, end) });
        }
    }

    out
}

fn is_in_kind(token: &q_parser::SyntaxToken, kind: SyntaxKind) -> bool {
    let mut cur = token.parent();
    while let Some(node) = cur {
        if node.kind() == kind {
            return true;
        }
        cur = node.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(src: &str, cursor: usize, include_decl: bool) -> Vec<usize> {
        let doc = Document::new(src.to_string(), 0);
        let uri: Uri = "file:///x.q".parse().unwrap();
        let pos = doc.position_of(cursor);
        find_references(&doc, pos, include_decl, &uri)
            .into_iter()
            .map(|loc| {
                doc.offset_of(loc.range.start)
            })
            .collect()
    }

    #[test]
    fn finds_global_uses() {
        let src = "x:1; y:x+x; z:x";
        let cursor = src.find("x:1").unwrap();
        let r = refs(src, cursor, true);
        assert_eq!(
            r,
            vec![
                src.find("x:1").unwrap(),
                src.find("x+x").unwrap(),
                src.find("x+x").unwrap() + 2,
                src.rfind('x').unwrap(),
            ]
        );
    }

    #[test]
    fn excludes_declaration_when_asked() {
        let src = "x:1; y:x+x";
        let cursor = src.find("x:1").unwrap();
        let r = refs(src, cursor, false);
        // Only the two uses, not the def.
        assert_eq!(r.len(), 2);
        assert!(r.iter().all(|&o| o != src.find("x:1").unwrap()));
    }

    #[test]
    fn lambda_param_scope() {
        let src = "x:99; f:{[x] x+1}; y:x";
        // Cursor on the parameter `[x]`.
        let cursor = src.find("[x]").unwrap() + 1;
        let r = refs(src, cursor, true);
        // Param def + the `x+1` reference. The outer `x:99` and `y:x`
        // refer to a *different* x.
        assert_eq!(r.len(), 2);
        assert!(r.contains(&(src.find("[x]").unwrap() + 1)));
        assert!(r.contains(&src.find("x+1").unwrap()));
    }

    #[test]
    fn rebindings_in_same_scope_are_one_symbol() {
        // Two local rebindings + one use — find_refs must return all 3.
        let src = "f:{a:1; a:2; a}";
        let cursor = src.rfind('a').unwrap();
        let r = refs(src, cursor, true);
        assert_eq!(r.len(), 3, "want all 3 a sites, got {r:?}");
        assert!(r.contains(&src.find("a:1").unwrap()));
        assert!(r.contains(&src.find("a:2").unwrap()));
    }

    #[test]
    fn global_rebindings_are_one_symbol() {
        let src = "a:1; a:2; a";
        let cursor = src.find("a:2").unwrap();
        let r = refs(src, cursor, true);
        assert_eq!(r.len(), 3, "want 3 a sites, got {r:?}");
    }

    #[test]
    fn symbol_token_included_in_refs_from_bare_name() {
        // Cursor on `cache` (def inside \d .cache); `` `.cache.cache `` upsert
        // site must appear in references.
        let src = "\\d .cache\ncache:1\n\\d .\n`.cache.cache upsert 2";
        let cursor = src.find("cache:1").unwrap();
        let r = refs(src, cursor, true);
        let sym_off = src.find("`.cache.cache").unwrap();
        assert!(r.contains(&sym_off), "symbol ref missing; got offsets {r:?}");
    }

    #[test]
    fn symbol_token_included_in_refs_from_dotted_name() {
        // Cursor on `.cache.cache` (dotted ident ref); same symbol must appear.
        let src = "\\d .cache\ncache:1\n\\d .\nuse:.cache.cache\n`.cache.cache upsert 2";
        let cursor = src.find("use:.cache.cache").unwrap() + "use:".len();
        let r = refs(src, cursor, true);
        let sym_off = src.find("`.cache.cache").unwrap();
        assert!(r.contains(&sym_off), "symbol ref missing from dotted cursor; got {r:?}");
    }

    #[test]
    fn cursor_off_ident_returns_empty() {
        let src = "a:1; b:2;";
        // Cursor on the leading semicolon — no ident here.
        let cursor = src.find(';').unwrap();
        let r = refs(src, cursor, true);
        assert!(r.is_empty(), "got {r:?}");
    }
}

#[cfg(test)]
mod aoc_tests {
    use super::*;
    use crate::document::Document;

    // Regression: `minus` in q1b is assigned inside $[...] and used later in
    // the same lambda. Both the def and ref sites must be found from either cursor.
    #[test]
    fn finds_minus_refs_in_q1b() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/parser/tests/data/real_q/aoc.q"
        )).expect("aoc.q");

        let doc = Document::new(src.clone(), 0);
        let uri: Uri = "file:///aoc.q".parse().unwrap();

        let def_cursor = src.find("minus:").expect("minus: def");
        let ref_cursor = src.find("not minus").expect("(not minus)") + "not ".len();

        let refs_from_def = find_references(&doc, doc.position_of(def_cursor), true, &uri);
        let refs_from_ref = find_references(&doc, doc.position_of(ref_cursor), true, &uri);

        assert!(refs_from_def.len() >= 2, "from def: expected ≥2, got {}", refs_from_def.len());
        assert!(refs_from_ref.len() >= 2, "from ref: expected ≥2, got {}", refs_from_ref.len());
    }
}

#[cfg(test)]
mod vscode_simulation {
    use super::*;
    use crate::document::Document;

    #[test]
    fn find_refs_exclude_decl_from_def_site() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/parser/tests/data/real_q/aoc.q"
        )).expect("aoc.q");
        let doc = Document::new(src.clone(), 0);
        let uri: Uri = "file:///aoc.q".parse().unwrap();
        let def_cursor = src.find("minus:").unwrap();
        let refs = find_references(&doc, doc.position_of(def_cursor), false, &uri);
        eprintln!("include_decl=false from def: {} refs", refs.len());
        for r in &refs { eprintln!("  {:?}", r.range); }
        assert!(!refs.is_empty(), "expected ref site to be found, got 0");
    }

    #[test]
    fn find_refs_loc_from_def_site() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/parser/tests/data/real_q/aoc.q"
        )).expect("aoc.q");
        let doc = Document::new(src.clone(), 0);
        let uri: Uri = "file:///aoc.q".parse().unwrap();
        // cursor on `loc` in `loc: x[0]+...`
        let def_cursor = src.find("loc:").unwrap();
        let refs = find_references(&doc, doc.position_of(def_cursor), false, &uri);
        eprintln!("loc include_decl=false from def: {} refs", refs.len());
        for r in &refs { eprintln!("  {:?}", r.range); }
        assert!(!refs.is_empty(), "expected ref sites to be found, got 0");
    }
}
