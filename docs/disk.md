# juku-disk

Files on EKDOS disk images: lists them, copies them off, puts them on and
removes them, and makes blank disks.

```sh
juku-disk ls IMAGE                              # list the files
juku-disk get IMAGE NAME... [-o FOLDER] [-f]    # copy files off; NAME may use * and ?
juku-disk get IMAGE -a [-o FOLDER] [-f]         # copy every file off
juku-disk put IMAGE FILE...                     # put files on, replacing any of the name
juku-disk rm IMAGE NAME...                      # remove files
juku-disk new IMAGE [--system FROM] [-f]        # a blank disk; with the system of FROM
juku-disk --version
```

```text
$ juku-disk new hello.juk --system ekdos.juk
$ juku-disk put hello.juk hello.com
HELLO.COM 61
$ juku-disk ls hello.juk
hello.juk: 1 file, 4 KB blocks, 780 KB free
HELLO.COM            128
```

- `get` writes into the current folder, or `-o FOLDER`, and leaves a file
  that is there alone unless `-f`. A file keeps its whole last record: a
  text file ends in ^Z padding. A file of user n other than 0 is shown and
  copied off as `n_NAME.EXT`; a `/` in a name, which EKDOS allows, becomes
  `_`.
- `put` names a file after the one it copies, in upper case, as user 0, and
  `rm` removes files by name or pattern. Both change the image in place,
  writing it anew beside it and renaming it over the old one.
- `new` makes a formatted disk with no files. Its system tracks are blank,
  so it cannot be booted, unless `--system FROM` copies them from another
  disk: an EKDOS disk to boot from, or a disk for drive B: without it.
- The image is the first name after the command. The options can come
  anywhere after the command, before the image too: `juku-disk get -a -o
  out game.juk`. After `--`, everything is a name, one starting with `-`
  too.
- A command that cannot do all it was asked says so on standard error and
  exits with 1.

It is a command and a library (`juku_disk::Disk`), in `crates/disk`.

## Disk images

An image is a raw sector dump of the Juku's "D" disk: 80 cylinders of 10
sectors of 512 bytes, double-sided (800 KB, the emulator's `.juk`) or
single-sided (400 KB). The layout, from juku-emu's
[machine description](https://github.com/sinijarv-juku/juku-emu/blob/main/docs/Juku_machine_description.md) (§10.3), is a best reading of the ROMs, the
disks and the documents, and corrections are welcome:

- The image holds the cylinders in order, side 0 before side 1. EKDOS
  numbers its tracks across the sides: logical track t is cylinder t, side
  0, below 80, and cylinder t - 80, side 1, from 80 up.
- Within a logical track, logical sectors 0-9 are sector IDs 1, 3, 5, 7, 9,
  2, 4, 6, 8, 10: EKDOS's reading order. The boot loader's order, 1, 3, 2,
  4, …, reads the same directory sectors in another order, so a reader
  using it lists the files right and scrambles their contents.
- Two system tracks, then the blocks. Block 0, at logical track 2, is the
  directory: 4 KB, 128 entries of 32 bytes laid out as CP/M's, with 8-bit
  block numbers, 16 to an entry.
- EKDOS's double-sided disks have 4 KB blocks, 197 of them: 784 KB for
  files. Its single-sided ones have 2 or 4 KB blocks, and the Juku's CP/M
  2.2 has 2 KB blocks on double-sided disks. `juku-disk` takes the size
  with which the most directory entries have the number of blocks their
  size needs, and 4 KB where that does not decide.

Files are read from all of these, and put on and removed from EKDOS's
double-sided disks only.
