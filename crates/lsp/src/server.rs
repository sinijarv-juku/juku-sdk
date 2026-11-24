//! The protocol: the messages the server takes and sends, as JSON values.

use std::path::{Path, PathBuf};

use juku_json::{Value, obj};

use crate::navigate::{Query, code_span};
use crate::project::{Problem, Project};
use crate::text;
use crate::uri::{self, normal};

/// JSON-RPC's and LSP's error codes.
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const SERVER_NOT_INITIALIZED: i64 = -32002;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// Before `initialize`.
    Starting,
    Running,
    /// After `shutdown`: only `exit` is taken.
    ShutDown,
}

/// A language server for juku-asm's source, as a state machine: it takes
/// the client's messages and returns its own, and the caller moves them.
/// Changes are assembled when [`Server::assemble`] is called, which the
/// caller does once the client has been quiet for a moment.
pub struct Server {
    state: State,
    exit: Option<i32>,
    /// Positions count UTF-8 bytes, else UTF-16 code units.
    utf8: bool,
    /// The client takes document symbols nested.
    nested_symbols: bool,
    /// The setting `inlayHints`: each line's address and cycles.
    inlay_hints: bool,
    project: Project,
}

impl Default for Server {
    fn default() -> Server {
        Server::new()
    }
}

impl Server {
    pub fn new() -> Server {
        Server { state: State::Starting, exit: None, utf8: false, nested_symbols: false, inlay_hints: false, project: Project::default() }
    }

    /// The exit code, once the client has said `exit`: 0 after a
    /// `shutdown`, else 1.
    pub fn exit_code(&self) -> Option<i32> {
        self.exit
    }

    /// Something waits to be assembled.
    pub fn waiting(&self) -> bool {
        self.state == State::Running && self.project.waiting()
    }

    /// Handles a message from the client. Returns the messages to send.
    pub fn handle(&mut self, message: &Value) -> Vec<Value> {
        let Some(method) = message["method"].as_str() else {
            // A response to a request of ours: none needs one.
            return Vec::new();
        };
        let id = &message["id"];
        let params = &message["params"];
        if id.is_null() {
            self.notification(method, params);
            return Vec::new();
        }
        // A question about names is answered from what has been typed.
        let mut out = Vec::new();
        if self.state == State::Running && (method.starts_with("textDocument/") || method.starts_with("workspace/")) && self.waiting() {
            out = self.assemble();
        }
        let answer = match (self.state, method) {
            (State::Starting, "initialize") => Ok(self.initialize(params)),
            (State::Starting, _) => Err((SERVER_NOT_INITIALIZED, "the server is not initialized".to_string())),
            (State::ShutDown, _) => Err((INVALID_REQUEST, "the server has shut down".to_string())),
            (_, "initialize") => Err((INVALID_REQUEST, "the server is initialized already".to_string())),
            (_, "shutdown") => {
                self.state = State::ShutDown;
                Ok(Value::Null)
            }
            (_, "textDocument/definition") => Ok(Query::new(&self.project, self.utf8).definition(params)),
            (_, "textDocument/references") => Ok(Query::new(&self.project, self.utf8).references(params)),
            (_, "textDocument/hover") => Ok(Query::new(&self.project, self.utf8).hover(params)),
            (_, "textDocument/documentSymbol") => Ok(Query::new(&self.project, self.utf8).document_symbols(params, self.nested_symbols)),
            (_, "workspace/symbol") => Ok(Query::new(&self.project, self.utf8).workspace_symbols(params)),
            (_, "juku/mainFiles") => Ok(self.main_files(params)),
            (_, "textDocument/completion") => Ok(Query::new(&self.project, self.utf8).completion(params)),
            (_, "textDocument/signatureHelp") => Ok(Query::new(&self.project, self.utf8).signature_help(params)),
            (_, "textDocument/prepareRename") => Query::new(&self.project, self.utf8).prepare_rename(params),
            (_, "textDocument/rename") => Query::new(&self.project, self.utf8).rename(params),
            (_, "textDocument/foldingRange") => Ok(Query::new(&self.project, self.utf8).folding_ranges(params)),
            (_, "textDocument/inlayHint") if self.inlay_hints => Ok(Query::new(&self.project, self.utf8).inlay_hints(params)),
            (_, "textDocument/inlayHint") => Ok(Vec::<Value>::new().into()),
            _ => Err((METHOD_NOT_FOUND, format!("no method {method}"))),
        };
        out.push(match answer {
            Ok(result) => obj([("jsonrpc", "2.0".into()), ("id", id.clone()), ("result", result)]),
            Err((code, text)) => obj([
                ("jsonrpc", "2.0".into()),
                ("id", id.clone()),
                ("error", obj([("code", code.into()), ("message", text.into())])),
            ]),
        });
        if method == "initialize" && self.state == State::Running {
            out.extend(self.watch(params));
        }
        out
    }

    fn notification(&mut self, method: &str, params: &Value) {
        if method == "exit" {
            self.exit = Some(if self.state == State::ShutDown { 0 } else { 1 });
            return;
        }
        if self.state != State::Running {
            return;
        }
        match method {
            "textDocument/didOpen" => {
                let doc = &params["textDocument"];
                if let (Some(uri), Some(text)) = (doc["uri"].as_str(), doc["text"].as_str())
                    && let Some(path) = uri::to_path(uri)
                {
                    self.project.set_text(path, uri, text.to_string());
                }
            }
            "textDocument/didChange" => {
                // The whole text, as the server asks for: the last change.
                let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
                let text = params["contentChanges"].as_array().iter().rev().find(|c| c["range"].is_null()).and_then(|c| c["text"].as_str());
                if let (Some(path), Some(text)) = (uri::to_path(uri), text) {
                    self.project.set_text(path, uri, text.to_string());
                }
            }
            "textDocument/didClose" => {
                if let Some(path) = params["textDocument"]["uri"].as_str().and_then(uri::to_path) {
                    self.project.close(&path);
                }
            }
            "workspace/didChangeWatchedFiles" => {
                for change in params["changes"].as_array() {
                    if let Some(path) = change["uri"].as_str().and_then(uri::to_path) {
                        self.project.changed(&path);
                    }
                }
            }
            "workspace/didChangeConfiguration" => {
                let settings = &params["settings"]["juku"];
                if !settings.is_null() {
                    self.configure(settings);
                    self.project.scan();
                    self.project.changed_all();
                }
            }
            _ => {}
        }
    }

    fn initialize(&mut self, params: &Value) -> Value {
        let capabilities = &params["capabilities"];
        self.utf8 = capabilities["general"]["positionEncodings"].as_array().iter().any(|e| e.as_str() == Some("utf-8"));
        self.nested_symbols = capabilities["textDocument"]["documentSymbol"]["hierarchicalDocumentSymbolSupport"].as_bool() == Some(true);
        self.project.headers = crate::names::write_headers();
        let mut folders: Vec<PathBuf> = params["workspaceFolders"].as_array().iter().filter_map(|f| f["uri"].as_str().and_then(uri::to_path)).collect();
        if folders.is_empty() {
            let root = params["rootUri"].as_str().and_then(uri::to_path);
            let path = params["rootPath"].as_str().map(|p| normal(Path::new(p))).filter(|p| p.is_absolute());
            folders.extend(root.or(path));
        }
        self.project.folders = folders;
        self.configure(&params["initializationOptions"]);
        self.project.scan();
        self.state = State::Running;
        let encoding = if self.utf8 { "utf-8" } else { "utf-16" };
        obj([
            (
                "capabilities",
                obj([
                    ("positionEncoding", encoding.into()),
                    ("textDocumentSync", obj([("openClose", true.into()), ("change", 1.into())])),
                    ("definitionProvider", true.into()),
                    ("referencesProvider", true.into()),
                    ("hoverProvider", true.into()),
                    ("documentSymbolProvider", true.into()),
                    ("workspaceSymbolProvider", true.into()),
                    ("completionProvider", obj([("triggerCharacters", vec![Value::from(".")].into())])),
                    ("signatureHelpProvider", obj([("triggerCharacters", vec![Value::from(" "), Value::from(",")].into())])),
                    ("renameProvider", obj([("prepareProvider", true.into())])),
                    ("foldingRangeProvider", true.into()),
                    ("inlayHintProvider", true.into()),
                ]),
            ),
            ("serverInfo", obj([("name", "juku-lsp".into()), ("version", env!("CARGO_PKG_VERSION").into())])),
        ])
    }

    /// `juku/mainFiles`, the SDK's own request: the main files whose
    /// programs have the document in them, as URIs; what to assemble to
    /// build it.
    fn main_files(&self, params: &Value) -> Value {
        let Some(path) = params["textDocument"]["uri"].as_str().and_then(uri::to_path) else { return Value::Null };
        self.project.mains_of(&path).iter().map(|m| Value::from(uri::from_path(m))).collect::<Vec<_>>().into()
    }

    /// Takes the settings `mainFiles` and `includePaths`, each a list of
    /// paths, absolute or from the first workspace folder.
    fn configure(&mut self, settings: &Value) {
        let base = self.project.folders.first().cloned().unwrap_or_default();
        let paths = |key: &str| -> Vec<PathBuf> {
            settings[key].as_array().iter().filter_map(Value::as_str).map(|p| normal(&base.join(p))).filter(|p| p.is_absolute()).collect()
        };
        self.project.main_files = paths("mainFiles");
        self.inlay_hints = settings["inlayHints"].as_bool() == Some(true);
        self.project.include = paths("includePaths");
    }

    /// The request that has the client tell the server of changes to source
    /// files on disk, if it can be asked.
    fn watch(&self, params: &Value) -> Option<Value> {
        let can = params["capabilities"]["workspace"]["didChangeWatchedFiles"]["dynamicRegistration"].as_bool() == Some(true);
        can.then(|| {
            let watcher = obj([("globPattern", "**/*.{asm,inc}".into())]);
            let registration = obj([
                ("id", "juku-source-files".into()),
                ("method", "workspace/didChangeWatchedFiles".into()),
                ("registerOptions", obj([("watchers", vec![watcher].into())])),
            ]);
            obj([
                ("jsonrpc", "2.0".into()),
                ("id", "juku-watch".into()),
                ("method", "client/registerCapability".into()),
                ("params", obj([("registrations", vec![registration].into())])),
            ])
        })
    }

    /// Assembles what has changed. Returns the diagnostics to send.
    pub fn assemble(&mut self) -> Vec<Value> {
        if self.state != State::Running {
            return Vec::new();
        }
        let changed = self.project.assemble();
        changed.into_iter().map(|(path, problems)| self.diagnostics(&path, &problems)).collect()
    }

    /// The notification of `path`'s problems.
    fn diagnostics(&self, path: &Path, problems: &[Problem]) -> Value {
        let uri = self.project.open.get(path).map_or_else(|| uri::from_path(path), |d| d.uri.clone());
        let text = if problems.is_empty() { None } else { self.project.text(path) };
        let lines: Vec<&str> = text.as_deref().map(text::split_lines).unwrap_or_default();
        let list: Vec<Value> = problems
            .iter()
            .map(|p| {
                let line = lines.get(p.line).copied().unwrap_or("");
                let (start, end) = code_span(line);
                let at = |byte: usize| obj([("line", p.line.into()), ("character", text::units(&line[..byte], self.utf8).into())]);
                obj([
                    ("range", obj([("start", at(start)), ("end", at(end))])),
                    ("severity", 1.into()),
                    ("source", "juku-asm".into()),
                    ("message", p.message.as_str().into()),
                ])
            })
            .collect();
        obj([
            ("jsonrpc", "2.0".into()),
            ("method", "textDocument/publishDiagnostics".into()),
            ("params", obj([("uri", uri.into()), ("diagnostics", list.into())])),
        ])
    }
}
