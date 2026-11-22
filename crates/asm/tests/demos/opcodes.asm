; opcodes.asm: every documented 8080 instruction, in opcode order, with
; 34h as each byte operand and 1234h as each address. Assembled, it is the
; 244 opcodes from 00h to FFh, without the twelve undocumented ones.

        org 100h

        nop
        lxi  b, 1234h
        stax b
        inx  b
        inr  b
        dcr  b
        mvi  b, 34h
        rlc
        dad  b
        ldax b
        dcx  b
        inr  c
        dcr  c
        mvi  c, 34h
        rrc
        lxi  d, 1234h
        stax d
        inx  d
        inr  d
        dcr  d
        mvi  d, 34h
        ral
        dad  d
        ldax d
        dcx  d
        inr  e
        dcr  e
        mvi  e, 34h
        rar
        lxi  h, 1234h
        shld 1234h
        inx  h
        inr  h
        dcr  h
        mvi  h, 34h
        daa
        dad  h
        lhld 1234h
        dcx  h
        inr  l
        dcr  l
        mvi  l, 34h
        cma
        lxi  sp, 1234h
        sta  1234h
        inx  sp
        inr  m
        dcr  m
        mvi  m, 34h
        stc
        dad  sp
        lda  1234h
        dcx  sp
        inr  a
        dcr  a
        mvi  a, 34h
        cmc
        mov  b, b
        mov  b, c
        mov  b, d
        mov  b, e
        mov  b, h
        mov  b, l
        mov  b, m
        mov  b, a
        mov  c, b
        mov  c, c
        mov  c, d
        mov  c, e
        mov  c, h
        mov  c, l
        mov  c, m
        mov  c, a
        mov  d, b
        mov  d, c
        mov  d, d
        mov  d, e
        mov  d, h
        mov  d, l
        mov  d, m
        mov  d, a
        mov  e, b
        mov  e, c
        mov  e, d
        mov  e, e
        mov  e, h
        mov  e, l
        mov  e, m
        mov  e, a
        mov  h, b
        mov  h, c
        mov  h, d
        mov  h, e
        mov  h, h
        mov  h, l
        mov  h, m
        mov  h, a
        mov  l, b
        mov  l, c
        mov  l, d
        mov  l, e
        mov  l, h
        mov  l, l
        mov  l, m
        mov  l, a
        mov  m, b
        mov  m, c
        mov  m, d
        mov  m, e
        mov  m, h
        mov  m, l
        hlt
        mov  m, a
        mov  a, b
        mov  a, c
        mov  a, d
        mov  a, e
        mov  a, h
        mov  a, l
        mov  a, m
        mov  a, a
        add  b
        add  c
        add  d
        add  e
        add  h
        add  l
        add  m
        add  a
        adc  b
        adc  c
        adc  d
        adc  e
        adc  h
        adc  l
        adc  m
        adc  a
        sub  b
        sub  c
        sub  d
        sub  e
        sub  h
        sub  l
        sub  m
        sub  a
        sbb  b
        sbb  c
        sbb  d
        sbb  e
        sbb  h
        sbb  l
        sbb  m
        sbb  a
        ana  b
        ana  c
        ana  d
        ana  e
        ana  h
        ana  l
        ana  m
        ana  a
        xra  b
        xra  c
        xra  d
        xra  e
        xra  h
        xra  l
        xra  m
        xra  a
        ora  b
        ora  c
        ora  d
        ora  e
        ora  h
        ora  l
        ora  m
        ora  a
        cmp  b
        cmp  c
        cmp  d
        cmp  e
        cmp  h
        cmp  l
        cmp  m
        cmp  a
        rnz
        pop  b
        jnz  1234h
        jmp  1234h
        cnz  1234h
        push b
        adi  34h
        rst  0
        rz
        ret
        jz   1234h
        cz   1234h
        call 1234h
        aci  34h
        rst  1
        rnc
        pop  d
        jnc  1234h
        out  34h
        cnc  1234h
        push d
        sui  34h
        rst  2
        rc
        jc   1234h
        in   34h
        cc   1234h
        sbi  34h
        rst  3
        rpo
        pop  h
        jpo  1234h
        xthl
        cpo  1234h
        push h
        ani  34h
        rst  4
        rpe
        pchl
        jpe  1234h
        xchg
        cpe  1234h
        xri  34h
        rst  5
        rp
        pop  psw
        jp   1234h
        di
        cp   1234h
        push psw
        ori  34h
        rst  6
        rm
        sphl
        jm   1234h
        ei
        cm   1234h
        cpi  34h
        rst  7
