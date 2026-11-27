//! `textDocument/references` — find all references to the symbol under
//! the cursor.
//!
//! "Same symbol" is the binder's: rebindings in one scope (`a:1; a:2; a`)
//! are one symbol; the same spelling in another scope is another symbol.

use q_parser::TextRange;
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;

use crate::binder::Symbol;
use crate::document::Document;
use crate::workspace_index::WorkspaceIndex;

/// Every occurrence of the symbol under the cursor: in this document, and
/// for a global in every other workspace file.
pub fn find_references(
    doc: &Document,
    pos: Position,
    include_declaration: bool,
    uri: &Uri,
    workspace: &WorkspaceIndex,
) -> Vec<Location> {
    let Some(name) = doc.bindings().name_at(doc.offset_of(pos)) else {
        return Vec::new();
    };
    let symbol = &name.symbol;
    let others = workspace
        .files()
        .filter(|(u, _)| *u != uri && matches!(symbol, Symbol::Global(_)));
    std::iter::once((uri, doc))
        .chain(others)
        .flat_map(|(u, d)| {
            d.bindings()
                .occurrences(symbol)
                .filter(|n| include_declaration || !n.is_def())
                .map(move |n| location(d, u, n.range))
        })
        .collect()
}

fn location(doc: &Document, uri: &Uri, range: TextRange) -> Location {
    Location::new(
        uri.clone(),
        Range::new(
            doc.position_of(range.start().into()),
            doc.position_of(range.end().into()),
        ),
    )
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
        "\\d .cache\n«»cache:1\n\\d .\n`».cache.cache upsert 2",
        "\\d .cache\n«»cache:1\n\\d .\ndelete from `».cache.cache where 1b",
        "\\d .cache\n«»cache:1\n\\d .\nupdate x:now from `».cache.cache where 1b",
        "\\d .cache\n«»cache:1\n\\d .\ndrop:{delete from `».cache.cache where id in x}",
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
