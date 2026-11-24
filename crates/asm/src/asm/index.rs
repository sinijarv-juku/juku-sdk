//! Where a program's names are defined and used, and the files it
//! includes: what an editor needs to go to a name's definition, find its
//! uses and show its value.

use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use super::{Assembler, CALLERS, Options, Program, Result};

/// What a name is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NameKind {
    Label,
    Equ,
    Set,
    /// A struct's or a union's field: its offset.
    Field,
    Macro,
    Struct,
    Union,
}

/// A name and the line that defines it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    /// Its full name, as the symbol file has it: `draw.loop`, `rb.tto`,
    /// `fcb.dr`.
    pub name: String,
    pub kind: NameKind,
    /// Its value: for a struct or a union its size, for a macro none.
    pub value: Option<i64>,
    /// The file and the line, from 1.
    pub file: String,
    pub line: usize,
}

/// A line that uses a name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Use {
    /// The name's full name, as its [`Definition`] has it.
    pub name: String,
    pub file: String,
    pub line: usize,
}

/// An INCLUDE.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Include {
    /// The file and line of the INCLUDE.
    pub file: String,
    pub line: usize,
    /// The file it includes, named as files are here.
    pub path: String,
    /// The name its names are under: AS's, or with a list of names and no
    /// AS, the file's.
    pub namespace: Option<String>,
}

/// What a line places in memory: in a macro's call, its expansion's; in a
/// REPT, every time's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub file: String,
    pub line: usize,
    /// The address of its first byte, and how many it places (or reserves,
    /// in BSS).
    pub address: i64,
    pub size: i64,
    /// The cycles of its instructions, if it has any: the taken count for a
    /// conditional call or return, as the listing has it.
    pub cycles: Option<u32>,
}

/// A program's names, where each is defined and used, and the files it
/// includes, as [`index`] finds them.
///
/// Files are named as the assembler names them: the main file as given, a
/// file an INCLUDE finds by its folder (the includer's or an include
/// folder) and the name, and a built-in include file
/// [`HEADER_PREFIX`](crate::HEADER_PREFIX) and its name. With an absolute
/// main file and include folders, every other name is absolute too. A line
/// in a macro's expansion is the line of the outermost call: a name the
/// macro uses is used on the line that calls it, and on the macro's own
/// line that has it. Local labels of the expansion itself are left out.
pub struct Index {
    /// The program, or the first error in the source.
    pub program: Result<Program>,
    /// In the order of the source, a SET once for each line that sets it.
    pub definitions: Vec<Definition>,
    pub uses: Vec<Use>,
    pub includes: Vec<Include>,
    /// What each line places, in the order of the source.
    pub placed: Vec<Placed>,
}

/// A file and a line in it.
type Place = (Rc<str>, usize);

/// What one pass found: names with the files and lines.
#[derive(Default)]
pub(crate) struct Record {
    definitions: Vec<(String, NameKind, Place)>,
    uses: Vec<(String, Place)>,
    /// Each INCLUDE's line, the file it includes, and the namespace.
    includes: Vec<(Place, Rc<str>, Option<String>)>,
    /// Bytes placed: the line, the address, the count, the cycles.
    placed: Vec<(Place, i64, i64, Option<u32>)>,
}

/// Assembles the program whose main file is `path`, as [`assemble_with`]
/// does, and finds where its names are defined and used. With an error,
/// the names are those of the whole source as the lines without errors
/// define and use them.
///
/// [`assemble_with`]: crate::assemble_with
pub fn index(path: &Path, options: &Options) -> Index {
    let mut asm = Assembler::new(path.to_string_lossy().into(), options);
    asm.recording = true;
    let ran = asm.run();
    // An error stops the source where it is; the assembling before it went
    // past each line with one.
    let record = if ran.is_ok() || asm.pass_done { std::mem::take(&mut asm.record) } else { std::mem::take(&mut asm.last_record) };

    let mut seen = HashSet::new();
    let definitions = record
        .definitions
        .into_iter()
        .filter(|(name, kind, place)| seen.insert((name.clone(), *kind, place.clone())))
        .map(|(name, kind, (file, line))| {
            let value = match kind {
                NameKind::Macro => None,
                NameKind::Struct | NameKind::Union => asm.structs.get(&name).map(|s| s.size()),
                _ => asm.symbols.get(&name).copied(),
            };
            Definition { name, kind, value, file: file.to_string(), line }
        })
        .collect();
    let mut seen = HashSet::new();
    let uses = record
        .uses
        .into_iter()
        .map(|(name, (file, line))| Use { name, file: file.to_string(), line })
        .filter(|u| seen.insert(u.clone()))
        .collect();
    let mut seen = HashSet::new();
    let includes = record
        .includes
        .into_iter()
        .map(|((file, line), path, namespace)| Include { file: file.to_string(), line, path: path.to_string(), namespace })
        .filter(|i| seen.insert(i.clone()))
        .collect();
    // Each line once: its first address, and all it places.
    let mut placed: Vec<Placed> = Vec::new();
    let mut lines: std::collections::HashMap<Place, usize> = std::collections::HashMap::new();
    for ((file, line), address, size, cycles) in record.placed {
        match lines.get(&(file.clone(), line)) {
            Some(&i) => {
                let p = &mut placed[i];
                p.size += size;
                p.cycles = match (p.cycles, cycles) {
                    (Some(a), Some(b)) => Some(a + b),
                    (a, b) => a.or(b),
                };
            }
            None => {
                lines.insert((file.clone(), line), placed.len());
                placed.push(Placed { file: file.to_string(), line, address, size, cycles });
            }
        }
    }
    let program = ran.and_then(|()| asm.program(path));
    Index { program, definitions, uses, includes, placed }
}

/// A name the source cannot write: a local label of a macro's expansion,
/// or a full name marked as one.
fn hidden(name: &str) -> bool {
    name.contains('#') || name.starts_with(CALLERS)
}

impl Assembler {
    /// Notes that the line being assembled defines `name`, a full name.
    pub(crate) fn note_definition(&mut self, name: &str, kind: NameKind) {
        if !self.recording || hidden(name) {
            return;
        }
        if let Some((file, line)) = &self.shown_at {
            self.record.definitions.push((name.to_string(), kind, (file.clone(), *line)));
        }
    }

    /// Notes that the line being assembled uses `name`, a full name: on the
    /// line an editor shows, and on the macro's line if it is in one.
    pub(crate) fn note_use(&mut self, name: &str) {
        if !self.recording || hidden(name) {
            return;
        }
        for (file, line) in [&self.shown_at, &self.written_at].into_iter().flatten() {
            if !self.record.uses.last().is_some_and(|u| u.0 == name && u.1.0 == *file && u.1.1 == *line) {
                self.record.uses.push((name.to_string(), (file.clone(), *line)));
            }
        }
    }

    /// Notes that the line being assembled places `size` bytes at
    /// `address`, with `cycles` if they are an instruction's.
    pub(crate) fn note_placed(&mut self, address: i64, size: i64, cycles: Option<u32>) {
        if !self.recording {
            return;
        }
        if let Some(at) = &self.at {
            self.record.placed.push((at.clone(), address, size, cycles));
        }
    }

    /// Notes that the line being assembled includes `path`, its names under
    /// `namespace`.
    pub(crate) fn note_include(&mut self, path: &Rc<str>, namespace: Option<&str>) {
        if !self.recording {
            return;
        }
        if let Some((file, line)) = &self.written_at {
            self.record.includes.push(((file.clone(), *line), path.clone(), namespace.map(str::to_string)));
        }
    }
}
