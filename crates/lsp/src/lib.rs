//! A language server for Juku programs in juku-asm's 8080 assembler: the
//! Language Server Protocol, over stdin and stdout as the command
//! `juku-lsp` runs it (docs/lsp.md).
//!
//! [`Server`] is the protocol as a state machine, with no input or output
//! of its own: it takes each message from the client as a JSON value and
//! returns the messages to send, and assembles the workspace when told to,
//! once the client has been quiet for a moment. The command moves the
//! messages and keeps the time.
//!
//! The errors shown are juku-asm's, the first in the source of each main
//! file, as the command reports them (`project.rs` says what a main file
//! is). Assembling takes the editor's unsaved text. Its index of names
//! (`names.rs`) answers the questions about them (`navigate.rs`) and helps
//! write source (`edit.rs`).

mod edit;
mod names;
mod navigate;
mod project;
mod server;
mod text;
pub mod uri;

pub use server::Server;
