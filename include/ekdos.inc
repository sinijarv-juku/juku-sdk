; ekdos.inc: EKDOS's page zero, BDOS functions and data structures, for
; programs run from its prompt.
;
; EKDOS is compatible with CP/M: a program is loaded at 0100h and called
; there, and asks for the system's services with call bdos, the function
; in C. The SDK's source includes this file as dos: call dos.bdos.
;
; EKDOS's BDOS and CCP are CP/M 2.2's, which its BIOS patches in memory
; (juku-emu's docs/Juku_machine_description.md, its sources, [EKD]), so the
; functions and structures are CP/M 2.2's, as Digital Research's CP/M 2.2
; Interface Guide gives them. The addresses are from §3.4; the BDOS takes
; functions 0-40 and returns at once for higher numbers (§10.3).
;
; juku-emu's debugger names the same addresses, and the constants and
; fields too (its resources/symbols/ekdos.dbg, checked against this file).

; Page zero
wboot     equ 0000h             ; JMP to the warm boot: the way back to EKDOS
iobyte    equ 0003h             ; the I/O byte, which EKDOS's BIOS does not use
cdisk     equ 0004h             ; the current drive, 0 = A:, and the user in bits 7-4
bdos      equ 0005h             ; JMP to the BDOS: the word at bdos + 1 is where the TPA ends
tfcb      equ 005ch             ; the FCB the CCP fills from the first argument
tfcb2     equ 006ch             ; and from the second
tbuff     equ 0080h             ; the buffer: the command tail, then the DMA buffer

RECORD_SIZE equ 128             ; a record: what a read or a write moves
EOF       equ 1ah               ; ^Z: where text ends in a file's last record

; BDOS functions, in C, named as Digital Research's CP/M 3 manuals name
; the same function (7 and 8 are CP/M 2.2's own). A byte goes in E and
; comes back in A, a word goes in DE and comes back in HL.
P_TERMCPM    equ 0              ; back to EKDOS, as jmp wboot
C_READ       equ 1              ; A: a character from the console, echoed
C_WRITE      equ 2              ; E to the console
A_READ       equ 3              ; A: a character from the reader
A_WRITE      equ 4              ; E to the punch
L_WRITE      equ 5              ; E to the printer
C_RAWIO      equ 6              ; E = 0ffh: A a key or 0, not echoed; else E to the console
GET_IOBYTE   equ 7              ; A: the I/O byte
SET_IOBYTE   equ 8              ; E: the I/O byte
C_WRITESTR   equ 9              ; DE: a string ended by '$', to the console
C_READSTR    equ 10             ; DE: a buffer: its size, then the length and the line read
C_STAT       equ 11             ; A: 0ffh if a key is waiting, else 0
S_BDOSVER    equ 12             ; HL: the version, 0022h
DRV_ALLRESET equ 13             ; every drive reset, A: the current one, the DMA at tbuff
DRV_SET      equ 14             ; E: the drive to make current, 0 = A:
F_OPEN       equ 15             ; DE: an FCB; A: 0ffh if not found
F_CLOSE      equ 16             ; DE: an FCB; A: 0ffh if not found
F_SFIRST     equ 17             ; DE: an FCB, ? for any character; A: 0ffh, or which dirent of the DMA buffer
F_SNEXT      equ 18             ; the next one, as F_SFIRST
F_DELETE     equ 19             ; DE: an FCB, ? for any character; A: 0ffh if none
F_READ       equ 20             ; DE: an FCB; the next record to the DMA, A: 0 if read
F_WRITE      equ 21             ; DE: an FCB; the DMA's record to the next, A: 0 if written
F_MAKE       equ 22             ; DE: an FCB; A: 0ffh if the directory is full
F_RENAME     equ 23             ; DE: an FCB, the new name 16 bytes on, in its d
DRV_LOGINVEC equ 24             ; HL: the drives in use, bit 0 for A:
DRV_GET      equ 25             ; A: the current drive, 0 = A:
F_DMAOFF     equ 26             ; DE: the DMA address, which reads and writes use
DRV_ALLOCVEC equ 27             ; HL: the current drive's allocation vector
DRV_SETRO    equ 28             ; the current drive read-only until a warm boot
DRV_ROVEC    equ 29             ; HL: the read-only drives, bit 0 for A:
F_ATTRIB     equ 30             ; DE: an FCB, its attributes in bit 7 of f and t
DRV_DPB      equ 31             ; HL: the current drive's dpb
F_USERNUM    equ 32             ; E = 0ffh: A the user; else E the user to make current
F_READRAND   equ 33             ; DE: an FCB; record r to the DMA, A: 0 if read
F_WRITERAND  equ 34             ; DE: an FCB; the DMA's record to r, A: 0 if written
F_SIZE       equ 35             ; DE: an FCB; r: the file's size in records
F_RANDREC    equ 36             ; DE: an FCB; r: the record a sequential read is at
DRV_RESET    equ 37             ; DE: the drives to reset, bit 0 for A:
F_WRITEZF    equ 40             ; as F_WRITERAND, with a new block's rest 0

; A file control block: what the file functions take in DE. The CCP fills
; tfcb's from the command's first argument: lda dos.tfcb + dos.fcb.dr.
fcb       struct
dr        db 0                  ; the drive: 0 the current one, 1 A:, 2 B:
f         ds 8, ' '             ; the name, in upper case, padded with spaces
t         ds 3, ' '             ; the type; bit 7 of t + 0 read-only, of t + 1 system
ex        db 0                  ; the extent: 0 to open a file
s1        db 0                  ; the BDOS's
s2        db 0                  ; the BDOS's: 0 to open a file
rc        db 0                  ; the records in the extent
d         ds 16                 ; the BDOS's: the extent's blocks
cr        db 0                  ; the record a sequential read or write is at
r         ds 3                  ; the record for F_READRAND and F_WRITERAND, low byte first
          ends

; A directory entry, as F_SFIRST and F_SNEXT put one in the DMA buffer.
dirent    struct
user      db 0                  ; the user, 0-15; 0e5h for an entry not in use
f         ds 8, ' '
t         ds 3, ' '
ex        db 0
s1        db 0
s2        db 0
rc        db 0
d         ds 16
          ends

; A disk parameter block, as DRV_DPB points to one.
dpb       struct
spt       dw 0                  ; records a track
bsh       db 0                  ; the block's shift: a block is 128 << bsh bytes
blm       db 0                  ; the block's mask, (1 << bsh) - 1
exm       db 0                  ; the extent's mask
dsm       dw 0                  ; the last block's number
drm       dw 0                  ; the last directory entry's number
al0       db 0                  ; the directory's blocks, a bit each, from bit 7 of al0
al1       db 0
cks       dw 0                  ; the directory entries checked for a changed disk
off       dw 0                  ; the system tracks, before the directory
          ends
