; hello.asm: greets, counts from 1 to 9, and goes back to the monitor.
;
; A Juku program at 0100h: load it there and start it with G0100 at the
; monitor's * prompt.

tto     equ 0ffd9h              ; RomBios: print the character in A
monitor equ 0ffc4h              ; RomBios: the monitor's warm start
CR      equ 0dh
LF      equ 0ah

        org 100h

main:   lxi h, greeting
        call print
        mvi b, '1'
.count: mov a, b
        call tto
        mvi a, ' '
        call tto
        inr b
        mov a, b
        cpi '9' + 1
        jnz .count
        lxi h, newline
        call print
        jmp monitor

; Prints the string at HL, ended by a 0.
print:  mov a, m
        ora a
        rz
        call tto
        inx h
        jmp print

greeting:
        db 'Hello, Juku!', CR, LF, 0
newline:
        db CR, LF, 0
