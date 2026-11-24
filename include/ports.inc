; ports.inc: the Juku's I/O ports.
;
; From juku-emu's docs/Juku_machine_description.md, §4 and the sections it
; names: a best reading of the ROMs, documents and schematics. As it reads
; them, the board decodes ports 00h-1Fh, and 80h and 81h are the mouse
; card's, on the bus.

; 8259A interrupt controller (§8)
pic_0      equ 00h              ; ICW1, OCW2, OCW3
pic_1      equ 01h              ; ICW2, OCW1: the mask

; 8255A #1: keyboard, sound, tape, the memory map, the picture off (§5)
ppi1_a     equ 04h
ppi1_b     equ 05h
ppi1_c     equ 06h
ppi1_ctl   equ 07h              ; the mode, and port C's bits set and reset

; 8251A USART: RS-232 and the network (§10.1)
usart_data equ 08h
usart_ctl  equ 09h              ; control, status

; 8255A #2: the user port, connector X2 (§4)
ppi2_a     equ 0ch
ppi2_b     equ 0dh
ppi2_c     equ 0eh
ppi2_ctl   equ 0fh

; 8253 #1: horizontal timing (§7.2); t1_1 is the horizontal blank
t1_0       equ 10h
t1_1       equ 11h
t1_2       equ 12h
t1_ctl     equ 13h

; 8253 #2: vertical timing (§7.2); t2_1 is the vertical blank (VER RTR)
t2_0       equ 14h
t2_1       equ 15h
t2_2       equ 16h
t2_ctl     equ 17h

; 8253 #3: baud rates and sound (§9); t3_1 is the speaker
t3_0       equ 18h
t3_1       equ 19h
t3_2       equ 1ah
t3_ctl     equ 1bh

; KR1818VG93 (WD1793) floppy disk controller (§10.3)
fdc_cmd    equ 1ch              ; command, status
fdc_track  equ 1dh
fdc_sector equ 1eh
fdc_data   equ 1fh

; E4701 mouse card (§10.4)
mouse_data equ 80h
mouse_ctl  equ 81h
