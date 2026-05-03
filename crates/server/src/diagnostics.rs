use tower_lsp::lsp_types::*;
use crate::document::Document;

pub fn compute_diagnostics(doc: &Document) -> Vec<Diagnostic> {
    doc.parse().errors.iter().map(|err| {
        let start = doc.position_of(err.offset);
        let end = doc.position_of(err.offset + err.len);
        Diagnostic {
            range: Range::new(start, end),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("q-ls".into()),
            message: err.message.clone(),
            ..Default::default()
        }
    }).collect()
}
