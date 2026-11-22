# juku-sdk

Tools for writing Juku programs in 8080 assembler, in Rust: a Cargo
workspace with a crate per tool in `crates/<name>`, the library
`juku_<name>` and, with a `src/main.rs`, the command `juku-<name>`.
`README.md` is the overview; `docs/<name>.md` describes each tool, file
formats too. Keep them in step with the code.

- `crates/asm` is `juku-asm`, the assembler. Names keep their case.
- `crates/disk` is `juku-disk`, files on EKDOS disk images. Its tests make
  their images: real disk images are other people's software, not to be put
  in the repository.

- `include/` holds the include files for RomBios, EKDOS and the ports, built
  into `juku-asm` (`crates/asm/src/headers.rs`, `include_str!`; a new file
  goes into its list). They are the source of the names: juku-emu's
  `resources/symbols/rombios.dbg` and `ekdos.dbg` must have the same names
  and values, which its debug server's test `system_symbols_are_the_sdks`
  checks. Change both together. Values come from juku-emu's machine
  description; cite its sections.

- `crates/i8080` is `juku-i8080`, a library: the 8080's 256 opcodes as a
  table (mnemonic, registers, data, cycles, documented or not). `juku-asm`
  assembles from it and `juku-dis` disassembles with it; juku-emu's debug
  server checks its core's own disassembler against it
  (`cpu_disassembler_is_the_sdks`).
- `vscode/` is the VS Code extension `juku` (publisher `sinijarv-juku`):
  the language `juku-asm` (`syntaxes/juku-asm.tmLanguage.json`, juku-asm's
  syntax: keep it in step with the assembler) and the debug type `juku`,
  which connects to juku-emu's debug server; the launch settings in
  `package.json` are what that server takes (juku-emu's spec §12).
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
- **Who uses it.** juku-emu (`../juku-emu`): its debug server reads the
  debug information (`debugger/dap/src/info.rs`, its spec §12.6) and its
  tests assemble its demos with `juku-asm`'s library, a git dependency on
  this repository at a release tag. Programs built with `juku-asm` rely on
  its output, so a change must keep what a source assembles to (image,
  listing, symbols, debug information) the same unless that is the point;
  the demos' expected outputs are the test. A change to the debug
  information's format goes into juku-emu's loader and spec too.
- **Assembly source** written here is lower case: mnemonics, registers,
  directives, labels and hex digits (`mvi a, 0ffh`). Constants defined with
  `equ` are upper case (`TTO equ 0ffd9h`). Names are case-sensitive;
  mnemonics, registers and directives are not.
- **Versions**: one for the SDK, semantic, in the workspace's `Cargo.toml`
  (`[workspace.package]`) only; crates take it with `version.workspace =
  true`. It has `-dev` after it while the next version is being made.
  `README.md`, Versions, says what is MAJOR, MINOR and PATCH. While the
  version is 0.x, a release need not be compatible with earlier ones. Don't
  bump it in a change; a release is its own commit, dropping `-dev`,
  `build: bump the version to X.Y.Z`, tagged `vX.Y.Z`, and the next commit
  starts `X.Y+1.0-dev`.
- **No dependencies**, but each other's crates in this workspace.
- **Commits:** a subject in the Neovim repository's style, `type(scope):
  subject`, with type one of `build ci docs feat fix perf refactor revert
  test`, imperative, under 72 characters; a body only when something is not
  obvious; no AI attribution. The scope is the crate (`asm`, `disk`) where
  there is one.
