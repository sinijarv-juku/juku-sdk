# juku-lsp

The SDK's language server for `juku-asm` source. An editor starts it and
speaks the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
to it on stdin and stdout. It shows `juku-asm`'s errors while the source
is being typed, before it is saved; goes to the names a program defines,
their definitions, uses and values; and helps write source: completion,
operands, renaming, folding, and each line's address and cycles.

```sh
juku-lsp [--stdio]
juku-lsp --version
```

`--stdio` is taken and changes nothing, since editors often pass it. The
server exits when the editor says `exit`, with 0 after a `shutdown` and 1
otherwise, or 1 when its input ends.

## Editors

The SDK's VS Code extension starts it for `.asm` and `.inc` files
([vscode/README.md](../vscode/README.md)). Another editor runs `juku-lsp`
for those files and passes the settings as initialization options; in
Neovim, for one:

```lua
vim.lsp.config('juku', {
  cmd = { 'juku-lsp' },
  filetypes = { 'asm' },
  root_markers = { '.git' },
  init_options = { inlayHints = true },
})
vim.lsp.enable('juku')
```

## Errors

The errors are `juku-asm`'s, with the same messages, and each is shown on
the line it names, in the file that line is in: an error in an included
file is shown there. As with the command, only the first error in a
program's source is shown; fixing it shows the next one. A program is
assembled again once the editor has sent no change for 200 ms.

- An error in a macro's expansion is shown on the line that calls the
  macro, and its message names the macro and the line in it: `in twice:
  p.asm:2: unknown mnemonic FOO`.
- An error that is in no one line, such as `no code`, is shown on the
  main file's first line.
- An error in one of the SDK's built-in include files is shown on the
  main file's first line, with the file and line in the message.

## Names

A name is matched as the assembler matched it, on its line: `.loop` is the
local label of the label above it, `rb.tto` and `tto` are the name an
`include` defines, `..x` in a macro is the caller's. A file the editor has
changed is assembled again before a question about it is answered.

- **Go to definition** goes to the line that defines the name: a label,
  an `equ` or `set` (each `set` of it), a macro, a struct or union, a
  field. A name from one of the SDK's built-in include files goes to that
  file, written out read-only to `juku-sdk-VERSION` in the system's
  temporary folder.
- **Find references** lists the words that use the name, in every file of
  the programs that have it; a use in a macro's expansion is listed on the
  macro's line that has the name.
- **Hover** shows a name's value (a label's address, an `equ`'s value in
  hex and decimal, a field's offset, a struct's size), its definition's
  line, and the comment lines right above that line. Over a number, its
  value in decimal, hex and binary, and as a character if it is one. Over
  an instruction, its opcode, size and cycles, from its operands (`mov
  a, m`: opcode 7eh, 1 byte, 7 cycles), or of each form of it
  (`mov r, r`: 5 or 7 cycles); a conditional call's or return's are when
  taken, as the listing shows them.
- **The outline** of a file lists the names it defines, each label's local
  labels and each struct's fields under it; a file included under a
  namespace shows its names as it writes them. **The workspace's symbols**
  are every name the programs define, by full name.

Anonymous labels (`:-`, `:+`) and names `set` gives a string or a list
are not names here.

## Writing

- **Completion** offers, where a line's mnemonic goes, the mnemonics, the
  directives and the macros; in its operands, the names the file can write
  there, the registers (for an instruction that takes one), the functions
  (`high`, `size`, `defined`, ...) and the namespaces (`rb`). A word
  starting with a dot is completed with the current label's local labels,
  and one with a dot in it with the names under what is before the dot:
  `rb.` gives `rb.tto` and the others. Mnemonics, directives, registers and
  functions come in lower case, or in upper case if the word is typed so.
  In an `include`'s quotes it offers the SDK's include files and the
  `.asm` and `.inc` files beside the file and in the include folders, two
  folders deep; in an `incbin`'s, the other files.
- **Operands**: while an instruction's or a macro's operands are typed, the
  editor shows what they are (`mvi r, byte`, a macro's parameters) and
  which one the cursor is in.
- **Rename** changes a name's last part (`loop` of `.loop`, `tto` of
  `rb.tto`) in its definitions and every use, however each writes it, and
  in the names under it: renaming `main` changes `main.loop` written in
  full. The new name is a name, not a mnemonic, directive or register, and
  not one defined already. The names of the SDK's include files are not
  renamed, nor a struct's fields, which each label laid out with the struct
  has too.
- **Folding**: `if` to `endif` (each branch on its own), `macro`, `rept` and
  `for` to `endm`, `struct` and `union` to `ends`, and runs of comment
  lines.
- **Addresses and cycles**, with the setting `inlayHints`: after each line
  that places bytes, their address, and the cycles of its instructions or
  else how many bytes. A macro's call has its expansion's; a line in a
  `rept`, every time's together.

## Main files

A program is assembled from its main file, the one `juku-asm` is given,
which includes the others. The server reads every `.asm` and `.inc` file
in the workspace's folders when it starts (not in hidden folders, `target`
or `node_modules`), and finds which file includes which from their
`include` lines. Each `.asm` file that no other file includes is a main
file, and each is assembled. A file is shown with the errors of the main
files that include it.

- A file open in the editor that no main file includes is assembled on its
  own. An `.inc` file assembled so is not said to have no code.
- The editor's text of an open file is assembled, saved or not, and an
  `include` finds an open file that is not saved yet. `incbin` reads the
  disk.
- A file changed on disk, by another program or by saving, is read again
  when the editor reports it: the server asks the editor to report changes
  to `.asm` and `.inc` files, if it can be asked.

## Settings

The editor gives the settings when it starts the server, as
`initializationOptions`, and can change them with
`workspace/didChangeConfiguration` under `juku`:

| Setting | |
|---|---|
| `mainFiles` | The main files, paths absolute or from the first workspace folder. Only these are assembled, with the open files no main file includes. By default, each `.asm` file no other file includes |
| `includePaths` | The folders `include` and `incbin` look in, as `juku-asm`'s `-I`: absolute or from the first workspace folder |
| `inlayHints` | `true` to show each line's address and cycles after it. Off by default |

```json
{ "mainFiles": ["src/game.asm"], "includePaths": ["include"] }
```

## Protocol

The server answers `textDocument/definition`, `references`, `hover`,
`documentSymbol` (nested, or as a list for a client that takes only that),
`completion`, `signatureHelp`, `prepareRename` and `rename`,
`foldingRange` and `inlayHint`, and `workspace/symbol`. A rename that
cannot be done is answered with its reason (`RequestFailed`). Its own
request `juku/mainFiles`, with a `textDocument`, answers the URIs of the
main files whose programs have the document in them, or the document
itself if it is an `.asm` file in none: what to assemble to build it, as
the VS Code extension's Juku: Build does. It takes the whole text of a document with each change
(`textDocumentSync` 1). Positions count UTF-8 bytes when the editor offers
them (`general.positionEncodings`), and UTF-16 code units otherwise. A
diagnostic covers its line's code, without the comment, or the whole line
if it is a comment.

It is a command and a library (`juku_lsp::Server`, the protocol as a state
machine that takes and returns messages as JSON values), in `crates/lsp`.
