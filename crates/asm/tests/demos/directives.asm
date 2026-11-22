; directives.asm: every directive, local labels and expressions. Not meant
; to run; its image, listing, symbols and debug information are the test.

DEBUG   equ 0
ROWS    equ 4
WIDTH   equ ROWS * 2 + 1        ; 9
SCREEN  equ 0d800h
AFTER   equ last                ; a label defined further on

        org 100h

start:  jmp main
        dw 0dcbah               ; data among the code
        dw image_end - 100h

main:   lxi h, SCREEN + WIDTH
        mvi a, high(SCREEN)
        mvi b, low(SCREEN + 1234h)
        lxi d, $                ; the address of this instruction
        lxi b, (WIDTH << 8) | ROWS
        mvi c, -1
        mvi a, 'J' - 'A' + 1
        mvi a, ''''
        lxi h, AFTER
        jmp draw.loop           ; a local label from outside its scope

draw:   mvi b, ROWS
.loop:  dcr b
        jnz .loop
        ret

        if DEBUG
        db 'debug'              ; left out
        else
        if ROWS > 2
        db 'rows'
        endif
        endif

        if !DEBUG & (WIDTH == 9)
        nop
        endif

table:  rept ROWS               ; four rows: the offset in the table, two nops
        db $ - table
        rept 2
        nop
        endm
        endm

        rept 0
        db 'never'
        endm

        include 'lib/text.asm'

        align 16
aligned:
        db 1010b, 12d, 0ffh, -128, 'x'
        ds 3                    ; three zeros
        ds 2, 0aah
message:
        db 'It''s a ', '"test"', 0
words:  dw 1, -1, 0ffffh + 2, message
        assert $ < 1000h, 'too big for the test'
        assert GLYPH_ROWS == 8
last:
image_end:

        org 300h                ; a gap of zeros in the image
        db 'end'

        bss                     ; reserved memory: not in the image
buffer: ds 64
.end:
flag:   ds 1
