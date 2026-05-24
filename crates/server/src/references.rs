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
            let tok_text = token.text();
            // Match bare name OR the qualified form (e.g. `.cache.cache` when
            // searching for `cache` inside `\d .cache`).
            let lookup_name: &str = if tok_text == name.as_str() {
                &name
            } else if tok_text == qualified_name.as_str() {
                &qualified_name
            } else {
                continue;
            };
            // Column names inside qSQL are not global refs, but the table
            // argument of a `from` clause IS (it names the global table).
            if is_in_qsql(&token) && !is_qsql_from_table_ident(&token) {
                continue;
            }

            let parent_kind = token.parent().map(|p| p.kind());
            let in_param_list = is_in_kind(&token, SyntaxKind::ParamList);

            let is_decl = def_offsets.contains(&off);
            // Table constructor column defs (`id` in `([id:...])`) are not
            // variable references even though resolve() would find the global.
            if is_col_def_in_table(&token) {
                continue;
            }
            let resolves_to_def = is_decl
                || (!in_param_list
                    && matches!(parent_kind, Some(SyntaxKind::IdentExpr | SyntaxKind::Namespace))
                    && table
                        .resolve(off, lookup_name)
                        .is_some_and(|d| def_offsets.contains(&d)));

            if !resolves_to_def {
                continue;
            }
            if is_decl && !include_declaration {
                continue;
            }

            let start = doc.position_of(off);
            let end = doc.position_of(off + tok_text.len());
            out.push(Location { uri: uri.clone(), range: Range::new(start, end) });

        } else if tk == SyntaxKind::Symbol {
            // Only treat backtick symbols as global table refs in in-place
            // modification contexts: `upsert`, `insert`, or qSQL
            // delete/update with a symbol table.  Excludes dict indexing
            // (r`id), symbol literals in assignments, etc.
            if !is_inplace_table_symbol(&token) {
                continue;
            }
            let sym_name = token.text().strip_prefix('`').unwrap_or("");
            if sym_name != name && sym_name != qualified_name {
                continue;
            }
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

fn is_in_qsql(token: &q_parser::SyntaxToken) -> bool {
    let mut cur = token.parent();
    while let Some(node) = cur {
        match node.kind() {
            // Statement-level qSQL nodes.
            SyntaxKind::SelectExpr
            | SyntaxKind::UpdateExpr
            | SyntaxKind::ExecExpr
            | SyntaxKind::DeleteExpr => return true,

            // Expression-level qSQL: inside $[…] or lambdas the parser emits
            // plain ApplyExpr chains.  Detect `update`/`select`/`exec`/`delete`
            // as the function position of an apply — these identifiers are only
            // valid as qSQL verbs, never as ordinary function names.
            SyntaxKind::ApplyExpr => {
                if let Some(func) = node.first_child()
                    && func.kind() == SyntaxKind::IdentExpr {
                        let text = func
                            .children_with_tokens()
                            .filter_map(|el| el.into_token())
                            .find(|t| t.kind() == SyntaxKind::Ident)
                            .map(|t| t.text().to_string())
                            .unwrap_or_default();
                        if matches!(
                            text.as_str(),
                            "update" | "select" | "exec" | "delete"
                        ) {
                            return true;
                        }
                    }
            }
            _ => {}
        }
        cur = node.parent();
    }
    false
}

/// True only when a Symbol token is used as a global table reference:
///
/// - LHS of a `upsert` / `insert` binary expression:
///   `` `.t upsert row `` → BinExpr { LiteralExpr(Symbol), "upsert", … }
/// - Table argument of a `from` clause in any context (statement or lambda):
///   `` delete from `.t where … `` / `` update … from `.t where … ``
///   Even inside lambdas the parser emits an ApplyExpr chain; we detect the
///   `from`-apply pattern: LiteralExpr is first child of ApplyExpr whose
///   parent is an ApplyExpr with `IdentExpr("from")` as its first child.
///
/// Excluded: dict/list indexing (`r`id`), assignment RHS (`x:`sym`),
/// symbol lists, function arguments, etc.
fn is_inplace_table_symbol(token: &q_parser::SyntaxToken) -> bool {
    let Some(lit) = token.parent() else { return false };
    if lit.kind() != SyntaxKind::LiteralExpr {
        return false;
    }

    let Some(parent) = lit.parent() else { return false };

    match parent.kind() {
        // `` `.t upsert rows `` / `` `.t insert rows ``
        // CST: BinExpr { LiteralExpr(sym), Ident("upsert"|"insert"), … }
        SyntaxKind::BinExpr => {
            parent.first_child().as_ref() == Some(&lit)
                && parent
                    .children_with_tokens()
                    .filter_map(|el| el.into_token())
                    .any(|t| t.kind() == SyntaxKind::Ident
                        && matches!(t.text(), "upsert" | "insert"))
        }

        SyntaxKind::ApplyExpr => {
            if parent.first_child().as_ref() != Some(&lit) {
                return false;
            }
            let Some(grandparent) = parent.parent() else { return false };

            // Pattern A: LiteralExpr → ApplyExpr → DeleteExpr|UpdateExpr|SelectExpr
            // (statement-level `delete from `.t` with no column list)
            if matches!(grandparent.kind(),
                SyntaxKind::DeleteExpr | SyntaxKind::UpdateExpr
                | SyntaxKind::SelectExpr | SyntaxKind::ExecExpr)
            {
                return true;
            }

            // Pattern B: `from`-apply chain — covers lambdas and update with
            // column assignments where the parser folds `from` into an apply.
            // LiteralExpr → ApplyExpr → ApplyExpr { IdentExpr("from"), … }
            is_from_apply(&grandparent)
        }
        _ => false,
    }
}

/// True when a token is an `IdentExpr`/`DottedIdent` used as the table
/// argument of a qSQL `from` clause.  Lets us skip the `is_in_qsql` guard
/// for the table name while still filtering out column names.
fn is_qsql_from_table_ident(token: &q_parser::SyntaxToken) -> bool {
    let Some(ident_expr) = token.parent() else { return false };
    if ident_expr.kind() != SyntaxKind::IdentExpr { return false; }
    let Some(parent) = ident_expr.parent() else { return false };

    // Case 1: IdentExpr is a direct child of a qSQL statement node.
    // Occurs in `select from .t` (no column list, so parser places .t directly).
    if matches!(parent.kind(),
        SyntaxKind::SelectExpr | SyntaxKind::UpdateExpr
        | SyntaxKind::DeleteExpr | SyntaxKind::ExecExpr)
    {
        return true;
    }

    // Case 2: `from`-apply chain (the common case — see is_inplace_table_symbol).
    // IdentExpr is first child of ApplyExpr whose parent is ApplyExpr { from, … }.
    if parent.kind() == SyntaxKind::ApplyExpr
        && parent.first_child().as_ref() == Some(&ident_expr)
    {
        if let Some(grandparent) = parent.parent() {
            return is_from_apply(&grandparent);
        }
    }

    false
}

/// True when `token` is the column-name LHS of a `BinExpr` with `:` inside
/// a `TableExpr` — e.g. the `id` in `([id:`u#`long$()]...)`.
/// These are column definitions, not references to globals.
fn is_col_def_in_table(token: &q_parser::SyntaxToken) -> bool {
    let Some(ident_expr) = token.parent() else { return false };
    if ident_expr.kind() != SyntaxKind::IdentExpr { return false };
    let Some(bin) = ident_expr.parent() else { return false };
    if bin.kind() != SyntaxKind::BinExpr { return false };
    if bin.first_child().as_ref() != Some(&ident_expr) { return false };
    let has_colon = bin.children_with_tokens()
        .filter_map(|el| el.into_token())
        .any(|t| matches!(t.kind(), SyntaxKind::Colon | SyntaxKind::ColonColon));
    has_colon && bin.ancestors().any(|n| n.kind() == SyntaxKind::TableExpr)
}

/// True when `node` is an `ApplyExpr` whose first child is `IdentExpr("from")`.
fn is_from_apply(node: &q_parser::SyntaxNode) -> bool {
    if node.kind() != SyntaxKind::ApplyExpr { return false; }
    node.first_child()
        .filter(|fc| fc.kind() == SyntaxKind::IdentExpr)
        .map(|fc| fc.children_with_tokens()
            .filter_map(|el| el.into_token())
            .any(|t| t.kind() == SyntaxKind::Ident && t.text() == "from"))
        .unwrap_or(false)
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
    fn symbol_upsert_included_in_refs_from_bare_name() {
        let src = "\\d .cache\ncache:1\n\\d .\n`.cache.cache upsert 2";
        let cursor = src.find("cache:1").unwrap();
        let r = refs(src, cursor, true);
        let sym_off = src.find("`.cache.cache").unwrap();
        assert!(r.contains(&sym_off), "upsert symbol ref missing; got {r:?}");
    }

    #[test]
    fn symbol_delete_included_in_refs_from_dotted_name() {
        let src = "\\d .cache\ncache:1\n\\d .\ndelete from `.cache.cache where 1b";
        let cursor = src.find("cache:1").unwrap();
        let r = refs(src, cursor, true);
        let sym_off = src.find("`.cache.cache").unwrap();
        assert!(r.contains(&sym_off), "delete symbol ref missing; got {r:?}");
    }

    #[test]
    fn symbol_update_from_included() {
        // update with column assignment: `update col:val from `.t where …`
        let src = "\\d .cache\ncache:1\n\\d .\nupdate x:now from `.cache.cache where 1b";
        let cursor = src.find("cache:1").unwrap();
        let r = refs(src, cursor, true);
        let sym_off = src.find("`.cache.cache").unwrap();
        assert!(r.contains(&sym_off), "update-from symbol ref missing; got {r:?}");
    }

    #[test]
    fn symbol_delete_in_lambda_included() {
        // Inside a lambda the parser emits ApplyExpr chains, not DeleteExpr.
        let src = "\\d .cache\ncache:1\n\\d .\ndrop:{delete from `.cache.cache where id in x}";
        let cursor = src.find("cache:1").unwrap();
        let r = refs(src, cursor, true);
        let sym_off = src.find("`.cache.cache").unwrap();
        assert!(r.contains(&sym_off), "lambda delete symbol ref missing; got {r:?}");
    }

    #[test]
    fn ident_from_table_in_select_included() {
        // `.cache.cache` (dotted ident) as the FROM table in a select.
        let src = "\\d .cache\ncache:1\n\\d .\nr:select id from .cache.cache where 1b";
        let cursor = src.find("cache:1").unwrap();
        let r = refs(src, cursor, true);
        let tbl_off = src.rfind(".cache.cache").unwrap();
        assert!(r.contains(&tbl_off), "select from table ref missing; got {r:?}");
    }

    #[test]
    fn qsql_column_name_not_included_in_refs() {
        // `id` in select column list is a column name, not a ref to global id
        let src = "id:0j\nselect date,id from t";
        let cursor = src.find("id:0j").unwrap();
        let r = refs(src, cursor, true);
        let qsql_off = src.find("date,id").unwrap() + "date,".len();
        assert!(!r.contains(&qsql_off), "qsql column falsely included; got {r:?}");
    }

    #[test]
    fn expr_level_qsql_column_not_included_in_refs() {
        // update inside $[…] is parsed as ApplyExpr chain, not UpdateExpr.
        // `id` in the where clause is a column name, not a ref to global id.
        let src = "id:0j\n$[1b; update lastaccess:now from t where id=1; 0]";
        let cursor = src.find("id:0j").unwrap();
        let r = refs(src, cursor, true);
        let qsql_off = src.find("where id").unwrap() + "where ".len();
        assert!(!r.contains(&qsql_off), "expr-level qsql col falsely included; got {r:?}");
    }

    #[test]
    fn dict_index_symbol_not_included_in_refs() {
        // r`id — dict indexing; `id must NOT appear as a ref to global `id`
        let src = "id:0j\nr`id";
        let cursor = src.find("id:0j").unwrap();
        let r = refs(src, cursor, true);
        let idx_off = src.find("r`id").unwrap() + 1; // offset of `id
        assert!(!r.contains(&idx_off), "dict index falsely included; got {r:?}");
    }

    #[test]
    fn symbol_assignment_rhs_not_included_in_refs() {
        // r:`id — `id is just a symbol value being assigned, not a table ref
        let src = "id:0j\nr:`id";
        let cursor = src.find("id:0j").unwrap();
        let r = refs(src, cursor, true);
        let sym_off = src.find(":`id").unwrap() + 1;
        assert!(!r.contains(&sym_off), "rhs symbol falsely included; got {r:?}");
    }

    #[test]
    fn table_col_def_lhs_not_included_as_ref() {
        // `id` in `([id:`u#`long$()]...)` is a column name, not a ref to global id
        let src = "\\d .cache\nid:0j\ncache:([id:`u#`long$()] size:`long$())\nuse:id";
        let cursor = src.find("id:0j").unwrap();
        let r = refs(src, cursor, true);
        // col def offset
        let col_off = src.find("([id:").unwrap() + 2; // offset of `id` inside ([
        assert!(!r.contains(&col_off), "table col def falsely included; got {r:?}");
        // the use site SHOULD be included
        let use_off = src.rfind("use:id").unwrap() + "use:".len();
        assert!(r.contains(&use_off), "use site missing; got {r:?}");
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
