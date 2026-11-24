//! juku-lsp: the language server for juku-asm's source, on stdin and
//! stdout.

use std::io::{self, Read, Write};
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use juku_json::Value;
use juku_json::frame::{take_message, write_message};
use juku_lsp::Server;

const USAGE: &str = "\
Usage: juku-lsp [--stdio]
       juku-lsp --version

The language server for Juku programs in juku-asm's 8080 assembler: an
editor starts it and speaks the Language Server Protocol to it on stdin
and stdout. It shows juku-asm's errors as the source is typed.";

/// How long the client must be quiet before a change is assembled.
const QUIET: Duration = Duration::from_millis(200);
/// Largest message taken: a document's whole text.
const MAX_BODY: usize = 64 << 20;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] | ["--stdio"] => {}
        ["--version" | "-V"] => {
            println!("juku-lsp {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        ["--help" | "-h"] => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => {
            eprintln!("juku-lsp: unknown arguments\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut input = Vec::new();
        let mut chunk = vec![0; 1 << 16];
        let mut stdin = io::stdin().lock();
        loop {
            loop {
                match take_message(&mut input, MAX_BODY) {
                    Ok(Some(body)) => match Value::parse(&body) {
                        Ok(message) => {
                            if tx.send(message).is_err() {
                                return;
                            }
                        }
                        Err(e) => eprintln!("juku-lsp: {e}"),
                    },
                    Ok(None) => break,
                    Err(_) => {
                        eprintln!("juku-lsp: a message that cannot be read");
                        return;
                    }
                }
            }
            match stdin.read(&mut chunk) {
                Ok(0) | Err(_) => return,
                Ok(n) => input.extend_from_slice(&chunk[..n]),
            }
        }
    });

    let mut server = Server::new();
    let mut stdout = io::stdout().lock();
    loop {
        let out = if server.waiting() {
            match rx.recv_timeout(QUIET) {
                Ok(message) => server.handle(&message),
                Err(RecvTimeoutError::Timeout) => server.assemble(),
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(message) => server.handle(&message),
                Err(_) => break,
            }
        };
        let mut bytes = Vec::new();
        for message in out {
            write_message(&mut bytes, &message.to_json());
        }
        if stdout.write_all(&bytes).and_then(|()| stdout.flush()).is_err() {
            break;
        }
        if let Some(code) = server.exit_code() {
            return ExitCode::from(code as u8);
        }
    }
    // The input ended without an exit.
    ExitCode::FAILURE
}
