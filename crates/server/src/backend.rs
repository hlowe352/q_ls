use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer};

use crate::config::Config;
use crate::document::Document;
use crate::workspace_index::WorkspaceIndex;

#[derive(Default)]
struct State {
    /// Every known document. Open documents hold the editor's text and are
    /// never overwritten from disk while open.
    index: WorkspaceIndex,
    open: HashSet<Uri>,
    root: Option<PathBuf>,
    config: Config,
}

impl State {
    fn diagnostics(&self, uri: &Uri) -> Option<(Uri, Vec<Diagnostic>, i32)> {
        let doc = self.index.get(uri)?;
        let diags = crate::diagnostics::compute_diagnostics(doc, &self.index, &self.config);
        Some((uri.clone(), diags, doc.version()))
    }
}

pub struct QLanguageServer {
    client: Client,
    state: Arc<RwLock<State>>,
}

impl QLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::default(),
        }
    }

    async fn publish(client: &Client, batch: Vec<(Uri, Vec<Diagnostic>, i32)>) {
        for (uri, diags, version) in batch {
            client.publish_diagnostics(uri, diags, Some(version)).await;
        }
    }

    async fn publish_one(&self, uri: &Uri) {
        let batch = self.state.read().await.diagnostics(uri);
        Self::publish(&self.client, batch.into_iter().collect()).await;
    }

    /// Run `f` on the document at `uri`, if known.
    async fn with_doc<T>(
        &self,
        uri: &Uri,
        f: impl FnOnce(&Document, &WorkspaceIndex) -> Option<T>,
    ) -> Result<Option<T>> {
        let st = self.state.read().await;
        Ok(st.index.get(uri).and_then(|doc| f(doc, &st.index)))
    }

    /// Load `.q-ls.json` from `root` and index its `.q` files in the
    /// background, then re-publish diagnostics for open documents.
    async fn index_workspace(&self, root: PathBuf) {
        let (cfg, status) = Config::load(&root);
        self.state.write().await.config = cfg;
        self.client.log_message(MessageType::INFO, status).await;
        let msg = format!("q-ls: indexing workspace at {}", root.display());
        self.client.log_message(MessageType::INFO, msg).await;

        let state = Arc::clone(&self.state);
        let client = self.client.clone();
        tokio::spawn(async move {
            let scan = tokio::task::spawn_blocking(move || collect_q_files(&root));
            let files = match scan.await {
                Ok(files) => files,
                Err(e) => {
                    let msg = format!("q-ls: workspace indexing failed: {e}");
                    client.log_message(MessageType::ERROR, msg).await;
                    return;
                }
            };
            let n = files.len();
            let batch: Vec<_> = {
                let mut st = state.write().await;
                for (uri, doc) in files {
                    if !st.open.contains(&uri) {
                        st.index.insert(uri, doc);
                    }
                }
                st.open.iter().filter_map(|u| st.diagnostics(u)).collect()
            };
            let msg = format!("q-ls: indexed {n} .q files");
            client.log_message(MessageType::INFO, msg).await;
            Self::publish(&client, batch).await;
        });
    }
}

impl LanguageServer for QLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        #[allow(deprecated)]
        let client_root = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .map(|f| &f.uri)
            .or(params.root_uri.as_ref())
            .and_then(|u| u.to_file_path().map(std::borrow::Cow::into_owned));
        // Upgrade to the nearest .git root so a client that sends a sub-folder
        // (or Neovim sending the file's parent dir) still gets the full repo.
        self.state.write().await.root = client_root.map(|p| find_git_root(&p).unwrap_or(p));

        let (token_types, token_modifiers) = crate::semantic::legend();
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
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
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types,
                                token_modifiers,
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
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
            // `None`: use the spec-default UTF-16, which `LineIndex` implements.
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "q-ls initialized")
            .await;

        let watchers = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/*.q".to_string()),
                kind: None,
            }],
        };
        let registration = Registration {
            id: "watch-q-files".to_string(),
            method: "workspace/didChangeWatchedFiles".to_string(),
            register_options: serde_json::to_value(watchers).ok(),
        };
        let _ = self.client.register_capability(vec![registration]).await;

        // With no root from the client, did_open detects it from the first
        // opened file instead.
        let root = self.state.read().await.root.clone();
        if let Some(root) = root {
            self.index_workspace(root).await;
        } else {
            self.client
                .log_message(
                    MessageType::INFO,
                    "q-ls: no workspace root from client — will detect from first opened file",
                )
                .await;
        }
    }

    fn shutdown(&self) -> impl Future<Output = Result<()>> + Send {
        std::future::ready(Ok(()))
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        let uri = doc.uri;

        // Fallback root: walk up from the opened file to its .git.
        let needs_root = self.state.read().await.root.is_none();
        if needs_root && let Some(path) = uri.to_file_path() {
            if let Some(git_root) = find_git_root(&path) {
                // Claim atomically so concurrent opens don't double-index.
                let claimed = {
                    let mut st = self.state.write().await;
                    let free = st.root.is_none();
                    if free {
                        st.root = Some(git_root.clone());
                    }
                    free
                };
                if claimed {
                    self.index_workspace(git_root).await;
                }
            } else {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        "q-ls: no .git root found — cross-file features unavailable",
                    )
                    .await;
            }
        }

        {
            let mut st = self.state.write().await;
            st.open.insert(uri.clone());
            st.index
                .insert(uri.clone(), Document::new(doc.text, doc.version));
        }
        self.publish_one(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        {
            let mut st = self.state.write().await;
            let Some(mut doc) = st.index.remove(&uri) else {
                return;
            };
            doc.apply_changes(params.content_changes, params.text_document.version);
            st.index.insert(uri.clone(), doc);
        }
        self.publish_one(&uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // The last editor text stays indexed until the file watcher reports
        // a change on disk.
        self.state
            .write()
            .await
            .open
            .remove(&params.text_document.uri);
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let mut st = self.state.write().await;
        for FileEvent { uri, typ } in params.changes {
            if st.open.contains(&uri) {
                continue;
            }
            if typ == FileChangeType::DELETED {
                st.index.remove(&uri);
            } else if let Some(text) = uri
                .to_file_path()
                .and_then(|p| std::fs::read_to_string(p).ok())
            {
                st.index.insert(uri, Document::new(text, 0));
            }
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let tdp = params.text_document_position;
        self.with_doc(&tdp.text_document.uri, |doc, ws| {
            let items = crate::completion::complete(doc, tdp.position, ws);
            Some(CompletionResponse::Array(items))
        })
        .await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let tdp = params.text_document_position_params;
        self.with_doc(&tdp.text_document.uri, |doc, ws| {
            crate::hover::hover(doc, tdp.position, ws)
        })
        .await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let tdp = params.text_document_position_params;
        let uri = &tdp.text_document.uri;
        self.with_doc(uri, |doc, ws| {
            crate::goto_def::goto_definition(doc, tdp.position, uri, ws)
        })
        .await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let tdp = params.text_document_position;
        let uri = &tdp.text_document.uri;
        let include_decl = params.context.include_declaration;
        self.with_doc(uri, |doc, ws| {
            let locs = crate::references::find_references(doc, tdp.position, include_decl, uri, ws);
            Some(locs)
        })
        .await
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        self.with_doc(&params.text_document.uri, |doc, _| {
            crate::rename::prepare_rename(doc, params.position)
        })
        .await
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let tdp = params.text_document_position;
        let uri = &tdp.text_document.uri;
        self.with_doc(uri, |doc, ws| {
            crate::rename::rename(doc, tdp.position, &params.new_name, uri, ws)
        })
        .await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        self.with_doc(&params.text_document.uri, |doc, _| {
            Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: crate::semantic::semantic_tokens(doc),
            }))
        })
        .await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        self.with_doc(&params.text_document.uri, |doc, _| {
            Some(DocumentSymbolResponse::Nested(
                crate::symbols::document_symbols(doc),
            ))
        })
        .await
    }
}

/// Walk up from `path` to the nearest directory containing `.git`.
fn find_git_root(path: &Path) -> Option<PathBuf> {
    let start = if path.is_file() { path.parent()? } else { path };
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Parse every `.q` file under `root`, keyed by canonical URI. Honours
/// `.gitignore`/`.ignore` files and skips hidden directories, the same way
/// ripgrep does.
fn collect_q_files(root: &Path) -> Vec<(Uri, Document)> {
    ignore::WalkBuilder::new(root)
        .build()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "q"))
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path()).ok()?;
            let uri = Uri::from_file_path(e.path().canonicalize().ok()?)?;
            Some((uri, Document::new(text, 0)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_q_files_honours_ignores_and_finds_git_root() {
        let root = std::env::temp_dir().join(format!("q-ls-backend-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in [
            (".git/x.q", "x:1"),
            (".gitignore", "ignored/\n"),
            (".ignore", "fixtures/\n"),
            ("my dir/a.q", "a:1"),
            ("my dir/notes.txt", "x"),
            ("ignored/b.q", "b:1"),
            ("fixtures/c.q", "c:1"),
            (".hidden/d.q", "d:1"),
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }

        let files = collect_q_files(&root);
        let uris: Vec<&str> = files.iter().map(|(u, _)| u.as_str()).collect();
        assert_eq!(uris.len(), 1, "only my dir/a.q expected: {uris:?}");
        assert!(
            uris[0].ends_with("my%20dir/a.q"),
            "spaces percent-encoded: {uris:?}"
        );

        assert_eq!(find_git_root(&root.join("my dir/a.q")), Some(root.clone()));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
