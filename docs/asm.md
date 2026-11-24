# juku-asm

The SDK's Intel 8080 assembler. Besides the image it writes a listing with
cycle counts, a symbol file, and debug information: what a debugger needs to
debug a program from its source.

```sh
juku-asm [-l] [-s] [-g] [-o image.bin] [-I folder]... main.asm
juku-asm --version
```

- The image is written beside the source, with the extension `.bin`:
  `juku-asm src/demo.asm` writes `src/demo.bin`.
- `-l` also writes the listing (`demo.lst`), `-s` the symbols
  (`demo.sym`) and `-g` the debug information (`demo.dbg`), each named as
  the image is. They can be given together: `juku-asm -lsg demo.asm`.
- `-o` names the image, with any extension, and so the other files too:
  `juku-asm -o build/game.com -g src/main.asm` writes `build/game.com` and
  `build/game.dbg`.
- `-I` adds a folder that `include` and `incbin` look in (Include files,
  below); it can be given more than once.
- The options and the source can come in any order. A file is never
  written over the source or over another output: such a name is an error,
  and nothing is written.

It is a command and a library (`juku_asm::assemble`, and `assemble_with`
for include folders and for source text an editor has not saved), in
`crates/asm`. An error names its file and line apart from what is wrong
(`Error::place`, `Error::message`). For an editor, `juku_asm::index`
assembles a program and lists where each name is defined and used, what
each `include` includes, and what each line places in memory, at what
address and in how many cycles; with an error it still lists the names of the
whole source, as the lines without errors define and use them.
`crates/asm/tests/demos/` has example programs, which `cargo test`
assembles and compares with the outputs beside them.

## Source

```asm
tto     equ 0ffd9h              ; RomBios: print the character in A

        org 100h

main:   lxi h, greeting
.next:  mov a, m
        ora a
        rz
        call tto
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
- A colon alone in the first column is an anonymous label. `:-` is the
  last one before the line, or on it, and `:+` the next one after it; each
  sign more goes one further (`:--`, `:++`). A reference reaches only those
  of its own scope, up to the labels not starting with a dot on either side,
  and of its own file. They are not symbols, start no scope, and in a
  `rept` body each time round has its own:

  ```asm
  count:  mvi b, 8                ; the bits set in A, into C
  :       rlc
          jnc :+
          inr c
  :       dcr b
          jnz :--
          ret
  ```
- Numbers: `123`, `0ffh`, `1010b`, `12d`, `'c'`. Expressions use `+ - * / %
  & | ^ << >>`, `== != < > <= >=` (1 or 0), parentheses, unary `- ~ !`,
  `high(x)`, `low(x)`, `start(s)`, `end(s)` and `size(s)` of a section
  (below), `size(t)` of a struct or a union (below), and `$` for the
  current address. Division rounds down, and by 0 is 0.
- Directives:

  | | |
  |---|---|
  | `name equ expr` | a constant |
  | `name set value` | a number, a string or a list that a later `set` can change (below) |
  | `org expr` | the address in the section; the first `org` outside `bss` is the load address |
  | `db expr or 'string', ...` | bytes |
  | `dw expr, ...` | words, low byte first |
  | `ds count[, fill]` | `count` bytes of 00h or `fill` |
  | `align n` | 00h up to a multiple of `n` |
  | `rept count` ... `endm` | the lines in between, `count` times |
  | `for name, list` ... `endm` | the lines in between for each item of the list (below) |
  | `name macro params` ... `endm` | a macro (below) |
  | `name struct [size]` ... `ends` | a struct: fields' offsets, and a name to lay them out with (below) |
  | `name union [size]` ... `ends` | a union: a struct whose fields all start at 0 (below) |
  | `opt on`, `opt off` | whether expansions leave out what does nothing (below) |
  | `if expr` ... [`elseif expr` ...] ... [`else` ...] `endif` | the lines of the first branch whose condition is not 0, or of `else` |
  | `include 'file' [as name] [(name, ...)]`, `incbin 'file'` | see below |
  | `assert expr[, 'message']` | |
  | `code`, `const`, `data`, `bss` | go on in that section (below) |

- A name can be used above its definition, whatever it depends on. Of a
  source's errors, the one reported is the first in it, so an `assert`
  guards the lines after it.
- Memory ends at FFFFh: a line that would place a byte or reserve one past
  it is an error, and so is a count larger than memory (`ds`, `align`,
  `rept`, a struct's size).

## Sections

`code`, `const`, `data` and `bss` each go on in that section, where it was
left; a program starts in `code`, and one that names no section is all
code. They are laid out in that order, each from where the one before
ends; an `org` in a section places it at an address of its own instead,
and the next one after it.

```asm
        include 'rombios.inc' as rb
        org 100h
main:   lxi b, greeting         ; code
        call rb.ttcon
        lxi h, count
        inr m
        ret

        const                   ; read-only, by convention
greeting:
        db 'Hello', 0

        data                    ; written, with a first value
count:  db 0

        bss                     ; reserved: not in the image
buffer: ds 64
```

- `code`, `const` and `data` are in the image; `bss` only reserves memory
  past it and takes only `ds` with no fill, `align` and structs (below).
  Nothing else is checked: code in `const` and a write to it are the
  program's own business.
- `start(data)`, `end(data)` and `size(data)` are where a section starts,
  where it ends (past its last byte) and its length; `end(data)` is the end
  of the image, and `start(bss)` and `size(bss)` the memory to clear. A
  section with nothing in it starts and ends where it would start.
- `$` is the current address in the current section. `align` pads to an
  address, so it holds wherever the section starts.
- After an `include`, the includer goes on in the section it was in: a
  file can put its strings in `const` and leave the includer in `code`.
- The listing keeps the lines in their order, each at its address.

## Structs

```asm
        include 'ekdos.inc' as dos  ; dos.fcb, a CP/M file control block

point   struct
x       db 0
y       db 0
        ends

sprite  struct
pos     point 10, 20            ; a struct in a struct, with values
name    ds 8, ' '               ; padded with spaces
speed   dw 100h
        ds 2                    ; no name: bytes no value is given for
        ends

        org 100h
main:   lxi d, file
        mvi c, dos.F_OPEN
        call dos.bdos
        lda hero.pos.y          ; a field of the data below
        lxi d, sprite.speed     ; an offset, for a pointer in HL
        dad d
        ret

        data
file    dos.fcb f='HELLO', t='TXT'
hero    sprite [5], 'HERO'      ; pos.x 5, pos.y 20, name 'HERO    '
        sprite speed=0          ; with no label

        bss
foes    ds 4 * size(sprite)
spare   sprite                  ; reserved: spare.pos.x and the rest
```

- `name struct` up to its `ends` defines a struct: a field on each line, a
  name and `db`, `dw`, `ds` or another struct's name, as data is laid out.
  Its bytes are the field's when no value is given for it. A line with no
  name is bytes of the struct that no value can be given for.
- Each field's offset is a symbol under the struct's name: `sprite.name`
  is 2, and `sprite.pos.y` 1. The struct's own name is not a symbol;
  `size(sprite)` is its size, 14. A struct is named under the namespaces
  around it, as a label is (`dos.fcb`), and can be used above its
  definition.
- A struct's name where a mnemonic would be lays it out, as `db` would,
  and a label on the line names each field under it: `hero.pos.y` is the
  address of hero's `pos.y`. In `bss` it only reserves, and takes no
  values; the labels are the same.
- Values are given in the order of the fields, or as `field=value`, after
  those in order; a field left out, or left blank (`, 'X'`), keeps its
  own. A string is padded to its field with the `ds`'s fill, or 00h; a
  number fills the field, low byte first; a struct's value is a list of
  its own values, `[5]` or `[x=5]`. A `dw` field's value is a number,
  even `'c'`.
- `name struct size` makes the struct `size` bytes, padded with 00h, which
  no value can be given for; fields of more bytes are an error. `obj
  struct 16` keeps an object at 16 bytes when a field is added.
- In the debug information, a label of a struct or a field has its size,
  what a watchpoint on it watches, and the offsets are among the `equs`.

### Unions

```asm
lift    struct                  ; goes between top and bottom
top     db 0
bottom  db 0
        ends

params  union                   ; what an object's kind keeps
lift    lift
wait    db 0                    ; a drip: the updates between drops
        ends

obj     struct 8
kind    db 0
k       params                  ; obj.k.lift.top and obj.k.wait are both 1
        ends

        data
cage    obj 1, [lift=[10, 40]]
drip    obj 2, [wait=25]
```

- `name union` up to its `ends` is a struct whose fields all start at 0,
  the same bytes: it is as long as its longest field, or the size given.
  It is used as a struct is, on its own or as a struct's field.
- Where fields overlap, the union's bytes are the first one's when no value
  is given; past it, the next longer one's.
- A value is given to one field only, by name or in order: `[lift=[10,
  40]]`. Values for two of them are an error.

## Macros

```asm
print   macro text              ; print a string kept in const
        lxi b, cstr(text)
        call rb.ttcon
        endm

pushr   macro regs...           ; push each pair, in order
        for r, regs
        push r
        endm
        endm

popr    macro regs...           ; pop them, last first
        for r, reverse(regs)
        pop r
        endm
        endm

main:   pushr b, d, h
        print 'Hello'
        popr b, d, h
        ret
```

- `name macro params` up to its `endm` defines a macro, which a line then
  uses as it would a mnemonic: `print 'Hello'`. A macro is defined above
  its first use, cannot have a mnemonic's or a directive's name, and is
  named under the namespaces around it as a label is (`tx.print`).
- Parameters are names, not registers' names. `row=0` gives one a text for
  an argument left out; one left out with none is blank. The last can be
  `name...`, which takes the rest of the arguments as a list,
  `[b, d, h]`. An argument can have commas in it inside parentheses,
  brackets or quotes.
- Each parameter's name in the body stands for its argument's text,
  outside quotes. An argument means what it would on the line that uses
  the macro: `.done` is the caller's local label, and `foo` the caller's
  `foo` even in a macro whose own file has a `foo`; the listing shows that
  one as `^foo`. `:+` and `:-` cannot be arguments.
- `{name}` joins a parameter's text to what is around it, and
  `{upper(name)}` and `{lower(name)}` change its case first:

  ```asm
  sfx     macro name              ; sfx jump: the effect sfx_jump
          lxi h, sfx_{name}
          call snd_play
          endm

  dirty   macro part              ; dirty oil: S_OIL to set_dirty
          mvi a, S_{upper(part)}
          call set_dirty
          endm
  ```

  The same goes for a `for` name and a string or list `set` gave a name.
  A string's case changes inside its quotes: `{upper(text)}` with `'hi'`
  is `'HI'`.
- An expansion is a scope of its own. Its local labels (`.loop`) and
  anonymous labels are its own, in each expansion: they do not clash with
  another's or with the caller's, and are not symbols. Its other names are
  those of where the macro is defined. A label not starting with a dot is
  a global one, defined twice if the macro is used twice.
- `..name` in a macro is a local label, or a `set`, of the scope the macro
  is used in: `jmp ..done` in a macro used in `draw` is `jmp draw.done`.
  Macros that work together in a routine keep what they share there, each
  routine its own. In a macro used by another macro, it is that one's
  expansion's; out of a macro, it is an error.
- `for name, list` up to its `endm` assembles the lines in between once
  for each item of `list`, with `name` standing for the item, in a macro
  or out of one. A list is items separated by commas, `[a, b, c]`, a
  `name...` parameter, a name `set` made a list, or `reverse(list)`.
- Functions of an argument's text, 1 or 0, for `if` and the rest:

  | | |
  |---|---|
  | `same(x, y)` | the same text but for spaces; a register in any case |
  | `reg(x)`, `pair(x)` | a register (`a`…`l`, `m`), a register pair (`b`, `d`, `h`, `sp`, `psw`) |
  | `const(x)` | a value with every name in it defined above: not a register |
  | `blank(x)` | nothing, as an argument left out, or `[]` |
  | `count(list)` | the number of items |
  | `defined(name)` | a name defined above: a label, a constant or a `set` |

  ```asm
  put     macro r, v=0            ; r = v; for A = 0 the flags change
          if const(v) & same(v, 0) & same(r, a)
          xra a
          else
          mvi r, v
          endif
          endm
  ```
- `cstr('text'[, byte, ...])` puts the string and a 0 after it at the end
  of `const`, and is its address. Each string is there once, and one that
  ends another, as `'lo'` ends `'Hello'`, is in that one. The listing shows
  them after the rest, each at its first use.
- With `opt on`, the default, an expansion leaves out a `mov` of a
  register to itself, which does nothing: `load a, a` with `mov dst, src`
  in the body makes no code. The listing shows the line as `(left out)`.
  Code written as it is never changes; `opt off` keeps expansions as they
  are too, for code whose cycles are counted.
- `name set value` gives a name a value that another `set` can change: a
  macro can keep a count, or what it did, for the next one. The value is
  a number (`x + 1`), a string (`'Hello'`) or a list (`[b, d, h]`), as it
  is written, and the next `set` can give it another kind.
  - A string or a list is text: where the name is, but as a label and in
    `defined()`, the text is put in its place, as a macro's parameter is.
    `msg set 'Hello'` then `db msg, 0`; `regs set [b, d]` then
    `for r, regs`. It is the text when it is set, with a macro's arguments
    in it. A list where a number is needed is an error.
  - A name is used after its first `set` in the source, and has the value
    of the last one above. It is not in the symbols or the debug
    information, and a name `equ` or a label defined cannot be `set`.
- The listing shows an expansion under the line that uses the macro, with
  its cycles added up on that line: only the lines that place something,
  not its `if`, `for` or `set` lines. In the debug information, its code is
  on that line, so a debugger steps over a macro as over an instruction.

### macros.inc

The SDK's `macros.inc`, built in as the other include files are, has
macros for saving register pairs:

```asm
        include 'macros.inc'

work:   save b, d, h            ; push b, push d, push h
        ...
        jz .out
        restore                 ; pop h, pop d, pop b
        ret
.out:   restore
        ret

        pushr b, d              ; push b, push d
        popr b, d               ; pop d, pop b
```

- `pushr` pushes the pairs in order, and `popr`, given the same list, pops
  them the last first.
- `save` pushes them and keeps their list in the routine, as
  `..saved_pairs`; each `restore` after it in that routine pops them the
  last first, so a routine can restore at each of its exits. A `restore` in
  a routine with no `save` is an error. A `save` replaces the routine's one
  before it, so saves do not nest; `pushr` and `popr` do.

  ```asm
  save    macro pairs...
          for r, pairs
          push r
          endm
  ..saved_pairs set pairs
          endm

  restore macro
          assert defined(..saved_pairs), 'restore without a save in its routine'
          for r, reverse(..saved_pairs)
          pop r
          endm
          endm
  ```

## Include files

`include 'name'` and `incbin 'name'` look for the file:

1. beside the file the line is in,
2. in each folder given with `-I`, in order,
3. for `include`, among the SDK's include files, built into `juku-asm`:
   `rombios.inc`, `ekdos.inc`, `ports.inc` and `macros.inc` (the SDK's
   README). A file of
   the same name found earlier is taken instead.

An absolute path is taken as it is. Messages and the listing name a
built-in file as `<juku-sdk>/rombios.inc`.

### Namespaces

A plain `include` defines the file's names as they are, beside the
program's own, so a program cannot also have a `fill` or a `move` of its
own. `as` and a list of names keep them apart:

```asm
        include 'rombios.inc' as rb
        include 'ports.inc' (t2_0, ppi1_c)

        call rb.fill
        in t2_0
        out ports.ppi1_a
        call fill               ; the program's own
        ret

fill:   mvi b, 20
        ret
```

- With `as rb`, every name the file defines is under `rb`: `tto` is
  `rb.tto`, as a local label is under its label.
- With a list, the names in it are defined as they are, and the local
  labels under them too: `(draw)` also gives `draw.loop`. The others are
  under the name given with `as`, or else under the file's name without its
  extension (`ports`); a file whose name is not a name needs `as`. A
  listed name the file does not define is an error.
- In the file, a name is the file's own if the file defines it, else the
  includer's: the file's `helper` before the program's, the program's
  `main` where the file has none. A file it includes with a plain
  `include` is under the same name; one with `as` or a list, under a name of
  its own inside it (`gfx.font.WIDTH`).
- An included file's labels, with `as` or a list or without, do not
  change the includer's scope: a `.next` after the `include` is still
  under the includer's last label.

The listing, the symbols and the debug information name each symbol in full
(`rb.tto`). Every name in a program is defined once, so a full name is
the symbol, whichever file defines it.

## Outputs

- **Image** (`<out.bin>`): the bytes from the load address to the last one
  made, with 00h in the gaps: `code`, `const` and `data`.
- **Listing** (`-l`): each line's address, its first four bytes and, for an
  instruction, its cycle count (the taken count for a conditional call or
  return), then the line.
- **Symbols** (`-s`): every label, constant and struct's field as `name
  1234`, in hex, in order of name (upper case first): not a `set` or a
  macro expansion's own labels.
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
      ["tto",65497]
    ]
  }
  ```

  - Paths are relative to the `.dbg` file.
  - `lines` says which source line each instruction came from: what a
    debugger needs to stop at a line and to show where the program is.
    Only instructions are in it; `data` has the rest. A row is a run of
    instructions that follow each other in memory, from one file:

    ```text
    [address, file, line, length,  step, length,  step, length, ...]
    ```

    - The first four numbers are the run's first instruction: its address,
      its file as an index into `files`, its line (the first line of a
      file is 1) and its length in bytes.
    - Each `step, length` pair after them is the next instruction. It
      starts where the one before it ends, `step` lines further down, and
      is `length` bytes long. `step` is 1 for the next line, more where
      lines with no instruction are between, and 0 for another instruction
      on the same line: a macro's expansion is all on the line that uses
      it, and a one-line `rept` body repeats its line.
    - A new row starts where the next instruction is not right after the
      last one in memory (data or an `org` is between them), is in another
      file, or is on a line above the last one, as the next time round a
      `rept` body of more lines is.

    So a reader starts at the row's address and line, and for each pair
    adds the length before it to the address and the step to the line. The
    row above reads:

    | Address | Line | Bytes | Source |
    |---|---|---|---|
    | 256 | 5 | 3 | `main:   lxi h, greeting` |
    | 259 | 6 | 1 | `.next:  mov a, m` |
    | 260 | 7 | 1 | `ora a` |
    | 261 | 8 | 1 | `rz` |
    | 262 | 9 | 3 | `call tto` |
    | 265 | 10 | 1 | `inx h` |
    | 266 | 11 | 3 | `jmp .next` |

    The other rules, in a program with a macro, a `rept` and a byte of
    data between two instructions:

    ```asm
            org 100h                ; line 1
            include 'macros.inc'
    main:   pushr b, d              ; line 3: push b, push d
            mvi a, 1
            rept 2
            nop                     ; line 6, twice
            endm
            db 0                    ; data: the run ends
            ret                     ; line 9
    ```

    ```json
    "lines": [
      [256,0,3,1,0,1,1,2,2,1,0,1],
      [263,0,9,1]
    ]
    ```

    `push b` is at 256 on line 3 and `push d` on the same line (`0,1`),
    `mvi a, 1` a line further down (`1,2`), the first `nop` two lines
    further (`2,1`) and the second on the same line (`0,1`). The `db 0` at
    262 is not an instruction, so `ret`, at 263, starts a row of its own.
  - `data` is the ranges `db`, `dw`, `ds`, `align` and `incbin` made, joined
    where they meet.
  - `labels` are `[name, address]`, with the size of the line of data or of
    the memory reserved in `bss` at the label as a third field: what a
    watchpoint on it watches. `equs` are `[name, value]`. Both are in order
    of value. Memory reserved in `bss` is named in the labels, not in the
    lines.

  The Juku emulator's debug server, `juku-dap` in juku-emu, reads this
  format.
