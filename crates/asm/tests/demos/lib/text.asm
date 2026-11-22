; text.asm: included by directives.asm, from the folder it is in.

; Copies the string at HL, ended by a 0, to DE; HL and DE end past it.
copy:   mov a, m
        stax d
        inx h
        inx d
        ora a
        jnz copy
        ret

glyph:  incbin 'glyph.bin'      ; the letter A, 8 rows; relative to this file
.end:
GLYPH_ROWS equ glyph.end - glyph
