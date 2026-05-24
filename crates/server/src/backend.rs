use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer};

use crate::document::Document;
use crate::workspace_index::WorkspaceIndex;

pub struct QLanguageServer {
    client: Client,
    documents: Arc<RwLock<HashMap<Uri, Document>>>,
    workspace_index: Arc<RwLock<WorkspaceIndex>>,
    workspace_root: Arc<RwLock<Option<PathBuf>>>,
}

impl QLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            workspace_index: Arc::new(RwLock::new(WorkspaceIndex::default())),
            workspace_root: Arc::new(RwLock::new(None)),
        }
    }

    async fn on_change(&self, uri: Uri, doc: &Document) {
        let idx = self.workspace_index.read().await;
        let diagnostics = crate::diagnostics::compute_diagnostics_with_workspace(doc, &idx);
        self.client
            .publish_diagnostics(uri, diagnostics, Some(doc.version()))
            .await;
    }
}

impl LanguageServer for QLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        #[allow(deprecated)]
        let root: Option<PathBuf> = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .and_then(|f| f.uri.to_file_path().map(Cow::into_owned))
            .or_else(|| {
                params
                    .root_uri
                    .as_ref()
                    .and_then(|u| u.to_file_path().map(Cow::into_owned))
            });
        *self.workspace_root.write().await = root;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(false),
                        })),
                        ..Default::default()
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".into(), "`".into()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                        legend: {
                            let (token_types, token_modifiers) = crate::semantic::legend();
                            SemanticTokensLegend { token_types, token_modifiers }
                        },
                        range: Some(false),
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                    }),
                ),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "q-ls".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            // tower-lsp-server defaults to UTF-16, which is what LSP spec
            // mandates and what our `LineIndex` is built for. `None` ==
            // "do not negotiate, use spec default" — explicit so the field
            // doesn't read as forgotten.
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "q-ls initialized").await;

        let registration = Registration {
            id: "watch-q-files".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: Some(
                serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                    watchers: vec![FileSystemWatcher {
                        glob_pattern: GlobPattern::String("**/*.q".to_string()),
                        kind: None,
                    }],
                })
                .unwrap(),
            ),
        };
        let _ = self.client.register_capability(vec![registration]).await;

        let root = self.workspace_root.read().await.clone();
        if let Some(root) = root {
            let idx = Arc::clone(&self.workspace_index);
            let client = self.client.clone();
            tokio::spawn(async move {
                match tokio::task::spawn_blocking(move || collect_and_parse_q_files(&root)).await {
                    Ok(pairs) => {
                        let mut index = idx.write().await;
                        for (uri, doc) in pairs {
                            index.index_file(uri, doc);
                        }
                    }
                    Err(e) => {
                        client.log_message(MessageType::ERROR, format!("workspace indexing failed: {e}")).await;
                    }
                }
            });
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text;
        let version = params.text_document.version;

        {
            let mut idx = self.workspace_index.write().await;
            idx.index_file(uri.clone(), Document::new(text.clone(), version));
        }

        let doc = Document::new(text, version);
        self.on_change(uri.clone(), &doc).await;
        self.documents.write().await.insert(uri, doc);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        let (text, version) = {
            let mut docs = self.documents.write().await;
            let Some(doc) = docs.get_mut(&uri) else { return };
            doc.apply_changes(params.content_changes, params.text_document.version);
            (doc.text().to_string(), doc.version())
        };

        {
            let mut idx = self.workspace_index.write().await;
            idx.index_file(uri.clone(), Document::new(text.clone(), version));
        }

        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            self.on_change(uri, doc).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.write().await.remove(&params.text_document.uri);
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri) {
            let mut idx = self.workspace_index.write().await;
            idx.index_file(uri, Document::new(doc.text().to_string(), doc.version()));
        }
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        for change in params.changes {
            let uri = change.uri;
            match change.typ {
                FileChangeType::CREATED | FileChangeType::CHANGED => {
                    let Some(path) = uri.to_file_path() else { continue };
                    let Ok(text) = std::fs::read_to_string(&path) else { continue };
                    let mut idx = self.workspace_index.write().await;
                    idx.index_file(uri, Document::new(text, 0));
                }
                FileChangeType::DELETED => {
                    let mut idx = self.workspace_index.write().await;
                    idx.remove_file(&uri);
                }
                _ => {}
            }
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else { return Ok(None) };
        let items = crate::completion::complete(doc, pos);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else { return Ok(None) };
        Ok(crate::hover::hover(doc, pos))
    }

    async fn goto_definition(&self, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri.clone();
        let pos = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        let idx = self.workspace_index.read().await;
        let Some(doc) = docs.get(&uri) else { return Ok(None) };
        Ok(crate::goto_def::goto_definition_with_workspace(doc, pos, &uri, &docs, &idx))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else { return Ok(None) };
        let locs = crate::references::find_references(doc, pos, include_declaration, &uri);
        Ok(Some(locs))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let pos = params.position;
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else { return Ok(None) };
        Ok(crate::rename::prepare_rename(doc, pos))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let pos = params.text_document_position.position;
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else { return Ok(None) };
        Ok(crate::rename::rename(doc, pos, &params.new_name, &uri))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else { return Ok(None) };
        let data = crate::semantic::semantic_tokens(doc);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn document_symbol(&self, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(uri) else { return Ok(None) };
        let symbols = crate::symbols::document_symbols(doc);
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }
}

fn collect_and_parse_q_files(root: &std::path::Path) -> Vec<(Uri, Document)> {
    let mut result = Vec::new();
    collect_recursive(root, &mut result);
    result
}

fn collect_recursive(dir: &std::path::Path, out: &mut Vec<(Uri, Document)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recursive(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("q") {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(uri) = path_to_uri(&path) else {
                continue;
            };
            out.push((uri, Document::new(text, 0)));
        }
    }
}

fn path_to_uri(path: &std::path::Path) -> std::result::Result<Uri, ()> {
    let abs = path.canonicalize().map_err(|_| ())?;
    let s = format!("file://{}", abs.display());
    s.parse().map_err(|_| ())
}
