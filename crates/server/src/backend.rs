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

    /// Set workspace root to `root` (if not already set) and kick off background
    /// indexing. Returns `true` if indexing was started, `false` if a root was
    /// already set and this call was a no-op.
    async fn try_start_indexing(&self, root: PathBuf) -> bool {
        // Atomically claim the root so concurrent did_open calls don't double-index.
        {
            let mut guard = self.workspace_root.write().await;
            if guard.is_some() {
                return false;
            }
            *guard = Some(root.clone());
        }
        self.client
            .log_message(
                MessageType::INFO,
                format!("q-ls: indexing workspace at {}", root.display()),
            )
            .await;
        let idx = Arc::clone(&self.workspace_index);
        let docs = Arc::clone(&self.documents);
        let client = self.client.clone();
        tokio::spawn(async move {
            match tokio::task::spawn_blocking(move || collect_and_parse_q_files(&root)).await {
                Ok(pairs) => {
                    let n = pairs.len();
                    {
                        let mut index = idx.write().await;
                        for (uri, doc) in pairs {
                            index.index_file(uri, doc);
                        }
                    }
                    client
                        .log_message(MessageType::INFO, format!("q-ls: indexed {n} .q files"))
                        .await;
                    // Re-publish diagnostics for all open documents now that the
                    // full workspace index is available.
                    let open = docs.read().await;
                    let index = idx.read().await;
                    for (uri, doc) in open.iter() {
                        let diags = crate::diagnostics::compute_diagnostics_with_workspace(doc, &index);
                        client.publish_diagnostics(uri.clone(), diags, Some(doc.version())).await;
                    }
                }
                Err(e) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("q-ls: workspace indexing failed: {e}"),
                        )
                        .await;
                }
            }
        });
        true
    }
}

impl LanguageServer for QLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        #[allow(deprecated)]
        let client_root: Option<PathBuf> = params
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
        // Upgrade to the nearest .git root so a client that sends a sub-folder
        // (or Neovim sending the file's parent dir) still gets the full repo.
        let root = client_root.and_then(|p| find_git_root(&p).or(Some(p)));
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

        // Start indexing if initialize resolved a root. If it didn't (client
        // sent no root at all), did_open will detect the root from the first
        // opened file and call try_start_indexing then.
        let root = self.workspace_root.read().await.clone();
        if let Some(root) = root {
            // workspace_root already set by initialize; spawn indexing directly.
            // try_start_indexing's atomic guard would no-op here.
            let idx = Arc::clone(&self.workspace_index);
            let docs = Arc::clone(&self.documents);
            let client = self.client.clone();
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("q-ls: indexing workspace at {}", root.display()),
                )
                .await;
            tokio::spawn(async move {
                match tokio::task::spawn_blocking(move || collect_and_parse_q_files(&root)).await {
                    Ok(pairs) => {
                        let n = pairs.len();
                        {
                            let mut index = idx.write().await;
                            for (uri, doc) in pairs {
                                index.index_file(uri, doc);
                            }
                        }
                        client
                            .log_message(MessageType::INFO, format!("q-ls: indexed {n} .q files"))
                            .await;
                        let open = docs.read().await;
                        let index = idx.read().await;
                        for (uri, doc) in open.iter() {
                            let diags = crate::diagnostics::compute_diagnostics_with_workspace(doc, &index);
                            client.publish_diagnostics(uri.clone(), diags, Some(doc.version())).await;
                        }
                    }
                    Err(e) => {
                        client
                            .log_message(
                                MessageType::ERROR,
                                format!("q-ls: workspace indexing failed: {e}"),
                            )
                            .await;
                    }
                }
            });
        } else {
            self.client
                .log_message(
                    MessageType::INFO,
                    "q-ls: no workspace root from client — will detect from first opened file",
                )
                .await;
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text;
        let version = params.text_document.version;

        // Fallback: client sent no root in InitializeParams. Walk up from the
        // opened file to find .git and index the whole repo from there.
        if self.workspace_root.read().await.is_none() {
            if let Some(file_path) = uri.to_file_path().map(Cow::into_owned) {
                if let Some(git_root) = find_git_root(&file_path) {
                    self.try_start_indexing(git_root).await;
                }
            }
        }

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
        let idx = self.workspace_index.read().await;
        let Some(doc) = docs.get(uri) else { return Ok(None) };
        let items = crate::completion::complete_with_workspace(doc, pos, &idx);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        let idx = self.workspace_index.read().await;
        let Some(doc) = docs.get(uri) else { return Ok(None) };
        Ok(crate::hover::hover_with_workspace(doc, pos, &idx))
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
        let idx = self.workspace_index.read().await;
        let Some(doc) = docs.get(&uri) else { return Ok(None) };
        let locs = crate::references::find_references_with_workspace(
            doc, pos, include_declaration, &uri, &docs, &idx,
        );
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

/// Walk up the directory tree from `path` until a `.git` entry is found.
/// Returns the directory that contains `.git`, or `None` if never found.
fn find_git_root(path: &std::path::Path) -> Option<PathBuf> {
    let mut dir = if path.is_file() {
        path.parent()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
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
