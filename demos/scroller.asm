; scroller.asm - a bouncing bar with scrolling text: a Juku raster demo.
;
; The bar is the first 38 lines of video memory: dithered edges that fade
; in towards the middle, and between them a band of text, the ROM font
; drawn at double height, that scrolls right to left a pixel per frame.
; The bar bounces like a ball, with a thump each time it hits the floor.
;
; - The bounce is the trick from bounce.asm: each frame T2.1's count, the
;   vertical blank, is set to 72 + Y lines for the next frame. The video
;   address counter is loaded with D800h when the blank ends, so the bar
;   is drawn Y lines down, and the cleared memory after it keeps the rest
;   of the screen black.
; - The scrolling is done by the CPU: every line of the text band is shifted
;   a bit to the left through the carry, 40 bytes from right to left, and
;   the next column of the current character comes in at the right. That
;   takes about 18 800 cycles, 147 lines' worth, almost half a frame. In
;   mode 1 video memory reads as ROM, so the shift runs in mode 3, with
;   interrupts held off until it is done.
; - The shift is timed so that the beam never meets it: it starts when the
;   beam has passed this frame's text band, which the program finds by
;   reading T2.0, the line counter, and it is done before the next frame's
;   band. Started at the frame interrupt, it would race the beam whenever
;   the bar is in the top third, and the lower text lines would lag a pixel.
;   The frame start is found from T2.0 too: its count falls back at each
;   frame, also when that happens during the shift.
; - When the bar is in the lower half, the shift runs over the start of the
;   next frame. An emulator that copies video memory once a frame, at the
;   start of the vertical blank, catches it half done there and shows the
;   text band with a one-pixel step; the real machine draws the band well
;   after the shift has finished.
;
; Build:  juku-asm -g scroller.asm    (scroller.bin, and scroller.dbg for a debugger)
; Run:    in the Juku emulator, choose scroller.bin as the expansion ROM
;         (Settings, Machine), then type A at the * prompt. Any key goes
;         back to the monitor.
;
; MD n is §n of juku-emu's docs/Juku_machine_description.md.

        include 'rombios.inc' as rb
        include 'ports.inc' as io

ESC     equ 1bh

screen  equ 0d800h
BPL     equ 40          ; bytes per line
BARH    equ 38          ; the bar's height in lines
TEXTTOP equ 10          ; the text band's first line in the bar
TEXTH   equ 18          ; 9 font rows, each drawn twice
VBLANK  equ 72          ; the BIOS's vertical blank, lines
FRAMES  equ 100         ; frames per bounce, entries in the tables

        org 0100h
        jmp start       ; bootable program header (MD 14.1)
        dw 0dcbah       ; key: not started at power-on
        dw LENGTH       ; bytes the A command copies to 0100h

start:  mvi a, 1        ; mode 1: RAM up to D7FFh, ROM above it
        call rb.pmemmd
        lxi h, clear    ; clear the screen, cursor off
        call print
        mvi a, 1
        call rb.bleepvol
        call drawbar
        lhld rb.latsym
        shld font
        call getline
        shld lastln

frame:  call waitfr     ; a frame has started

; The next frame's vertical blank, and the bar's height in this one.
        lda phase
        mov l, a
        mvi h, 0
        push h
        dad h
        lxi d, vbcd
        dad d
        mov a, m
        out io.t2_1
        inx h
        mov a, m
        out io.t2_1
        pop h
        lxi d, ytab
        dad d
        lda nexty       ; this frame shows the value written last time
        sta cury
        mov a, m
        sta nexty
        lda phase       ; entry 0 puts the bar on the floor
        ora a
        cz thump
        lda phase
        inr a
        cpi FRAMES
        jc nowrap
        xra a
nowrap: sta phase

; Every 8 frames, the next character's glyph.
        lda colcnt
        dcr a
        jnz samech
        lhld msgptr
        mov a, m
        ora a
        jnz havech
        lxi h, message  ; back to the start of the text
        mov a, m
havech: inx h
        shld msgptr
        sui 20h         ; the font starts at code 20h, 9 bytes a glyph
        mov l, a
        mvi h, 0
        mov d, h
        mov e, l
        dad h
        dad h
        dad h
        dad d
        xchg
        lhld font
        dad d
        lxi d, glyph
        mvi c, 9
copy:   mov a, m
        stax d
        inx h
        inx d
        dcr c
        jnz copy
        mvi a, 8
samech: sta colcnt

; Wait for the beam to pass this frame's text band, with a line to spare.
        lda cury
        mov c, a
        mvi b, 0
        lxi h, VBLANK+TEXTTOP+TEXTH+1
        dad b
        mov b, h
        mov c, l
        call waitln

; Shift the text band a pixel to the left.
        di
        in io.ppi1_c    ; mode 3, so that video memory can be read
        ori 03h
        out io.ppi1_c
        lxi h, screen+TEXTTOP*BPL+BPL-1
        lxi d, glyph
        mvi c, 9        ; 9 glyph rows, each drawn on two lines
row:    ldax d
        add a           ; the row's next pixel into the carry
        stax d
        inx d
        sbb a           ; A = 00h or FFh: the pixel, kept for both lines
        mov b, a
        ral
        call shl40
        call nextln
        mov a, b
        ral
        call shl40
        call nextln
        dcr c
        jnz row
        in io.ppi1_c    ; mode 1 again
        ani 0fch
        ori 01h
        out io.ppi1_c
        ei

        call rb.constat
        ora a
        jz frame

        call rb.tti     ; take the key
        mvi a, 72h      ; the BIOS's 72 lines again
        out io.t2_1
        xra a
        out io.t2_1
        xra a
        call rb.bleepvol
        lxi h, restore
        call print
        jmp rb.monitor

; HL = lines since the frame started, from T2.0's count.
getline:
        xra a
        out io.t2_ctl   ; latch T2.0
        in io.t2_0
        mov l, a
        in io.t2_0
        mov h, a
        mvi a, low(313) ; 313 - count
        sub l
        mov l, a
        mvi a, high(313)
        sbb h
        mov h, a
        ret

; Waits for the next frame: the line number falls.
waitfr: call getline
        xchg            ; DE = this line
        lhld lastln
        xchg            ; HL = this line, DE = the last one read
        shld lastln
        mov a, l
        sub e
        mov a, h
        sbb d
        jnc waitfr
        ret

; Waits until the line number reaches BC.
waitln: call getline
        shld lastln
        mov a, l
        sub c
        mov a, h
        sbb b
        jc waitln
        ret

; Shifts the 40 bytes ending at HL a bit to the left, the carry coming in
; at the right. Returns with HL on the byte before the line.
shl40:  rept BPL
        mov a, m
        ral
        mov m, a
        dcx h
        endm
        ret

; From the byte before a line to the last byte of the next one.
nextln: push d
        lxi d, 2*BPL
        dad d
        pop d
        ret

thump:  mvi a, 1        ; 100 ms at 150 Hz
        lxi d, 13333
        jmp rb.bleep

; Fills the bar's lines from the pattern table.
drawbar:
        lxi h, screen
        lxi d, pattern
        mvi c, BARH
dbline: ldax d
        inx d
        mvi b, BPL
dbbyte: mov m, a
        inx h
        dcr b
        jnz dbbyte
        dcr c
        jnz dbline
        ret

; Prints the string at HL, up to a 00h.
print:  mov a, m
        ora a
        rz
        call rb.tto
        inx h
        jmp print

clear:  db ESC, 'L', ESC, '4', 0
restore:
        db ESC, '5', ESC, 'L', 0

; One byte per line of the bar. Alternate lines take opposite phases of a
; pattern, so 88h/22h, AAh/55h and BBh/EEh dither to 25, 50 and 75%.
pattern:
        db 88h, 22h, 0aah, 55h, 0bbh, 0eeh, 0ffh, 0ffh, 00h, 00h
        rept TEXTH
        db 00h
        endm
        db 00h, 00h, 0ffh, 0ffh, 0eeh, 0bbh, 55h, 0aah, 22h, 88h

message:
        db '          Juku E5101 raster demo   *   '
        db 'the bar bounces on the vertical blank of the 8253   *   '
        db 'the 8080 shifts every pixel of this text, one step a frame   *   '
        db 'greetings to the Estonian Computer Museum   *   '
        db 'press any key to go back to the monitor   *   ', 0

; A bouncing ball: Y = 203 (1 - sin), from the floor (203, the 241 visible
; lines less the bar's 38) to the top and back in 100 frames, 2 seconds.
; vbcd is the vertical blank for each frame, 72 + Y lines in BCD, and ytab
; is Y.
vbcd:   dw 0275h, 0269h, 0262h, 0256h, 0250h, 0243h, 0237h, 0231h, 0225h, 0218h
        dw 0212h, 0206h, 0200h, 0194h, 0189h, 0183h, 0177h, 0172h, 0166h, 0161h
        dw 0156h, 0151h, 0146h, 0141h, 0136h, 0131h, 0127h, 0123h, 0119h, 0115h
        dw 0111h, 0107h, 0104h, 0100h, 0097h, 0094h, 0091h, 0089h, 0086h, 0084h
        dw 0082h, 0080h, 0078h, 0077h, 0076h, 0074h, 0074h, 0073h, 0072h, 0072h
        dw 0072h, 0072h, 0072h, 0073h, 0074h, 0074h, 0076h, 0077h, 0078h, 0080h
        dw 0082h, 0084h, 0086h, 0089h, 0091h, 0094h, 0097h, 0100h, 0104h, 0107h
        dw 0111h, 0115h, 0119h, 0123h, 0127h, 0131h, 0136h, 0141h, 0146h, 0151h
        dw 0156h, 0161h, 0166h, 0172h, 0177h, 0183h, 0189h, 0194h, 0200h, 0206h
        dw 0212h, 0218h, 0225h, 0231h, 0237h, 0243h, 0250h, 0256h, 0262h, 0269h

ytab:   db 203, 197, 190, 184, 178, 171, 165, 159, 153, 146
        db 140, 134, 128, 122, 117, 111, 105, 100, 94, 89
        db 84, 79, 74, 69, 64, 59, 55, 51, 47, 43
        db 39, 35, 32, 28, 25, 22, 19, 17, 14, 12
        db 10, 8, 6, 5, 4, 2, 2, 1, 0, 0
        db 0, 0, 0, 1, 2, 2, 4, 5, 6, 8
        db 10, 12, 14, 17, 19, 22, 25, 28, 32, 35
        db 39, 43, 47, 51, 55, 59, 64, 69, 74, 79
        db 84, 89, 94, 100, 105, 111, 117, 122, 128, 134
        db 140, 146, 153, 159, 165, 171, 178, 184, 190, 197

phase:  db 0            ; entry to write next
nexty:  db 0            ; Y written for the next frame
cury:   db 0            ; Y of the frame being drawn
colcnt: db 1            ; pixels left of the current character
msgptr: dw message
font:   dw 0
lastln: dw 0            ; the last line number read
glyph:  ds 9            ; rows of the current character, shifted as it goes
progend:

LENGTH  equ progend-0100h
