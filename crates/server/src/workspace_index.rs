use std::collections::{HashMap, HashSet};
use smol_str::SmolStr;
use tower_lsp_server::ls_types::Uri;
use crate::document::Document;

#[allow(dead_code)]
#[derive(Default)]
pub struct WorkspaceIndex {
    files: HashMap<Uri, Document>,
    globals: HashMap<SmolStr, Vec<(Uri, u32)>>,
    file_globals: HashMap<Uri, HashSet<SmolStr>>,
}

#[allow(dead_code)]
impl WorkspaceIndex {
    pub fn index_file(&mut self, uri: Uri, doc: Document) {
        self.remove_file(&uri);

        let mut names: HashSet<SmolStr> = HashSet::new();
        for (name, offsets) in doc.sym_table().global_entries() {
            let sym = SmolStr::new(name);
            names.insert(sym.clone());
            for &off in offsets {
                self.globals
                    .entry(sym.clone())
                    .or_default()
                    .push((uri.clone(), off));
            }
        }

        self.file_globals.insert(uri.clone(), names);
        self.files.insert(uri, doc);
    }

    pub fn remove_file(&mut self, uri: &Uri) {
        if let Some(names) = self.file_globals.remove(uri) {
            for name in &names {
                if let Some(v) = self.globals.get_mut(name) {
                    v.retain(|(u, _)| u != uri);
                    if v.is_empty() {
                        self.globals.remove(name);
                    }
                }
            }
        }
        self.files.remove(uri);
    }

    pub fn resolve_global(&self, name: &str) -> Option<&Vec<(Uri, u32)>> {
        self.globals.get(name)
    }

    pub fn files(&self) -> &HashMap<Uri, Document> {
        &self.files
    }

    pub fn all_idents(&self) -> impl Iterator<Item = &str> {
        self.files.values().flat_map(|doc| doc.sym_table().idents())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    fn uri(s: &str) -> Uri {
        s.parse().unwrap()
    }

    fn doc(src: &str) -> Document {
        Document::new(src.to_string(), 0)
    }

    #[test]
    fn index_file_adds_globals() {
        let mut idx = WorkspaceIndex::default();
        idx.index_file(uri("file:///a.q"), doc("foo:1; bar:2"));
        assert!(idx.resolve_global("foo").is_some());
        assert!(idx.resolve_global("bar").is_some());
        assert!(idx.resolve_global("baz").is_none());
    }

    #[test]
    fn reindex_replaces_stale_globals() {
        let mut idx = WorkspaceIndex::default();
        let u = uri("file:///a.q");
        idx.index_file(u.clone(), doc("old:1"));
        idx.index_file(u.clone(), doc("new:1"));
        let sites = idx.resolve_global("new").expect("new must be indexed");
        assert_eq!(sites.len(), 1, "exactly one def site");
        assert_eq!(idx.files().len(), 1, "exactly one file in index");
        assert!(idx.resolve_global("old").is_none(), "stale global must be evicted");
    }

    #[test]
    fn remove_file_evicts_globals() {
        let mut idx = WorkspaceIndex::default();
        let u = uri("file:///a.q");
        idx.index_file(u.clone(), doc("foo:1"));
        idx.remove_file(&u);
        assert!(idx.resolve_global("foo").is_none());
    }

    #[test]
    fn resolve_global_returns_all_def_sites() {
        let mut idx = WorkspaceIndex::default();
        idx.index_file(uri("file:///a.q"), doc("foo:1"));
        idx.index_file(uri("file:///b.q"), doc("foo:2"));
        let sites = idx.resolve_global("foo").expect("found");
        assert_eq!(sites.len(), 2);
        let uris: Vec<_> = sites.iter().map(|(u, _)| u).collect();
        assert_ne!(uris[0], uris[1], "def sites must be from distinct files");
    }

    #[test]
    fn all_idents_merges_across_files() {
        let mut idx = WorkspaceIndex::default();
        idx.index_file(uri("file:///a.q"), doc("alpha:1"));
        idx.index_file(uri("file:///b.q"), doc("beta:2"));
        let idents: Vec<&str> = idx.all_idents().collect();
        assert!(idents.contains(&"alpha"), "got: {idents:?}");
        assert!(idents.contains(&"beta"), "got: {idents:?}");
    }

    #[test]
    fn cross_file_goto_def_end_to_end() {
        use crate::goto_def::goto_definition_with_workspace;
        use std::collections::HashMap;
        use tower_lsp_server::ls_types::{GotoDefinitionResponse, Position};

        let uri_a: Uri = "file:///a.q".parse().unwrap();
        let uri_b: Uri = "file:///b.q".parse().unwrap();

        let doc_b = doc("sharedFn 99");

        let mut idx = WorkspaceIndex::default();
        idx.index_file(uri_a.clone(), doc("sharedFn:{x+1}"));

        let open_docs: HashMap<Uri, crate::document::Document> = HashMap::new();
        let result = goto_definition_with_workspace(
            &doc_b,
            Position::new(0, 0),
            &uri_b,
            &open_docs,
            &idx,
        );
        assert!(result.is_some(), "cross-file goto-def must resolve sharedFn");
        match result.unwrap() {
            GotoDefinitionResponse::Array(locs) => {
                assert!(!locs.is_empty());
                assert_eq!(locs[0].uri, uri_a);
            }
            GotoDefinitionResponse::Scalar(loc) => assert_eq!(loc.uri, uri_a),
            _ => panic!("unexpected response variant"),
        }
    }

    #[test]
    fn cross_file_diagnostic_suppression_end_to_end() {
        use crate::diagnostics::compute_diagnostics_with_workspace;

        let mut idx = WorkspaceIndex::default();
        idx.index_file(
            "file:///lib.q".parse().unwrap(),
            doc("libFn:{x*2}"),
        );

        let consumer = doc("libFn 5");
        let diags = compute_diagnostics_with_workspace(&consumer, &idx);
        let bad: Vec<_> = diags.iter().filter(|d| d.message.contains("libFn")).collect();
        assert!(bad.is_empty(),
            "workspace-defined symbol must not produce unresolved warning: {diags:?}");
    }
}
