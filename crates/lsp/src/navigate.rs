//! Going to a name: its definition, its uses, what it is, and the names a
//! file or the workspace defines. Each answer is the LSP result, as JSON.
//!
//! The word at the cursor is matched to a name by the main files' indexes:
//! each line's uses and definitions are the full names it has, so `.loop`
//! is the `main.loop` the assembler took it for, and `tto` the `rb.tto`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use juku_asm::NameKind;
use juku_i8080::{Data, OPS, Op};
use juku_json::{Value, obj};

use crate::names::{Def, refers};
use crate::project::Project;
use crate::text::{self, Word};
use crate::uri;

/// Most symbols a workspace search returns.
const MAX_SYMBOLS: usize = 1000;

/// The answers for one request: the project, the position units, and the
/// files' text as it is read.
pub struct Query<'a> {
    pub(crate) project: &'a Project,
    pub(crate) utf8: bool,
    texts: HashMap<PathBuf, String>,
}

/// A word at a position.
pub(crate) struct At {
    pub path: PathBuf,
    pub line: usize,
    pub text: String,
    pub kind: Word,
    pub start: usize,
    pub end: usize,
}

impl At {
    pub fn word(&self) -> &str {
        &self.text[self.start..self.end]
    }
}

impl<'a> Query<'a> {
    pub fn new(project: &'a Project, utf8: bool) -> Query<'a> {
        Query { project, utf8, texts: HashMap::new() }
    }

    /// Line `n` of `path`, from 0.
    pub(crate) fn line(&mut self, path: &Path, n: usize) -> String {
        if !self.texts.contains_key(path) {
            let text = self.project.text(path).unwrap_or_default();
            self.texts.insert(path.to_path_buf(), text);
        }
        text::line(&self.texts[path], n).to_string()
    }

    pub(crate) fn uri(&self, path: &Path) -> String {
        self.project.open.get(path).map_or_else(|| uri::from_path(path), |d| d.uri.clone())
    }

    /// The word at the request's position.
    pub(crate) fn at(&mut self, params: &Value) -> Option<At> {
        let path = uri::to_path(params["textDocument"]["uri"].as_str()?)?;
        let line = params["position"]["line"].as_i64()? as usize;
        let character = params["position"]["character"].as_i64()? as usize;
        let text = self.line(&path, line);
        let byte = text::byte_at(&text, character, self.utf8);
        let (kind, start, end) = text::word_at(&text, byte)?;
        Some(At { path, line, text, kind, start, end })
    }

    pub(crate) fn position(&self, line: usize, text: &str, byte: usize) -> Value {
        obj([("line", line.into()), ("character", text::units(&text[..byte], self.utf8).into())])
    }

    pub(crate) fn range(&self, line: usize, text: &str, start: usize, end: usize) -> Value {
        obj([("start", self.position(line, text, start)), ("end", self.position(line, text, end))])
    }

    /// The words of line `line` of `path` that can be `name`.
    pub(crate) fn words_for(&mut self, path: &Path, line: usize, name: &str) -> (String, Vec<(usize, usize)>) {
        let text = self.line(path, line);
        let found = text::words(&text).into_iter().filter(|&(k, s, e)| k == Word::Name && refers(name, &text[s..e])).map(|(_, s, e)| (s, e)).collect();
        (text, found)
    }

    /// Where `name` is on line `line` of `path`: the first word that can be
    /// it, else the line's code.
    pub(crate) fn location(&mut self, path: &Path, line: usize, name: &str) -> Value {
        let (text, found) = self.words_for(path, line, name);
        let (start, end) = found.first().copied().unwrap_or_else(|| code_span(&text));
        obj([("uri", self.uri(path).into()), ("range", self.range(line, &text, start, end))])
    }

    /// The full name the word at `at` is, and the main files that have it
    /// there; else the main files that define a name it can be.
    pub(crate) fn resolve(&self, at: &At) -> Option<(String, Vec<&'a PathBuf>)> {
        let word = at.word();
        let mut found: Vec<(&str, &PathBuf)> = Vec::new();
        for (main, names) in &self.project.names {
            let defs = names.defs.iter().filter(|d| d.line == at.line && d.path == at.path).map(|d| d.name.as_str());
            let uses = names.uses.iter().filter(|u| u.line == at.line && u.path == at.path).map(|u| u.name.as_str());
            found.extend(defs.chain(uses).filter(|n| refers(n, word)).map(|n| (n, main)));
        }
        if found.is_empty() {
            // Not on a line the indexes know, or not as written: a name
            // defined so, or else the one name that ends so.
            for (main, names) in &self.project.names {
                found.extend(names.defs.iter().filter(|d| refers(&d.name, word)).map(|d| (d.name.as_str(), main)));
            }
            let bare = word.trim_start_matches('.');
            if found.iter().any(|(n, _)| *n == bare) {
                found.retain(|(n, _)| *n == bare);
            }
            let first = found.first()?.0;
            if found.iter().any(|(n, _)| *n != first) {
                return None;
            }
        }
        let bare = word.trim_start_matches('.');
        let name = found.iter().map(|(n, _)| *n).min_by_key(|n| (*n != bare, n.len()))?.to_string();
        let mut mains: Vec<&PathBuf> = found.into_iter().filter(|(n, _)| *n == name).map(|(_, m)| m).collect();
        text::sort(&mut mains);
        mains.dedup();
        Some((name, mains))
    }

    /// The definitions of `name` in `mains`, each place once.
    pub(crate) fn defs(&self, name: &str, mains: &[&PathBuf]) -> Vec<&'a Def> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for main in mains {
            for d in self.project.names[*main].defs.iter().filter(|d| d.name == name) {
                if seen.insert((&d.path, d.line)) {
                    out.push(d);
                }
            }
        }
        out
    }

    /// `textDocument/definition`.
    pub fn definition(&mut self, params: &Value) -> Value {
        let Some(at) = self.at(params) else { return Value::Null };
        let Some((name, mains)) = self.resolve(&at) else { return Value::Null };
        let defs = self.defs(&name, &mains);
        defs.iter().map(|d| self.location(&d.path, d.line, &name)).collect::<Vec<_>>().into()
    }

    /// `textDocument/references`.
    pub fn references(&mut self, params: &Value) -> Value {
        let Some(at) = self.at(params) else { return Value::Null };
        let Some((name, mains)) = self.resolve(&at) else { return Value::Null };
        let mut lines: Vec<(&PathBuf, usize)> = Vec::new();
        for main in &mains {
            lines.extend(self.project.names[*main].uses.iter().filter(|u| u.name == name).map(|u| (&u.path, u.line)));
        }
        if params["context"]["includeDeclaration"].as_bool() == Some(true) {
            lines.extend(self.defs(&name, &mains).into_iter().map(|d| (&d.path, d.line)));
        }
        text::sort(&mut lines);
        lines.dedup();
        let mut out = Vec::new();
        for (path, line) in lines {
            let (text, found) = self.words_for(path, line, &name);
            for (start, end) in found {
                out.push(obj([("uri", self.uri(path).into()), ("range", self.range(line, &text, start, end))]));
            }
        }
        out.into()
    }

    /// `textDocument/hover`: a name's value, its definition and the comment
    /// on it; a number's value; an instruction's opcode, size and cycles.
    pub fn hover(&mut self, params: &Value) -> Value {
        let Some(at) = self.at(params) else { return Value::Null };
        let text = match at.kind {
            Word::Number => number(at.word()),
            Word::Name => match self.resolve(&at) {
                Some((name, mains)) => self.defs(&name, &mains).first().map(|d| self.describe(d)),
                None => instruction(at.word(), &text::strip_comment(&at.text)[at.end..]),
            },
        };
        match text {
            Some(text) => obj([
                ("contents", obj([("kind", "markdown".into()), ("value", text.into())])),
                ("range", self.range(at.line, &at.text, at.start, at.end)),
            ]),
            None => Value::Null,
        }
    }

    /// What `d` is, its value, its line and the comment above it.
    pub(crate) fn describe(&mut self, d: &Def) -> String {
        let line = self.line(&d.path, d.line);
        let mut above = Vec::new();
        for n in (0..d.line).rev() {
            let l = self.line(&d.path, n);
            match l.trim_start().strip_prefix(';') {
                Some(c) if !l.trim().is_empty() => above.push(c.strip_prefix(' ').unwrap_or(c).trim_end().to_string()),
                _ => break,
            }
        }
        above.reverse();
        let mut out = format!("`{}`", d.name);
        if let Some(what) = value_text(d) {
            out += &format!(" {what}");
        }
        out += &format!("\n\n```juku-asm\n{}\n```", line.trim_end());
        if !above.is_empty() {
            out += &format!("\n\n{}", above.join("\n"));
        }
        out
    }

    /// `textDocument/documentSymbol`: the names the file defines, each
    /// label's local labels and each struct's fields under it.
    pub fn document_symbols(&mut self, params: &Value, nested: bool) -> Value {
        let Some(path) = params["textDocument"]["uri"].as_str().and_then(uri::to_path) else { return Value::Null };
        let mut defs: Vec<(&Def, &str)> = Vec::new();
        let mut seen = HashSet::new();
        for names in self.project.names.values() {
            for d in names.defs.iter().filter(|d| d.path == path) {
                if seen.insert((&d.name, d.line)) {
                    defs.push((d, names.shown(&d.name, &path)));
                }
            }
        }
        text::sort_by_key(&mut defs, |(d, _)| (d.line, d.name.len()));
        let index: HashMap<&str, usize> = defs.iter().enumerate().map(|(i, (d, _))| (d.name.as_str(), i)).collect();
        let parent = |d: &Def| d.name.rsplit_once('.').and_then(|(p, _)| index.get(p).copied()).filter(|&p| defs[p].0.line <= d.line);
        let symbols: Vec<Value> = defs
            .iter()
            .map(|(d, shown)| {
                // Under its parent, a local label is `.loop` and a field `x`.
                let name = match parent(d).filter(|_| nested) {
                    Some(p) => {
                        let rest = &d.name[defs[p].0.name.len() + 1..];
                        if d.kind == NameKind::Label { format!(".{rest}") } else { rest.to_string() }
                    }
                    None => shown.to_string(),
                };
                self.symbol(d, &name)
            })
            .collect();
        if !nested {
            let flat: Vec<Value> = defs
                .iter()
                .zip(symbols)
                .map(|((d, _), s)| {
                    let location = obj([("uri", self.uri(&path).into()), ("range", s["range"].clone())]);
                    let mut info = obj([("name", s["name"].clone()), ("kind", s["kind"].clone()), ("location", location)]);
                    if let Some(p) = parent(d) {
                        info.set("containerName", defs[p].1.into());
                    }
                    info
                })
                .collect();
            return flat.into();
        }
        // Children first, from the last: each goes into its parent.
        let mut built: Vec<Option<Value>> = symbols.into_iter().map(Some).collect();
        let mut children: Vec<Vec<Value>> = vec![Vec::new(); defs.len()];
        for i in (0..defs.len()).rev() {
            let mut s = built[i].take().expect("there");
            let mine = std::mem::take(&mut children[i]);
            if !mine.is_empty() {
                s.set("children", mine.into_iter().rev().collect::<Vec<_>>().into());
            }
            match parent(defs[i].0) {
                Some(p) if p < i => children[p].push(s),
                _ => built[i] = Some(s),
            }
        }
        built.into_iter().flatten().collect::<Vec<_>>().into()
    }

    /// `d` as a DocumentSymbol, named `shown`.
    fn symbol(&mut self, d: &Def, shown: &str) -> Value {
        let (text, found) = self.words_for(&d.path, d.line, &d.name);
        let code = code_span(&text);
        let (start, end) = found.first().copied().unwrap_or(code);
        let range = self.range(d.line, &text, code.0.min(start), code.1.max(end));
        let mut s = obj([
            ("name", shown.into()),
            ("kind", symbol_kind(d).into()),
            ("range", range),
            ("selectionRange", self.range(d.line, &text, start, end)),
        ]);
        if let Some(detail) = value_text(d) {
            s.set("detail", detail.into());
        }
        s
    }

    /// `workspace/symbol`: the names whose full name has `query` in it,
    /// in any case.
    pub fn workspace_symbols(&mut self, params: &Value) -> Value {
        let query = params["query"].as_str().unwrap_or("").to_lowercase();
        let mut seen = HashSet::new();
        let mut defs: Vec<&Def> = Vec::new();
        for names in self.project.names.values() {
            for d in &names.defs {
                if d.name.to_lowercase().contains(&query) && seen.insert((&d.name, &d.path, d.line)) {
                    defs.push(d);
                }
            }
        }
        text::sort_by(&mut defs, |a, b| (&a.name, &a.path, a.line).cmp(&(&b.name, &b.path, b.line)));
        defs.truncate(MAX_SYMBOLS);
        defs.into_iter()
            .map(|d| {
                let location = self.location(&d.path, d.line, &d.name);
                let file = d.path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                obj([("name", d.name.as_str().into()), ("kind", symbol_kind(d).into()), ("location", location), ("containerName", file.into())])
            })
            .collect::<Vec<_>>()
            .into()
    }
}

/// The byte range of `line`'s code, without the comment, or of the whole
/// line if it is a comment.
pub fn code_span(line: &str) -> (usize, usize) {
    let code = match text::strip_comment(line).trim_end() {
        "" => line.trim_end(),
        code => code,
    };
    (code.len() - code.trim_start().len(), code.len())
}

/// LSP's SymbolKind for `d`.
pub(crate) fn symbol_kind(d: &Def) -> i64 {
    match d.kind {
        NameKind::Label => 12,                    // Function
        NameKind::Macro => 6,                     // Method
        NameKind::Equ => 14,                      // Constant
        NameKind::Set => 13,                      // Variable
        NameKind::Field => 8,                     // Field
        NameKind::Struct | NameKind::Union => 23, // Struct
    }
}

/// A value as source writes it: hex, lower case, with `h`: `0ffd9h`.
fn hex(v: i64) -> String {
    hex_digits(v, if v > 0xFF { 4 } else { 1 })
}

/// `v` in hex with at least `n` digits, and a 0 before a letter.
pub(crate) fn hex_digits(v: i64, n: usize) -> String {
    let digits = format!("{v:0n$x}");
    if digits.starts_with(|c: char| c.is_ascii_alphabetic()) { format!("0{digits}h") } else { format!("{digits}h") }
}

/// What a definition is, with its value: `label at 0102h`, `= 3`.
pub(crate) fn value_text(d: &Def) -> Option<String> {
    let v = d.value;
    Some(match d.kind {
        NameKind::Label => format!("label at {}", hex_digits(v? & 0xFFFF, 4)),
        NameKind::Equ | NameKind::Set if v? < 0 => format!("= {}", v?),
        NameKind::Equ | NameKind::Set => format!("= {} ({})", hex(v?), v?),
        NameKind::Field => format!("field at offset {}", v?),
        NameKind::Struct => format!("struct of {} bytes", v?),
        NameKind::Union => format!("union of {} bytes", v?),
        NameKind::Macro => "macro".to_string(),
    })
}

/// A number word's value in hex, decimal and binary, and as a character.
fn number(word: &str) -> Option<String> {
    let w = word.to_ascii_lowercase();
    let (digits, radix) = if let Some(h) = w.strip_suffix('h') {
        (h, 16)
    } else if let Some(b) = w.strip_suffix('b').filter(|b| b.bytes().all(|c| c == b'0' || c == b'1')) {
        (b, 2)
    } else if let Some(d) = w.strip_suffix('d') {
        (d, 10)
    } else {
        (w.as_str(), 10)
    };
    let v = i64::from_str_radix(digits, radix).ok()?;
    let mut out = format!("`{word}` = {v} = {} = {:b}b", hex(v), v);
    if (0x20..0x7F).contains(&v) && v != 0x27 {
        out += &format!(" = '{}'", v as u8 as char);
    }
    Some(out)
}

/// What instruction `mn`'s operands are, as words: `r`, `rp`, `n` (RST's),
/// `byte`, `word`. None for no instruction.
pub(crate) fn operand_shape(mn: &str) -> Option<Vec<&'static str>> {
    let mn = mn.to_uppercase();
    let first = OPS.iter().find(|op| op.documented && op.mnemonic == mn)?;
    let mut shape: Vec<&str> = (0..first.reg_count()).map(|_| if mn == "RST" { "n" } else if first.pairs { "rp" } else { "r" }).collect();
    match first.data {
        Data::Byte => shape.push("byte"),
        Data::Word => shape.push("word"),
        Data::None => {}
    }
    Some(shape)
}

/// The conditional calls and returns, whose cycle count is when taken.
const CONDITIONAL: [&str; 16] = ["CNZ", "CZ", "CNC", "CC", "CPO", "CPE", "CP", "CM", "RNZ", "RZ", "RNC", "RC", "RPO", "RPE", "RP", "RM"];

/// What mnemonic `word` is, with the operands `operands` written after it:
/// the opcode, size and cycles of that instruction, or of its forms.
fn instruction(word: &str, operands: &str) -> Option<String> {
    let mn = word.to_uppercase();
    let forms: Vec<(usize, &Op)> = OPS.iter().enumerate().filter(|(_, op)| op.documented && op.mnemonic == mn).collect();
    let first = forms.first()?.1;
    let given: Vec<String> = operands.split(',').map(|o| o.trim().to_uppercase()).filter(|o| !o.is_empty()).collect();
    let exact = forms.iter().find(|(_, op)| op.operand_count() == given.len() && op.regs[..op.reg_count()].iter().zip(&given).all(|(r, g)| r == g));
    let size = |op: &Op| if op.size() == 1 { "1 byte".to_string() } else { format!("{} bytes", op.size()) };
    let taken = if CONDITIONAL.contains(&mn.as_str()) { ", when taken" } else { "" };
    if let Some((code, op)) = exact {
        let shown = if given.is_empty() { word.to_string() } else { format!("{word} {}", operands.trim()) };
        let opcode = hex_digits(*code as i64, 2);
        return Some(format!("`{shown}`: opcode {opcode}, {}, {} cycles{taken}", size(op), op.cycles));
    }
    let shape = operand_shape(&mn).unwrap_or_default();
    let mut cycles: Vec<u8> = forms.iter().map(|(_, op)| op.cycles).collect();
    text::sort(&mut cycles);
    cycles.dedup();
    let cycles = match cycles[..] {
        [one] => one.to_string(),
        [a, b] => format!("{a} or {b}"),
        _ => format!("{} to {}", cycles[0], cycles[cycles.len() - 1]),
    };
    let mut shown = word.to_lowercase();
    if !shape.is_empty() {
        shown += &format!(" {}", shape.join(", "));
    }
    Some(format!("`{shown}`: {}, {cycles} cycles{taken}", size(first)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values() {
        assert_eq!(hex(0xFFD9), "0ffd9h");
        assert_eq!(hex(0x102), "0102h");
        assert_eq!(hex(3), "3h");
        assert_eq!((hex_digits(1, 4), hex_digits(0xffd9, 4), hex_digits(0xc9, 2)), ("0001h".into(), "0ffd9h".into(), "0c9h".into()));
        assert_eq!(number("0ffh").unwrap(), "`0ffh` = 255 = 0ffh = 11111111b");
        assert_eq!(number("65").unwrap(), "`65` = 65 = 41h = 1000001b = 'A'");
        assert_eq!(number("12x"), None);
    }

    #[test]
    fn instructions() {
        assert_eq!(instruction("mov", " a, m").unwrap(), "`mov a, m`: opcode 7eh, 1 byte, 7 cycles");
        assert_eq!(instruction("MVI", " A, COUNT").unwrap(), "`MVI A, COUNT`: opcode 3eh, 2 bytes, 7 cycles");
        assert_eq!(instruction("nop", "").unwrap(), "`nop`: opcode 00h, 1 byte, 4 cycles");
        assert_eq!(instruction("ret", "").unwrap(), "`ret`: opcode 0c9h, 1 byte, 10 cycles");
        assert_eq!(instruction("cnz", " x").unwrap(), "`cnz x`: opcode 0c4h, 3 bytes, 17 cycles, when taken");
        assert_eq!(instruction("mov", "").unwrap(), "`mov r, r`: 1 byte, 5 or 7 cycles");
        assert_eq!(instruction("push", " q").unwrap(), "`push rp`: 1 byte, 11 cycles");
        assert_eq!(instruction("frob", ""), None);
    }
}
