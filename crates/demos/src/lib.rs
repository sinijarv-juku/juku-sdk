//! The SDK's demo programs, in `demos/`: each one's source and the image it
//! assembles to, which the tests check. Each is a bootable program for the
//! expansion ROM window, started with `A` at the monitor's `*` prompt.

/// A demo program.
pub struct Demo {
    /// Its name: `bounce` for `demos/bounce.asm`.
    pub name: &'static str,
    /// The source, which includes only the SDK's built-in include files.
    pub source: &'static str,
    /// What it assembles to, loaded at 0100h.
    pub image: &'static [u8],
}

/// A raster bar that bounces up and down the screen: a timer write a frame.
pub const BOUNCE: Demo = Demo {
    name: "bounce",
    source: include_str!("../../../demos/bounce.asm"),
    image: include_bytes!("../../../demos/bounce.bin"),
};

/// A bouncing bar with scrolling text: the CPU shifting video memory for
/// half of each frame.
pub const SCROLLER: Demo = Demo {
    name: "scroller",
    source: include_str!("../../../demos/scroller.asm"),
    image: include_bytes!("../../../demos/scroller.bin"),
};

/// Every demo.
pub const DEMOS: [Demo; 2] = [BOUNCE, SCROLLER];
