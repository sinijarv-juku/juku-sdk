//! STRUCT and UNION: a layout of named fields, their offsets as symbols,
//! and data laid out in it.

use std::rc::Rc;

use super::{Assembler, BSS, DIRECTIVES, Error, Line, Result, SymbolKind, head, is_name, memory, split_operands, string_bytes, strip_comment};
use crate::{encode, macros};

/// A struct: its fields in order, and their bytes when no value is given.
/// A union is one whose fields all start at 0.
pub(crate) struct Struct {
    /// Its full name.
    name: String,
    fields: Vec<Field>,
    default: Vec<u8>,
    union: bool,
}

impl Struct {
    pub fn size(&self) -> i64 {
        self.default.len() as i64
    }
}

struct Field {
    /// None for bytes with no name, which a value cannot be given for.
    name: Option<String>,
    offset: usize,
    size: usize,
    /// What pads a string shorter than the field: DS's fill, else 00h.
    pad: u8,
    /// A DW's value is a number, even `'c'`.
    word: bool,
    /// A field that is a struct, whose value is a list.
    inner: Option<Rc<Struct>>,
}

impl Assembler {
    /// Line `i` of `lines`, as `line`, if it defines a struct or a union,
    /// up to its ENDS: the line to go on with.
    pub(super) fn struct_line(&mut self, lines: &[Line], i: usize, line: &Line) -> Result<Option<usize>> {
        let (label, word, rest) = head(strip_comment(&line.raw).trim_end());
        if word.eq_ignore_ascii_case("ENDS") {
            return Err(Error::new("ENDS without STRUCT or UNION"));
        }
        let union = word.eq_ignore_ascii_case("UNION");
        if !union && !word.eq_ignore_ascii_case("STRUCT") {
            return Ok(None);
        }
        let kind = word.to_uppercase();
        let name = label.filter(|l| is_name(l)).ok_or_else(|| Error::new(format!("{kind} needs a name")))?;
        let upper = name.to_uppercase();
        if encode::is_mnemonic(&upper) || DIRECTIVES.contains(&upper.as_str()) {
            return Err(Error::new(format!("{name} is a mnemonic or a directive")));
        }
        let ends = |l: &Line| head(strip_comment(&l.raw).trim_end()).1.eq_ignore_ascii_case("ENDS");
        let end = i + 1 + lines[i + 1..].iter().position(ends).ok_or_else(|| Error::new(format!("{kind} without ENDS")))?;
        let full = self.defining(name);
        if !self.structs_defined.insert(full.clone()) || self.macros.contains_key(&full) {
            return Err(Error::new(format!("{full} defined twice")));
        }
        for l in &lines[i..=end] {
            self.note(&l.raw);
        }
        // An error is its field's line's; before the last pass the struct
        // is left as it was, and the lines after it go on.
        match self.fields(&full, union, rest, &lines[i + 1..end]) {
            Ok(s) => {
                self.name_fields(&full, &s, 0, SymbolKind::Field)?;
                self.broken.remove(&full);
                self.structs.insert(full, Rc::new(s));
            }
            Err(_) if !self.last_pass => {
                self.unresolved = true;
                self.broken.insert(full);
            }
            Err(e) => return Err(e),
        }
        Ok(Some(end + 1))
    }

    /// The struct, or with `union` the union, `name` whose fields are
    /// `lines`: `size` bytes, padded with 00h, if it is not blank.
    fn fields(&mut self, name: &str, union: bool, size: &str, lines: &[Line]) -> Result<Struct> {
        let mut s = Struct { name: name.to_string(), fields: Vec::new(), default: Vec::new(), union };
        for l in lines {
            self.at = Some((l.path.clone(), l.number));
            self.field(&mut s, l).map_err(|e| e.at(l))?;
        }
        if !size.is_empty() {
            let size = self.ev(size)?;
            memory(size)?;
            let n = s.default.len();
            if n as i64 > size {
                let short = name.rsplit('.').next().unwrap_or(name);
                return Err(Error::new(format!("{short}'s fields are {n} bytes, more than its {size}")));
            }
            s.default.resize(size as usize, 0);
        }
        Ok(s)
    }

    /// Adds the field on line `l` to `s`.
    fn field(&mut self, s: &mut Struct, l: &Line) -> Result<()> {
        let code = strip_comment(&l.raw).trim_end();
        if code.trim().is_empty() {
            return Ok(());
        }
        let (label, word, ops) = head(code);
        let name = match label {
            Some(n) if is_name(n) && !n.contains('.') => Some(n.to_string()),
            Some(n) => return Err(Error::new(format!("a field's name has no dot: {n}"))),
            None => None,
        };
        if let Some(n) = &name
            && s.fields.iter().any(|f| f.name.as_ref() == Some(n))
        {
            return Err(Error::new(format!("{n} twice in {}", s.name)));
        }
        let (mut pad, mut word_field, mut inner) = (0, false, None);
        let bytes = match word.to_uppercase().as_str() {
            "DB" => {
                let mut bytes = Vec::new();
                for op in split_operands(ops) {
                    if macros::is_string(op) && op.chars().count() != 3 {
                        bytes.extend(string_bytes(op)?);
                    } else {
                        let v = self.ev(op)?;
                        bytes.push(self.byte(v)? as u8);
                    }
                }
                bytes
            }
            "DW" => {
                word_field = true;
                let mut bytes = Vec::new();
                for op in split_operands(ops) {
                    let v = self.ev(op)?;
                    bytes.extend([v as u8, (v >> 8) as u8]);
                }
                bytes
            }
            "DS" => {
                let ops = split_operands(ops);
                let count = self.ev(ops.first().ok_or_else(|| Error::new("DS takes a count"))?)?;
                if let Some(fill) = ops.get(1) {
                    let v = self.ev(fill)?;
                    pad = self.byte(v)? as u8;
                }
                vec![pad; memory(count)?]
            }
            _ => {
                let t = self.find_struct(word).ok_or_else(|| Error::new(format!("a field is DB, DW, DS or a struct: {word}")))?;
                let bytes = self.values(&t, t.default.clone(), ops)?;
                inner = Some(t);
                bytes
            }
        };
        // A union's fields all start at 0, and where they overlap its bytes
        // are the first one's.
        let offset = if s.union { 0 } else { s.default.len() };
        s.fields.push(Field { name, offset, size: bytes.len(), pad, word: word_field, inner });
        if offset + bytes.len() > s.default.len() {
            s.default.extend(&bytes[s.default.len() - offset..]);
        }
        Ok(())
    }

    /// Defines `prefix.field` for each named field of `s`, at `base` and
    /// its offset, and a field's own fields under it; labels have their
    /// field's size.
    fn name_fields(&mut self, prefix: &str, s: &Struct, base: i64, kind: SymbolKind) -> Result<()> {
        for f in &s.fields {
            let Some(name) = &f.name else { continue };
            let full = format!("{prefix}.{name}");
            let at = base + f.offset as i64;
            self.define(full.clone(), at, kind)?;
            if kind == SymbolKind::Label {
                self.sizes.insert(full.clone(), f.size as i64);
            }
            if let Some(inner) = &f.inner {
                self.name_fields(&full, inner, at, kind)?;
            }
        }
        Ok(())
    }

    /// The struct `name` names, as the line it is on means it.
    pub(super) fn find_struct(&self, name: &str) -> Option<Rc<Struct>> {
        (0..=self.frames.len()).rev().map(|d| self.full(name, d)).find_map(|n| self.structs.get(&n).cloned())
    }

    /// SIZE() of `name`, qualified, when it is not a section: the struct's
    /// size. Before the last pass one not found is 0, and noted.
    pub fn struct_size(&mut self, name: &str) -> Result<i64> {
        match self.find_struct(name) {
            Some(s) if self.strict && !self.structs_defined.contains(&s.name) => Err(Error::new("not known yet")),
            Some(s) => Ok(s.size()),
            None if self.last_pass => Err(Error::new(format!("size() takes a section (code, const, data, bss) or a struct: {name}"))),
            None => {
                self.unresolved = true;
                Ok(0)
            }
        }
    }

    /// `name`, qualified, is a struct's.
    pub fn is_struct(&self, name: &str) -> bool {
        self.find_struct(name).is_some()
    }

    /// Lays out struct `s` with the values `operands` at the current
    /// address; `label` is the full name of the line's label, which names
    /// the fields under it. In BSS it only reserves.
    pub(super) fn instance(&mut self, label: Option<String>, s: &Struct, operands: &str, raw: &str) -> Result<()> {
        if let Some(label) = label {
            self.sizes.insert(label.clone(), s.size());
            self.name_fields(&label, s, self.here, SymbolKind::Label)?;
        }
        let bytes = if self.section == BSS {
            if !operands.trim().is_empty() {
                return Err(Error::new("a struct in BSS takes no values: it is reserved, not set"));
            }
            vec![0; s.default.len()]
        } else {
            self.values(s, s.default.clone(), operands)?
        };
        self.emit(bytes.into_iter().map(i64::from).collect(), raw, None)
    }

    /// `bytes`, laid out as `s`, with the values `text`: each one in the
    /// order of the fields, or `field=value`; a field with none, or a blank
    /// one, keeps its bytes.
    fn values(&mut self, s: &Struct, mut bytes: Vec<u8>, text: &str) -> Result<Vec<u8>> {
        let named: Vec<usize> = (0..s.fields.len()).filter(|&i| s.fields[i].name.is_some()).collect();
        let mut given = vec![false; s.fields.len()];
        let (mut next, mut by_name) = (0, false);
        for item in split_operands(text) {
            let (i, value) = match named_value(item) {
                Some((name, value)) => {
                    by_name = true;
                    let i = named.iter().copied().find(|&i| s.fields[i].name.as_deref() == Some(name));
                    (i.ok_or_else(|| Error::new(format!("{} has no field {name}", s.name)))?, value)
                }
                None if by_name => return Err(Error::new("a value without its field's name after one with it")),
                None => {
                    let i = *named.get(next).ok_or_else(|| Error::new(format!("{} has {} fields", s.name, named.len())))?;
                    next += 1;
                    (i, item)
                }
            };
            let f = &s.fields[i];
            if std::mem::replace(&mut given[i], true) {
                return Err(Error::new(format!("{} given twice", f.name.as_deref().unwrap_or(""))));
            }
            if s.union
                && let Some(j) = (0..s.fields.len()).find(|&j| j != i && given[j])
            {
                let (a, b) = (s.fields[j].name.as_deref().unwrap_or(""), f.name.as_deref().unwrap_or(""));
                return Err(Error::new(format!("{a} and {b} are the same bytes of {}: give only one a value", s.name)));
            }
            if value.is_empty() {
                continue;
            }
            let range = f.offset..f.offset + f.size;
            let value = self.field_value(f, bytes[range.clone()].to_vec(), value)?;
            bytes[range].copy_from_slice(&value);
        }
        Ok(bytes)
    }

    /// The bytes of field `f`, now `bytes`, with the value `text`: a
    /// struct's from a list, a string padded to the field, or a number, low
    /// byte first.
    fn field_value(&mut self, f: &Field, bytes: Vec<u8>, text: &str) -> Result<Vec<u8>> {
        let name = f.name.as_deref().unwrap_or("");
        if let Some(inner) = &f.inner {
            if !macros::is_list(text) {
                return Err(Error::new(format!("{name} is a struct: its value is a list, [...]")));
            }
            let t = text.trim();
            return self.values(inner, bytes, &t[1..t.len() - 1]);
        }
        if !f.word && macros::is_string(text) {
            let mut bytes = string_bytes(text)?;
            if bytes.len() > f.size {
                return Err(Error::new(format!("{text} is longer than {name}, {} bytes", f.size)));
            }
            bytes.resize(f.size, f.pad);
            return Ok(bytes);
        }
        let v = self.ev(text)?;
        if f.size == 1 {
            return Ok(vec![self.byte(v)? as u8]);
        }
        Ok((0..f.size).map(|i| if i < 8 { (v >> (8 * i)) as u8 } else { 0 }).collect())
    }
}

/// `name=value`, for a field's name: not `==` or another operator.
fn named_value(item: &str) -> Option<(&str, &str)> {
    let (name, value) = item.split_once('=')?;
    let name = name.trim();
    (is_name(name) && !name.contains('.') && !value.starts_with('=')).then(|| (name, value.trim()))
}
