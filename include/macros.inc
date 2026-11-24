; macros.inc: general macros for juku-asm programs.
;
; Included as it is (include 'macros.inc'), they are pushr, popr, save and
; restore; save keeps its list for restore in the calling routine's
; .saved_pairs, made with set.

; Pushes the register pairs, in order: pushr b, d, h.
pushr   macro pairs...
        for r, pairs
        push r
        endm
        endm

; Pops the register pairs pushr pushed, given the same way: the last first.
popr    macro pairs...
        for r, reverse(pairs)
        pop r
        endm
        endm

; Pushes the register pairs, as pushr does, and keeps them for restore in
; the routine that saves them.
save    macro pairs...
        for r, pairs
        push r
        endm
..saved_pairs set pairs
        endm

; Pops what save pushed in this routine, the last first: a routine can
; restore at each of its exits. A save replaces the routine's one before it.
restore macro
        assert defined(..saved_pairs), 'restore without a save in its routine'
        for r, reverse(..saved_pairs)
        pop r
        endm
        endm
