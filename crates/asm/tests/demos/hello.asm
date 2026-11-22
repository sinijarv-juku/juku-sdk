; hello.asm: greets, counts from 1 to 9, and goes back to the monitor.
;
; A Juku program at 0100h: load it there and start it with G0100 at the
; monitor's * prompt.

TTO     equ 0ffd9h              ; RomBios: print the character in A
MONITOR equ 0ffc4h              ; RomBios: the monitor's warm start
CR      equ 0dh
LF      equ 0ah

        org 100h

main:   lxi h, greeting
        call print
        mvi b, '1'
.count: mov a, b
        call TTO
        mvi a, ' '
        call TTO
        inr b
        mov a, b
        cpi '9' + 1
        jnz .count
        lxi h, newline
        call print
        jmp MONITOR

; Prints the string at HL, ended by a 0.
print:  mov a, m
        ora a
        rz
        call TTO
        inx h
        jmp print

greeting:
        db 'Hello, Juku!', CR, LF, 0
newline:
        db CR, LF, 0
