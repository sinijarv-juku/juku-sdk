# Demos

Programs for the Juku that show what its video circuit can do, written
with the SDK's tools. Each is a bootable program for the expansion ROM
window: in the Juku emulator, choose its `.bin` as the expansion ROM
(Settings, Machine), then type `A` at the monitor's `*` prompt. Any key
goes back to the monitor.

| Demo | |
|---|---|
| `bounce.asm` | a raster bar that bounces up and down the screen, by changing the vertical blank's length each frame |
| `scroller.asm` | a bouncing bar with text that scrolls a pixel a frame, the CPU shifting video memory behind the beam |

```sh
juku-asm -g bounce.asm         # bounce.bin, and bounce.dbg for the debugger
```

The `.bin` files are what the sources assemble to: the crate
`juku-demos` (`crates/demos`) holds both, for other programs' tests, and
its test checks that they match. After a change to a demo,
`JUKU_ASM_BLESS=1 cargo test -p juku-demos` writes its image anew.
