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

use backend::QLanguageServer;
use tower_lsp_server::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(QLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
