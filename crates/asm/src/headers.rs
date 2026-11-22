//! The SDK's include files, `include/` in its repository, built in: an
//! INCLUDE finds one when no file of that name is beside the including file
//! or in an include folder.

/// Each one's name and text.
pub const HEADERS: [(&str, &str); 3] = [
    ("ekdos.inc", include_str!("../../../include/ekdos.inc")),
    ("ports.inc", include_str!("../../../include/ports.inc")),
    ("rombios.inc", include_str!("../../../include/rombios.inc")),
];

/// What a built-in include file's path starts with, in messages and the
/// listing: `<juku-sdk>/rombios.inc`.
pub(crate) const PREFIX: &str = "<juku-sdk>/";

/// The built-in include file `name`'s text.
pub fn header(name: &str) -> Option<&'static str> {
    HEADERS.iter().find(|h| h.0 == name).map(|h| h.1)
}
