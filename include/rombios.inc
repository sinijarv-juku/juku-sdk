; rombios.inc: RomBios 3.43's entry points and system variables.
;
; From juku-emu's docs/Juku_machine_description.md, §14.2 and §14.3: a
; best reading of the ROM. As it reads it, the entries are callable in
; addressing modes 1 and 2, where the ROM is at D800h-FFFFh; MEMMD, PMEMMD
; and MEMMDRST in every mode. Keep SP at 8800h or above when calling them.
; Registers marked * are all kept.
;
; juku-emu's debugger names the same addresses (its
; resources/symbols/rombios.dbg, checked against this file).

; Console and keyboard
MONITOR   equ 0ffc4h            ; the monitor's warm start
MONVER    equ 0ff6eh            ; D = major + 30h, E = minor + 30h
TTI       equ 0ffd3h            ; read a key, no echo -> A
TTIO      equ 0ffd6h            ; read a key with echo -> A
TTSTAT    equ 0ffc7h            ; * Z = 0 and A = code if a key is waiting
CONSTAT   equ 0ff98h            ; A = FFh if a key is waiting, else 00h
KEYBRD    equ 0ff71h            ; scan the keyboard, with the timer interrupt off
TTO       equ 0ffd9h            ; * print character A
CRLF      equ 0ffcah            ; new line
TTCON     equ 0ffcdh            ; print the string at BC, up to 00h or '$'
TTCLF     equ 0ffd0h            ; print the string at BC, then a new line
OUTH      equ 0ffdch            ; * print A in hex
OUTH2     equ 0ffdfh            ; * print BC in hex
INHEX     equ 0ffe2h            ; * read hex into A; CY = 1 on error
INX2      equ 0ffe5h            ; * read hex into BC; CY = 1 on error
NIBBLE    equ 0ffe8h            ; CY = 0 if A is a hex digit
PICTON    equ 0ff68h            ; restart the screen blanker; CY = 1 if it was off

; Memory
DMEM      equ 0fff1h            ; dump memory BC to HL
MOVE      equ 0fff4h            ; copy BC..HL to DE
COMP      equ 0fff7h            ; compare BC..HL with DE
FILL      equ 0fffah            ; fill BC..HL with A
CNST      equ 0fffdh            ; search BC..HL for bytes other than A
MEMMD     equ 0ffb9h            ; select a temporary addressing mode A (0-3)
PMEMMD    equ 0ffbch            ; select the permanent addressing mode A
MEMMDRST  equ 0ffbfh            ; restore the permanent mode
MEMCOPY   equ 0ff83h            ; copy HL bytes from BC to DE; A = modes
LDROMPRG  equ 0ff9bh            ; load and run the program in ROM

; Printer
PRINTCHAR equ 0ffeeh            ; print A
HARDCOPY  equ 0ff77h            ; print the screen

; Sound and timers
BLEEP     equ 0ff86h            ; A = 100 ms units, DE = 2 000 000 / Hz, 0 = off
BLEEPVOL  equ 0ffb0h            ; A = 0 low volume, else high
INTCNT    equ 0ff9eh            ; one-shot timer: DE = 20 ms units
TIMCNT    equ 0ffa1h            ; stopwatch: E = 0 resets, FFh reads into DE
INTRSY    equ 0ff89h            ; hook interrupt level A to DE; old handler in DE

; Graphics: X 0-319, Y 0-239 (40 x 24, 53 x 24); X 0-383, Y 0-199 (64 x 20)
HGR       equ 0ff8ch            ; graphics mode: cursor to 0,0, white
HCOLOR    equ 0ff8fh            ; E = 0 black, else white
HPLOT     equ 0ff92h            ; plot a point at X = DE, Y = B
HPLOTT    equ 0ff95h            ; draw a line to X = DE, Y = B

; Network, on the 8251 at 08h; timers in 20 ms units
SETPARM   equ 0ff26h            ; DE = ROM parameter table
GETPARM   equ 0ff29h
NETSTART  equ 0ff2ch
ONGROUP   equ 0ff2fh
OFFGROUP  equ 0ff32h
MWRITE    equ 0ff35h
MREADID   equ 0ff38h
MREAD     equ 0ff3bh
MIGNORE   equ 0ff3eh
STMOUT    equ 0ff41h
RTMOUT    equ 0ff44h

; System variables, as RomBios 3.43m places them
NOWATY    equ 0d488h            ; graphics cursor Y
NOWATX    equ 0d489h            ; graphics cursor X
FUNKEY    equ 0d4b8h            ; the function key pressed with SHIFT+CTRL
SWSTATE   equ 0d4b9h            ; the keyboard's DIP switches
CONSW     equ 0d4bah            ; bit 0 CAPS LOCK, bit 1 RUS
CONCH     equ 0d4bbh            ; console flags; bit 7 = key click off
SQKBSW    equ 0d4bdh            ; bit 0 = a key is pending
SQIKEY    equ 0d4beh            ; the code of the key pressed, 0 = none
SQHCODER  equ 0d4c0h            ; bits 0-5 row (1-6), bit 6 SHIFT, bit 7 CTRL
SQHCODEC  equ 0d4c1h            ; column (1-16)
USEREND   equ 0d7f7h            ; word: the start of the system area
EXTSYM    equ 0d7f9h            ; word: the font for codes 80h-BFh, 0 = none
LATSYM    equ 0d7fbh            ; word: the Latin and Estonian font
RUSSYM    equ 0d7fdh            ; word: the Cyrillic font
LINELEN   equ 0d7ffh            ; characters a line: 40, 53 or 64
