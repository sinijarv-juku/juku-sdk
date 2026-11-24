//! The server on workspaces written for each test: the messages a client
//! sends, and what comes back.

use std::path::{Path, PathBuf};

use juku_json::Value;
use juku_lsp::Server;
use juku_lsp::uri::{from_path, normal};

/// A folder of the test's own with `files`, (name, text), in it.
fn workspace(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = normal(&Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp").join(test));
    let _ = std::fs::remove_dir_all(&dir);
    for (name, text) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn json(text: &str) -> Value {
    Value::parse(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// JSON's text for `s`, quoted.
fn quoted(s: &str) -> String {
    Value::from(s).to_json()
}

fn uri(dir: &Path, name: &str) -> String {
    from_path(&normal(&dir.join(name)))
}

/// A server initialized on `dir`, with `capabilities` and
/// `initializationOptions`, and its answer to `initialize`.
fn start(dir: &Path, capabilities: &str, options: &str) -> (Server, Vec<Value>) {
    let mut server = Server::new();
    let root = quoted(&from_path(dir));
    let out = server.handle(&json(&format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":{root},"capabilities":{capabilities},"initializationOptions":{options}}}}}"#
    )));
    assert!(server.handle(&json(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#)).is_empty());
    (server, out)
}

fn notify(server: &mut Server, method: &str, params: &str) {
    let out = server.handle(&json(&format!(r#"{{"jsonrpc":"2.0","method":"{method}","params":{params}}}"#)));
    assert!(out.is_empty(), "a notification has no answer");
}

fn open(server: &mut Server, uri: &str, text: &str) {
    let params = format!(r#"{{"textDocument":{{"uri":{},"languageId":"juku-asm","version":1,"text":{}}}}}"#, quoted(uri), quoted(text));
    notify(server, "textDocument/didOpen", &params);
}

fn change(server: &mut Server, uri: &str, text: &str) {
    let params = format!(r#"{{"textDocument":{{"uri":{},"version":2}},"contentChanges":[{{"text":{}}}]}}"#, quoted(uri), quoted(text));
    notify(server, "textDocument/didChange", &params);
}

/// The diagnostics `out` sends: each file's name, and its problems as
/// (line, first character, end, message).
type Shown = Vec<(String, Vec<(i64, i64, i64, String)>)>;

fn diagnostics(out: &[Value]) -> Shown {
    out.iter()
        .map(|m| {
            assert_eq!(m["method"].as_str(), Some("textDocument/publishDiagnostics"));
            let uri = m["params"]["uri"].as_str().unwrap();
            let name = uri.rsplit('/').next().unwrap().to_string();
            let list = m["params"]["diagnostics"]
                .as_array()
                .iter()
                .map(|d| {
                    assert_eq!((d["severity"].as_i64(), d["source"].as_str()), (Some(1), Some("juku-asm")));
                    let (start, end) = (&d["range"]["start"], &d["range"]["end"]);
                    assert_eq!(start["line"], end["line"]);
                    let n = |v: &Value| v.as_i64().unwrap();
                    (n(&start["line"]), n(&start["character"]), n(&end["character"]), d["message"].as_str().unwrap().to_string())
                })
                .collect();
            (name, list)
        })
        .collect()
}

fn problem(line: i64, start: i64, end: i64, message: &str) -> (i64, i64, i64, String) {
    (line, start, end, message.to_string())
}

#[test]
fn lifecycle() {
    let mut server = Server::new();
    let early = server.handle(&json(r#"{"jsonrpc":"2.0","id":0,"method":"shutdown"}"#));
    assert_eq!(early[0]["error"]["code"].as_i64(), Some(-32002), "not initialized");

    let dir = workspace("lifecycle", &[]);
    let (mut server, out) = start(&dir, r#"{"workspace":{"didChangeWatchedFiles":{"dynamicRegistration":true}}}"#, "null");
    let result = &out[0]["result"];
    assert_eq!(out[0]["id"].as_i64(), Some(1));
    assert_eq!(result["capabilities"]["positionEncoding"].as_str(), Some("utf-16"));
    assert_eq!(result["capabilities"]["textDocumentSync"]["change"].as_i64(), Some(1), "the whole text");
    assert_eq!(result["serverInfo"]["name"].as_str(), Some("juku-lsp"));
    // The client is asked to say when source files change on disk.
    assert_eq!(out[1]["method"].as_str(), Some("client/registerCapability"));
    let watcher = &out[1]["params"]["registrations"].as_array()[0]["registerOptions"]["watchers"].as_array()[0];
    assert_eq!(watcher["globPattern"].as_str(), Some("**/*.{asm,inc}"));
    // A response to it is taken, and needs nothing.
    assert!(server.handle(&json(r#"{"jsonrpc":"2.0","id":"juku-watch","result":null}"#)).is_empty());

    let again = server.handle(&json(r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{}}"#));
    assert_eq!(again[0]["error"]["code"].as_i64(), Some(-32600));
    let unknown = server.handle(&json(r#"{"jsonrpc":"2.0","id":"x","method":"textDocument/unknown","params":{}}"#));
    assert_eq!((unknown[0]["id"].as_str(), unknown[0]["error"]["code"].as_i64()), (Some("x"), Some(-32601)));
    notify(&mut server, "$/cancelRequest", r#"{"id":1}"#);

    let down = server.handle(&json(r#"{"jsonrpc":"2.0","id":3,"method":"shutdown"}"#));
    assert!(down[0]["result"].is_null() && down[0]["error"].is_null() && down[0].to_json().contains(r#""result":null"#));
    let late = server.handle(&json(r#"{"jsonrpc":"2.0","id":4,"method":"shutdown"}"#));
    assert_eq!(late[0]["error"]["code"].as_i64(), Some(-32600));
    assert_eq!(server.exit_code(), None);
    notify(&mut server, "exit", "null");
    assert_eq!(server.exit_code(), Some(0));

    // An exit without a shutdown.
    let (mut server, _) = start(&dir, "{}", "null");
    notify(&mut server, "exit", "null");
    assert_eq!(server.exit_code(), Some(1));
}

/// The errors of the workspace's main files, as soon as it starts, in the
/// file each one is in; the editor's text in place of the disk's.
#[test]
fn errors_as_typed() {
    let dir = workspace(
        "as_typed",
        &[
            ("main.asm", "        org 100h\n        include 'lib/lib.asm'\nmain:   call helper\n        ret\n"),
            ("lib/lib.asm", "helper: mvi a, 1\n        foo  ; no such\n        ret\n"),
        ],
    );
    let (mut server, _) = start(&dir, "{}", "null");
    assert!(server.waiting());
    let shown = diagnostics(&server.assemble());
    assert_eq!(shown, [("lib.asm".to_string(), vec![problem(1, 8, 11, "unknown mnemonic FOO")])]);
    assert!(!server.waiting());
    assert!(server.assemble().is_empty(), "nothing changed");

    // Fixed in the editor: the error goes.
    let lib = uri(&dir, "lib/lib.asm");
    open(&mut server, &lib, "helper: mvi a, 1\n        ret\n");
    assert!(server.waiting());
    assert_eq!(diagnostics(&server.assemble()), [("lib.asm".to_string(), vec![])]);

    // An error in the main file, typed and not saved.
    let main = uri(&dir, "main.asm");
    open(&mut server, &main, "        org 100h\n        include 'lib/lib.asm'\nmain:   call helper\n");
    assert!(server.assemble().is_empty());
    change(&mut server, &main, "        org 100h\n        include 'lib/lib.asm'\nmain:   call helpr\n");
    assert_eq!(diagnostics(&server.assemble()), [("main.asm".to_string(), vec![problem(2, 0, 18, "undefined symbol helpr")])]);
    let out = server.assemble();
    assert!(out.is_empty());
    // Closed unsaved: the disk's text again, and lib.asm's error on disk.
    notify(&mut server, "textDocument/didClose", &format!(r#"{{"textDocument":{{"uri":{}}}}}"#, quoted(&main)));
    assert_eq!(diagnostics(&server.assemble()), [("main.asm".to_string(), vec![])]);
    notify(&mut server, "textDocument/didClose", &format!(r#"{{"textDocument":{{"uri":{}}}}}"#, quoted(&lib)));
    assert_eq!(diagnostics(&server.assemble()), [("lib.asm".to_string(), vec![problem(1, 8, 11, "unknown mnemonic FOO")])]);

    // Changed on disk, as the client's watcher says.
    std::fs::write(dir.join("lib/lib.asm"), "helper: ret\n").unwrap();
    notify(&mut server, "workspace/didChangeWatchedFiles", &format!(r#"{{"changes":[{{"uri":{},"type":2}}]}}"#, quoted(&lib)));
    assert_eq!(diagnostics(&server.assemble()), [("lib.asm".to_string(), vec![])]);
}

/// Where errors are shown that are not on a line of their own file.
#[test]
fn error_places() {
    let dir = workspace(
        "places",
        &[
            ("macro.asm", "twice   macro what\n        what\n        what\n        endm\n        nop\n        twice foo\n"),
            ("empty.asm", "; nothing yet\n"),
            ("names.inc", "ROWS    equ 4\n"),
        ],
    );
    let (mut server, _) = start(&dir, "{}", "null");
    let shown = diagnostics(&server.assemble());
    let name = |n: &str| shown.iter().find(|(f, _)| f == n).map(|(_, p)| p.clone());
    let macro_error = name("macro.asm").unwrap();
    assert_eq!((macro_error[0].0, macro_error[0].1), (5, 8), "at the call");
    assert!(macro_error[0].3.starts_with("in twice: ") && macro_error[0].3.ends_with("unknown mnemonic FOO\n          foo"), "{:?}", macro_error[0].3);
    assert_eq!(name("empty.asm"), Some(vec![problem(0, 0, 13, "no code")]));
    assert_eq!(name("names.inc"), None, "an include file is not a main file");

    // Opened on its own, an include file is assembled, but has no code to
    // be missing.
    open(&mut server, &uri(&dir, "names.inc"), "ROWS    equ 4\nCOLS    equ\n");
    assert_eq!(diagnostics(&server.assemble()), [("names.inc".to_string(), vec![problem(1, 0, 11, "bad expression")])]);
}

/// The settings name the main files and the include folders.
#[test]
fn settings() {
    let dir = workspace(
        "settings",
        &[
            ("src/a.asm", "        include 'defs.inc'\n        mvi a, COUNT\n"),
            ("src/b.asm", "        bad\n"),
            ("inc/defs.inc", "COUNT   equ 3\n"),
        ],
    );
    let (mut server, _) = start(&dir, "{}", r#"{"mainFiles":["src/a.asm"],"includePaths":["inc"]}"#);
    assert!(server.assemble().is_empty(), "only a.asm, which finds defs.inc in inc");

    // Without the include folder.
    notify(&mut server, "workspace/didChangeConfiguration", r#"{"settings":{"juku":{"mainFiles":["src/a.asm"]}}}"#);
    let shown = diagnostics(&server.assemble());
    assert_eq!(shown.len(), 1);
    assert_eq!((shown[0].0.as_str(), shown[0].1[0].0), ("a.asm", 0), "{shown:?}");
    // Without either: b.asm is a main file too.
    notify(&mut server, "workspace/didChangeConfiguration", r#"{"settings":{"juku":{}}}"#);
    let shown = diagnostics(&server.assemble());
    let names: Vec<&str> = shown.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(names, ["b.asm"], "a.asm's error stays, and is not sent again");
}

/// Positions count UTF-16 code units, or UTF-8 bytes when the client can
/// take them.
#[test]
fn position_units() {
    let source = "        db 'äö', x   ; ü\n";
    let dir = workspace("units", &[("p.asm", source)]);
    let (mut server, _) = start(&dir, "{}", "null");
    assert_eq!(diagnostics(&server.assemble())[0].1, [problem(0, 8, 18, "undefined symbol x")]);
    let (mut server, out) = start(&dir, r#"{"general":{"positionEncodings":["utf-8","utf-16"]}}"#, "null");
    assert_eq!(out[0]["result"]["capabilities"]["positionEncoding"].as_str(), Some("utf-8"));
    assert_eq!(diagnostics(&server.assemble())[0].1, [problem(0, 8, 20, "undefined symbol x")]);
}

/// The answer to request `method` with `params`, and the messages sent
/// before it.
fn ask(server: &mut Server, method: &str, params: &str) -> (Value, Vec<Value>) {
    let mut out = server.handle(&json(&format!(r#"{{"jsonrpc":"2.0","id":7,"method":"{method}","params":{params}}}"#)));
    let answer = out.pop().unwrap();
    assert_eq!(answer["id"].as_i64(), Some(7));
    assert!(answer["error"].is_null(), "{}", answer.to_json());
    (answer["result"].clone(), out)
}

/// The params of a request at `line` and `character` of `uri`, with
/// `more` members.
fn at(uri: &str, line: usize, character: usize, more: &str) -> String {
    format!(r#"{{"textDocument":{{"uri":{}}},"position":{{"line":{line},"character":{character}}}{more}}}"#, quoted(uri))
}

/// A location's file name, line and characters.
type Place = (String, i64, i64, i64);

fn place(location: &Value) -> Place {
    let uri = location["uri"].as_str().unwrap();
    let r = &location["range"];
    let n = |v: &Value| v.as_i64().unwrap();
    (uri.rsplit('/').next().unwrap().to_string(), n(&r["start"]["line"]), n(&r["start"]["character"]), n(&r["end"]["character"]))
}

fn places(result: &Value) -> Vec<Place> {
    result.as_array().iter().map(place).collect()
}

fn p(file: &str, line: i64, start: i64, end: i64) -> Place {
    (file.to_string(), line, start, end)
}

const NAMES_MAIN: &str = "        include 'rombios.inc' as rb
        include 'lib.asm' as lib
COUNT   equ 3                   ; how many
; Prints COUNT stars.
; Then returns.
stars:  mvi b, COUNT
.loop:  mvi a, '*'
        call rb.tto
        dcr b
        jnz .loop
        call lib.helper
        ret
point   struct
x       db 0
y       db 0
        ends
here    point
";

fn names_workspace(test: &str) -> (PathBuf, Server) {
    let dir = workspace(test, &[("main.asm", NAMES_MAIN), ("lib.asm", "helper: ret\n")]);
    let (mut server, _) = start(&dir, r#"{"textDocument":{"documentSymbol":{"hierarchicalDocumentSymbolSupport":true}}}"#, "null");
    assert!(server.assemble().is_empty(), "no errors");
    (dir, server)
}

#[test]
fn definitions() {
    let (dir, mut server) = names_workspace("definitions");
    let main = uri(&dir, "main.asm");
    let mut def = |line, character| ask(&mut server, "textDocument/definition", &at(&main, line, character, "")).0;
    assert_eq!(places(&def(5, 17)), [p("main.asm", 2, 0, 5)], "COUNT");
    assert_eq!(places(&def(9, 14)), [p("main.asm", 6, 0, 5)], ".loop is stars.loop");
    assert_eq!(places(&def(10, 20)), [p("lib.asm", 0, 0, 6)], "lib.helper");
    assert_eq!(places(&def(2, 1)), [p("main.asm", 2, 0, 5)], "a definition is its own");
    assert_eq!(places(&def(16, 9)), [p("main.asm", 12, 0, 5)], "a struct");
    assert!(def(6, 8).is_null(), "mvi is no name");
    assert!(def(3, 10).is_null(), "a comment");
    // A built-in include file, written out to be opened.
    let tto = def(7, 17).as_array()[0].clone();
    assert_eq!(place(&tto), p("rombios.inc", 19, 0, 3));
    let path = juku_lsp::uri::to_path(tto["uri"].as_str().unwrap()).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), juku_asm::header("rombios.inc").unwrap());
}

#[test]
fn references() {
    let (dir, mut server) = names_workspace("references");
    let main = uri(&dir, "main.asm");
    let mut refs = |line, character, declaration: bool| {
        let context = format!(r#","context":{{"includeDeclaration":{declaration}}}"#);
        places(&ask(&mut server, "textDocument/references", &at(&main, line, character, &context)).0)
    };
    assert_eq!(refs(2, 0, true), [p("main.asm", 2, 0, 5), p("main.asm", 5, 15, 20)], "not in the comment");
    assert_eq!(refs(6, 2, false), [p("main.asm", 9, 12, 17)]);
    assert_eq!(refs(16, 1, false), [], "here: no uses");
}

#[test]
fn hovers() {
    let (dir, mut server) = names_workspace("hovers");
    let main = uri(&dir, "main.asm");
    let mut hover = |line, character| {
        let (result, _) = ask(&mut server, "textDocument/hover", &at(&main, line, character, ""));
        if result.is_null() {
            return None;
        }
        assert_eq!(result["contents"]["kind"].as_str(), Some("markdown"));
        Some(result["contents"]["value"].as_str().unwrap().to_string())
    };
    assert_eq!(hover(5, 16).unwrap(), "`COUNT` = 3h (3)\n\n```juku-asm\nCOUNT   equ 3                   ; how many\n```");
    assert_eq!(hover(5, 2).unwrap(), "`stars` label at 0001h\n\n```juku-asm\nstars:  mvi b, COUNT\n```\n\nPrints COUNT stars.\nThen returns.");
    assert!(hover(7, 15).unwrap().starts_with("`rb.tto` = 0ffd9h (65497)\n\n```juku-asm\ntto       equ 0ffd9h"));
    assert_eq!(hover(5, 9).unwrap(), "`mvi b, COUNT`: opcode 06h, 2 bytes, 7 cycles");
    assert_eq!(hover(13, 11).unwrap(), "`0` = 0 = 0h = 0b");
    assert_eq!(hover(12, 2).unwrap(), "`point` struct of 2 bytes\n\n```juku-asm\npoint   struct\n```");
    assert!(hover(16, 0).unwrap().starts_with("`here` label at "));
    assert_eq!(hover(5, 12), None, "a register");
}

#[test]
fn symbols() {
    let (dir, mut server) = names_workspace("symbols");
    fn tree(list: &Value) -> Vec<String> {
        list.as_array()
            .iter()
            .map(|s| {
                let children = tree(&s["children"]);
                let name = s["name"].as_str().unwrap().to_string();
                if children.is_empty() { name } else { format!("{name} [{}]", children.join(", ")) }
            })
            .collect()
    }
    let mut symbols = |file| ask(&mut server, "textDocument/documentSymbol", &format!(r#"{{"textDocument":{{"uri":{}}}}}"#, quoted(&uri(&dir, file)))).0;
    let main = symbols("main.asm");
    assert_eq!(tree(&main), ["COUNT", "stars [.loop]", "point [x, y]", "here [.x, .y]"]);
    let count = &main.as_array()[0];
    assert_eq!((count["kind"].as_i64(), count["detail"].as_str()), (Some(14), Some("= 3h (3)")));
    // A file included under a namespace has its names as it writes them.
    assert_eq!(tree(&symbols("lib.asm")), ["helper"]);

    let mut search = |query: &str| ask(&mut server, "workspace/symbol", &format!(r#"{{"query":{}}}"#, quoted(query))).0;
    let found: Vec<(String, Place)> = search("LOOP").as_array().iter().map(|s| (s["name"].as_str().unwrap().to_string(), place(&s["location"]))).collect();
    assert_eq!(found, [("stars.loop".to_string(), p("main.asm", 6, 0, 5))]);
    assert_eq!(search("helper").as_array()[0]["name"].as_str(), Some("lib.helper"));
}

/// A client that takes symbols only as a list.
#[test]
fn flat_symbols() {
    let dir = workspace("flat_symbols", &[("main.asm", NAMES_MAIN), ("lib.asm", "helper: ret\n")]);
    let (mut server, _) = start(&dir, "{}", "null");
    let (result, _) = ask(&mut server, "textDocument/documentSymbol", &format!(r#"{{"textDocument":{{"uri":{}}}}}"#, quoted(&uri(&dir, "main.asm"))));
    let list: Vec<(&str, Option<&str>)> = result.as_array().iter().map(|s| (s["name"].as_str().unwrap(), s["containerName"].as_str())).collect();
    assert_eq!(list[1..3], [("stars", None), ("stars.loop", Some("stars"))]);
    assert!(result.as_array()[0]["location"]["uri"].as_str().unwrap().ends_with("main.asm"));
}

/// A question is answered from what has been typed: it is assembled
/// first. And with an error, the names below it are still there.
#[test]
fn typed_names() {
    let (dir, mut server) = names_workspace("typed_names");
    let main = uri(&dir, "main.asm");
    open(&mut server, &main, &NAMES_MAIN.replace("        ret\n", "        bad\n        ret\nlater:  jmp stars\n"));
    let (result, sent) = ask(&mut server, "textDocument/definition", &at(&main, 13, 14, ""));
    assert_eq!(diagnostics(&sent), [("main.asm".to_string(), vec![problem(11, 8, 11, "unknown mnemonic BAD")])]);
    assert_eq!(places(&result), [p("main.asm", 5, 0, 5)], "stars, from below the error");
}

const EDIT_MAIN: &str = "        include 'rombios.inc' as rb
        include 'lib.asm' as lib
twice   macro what, count
        call what
        endm
COUNT   equ 3
        org 100h
stars:  mvi b, COUNT
.loop:  call rb.tto
        dcr b
        jnz .loop
        twice stars, 2
        jmp stars.loop
        if COUNT > 2
        nop
        else
        hlt
        endif
point   struct
x       db 0
        ends
; one
; two
data:   db 1, 2, 3
";

fn edit_workspace(test: &str, options: &str) -> (PathBuf, Server, String) {
    let dir = workspace(test, &[("main.asm", EDIT_MAIN), ("lib.asm", "helper: ret\n")]);
    let (mut server, _) = start(&dir, "{}", options);
    assert!(server.assemble().is_empty(), "no errors");
    let main = uri(&dir, "main.asm");
    (dir, server, main)
}

/// The labels of a completion's items.
fn labels(result: &Value) -> Vec<String> {
    result.as_array().iter().map(|i| i["label"].as_str().unwrap().to_string()).collect()
}

#[test]
fn completion() {
    let (_, mut server, main) = edit_workspace("completion", "null");
    let mut complete = |line, character| ask(&mut server, "textDocument/completion", &at(&main, line, character, "")).0;

    // A line's head: mnemonics, directives, macros.
    let head = complete(9, 9);
    let items = labels(&head);
    for word in ["dcr", "db", "org", "twice", "mvi"] {
        assert!(items.iter().any(|i| i == word), "{word}: {items:?}");
    }
    assert!(!items.iter().any(|i| i == "COUNT") && !items.iter().any(|i| i == "a"));
    let dcr = head.as_array().iter().find(|i| i["label"].as_str() == Some("dcr")).unwrap();
    assert_eq!((dcr["detail"].as_str(), dcr["kind"].as_i64()), (Some("r"), Some(14)));
    let edit = &dcr["textEdit"]["range"];
    assert_eq!((edit["start"]["character"].as_i64(), edit["end"]["character"].as_i64()), (Some(8), Some(9)), "the word typed");

    // Operands: the names written so here, registers, functions,
    // namespaces; not local labels, fields or macros.
    let items = labels(&complete(7, 15));
    for word in ["COUNT", "stars", "data", "point", "a", "high", "rb", "lib"] {
        assert!(items.iter().any(|i| i == word), "{word}: {items:?}");
    }
    for word in ["twice", "stars.loop", "x", "point.x", "dcr"] {
        assert!(!items.iter().any(|i| i == word), "{word}: {items:?}");
    }
    // A local label: the current label's.
    assert_eq!(labels(&complete(10, 17)), [".loop"]);
    // Under a namespace.
    let items = labels(&complete(8, 17));
    assert!(items.iter().any(|i| i == "rb.tto") && items.iter().all(|i| i.starts_with("rb.")), "{items:?}");
    assert!(labels(&complete(21, 4)).is_empty(), "in a comment");
}

#[test]
fn completion_of_files() {
    let (dir, mut server, main) = edit_workspace("completion_files", "null");
    std::fs::create_dir_all(dir.join("gfx")).unwrap();
    std::fs::write(dir.join("gfx/font.bin"), [0u8; 8]).unwrap();
    open(&mut server, &main, &EDIT_MAIN.replace("        org 100h\n", "        include 'r\n        incbin 'g\n        org 100h\n"));
    let items = labels(&ask(&mut server, "textDocument/completion", &at(&main, 6, 18, "")).0);
    assert!(items.iter().any(|i| i == "rombios.inc") && items.iter().any(|i| i == "lib.asm"), "{items:?}");
    assert!(!items.iter().any(|i| i == "main.asm") && !items.iter().any(|i| i == "gfx/font.bin"), "{items:?}");
    let items = labels(&ask(&mut server, "textDocument/completion", &at(&main, 7, 17, "")).0);
    assert_eq!(items, ["gfx/font.bin"]);
}

#[test]
fn signatures() {
    let (_, mut server, main) = edit_workspace("signatures", "null");
    let mut help = |line, character| {
        let r = ask(&mut server, "textDocument/signatureHelp", &at(&main, line, character, "")).0;
        if r.is_null() {
            return None;
        }
        let s = &r["signatures"].as_array()[0];
        Some((s["label"].as_str().unwrap().to_string(), r["activeParameter"].as_i64().unwrap()))
    };
    assert_eq!(help(11, 14), Some(("twice what, count".to_string(), 0)));
    assert_eq!(help(11, 21), Some(("twice what, count".to_string(), 1)));
    assert_eq!(help(7, 13), Some(("mvi r, byte".to_string(), 0)));
    assert_eq!(help(7, 16), Some(("mvi r, byte".to_string(), 1)));
    assert_eq!(help(9, 9), None, "on the mnemonic");
    assert_eq!(help(14, 11), None, "nop has none");
}

/// The edits of a rename's answer: each file's name and (line, start,
/// end, new text).
fn renames(result: &Value) -> Vec<(String, i64, i64, i64, String)> {
    let Value::Object(changes) = &result["changes"] else { panic!("{}", result.to_json()) };
    let mut out = Vec::new();
    for (uri, edits) in changes {
        let file = uri.rsplit('/').next().unwrap().to_string();
        for e in edits.as_array() {
            let r = &e["range"];
            let n = |v: &Value| v.as_i64().unwrap();
            out.push((file.clone(), n(&r["start"]["line"]), n(&r["start"]["character"]), n(&r["end"]["character"]), e["newText"].as_str().unwrap().to_string()));
        }
    }
    out
}

#[test]
fn rename() {
    let (_, mut server, main) = edit_workspace("rename", "null");
    let (prepared, _) = ask(&mut server, "textDocument/prepareRename", &at(&main, 12, 20, ""));
    assert_eq!(prepared["placeholder"].as_str(), Some("loop"), "the last part of stars.loop");

    let rename = |server: &mut Server, line, character, new: &str| {
        let params = at(&main, line, character, &format!(r#","newName":{}"#, quoted(new)));
        let mut out = server.handle(&json(&format!(r#"{{"jsonrpc":"2.0","id":8,"method":"textDocument/rename","params":{params}}}"#)));
        out.pop().unwrap()
    };
    let e = |line, start, end, text: &str| ("main.asm".to_string(), line, start, end, text.to_string());
    // A label, its uses, a macro's argument, and its local label written
    // in full.
    let answer = rename(&mut server, 7, 2, "dots");
    assert_eq!(renames(&answer["result"]), [e(7, 0, 5, "dots"), e(11, 14, 19, "dots"), e(12, 12, 17, "dots")]);
    // A local label, however it is written.
    let answer = rename(&mut server, 10, 14, "again");
    assert_eq!(renames(&answer["result"]), [e(8, 1, 5, "again"), e(10, 13, 17, "again"), e(12, 18, 22, "again")]);
    let failure = |answer: Value| (answer["error"]["code"].as_i64(), answer["error"]["message"].as_str().unwrap_or("").to_string());
    let (code, message) = failure(rename(&mut server, 8, 15, "out"));
    assert_eq!((code, message.as_str()), (Some(-32803), "rb.tto is the SDK's: its include files are not changed"));
    assert!(failure(rename(&mut server, 7, 2, "mov")).1.contains("cannot be a name"));
    assert_eq!(failure(rename(&mut server, 7, 2, "COUNT")).1, "COUNT is defined already");
    assert!(failure(rename(&mut server, 19, 0, "y")).1.starts_with("point.x is a field"));
}

#[test]
fn folding() {
    let (_, mut server, main) = edit_workspace("folding", "null");
    let (result, _) = ask(&mut server, "textDocument/foldingRange", &format!(r#"{{"textDocument":{{"uri":{}}}}}"#, quoted(&main)));
    let mut ranges: Vec<(i64, i64, String)> =
        result.as_array().iter().map(|r| (r["startLine"].as_i64().unwrap(), r["endLine"].as_i64().unwrap(), r["kind"].as_str().unwrap().to_string())).collect();
    ranges.sort();
    let r = |s, e, k: &str| (s, e, k.to_string());
    assert_eq!(ranges, [r(2, 3, "region"), r(13, 14, "region"), r(15, 16, "region"), r(18, 19, "region"), r(21, 22, "comment")]);
}

#[test]
fn inlay_hints() {
    let range = r#","range":{"start":{"line":0,"character":0},"end":{"line":30,"character":0}}"#;
    let (_, mut server, main) = edit_workspace("hints_off", "null");
    let params = format!(r#"{{"textDocument":{{"uri":{}}}{range}}}"#, quoted(&main));
    assert!(ask(&mut server, "textDocument/inlayHint", &params).0.as_array().is_empty(), "off unless asked for");

    let (_, mut server, main) = edit_workspace("hints_on", r#"{"inlayHints":true}"#);
    let params = format!(r#"{{"textDocument":{{"uri":{}}}{range}}}"#, quoted(&main));
    let (result, _) = ask(&mut server, "textDocument/inlayHint", &params);
    let hints: Vec<(i64, i64, String)> = result
        .as_array()
        .iter()
        .map(|h| (h["position"]["line"].as_i64().unwrap(), h["position"]["character"].as_i64().unwrap(), h["label"].as_str().unwrap().to_string()))
        .collect();
    assert_eq!(hints[0], (7, 20, "0100h · 7 cycles".to_string()));
    assert!(hints.contains(&(11, 22, "0109h · 17 cycles".to_string())), "a macro's call: its expansion's {hints:?}");
    assert!(hints.contains(&(23, 18, "0110h · 3 bytes".to_string())), "{hints:?}");
    assert!(!hints.iter().any(|h| h.0 == 16), "hlt is not assembled");
}

/// The SDK's own request: the main files to assemble to build a file.
#[test]
fn main_files() {
    let dir = workspace(
        "main_files",
        &[
            ("a.asm", "        include 'lib.asm'\n        nop\n"),
            ("b.asm", "        include 'lib.asm'\n        ret\n"),
            ("lib.asm", "        nop\n"),
            ("alone.asm", "        nop\n"),
            ("names.inc", "X       equ 1\n"),
        ],
    );
    let (mut server, _) = start(&dir, "{}", "null");
    let mut mains = |file: &str| {
        let (result, _) = ask(&mut server, "juku/mainFiles", &format!(r#"{{"textDocument":{{"uri":{}}}}}"#, quoted(&uri(&dir, file))));
        result.as_array().iter().map(|u| u.as_str().unwrap().rsplit('/').next().unwrap().to_string()).collect::<Vec<_>>()
    };
    assert_eq!(mains("lib.asm"), ["a.asm", "b.asm"]);
    assert_eq!(mains("a.asm"), ["a.asm"]);
    assert_eq!(mains("alone.asm"), ["alone.asm"]);
    assert!(mains("names.inc").is_empty(), "an include file no program has");
}
