//! The 8080 disassembler: an image back into juku-asm's source, one
//! instruction after another from where it is told to start.
//!
//! Instructions are written as the SDK's source is: `lxi h, 012bh`,
//! `call rb.tto`. An undocumented opcode is written as the bytes it is, `db`,
//! with what it does in a comment, so that the source assembles back to the
//! same bytes; so is an instruction cut off by the end of the image.

use std::collections::{HashMap, HashSet};

use juku_i8080::{Data, OPS};

/// The name the SDK's source includes the include file `file` under, as in
/// `include 'rombios.inc' as rb`: `rb` for `rombios.inc`, `dos` for
/// `ekdos.inc`, `io` for `ports.inc`; none for another file, which is
/// included as it is.
pub fn namespace(file: &str) -> Option<&'static str> {
    match file {
        "rombios.inc" => Some("rb"),
        "ekdos.inc" => Some("dos"),
        "ports.inc" => Some("io"),
        _ => None,
    }
}

/// `name` from the include file `file`, as the source names it: `rb.tto`.
pub fn qualified(file: &str, name: &str) -> String {
    match namespace(file) {
        Some(ns) => format!("{ns}.{name}"),
        None => name.to_string(),
    }
}

/// The line that includes `file`, as the SDK's source writes it.
pub fn include_line(file: &str) -> String {
    match namespace(file) {
        Some(ns) => format!("        include '{file}' as {ns}\n"),
        None => format!("        include '{file}'\n"),
    }
}

/// Names for values: addresses for word operands, ports for IN and OUT.
#[derive(Default)]
pub struct Names {
    words: HashMap<u16, String>,
    ports: HashMap<u8, String>,
}

impl Names {
    /// Names word operands equal to `value` `name`, unless another name
    /// was given for it first.
    pub fn word(&mut self, value: u16, name: &str) {
        self.words.entry(value).or_insert_with(|| name.to_string());
    }

    /// Names the port `port` of IN and OUT `name`, unless another name was
    /// given for it first.
    pub fn port(&mut self, port: u8, name: &str) {
        self.ports.entry(port).or_insert_with(|| name.to_string());
    }
}

/// A line of the disassembly.
pub struct Line {
    pub addr: u32,
    pub bytes: Vec<u8>,
    /// A label for the address, when an operand names it.
    pub label: Option<String>,
    /// The instruction, as source: `mvi a, 34h`, or `db 0cbh, 34h, 12h`.
    pub text: String,
    pub comment: Option<String>,
}

/// A number as juku-asm reads it: `34h`, `0ffd9h`.
pub fn hex(v: u32, digits: usize) -> String {
    let text = format!("{v:0digits$x}h");
    if text.starts_with(|c: char| c.is_ascii_alphabetic()) { format!("0{text}") } else { text }
}

/// The label of address `addr`: `l0103`.
fn label(addr: u32) -> String {
    format!("l{addr:04x}")
}

/// Disassembles `image`, whose first byte is at `base`, from `start` up to
/// `end` (not included; an instruction that starts before it is whole).
/// With `labels`, a word operand that is the address of one of the
/// instructions is written as that instruction's label.
pub fn disassemble(image: &[u8], base: u32, start: u32, end: u32, names: &Names, labels: bool) -> Vec<Line> {
    let byte = |a: u32| a.checked_sub(base).and_then(|i| image.get(i as usize)).copied();
    let mut lines = Vec::new();
    let mut at = start;
    while at < end {
        let Some(code) = byte(at) else { break };
        let op = &OPS[code as usize];
        let bytes: Vec<u8> = (at..at + op.size() as u32).map_while(byte).collect();
        lines.push(Line { addr: at, bytes, label: None, text: String::new(), comment: None });
        at += op.size() as u32;
    }
    let starts: HashSet<u32> = lines.iter().map(|l| l.addr).collect();
    let mut targets = HashSet::new();
    for line in &mut lines {
        let op = &OPS[line.bytes[0] as usize];
        let data = match (op.data, &line.bytes[..]) {
            (Data::Byte, [_, b]) => {
                let name = if matches!(op.mnemonic, "IN" | "OUT") { names.ports.get(b) } else { None };
                Some(name.cloned().unwrap_or_else(|| hex(*b as u32, 2)))
            }
            (Data::Word, [_, lo, hi]) => {
                let v = u16::from_le_bytes([*lo, *hi]);
                Some(if labels && starts.contains(&(v as u32)) {
                    targets.insert(v as u32);
                    label(v as u32)
                } else {
                    names.words.get(&v).cloned().unwrap_or_else(|| hex(v as u32, 4))
                })
            }
            (Data::None, _) => None,
            _ => {
                line.text = db(&line.bytes);
                line.comment = Some("cut off by the end of the image".to_string());
                continue;
            }
        };
        let mut operands: Vec<String> = op.regs[..op.reg_count()].iter().map(|r| r.to_lowercase()).collect();
        operands.extend(data);
        let mut text = op.mnemonic.to_lowercase();
        if !operands.is_empty() {
            text = format!("{text} {}", operands.join(", "));
        }
        if op.documented {
            line.text = text;
        } else {
            line.text = db(&line.bytes);
            line.comment = Some(format!("undocumented: {text}"));
        }
    }
    for line in &mut lines {
        if targets.contains(&line.addr) {
            line.label = Some(label(line.addr));
        }
    }
    lines
}

fn db(bytes: &[u8]) -> String {
    format!("db {}", bytes.iter().map(|&b| hex(b as u32, 2)).collect::<Vec<_>>().join(", "))
}

/// The lines as a listing: each one's address and bytes, then the
/// instruction.
pub fn listing(lines: &[Line]) -> String {
    let mut out = String::new();
    for l in lines {
        let bytes = l.bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
        let mut text = format!("{:04X}  {bytes:9} {}", l.addr, l.text);
        if let Some(c) = &l.comment {
            text = format!("{text:32} ; {c}");
        }
        out += text.trim_end();
        out.push('\n');
    }
    out
}

/// The lines as source juku-asm assembles back to the same bytes, after
/// `head`: comments, `include` lines.
pub fn source(lines: &[Line], head: &str) -> String {
    let mut out = head.to_string();
    if let Some(first) = lines.first() {
        out += &format!("        org {}\n\n", hex(first.addr, 4));
    }
    for l in lines {
        let label = l.label.as_ref().map(|n| format!("{n}:")).unwrap_or_default();
        let mut text = format!("{label:7} {}", l.text);
        if let Some(c) = &l.comment {
            text = format!("{text:31} ; {c}");
        }
        out += text.trim_end();
        out.push('\n');
    }
    out
}
