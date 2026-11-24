# juku-dis

The SDK's 8080 disassembler. It writes a listing, or source that `juku-asm`
assembles back to the same bytes.

```sh
juku-dis [-s] [-i FILE]... [-p] [-b ADDRESS] [-r START-END] IMAGE
juku-dis --version
```

- `IMAGE`'s first byte is at `-b ADDRESS` (or `--base`): 100h, where
  EKDOS loads a COM file, if not given; 0 for a ROM image read at its ROM
  offsets. Addresses are hex: `100`, `0100h` or `0x100`.
- `-r START-END` (or `--range`) is the part to disassemble: from `START`,
  one instruction after another, up to `END`, not included. Either can be
  left out: `-r 120-` runs to the end of the image, `-r -10c` starts at
  its first byte, and `-r 120` is `-r 120-`. Without `-r`, the whole image.
- The options and `IMAGE` can come in any order.
- `-s` writes source: an `org`, then the instructions, with a label
  (`l0103`) for each word operand that is the address of one of them.
  Without it, a listing of addresses, bytes and instructions.
- `-i FILE` names addresses by the names an include file defines:
  `-i rombios.inc` writes `call rb.tto` for `call 0ffd9h`. A name in upper
  case only is a constant, as the SDK names them (`F_OPEN`), and a
  struct's fields are offsets, so neither names an address. The SDK's
  `rombios.inc` and `ekdos.inc` are built in; another file is read from
  where it is given. `-p` (or `--ports`) names the ports of `in` and
  `out` as the SDK's `ports.inc` does: `out io.t2_1`. With `-s`, the
  source includes the files, as the SDK's source does: `rombios.inc` as
  `rb`, `ekdos.inc` as `dos`, `ports.inc` as `io`, and the others as they
  are.
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
$ juku-dis -r 100-10c hello.bin
0100  21 2B 01  lxi h, 012bh
0103  CD 21 01  call 0121h
0106  06 31     mvi b, 31h
0108  78        mov a, b
0109  CD D9 FF  call 0ffd9h
$ juku-dis -s -i rombios.inc -r 100-10c hello.bin
; hello.bin, disassembled by juku-dis 0.2.0

        include 'rombios.inc' as rb

        org 0100h

        lxi h, 012bh
        call 0121h
        mvi b, 31h
        mov a, b
        call rb.tto
```

It is a command and a library (`juku_dis::disassemble`), in `crates/dis`.
