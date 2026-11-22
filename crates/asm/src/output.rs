//! The files written beside the image: the listing, the symbols and the
//! debug information.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;

use crate::asm::{Kind, Program, SymbolKind};
use crate::path;

impl Program {
    /// The listing: each line's address, its first four bytes and, for an
    /// instruction, its cycle count, then the line as written.
    pub fn listing(&self) -> String {
        let mut out = String::new();
        for l in &self.listing {
            let head = l.addr.map(|a| format!("{a:04X}")).unwrap_or_default();
            let mut hexes = l.bytes.iter().take(4).map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
            if l.bytes.len() > 4 {
                hexes.push_str(" …");
            }
            let cycles = l.cycles.map(|c| c.to_string()).unwrap_or_default();
            _ = writeln!(out, "{head:4}  {hexes:14} {cycles:>3}  {}", l.raw);
        }
        out
    }

    /// Every label and EQU as `name 1234`, in hex, in order of name.
    pub fn symbols(&self) -> String {
        let mut out = String::new();
        for (name, v) in &self.symbols {
            _ = writeln!(out, "{name} {:04X}", v & 0xFFFF);
        }
        out
    }

    /// The debug information (the SDK's docs/asm.md, Outputs), in JSON, to
    /// be written to `dbg`, for the image written to `image`. Paths are
    /// relative to the file's folder.
    ///
    /// `lines` has a row for each run of code from one file with no gap: its
    /// address, file and first line, the first line's length in bytes, then
    /// for each line after it how many lines on it is and its length. `data`
    /// is the ranges DB, DW, DS, ALIGN and INCBIN made. A label on a line of
    /// data, or on memory reserved after BSS, has the size of that line as a
    /// third field: what a watchpoint on it watches.
    pub fn debug_info(&self, dbg: &Path, image: &Path) -> String {
        let folder = path::absolute(dbg).parent().map(Path::to_path_buf).unwrap_or_default();
        let mut spans: Vec<_> = self.spans.iter().collect();
        spans.sort_by(|a, b| (a.addr, a.len, &*a.path, a.line, a.kind).cmp(&(b.addr, b.len, &*b.path, b.line, b.kind)));

        let mut files: Vec<String> = Vec::new();
        let mut file_of: HashMap<&str, usize> = HashMap::new();
        let mut blocks: Vec<Vec<i64>> = Vec::new();
        let (mut end, mut last) = (0, 0);
        for s in spans.iter().filter(|s| s.kind == Kind::Code) {
            let f = *file_of.entry(&s.path).or_insert_with(|| {
                let name = path::portable(Path::new(&*s.path), &folder);
                files.iter().position(|n| *n == name).unwrap_or_else(|| {
                    files.push(name);
                    files.len() - 1
                })
            }) as i64;
            let line = s.line as i64;
            match blocks.last_mut() {
                Some(b) if s.addr == end && f == b[1] && line >= last => b.extend([line - last, s.len]),
                _ => blocks.push(vec![s.addr, f, line, s.len]),
            }
            (end, last) = (s.addr + s.len, line);
        }

        let mut data: Vec<[i64; 2]> = Vec::new();
        for s in spans.iter().filter(|s| s.kind == Kind::Data) {
            match data.last_mut() {
                Some(d) if d[0] + d[1] == s.addr => d[1] += s.len,
                _ => data.push([s.addr, s.len]),
            }
        }

        let code_at: HashSet<i64> = spans.iter().filter(|s| s.kind == Kind::Code).map(|s| s.addr).collect();
        let mut sizes = HashMap::new();
        let data_spans = spans.iter().filter(|s| s.kind == Kind::Data).map(|s| (s.addr, s.len));
        for (a, n) in data_spans.chain(self.reserved.iter().copied()) {
            sizes.entry(a).or_insert(n);
        }
        let mut symbols: Vec<_> = self.symbols.iter().map(|(name, v)| (v & 0xFFFF, name)).collect();
        symbols.sort();
        let (mut labels, mut equs) = (Vec::new(), Vec::new());
        for (v, name) in symbols {
            let quoted = json_string(name);
            if self.kinds[name] == SymbolKind::Equ {
                equs.push(format!("[{quoted},{v}]"));
            } else if let Some(n) = sizes.get(&v)
                && !code_at.contains(&v)
            {
                labels.push(format!("[{quoted},{v},{n}]"));
            } else {
                labels.push(format!("[{quoted},{v}]"));
            }
        }

        let numbers = |row: &[i64]| row.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let blocks: Vec<_> = blocks.iter().map(|b| format!("[{}]", numbers(b))).collect();
        let data: Vec<_> = data.iter().map(|d| format!("[{}]", numbers(d))).collect();
        let files: Vec<_> = files.iter().map(|f| json_string(f)).collect();
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str("  \"version\": 2,\n");
        _ = writeln!(out, "  \"image\": {},", json_string(&path::portable(image, &folder)));
        _ = writeln!(out, "  \"load\": {},", self.load);
        _ = writeln!(out, "  \"size\": {},", self.image.len());
        _ = writeln!(out, "  \"entry\": {},", self.load);
        _ = writeln!(out, "  \"files\": [{}],", files.join(", "));
        _ = writeln!(out, "  \"lines\": {},", table(&blocks));
        _ = writeln!(out, "  \"data\": [{}],", data.join(","));
        _ = writeln!(out, "  \"labels\": {},", table(&labels));
        _ = writeln!(out, "  \"equs\": {}", table(&equs));
        out.push_str("}\n");
        out
    }
}

/// A JSON array with a row on each line.
fn table(rows: &[String]) -> String {
    if rows.is_empty() {
        return "[]".to_string();
    }
    let rows: Vec<_> = rows.iter().map(|r| format!("    {r}")).collect();
    format!("[\n{}\n  ]", rows.join(",\n"))
}

/// A JSON string, in ASCII.
fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            c => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
    out
}
