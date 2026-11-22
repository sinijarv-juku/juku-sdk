; ekdos.inc: EKDOS's fixed addresses, for programs run from its prompt.
;
; From juku-emu's docs/Juku_machine_description.md, §3.4. EKDOS is
; compatible with CP/M: a program is loaded at 0100h and called there, and
; asks for the system's services with CALL BDOS, the function in C.
;
; juku-emu's debugger names the same addresses (its
; resources/symbols/ekdos.dbg, checked against this file).

WBOOT     equ 0000h             ; JMP to the warm boot: the way back to EKDOS
BDOS      equ 0005h             ; JMP to the BDOS
TFCB      equ 005ch             ; the FCB the CCP fills from the first argument
TFCB2     equ 006ch             ; and from the second
TBUFF     equ 0080h             ; the buffer: the command tail, then the DMA buffer
