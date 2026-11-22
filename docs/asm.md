# juku-asm

The SDK's Intel 8080 assembler. Besides the image it writes a listing with
cycle counts, a symbol file, and debug information: what a debugger needs to
debug a program from its source.

```sh
juku-asm main.asm main.bin [-l main.lst] [-s main.sym] [-g main.dbg] [-I folder]...
juku-asm --version
```

It is a command and a library (`juku_asm::assemble`, and `assemble_with`
for include folders), in `crates/asm`.
`crates/asm/tests/demos/` has example programs, which `cargo test`
assembles and compares with the outputs beside them.

## Source

```asm
TTO     equ 0ffd9h              ; RomBios: print the character in A

        org 100h

main:   lxi h, greeting
.next:  mov a, m
        ora a
        rz
        call TTO
        inx h
        jmp .next

greeting:
        db 'Hello, Juku!', 0
```

- Intel's mnemonics, registers and directives, in either case. Names are
  case-sensitive: `COUNT equ 3` and a label `count:` are two symbols. A
  label is a name with a colon, or a name in the first column.
- A label starting with a dot is local: `.next` after `main:` is
  `main.next`, and can be named that way from elsewhere. `equ` does not
  start a scope.
- Numbers: `123`, `0ffh`, `1010b`, `12d`, `'c'`. Expressions use `+ - * / %
  & | ^ << >>`, `== != < > <= >=` (1 or 0), parentheses, unary `- ~ !`,
  `high(x)`, `low(x)`, and `$` for the current address. Division rounds
  down, and by 0 is 0.
- Directives:

  | | |
  |---|---|
  | `name equ expr` | a constant |
  | `org expr` | the address; the first `org` is the load address |
  | `db expr or 'string', ...` | bytes |
  | `dw expr, ...` | words, low byte first |
  | `ds count[, fill]` | `count` bytes of 00h or `fill` |
  | `align n` | 00h up to a multiple of `n` |
  | `rept count` ... `endm` | the lines in between, `count` times |
  | `if expr` ... [`else` ...] `endif` | |
  | `include 'file'`, `incbin 'file'` | see below |
  | `assert expr[, 'message']` | |
  | `bss` | from here on only `ds`: memory reserved, not in the image |

- Symbols are resolved by assembling again until none moves. An undefined
  symbol, a byte out of range and a failed `assert` are errors in the last
  pass.

## Include files

`include 'name'` and `incbin 'name'` look for the file:

1. beside the file the line is in,
2. in each folder given with `-I`, in order,
3. for `include`, among the SDK's include files, built into `juku-asm`:
   `rombios.inc`, `ekdos.inc` and `ports.inc` (the SDK's README). A file of
   the same name found earlier is taken instead.

An absolute path is taken as it is. Messages and the listing name a
built-in file as `<juku-sdk>/rombios.inc`.

## Outputs

- **Image** (`<out.bin>`): the bytes from the load address to the last one
  made, with 00h in the gaps.
- **Listing** (`-l`): each line's address, its first four bytes and, for an
  instruction, its cycle count (the taken count for a conditional call or
  return), then the line.
- **Symbols** (`-s`): every label and constant as `name 1234`, in hex, in
  order of name (upper case first).
- **Debug information** (`-g`), in JSON, version 2. For the program above,
  as `hello.asm`, assembled to `hello.bin`:

  ```json
  {
    "version": 2,
    "image": "hello.bin",
    "load": 256,
    "size": 26,
    "entry": 256,
    "files": ["hello.asm"],
    "lines": [
      [256,0,5,3,1,1,1,1,1,1,1,3,1,1,1,3]
    ],
    "data": [[269,13]],
    "labels": [
      ["main",256],
      ["main.next",259],
      ["greeting",269,13]
    ],
    "equs": [
      ["TTO",65497]
    ]
  }
  ```

  - Paths are relative to the `.dbg` file.
  - `lines` has a row for each run of code from one file with no gap
    between its lines: the address, the file, the first line and that
    line's length in bytes, then for each line after it how many lines on it
    is (0 in a `rept` body repeated on one line) and its length. A run ends
    where the code skips bytes or the file changes, or a line goes back, as
    each time round a `rept` body does.
  - `data` is the ranges `db`, `dw`, `ds`, `align` and `incbin` made, joined
    where they meet.
  - `labels` are `[name, address]`, with the size of the line of data or of
    the memory reserved after `bss` at the label as a third field: what a
    watchpoint on it watches. `equs` are `[name, value]`. Both are in order
    of value. Memory reserved after `bss` is named in the labels, not in the
    lines.

  The Juku emulator's debug server, `juku-dap` in juku-emu, reads this
  format.
