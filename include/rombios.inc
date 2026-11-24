; rombios.inc: RomBios 3.43's entry points and system variables.
;
; From juku-emu's docs/Juku_machine_description.md, §14.2 and §14.3: a
; best reading of the ROM. As it reads it, the entries are callable in
; addressing modes 1 and 2, where the ROM is at D800h-FFFFh; memmd, pmemmd
; and memmdrst in every mode. Keep SP at 8800h or above when calling them.
; Registers marked * are all kept.
;
; juku-emu's debugger names the same addresses (its
; resources/symbols/rombios.dbg, checked against this file).

; Console and keyboard
monitor   equ 0ffc4h            ; the monitor's warm start
monver    equ 0ff6eh            ; D = major + 30h, E = minor + 30h
tti       equ 0ffd3h            ; read a key, no echo -> A
ttio      equ 0ffd6h            ; read a key with echo -> A
ttstat    equ 0ffc7h            ; * Z = 0 and A = code if a key is waiting
constat   equ 0ff98h            ; A = FFh if a key is waiting, else 00h
keybrd    equ 0ff71h            ; scan the keyboard, with the timer interrupt off
tto       equ 0ffd9h            ; * print character A
crlf      equ 0ffcah            ; new line
ttcon     equ 0ffcdh            ; print the string at BC, up to 00h or '$'
ttclf     equ 0ffd0h            ; print the string at BC, then a new line
outh      equ 0ffdch            ; * print A in hex
outh2     equ 0ffdfh            ; * print BC in hex
inhex     equ 0ffe2h            ; * read hex into A; CY = 1 on error
inx2      equ 0ffe5h            ; * read hex into BC; CY = 1 on error
nibble    equ 0ffe8h            ; CY = 0 if A is a hex digit
picton    equ 0ff68h            ; restart the screen blanker; CY = 1 if it was off

; Memory
dmem      equ 0fff1h            ; dump memory BC to HL
move      equ 0fff4h            ; copy BC..HL to DE
comp      equ 0fff7h            ; compare BC..HL with DE
fill      equ 0fffah            ; fill BC..HL with A
cnst      equ 0fffdh            ; search BC..HL for bytes other than A
memmd     equ 0ffb9h            ; select a temporary addressing mode A (0-3)
pmemmd    equ 0ffbch            ; select the permanent addressing mode A
memmdrst  equ 0ffbfh            ; restore the permanent mode
memcopy   equ 0ff83h            ; copy HL bytes from BC to DE; A = modes
ldromprg  equ 0ff9bh            ; load and run the program in ROM

; Printer
printchar equ 0ffeeh            ; print A
hardcopy  equ 0ff77h            ; print the screen

; Sound and timers
bleep     equ 0ff86h            ; A = 100 ms units, DE = 2 000 000 / Hz, 0 = off
bleepvol  equ 0ffb0h            ; A = 0 low volume, else high
intcnt    equ 0ff9eh            ; one-shot timer: DE = 20 ms units
timcnt    equ 0ffa1h            ; stopwatch: E = 0 resets, FFh reads into DE
intrsy    equ 0ff89h            ; hook interrupt level A to DE; old handler in DE

; Graphics: X 0-319, Y 0-239 (40 x 24, 53 x 24); X 0-383, Y 0-199 (64 x 20)
hgr       equ 0ff8ch            ; graphics mode: cursor to 0,0, white
hcolor    equ 0ff8fh            ; E = 0 black, else white
hplot     equ 0ff92h            ; plot a point at X = DE, Y = B
hplott    equ 0ff95h            ; draw a line to X = DE, Y = B

; Network, on the 8251 at 08h; timers in 20 ms units
setparm   equ 0ff26h            ; DE = ROM parameter table
getparm   equ 0ff29h
netstart  equ 0ff2ch
ongroup   equ 0ff2fh
offgroup  equ 0ff32h
mwrite    equ 0ff35h
mreadid   equ 0ff38h
mread     equ 0ff3bh
mignore   equ 0ff3eh
stmout    equ 0ff41h
rtmout    equ 0ff44h

; System variables, as RomBios 3.43m places them
nowaty    equ 0d488h            ; graphics cursor Y
nowatx    equ 0d489h            ; graphics cursor X
funkey    equ 0d4b8h            ; the function key pressed with SHIFT+CTRL
swstate   equ 0d4b9h            ; the keyboard's DIP switches
consw     equ 0d4bah            ; bit 0 CAPS LOCK, bit 1 RUS
conch     equ 0d4bbh            ; console flags; bit 7 = key click off
sqkbsw    equ 0d4bdh            ; bit 0 = a key is pending
sqikey    equ 0d4beh            ; the code of the key pressed, 0 = none
sqhcoder  equ 0d4c0h            ; bits 0-5 row (1-6), bit 6 SHIFT, bit 7 CTRL
sqhcodec  equ 0d4c1h            ; column (1-16)
userend   equ 0d7f7h            ; word: the start of the system area
extsym    equ 0d7f9h            ; word: the font for codes 80h-BFh, 0 = none
latsym    equ 0d7fbh            ; word: the Latin and Estonian font
russym    equ 0d7fdh            ; word: the Cyrillic font
linelen   equ 0d7ffh            ; characters a line: 40, 53 or 64
