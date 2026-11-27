# juku-sdk

Tools for writing programs for the Juku E5101/E5104, the Estonian school
computer of the late 1980s, in Intel 8080 assembler.

| Command | | |
|---|---|---|
| `juku-asm` | the assembler: image, listing, symbols, debug information | [docs/asm.md](docs/asm.md) |
| `juku-disk` | files on EKDOS disk images: list, copy off, put on, remove; blank disks | [docs/disk.md](docs/disk.md) |
| `juku-dis` | the disassembler: a listing, or source that assembles back to the same bytes | [docs/dis.md](docs/dis.md) |
| `juku-lsp` | the language server: `juku-asm`'s errors in an editor as the source is typed | [docs/lsp.md](docs/lsp.md) |

The debug information is what the Juku emulator's debugger, in
[juku-emu](https://github.com/sinijarv-juku/juku-emu), loads to debug a
program from its source in VS Code, with the SDK's VS Code extension in
`vscode/`: the language `juku-asm` for `.asm` files, with `juku-lsp` behind
it, and the debug type `juku`, which connects to the emulator
([vscode/README.md](vscode/README.md)).

`demos/` has demo programs written with the SDK, raster effects for the
expansion ROM window ([demos/README.md](demos/README.md)).

## Include files

`include/` has the machine's names, defined with `equ`, for any program to
include. `juku-asm` has them built in, so `include 'rombios.inc'` works
without a path. Addresses and ports are in lower case, as labels are; the
SDK keeps upper case for constants such as counts, sizes, masks and the
BDOS's function numbers.

| File | |
|---|---|
| `rombios.inc` | RomBios 3.43's entry points (`tto`, `constat`, `monitor`, …) and system variables (`conch`, `sqikey`, …) |
| `ekdos.inc` | EKDOS's page zero (`wboot`, `bdos`, `tfcb`, `tbuff`, …), the BDOS functions (`C_WRITESTR`, `F_OPEN`, …) and CP/M 2.2's structs: `fcb`, `dirent`, `dpb` |
| `ports.inc` | the I/O ports: `pic_0`, `ppi1_a`…`ppi1_ctl`, `t1_0`…`t3_ctl`, `fdc_cmd`…, `mouse_data`, … |
| `macros.inc` | macros, not names: `pushr` and `popr`, `save` and `restore` for register pairs ([docs/asm.md](docs/asm.md)) |

```asm
        include 'rombios.inc' as rb
        include 'ekdos.inc' as dos
        include 'ports.inc' as io

        org 100h
main:   mvi a, 'J'
        call rb.tto
        in io.t2_0              ; lines left in the frame
        jmp dos.wboot
```

The SDK's own source includes them so, under `rb`, `dos` and `io`, and
`juku-dis` writes them so: a program can then have names of its own like
these (`move`, `fill`, `fcb`). A plain `include 'rombios.inc'` defines them as they
are, and `include 'rombios.inc' (tto, crlf)` only those it lists
([docs/asm.md](docs/asm.md), Namespaces).

They come from juku-emu's [machine description](https://github.com/sinijarv-juku/juku-emu/blob/main/docs/Juku_machine_description.md), whose debugger
names the same RomBios and EKDOS addresses. The names and addresses are a
best reading of the ROMs, disks, documents and schematics, and corrections
are welcome. No ROM code is included, only names and numbers.

## Building

```sh
./build.sh                    # every command -> target/bin/
./build.sh install            # every command into cargo's bin folder, on the PATH
./build.sh debug              # unoptimized, with debug info
./build.sh x86_64             # on a Mac: for Intel Macs -> target/bin-x86_64/ (arm64 for Apple silicon)
cargo test                    # the tests, of every crate
python3 vscode/package.py     # the VS Code extension -> target/juku.vsix
code --install-extension target/juku.vsix
```

Building needs nightly Rust with `rust-src`. `rust-toolchain.toml` names a
nightly by its date, so that every build uses the same compiler, and
`rustup` fetches it on the first build. `build.sh` builds
the standard library from source, to make the commands smaller, as juku-emu
is built. On Windows `build.cmd` does the same (`target\bin\*.exe`), and
needs the Visual Studio Build Tools too.

Each tool is a crate in `crates/<name>`: the library `juku_<name>` and the
command `juku-<name>`. `crates/i8080`, a library only, is the 8080's
instruction set as a table, which the assembler and the disassembler share.
`crates/demos`, a library only, holds the demos' sources and images for
other programs' tests. `crates/json`, a library only, is JSON and the
`Content-Length` framing of the Debug Adapter Protocol and the Language
Server Protocol, which juku-emu's debug server uses. They have no
dependencies but each other.

## Versions

The SDK has one version, for all its tools: semantic, `MAJOR.MINOR.PATCH`,
in the workspace's `Cargo.toml` only, with `-dev` while the next version is
being made (`X.Y.Z-dev`, which semver puts before `X.Y.Z`). Each command
prints it with `--version`.

While the version is 0.x, nothing is promised: any release may break
compatibility with earlier ones. One that adds or changes something
bumps MINOR, one with fixes only PATCH.

From 1.0.0 the interface is each command's command line, the source syntax,
the include files' names, what the tools write (images, symbol files, debug
information) and the libraries' public APIs. A listing is for people, and
its layout may change in any release.

- **PATCH**: fixes. A program that assembled before assembles to the same
  image, symbols and debug information, unless they were wrong.
- **MINOR**: additions that keep that: new directives, options, outputs or
  commands.
- **MAJOR**: anything else: a program that assembled no longer does, or to
  something else; a change a reader of the outputs must follow; a change to
  a library's API.

A release is the commit that drops `-dev` from the version, `build: bump the
version to X.Y.Z`, tagged `vX.Y.Z`; the next commit starts the next one,
`build: start version X.Y+1.0-dev`. Pushing the tag drafts the GitHub
release: the commands for macOS (Apple silicon and Intel), Linux and
Windows with the include files and the demos, and the VS Code extension (`.github/workflows/release.yml`),
with places for the changes since the previous release, which are written
in before it is published by hand. Every push to `main` and every pull request is tested
and built on the three systems (`.github/workflows/ci.yml`), except one
that changes only documentation (Markdown files and `docs/`).

## License

MIT: see [LICENSE](LICENSE).
