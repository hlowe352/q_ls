#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;
use std::collections::HashMap;
use crate::document::Document;
use crate::workspace_index::WorkspaceIndex;

#[allow(dead_code)]
pub fn goto_definition(doc: &Document, pos: Position, uri: &Uri) -> Option<GotoDefinitionResponse> {
    goto_definition_with_workspace(doc, pos, uri, &HashMap::new(), &WorkspaceIndex::default())
}

pub fn goto_definition_with_workspace(
    doc: &Document,
    pos: Position,
    uri: &Uri,
    _open_docs: &HashMap<Uri, Document>,
    workspace: &WorkspaceIndex,
) -> Option<GotoDefinitionResponse> {
    let offset = doc.offset_of(pos);
    let (name, _, _) = doc.ident_at(offset)?;

    // 1. Try same-file resolution first.
    if let Some(def_offset) = doc.sym_table().resolve(offset, name) {
        let def_pos = doc.position_of(def_offset);
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: Range::new(def_pos, def_pos),
        }));
    }

    // 2. Fall back to workspace-wide globals.
    let sites = workspace.resolve_global(name)?;
    let locations: Vec<Location> = sites
        .iter()
        .map(|(def_uri, off)| {
            let def_pos = workspace
                .files()
                .get(def_uri)
                .map_or(Position::new(0, 0), |d| d.position_of(*off as usize));
            Location {
                uri: def_uri.clone(),
                range: Range::new(def_pos, def_pos),
            }
        })
        .collect();
    if locations.is_empty() {
        None
    } else {
        Some(GotoDefinitionResponse::Array(locations))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def_offset(src: &str, cursor_byte: usize, name: &str) -> Option<usize> {
        let doc = Document::new(src.to_string(), 0);
        doc.sym_table().resolve(cursor_byte, name)
    }

    #[test]
    fn lambda_param_wins_over_top_level() {
        let src = "fn:99;\nf:{[fn] fn+1}";
        let cursor = src.find("fn+1").unwrap();
        let off = def_offset(src, cursor, "fn").expect("found");
        let expected = src.find("[fn]").unwrap() + 1;
        assert_eq!(off, expected);
    }

    #[test]
    fn local_let_wins_over_top_level() {
        let src = "fn:99;\nf:{x:42; x+fn}";
        let cursor = src.find("x+fn").unwrap();
        let off = def_offset(src, cursor, "x").expect("found");
        let expected = src.find("x:42").unwrap();
        assert_eq!(off, expected);
    }

    #[test]
    fn top_level_resolves_when_outside_lambda() {
        let src = "fn:99;\nbar:fn";
        let cursor = src.rfind("fn").unwrap();
        let off = def_offset(src, cursor, "fn").expect("found");
        let expected = src.find("fn:99").unwrap();
        assert_eq!(off, expected);
    }

    #[test]
    fn nested_lambda_param_shadows_outer_param() {
        let src = "f:{[fn] g:{[fn] fn+1}; fn}";
        let cursor = src.find("fn+1").unwrap();
        let off = def_offset(src, cursor, "fn").expect("found");
        let inner = src.find("g:{[fn]").unwrap() + "g:{[".len();
        assert_eq!(off, inner);
    }

    #[test]
    fn local_in_other_function_is_not_visible() {
        let src = "f:{[x] fn:99; x};\ng:{[y] fn+y}";
        let cursor = src.find("fn+y").unwrap();
        let off = def_offset(src, cursor, "fn");
        assert!(off.is_none(), "must not see fn from f's body, got {off:?}");
    }

    #[test]
    fn last_occurrence_wins() {
        let src = "a:1;\na:2;\na";
        let cursor = src.rfind('a').unwrap();
        let off = def_offset(src, cursor, "a").expect("found");
        let expected = src.find("a:2").unwrap();
        assert_eq!(off, expected);
    }

    #[test]
    fn forward_reference_to_top_level_resolves() {
        let src = "a;\na:1";
        let cursor = src.find('a').unwrap();
        let off = def_offset(src, cursor, "a").expect("found");
        let expected = src.find("a:1").unwrap();
        assert_eq!(off, expected);
    }

    #[test]
    fn truly_undefined_returns_none() {
        let src = "f:{[x] x+y}";
        let cursor = src.find("y}").unwrap();
        let off = def_offset(src, cursor, "y");
        assert!(off.is_none(), "y is not defined anywhere, got {off:?}");
    }

    #[test]
    fn dotted_assignment_inside_lambda_is_global() {
        let src = "init:{.app.cfg:1};\nuse:.app.cfg";
        let cursor = src.rfind(".app.cfg").unwrap();
        let off = def_offset(src, cursor, ".app.cfg").expect("found");
        let expected = src.find(".app.cfg:1").unwrap();
        assert_eq!(off, expected);
    }

    #[test]
    fn double_colon_inside_lambda_is_global() {
        let src = "init:{counter::5};\nuse:counter";
        let cursor = src.rfind("counter").unwrap();
        let off = def_offset(src, cursor, "counter").expect("found");
        let expected = src.find("counter::5").unwrap();
        assert_eq!(off, expected);
    }

    #[test]
    fn implicit_x_resolves_inside_paramless_lambda() {
        let src = "{0=count x}";
        let cursor = src.find("count x").unwrap() + "count ".len();
        let off = def_offset(src, cursor, "x").expect("found");
        let lambda_open = src.find('{').unwrap();
        assert_eq!(off, lambda_open);
    }

    #[test]
    fn local_let_shadows_implicit_x() {
        let src = "{x:42; x+1}";
        let cursor = src.find("x+1").unwrap();
        let off = def_offset(src, cursor, "x").expect("found");
        let expected = src.find("x:42").unwrap();
        assert_eq!(off, expected, "local let must shadow implicit x");
    }

    #[test]
    fn list_pattern_assignment_binds_each_name() {
        let src = "{[p] (a;b;c):p; a+b+c}";
        let cursor = src.find("a+b").unwrap();
        let off = def_offset(src, cursor, "a").expect("found a");
        let expected = src.find("(a").unwrap() + 1;
        assert_eq!(off, expected, "expected `a` from `(a;b;c):p`");
    }

    #[test]
    fn d_directive_bare_ref_resolves_inside_block() {
        // Inside \d .cache, `trackperf` refers to .cache.trackperf
        let src = "\\d .cache\ntrackperf:{[id] id}\nadd:{trackperf[1]}\n\\d .";
        let cursor = src.find("trackperf[1]").unwrap();
        let expected = src.find("trackperf:{").unwrap();
        let off = def_offset(src, cursor, "trackperf").expect("found");
        assert_eq!(off, expected);
    }

    #[test]
    fn d_directive_dotted_ref_resolves_from_outside() {
        // Outside \d .cache, .cache.trackperf resolves to its definition
        let src = "\\d .cache\ntrackperf:{[id] id}\n\\d .\nuse:.cache.trackperf";
        let cursor = src.rfind(".cache.trackperf").unwrap();
        let expected = src.find("trackperf:{").unwrap();
        let off = def_offset(src, cursor, ".cache.trackperf").expect("found");
        assert_eq!(off, expected);
    }

    /// Wide-stack thread: rowan's `GreenNode` drops recursively and
    /// dbmaint.q nests deep enough to overflow the default 2 MB test
    /// thread stack on teardown — not a logic issue.
    #[test]
    fn cross_file_goto_def() {
        use crate::workspace_index::WorkspaceIndex;
        use std::collections::HashMap;

        let uri_a: Uri = "file:///a.q".parse().unwrap();
        let uri_b: Uri = "file:///b.q".parse().unwrap();

        let doc_b = Document::new("foo+1".to_string(), 0);

        let mut idx = WorkspaceIndex::default();
        idx.index_file(uri_a.clone(), Document::new("foo:42".to_string(), 0));

        let open_docs: HashMap<Uri, Document> = HashMap::new();
        let result = goto_definition_with_workspace(&doc_b, Position::new(0, 0), &uri_b, &open_docs, &idx);
        let result = result.expect("should resolve cross-file");
        match result {
            GotoDefinitionResponse::Array(locs) => {
                assert!(!locs.is_empty(), "expected at least one location");
                assert_eq!(locs[0].uri, uri_a);
            }
            GotoDefinitionResponse::Scalar(loc) => {
                assert_eq!(loc.uri, uri_a);
            }
            _ => panic!("unexpected response variant"),
        }
    }

    #[test]
    fn same_file_def_still_works_with_workspace() {
        use crate::workspace_index::WorkspaceIndex;
        use std::collections::HashMap;

        let uri: Uri = "file:///x.q".parse().unwrap();
        let doc = Document::new("bar:99; bar+1".to_string(), 0);
        let idx = WorkspaceIndex::default();
        let open_docs: HashMap<Uri, Document> = HashMap::new();
        let result = goto_definition_with_workspace(&doc, doc.position_of(8), &uri, &open_docs, &idx);
        assert!(result.is_some(), "same-file def should still resolve");
    }

    #[test]
    fn dbmaint_fn_resolves_to_lambda_param() {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let src = std::fs::read_to_string(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../parser/tests/data/real_q/dbmaint.q",
                )).expect("dbmaint.q fixture");

                let body_marker = "newVal:fn ";
                let cursor = src.find(body_marker).unwrap() + "newVal:".len();
                let lambda_open = src.find("fn1Col:{[").unwrap();
                let param_off = src[lambda_open..].find("fn]").unwrap() + lambda_open;

                let off = def_offset(&src, cursor, "fn").expect("found");
                assert_eq!(off, param_off,
                    "expected goto-def to land on fn1Col's `fn` parameter \
                     at byte {param_off}, got {off}");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
