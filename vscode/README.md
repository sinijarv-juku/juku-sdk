# Juku for VS Code

Writing and debugging Juku programs in VS Code:

- the language `juku-asm` for `.asm` and `.inc` files: juku-asm's syntax,
  colored, and breakpoints;
- with the SDK's language server, `juku-lsp`: juku-asm's errors as the
  source is typed, going to a name's definition and uses, hovers with
  values and comments, the outline, completion, operands, renaming,
  folding, and each line's address and cycles ([docs/lsp.md](../docs/lsp.md));
- **Juku: Build**, which assembles the program being edited with `juku-asm`;
- the debug type `juku`, which connects VS Code to the debug server of the
  Juku emulator, [juku-emu](https://github.com/sinijarv-juku/juku-emu) (its
  spec §12, in `docs/Emulator_spec.md`). The emulator speaks the Debug
  Adapter Protocol itself; the extension only names the debug type and
  connects, and starts the emulator when it is not running.

The language works on any system. Debugging needs Juku's macOS or Windows
app.

## Trying it

1. Install the extension: pack it and give the package to VS Code.

   ```sh
   python3 vscode/package.py      # target/juku.vsix
   code --install-extension target/juku.vsix
   ```

   Do it again after changing the extension, and reload VS Code's windows.
   To try a change without installing, run a window with the extension
   loaded from this folder: `code --extensionDevelopmentPath="$PWD/vscode" demos`.
   Only that window has it.

   A `files.associations` setting that maps `*.asm` to another language
   overrides the extension's, and VS Code then allows no breakpoints in
   `.asm` files: map them to `juku-asm`, or remove the entry.
2. Start a debug session (below). If nothing listens on the port, the
   extension starts Juku with its debug server on: `open -a Juku`, or the
   app the `juku.app` setting names, such as
   `/path/to/juku-emu/target/Juku.app`. A Juku already running needs
   Settings → Machine → Debugger: Accept Connections ticked, or
   `--debug-server` on its command line.

   On Windows the extension starts `Juku.exe` where the installer puts it
   (`%LOCALAPPDATA%\Programs\Juku`), or from the PATH; `juku.app` can name
   another, such as `D:\juku-emu\target\windows\Juku.exe`. A Juku already
   running there takes the port from the new start, which then exits, so
   it need not have Accept Connections ticked.
3. Pause stops the machine wherever it is. When it stops where there is no
   source, as a Step Into a RomBios routine (`call rb.tto`) does, the
   extension shows the frame the CPU is in, in the Disassembly view, where
   steps go by instruction; a `ret` back into the program's code shows its
   source again, and a click in the editor steps by line there. VS Code by
   itself would show the call's line, the nearest frame with source, while
   the CPU runs on elsewhere. Another frame can be picked in Call Stack, and
   any one opened with right-click, Open Disassembly View. Registers are
   under Variables and can be changed.

## The language server

The extension starts `juku-lsp` for `.asm` and `.inc` files: the one the
`juku.lsp.path` setting names, else the one on the PATH, else the one in
cargo's bin folder, where the SDK's `./build.sh install` (or `build.cmd
install`) puts it. Without it, the files are colored and nothing more,
and the extension says so once.

| Setting | |
|---|---|
| `juku.mainFiles` | The main files to assemble, absolute or from the workspace folder. By default each `.asm` file no other file includes |
| `juku.includePaths` | The folders `include` and `incbin` look in, as `juku-asm -I` |
| `juku.inlayHints` | Each line's address, and its cycles or bytes, after it. Off by default |
| `juku.lsp.enabled` | `false` runs no language server |
| `juku.lsp.path` | The `juku-lsp` program |

**Juku: Restart Language Server** in the Command Palette starts it again,
as does changing a `juku.lsp` setting. A server that stops by itself is
started again, up to three times in three minutes; what it writes goes to
the Juku Language Server output.

The client is written in `lsp.js` and `rpc.js`, without the
`vscode-languageclient` package, so that the extension needs no npm.

## Building

**Juku: Build** (Ctrl+Shift+B, ⇧⌘B on macOS), with a `juku-asm` file
being edited, saves
the files and assembles the program the file is in: its main file, which
the language server knows (a file included by another is built by building
that one), or the file itself if it is an `.asm` file in no other's
program. It runs `juku-asm -g` in the main file's folder, so the image
(`.bin`) and the debug information (`.dbg`) are written beside it; the
status bar says how many bytes at what address. An error is shown, and the
Juku Build output has all `juku-asm` wrote. The shortcut is VS Code's Run
Build Task elsewhere, and in `juku-asm` files while not debugging is Juku:
Build; Terminal → Run Build Task still runs a `tasks.json` build, and the
Keyboard Shortcuts editor can give Juku: Build another key.

| Setting | |
|---|---|
| `juku.build.arguments` | What `juku-asm` is given before the include folders (`juku.includePaths`) and the main file: `["-g"]` by default; `-l` adds a listing with cycle counts, `-s` the symbols |
| `juku.asm.path` | The `juku-asm` program: by default the one on the PATH, or in cargo's bin folder |

To build a `.COM` file, name the image with `-o`, in the main file's
folder: `"juku.build.arguments": ["-g", "-o", "game.com"]` writes
`game.com` and `game.dbg`. `-o` names the image of every program built,
so this is for a workspace with one main file.

## Launching a program

A launch loads the program into memory and starts it, as the monitor or
EKDOS would. Have the machine at the monitor's `*` prompt, or at EKDOS's
`A>` for a `.COM` file that uses EKDOS; the program goes in when the
machine next waits for a key there.

```json
{
    "type": "juku",
    "request": "launch",
    "name": "Hello",
    "program": "${workspaceFolder}/hello.com",
    "stopOnEntry": true
}
```

- `program` is loaded at 0100h, or where its debug information says.
- `debugInfo` names `.dbg` files to load; by default the one beside the
  program, if there is one.
- `args` is the command tail under EKDOS (`"foo.txt b:"`), set at 0080h with
  the FCBs at 005Ch and 006Ch.
- `stopOnEntry` stops before the first instruction; `reset` power cycles
  first, so the program starts at the monitor's prompt.
- `snapshot` opens a Juku snapshot first, as Machine → Open Snapshot would
  (asking about changed disks): save one at `A>` with the disks the program
  needs, and every run starts from the same machine. With `snapshot` or
  `reset`, Restart (⇧⌘F5 on a Mac) starts the program again from there, reading the
  program and its debug information again: assemble, restart.
- `load`, `entry` and `exit` override the addresses; a program ends when it
  returns to the address pushed for it, EKDOS's warm boot (0) or MONITOR
  (FFC4h). Then the session ends, and the machine carries on.

## Debugging from source

Assemble with debug information:

```sh
./build.sh install                 # juku-sdk's commands, the assembler juku-asm too
juku-asm -g bounce.asm             # bounce.bin and bounce.dbg
```

Breakpoints can then be set in the `.asm` files, which the extension colors
as the language `juku-asm`. Steps go by source line, over calls (F10) or
into them (F11); frames and the disassembly are named by the program's
labels.

## Data

- **Watchpoints:** right-click a symbol under Variables → Symbols, or a
  port under Devices → Ports, and choose Break on Value Change (writes,
  OUT), Break on Value Read (reads, IN) or Break on Value Access. An
  address: Add Data Breakpoint at Address in the Breakpoints view.
- **Breakpoints** take a condition (`A == 'J'`, `w[200h] >= 3`), a hit
  count (`5`, `== 3`, `% 10`) or a log message (`char {A} at {HL}`; `{A,d}`
  in decimal), which prints to the Debug Console without stopping.
- **Expressions** in the Debug Console, Watch and hovers: numbers as in the
  assembler (`0FFh`, `1010b`, `'J'`), registers, symbols (in another case
  too, if only one matches: `CONCH` is `conch`), `[x]` for the byte
  at x, `w[x]` for the word, `HIGH()`, `LOW()` and the assembler's operators,
  with `&&` and `||`.
- **Jump to Cursor** in a source file moves PC to that line.
- RomBios's entry points and variables, and EKDOS's addresses, are known
  by name (`CALL tto`, `LDA conch`); `"systemSymbols": false` leaves them
  out.

## Attaching

To debug a program already in memory, attach with its debug information:

```json
{
    "type": "juku",
    "request": "attach",
    "name": "Attach to Juku",
    "debugInfo": ["${workspaceFolder}/bounce.dbg"]
}
```

With the SDK's `demos/bounce.bin` as the expansion ROM, start it with `A` at
the `*` prompt. `port` picks another port than the `juku.port` setting (5104), and
`stepIntoInterrupts` stops steps at an interrupt handler's first
instruction.
