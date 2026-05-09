mod backend;
mod builtins;
mod document;
mod diagnostics;
mod completion;
mod hover;
mod goto_def;
mod line_index;
mod references;
mod rename;
mod semantic;
mod sym_table;
mod symbols;

use backend::QLanguageServer;
use tower_lsp_server::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(QLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
