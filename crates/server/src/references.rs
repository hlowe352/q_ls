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
use crate::syntax::{has_ancestor, in_row_context, is_assignment_lhs, tokens};
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
/// not qSQL column names (see [`in_row_context`]) and not column
/// definitions in a table constructor.
fn candidate_idents(doc: &Document) -> impl Iterator<Item = SyntaxToken> {
    tokens(&doc.parse().syntax()).filter(|t| {
        matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::DottedIdent)
            && t.parent()
                .is_some_and(|p| !in_row_context(&p) && !is_col_def_in_table(&p))
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
            && is_table_symbol(tok)
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

/// Is this backtick symbol naming a table in place: the table of a qSQL
/// `from` clause, or the LHS of `upsert` / `insert`? Excludes dict
/// indexing (`` r`id ``), assignment RHS, etc.
fn is_table_symbol(token: &SyntaxToken) -> bool {
    let Some(lit) = token
        .parent()
        .filter(|l| l.kind() == SyntaxKind::LiteralExpr)
    else {
        return false;
    };
    let Some(parent) = lit.parent() else {
        return false;
    };
    match parent.kind() {
        SyntaxKind::FromClause => true,
        // `` `.t upsert rows `` / `` `.t insert rows ``
        SyntaxKind::BinExpr => {
            parent.first_child().as_ref() == Some(&lit)
                && parent.children_with_tokens().any(|t| {
                    t.as_token().is_some_and(|t| {
                        t.kind() == SyntaxKind::Ident && matches!(t.text(), "upsert" | "insert")
                    })
                })
        }
        _ => false,
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
    use crate::workspace_index::tests::{doc, index, markers, uri};

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

    /// Single-file cases: `«` marks the cursor and `»` each expected
    /// reference (declarations included). The set must match exactly.
    #[rustfmt::skip]
    const CASES: &[&str] = &[
        "«»x:1; y:»x+»x; z:»x",
        "x:99; f:{[«»x] »x+1}; y:x",                        // param scope excludes outer x
        "f:{»a:1; »a:2; «»a}",                              // rebindings are one symbol
        "»a:1; «»a:2; »a",
        "a:1«; b:2;",                                       // not on an identifier
        // Tables named by backtick symbols in upsert / from clauses
        "\\d .cache\n«»cache:1\n\\d .\n»`.cache.cache upsert 2",
        "\\d .cache\n«»cache:1\n\\d .\ndelete from »`.cache.cache where 1b",
        "\\d .cache\n«»cache:1\n\\d .\nupdate x:now from »`.cache.cache where 1b",
        "\\d .cache\n«»cache:1\n\\d .\ndrop:{delete from »`.cache.cache where id in x}",
        "\\d .cache\n«»cache:1\n\\d .\nr:select id from ».cache.cache where 1b",
        // Qualified and bare forms are the same symbol
        "\\d .cache\n»cache:1\nf:{exec size from »cache}\n\\d .\nshow «».cache.cache",
        "\\d .cache\n«»cache:1\nf:{if[1b; exec size from »cache; 0]}",
        "\\d .cache\n«»cache:1\nevict:{[r] r:select lastaccess,id from »cache; r}",
        // `from` expressions are code; columns (incl. of nested queries) are not
        "«»f:{x}\nselect from »f[1]",
        "«»id:0j\nselect from `a xasc select id from t",
        "«»id:0j\nselect date,id from t",
        "«»id:0j\n$[1b; update lastaccess:now from t where id=1; 0]",
        // Symbols that are data, and table column definitions, are not refs
        "«»id:0j\nr`id",
        "«»id:0j\nr:`id",
        "\\d .cache\n«»id:0j\ncache:([id:`u#`long$()] size:`long$())\nuse:»id",
    ];

    #[test]
    fn single_file_cases() {
        for case in CASES {
            let (src, cursor, want) = markers(case);
            assert_eq!(refs(&src, cursor, true), want, "case {case:?}");
        }
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
        assert_ne!(refs(src, def, false), [0usize; 0]);
        assert_ne!(refs(src, src.find("loc:").unwrap(), false), [0usize; 0]);
    }
}
