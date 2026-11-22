//! The passes over the source: lines, conditions and repeats, directives,
//! symbols.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::headers::{PREFIX, header};
use crate::{encode, expr, path};

/// An error in the source, or a file that could not be read. Once placed,
/// it starts with the file and line and ends with the line.
#[derive(Debug)]
pub struct Error {
    message: String,
    located: bool,
}

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Error {
        Error { message: message.into(), located: false }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.message)
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
}

/// A line of the listing.
pub(crate) struct ListLine {
    pub addr: Option<i64>,
    pub bytes: Vec<u8>,
    pub raw: String,
    /// For an instruction.
    pub cycles: Option<u32>,
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
    /// The load address: the first ORG's, or else the lowest one with a
    /// byte.
    pub load: i64,
    /// The bytes from the load address to the last one made, with 00h in
    /// the gaps.
    pub image: Vec<u8>,
    pub(crate) symbols: BTreeMap<String, i64>,
    pub(crate) kinds: HashMap<String, SymbolKind>,
    pub(crate) listing: Vec<ListLine>,
    pub(crate) spans: Vec<Span>,
    /// (address, length) of the memory reserved after BSS.
    pub(crate) reserved: Vec<(i64, i64)>,
}

/// How to assemble.
#[derive(Default)]
pub struct Options {
    /// The folders INCLUDE and INCBIN look in, in order, for a file that is
    /// not beside the including file.
    pub include: Vec<PathBuf>,
}

/// Assembles the program whose main file is `path`.
pub fn assemble(path: &Path) -> Result<Program> {
    assemble_with(path, &Options::default())
}

/// Assembles the program whose main file is `path`, as `options` say.
pub fn assemble_with(path: &Path, options: &Options) -> Result<Program> {
    let mut asm = Assembler::new(path.to_string_lossy().into(), options.include.clone());
    asm.run()?;
    let (Some((&first, _)), Some((&last, _))) = (asm.out.first_key_value(), asm.out.last_key_value()) else {
        return Err(Error::new(format!("{}: no code", path.display())));
    };
    let load = asm.org.unwrap_or(first);
    let image = (load..=last).map(|a| asm.out.get(&a).copied().unwrap_or(0)).collect();
    Ok(Program {
        load,
        image,
        symbols: asm.symbols,
        kinds: asm.kinds,
        listing: asm.listing,
        spans: asm.spans,
        reserved: asm.reserved,
    })
}

/// The names include file `name` defines, labels and constants, with their
/// values, in order of name: found as an INCLUDE in the current folder
/// would find it, so `rombios.inc` is the SDK's own unless there is one
/// here or in an include folder.
pub fn names(name: &str, options: &Options) -> Result<Vec<(String, i64)>> {
    let mut asm = Assembler::new("".into(), options.include.clone());
    asm.path = asm.find("", name, true);
    asm.run()?;
    Ok(asm.symbols.into_iter().collect())
}

/// A line of a source file.
struct Line {
    path: Rc<str>,
    number: usize,
    raw: String,
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
    defined: HashSet<String>,
    bss: bool,
    /// The file and line being assembled.
    at: Option<(Rc<str>, usize)>,
    spans: Vec<Span>,
    reserved: Vec<(i64, i64)>,
}

impl Assembler {
    fn new(path: Rc<str>, include: Vec<PathBuf>) -> Assembler {
        Assembler {
            path,
            include,
            symbols: BTreeMap::new(),
            kinds: HashMap::new(),
            last_pass: false,
            unresolved: false,
            out: BTreeMap::new(),
            listing: Vec::new(),
            org: None,
            here: 0,
            scope: String::new(),
            defined: HashSet::new(),
            bss: false,
            at: None,
            spans: Vec::new(),
            reserved: Vec::new(),
        }
    }

    /// `name` as a symbol: a local label under its scope. Names are
    /// case-sensitive.
    pub fn qualify(&self, name: &str) -> String {
        if name.starts_with('.') { format!("{}{name}", self.scope) } else { name.to_string() }
    }

    fn run(&mut self) -> Result<()> {
        for _ in 0..10 {
            self.last_pass = false;
            let before = self.symbols.clone();
            self.unresolved = false;
            self.pass()?;
            if self.symbols == before && !self.unresolved {
                break;
            }
        }
        self.last_pass = true;
        self.pass()
    }

    fn pass(&mut self) -> Result<()> {
        self.out.clear();
        self.listing.clear();
        self.org = None;
        self.here = 0;
        self.scope.clear();
        self.defined.clear();
        self.bss = false;
        self.at = None;
        self.spans.clear();
        self.reserved.clear();
        self.source(self.path.clone())
    }

    fn source(&mut self, path: Rc<str>) -> Result<()> {
        let text = match path.strip_prefix(PREFIX).and_then(header) {
            Some(text) => text.to_string(),
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
        self.block(&lines)
    }

    fn block(&mut self, lines: &[Line]) -> Result<()> {
        let mut conds = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            match self.statement(lines, i, &mut conds) {
                Ok(next) => i = next,
                Err(e) if e.located => return Err(e),
                Err(e) => {
                    let line = &lines[i];
                    let message = format!("{}:{}: {}\n  {}", path::shown(&line.path), line.number, e, line.raw);
                    return Err(Error { message, located: true });
                }
            }
        }
        match lines.last() {
            Some(line) if !conds.is_empty() => Err(Error::new(format!("{}: IF without ENDIF", path::shown(&line.path)))),
            _ => Ok(()),
        }
    }

    /// Assembles line `i` of `lines`, inside the IFs `conds`, each
    /// (taking, taken). Returns the line to go on with.
    fn statement(&mut self, lines: &[Line], i: usize, conds: &mut Vec<[bool; 2]>) -> Result<usize> {
        let line = &lines[i];
        let raw = line.raw.as_str();
        let text = strip_comment(raw).trim();
        let word = text.split_whitespace().next().unwrap_or("").to_uppercase();
        let active = conds.iter().all(|c| c[0]);
        if word == "IF" {
            let taking = active && self.ev(after_chars(text, 2))? != 0;
            conds.push([taking, taking || !active]);
        } else if word == "ELSE" {
            let c = conds.last_mut().ok_or_else(|| Error::new("ELSE without IF"))?;
            *c = [!c[1], true];
        } else if word == "ENDIF" {
            conds.pop().ok_or_else(|| Error::new("ENDIF without IF"))?;
        } else if !active {
            if rept(text).is_some() {
                let end = self.rept_end(lines, i)?;
                self.note(raw);
                return Ok(end);
            }
        } else if let Some((label, count)) = rept(text) {
            let end = self.rept_end(lines, i)?;
            if let Some(label) = label {
                self.line(&format!("{label}:"), raw, &line.path)?;
            }
            for _ in 0..self.ev(count)? {
                self.block(&lines[i + 1..end - 1])?;
            }
            return Ok(end);
        } else if word == "ENDM" {
            return Err(Error::new("ENDM without REPT"));
        } else if word == "INCLUDE" {
            self.note(raw);
            let name = unquote(text.split_once(char::is_whitespace).map_or("", |(_, name)| name))?;
            self.source(self.find(&line.path, name, true))?;
            return Ok(i + 1);
        } else {
            self.at = Some((line.path.clone(), line.number));
            self.line(strip_comment(raw).trim_end(), raw, &line.path)?;
            return Ok(i + 1);
        }
        self.note(raw);
        Ok(i + 1)
    }

    /// The file an INCLUDE (`headers`) or INCBIN of `name` in file `from`
    /// reads: beside `from`, else in the first include folder that has it,
    /// else, for an INCLUDE, the SDK's include file of that name. A file
    /// found nowhere is the one beside `from`, which then cannot be read.
    fn find(&self, from: &str, name: &str, headers: bool) -> Rc<str> {
        let beside = path::beside(from, name);
        if Path::new(&*beside).is_file() || Path::new(name).is_absolute() {
            return beside;
        }
        if let Some(path) = self.include.iter().map(|dir| dir.join(name)).find(|path| path.is_file()) {
            return path.to_string_lossy().into();
        }
        if headers && header(name).is_some() {
            return format!("{PREFIX}{name}").into();
        }
        beside
    }

    /// The line after the ENDM of the REPT on line `i`.
    fn rept_end(&self, lines: &[Line], i: usize) -> Result<usize> {
        let mut depth = 1;
        let mut j = i + 1;
        while j < lines.len() && depth > 0 {
            let text = strip_comment(&lines[j].raw).trim().to_uppercase();
            if rept(&text).is_some() {
                depth += 1;
            } else if text == "ENDM" {
                depth -= 1;
            }
            j += 1;
        }
        if depth > 0 { Err(Error::new("REPT without ENDM")) } else { Ok(j) }
    }

    pub fn ev(&mut self, text: &str) -> Result<i64> {
        expr::eval(text, self)
    }

    fn define(&mut self, name: String, value: i64, kind: SymbolKind) -> Result<()> {
        self.kinds.insert(name.clone(), kind);
        if self.defined.contains(&name) {
            return Err(Error::new(format!("{name} defined twice")));
        }
        if self.last_pass && self.symbols.get(&name).is_some_and(|&v| v != value) {
            return Err(Error::new(format!("{name} moved in the last pass")));
        }
        self.defined.insert(name.clone());
        self.symbols.insert(name, value);
        Ok(())
    }

    /// A line of the listing with no address.
    fn note(&mut self, raw: &str) {
        self.listing.push(ListLine { addr: None, bytes: Vec::new(), raw: raw.to_string(), cycles: None });
    }

    /// A line of the listing with an address and no bytes.
    fn mark(&mut self, addr: i64, raw: &str) {
        self.listing.push(ListLine { addr: Some(addr), bytes: Vec::new(), raw: raw.to_string(), cycles: None });
    }

    /// Puts `data` at the current address: an instruction's bytes if it has
    /// `cycles`, otherwise data.
    fn emit(&mut self, data: Vec<i64>, raw: &str, cycles: Option<u32>) -> Result<()> {
        let start = self.here;
        let len = data.len() as i64;
        if self.bss {
            if data.iter().any(|&b| b != 0) {
                return Err(Error::new("only DS after BSS"));
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
        self.listing.push(ListLine { addr: Some(start), bytes, raw: raw.to_string(), cycles });
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
        let (label, rest) = if n > 0 && line[n..].starts_with(':') {
            (Some(&line[..n]), line[n + 1..].trim_start())
        } else if !line.starts_with(char::is_whitespace) {
            let (label, rest) = line.split_once(' ').unwrap_or((line, ""));
            (Some(label), rest.trim())
        } else {
            (None, line.trim())
        };
        let (mnemonic, operands) = rest.split_once(' ').unwrap_or((rest, ""));
        let mnemonic = mnemonic.to_uppercase();
        let operands = operands.trim();
        let ops = split_operands(operands);

        if mnemonic == "EQU" {
            let label = label.ok_or_else(|| Error::new("EQU needs a name"))?;
            let name = self.qualify(label);
            let value = self.ev(operands)?;
            self.define(name, value, SymbolKind::Equ)?;
            self.note(raw);
            return Ok(());
        }
        if let Some(label) = label {
            if !label.starts_with('.') {
                self.scope = label.to_string();
            }
            let name = self.qualify(label);
            self.define(name, self.here, SymbolKind::Label)?;
        }
        match mnemonic.as_str() {
            "" => self.mark(self.here, raw),
            "BSS" => {
                self.bss = true;
                self.mark(self.here, raw);
            }
            "ORG" => {
                self.here = self.ev(operands)? & 0xFFFF;
                self.org.get_or_insert(self.here);
                self.mark(self.here, raw);
            }
            "DB" => {
                let mut data = Vec::new();
                for op in ops {
                    let n = op.chars().count();
                    if n >= 2 && op.starts_with('\'') && op.ends_with('\'') && n != 3 {
                        data.extend(op[1..op.len() - 1].replace("''", "'").chars().map(|c| c as i64));
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
                return self.emit(vec![fill; count.max(0) as usize], raw, None);
            }
            "ALIGN" => {
                let n = self.ev(operands)?;
                if n <= 0 && self.last_pass {
                    return Err(Error::new("ALIGN takes a number above 0"));
                }
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
                let (data, cycles) = encode::encode(self, &mnemonic, &ops)?;
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

/// Splits on commas outside quotes.
fn split_operands(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c == '\'' {
            quoted = !quoted;
        }
        if c == ',' && !quoted {
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

/// `text` after its first `n` characters.
fn after_chars(text: &str, n: usize) -> &str {
    text.char_indices().nth(n).map_or("", |(i, _)| &text[i..])
}
