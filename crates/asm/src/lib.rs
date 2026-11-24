//! An Intel 8080 assembler for Juku programs.
//!
//! Syntax, as in Intel's assembler:
//!
//! ```text
//! label:  MNEMONIC operands   ; comment
//! name    EQU expression
//! name    SET value           (a number, a 'string' or a [list], which
//!                              another SET can change)
//!         ORG expression      (the first ORG outside BSS sets the load address)
//!         DB  expression or 'string', ...
//!         DW  expression, ...
//!         DS  count[, fill]   (reserve bytes, filled with 00h or fill)
//!         ALIGN n             (pad with 00h up to a multiple of n)
//!         REPT count ... ENDM (repeat the lines in between)
//!         FOR name, list ... ENDM (the lines for each item, as name)
//! name    MACRO params ... ENDM (a macro: docs/asm.md, Macros; {name}
//!                              joins an argument to a name, {upper(name)}
//!                              and {lower(name)} with its case changed)
//! name    STRUCT [size] ... ENDS (fields, each with DB, DW, DS or a
//!                              struct: name.field is its offset, SIZE(name)
//!                              the struct's, padded to size if given;
//!                              `label name values` lays it out,
//!                              label.field each one's address)
//! name    UNION [size] ... ENDS (a struct whose fields all start at 0)
//!         OPT ON|OFF          (expansions leave out MOV r, r)
//!         IF expr ... [ELSEIF expr ...] ... [ELSE ...] ENDIF
//!         INCLUDE 'file'      (beside the including file, else in an include
//!                              folder, else the SDK's own: rombios.inc,
//!                              ekdos.inc, ports.inc, macros.inc)
//!         INCBIN 'file'       (beside the including file, else in an
//!                              include folder)
//!         ASSERT expr[, 'message']
//!         CODE, CONST, DATA, BSS  (go on in that section; they are laid out
//!                              in that order, each after the one before
//!                              unless an ORG in it places it; BSS takes only
//!                              DS with no fill, ALIGN and structs, and
//!                              reserves memory that is not part of the
//!                              output file)
//! ```
//!
//! Names are case-sensitive; mnemonics, registers and directives are not.
//! A label starting with a dot is local: `.loop` after `draw:` is
//! `draw.loop`, and can be named that way from elsewhere.
//!
//! A colon alone in the first column is an anonymous label: `:-` is the last
//! one up to the line, `:+` the next one after it, `:--` and `:++` one
//! further. A reference reaches only those in its file and in the scope of
//! its label.
//!
//! `INCLUDE 'file' AS name` puts the file's names under `name`: `name.tto`.
//! `INCLUDE 'file' (tto, crlf)` defines those as they are, and the others
//! under the AS name or else the file's name: `rombios.fill`. In the file, a
//! name is its own if it defines it, else the includer's.
//!
//! Numbers: 123, 0FFh, 1010b, 'c'. Expressions use + - * / % & | ^ << >>,
//! == != < > <= >= (1 or 0), ( ), unary - ~ !, HIGH(x), LOW(x), START(s),
//! END(s) and SIZE(s) of a section, SIZE(t) of a struct, and $ for the
//! current address.
//!
//! Symbols are resolved by assembling again until none moves, at most ten
//! times, and then once more: the last pass. Errors are that pass's, the
//! first in the source; before it, a line with one makes nothing.
//!
//! The listing shows each instruction's cycle count (the taken count for a
//! conditional call or return). The symbol file lists every label and EQU as
//! `name 1234`, in hex, for tools and tests. The debug file is what a
//! debugger needs (the SDK's docs/asm.md, Outputs): the load address, the
//! source files, where each line's instructions and data are, and the
//! symbols.
//!
//! For an editor, [`index`] also lists where each name is defined and used.
//! It records them in every pass, and with an error takes the pass before
//! the last, which went past each line with one, so the names below the
//! error are there too.

mod asm;
mod encode;
mod expr;
mod headers;
mod macros;
mod output;
mod path;

pub use asm::{Definition, Error, Include, Index, NameKind, Options, Placed, Program, Use, assemble, assemble_with, index, names};
pub use headers::{HEADERS, PREFIX as HEADER_PREFIX, header};
