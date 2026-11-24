//! The command: a session over stdin and stdout.

use std::io::Write;
use std::process::{Command, Stdio};

use juku_json::Value;
use juku_json::frame::{take_message, write_message};

const LSP: &str = env!("CARGO_BIN_EXE_juku-lsp");

#[test]
fn version() {
    let out = Command::new(LSP).arg("--version").output().unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), format!("juku-lsp {}\n", env!("CARGO_PKG_VERSION")));
    assert!(!Command::new(LSP).arg("--nonsense").output().unwrap().status.success());
}

/// A session: errors come once the client has been quiet, and the exit
/// after a shutdown is 0.
#[test]
fn session() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp-cli");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.asm"), "        nop\n").unwrap();
    let uri = juku_lsp::uri::from_path(&juku_lsp::uri::normal(&dir.join("p.asm")));
    let root = juku_lsp::uri::from_path(&juku_lsp::uri::normal(&dir));
    let text = Value::from("        mvi a, 300\n").to_json();
    let messages = [
        format!(r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"rootUri":"{root}","capabilities":{{}}}}}}"#),
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#.to_string(),
        format!(r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{uri}","languageId":"juku-asm","version":1,"text":{text}}}}}}}"#),
    ];
    let mut child = Command::new(LSP).arg("--stdio").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut input = Vec::new();
    for m in &messages {
        write_message(&mut input, m);
    }
    stdin.write_all(&input).unwrap();
    stdin.flush().unwrap();
    // Quiet for longer than the server waits.
    std::thread::sleep(std::time::Duration::from_millis(600));
    let mut input = Vec::new();
    write_message(&mut input, r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#);
    write_message(&mut input, r#"{"jsonrpc":"2.0","method":"exit"}"#);
    stdin.write_all(&input).unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));

    let mut bytes = out.stdout;
    let mut got = Vec::new();
    while let Some(body) = take_message(&mut bytes, 1 << 20).unwrap() {
        got.push(Value::parse(&body).unwrap());
    }
    assert_eq!(got[0]["result"]["serverInfo"]["name"].as_str(), Some("juku-lsp"));
    let shown = got.iter().find(|m| m["method"].as_str() == Some("textDocument/publishDiagnostics")).expect("diagnostics");
    assert_eq!(shown["params"]["uri"].as_str(), Some(uri.as_str()));
    assert_eq!(shown["params"]["diagnostics"].as_array()[0]["message"].as_str(), Some("300 does not fit in a byte"));
    assert_eq!(got.last().unwrap()["id"].as_i64(), Some(2));
}
