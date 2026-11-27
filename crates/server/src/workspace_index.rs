//! Every known `.q` document (open or on disk) plus a cross-file index of
//! their global definitions.

use crate::document::Document;
use smol_str::SmolStr;
use std::collections::HashMap;
use tower_lsp_server::ls_types::Uri;

#[derive(Default)]
pub struct WorkspaceIndex {
    files: HashMap<Uri, Document>,
    globals: HashMap<SmolStr, Vec<(Uri, u32)>>,
}

impl WorkspaceIndex {
    /// Add or replace `uri`, re-indexing its globals.
    pub fn insert(&mut self, uri: Uri, doc: Document) {
        self.remove(&uri);
        for (name, offsets) in doc.bindings().globals() {
            let sites = self.globals.entry(SmolStr::new(name)).or_default();
            sites.extend(offsets.iter().map(|&off| (uri.clone(), off)));
        }
        self.files.insert(uri, doc);
    }

    /// Remove `uri` and its globals, returning the document if present.
    pub fn remove(&mut self, uri: &Uri) -> Option<Document> {
        let doc = self.files.remove(uri)?;
        for (name, _) in doc.bindings().globals() {
            if let Some(sites) = self.globals.get_mut(name) {
                sites.retain(|(u, _)| u != uri);
                if sites.is_empty() {
                    self.globals.remove(name);
                }
            }
        }
        Some(doc)
    }

    pub fn get(&self, uri: &Uri) -> Option<&Document> {
        self.files.get(uri)
    }

    pub fn resolve_global(&self, name: &str) -> Option<&Vec<(Uri, u32)>> {
        self.globals.get(name)
    }

    pub fn files(&self) -> impl Iterator<Item = (&Uri, &Document)> {
        self.files.iter()
    }

    pub fn all_idents(&self) -> impl Iterator<Item = &str> {
        self.globals.keys().map(SmolStr::as_str)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn uri(s: &str) -> Uri {
        format!("file:///{s}").parse().unwrap()
    }

    pub fn doc(src: &str) -> Document {
        Document::new(src.to_string(), 0)
    }

    /// Strip test markers from `case`: `«` marks the cursor and each `»` an
    /// expected site. Returns `(source, cursor, sites)` as byte offsets.
    pub fn markers(case: &str) -> (String, usize, Vec<usize>) {
        let mut src = String::new();
        let (mut cursor, mut sites) = (0, Vec::new());
        for c in case.chars() {
            match c {
                '«' => cursor = src.len(),
                '»' => sites.push(src.len()),
                _ => src.push(c),
            }
        }
        (src, cursor, sites)
    }

    /// Index built from `(file name, source)` pairs.
    pub fn index(files: &[(&str, &str)]) -> WorkspaceIndex {
        let mut idx = WorkspaceIndex::default();
        for (name, src) in files {
            idx.insert(uri(name), doc(src));
        }
        idx
    }

    #[test]
    fn insert_reindex_and_remove() {
        let mut idx = index(&[("a.q", "foo:1; bar:2")]);
        assert!(idx.resolve_global("foo").is_some());
        assert!(idx.resolve_global("bar").is_some());
        assert!(idx.resolve_global("baz").is_none());

        // Re-inserting replaces stale globals.
        idx.insert(uri("a.q"), doc("new:1"));
        assert_eq!(idx.resolve_global("new").map(Vec::len), Some(1));
        assert!(idx.resolve_global("foo").is_none(), "stale global evicted");
        assert_eq!(idx.files().count(), 1);

        assert!(idx.remove(&uri("a.q")).is_some());
        assert!(idx.resolve_global("new").is_none());
        assert!(idx.remove(&uri("a.q")).is_none());
    }

    #[test]
    fn resolve_global_returns_all_def_sites() {
        let idx = index(&[("a.q", "foo:1"), ("b.q", "foo:2")]);
        let sites = idx.resolve_global("foo").expect("found");
        assert_eq!(sites.len(), 2);
        assert_ne!(sites[0].0, sites[1].0, "def sites from distinct files");
    }

    #[test]
    fn all_idents_merges_files_and_excludes_locals() {
        let idx = index(&[
            ("a.q", "alpha:1"),
            ("b.q", "globalFn:{[localParam] tmp:localParam+1; tmp}"),
        ]);
        let idents: Vec<&str> = idx.all_idents().collect();
        for want in ["alpha", "globalFn"] {
            assert!(idents.contains(&want), "{want} missing: {idents:?}");
        }
        for local in ["localParam", "tmp"] {
            assert!(!idents.contains(&local), "{local} leaked: {idents:?}");
        }
    }
}
