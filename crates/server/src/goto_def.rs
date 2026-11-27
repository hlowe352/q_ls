use crate::binder::Symbol;
use crate::document::Document;
use crate::workspace_index::WorkspaceIndex;
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;

/// The name's nearest definition in this file, else its definitions
/// elsewhere in the workspace.
pub fn goto_definition(
    doc: &Document,
    pos: Position,
    uri: &Uri,
    workspace: &WorkspaceIndex,
) -> Option<GotoDefinitionResponse> {
    let bindings = doc.bindings();
    let name = bindings.name_at(doc.offset_of(pos))?;
    let location = |uri: &Uri, d: &Document, off: u32| {
        let p = d.position_of(off as usize);
        Location::new(uri.clone(), Range::new(p, p))
    };
    if let Some(def) = bindings.definition(name) {
        return Some(GotoDefinitionResponse::Scalar(location(uri, doc, def)));
    }
    let Symbol::Global(qualified) = &name.symbol else {
        return None;
    };
    let locations: Vec<Location> = workspace
        .resolve_global(qualified)?
        .iter()
        .filter_map(|(u, off)| Some(location(u, workspace.get(u)?, *off)))
        .collect();
    (!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::workspace_index::tests::{doc, markers, uri};

    /// Flatten a goto-definition response, asserting it is non-empty.
    pub fn locations(resp: Option<GotoDefinitionResponse>) -> Vec<Location> {
        let locs = match resp.expect("expected a definition") {
            GotoDefinitionResponse::Scalar(loc) => vec![loc],
            GotoDefinitionResponse::Array(locs) => locs,
            GotoDefinitionResponse::Link(_) => panic!("unexpected Link response"),
        };
        assert_ne!(locs, []);
        locs
    }

    /// Same-file definition offset for the identifier at byte `cursor`.
    fn definition(src: &str, cursor: usize) -> Option<usize> {
        let d = doc(src);
        let resp = goto_definition(
            &d,
            d.position_of(cursor),
            &uri("x.q"),
            &WorkspaceIndex::default(),
        );
        resp.map(|r| d.offset_of(locations(Some(r))[0].range.start))
    }

    /// Scoping cases: `«` marks the cursor, `»` the expected definition
    /// (absent = unresolved). Markers are removed before parsing.
    #[rustfmt::skip]
    const CASES: &[&str] = &[
        "fn:99;\nf:{[»fn] «fn+1}",                          // param beats global
        "fn:99;\nf:{»x:42; «x+fn}",                         // local beats global
        "»fn:99;\nbar:«fn",                                 // top level
        "f:{[fn] g:{[»fn] «fn+1}; fn}",                     // inner param shadows outer
        "»a:100;\nf:{[a] g:{«a+1}; g[]}",                   // no closures: global, not outer param
        "f:{[a] g:{«a+1}; g[]}",                            // ...and outer param not visible
        "f:{[x] fn:99; x};\ng:{[y] «fn+y}",                 // other function's local
        "a:1;\n»a:2;\n«a",                                  // last definition before use
        "«a;\n»a:1",                                        // forward reference
        "f:{[x] x+«y}",                                     // undefined
        "init:{».app.cfg:1};\nuse:«.app.cfg",               // dotted assign in lambda is global
        "init:{»counter::5};\nuse:«counter",                // `::` in lambda is global
        "»{0=count «x}",                                    // implicit x
        "{»x:42; «x+1}",                                    // local shadows implicit x
        "{[p] (»a;b;c):p; «a+b+c}",                         // list-pattern binding
        "\\d .cache\n»trackperf:{[id] id}\nadd:{«trackperf[1]}\n\\d .",
        "\\d .cache\n»trackperf:{[id] id}\n\\d .\nuse:«.cache.trackperf",
        "f:{[path] »a,:raze 1 2 3; «a}",                    // `,:` on a non-local amends a global
        "f:{a,:1 2; »a,:3 4; «a}",                          // ...last amend before use
        "f:{a:0; »a,:1; «a}",                               // `,:` on a local stays local
        "f:{[a] »a::5; «a}",                                // `::` on a param assigns the param
        "»g:1\nf:{[x] if[x in «g;:()]; g,:enlist x}",       // `,:` amends an existing global
        "init:{»n+::1};\nuse:«n",                           // `+::` in lambda is global
        "init:{op::+};\nuse:«op",                           // `op::+` composes: no definition
    ];

    #[test]
    fn scoping_cases() {
        for case in CASES {
            let (src, cursor, defs) = markers(case);
            assert_eq!(
                definition(&src, cursor),
                defs.first().copied(),
                "case {case:?}"
            );
        }
    }

    /// Wide-stack thread: rowan's `GreenNode` drops recursively and
    /// dbmaint.q nests deep enough to overflow the default 2 MB test
    /// thread stack on teardown — not a logic issue.
    #[test]
    fn dbmaint_fn_resolves_to_lambda_param() {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let src = include_str!("../../parser/tests/data/real_q/dbmaint.q");
                let cursor = src.find("newVal:fn ").unwrap() + "newVal:".len();
                let lambda_open = src.find("fn1Col:{[").unwrap();
                let param = src[lambda_open..].find("fn]").unwrap() + lambda_open;
                assert_eq!(definition(src, cursor), Some(param));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn cross_file_definition_is_namespace_qualified() {
        use crate::workspace_index::tests::index;
        // `f` inside `\d .ns` is `.ns.f`, defined in b.q — not root `f`.
        let idx = index(&[("b.q", "\\d .ns\nf:{x}\n\\d ."), ("c.q", "f:0")]);
        let src = "\\d .ns\nuse:f 1";
        let d = doc(src);
        let pos = d.position_of(src.find("f 1").unwrap());
        let locs = locations(goto_definition(&d, pos, &uri("a.q"), &idx));
        assert_eq!(
            locs.iter().map(|l| &l.uri).collect::<Vec<_>>(),
            [&uri("b.q")]
        );
    }
}
