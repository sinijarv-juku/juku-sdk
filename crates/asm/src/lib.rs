//! An Intel 8080 assembler for Juku programs.
//!
//! Syntax, as in Intel's assembler:
//!
//! ```text
//! label:  MNEMONIC operands   ; comment
//! name    EQU expression
//!         ORG expression      (the first ORG sets the load address)
//!         DB  expression or 'string', ...
//!         DW  expression, ...
//!         DS  count[, fill]   (reserve bytes, filled with 00h or fill)
//!         ALIGN n             (pad with 00h up to a multiple of n)
//!         REPT count ... ENDM (repeat the lines in between)
//!         IF expr ... [ELSE ...] ENDIF
//!         INCLUDE 'file'      (beside the including file, else in an include
//!                              folder, else the SDK's own: rombios.inc,
//!                              ekdos.inc, ports.inc)
//!         INCBIN 'file'       (beside the including file, else in an
//!                              include folder)
//!         ASSERT expr[, 'message']
//!         BSS                 (from here on only DS: it reserves memory that
//!                              is not part of the output file)
//! ```
//!
//! Names are case-sensitive; mnemonics, registers and directives are not.
//! A label starting with a dot is local: `.loop` after `draw:` is
//! `draw.loop`, and can be named that way from elsewhere.
//!
//! Numbers: 123, 0FFh, 1010b, 'c'. Expressions use + - * / % & | ^ << >>,
//! == != < > <= >= (1 or 0), ( ), unary - ~ !, HIGH(x), LOW(x), and $ for
//! the current address.
//!
//! Symbols are resolved by assembling again until none moves, at most ten
//! times, and then once more: the last pass, in which an undefined symbol,
//! a byte out of range and a failed ASSERT are errors.
//!
//! The listing shows each instruction's cycle count (the taken count for a
//! conditional call or return). The symbol file lists every label and EQU as
//! `name 1234`, in hex, for tools and tests. The debug file is what a
//! debugger needs (the SDK's docs/asm.md, Outputs): the load address, the
//! source files, where each line's instructions and data are, and the
//! symbols.

mod asm;
mod encode;
mod expr;
mod headers;
mod output;
mod path;

pub use asm::{Error, Options, Program, assemble, assemble_with, names};
pub use headers::{HEADERS, header};
