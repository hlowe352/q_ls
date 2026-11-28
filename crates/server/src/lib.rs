//! The q language server. The binary (`main.rs`) serves it over stdio.

mod backend;
mod binder;
#[cfg(test)]
mod binding_oracle;
mod builtins;
mod completion;
mod config;
mod diagnostics;
mod document;
mod goto_def;
mod hover;
mod line_index;
mod references;
mod rename;
mod semantic;
mod symbols;
mod syntax;
mod workspace_index;

pub use backend::QLanguageServer;

/// Run every editor feature over `text`, at every token, for the fuzzer
/// (`fuzz/fuzz_targets/document.rs`) to look for panics. Also checks that
/// each char boundary maps to an editor position and back.
#[doc(hidden)]
pub fn exercise(text: &str) {
    use tower_lsp_server::ls_types::Uri;
    let doc = document::Document::new(text.to_string(), 0);
    let workspace = workspace_index::WorkspaceIndex::default();
    let uri: Uri = "file:///fuzz.q".parse().unwrap();
    diagnostics::compute_diagnostics(&doc, &workspace, &config::Config::default());
    semantic::semantic_tokens(&doc);
    symbols::document_symbols(&doc);
    for (offset, _) in text.char_indices().chain([(text.len(), ' ')]) {
        let pos = doc.position_of(offset);
        assert_eq!(
            doc.offset_of(pos),
            offset,
            "position round trip at {offset}"
        );
    }
    for token in syntax::tokens(&doc.parse().syntax()) {
        let pos = doc.position_of(token.text_range().start().into());
        goto_def::goto_definition(&doc, pos, &uri, &workspace);
        hover::hover(&doc, pos, &workspace);
        completion::complete(&doc, pos, &workspace);
        references::find_references(&doc, pos, true, &uri, &workspace);
        rename::rename(&doc, pos, "renamed", &uri, &workspace);
    }
}
