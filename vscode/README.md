# Juku for VS Code

Writing and debugging Juku programs in VS Code:

- the language `juku-asm` for `.asm` files: juku-asm's syntax, coloured,
  and breakpoints;
- the debug type `juku`, which connects VS Code to the debug server of the
  Juku emulator, [juku-emu](https://github.com/sinijarv-juku/juku-emu) (its
  spec §12, in `docs/Emulator_spec.md`). The emulator speaks the Debug
  Adapter Protocol itself; the extension only names the debug type and
  connects, and starts the emulator when it is not running.

The language works on any system. Debugging needs Juku's macOS app for
now: the Windows app has no debug server yet.

## Trying it

1. Install the extension: pack it and give the package to VS Code.

   ```sh
   python3 vscode/package.py      # target/juku.vsix
   code --install-extension target/juku.vsix
   ```

   Do it again after changing the extension, and reload VS Code's windows.
   To try a change without installing, run a window with the extension
   loaded from this folder: `code --extensionDevelopmentPath="$PWD/vscode" crates/asm/tests/demos`.
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
3. Pause stops the machine wherever it is. Where there is no source,
   right-click a frame and choose Open Disassembly View to step by
   instruction. Registers are under Variables and can be changed.

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
juku-asm bounce.asm bounce.bin -g bounce.dbg
```

Breakpoints can then be set in the `.asm` files, which the extension colours
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
  too, if only one matches: `conch` is CONCH), `[x]` for the byte
  at x, `w[x]` for the word, `HIGH()`, `LOW()` and the assembler's operators,
  with `&&` and `||`.
- **Jump to Cursor** in a source file moves PC to that line.
- RomBios's entry points and variables, and EKDOS's addresses, are known
  by name (`CALL TTO`, `LDA CONCH`); `"systemSymbols": false` leaves them
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

With juku-emu's `demos/bounce.bin` as the expansion ROM, start it with `A` at the `*`
prompt. `port` picks another port than the `juku.port` setting (5104), and
`stepIntoInterrupts` stops steps at an interrupt handler's first
instruction.
