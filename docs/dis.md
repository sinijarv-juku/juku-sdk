# juku-dis

The SDK's 8080 disassembler. It writes a listing, or source that `juku-asm`
assembles back to the same bytes.

```sh
juku-dis IMAGE [START [END]] [--base ADDRESS] [-s] [-i FILE]... [--ports]
juku-dis --version
```

- `IMAGE`'s first byte is at `--base`: 100h, where EKDOS loads a COM file,
  if not given; 0 for a ROM image read at its ROM offsets. The disassembly
  runs from `START`, the first byte if not given, one instruction after
  another, up to `END`, not included, or the end of the image. Addresses
  are hex: `100`, `0100h` or `0x100`.
- `-s` writes source: an `org`, then the instructions, with a label
  (`l0103`) for each word operand that is the address of one of them.
  Without it, a listing of addresses, bytes and instructions.
- `-i FILE` names addresses by the names an include file defines:
  `-i rombios.inc` writes `call TTO` for `call 0ffd9h`. The SDK's
  `rombios.inc` and `ekdos.inc` are built in; another file is read from
  where it is given. `--ports` names the ports of `in` and `out` as the
  SDK's `ports.inc` does. With `-s`, the source includes the files.
- Instructions are written as the SDK's source is, in lower case:
  `lxi h, 012bh`. An undocumented opcode is written as its bytes, `db`, with
  what it does in a comment (`; undocumented: jmp 1234h`), and so is an
  instruction cut off by the end of the image, so that the source
  assembles back to the same bytes.

Data is disassembled as if it were code: the disassembler does not know
where the code is.

The example is the assembler's demo `hello.bin`, in
`crates/asm/tests/demos/`:

```text
$ juku-dis hello.bin 100 10c
0100  21 2B 01  lxi h, 012bh
0103  CD 21 01  call 0121h
0106  06 31     mvi b, 31h
0108  78        mov a, b
0109  CD D9 FF  call 0ffd9h
$ juku-dis hello.bin 100 10c -s -i rombios.inc
; hello.bin, disassembled by juku-dis 0.1.0

        include 'rombios.inc'

        org 0100h

        lxi h, 012bh
        call 0121h
        mvi b, 31h
        mov a, b
        call TTO
```

It is a command and a library (`juku_dis::disassemble`), in `crates/dis`.
