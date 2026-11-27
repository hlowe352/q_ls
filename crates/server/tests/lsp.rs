//! End-to-end tests: run the `q-ls` binary over stdio and talk LSP to it,
//! the way an editor does. Unit tests cover each feature's logic; these
//! cover the wiring in `backend.rs` (document sync, indexing, file
//! watching, config, root detection) and that every request is routed.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);

/// A temporary git workspace, removed on drop.
struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str, files: &[(&str, &str)]) -> Self {
        let dir = std::env::temp_dir().join(format!("q-ls-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let ws = Self(dir.canonicalize().unwrap());
        for (path, text) in files {
            ws.write(path, text);
        }
        ws
    }

    fn write(&self, path: &str, text: &str) {
        std::fs::write(self.0.join(path), text).unwrap();
    }

    fn uri(&self, path: &str) -> String {
        uri(&self.0.join(path))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// A running server plus the client half of the connection.
struct Server {
    child: Child,
    stdin: ChildStdin,
    inbox: Receiver<Value>,
    /// Notifications that arrived while waiting for something else.
    notes: Vec<Value>,
    next_id: i64,
}

impl Server {
    /// Start the server and complete the `initialize` handshake.
    fn start(root: Option<&Path>) -> (Self, Value) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_q-ls"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, inbox) = channel();
        std::thread::spawn(move || {
            loop {
                let mut len = 0;
                loop {
                    let mut line = String::new();
                    if stdout.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    match line.trim_end() {
                        "" => break,
                        h => {
                            if let Some(n) = h.strip_prefix("Content-Length: ") {
                                len = n.parse().unwrap();
                            }
                        }
                    }
                }
                let mut body = vec![0; len];
                stdout.read_exact(&mut body).unwrap();
                if tx.send(serde_json::from_slice(&body).unwrap()).is_err() {
                    return;
                }
            }
        });
        let stdin = child.stdin.take().unwrap();
        let mut server = Self {
            child,
            stdin,
            inbox,
            notes: Vec::new(),
            next_id: 0,
        };
        let root_uri = root.map_or(Value::Null, |r| json!(uri(r)));
        let init = server.request(
            "initialize",
            json!({ "processId": null, "rootUri": root_uri, "capabilities": {} }),
        );
        server.notify("initialized", json!({}));
        (server, init)
    }

    fn write(&mut self, msg: &Value) -> std::io::Result<()> {
        let body = msg.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len())?;
        self.stdin.flush()
    }

    fn send(&mut self, msg: &Value) {
        self.write(msg).unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        let mut msg = json!({ "jsonrpc": "2.0", "method": method });
        msg["params"] = params;
        self.send(&msg);
    }

    /// Next message from the server, answering its own requests (e.g.
    /// `client/registerCapability`) and stashing notifications.
    fn pump(&mut self) -> Value {
        let msg = self.inbox.recv_timeout(TIMEOUT).expect("server went quiet");
        if msg.get("method").is_some() {
            if let Some(id) = msg.get("id") {
                let reply = json!({ "jsonrpc": "2.0", "id": id, "result": null });
                self.send(&reply);
            }
            self.notes.push(msg.clone());
        }
        msg
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let mut msg = json!({ "jsonrpc": "2.0", "id": id, "method": method });
        msg["params"] = params;
        self.send(&msg);
        loop {
            let msg = self.pump();
            if msg.get("method").is_none() && msg["id"] == id {
                assert!(msg.get("error").is_none(), "{method} failed: {msg}");
                return msg["result"].clone();
            }
        }
    }

    /// Wait for (and consume) the first notification matching `pred`.
    fn wait_for(&mut self, method: &str, pred: impl Fn(&Value) -> bool) -> Value {
        loop {
            if let Some(i) = self
                .notes
                .iter()
                .position(|n| n["method"] == method && pred(&n["params"]))
            {
                return self.notes.remove(i)["params"].clone();
            }
            self.pump();
        }
    }

    fn wait_indexed(&mut self) -> String {
        let log = self.wait_for("window/logMessage", |p| {
            p["message"].as_str().is_some_and(|m| m.contains("indexed"))
        });
        log["message"].as_str().unwrap().to_string()
    }

    fn open(&mut self, uri: &str, text: &str) -> Value {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "q", "version": 1, "text": text } }),
        );
        self.diagnostics(uri)
    }

    fn diagnostics(&mut self, uri: &str) -> Value {
        self.wait_for("textDocument/publishDiagnostics", |p| p["uri"] == uri)
    }

    /// `textDocument/<method>` at `(line, character)` in `uri`.
    fn at(&mut self, method: &str, uri: &str, line: u32, character: u32, extra: Value) -> Value {
        let mut params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
        });
        if let (Value::Object(p), Value::Object(e)) = (&mut params, extra) {
            p.extend(e);
        }
        self.request(&format!("textDocument/{method}"), params)
    }

    fn completion_labels(&mut self, uri: &str, line: u32, character: u32) -> Vec<String> {
        let items = self.at("completion", uri, line, character, json!({}));
        items
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["label"].as_str().unwrap().to_string())
            .collect()
    }
}

impl Server {
    /// `shutdown` + `exit`, returning whether the process exited cleanly.
    fn stop(&mut self) -> bool {
        self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        self.child.wait().is_ok_and(|s| s.success())
    }
}

impl Drop for Server {
    /// Ask the server to exit rather than killing it (a killed process never
    /// writes its coverage profile), then reap it.
    fn drop(&mut self) {
        let _ = self.write(&json!({ "jsonrpc": "2.0", "method": "exit" }));
        let _ = self.child.wait();
    }
}

fn messages(diags: &Value) -> Vec<String> {
    diags["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["message"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn initialize_and_index_respect_ignore_files() {
    let ws = Workspace::new(
        "init",
        &[
            ("a.q", "helper:{x+1}"),
            ("b.q", "helper 1"),
            ("skip.q", "skipped:1"),
            (".gitignore", "skip.q\n"),
        ],
    );
    let (mut s, init) = Server::start(Some(&ws.0));
    let caps = &init["capabilities"];
    for cap in [
        "completionProvider",
        "hoverProvider",
        "definitionProvider",
        "referencesProvider",
        "documentSymbolProvider",
        "renameProvider",
        "semanticTokensProvider",
    ] {
        assert!(!caps[cap].is_null(), "missing {cap}: {caps}");
    }
    assert_eq!(s.wait_indexed(), "q-ls: indexed 2 .q files");
    // The `**/*.q` file watcher was registered.
    assert!(
        s.notes
            .iter()
            .any(|n| n["method"] == "client/registerCapability"),
        "no watcher registration: {:?}",
        s.notes
    );
}

#[test]
fn diagnostics_follow_edits() {
    let ws = Workspace::new("diag", &[("lib.q", "helper:{x+1}")]);
    let (mut s, _) = Server::start(Some(&ws.0));
    s.wait_indexed();
    let b = ws.uri("b.q");

    // `helper` resolves cross-file; `ghost` does not.
    let d = s.open(&b, "helper 1\nghost 2");
    assert_eq!(messages(&d), ["unresolved reference `ghost`"]);
    assert_eq!(d["version"], 1);

    // Incremental edit: replace `ghost` (line 1, cols 0..5) with `helper`.
    s.notify(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": b, "version": 2 },
            "contentChanges": [{
                "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 5 } },
                "text": "helper",
            }],
        }),
    );
    let d = s.diagnostics(&b);
    assert_eq!(messages(&d), Vec::<String>::new());
    assert_eq!(d["version"], 2);

    // Whole-document replacement (no range).
    s.notify(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": b, "version": 3 },
            "contentChanges": [{ "text": "nope" }],
        }),
    );
    let d = s.diagnostics(&b);
    assert_eq!(messages(&d), ["unresolved reference `nope`"]);
    assert_eq!(d["version"], 3);
}

#[test]
fn every_request_is_routed() {
    let ws = Workspace::new(
        "routes",
        &[("lib.q", "helper:{[x] x+1}"), ("use.q", "y:helper 1")],
    );
    let (mut s, _) = Server::start(Some(&ws.0));
    s.wait_indexed();
    let (lib, use_) = (ws.uri("lib.q"), ws.uri("use.q"));
    s.open(&use_, "y:helper 1");

    // Cursor on `helper` in use.q (line 0, col 2).
    let def = s.at("definition", &use_, 0, 2, json!({}));
    assert_eq!(def[0]["uri"], lib, "{def}");

    let refs = s.at(
        "references",
        &use_,
        0,
        2,
        json!({ "context": { "includeDeclaration": true } }),
    );
    let uris: Vec<&Value> = refs.as_array().unwrap().iter().map(|l| &l["uri"]).collect();
    assert!(
        uris.contains(&&json!(lib)) && uris.contains(&&json!(use_)),
        "{refs}"
    );

    let prep = s.at("prepareRename", &use_, 0, 2, json!({}));
    assert_eq!(prep["placeholder"], "helper");
    let edit = s.at("rename", &use_, 0, 2, json!({ "newName": "assist" }));
    assert!(
        edit["changes"][&lib].is_array() && edit["changes"][&use_].is_array(),
        "{edit}"
    );

    let hover = s.at("hover", &use_, 0, 2, json!({}));
    assert!(
        hover["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("helper"),
        "{hover}"
    );

    assert!(
        s.completion_labels(&use_, 0, 4)
            .contains(&"helper".to_string())
    );

    let td = json!({ "textDocument": { "uri": use_ } });
    let tokens = s.request("textDocument/semanticTokens/full", td.clone());
    assert_ne!(tokens["data"], json!([]));
    let symbols = s.request("textDocument/documentSymbol", td);
    assert_eq!(symbols[0]["name"], "y");

    // Unknown documents get empty answers, not errors.
    let missing = ws.uri("missing.q");
    assert!(s.at("hover", &missing, 0, 0, json!({})).is_null());

    assert!(s.stop(), "server did not exit cleanly");
}

#[test]
fn watched_file_events_update_index_but_not_open_documents() {
    let ws = Workspace::new("watch", &[("main.q", "x:1")]);
    let (mut s, _) = Server::start(Some(&ws.0));
    s.wait_indexed();
    let main = ws.uri("main.q");
    s.open(&main, "x:1\nfresh");
    let change = |s: &mut Server, uri: &str, typ: u8| {
        s.notify(
            "workspace/didChangeWatchedFiles",
            json!({ "changes": [{ "uri": uri, "type": typ }] }),
        );
    };

    // CREATED on disk: its globals become completions.
    ws.write("new.q", "freshName:1");
    change(&mut s, &ws.uri("new.q"), 1);
    assert!(
        s.completion_labels(&main, 1, 5)
            .contains(&"freshName".to_string())
    );

    // DELETED: gone again.
    std::fs::remove_file(ws.0.join("new.q")).unwrap();
    change(&mut s, &ws.uri("new.q"), 3);
    assert!(
        !s.completion_labels(&main, 1, 5)
            .contains(&"freshName".to_string())
    );

    // CHANGED for an open file: the editor's text wins over disk.
    ws.write("main.q", "diskOnly:1");
    change(&mut s, &main, 2);
    let symbols = |s: &mut Server| {
        let r = s.request(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": main } }),
        );
        r[0]["name"].clone()
    };
    assert_eq!(symbols(&mut s), "x");

    // Once closed, disk changes apply again.
    s.notify(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": main } }),
    );
    change(&mut s, &main, 2);
    assert_eq!(symbols(&mut s), "diskOnly");
}

#[test]
fn root_detected_from_first_opened_file_and_config_loaded() {
    let ws = Workspace::new(
        "root",
        &[
            ("lib.q", "helper:1"),
            (".q-ls.json", r#"{"suppress_unresolved": ["dynamicName"]}"#),
        ],
    );
    // No root from the client: the server finds .git from the opened file.
    let (mut s, _) = Server::start(None);
    s.wait_for("window/logMessage", |p| {
        p["message"]
            .as_str()
            .is_some_and(|m| m.contains("will detect from first opened file"))
    });
    let main = ws.uri("main.q");
    s.open(&main, "helper dynamicName ghost");
    assert_eq!(s.wait_indexed(), "q-ls: indexed 1 .q files");
    // Diagnostics are re-published once the index (and config) are ready.
    let d = s.diagnostics(&main);
    assert_eq!(messages(&d), ["unresolved reference `ghost`"]);
}

#[test]
fn file_outside_any_git_repo_warns_and_still_works() {
    let dir = std::env::temp_dir().join(format!("q-ls-e2e-nogit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.canonicalize().unwrap().join("solo.q");
    let (mut s, _) = Server::start(None);
    let d = s.open(&uri(&file), "a:1\nb:a+ghost");
    s.wait_for("window/logMessage", |p| {
        p["message"]
            .as_str()
            .is_some_and(|m| m.contains("no .git root found"))
    });
    assert_eq!(messages(&d), ["unresolved reference `ghost`"]);
    std::fs::remove_dir_all(&dir).unwrap();
}
