//! `textDocument/references` — find all references to the symbol under
//! the cursor.
//!
//! "Same symbol" here means: same name, same scope. Multiple rebindings of
//! a name within one scope (`a:1; a:2; a`) are treated as one symbol with
//! several def sites — they all get returned, and rename rewrites all of
//! them. A name with the same spelling but a different scope (an outer
//! global vs. a lambda parameter) is a *different* symbol and is excluded.

use std::collections::HashSet;

use q_parser::{SyntaxKind, SyntaxNode, SyntaxToken};
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;

use crate::document::Document;
use crate::syntax::{
    has_ancestor, in_qsql, is_assignment_lhs, is_from_apply, is_qsql_stmt, tokens,
};
use crate::workspace_index::WorkspaceIndex;

pub fn find_references(
    doc: &Document,
    pos: Position,
    include_declaration: bool,
    uri: &Uri,
    workspace: &WorkspaceIndex,
) -> Vec<Location> {
    let cursor = doc.offset_of(pos);
    let table = doc.sym_table();
    let Some((name, _, _)) = doc.ident_at(cursor) else {
        return Vec::new();
    };
    let name = name.to_string();

    // All def sites of `name` in the scope the cursor lives in. If there is
    // none here, a workspace-known global still gets a cross-file scan.
    let def_offsets: HashSet<u32> = table.def_offsets_for(cursor, &name).into_iter().collect();
    let is_workspace_global = workspace.resolve_global(&name).is_some();
    if def_offsets.is_empty() && !is_workspace_global {
        return Vec::new();
    }

    // Qualified form (`.cache.cache` for `cache` inside `\d .cache`), used
    // to match dotted idents and backtick symbols.
    let qualified = table
        .qualified_for(cursor, &name)
        .map_or_else(|| name.clone(), |q| q.to_string());

    let mut out = if def_offsets.is_empty() {
        Vec::new()
    } else {
        scoped_refs(
            doc,
            uri,
            &name,
            &qualified,
            &def_offsets,
            include_declaration,
        )
    };

    // Cross-file only for globals: either no local def at all (a use whose
    // def lives elsewhere), or a local def that is in the global table
    // (not a lambda param/local).
    let g_name = table.global_def_offsets(&name);
    let g_qual = table.global_def_offsets(&qualified);
    let cursor_on_global = def_offsets.is_empty()
        || def_offsets
            .iter()
            .any(|off| g_name.contains(off) || g_qual.contains(off));
    let is_local_global = !g_name.is_empty() || !g_qual.is_empty();

    if cursor_on_global && (is_local_global || is_workspace_global) {
        if def_offsets.is_empty() {
            out.extend(global_refs(doc, uri, &name, &qualified));
        }
        // O(total tokens × files) — add an inverted index if this gets slow.
        for (other_uri, other_doc) in workspace.files().filter(|(u, _)| *u != uri) {
            out.extend(global_refs(other_doc, other_uri, &name, &qualified));
        }
    }
    out
}

fn location(doc: &Document, uri: &Uri, token: &SyntaxToken) -> Location {
    let r = token.text_range();
    Location::new(
        uri.clone(),
        Range::new(
            doc.position_of(r.start().into()),
            doc.position_of(r.end().into()),
        ),
    )
}

/// Ident/dotted-ident tokens of `doc` that could be variable references:
/// not qSQL column names (a `from` table is fine) and not column
/// definitions in a table constructor.
fn candidate_idents(doc: &Document) -> impl Iterator<Item = SyntaxToken> {
    tokens(&doc.parse().syntax()).filter(|t| {
        matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::DottedIdent)
            && t.parent().is_some_and(|p| {
                (!in_qsql(&p) || is_qsql_from_table(&p)) && !is_col_def_in_table(&p)
            })
    })
}

/// Is the token a plain reference expression (not a parameter declaration)?
fn is_ident_ref(token: &SyntaxToken) -> bool {
    token.parent().is_some_and(|p| {
        matches!(p.kind(), SyntaxKind::IdentExpr | SyntaxKind::Namespace)
            && !has_ancestor(&p, SyntaxKind::ParamList)
    })
}

/// References in `doc` that resolve to one of `def_offsets`.
fn scoped_refs(
    doc: &Document,
    uri: &Uri,
    name: &str,
    qualified: &str,
    def_offsets: &HashSet<u32>,
    include_declaration: bool,
) -> Vec<Location> {
    let table = doc.sym_table();
    // Also match the bare last component of a qualified name, so refs from
    // `.cache.cache` find bare `cache` uses inside `\d .cache`; resolution
    // against `def_offsets` still enforces correctness.
    let q = if qualified.starts_with('.') {
        qualified
    } else {
        name
    };
    let bare = q
        .rsplit('.')
        .next()
        .filter(|b| !b.is_empty() && *b != name && *b != qualified);

    let idents = candidate_idents(doc).filter(|tok| {
        let text = tok.text();
        if text != name && text != qualified && Some(text) != bare {
            return false;
        }
        let off: u32 = tok.text_range().start().into();
        if def_offsets.contains(&off) {
            return include_declaration;
        }
        is_ident_ref(tok)
            && table
                .resolve(off as usize, text)
                .is_some_and(|d| def_offsets.contains(&d))
    });
    // Backtick symbols naming the table in in-place modification contexts.
    let symbols = tokens(&doc.parse().syntax()).filter(|tok| {
        tok.kind() == SyntaxKind::Symbol
            && is_inplace_table_symbol(tok)
            && tok
                .text()
                .strip_prefix('`')
                .is_some_and(|s| s == name || s == qualified)
    });
    let mut out: Vec<SyntaxToken> = idents.chain(symbols).collect();
    out.sort_by_key(|t| t.text_range().start());
    out.iter().map(|t| location(doc, uri, t)).collect()
}

/// References in `doc` to the *global* `name`: tokens that resolve to a
/// global def in `doc`, or don't resolve locally at all (cross-file use).
fn global_refs(doc: &Document, uri: &Uri, name: &str, qualified: &str) -> Vec<Location> {
    let table = doc.sym_table();
    candidate_idents(doc)
        .filter(|tok| {
            let text = tok.text();
            (text == name || text == qualified)
                && is_ident_ref(tok)
                && table
                    .resolve(tok.text_range().start().into(), text)
                    .is_none_or(|r| table.global_def_offsets(text).contains(&r))
        })
        .map(|t| location(doc, uri, &t))
        .collect()
}

/// True only when a Symbol token is used as a global table reference:
/// the LHS of `upsert` / `insert`, or the table of a qSQL `from` clause
/// (statement-level, or the `ApplyExpr` chains the parser emits inside
/// lambdas). Excludes dict indexing (`` r`id ``), assignment RHS, etc.
fn is_inplace_table_symbol(token: &SyntaxToken) -> bool {
    let Some(lit) = token
        .parent()
        .filter(|l| l.kind() == SyntaxKind::LiteralExpr)
    else {
        return false;
    };
    let Some(parent) = lit.parent() else {
        return false;
    };
    let lit_first = parent.first_child().as_ref() == Some(&lit);
    match parent.kind() {
        _ if is_qsql_stmt(&parent) => true,
        // `` `.t upsert rows `` / `` `.t insert rows ``
        SyntaxKind::BinExpr => {
            lit_first
                && parent.children_with_tokens().any(|t| {
                    t.as_token().is_some_and(|t| {
                        t.kind() == SyntaxKind::Ident && matches!(t.text(), "upsert" | "insert")
                    })
                })
        }
        // `from tbl where …` / `from tbl[args]`: first child of an apply
        // nested in a qSQL statement or a `from`-apply.
        SyntaxKind::ApplyExpr if lit_first => parent
            .parent()
            .is_some_and(|gp| is_qsql_stmt(&gp) || is_from_apply(&gp)),
        // `from tbl`: non-first argument of a `from`-apply.
        SyntaxKind::ApplyExpr => is_from_apply(&parent),
        _ => false,
    }
}

/// Is an `IdentExpr` the table argument of a qSQL `from` clause? Lets
/// the table name through the qSQL column-name filter.
fn is_qsql_from_table(ident_expr: &SyntaxNode) -> bool {
    if ident_expr.kind() != SyntaxKind::IdentExpr {
        return false;
    }
    let Some(parent) = ident_expr.parent() else {
        return false;
    };
    // `select from .t` — no column list, table is a direct child.
    if is_qsql_stmt(&parent) {
        return true;
    }
    // `from tbl[args]` (first child of an apply inside a `from`-apply) or
    // `from tbl` (non-first argument of a `from`-apply).
    parent.kind() == SyntaxKind::ApplyExpr
        && if parent.first_child().as_ref() == Some(ident_expr) {
            parent.parent().is_some_and(|gp| is_from_apply(&gp))
        } else {
            is_from_apply(&parent)
        }
}

/// Is `ident_expr` the column-name LHS of `col:value` inside a table
/// constructor (`([id:...] ...)`)? Column definitions aren't references.
fn is_col_def_in_table(ident_expr: &SyntaxNode) -> bool {
    ident_expr.kind() == SyntaxKind::IdentExpr
        && is_assignment_lhs(ident_expr)
        && has_ancestor(ident_expr, SyntaxKind::TableExpr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_index::tests::{doc, index, uri};

    /// Start offsets of single-file references from byte `cursor`.
    fn refs(src: &str, cursor: usize, include_decl: bool) -> Vec<usize> {
        let d = doc(src);
        let idx = WorkspaceIndex::default();
        find_references(&d, d.position_of(cursor), include_decl, &uri("x.q"), &idx)
            .into_iter()
            .map(|loc| d.offset_of(loc.range.start))
            .collect()
    }

    /// Cross-file references from `cursor` in `src` (file `a.q`), with the
    /// workspace holding `others`; returns the URIs of the hits.
    fn cross_refs(src: &str, cursor: usize, others: &[(&str, &str)]) -> Vec<Uri> {
        let d = doc(src);
        let idx = index(others);
        find_references(&d, d.position_of(cursor), true, &uri("a.q"), &idx)
            .into_iter()
            .map(|l| l.uri)
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
        assert!(
            r.contains(&sym_off),
            "update-from symbol ref missing; got {r:?}"
        );
    }

    #[test]
    fn symbol_delete_in_lambda_included() {
        // Inside a lambda the parser emits ApplyExpr chains, not DeleteExpr.
        let src = "\\d .cache\ncache:1\n\\d .\ndrop:{delete from `.cache.cache where id in x}";
        let cursor = src.find("cache:1").unwrap();
        let r = refs(src, cursor, true);
        let sym_off = src.find("`.cache.cache").unwrap();
        assert!(
            r.contains(&sym_off),
            "lambda delete symbol ref missing; got {r:?}"
        );
    }

    #[test]
    fn ident_from_table_in_select_included() {
        // `.cache.cache` (dotted ident) as the FROM table in a select.
        let src = "\\d .cache\ncache:1\n\\d .\nr:select id from .cache.cache where 1b";
        let cursor = src.find("cache:1").unwrap();
        let r = refs(src, cursor, true);
        let tbl_off = src.rfind(".cache.cache").unwrap();
        assert!(
            r.contains(&tbl_off),
            "select from table ref missing; got {r:?}"
        );
    }

    #[test]
    fn refs_from_qualified_name_finds_bare_refs() {
        // go-to-ref from `.cache.cache` (dotted form) must find bare `cache`
        // refs inside `\d .cache`, same as go-to-ref from the bare definition.
        let src = "\\d .cache\ncache:1\nf:{exec size from cache}\n\\d .\nshow .cache.cache";
        // cursor on `.cache.cache` (the dotted-ident use outside the namespace)
        let r = refs(src, src.rfind(".cache.cache").unwrap(), true);
        // Must find the bare `cache` def at line 2.
        let def_off = src.find("cache:1").unwrap();
        assert!(
            r.contains(&def_off),
            "bare cache def not found from qualified cursor; got {r:?}"
        );
        // Must find the bare `cache` use inside the lambda.
        let use_off = src.rfind("from cache").unwrap() + "from ".len();
        assert!(
            r.contains(&use_off),
            "bare cache use inside lambda not found from qualified cursor; got {r:?}"
        );
    }

    #[test]
    fn exec_from_table_inside_if_included() {
        // `exec size from cache` inside if[…] is parsed as an ApplyExpr chain,
        // not ExecExpr. `cache` must still be detected as the from-table.
        let src = "\\d .cache\ncache:1\nf:{if[1b; exec size from cache; 0]}";
        let cursor = src.find("cache:1").unwrap();
        let r = refs(src, cursor, true);
        let tbl_off = src.rfind("from cache").unwrap() + "from ".len();
        assert!(
            r.contains(&tbl_off),
            "exec from-table ref inside if missing; got {r:?}"
        );
    }

    #[test]
    fn select_from_table_inside_lambda_included() {
        // `select … from cache` inside a lambda body — parsed as ApplyExpr chain.
        let src = "\\d .cache\ncache:1\nevict:{[r] r:select lastaccess,id from cache; r}";
        let cursor = src.find("cache:1").unwrap();
        let r = refs(src, cursor, true);
        let tbl_off = src.rfind("from cache").unwrap() + "from ".len();
        assert!(
            r.contains(&tbl_off),
            "select from-table ref inside lambda missing; got {r:?}"
        );
    }

    #[test]
    fn qsql_column_name_not_included_in_refs() {
        // `id` in select column list is a column name, not a ref to global id
        let src = "id:0j\nselect date,id from t";
        let cursor = src.find("id:0j").unwrap();
        let r = refs(src, cursor, true);
        let qsql_off = src.find("date,id").unwrap() + "date,".len();
        assert!(
            !r.contains(&qsql_off),
            "qsql column falsely included; got {r:?}"
        );
    }

    #[test]
    fn expr_level_qsql_column_not_included_in_refs() {
        // update inside $[…] is parsed as ApplyExpr chain, not UpdateExpr.
        // `id` in the where clause is a column name, not a ref to global id.
        let src = "id:0j\n$[1b; update lastaccess:now from t where id=1; 0]";
        let cursor = src.find("id:0j").unwrap();
        let r = refs(src, cursor, true);
        let qsql_off = src.find("where id").unwrap() + "where ".len();
        assert!(
            !r.contains(&qsql_off),
            "expr-level qsql col falsely included; got {r:?}"
        );
    }

    #[test]
    fn dict_index_symbol_not_included_in_refs() {
        // r`id — dict indexing; `id must NOT appear as a ref to global `id`
        let src = "id:0j\nr`id";
        let cursor = src.find("id:0j").unwrap();
        let r = refs(src, cursor, true);
        let idx_off = src.find("r`id").unwrap() + 1; // offset of `id
        assert!(
            !r.contains(&idx_off),
            "dict index falsely included; got {r:?}"
        );
    }

    #[test]
    fn symbol_assignment_rhs_not_included_in_refs() {
        // r:`id — `id is just a symbol value being assigned, not a table ref
        let src = "id:0j\nr:`id";
        let cursor = src.find("id:0j").unwrap();
        let r = refs(src, cursor, true);
        let sym_off = src.find(":`id").unwrap() + 1;
        assert!(
            !r.contains(&sym_off),
            "rhs symbol falsely included; got {r:?}"
        );
    }

    #[test]
    fn table_col_def_lhs_not_included_as_ref() {
        // `id` in `([id:`u#`long$()]...)` is a column name, not a ref to global id
        let src = "\\d .cache\nid:0j\ncache:([id:`u#`long$()] size:`long$())\nuse:id";
        let cursor = src.find("id:0j").unwrap();
        let r = refs(src, cursor, true);
        // col def offset
        let col_off = src.find("([id:").unwrap() + 2; // offset of `id` inside ([
        assert!(
            !r.contains(&col_off),
            "table col def falsely included; got {r:?}"
        );
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

    #[test]
    fn find_refs_from_use_site_when_def_is_in_another_file() {
        // `sharedFn` defined in b.q, used in a.q; cursor on the use.
        let uris = cross_refs("sharedFn 99", 0, &[("b.q", "sharedFn:{x+1}")]);
        assert!(uris.contains(&uri("a.q")), "use site in a.q: {uris:?}");
        assert!(uris.contains(&uri("b.q")), "def site in b.q: {uris:?}");
    }

    #[test]
    fn cross_file_global_refs_found() {
        let uris = cross_refs("foo:1", 0, &[("b.q", "foo+2")]);
        assert!(uris.contains(&uri("b.q")), "ref in b.q: {uris:?}");
    }

    #[test]
    fn lambda_param_not_found_cross_file() {
        // Cursor on param `x`; a.q has a global `x:99`, b.q its own `x:77`.
        let src = "x:99\nf:{[x] x+1}";
        let d = doc(src);
        let idx = index(&[("b.q", "x:77")]);
        let pos = d.position_of(src.find("{[x]").unwrap() + 2);
        let locs = find_references(&d, pos, true, &uri("a.q"), &idx);
        assert!(locs.iter().all(|l| l.uri == uri("a.q")), "{locs:?}");
        let global_x = src.find("x:99").unwrap();
        assert!(
            locs.iter().all(|l| d.offset_of(l.range.start) != global_x),
            "global x:99 must not appear in param refs: {locs:?}"
        );
    }

    #[test]
    fn lambda_local_not_found_cross_file() {
        let src = "f:{x:42; x}";
        let uris = cross_refs(src, src.find("x:42").unwrap(), &[("b.q", "g:{x:99; x}")]);
        assert!(uris.iter().all(|u| *u == uri("a.q")), "{uris:?}");
    }

    /// Regression: `minus` (assigned inside `$[...]`) and `loc` in aoc.q's
    /// q1b lambda must find their refs from the def and the use.
    #[test]
    fn aoc_lambda_local_refs() {
        let src = include_str!("../../parser/tests/data/real_q/aoc.q");
        let def = src.find("minus:").unwrap();
        let usage = src.find("not minus").unwrap() + "not ".len();
        assert!(refs(src, def, true).len() >= 2);
        assert!(refs(src, usage, true).len() >= 2);
        assert!(!refs(src, def, false).is_empty());
        assert!(!refs(src, src.find("loc:").unwrap(), false).is_empty());
    }
}
