; bounce.asm - a raster bar that bounces up and down the screen.
;
; The bar is the first four text rows of video memory, from D800h, and the
; rest of the screen is cleared. Each frame the program makes the vertical
; blank Y lines longer by writing 72 + Y to T2.1's count (VER RTR). The
; video address counter is loaded with D800h when the blank ends, so the
; bar is drawn Y lines further down, and the cleared memory after it keeps
; the rest of the screen black (MD 7.1, 7.5). The frame and its sync pulses
; stay the same, so the monitor sees nothing unusual.
;
; A count written to a mode 1 counter is loaded at its next trigger, the
; start of the next frame, so the value written after the frame interrupt
; places the next frame. The BIOS keeps running: its frame interrupt still
; scans the keyboard.
;
; Build:  juku-asm -g bounce.asm      (bounce.bin, and bounce.dbg for a debugger)
; Run:    in the Juku emulator, choose bounce.bin as the expansion ROM
;         (Settings, Machine), then type A at the * prompt. Any key goes
;         back to the monitor.
;
; MD n is §n of juku-emu's docs/Juku_machine_description.md.

        include 'rombios.inc' as rb
        include 'ports.inc' as io

ESC     equ 1bh

        org 0100h
        jmp start       ; bootable program header (MD 14.1)
        dw 0dcbah       ; key: not started at power-on
        dw LENGTH       ; bytes the A command copies to 0100h

start:  mvi a, 1        ; mode 1: RAM up to D7FFh; video memory is written,
        call rb.pmemmd  ; never read
        lxi h, setup    ; clear the screen and print the bar
        call print
        lxi h, table

loop:   ei
        hlt             ; sleep until the frame interrupt
        mov a, m        ; the next frame's vertical blank
        out io.t2_1
        inx h
        mov a, m
        out io.t2_1
        inx h
        mov a, l        ; back to the start at the end of the table
        cpi low(tabend)
        jnz key
        mov a, h
        cpi high(tabend)
        jnz key
        lxi h, table

key:    push h
        call rb.constat
        pop h
        ora a
        jz loop

        call rb.tti     ; take the key
        mvi a, 72h      ; the BIOS's 72 lines again
        out io.t2_1
        xra a
        out io.t2_1
        lxi h, restore
        call print
        jmp rb.monitor

; Prints the string at HL, up to a 00h.
print:  mov a, m
        ora a
        rz
        call rb.tto
        inx h
        jmp print

; Four rows of reverse video, 40 characters each, then the cursor off.
setup:  db ESC, 'L', ESC, ''''
        db '                                        '
        db '               JUKU E5101               '
        db '     bouncing on the vertical blank     '
        db '                                        '
        db ESC, '(', ESC, '4', 0

restore:
        db ESC, '5', ESC, 'L', 0

; Vertical blank for each frame, 72 + Y lines in BCD: Y = 100.5 (1 - cos),
; from 0 to 201 (the 241 visible lines less the bar's 40) and back in
; 100 frames, 2 seconds.
table:  dw 0072h, 0072h, 0073h, 0074h, 0075h, 0077h, 0079h, 0082h, 0084h, 0088h
        dw 0091h, 0095h, 0099h, 0104h, 0108h, 0113h, 0119h, 0124h, 0130h, 0136h
        dw 0141h, 0148h, 0154h, 0160h, 0166h, 0172h, 0179h, 0185h, 0191h, 0197h
        dw 0204h, 0209h, 0215h, 0221h, 0226h, 0232h, 0237h, 0241h, 0246h, 0250h
        dw 0254h, 0257h, 0261h, 0263h, 0266h, 0268h, 0270h, 0271h, 0272h, 0273h
        dw 0273h, 0273h, 0272h, 0271h, 0270h, 0268h, 0266h, 0263h, 0261h, 0257h
        dw 0254h, 0250h, 0246h, 0241h, 0237h, 0232h, 0226h, 0221h, 0215h, 0209h
        dw 0204h, 0197h, 0191h, 0185h, 0179h, 0173h, 0166h, 0160h, 0154h, 0148h
        dw 0141h, 0136h, 0130h, 0124h, 0119h, 0113h, 0108h, 0104h, 0099h, 0095h
        dw 0091h, 0088h, 0084h, 0082h, 0079h, 0077h, 0075h, 0074h, 0073h, 0072h
tabend:

LENGTH  equ tabend-0100h
