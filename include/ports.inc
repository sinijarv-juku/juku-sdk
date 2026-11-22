; ports.inc: the Juku's I/O ports.
;
; From juku-emu's docs/Juku_machine_description.md, §4 and the sections it
; names: a best reading of the ROMs, documents and schematics. As it reads
; them, the board decodes ports 00h-1Fh, and 80h and 81h are the mouse
; card's, on the bus.

; 8259A interrupt controller (§8)
PIC_0      equ 00h              ; ICW1, OCW2, OCW3
PIC_1      equ 01h              ; ICW2, OCW1: the mask

; 8255A #1: keyboard, sound, tape, the memory map, the picture off (§5)
PPI1_A     equ 04h
PPI1_B     equ 05h
PPI1_C     equ 06h
PPI1_CTL   equ 07h              ; the mode, and port C's bits set and reset

; 8251A USART: RS-232 and the network (§10.1)
USART_DATA equ 08h
USART_CTL  equ 09h              ; control, status

; 8255A #2: the user port, connector X2 (§4)
PPI2_A     equ 0ch
PPI2_B     equ 0dh
PPI2_C     equ 0eh
PPI2_CTL   equ 0fh

; 8253 #1: horizontal timing (§7.2); T1_1 is the horizontal blank
T1_0       equ 10h
T1_1       equ 11h
T1_2       equ 12h
T1_CTL     equ 13h

; 8253 #2: vertical timing (§7.2); T2_1 is the vertical blank (VER RTR)
T2_0       equ 14h
T2_1       equ 15h
T2_2       equ 16h
T2_CTL     equ 17h

; 8253 #3: baud rates and sound (§9); T3_1 is the speaker
T3_0       equ 18h
T3_1       equ 19h
T3_2       equ 1ah
T3_CTL     equ 1bh

; KR1818VG93 (WD1793) floppy disk controller (§10.3)
FDC_CMD    equ 1ch              ; command, status
FDC_TRACK  equ 1dh
FDC_SECTOR equ 1eh
FDC_DATA   equ 1fh

; E4701 mouse card (§10.4)
MOUSE_DATA equ 80h
MOUSE_CTL  equ 81h
