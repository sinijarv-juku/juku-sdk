# juku-sdk

Tools for writing Juku programs in 8080 assembler, in Rust: a Cargo
workspace with a crate per tool in `crates/<name>`, the library
`juku_<name>` and, with a `src/main.rs`, the command `juku-<name>`.
`README.md` is the overview; `docs/<name>.md` describes each tool, file
formats too. Keep them in step with the code. Write them as a professional
technical writer would: neutral and factual, with no hype or marketing
tone, in American spelling (behavior, color, center, gray, catalog,
startup). Nothing has been tried on a real machine, so do not state that
a program or a name behaves there as it does in the emulator: say what it
is based on, or that it should.

- `crates/asm` is `juku-asm`, the assembler. Names keep their case.
- **juku-asm is to be easy to use**, with simple syntax, and its author
  should never have to think about how it works inside, its passes above
  all. Say a rule in the source's terms (above, below, in this file, the
  first error in the source), never in passes; an error names the line and
  what is wrong there. A feature whose behavior depends on the passes is
  reworked until it does not, as errors were when an early pass's error
  hid an `assert` above it.
- `crates/disk` is `juku-disk`, files on EKDOS disk images. Its tests make
  their images: real disk images are other people's software, not to be put
  in the repository.
- `demos/` holds demo programs for the machine (`bounce.asm`,
  `scroller.asm`) and the images they assemble to. `crates/demos` is
  `juku-demos`, a library with both (`include_str!`, `include_bytes!`):
  juku-emu's tests and benchmark run them, and its debug server's tests
  find `bounce.asm`'s lines by their text (`bounce_line`), so change those
  lines with them. Its test checks that each image is what its source
  assembles to; `JUKU_ASM_BLESS=1 cargo test -p juku-demos` writes them
  anew.

- `include/` holds the include files for RomBios, EKDOS and the ports, and
  `macros.inc` (macros, no names), built into `juku-asm`
  (`crates/asm/src/headers.rs`, `include_str!`; a new file goes into its
  list). They are the source of the names: juku-emu's
  `resources/symbols/rombios.dbg` and `ekdos.dbg` must have the same names
  and values, which its debug server's test `system_symbols_are_the_sdks`
  checks. Change both together. Values come from juku-emu's machine
  description; cite its sections.

- `crates/i8080` is `juku-i8080`, a library: the 8080's 256 opcodes as a
  table (mnemonic, registers, data, cycles, documented or not). `juku-asm`
  assembles from it and `juku-dis` disassembles with it; juku-emu's debug
  server checks its core's own disassembler against it
  (`cpu_disassembler_is_the_sdks`).
- `crates/lsp` is `juku-lsp`, the language server: LSP on stdin and
  stdout, with `juku-json`'s framing. `Server` (`server.rs`) is the
  protocol as a state machine, messages in and out, which its tests drive;
  `project.rs` is the workspace: open documents, which file includes
  which, the main files, and assembling them with `juku_asm::index` and
  the editor's unsaved text. Its errors are juku-asm's, unchanged.
  `names.rs` keeps each main file's names; `navigate.rs` answers
  definition, references, hover and symbols from them, a word matched to
  a name by the uses the index has on its line; `edit.rs` completion,
  operands, rename, folding and inlay hints.
- `crates/json` is `juku-json`, a library: JSON values, their parser and
  writer, and the `Content-Length` framing DAP and LSP share
  (`frame.rs`). juku-emu's debug server is built on it, so juku-emu's app
  links it: it has no `format!` (numbers as text by hand, `text.rs`) and
  no floating point (a number that is not whole stays text), to keep that
  app small (juku-emu's spec §12.1-12.2). Keep it so, and check the
  app's size after a change.
- `vscode/` is the VS Code extension `juku` (publisher `sinijarv-juku`):
  the language `juku-asm` (`syntaxes/juku-asm.tmLanguage.json`, juku-asm's
  syntax: keep it in step with the assembler) and the debug type `juku`,
  which connects to juku-emu's debug server; the launch settings in
  `package.json` are what that server takes (juku-emu's spec §12). It
  also starts `juku-lsp` (`lsp.js`, a client written by hand on `rpc.js`,
  JSON-RPC with no vscode in it); its settings are the server's
  (docs/lsp.md, Settings), so change them together. Juku: Build
  (`build.js`) runs `juku-asm` on the main files the server's
  `juku/mainFiles` names; `tools.js` finds the SDK's commands.
  `vscode/package.py` packs it as `target/juku.vsix` without npm, with the
  SDK's version.
- `crates/dis` is `juku-dis`, the disassembler. With `-s` it writes source
  that `juku-asm` assembles back to the same bytes; its tests check that on
  the asm demos.

```sh
./build.sh                    # release build of every command -> target/bin/
./build.sh debug              # unoptimized, with debug info
./build.sh install            # every command into cargo's bin folder, on the PATH
build.cmd [debug|install]     # the same on Windows -> target\bin\*.exe
cargo test                    # every crate's tests
JUKU_ASM_BLESS=1 cargo test -p juku-asm   # writes the asm demos' expected outputs anew
JUKU_ASM_BLESS=1 cargo test -p juku-demos # writes demos/*.bin anew
python3 vscode/package.py && code --install-extension target/juku.vsix   # the VS Code extension
```

- **Built as juku-emu builds its Rust code** (its spec §3.1-3.2): nightly
  Rust with `rust-src`, pinned by date in `rust-toolchain.toml`, the same
  date as juku-emu's (change both together), std built
  from source (`-Z build-std`), the release profile's `panic =
  "immediate-abort"`, LTO and one codegen unit, and `-Zlocation-detail=none
  -Zfmt-debug=none`; on Windows (`build.cmd`, as
  juku-emu's) also the C runtime linked in, all but the Universal CRT. Keep
  `build.sh` and `build.cmd` doing the same; both find the commands by the
  `crates/<name>/src/main.rs` naming. Plain `cargo build --release`, `cargo
  run --release` and `cargo install` fail for want of `build-std`: use
  `build.sh`. `cargo test` works; `{:?}` prints nothing in a release build,
  so messages use `{}`.
- **Independent.** Nothing here reads or builds anything outside this
  repository; each crate's tests use its own files only.
- **The demos' outputs are the test** (`crates/asm/tests/demos/`). Each
  demo's `.bin`, `.lst`, `.sym` and `.dbg` beside it are what it must
  assemble to. Bless only after a change meant to change them, and read the
  diff.
- **Who uses it.** juku-emu (`../juku-emu`): its debug server is built on
  `juku-json` and reads the
  debug information (`debugger/dap/src/info.rs`, its spec §12.6), and its
  tests assemble `demos/bounce.asm` with `juku-asm`'s library and run the
  demos' images from `juku-demos`, git dependencies on this repository at a
  release tag. Programs built with `juku-asm` rely on
  its output, so a change must keep what a source assembles to (image,
  listing, symbols, debug information) the same unless that is the point;
  the demos' expected outputs are the test. A change to the debug
  information's format goes into juku-emu's loader and spec too.
- **Assembly source** written here is lower case: mnemonics, registers,
  directives, labels and hex digits (`mvi a, 0ffh`). A name defined with
  `equ` is upper case only for a constant, such as a count, size, mask or
  character (`ROWS equ 4`, `ESC equ 1bh`); one for an address, a port or
  an offset is lower case, as a label is (`tto equ 0ffd9h`, `o_x equ 1`).
  The include files keep to this. Source here includes `rombios.inc` as
  `rb`, `ekdos.inc` as `dos` and `ports.inc` as `io` (`call rb.tto`, `call
  dos.bdos`, `out io.t2_1`), and so does what `juku-dis -s` writes. An
  anonymous label (`:`, `jnz :-`) only where a name would be noise, one
  that would have to be made up (`.ok`, `.wait2` beside `.wait`). Names are
  case-sensitive; mnemonics, registers and directives are not.
- **Versions**: one for the SDK, semantic, in the workspace's `Cargo.toml`
  (`[workspace.package]`) only; crates take it with `version.workspace =
  true`. It has `-dev` after it while the next version is being made.
  `README.md`, Versions, says what is MAJOR, MINOR and PATCH. While the
  version is 0.x, a release need not be compatible with earlier ones. Don't
  bump it in a change; a release is its own commit, dropping `-dev`,
  `build: bump the version to X.Y.Z`, tagged `vX.Y.Z`, and the next commit
  starts `X.Y+1.0-dev`.
- **Release notes**: the tag's draft (`release.yml`) has the summary line
  and TODOs for the changes, which are written before it is published
  (`git log vPREV..vX.Y.Z`): first what a source or script must change,
  or that nothing must, then the changes by tool. Only what a user of a
  command or the VS Code extension sees changed: no library API (a new
  function, an `Options` field), no internal crates (`juku-json`,
  `juku-demos`), no refactoring or code moved, no changes to tests, the
  build or the documentation alone. Neutral and factual, as the docs are:
  say what was added or changed, in plain items, with no bold labels and no
  tour of a feature's details. A new tool is one line saying what it is,
  with a link to its doc page. No installation instructions: each tool's
  doc page has them.
- **No dependencies**, but each other's crates in this workspace.
- **Commits:** a subject in the Neovim repository's style, `type(scope):
  subject`, with type one of `build ci docs feat fix perf refactor revert
  test`, imperative, under 72 characters; a body only when something is not
  obvious; no AI attribution. The scope is the crate (`asm`, `disk`) where
  there is one.
