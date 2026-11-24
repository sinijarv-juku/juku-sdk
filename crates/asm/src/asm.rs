//! The passes over the source: lines, conditions and repeats, directives,
//! symbols.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::headers::{PREFIX, header};
use crate::macros::{self, Macro, Macros, Values};
use crate::{encode, expr, path};

mod index;
mod structs;
pub use index::{Definition, Include, Index, NameKind, Placed, Use, index};
use index::Record;
use structs::Struct;

/// An error in the source, or a file that could not be read. Once placed,
/// it shows the file and line first and the line as written last.
#[derive(Debug)]
pub struct Error {
    /// The file, as the assembler names it, and the line in it, from 1.
    place: Option<(String, usize)>,
    /// What is wrong.
    text: String,
    /// The line as written.
    raw: Option<String>,
}

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Error {
        Error { place: None, text: message.into(), raw: None }
    }

    /// The error, placed at `line` unless it is placed already.
    fn at(self, line: &Line) -> Error {
        if self.place.is_some() {
            return self;
        }
        Error { place: Some((line.path.to_string(), line.number)), raw: Some(line.raw.clone()), ..self }
    }

    /// The file the error is in, named as [`Index`] names files, and the
    /// line, from 1. None for an error in no one line: a main file that
    /// cannot be read, a program with no code, sections that never settle.
    pub fn place(&self) -> Option<(&str, usize)> {
        self.place.as_ref().map(|(file, line)| (file.as_str(), *line))
    }

    /// What is wrong, without the file and the line. In a macro's expansion
    /// it is placed at the call, and this names the macro and the line in
    /// it: `in name: file:line: message`.
    pub fn message(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some((file, line)) = &self.place {
            write!(f, "{}:{line}: ", path::shown(file))?;
        }
        f.write_str(&self.text)?;
        if let Some(raw) = &self.raw {
            write!(f, "\n  {raw}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// What a run of bytes from one line is. Code sorts first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Code,
    Data,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SymbolKind {
    Label,
    Equ,
    /// SET: a value that can change, not in the outputs.
    Set,
    /// A struct's field: its offset.
    Field,
}

/// A line of the listing.
pub(crate) struct ListLine {
    pub addr: Option<i64>,
    pub bytes: Vec<u8>,
    pub raw: String,
    /// For an instruction; for a macro's call, its expansion's.
    pub cycles: Option<u32>,
    /// A macro's call, whose expansion follows.
    pub call: bool,
    /// An instruction OPT took out of an expansion.
    pub dropped: bool,
}

/// The bytes one line made.
pub(crate) struct Span {
    pub addr: i64,
    pub len: i64,
    pub path: Rc<str>,
    pub line: usize,
    pub kind: Kind,
}

/// An assembled program.
pub struct Program {
    /// The load address: the first ORG's outside BSS, or else the lowest
    /// one with a byte.
    pub load: i64,
    /// The bytes from the load address to the last one made, with 00h in
    /// the gaps.
    pub image: Vec<u8>,
    pub(crate) symbols: BTreeMap<String, i64>,
    pub(crate) kinds: HashMap<String, SymbolKind>,
    pub(crate) listing: Vec<ListLine>,
    pub(crate) spans: Vec<Span>,
    /// (address, length) of the memory reserved in BSS.
    pub(crate) reserved: Vec<(i64, i64)>,
    /// The size of each label a struct laid out names: what a watchpoint
    /// on it watches.
    pub(crate) sizes: HashMap<String, i64>,
}

/// The sections, in the order they are laid out, each where the one before
/// ends unless an ORG in it places it.
pub(crate) const SECTIONS: [&str; 4] = ["CODE", "CONST", "DATA", "BSS"];
/// The section CSTR() puts its strings in.
const CONST: usize = 1;
/// The section that only reserves memory: not in the image.
const BSS: usize = 3;

/// How to assemble.
#[derive(Default)]
pub struct Options {
    /// The folders INCLUDE and INCBIN look in, in order, for a file that is
    /// not beside the including file.
    pub include: Vec<PathBuf>,
    /// Source files' text to take in place of what is on disk, as an
    /// editor has it before saving, by path. Paths are compared made
    /// absolute, from the current folder, as text: two names for one file
    /// (a link, a drive letter in another case) are two files. INCBIN reads
    /// the disk.
    pub sources: HashMap<PathBuf, String>,
}

/// Assembles the program whose main file is `path`.
pub fn assemble(path: &Path) -> Result<Program> {
    assemble_with(path, &Options::default())
}

/// Assembles the program whose main file is `path`, as `options` say.
pub fn assemble_with(path: &Path, options: &Options) -> Result<Program> {
    let mut asm = Assembler::new(path.to_string_lossy().into(), options);
    asm.run()?;
    asm.program(path)
}

/// The names include file `name` defines, labels and constants but not a
/// struct's fields, with their values, in order of name: found as an INCLUDE in the current folder
/// would find it, so `rombios.inc` is the SDK's own unless there is one
/// here or in an include folder.
pub fn names(name: &str, options: &Options) -> Result<Vec<(String, i64)>> {
    let mut asm = Assembler::new("".into(), options);
    asm.path = asm.find("", name, true);
    asm.run()?;
    let fields: HashSet<&String> = asm.kinds.iter().filter(|(_, k)| **k == SymbolKind::Field).map(|(n, _)| n).collect();
    Ok(asm.symbols.iter().filter(|(n, _)| !fields.contains(n)).map(|(n, v)| (n.clone(), *v)).collect())
}

/// An INCLUDE with AS or a list of names: the file's names are under
/// `prefix`, all but those in `only`.
#[derive(Clone)]
pub(crate) struct Frame {
    prefix: String,
    only: Vec<String>,
}

impl Frame {
    /// `name` is one of `only`, or a local label under one.
    fn lists(&self, name: &str) -> bool {
        self.only.iter().any(|n| name.strip_prefix(n.as_str()).is_some_and(|rest| rest.is_empty() || rest.starts_with('.')))
    }
}

/// A line of a source file.
#[derive(Clone)]
pub(crate) struct Line {
    path: Rc<str>,
    number: usize,
    raw: String,
}

impl Values for Assembler {
    fn value(&mut self, text: &str) -> Result<i64> {
        self.ev(text)
    }

    fn defined(&mut self, name: &str) -> bool {
        self.is_defined(&self.qualify(name))
    }

    fn known(&mut self, text: &str) -> bool {
        let (strict, unresolved) = (std::mem::replace(&mut self.strict, true), self.unresolved);
        let known = self.ev(text).is_ok();
        (self.strict, self.unresolved) = (strict, unresolved);
        known
    }

    fn cstr(&mut self, bytes: Vec<u8>) -> Result<i64> {
        if self.strict {
            return Err(Error::new("not known yet"));
        }
        if !self.cstrs.iter().any(|c| c.0 == bytes) {
            let (path, line) = self.at.clone().unwrap_or_else(|| (self.path.clone(), 0));
            self.cstrs.push((bytes.clone(), path, line));
        }
        match self.last_pool.as_ref().and_then(|pool| pool.get(&bytes)) {
            Some(&addr) => Ok(addr),
            None if self.last_pass => Err(Error::new("a cstr() string never settles at an address")),
            None => {
                self.unresolved = true;
                Ok(0)
            }
        }
    }
}

pub(crate) struct Assembler {
    path: Rc<str>,
    include: Vec<PathBuf>,
    /// Kept from pass to pass.
    pub symbols: BTreeMap<String, i64>,
    kinds: HashMap<String, SymbolKind>,
    pub last_pass: bool,
    /// A symbol was used before it was defined in this pass.
    pub unresolved: bool,
    out: BTreeMap<i64, u8>,
    listing: Vec<ListLine>,
    org: Option<i64>,
    pub here: i64,
    /// The last label not starting with a dot, which local labels go under.
    scope: String,
    /// The INCLUDEs with AS or a list being assembled, outermost first.
    frames: Vec<Frame>,
    /// The file being assembled and the scope in it, as numbered in this
    /// pass: what an anonymous label belongs to.
    file: usize,
    scope_id: usize,
    /// The files and scopes numbered so far in this pass.
    files: usize,
    scopes: usize,
    /// The anonymous labels defined in this pass, in order: their file and
    /// scope, and address. And those of the pass before, which `:+` finds.
    anons: Vec<((usize, usize), i64)>,
    last_anons: Vec<((usize, usize), i64)>,
    defined: HashSet<String>,
    /// The section being assembled (SECTIONS), and each one's address: the
    /// current one's is `here`.
    section: usize,
    heres: [i64; 4],
    /// Where each section starts in this pass, unless an ORG moves it.
    initial: [i64; 4],
    /// The lowest and highest address each one has a label or a byte at.
    extents: [Option<(i64, i64)>; 4],
    /// Each one's start and end in the pass before.
    last_bounds: Option<[(i64, i64); 4]>,
    /// The macros defined so far in this pass, and the names of those of the
    /// pass before.
    macros: Macros,
    last_macros: HashSet<String>,
    /// The scopes and namespaces of the lines that called the expansions this
    /// line is in, innermost last: what `..name` is in.
    callers: Vec<(String, Vec<Frame>)>,
    /// The macro expansions so far in this pass, which name their scopes,
    /// and how many this line is in.
    expansions: usize,
    depth: usize,
    /// The line an expansion's code is on in the debug information: the
    /// outermost call's.
    call: Option<(Rc<str>, usize)>,
    /// The strings and lists SET gave names in this pass, by full name.
    texts: HashMap<String, String>,
    /// The structs, by full name, kept from pass to pass so that one can be
    /// used above its definition, and those defined in this pass.
    structs: HashMap<String, Rc<Struct>>,
    structs_defined: HashSet<String>,
    /// The size of each label a struct laid out names.
    sizes: HashMap<String, i64>,
    /// OPT ON: expansions leave out instructions that do nothing.
    opt: bool,
    /// For CONST(): a name must be defined already in this pass.
    pub strict: bool,
    /// The strings CSTR() asked for in this pass, in order, with the line of
    /// the first; and where each was in CONST in the pass before.
    cstrs: Vec<(Vec<u8>, Rc<str>, usize)>,
    last_pool: Option<HashMap<Vec<u8>, i64>>,
    /// The file and line being assembled.
    at: Option<(Rc<str>, usize)>,
    /// How many INCLUDEs the line is in.
    includes: usize,
    /// The structs whose definitions have an error, kept from pass to pass.
    broken: HashSet<String>,
    spans: Vec<Span>,
    reserved: Vec<(i64, i64)>,
    /// Options::sources, by absolute path.
    sources: HashMap<PathBuf, String>,
    /// For index(): where names are defined and used, in this pass and in
    /// the one before, and whether this pass went to the end.
    recording: bool,
    record: Record,
    last_record: Record,
    pass_done: bool,
    /// The line being assembled as an editor shows it: in an expansion,
    /// the outermost call's. And as written: in an expansion, the macro's.
    shown_at: Option<(Rc<str>, usize)>,
    written_at: Option<(Rc<str>, usize)>,
}

impl Assembler {
    fn new(path: Rc<str>, options: &Options) -> Assembler {
        Assembler {
            path,
            include: options.include.clone(),
            sources: options.sources.iter().map(|(p, text)| (path::absolute(p), text.clone())).collect(),
            recording: false,
            record: Record::default(),
            last_record: Record::default(),
            pass_done: false,
            shown_at: None,
            written_at: None,
            symbols: BTreeMap::new(),
            kinds: HashMap::new(),
            last_pass: false,
            unresolved: false,
            out: BTreeMap::new(),
            listing: Vec::new(),
            org: None,
            here: 0,
            scope: String::new(),
            frames: Vec::new(),
            file: 0,
            scope_id: 0,
            files: 0,
            scopes: 0,
            anons: Vec::new(),
            last_anons: Vec::new(),
            defined: HashSet::new(),
            section: 0,
            heres: [0; 4],
            initial: [0; 4],
            extents: [None; 4],
            last_bounds: None,
            macros: HashMap::new(),
            last_macros: HashSet::new(),
            callers: Vec::new(),
            expansions: 0,
            depth: 0,
            call: None,
            opt: true,
            texts: HashMap::new(),
            structs: HashMap::new(),
            structs_defined: HashSet::new(),
            sizes: HashMap::new(),
            strict: false,
            cstrs: Vec::new(),
            last_pool: None,
            at: None,
            includes: 0,
            broken: HashSet::new(),
            spans: Vec::new(),
            reserved: Vec::new(),
        }
    }

    /// `name` as a symbol: a local label under its scope. Names are
    /// case-sensitive.
    /// In a macro, `..name` is a local label of the caller's scope, in the
    /// caller's namespaces: its full name, marked so that `full` keeps it.
    pub fn qualify(&self, name: &str) -> String {
        if let Some(local) = name.strip_prefix("..")
            && let Some((scope, frames)) = self.callers.last()
        {
            return format!("{CALLERS}{}", full_in(frames, &format!("{scope}.{local}"), frames.len()));
        }
        if name.starts_with('.') && !name.starts_with("..") { format!("{}{name}", self.scope) } else { name.to_string() }
    }

    /// The symbol `name`, qualified, is in the outermost `depth` frames:
    /// under each one's prefix, innermost first, unless it lists the name.
    /// A caller's local is already its full name.
    fn full(&self, name: &str, depth: usize) -> String {
        match name.strip_prefix(CALLERS) {
            Some(full) => full.to_string(),
            None => full_in(&self.frames, name, depth),
        }
    }

    /// The symbol `name`, qualified, defines here.
    fn defining(&self, name: &str) -> String {
        self.full(name, self.frames.len())
    }

    /// The value of the symbol `name`, qualified: looked for in the
    /// innermost frame, then in each one around it, then as it is. A name
    /// SET gives a value is known only once it has one in this pass.
    pub fn lookup(&mut self, name: &str) -> Result<Option<i64>> {
        let found = (0..=self.frames.len()).rev().map(|depth| self.full(name, depth)).find(|n| self.symbols.contains_key(n));
        let Some(full) = found else {
            return Ok(None);
        };
        let name = name.trim_start_matches(CALLERS);
        if let Some(text) = self.texts.get(&full) {
            let kind = if macros::is_list(text) { "a list" } else { "a string" };
            return Err(Error::new(format!("{name} is {kind}, not a number")));
        }
        if self.kinds.get(&full) == Some(&SymbolKind::Set) && !self.defined.contains(&full) {
            if self.last_pass {
                return Err(Error::new(format!("{name} is used before it is set")));
            }
            self.unresolved = true;
            return Ok(Some(0));
        }
        self.note_use(&full);
        Ok(Some(self.symbols[&full]))
    }

    /// The address of the anonymous label `n` away in this file and scope:
    /// `:-` is -1, the last one before this line or on it; `:+` is 1, the
    /// next one after it. Before the last pass one not found is 0, and
    /// notes that the pass has one.
    pub fn anon(&mut self, n: i64) -> Result<i64> {
        let key = (self.file, self.scope_id);
        let found = if n < 0 {
            self.anons.iter().rev().filter(|a| a.0 == key).nth((-n - 1) as usize)
        } else {
            self.last_anons.iter().skip(self.anons.len()).filter(|a| a.0 == key).nth((n - 1) as usize)
        };
        match found {
            Some(&(_, addr)) => Ok(addr),
            None if self.last_pass => {
                let (sign, way) = if n < 0 { ("-", "before") } else { ("+", "after") };
                let reference = format!(":{}", sign.repeat(n.unsigned_abs() as usize));
                Err(Error::new(format!("{reference}: not that many anonymous labels {way} it in its scope")))
            }
            None => {
                self.unresolved = true;
                Ok(0)
            }
        }
    }

    /// START, END or SIZE (`what` 0, 1, 2) of section `section`, as the
    /// pass before laid it out; in the first pass 0, and noted.
    pub fn bound(&mut self, what: usize, section: usize) -> i64 {
        match self.last_bounds {
            Some(bounds) => {
                let (start, end) = bounds[section];
                [start, end, end - start][what]
            }
            None => {
                self.unresolved = true;
                0
            }
        }
    }

    /// Each section's start and end in this pass: from its lowest label or
    /// byte to past its highest, or where it would start if it has none.
    fn bounds(&self) -> [(i64, i64); 4] {
        std::array::from_fn(|s| self.extents[s].unwrap_or((self.initial[s], self.initial[s])))
    }

    /// The program assembled, from main file `path`.
    fn program(self, path: &Path) -> Result<Program> {
        let (Some((&first, _)), Some((&last, _))) = (self.out.first_key_value(), self.out.last_key_value()) else {
            return Err(Error::new(format!("{}: no code", path.display())));
        };
        let load = self.org.unwrap_or(first);
        let image = (load..=last).map(|a| self.out.get(&a).copied().unwrap_or(0)).collect();
        Ok(Program {
            load,
            image,
            symbols: self.symbols,
            kinds: self.kinds,
            listing: self.listing,
            spans: self.spans,
            reserved: self.reserved,
            sizes: self.sizes,
        })
    }

    fn run(&mut self) -> Result<()> {
        for _ in 0..10 {
            self.last_pass = false;
            let before = (self.symbols.clone(), self.last_bounds, self.last_pool.clone());
            self.unresolved = false;
            self.pass()?;
            let after = (&self.symbols, self.last_bounds, &self.last_pool);
            if after == (&before.0, before.1, &before.2) && self.anons == self.last_anons && !self.unresolved {
                break;
            }
        }
        self.last_pass = true;
        let before = self.last_bounds;
        self.pass()?;
        if self.last_bounds != before {
            return Err(Error::new(format!("{}: the sections never settle: where one starts depends on itself", path::shown(&self.path))));
        }
        Ok(())
    }

    /// Goes on in section `section`, where it was left.
    fn switch(&mut self, section: usize) {
        self.heres[self.section] = self.here;
        self.section = section;
        self.here = self.heres[section];
    }

    /// Notes a label or `len` bytes at `addr` in the current section.
    fn place(&mut self, addr: i64, len: i64) {
        let e = &mut self.extents[self.section];
        *e = Some(match *e {
            Some((lo, hi)) => (lo.min(addr), hi.max(addr + len)),
            None => (addr, addr + len),
        });
    }

    fn pass(&mut self) -> Result<()> {
        self.last_record = std::mem::take(&mut self.record);
        self.pass_done = false;
        self.out.clear();
        self.listing.clear();
        self.org = None;
        // Each section after the one before, as the pass before laid it out.
        let last = self.last_bounds.unwrap_or_default();
        self.initial = std::array::from_fn(|s| if s == 0 { 0 } else { last[s - 1].1 });
        self.heres = self.initial;
        self.extents = [None; 4];
        self.section = 0;
        self.here = 0;
        self.scope.clear();
        self.frames.clear();
        (self.file, self.scope_id, self.files, self.scopes) = (0, 0, 0, 0);
        self.last_anons = std::mem::take(&mut self.anons);
        self.defined.clear();
        self.at = None;
        self.spans.clear();
        self.reserved.clear();
        self.last_macros = self.macros.drain().map(|(name, _)| name).collect();
        (self.expansions, self.depth, self.call, self.opt) = (0, 0, None, true);
        self.cstrs.clear();
        self.texts.clear();
        self.structs_defined.clear();
        self.sizes.clear();
        self.source(self.path.clone())?;
        self.place_strings()?;
        self.last_bounds = Some(self.bounds());
        // A name this pass did not define is gone: the next pass does not
        // find it as the one before left it.
        let defined = &self.defined;
        self.symbols.retain(|name, _| defined.contains(name));
        self.kinds.retain(|name, _| defined.contains(name));
        let structs = &self.structs_defined;
        self.structs.retain(|name, _| structs.contains(name));
        self.pass_done = true;
        Ok(())
    }

    /// Puts the strings CSTR() asked for at the end of CONST, each once.
    fn place_strings(&mut self) -> Result<()> {
        if self.cstrs.is_empty() {
            self.last_pool = None;
            return Ok(());
        }
        self.switch(CONST);
        let base = self.here;
        let strings: Vec<Vec<u8>> = self.cstrs.iter().map(|c| c.0.clone()).collect();
        let (bytes, offsets) = macros::pool(&strings);
        // A line of the listing for each string laid out, at its first use:
        // not those inside another one.
        let mut laid: Vec<(i64, usize)> = (0..strings.len())
            .filter(|&i| !strings.iter().any(|t| t.len() > strings[i].len() && t.ends_with(&strings[i])))
            .map(|i| (offsets[&strings[i]], i))
            .collect();
        laid.sort();
        for (o, i) in laid {
            let (s, path, line) = self.cstrs[i].clone();
            self.here = base + o;
            self.at = Some((path, line));
            let raw = format!("        db {}   ; cstr", macros::shown(&s));
            self.emit(s.iter().map(|&b| b as i64).collect(), &raw, None)?;
        }
        self.here = base + bytes.len() as i64;
        self.at = None;
        self.last_pool = Some(offsets.into_iter().map(|(s, o)| (s, base + o)).collect());
        Ok(())
    }


    fn source(&mut self, path: Rc<str>) -> Result<()> {
        let open = || self.sources.get(&path::absolute(Path::new(&*path))).cloned();
        let text = match path.strip_prefix(PREFIX).and_then(header).map(str::to_string).or_else(open) {
            Some(text) => text,
            None => {
                let bytes = std::fs::read(&*path).map_err(|e| Error::new(format!("{}: {path}", path::strerror(&e))))?;
                String::from_utf8(bytes).map_err(|_| Error::new(format!("not UTF-8: {path}")))?
            }
        };
        let lines: Vec<Line> = split_lines(&text)
            .into_iter()
            .enumerate()
            .map(|(n, raw)| Line { path: path.clone(), number: n + 1, raw: raw.to_string() })
            .collect();
        // Its anonymous labels are its own.
        if self.includes >= 32 {
            return Err(Error::new("includes nested more than 32 deep: does a file include itself?"));
        }
        let includer = self.file;
        self.files += 1;
        self.file = self.files;
        self.includes += 1;
        let done = self.block(&lines);
        self.includes -= 1;
        self.file = includer;
        done
    }

    /// Assembles `lines`. Errors are the last pass's: before it, a line with
    /// one makes nothing and the next pass is needed, so the error the last
    /// pass stops at is the first in the source.
    fn block(&mut self, lines: &[Line]) -> Result<()> {
        let mut conds = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            match self.statement(lines, i, &mut conds) {
                Ok(next) => i = next,
                Err(_) if !self.last_pass => {
                    self.unresolved = true;
                    i += 1;
                }
                Err(e) => return Err(e.at(&lines[i])),
            }
        }
        match conds.last() {
            Some(_) if !self.last_pass => {
                self.unresolved = true;
                Ok(())
            }
            Some(&(_, open)) => Err(Error::new("IF without ENDIF").at(&lines[open])),
            None => Ok(()),
        }
    }

    /// Assembles line `i` of `lines`, inside the IFs `conds`, each
    /// (taking, taken, past its ELSE) and the IF's line. Returns the line
    /// to go on with.
    fn statement(&mut self, lines: &[Line], i: usize, conds: &mut Vec<([bool; 3], usize)>) -> Result<usize> {
        let written = (lines[i].path.clone(), lines[i].number);
        self.shown_at = Some(self.call.clone().unwrap_or_else(|| written.clone()));
        self.written_at = Some(written);
        let active = conds.iter().all(|c| c.0[0]);
        // An ELSEIF is the IF's, and weighed if the IF around it is taking.
        let first = strip_comment(&lines[i].raw).split_whitespace().next().unwrap_or("").to_uppercase();
        let outer = conds.len() <= 1 || conds[..conds.len() - 1].iter().all(|c| c.0[0]);
        let weighed = active || (first == "ELSEIF" && outer);
        let mut texts = if weighed && !self.texts.is_empty() { self.with_texts(&lines[i]) } else { None };
        // {text} joins a macro's argument to the names around it.
        let given = texts.as_ref().unwrap_or(&lines[i]);
        let code = strip_comment(&given.raw);
        if weighed && macros::outside_quotes(code).contains('{') {
            let raw = format!("{}{}", macros::join(code)?, &given.raw[code.len()..]);
            texts = Some(Line { raw, ..given.clone() });
        }
        let line = texts.as_ref().unwrap_or(&lines[i]);
        let raw = line.raw.as_str();
        let text = strip_comment(raw).trim();
        let word = text.split_whitespace().next().unwrap_or("").to_uppercase();
        if word == "IF" {
            let taking = active && self.ev(after_chars(text, 2))? != 0;
            conds.push(([taking, taking || !active, false], i));
        } else if word == "ELSEIF" {
            let c = conds.last().ok_or_else(|| Error::new("ELSEIF without IF"))?.0;
            if c[2] {
                return Err(Error::new("ELSEIF after ELSE"));
            }
            let taking = !c[1] && self.ev(after_chars(text, 6))? != 0;
            conds.last_mut().expect("there").0 = [taking, c[1] || taking, false];
        } else if word == "ELSE" {
            let c = &mut conds.last_mut().ok_or_else(|| Error::new("ELSE without IF"))?.0;
            if c[2] {
                return Err(Error::new("ELSE twice"));
            }
            *c = [!c[1], true, true];
        } else if word == "ENDIF" {
            conds.pop().ok_or_else(|| Error::new("ENDIF without IF"))?;
        } else if !active {
            if opens(text).is_some() {
                let end = self.block_end(lines, i)?;
                self.note(raw);
                return Ok(end);
            }
        } else if let Some((label, count)) = rept(text) {
            let end = self.block_end(lines, i)?;
            if let Some(label) = label {
                self.line(&format!("{label}:"), raw, &line.path)?;
            }
            let count = self.ev(count)?;
            if count > 0x10000 {
                return Err(Error::new(format!("REPT {count}: more times than memory has bytes")));
            }
            for _ in 0..count {
                self.block(&lines[i + 1..end - 1])?;
            }
            return Ok(end);
        } else if word == "FOR" {
            return self.for_loop(lines, i, line);
        } else if word == "ENDM" {
            return Err(Error::new("ENDM without REPT, FOR or MACRO"));
        } else if word == "INCLUDE" {
            self.note(raw);
            let (name, frame) = include_operands(after_chars(text, 7))?;
            let path = self.find(&line.path, name, true);
            self.note_include(&path, frame.as_ref().map(|f| f.prefix.as_str()));
            // The includer goes on in its own section, and its scope: the
            // file's labels are not its.
            let section = self.section;
            let scope = (std::mem::take(&mut self.scope), self.scope_id);
            let Some(frame) = frame else {
                let done = self.source(path);
                (self.scope, self.scope_id) = scope;
                done?;
                self.switch(section);
                return Ok(i + 1);
            };
            self.frames.push(frame);
            let done = self.source(path);
            let frame = self.frames.pop().expect("pushed");
            (self.scope, self.scope_id) = scope;
            done?;
            self.switch(section);
            if self.last_pass
                && let Some(n) = frame.only.iter().find(|n| {
                    let full = self.defining(n);
                    !self.defined.contains(&full) && !self.macros.contains_key(&full) && !self.structs_defined.contains(&full)
                })
            {
                return Err(Error::new(format!("{name} does not define {n}")));
            }
            return Ok(i + 1);
        } else {
            self.at = Some(self.call.clone().unwrap_or_else(|| (line.path.clone(), line.number)));
            if let Some(next) = self.struct_line(lines, i, line)? {
                return Ok(next);
            }
            if let Some(next) = self.macro_line(lines, i, line)? {
                return Ok(next);
            }
            self.line(strip_comment(raw).trim_end(), raw, &line.path)?;
            return Ok(i + 1);
        }
        self.note(raw);
        Ok(i + 1)
    }

    /// `line` with each name SET gave a string or a list replaced by it,
    /// but for its label and a name DEFINED() asks about; none if it has
    /// none.
    fn with_texts(&self, line: &Line) -> Option<Line> {
        let code = strip_comment(&line.raw);
        let label = match head(code.trim_end()).0 {
            Some(label) if label != ":" => label.len(),
            _ => 0,
        };
        let mut changed = false;
        let rest = macros::map_names_after(&code[label..], |name, before| {
            // defined(name) asks about the name.
            let asked = before.trim_end().strip_suffix('(').map(str::trim_end).is_some_and(|b| {
                let start = b.len().saturating_sub(7);
                b.get(start..).is_some_and(|w| w.eq_ignore_ascii_case("defined"))
                    && !b[..start].ends_with(|c: char| c.is_alphanumeric() || "_?@.".contains(c))
            });
            if asked {
                return None;
            }
            let qualified = self.qualify(name);
            let text = (0..=self.frames.len()).rev().find_map(|depth| self.texts.get(&self.full(&qualified, depth)).cloned());
            changed |= text.is_some();
            text
        });
        changed.then(|| Line { raw: format!("{}{rest}{}", &code[..label], &line.raw[code.len()..]), ..line.clone() })
    }

    /// Line `i` of `lines`, as `line`, if it defines a macro or calls one:
    /// the line to go on with.
    fn macro_line(&mut self, lines: &[Line], i: usize, line: &Line) -> Result<Option<usize>> {
        let (label, word, rest) = head(strip_comment(&line.raw).trim_end());
        if word.eq_ignore_ascii_case("MACRO") {
            let name = label.filter(|l| is_name(l)).ok_or_else(|| Error::new("MACRO needs a name"))?;
            let upper = name.to_uppercase();
            if encode::is_mnemonic(&upper) || DIRECTIVES.contains(&upper.as_str()) {
                return Err(Error::new(format!("{name} is a mnemonic or a directive")));
            }
            let end = self.block_end(lines, i)?;
            let full = self.defining(name);
            if self.macros.contains_key(&full) || self.structs_defined.contains(&full) {
                return Err(Error::new(format!("{full} defined twice")));
            }
            let m = Macro::new(full.clone(), rest, lines[i + 1..end - 1].to_vec(), self.frames.clone())?;
            self.note_definition(&full, NameKind::Macro);
            self.macros.insert(full, Rc::new(m));
            for l in &lines[i..end] {
                self.note(&l.raw);
            }
            return Ok(Some(end));
        }
        if word.is_empty() {
            return Ok(None);
        }
        let found = (0..=self.frames.len()).rev().map(|d| self.full(word, d)).find(|n| self.macros.contains_key(n));
        let Some(name) = found else {
            if (0..=self.frames.len()).any(|d| self.last_macros.contains(&self.full(word, d))) {
                return Err(Error::new(format!("{word} is used before its definition")));
            }
            return Ok(None);
        };
        if let Some(label) = label {
            self.define_label(label)?;
        }
        self.note_use(&name);
        let m = self.macros[&name].clone();
        self.expand(&m, rest, line)?;
        Ok(Some(i + 1))
    }

    /// Expands macro `m` with the arguments `args`, as written on line
    /// `call`. The expansion is a scope of its own; an argument's local
    /// labels are the caller's.
    fn expand(&mut self, m: &Macro, args: &str, call: &Line) -> Result<()> {
        if self.depth >= 64 {
            return Err(Error::new("macros nested more than 64 deep"));
        }
        let mut texts = Vec::new();
        for a in macros::split_top(args) {
            let bare = macros::outside_quotes(&a);
            if bare.contains(":+") || bare.contains(":-") {
                return Err(Error::new("an anonymous label cannot be a macro's argument"));
            }
            texts.push(macros::map_names(&a, |n| self.resolve(n, &m.frames)));
        }
        let binding = m.bind(&texts)?;
        let body: Vec<Line> = m.body.iter().map(|l| Line { raw: macros::substitute(&l.raw, &binding), ..l.clone() }).collect();

        let at = self.listing.len();
        self.listing.push(ListLine { addr: Some(self.here), bytes: Vec::new(), raw: call.raw.clone(), cycles: None, call: true, dropped: false });
        self.expansions += 1;
        self.scopes += 1;
        self.files += 1;
        let caller = (std::mem::replace(&mut self.scope, format!("#{}", self.expansions)), self.scope_id, self.file);
        // The body's names are those of where the macro is defined.
        let frames = std::mem::replace(&mut self.frames, m.frames.clone());
        self.callers.push((caller.0.clone(), frames.clone()));
        (self.scope_id, self.file) = (self.scopes, self.files);
        let outer = self.call.clone();
        self.call.get_or_insert((call.path.clone(), call.number));
        self.depth += 1;
        let done = self.block(&body);
        self.callers.pop();
        self.depth -= 1;
        self.call = outer;
        (self.scope, self.scope_id, self.file) = caller;
        self.frames = frames;
        // An error is shown at the outermost call, and where it is.
        match done {
            Err(e) if self.depth == 0 => {
                return Err(Error { place: Some((call.path.to_string(), call.number)), text: format!("in {}: {e}", m.name), raw: None });
            }
            Err(e) => return Err(e),
            Ok(()) => {}
        }
        let made = &self.listing[at + 1..];
        if made.iter().any(|l| !l.call && l.cycles.is_some()) {
            self.listing[at].cycles = Some(made.iter().filter(|l| !l.call).filter_map(|l| l.cycles).sum());
        }
        Ok(())
    }

    /// `FOR name, list` on line `i` of `lines`, up to its ENDM: the lines in
    /// between, once for each item, with `name` standing for it. Returns the
    /// line to go on with.
    fn for_loop(&mut self, lines: &[Line], i: usize, line: &Line) -> Result<usize> {
        let text = strip_comment(&line.raw).trim();
        let syntax = || Error::new("FOR takes a name and a list");
        let (name, list) = after_chars(text, 3).split_once(',').ok_or_else(syntax)?;
        let name = name.trim();
        if !is_name(name) || name.contains('.') {
            return Err(syntax());
        }
        let end = self.block_end(lines, i)?;
        self.note(&line.raw);
        for item in macros::list(list) {
            let binding = [(name.to_string(), item)];
            let body: Vec<Line> = lines[i + 1..end - 1].iter().map(|l| Line { raw: macros::substitute(&l.raw, &binding), ..l.clone() }).collect();
            self.block(&body)?;
        }
        Ok(end)
    }

    /// The file an INCLUDE (`headers`) or INCBIN of `name` in file `from`
    /// reads: beside `from`, else in the first include folder that has it,
    /// else, for an INCLUDE, the SDK's include file of that name. A file
    /// found nowhere is the one beside `from`, which then cannot be read.
    fn find(&self, from: &str, name: &str, headers: bool) -> Rc<str> {
        // An editor's text is a file, saved or not; INCBIN reads the disk.
        let there = |path: &Path| path.is_file() || headers && self.sources.contains_key(&path::absolute(path));
        let beside = path::beside(from, name);
        if there(Path::new(&*beside)) || Path::new(name).is_absolute() {
            return beside;
        }
        if let Some(path) = self.include.iter().map(|dir| dir.join(name)).find(|path| there(path)) {
            return path.to_string_lossy().into();
        }
        if headers && header(name).is_some() {
            return format!("{PREFIX}{name}").into();
        }
        beside
    }

    /// The line after the ENDM that closes the REPT, FOR or MACRO on line
    /// `i`.
    fn block_end(&self, lines: &[Line], i: usize) -> Result<usize> {
        let mut depth = 1;
        let mut j = i + 1;
        while j < lines.len() && depth > 0 {
            let text = strip_comment(&lines[j].raw).trim();
            if opens(text).is_some() {
                depth += 1;
            } else if text.eq_ignore_ascii_case("ENDM") {
                depth -= 1;
            }
            j += 1;
        }
        match opens(strip_comment(&lines[i].raw).trim()) {
            Some(kind) if depth > 0 => Err(Error::new(format!("{kind} without ENDM"))),
            _ => Ok(j),
        }
    }

    pub fn ev(&mut self, text: &str) -> Result<i64> {
        let text = macros::functions(text, self)?;
        expr::eval(&text, self)
    }

    /// Name `name` in an argument of a macro defined in `frames`, as the
    /// caller means it: a local label under the caller's scope, and a
    /// symbol by its full name; marked as full (`^name`) where the macro's
    /// namespaces would find another symbol by that name.
    fn resolve(&self, name: &str, frames: &[Frame]) -> Option<String> {
        let qualified = self.qualify(name);
        let full = (0..=self.frames.len()).rev().map(|depth| self.full(&qualified, depth)).find(|n| self.symbols.contains_key(n));
        let found = full.is_some();
        let full = full.unwrap_or(qualified);
        if found && !full.starts_with(CALLERS) {
            let there = (0..=frames.len()).rev().map(|depth| full_in(frames, &full, depth)).find(|n| self.symbols.contains_key(n));
            if there.is_some_and(|t| t != full) {
                return Some(format!("{CALLERS}{full}"));
            }
        }
        (full != name).then_some(full)
    }

    /// How many macro expansions the line is in.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// The symbol `name`, qualified, has been defined in this pass.
    pub fn is_defined(&self, name: &str) -> bool {
        (0..=self.frames.len()).any(|depth| self.defined.contains(&self.full(name, depth)))
    }

    /// Defines `label` at the current address: `:` an anonymous one.
    fn define_label(&mut self, label: &str) -> Result<()> {
        check_caller(label, self.depth)?;
        self.place(self.here, 0);
        if label == ":" {
            self.anons.push(((self.file, self.scope_id), self.here));
            return Ok(());
        }
        if name_len(label) != label.len() {
            return Err(Error::new(format!("not a name: {label}")));
        }
        if !label.starts_with('.') {
            self.scope = label.to_string();
            self.scopes += 1;
            self.scope_id = self.scopes;
        }
        let name = self.defining(&self.qualify(label));
        self.define(name, self.here, SymbolKind::Label)
    }

    fn define(&mut self, name: String, value: i64, kind: SymbolKind) -> Result<()> {
        let was = self.kinds.insert(name.clone(), kind);
        if self.defined.contains(&name) {
            // SET gives a name SET made a value again.
            if kind == SymbolKind::Set && was == Some(SymbolKind::Set) {
                self.note_definition(&name, NameKind::Set);
                self.symbols.insert(name, value);
                return Ok(());
            }
            if let Some(was) = was {
                self.kinds.insert(name.clone(), was);
            }
            return Err(Error::new(format!("{name} defined twice")));
        }
        if kind != SymbolKind::Set && self.last_pass && self.symbols.get(&name).is_some_and(|&v| v != value) {
            return Err(Error::new(format!("{name} never settles: its value depends on itself")));
        }
        let noted = match kind {
            SymbolKind::Label => NameKind::Label,
            SymbolKind::Equ => NameKind::Equ,
            SymbolKind::Set => NameKind::Set,
            SymbolKind::Field => NameKind::Field,
        };
        self.note_definition(&name, noted);
        self.defined.insert(name.clone());
        self.symbols.insert(name, value);
        Ok(())
    }

    /// A line of the listing with no address: not in an expansion, whose
    /// listing has only the lines that place something.
    fn note(&mut self, raw: &str) {
        if self.depth > 0 {
            return;
        }
        self.listing.push(ListLine { addr: None, bytes: Vec::new(), raw: raw.to_string(), cycles: None, call: false, dropped: false });
    }

    /// A line of the listing with an address and no bytes.
    fn mark(&mut self, addr: i64, raw: &str) {
        self.listing.push(ListLine { addr: Some(addr), bytes: Vec::new(), raw: raw.to_string(), cycles: None, call: false, dropped: false });
    }

    /// Puts `data` at the current address: an instruction's bytes if it has
    /// `cycles`, otherwise data.
    fn emit(&mut self, data: Vec<i64>, raw: &str, cycles: Option<u32>) -> Result<()> {
        let start = self.here;
        let len = data.len() as i64;
        if start + len > 0x10000 {
            return Err(Error::new(format!("past FFFFh, the end of memory: {len} bytes at {start:04X}h")));
        }
        self.place(start, len);
        self.note_placed(start, len, cycles);
        if self.section == BSS {
            if data.iter().any(|&b| b != 0) {
                return Err(Error::new("BSS is reserved, not set: DS takes no fill there"));
            }
            if len > 0 {
                self.reserved.push((start, len));
            }
            self.here += len;
            self.mark(start, raw);
            return Ok(());
        }
        let bytes: Vec<u8> = data.iter().map(|&b| b as u8).collect();
        for (i, &b) in bytes.iter().enumerate() {
            self.out.insert(start + i as i64, b);
        }
        if let Some((path, line)) = &self.at
            && len > 0
        {
            let kind = if cycles.is_some() { Kind::Code } else { Kind::Data };
            self.spans.push(Span { addr: start, len, path: path.clone(), line: *line, kind });
        }
        self.here += len;
        self.listing.push(ListLine { addr: Some(start), bytes, raw: raw.to_string(), cycles, call: false, dropped: false });
        Ok(())
    }

    /// Assembles a line with no comment: `raw` is the line as written, and
    /// `path` its file.
    fn line(&mut self, line: &str, raw: &str, path: &str) -> Result<()> {
        if line.trim().is_empty() {
            self.note(raw);
            return Ok(());
        }
        let n = name_len(line);
        let anonymous = line.strip_prefix(':').filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace));
        let (label, rest) = if let Some(rest) = anonymous {
            (None, rest.trim_start())
        } else if n > 0 && line[n..].starts_with(':') {
            (Some(&line[..n]), line[n + 1..].trim_start())
        } else if !line.starts_with(char::is_whitespace) {
            let (label, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            (Some(label), rest.trim())
        } else {
            (None, line.trim())
        };
        let (word, operands) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let mnemonic = word.to_uppercase();
        let operands = operands.trim();
        let ops = split_operands(operands);

        if mnemonic == "EQU" || mnemonic == "SET" {
            let label = label.ok_or_else(|| Error::new(format!("{mnemonic} needs a name")))?;
            check_caller(label, self.depth)?;
            let name = self.defining(&self.qualify(label));
            if mnemonic == "SET" && (macros::is_list(operands) || macros::is_string(operands)) {
                // A string or a list: the text it is.
                self.define(name.clone(), 0, SymbolKind::Set)?;
                self.texts.insert(name, operands.to_string());
                self.note(raw);
                return Ok(());
            }
            let value = self.ev(operands)?;
            let kind = if mnemonic == "SET" { SymbolKind::Set } else { SymbolKind::Equ };
            self.define(name.clone(), value, kind)?;
            self.texts.remove(&name);
            self.note(raw);
            return Ok(());
        }
        // A label on the line is in the section it starts.
        let section = SECTIONS.iter().position(|s| *s == mnemonic);
        if let Some(s) = section {
            if !operands.is_empty() {
                return Err(Error::new(format!("{mnemonic} takes no operands")));
            }
            self.switch(s);
        }
        if anonymous.is_some() {
            self.define_label(":")?;
        }
        if let Some(label) = label {
            self.define_label(label)?;
        }
        // A struct's name lays it out, its fields named under the label.
        if section.is_none()
            && let Some(s) = self.find_struct(word)
        {
            let label = label.map(|l| self.defining(&self.qualify(l)));
            return self.instance(label, &s, operands, raw);
        }
        // OPT: MOV r, r does nothing, and an expansion leaves it out.
        let reg = |r: &str| r.len() == 1 && "abcdehl".contains(&r.to_lowercase());
        if self.depth > 0 && self.opt && mnemonic == "MOV" && ops.len() == 2 && reg(ops[0]) && ops[0].eq_ignore_ascii_case(ops[1]) {
            self.listing.push(ListLine { addr: Some(self.here), bytes: Vec::new(), raw: raw.to_string(), cycles: None, call: false, dropped: true });
            return Ok(());
        }
        match mnemonic.as_str() {
            "" => self.mark(self.here, raw),
            _ if section.is_some() => self.mark(self.here, raw),
            "OPT" => {
                self.opt = match operands.to_uppercase().as_str() {
                    "ON" => true,
                    "OFF" => false,
                    _ => return Err(Error::new("OPT takes ON or OFF")),
                };
                self.note(raw);
            }
            "ORG" => {
                self.here = self.ev(operands)? & 0xFFFF;
                if self.section != BSS {
                    self.org.get_or_insert(self.here);
                }
                self.mark(self.here, raw);
            }
            "DB" | "DW" | "INCBIN" if self.section == BSS => return Err(Error::new("only DS, ALIGN and structs in BSS")),
            "DB" => {
                let mut data = Vec::new();
                for op in ops {
                    if macros::is_string(op) && op.chars().count() != 3 {
                        data.extend(string_bytes(op)?.into_iter().map(i64::from));
                    } else {
                        let v = self.ev(op)?;
                        data.push(self.byte(v)?);
                    }
                }
                return self.emit(data, raw, None);
            }
            "DW" => {
                let mut data = Vec::new();
                for op in ops {
                    let v = self.ev(op)? & 0xFFFF;
                    data.extend([v & 0xFF, v >> 8]);
                }
                return self.emit(data, raw, None);
            }
            "DS" => {
                let count = self.ev(ops.first().ok_or_else(|| Error::new("DS takes a count"))?)?;
                let fill = match ops.get(1) {
                    Some(op) => self.ev(op)?,
                    None => 0,
                };
                return self.emit(vec![fill; memory(count)?], raw, None);
            }
            "ALIGN" => {
                let n = self.ev(operands)?;
                if n <= 0 && self.last_pass {
                    return Err(Error::new("ALIGN takes a number above 0"));
                }
                memory(n)?;
                let pad = if n > 0 { (-self.here).rem_euclid(n) } else { 0 };
                return self.emit(vec![0; pad as usize], raw, None);
            }
            "INCBIN" => {
                let name = self.find(path, unquote(operands)?, false);
                let data = std::fs::read(&*name).map_err(|e| Error::new(format!("{}: {name}", path::strerror(&e))))?;
                return self.emit(data.into_iter().map(i64::from).collect(), raw, None);
            }
            "ASSERT" => {
                if self.last_pass {
                    let cond = ops.first().ok_or_else(|| Error::new("ASSERT takes an expression"))?;
                    if self.ev(cond)? == 0 {
                        let mut message = "assertion failed".to_string();
                        if let Some(text) = ops.get(1) {
                            message = format!("{message}: {}", unquote(text)?);
                        }
                        return Err(Error::new(message));
                    }
                }
                self.note(raw);
            }
            _ => {
                if self.last_pass
                    && let Some(name) = (0..=self.frames.len()).rev().map(|d| self.full(word, d)).find(|n| self.broken.contains(n))
                {
                    return Err(Error::new(format!("{name} is a struct with an error in its definition")));
                }
                let (data, cycles) = encode::encode(self, &mnemonic, &ops)?;
                if self.section == BSS {
                    return Err(Error::new("only DS, ALIGN and structs in BSS"));
                }
                return self.emit(data, raw, Some(cycles));
            }
        }
        Ok(())
    }

    /// `v` as a byte; in the last pass it must be one, signed or not.
    pub fn byte(&self, v: i64) -> Result<i64> {
        if self.last_pass && !(-128..=255).contains(&v) {
            return Err(Error::new(format!("{v} does not fit in a byte")));
        }
        Ok(v & 0xFF)
    }
}

/// `count` bytes, if they fit in memory.
pub(crate) fn memory(count: i64) -> Result<usize> {
    if count > 0x10000 {
        return Err(Error::new(format!("{count} bytes: more than memory holds")));
    }
    Ok(count.max(0) as usize)
}

/// The bytes of `text`, a string in quotes with any quote in it doubled:
/// each character's code, up to 0FFh.
pub(crate) fn string_bytes(text: &str) -> Result<Vec<u8>> {
    let t = text.trim();
    t[1..t.len() - 1]
        .replace("''", "'")
        .chars()
        .map(|c| u8::try_from(u32::from(c)).map_err(|_| Error::new(format!("{c} is not a byte: a string's characters go up to U+00FF"))))
        .collect()
}

/// The length of the name `s` starts with, 0 if none: a letter, `_`, `?`,
/// `@` or `.`, then those or digits.
pub(crate) fn name_len(s: &str) -> usize {
    let mut chars = s.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_ascii_alphabetic() || "_?@.".contains(c) => {}
        _ => return 0,
    }
    chars.find(|&(_, c)| !(c.is_alphanumeric() || "_?@.".contains(c))).map_or(s.len(), |(i, _)| i)
}

/// `[label:] REPT count`: the label and the count.
fn rept(text: &str) -> Option<(Option<&str>, &str)> {
    let n = name_len(text);
    let (label, rest) = match text[n..].strip_prefix(':') {
        Some(rest) if n > 0 => (Some(&text[..n]), rest),
        _ => (None, text),
    };
    let rest = rest.trim_start();
    if !rest.get(..4)?.eq_ignore_ascii_case("REPT") {
        return None;
    }
    let count = &rest[4..];
    count.starts_with(char::is_whitespace).then(|| (label, count.trim_start()))
}

/// The lines of `text`, ended by LF, CR LF or CR.
fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let end = rest.find(['\n', '\r']).unwrap_or(rest.len());
        lines.push(&rest[..end]);
        rest = &rest[end..];
        rest = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix(['\n', '\r'])).unwrap_or(rest);
    }
    lines
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        if c == '\'' {
            quoted = !quoted;
        } else if c == ';' && !quoted {
            return &line[..i];
        }
    }
    line
}

/// Splits on commas outside quotes, parentheses and brackets.
fn split_operands(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut quoted, mut depth) = (false, 0);
    let mut start = 0;
    for (i, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => depth -= 1,
            _ => {}
        }
        if c == ',' && !quoted && depth == 0 {
            out.push(text[start..i].trim());
            start = i + 1;
        }
    }
    let last = text[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

fn unquote(text: &str) -> Result<&str> {
    let text = text.trim();
    match text.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')) {
        Some(inner) => Ok(inner),
        None => Err(Error::new("expected a quoted file name")),
    }
}

/// The directives, which a macro cannot be named.
const DIRECTIVES: [&str; 26] = [
    "ORG", "EQU", "DB", "DW", "DS", "ALIGN", "REPT", "FOR", "MACRO", "ENDM", "IF", "ELSEIF", "ELSE", "ENDIF", "INCLUDE", "INCBIN",
    "ASSERT", "CODE", "CONST", "DATA", "BSS", "OPT", "SET", "STRUCT", "UNION", "ENDS",
];

/// The REPT, FOR or MACRO that line `text` opens, if it opens one.
fn opens(text: &str) -> Option<&'static str> {
    if rept(text).is_some() {
        return Some("REPT");
    }
    if text.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("FOR")) {
        return Some("FOR");
    }
    head(text).1.eq_ignore_ascii_case("MACRO").then_some("MACRO")
}

/// A line's label (`:` for an anonymous one), the word after it and the
/// rest.
fn head(line: &str) -> (Option<&str>, &str, &str) {
    let n = name_len(line);
    let (label, rest) = if line.strip_prefix(':').is_some_and(|r| r.is_empty() || r.starts_with(char::is_whitespace)) {
        (Some(":"), line[1..].trim_start())
    } else if n > 0 && line[n..].starts_with(':') {
        (Some(&line[..n]), line[n + 1..].trim_start())
    } else if !line.is_empty() && !line.starts_with(char::is_whitespace) {
        let (label, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        (Some(label), rest.trim_start())
    } else {
        (None, line.trim())
    };
    let (word, rest) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    (label, word, rest.trim())
}

/// What starts a full name: a macro caller's local (`..name`), or an
/// argument the macro's namespaces would take for another name.
pub(crate) const CALLERS: &str = "^";

/// `name` under the outermost `depth` of `frames`: each one's prefix,
/// innermost first, unless it lists the name.
fn full_in(frames: &[Frame], name: &str, depth: usize) -> String {
    let mut name = name.to_string();
    for frame in frames[..depth].iter().rev() {
        if !frame.lists(&name) {
            name = format!("{}.{name}", frame.prefix);
        }
    }
    name
}

/// `..name` is only in a macro, `depth` expansions deep.
pub(crate) fn check_caller(name: &str, depth: usize) -> Result<()> {
    if name.starts_with("..") && depth == 0 {
        return Err(Error::new(format!("{name} is a macro's caller's local: only in a macro")));
    }
    Ok(())
}

/// `text` is a name to define: not a local label.
pub(crate) fn is_name(text: &str) -> bool {
    !text.starts_with('.') && name_len(text) == text.len() && !text.is_empty()
}

/// INCLUDE's operands, `'file' [AS name] [(name, ...)]`: the file and, with
/// AS or a list, the frame to assemble it in. A list without AS puts the
/// other names under the file's name without its extension.
fn include_operands(text: &str) -> Result<(&str, Option<Frame>)> {
    let quoted = || Error::new("expected a quoted file name");
    let (file, rest) = text.trim_start().strip_prefix('\'').and_then(|t| t.split_once('\'')).ok_or_else(quoted)?;
    let mut rest = rest.trim_start();
    let mut prefix = None;
    if rest.get(..2).is_some_and(|w| w.eq_ignore_ascii_case("AS")) && rest[2..].starts_with(char::is_whitespace) {
        let name = rest[2..].trim_start();
        let n = name_len(name);
        if !is_name(&name[..n]) {
            return Err(Error::new("AS takes a name"));
        }
        prefix = Some(name[..n].to_string());
        rest = name[n..].trim_start();
    }
    let mut only = None;
    if let Some(list) = rest.strip_prefix('(') {
        let (list, after) = list.split_once(')').ok_or_else(|| Error::new("missing )"))?;
        let names: Vec<String> = list.split(',').map(|n| n.trim().to_string()).collect();
        if let Some(n) = names.iter().find(|n| !is_name(n)) {
            return Err(Error::new(format!("not a name to include: {n}")));
        }
        only = Some(names);
        rest = after.trim_start();
    }
    if !rest.is_empty() {
        return Err(Error::new("unexpected text after INCLUDE's file"));
    }
    if prefix.is_none() && only.is_none() {
        return Ok((file, None));
    }
    let prefix = match prefix {
        Some(prefix) => prefix,
        None => {
            let stem = Path::new(file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if !is_name(&stem) {
                return Err(Error::new(format!("{stem} is not a name: give one with AS")));
            }
            stem
        }
    };
    Ok((file, Some(Frame { prefix, only: only.unwrap_or_default() })))
}

/// `text` after its first `n` characters.
fn after_chars(text: &str, n: usize) -> &str {
    text.char_indices().nth(n).map_or("", |(i, _)| &text[i..])
}
