//! Writing source: completion, the operands of the instruction or macro
//! being written, renaming a name, folding blocks, and the address and
//! cycles beside each line.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use juku_asm::NameKind;
use juku_i8080::OPS;
use juku_json::{Value, obj};

use crate::names::{Def, Names};
use crate::navigate::{Query, hex_digits, operand_shape, value_text};
use crate::text::{self, Word};
use crate::uri;

/// LSP's error for a request that cannot be done as asked.
pub const REQUEST_FAILED: i64 = -32803;

/// The directives, as source writes them.
const DIRECTIVES: [&str; 28] = [
    "org", "equ", "set", "db", "dw", "ds", "align", "rept", "for", "macro", "endm", "if", "elseif", "else", "endif", "include", "incbin",
    "assert", "code", "const", "data", "bss", "opt", "struct", "union", "ends", "samepage", "endsamepage",
];
/// The functions an operand can call.
const FUNCTIONS: [&str; 16] =
    ["high", "low", "start", "end", "size", "same", "reg", "pair", "const", "blank", "count", "defined", "cstr", "upper", "lower", "reverse"];
const REGISTERS: [&str; 10] = ["a", "b", "c", "d", "e", "h", "l", "m", "sp", "psw"];

/// LSP's CompletionItemKind for each kind of word.
const KEYWORD: i64 = 14;
const OPERATOR: i64 = 24;
const VALUE: i64 = 12;
const MODULE: i64 = 9;
const FILE: i64 = 17;

/// Where on a line a word is.
enum Role<'t> {
    /// In the first column: a label being defined.
    Label,
    /// Where the mnemonic, directive or macro goes.
    Head,
    /// In the operands of the line's mnemonic, directive or macro.
    Operands(&'t str),
}

/// The role of a word that starts at byte `start` of `line`, and where
/// the head word of the line ends.
fn role(line: &str, start: usize) -> (Role<'_>, usize) {
    let words: Vec<(usize, usize)> = text::words(line).into_iter().filter(|w| w.0 == Word::Name).map(|(_, s, e)| (s, e)).collect();
    // A label in the first column, `@:` too, comes before the head word.
    let labelled = line.starts_with(|c: char| !c.is_whitespace()) && words.first().is_some_and(|w| w.0 == 0);
    let head = if labelled { words.get(1) } else { words.first() };
    if start == 0 && labelled {
        return (Role::Label, 0);
    }
    match head {
        Some(&(s, e)) if start > e => (Role::Operands(&line[s..e]), e),
        _ => (Role::Head, 0),
    }
}

/// `name` is one a renamed name can be: no mnemonic, directive or
/// register, no dot, and not `@` alone. A function is one only when called.
fn is_new_name(name: &str) -> bool {
    let mut chars = name.chars();
    let lower = name.to_lowercase();
    name != "@"
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic() || "_?@".contains(c))
        && chars.all(|c| c.is_alphanumeric() || "_?@".contains(c))
        && !OPS.iter().any(|op| op.mnemonic.eq_ignore_ascii_case(name))
        && ![&DIRECTIVES[..], &REGISTERS[..]].iter().any(|list| list.contains(&lower.as_str()))
}

/// LSP's CompletionItemKind for `d`.
fn item_kind(d: &Def) -> i64 {
    match d.kind {
        NameKind::Label => 3,                     // Function
        NameKind::Macro => 2,                     // Method
        NameKind::Equ => 21,                      // Constant
        NameKind::Set => 6,                       // Variable
        NameKind::Field => 5,                     // Field
        NameKind::Struct | NameKind::Union => 22, // Struct
    }
}

/// The commas in `text` outside quotes, parentheses and brackets.
fn commas(text: &str) -> usize {
    let (mut quoted, mut depth, mut n) = (false, 0i32, 0);
    for c in text.chars() {
        match c {
            '\'' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => depth -= 1,
            ',' if !quoted && depth <= 0 => n += 1,
            _ => {}
        }
    }
    n
}

impl Query<'_> {
    /// The names of the programs `path` is in; of all, if it is in none.
    fn programs(&self, path: &Path) -> Vec<&Names> {
        let all: Vec<(&std::path::PathBuf, &Names)> = self.project.names.iter().collect();
        let mine: Vec<&Names> = all
            .iter()
            .filter(|(main, n)| {
                *main == path || n.defs.iter().any(|d| d.path == path) || n.uses.iter().any(|u| u.path == path) || n.placed.iter().any(|p| p.path == path)
            })
            .map(|(_, n)| *n)
            .collect();
        if mine.is_empty() { all.into_iter().map(|(_, n)| n).collect() } else { mine }
    }

    /// The text of the request's line, and the byte of its position.
    fn cursor(&mut self, params: &Value) -> Option<(std::path::PathBuf, usize, String, usize)> {
        let path = uri::to_path(params["textDocument"]["uri"].as_str()?)?;
        let line = params["position"]["line"].as_i64()? as usize;
        let text = self.line(&path, line);
        let byte = text::byte_at(&text, params["position"]["character"].as_i64()? as usize, self.utf8);
        Some((path, line, text, byte))
    }

    /// `textDocument/completion`: on a line's head, the mnemonics,
    /// directives and macros; in its operands, the names it can use there,
    /// the registers and the functions; in an INCLUDE's quotes, files.
    pub fn completion(&mut self, params: &Value) -> Value {
        let Some((path, line, text, cursor)) = self.cursor(params) else { return Value::Null };
        let before = &text[..cursor];
        if text::strip_comment(before).len() < before.len() {
            return Vec::<Value>::new().into();
        }
        // A word being typed: back to its first character.
        let start = before.char_indices().rev().take_while(|(_, c)| c.is_alphanumeric() || "_?@.".contains(*c)).last().map_or(cursor, |(i, _)| i);
        let prefix = &before[start..];
        let (role, head_end) = role(&text, start);
        if let Role::Operands(head) = role
            && (head.eq_ignore_ascii_case("include") || head.eq_ignore_ascii_case("incbin"))
        {
            let rest = text[head_end..cursor].trim_start();
            return match rest.strip_prefix('\'').filter(|r| !r.contains('\'')) {
                Some(typed) => self.files(&path, line, &text, cursor - typed.len(), cursor, head.eq_ignore_ascii_case("include")),
                None => Vec::<Value>::new().into(),
            };
        }
        if before.matches('\'').count() % 2 == 1 || prefix.starts_with(|c: char| c.is_ascii_digit()) {
            return Vec::<Value>::new().into();
        }
        let upper = prefix.chars().any(|c| c.is_ascii_uppercase()) && !prefix.chars().any(|c| c.is_ascii_lowercase());
        let case = |w: &str| if upper { w.to_uppercase() } else { w.to_string() };
        let mut items: BTreeMap<String, (i64, Option<String>)> = BTreeMap::new();
        match role {
            Role::Label => {}
            Role::Head => {
                for op in OPS.iter().filter(|op| op.documented) {
                    let shape = operand_shape(op.mnemonic).unwrap_or_default().join(", ");
                    items.insert(case(&op.mnemonic.to_lowercase()), (KEYWORD, Some(shape)));
                }
                for d in DIRECTIVES {
                    items.insert(case(d), (KEYWORD, Some("directive".to_string())));
                }
                for (written, d) in self.visible(&path, line, prefix) {
                    if d.kind == NameKind::Macro {
                        items.insert(written, (item_kind(d), value_text(d)));
                    }
                }
            }
            Role::Operands(head) => {
                if operand_shape(head).is_some_and(|s| s.contains(&"r") || s.contains(&"rp")) && !prefix.contains('.') {
                    for r in REGISTERS {
                        items.insert(case(r), (VALUE, Some("register".to_string())));
                    }
                }
                if !prefix.contains('.') {
                    for f in FUNCTIONS {
                        items.insert(case(f), (OPERATOR, Some("function".to_string())));
                    }
                }
                for (written, d) in self.visible(&path, line, prefix) {
                    if d.kind != NameKind::Macro {
                        items.insert(written, (item_kind(d), value_text(d)));
                    }
                }
                if !prefix.contains('.') {
                    for names in self.programs(&path) {
                        for (file, ns) in &names.namespaces {
                            if file != &path {
                                let first = ns.split('.').next().unwrap_or(ns);
                                items.entry(first.to_string()).or_insert((MODULE, Some("namespace".to_string())));
                            }
                        }
                    }
                }
            }
        }
        let range = self.range(line, &text, start, cursor);
        items
            .into_iter()
            .map(|(label, (kind, detail))| {
                let mut item = obj([
                    ("label", label.as_str().into()),
                    ("kind", kind.into()),
                    ("textEdit", obj([("range", range.clone()), ("newText", label.as_str().into())])),
                ]);
                if let Some(detail) = detail.filter(|d| !d.is_empty()) {
                    item.set("detail", detail.into());
                }
                item
            })
            .collect::<Vec<_>>()
            .into()
    }

    /// The names file `path` can write at line `line` that complete
    /// `prefix`: as written there, with their definitions. A local label
    /// (`.loop`) is the current label's; a name with a dot (`rb.t`) one
    /// under that name; a name without one, a name the file writes so.
    fn visible(&self, path: &Path, line: usize, prefix: &str) -> Vec<(String, &Def)> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for names in self.programs(path) {
            let shown = |d: &Def| names.shown(&d.name, path).to_string();
            // The label local labels are under here: the last one above.
            let scope = names
                .defs
                .iter()
                .filter(|d| d.path == path && d.line <= line && d.kind == NameKind::Label && !shown(d).contains('.'))
                .max_by_key(|d| d.line)
                .map(shown);
            for d in &names.defs {
                let written = shown(d);
                let label = if let Some(local) = prefix.strip_prefix('.') {
                    let Some(scope) = &scope else { continue };
                    match written.strip_prefix(scope.as_str()).and_then(|r| r.strip_prefix('.')) {
                        Some(rest) if !rest.contains('.') && rest.starts_with(local.split('.').next().unwrap_or("")) => format!(".{rest}"),
                        _ => continue,
                    }
                } else if let Some(dot) = prefix.rfind('.') {
                    if !written.starts_with(&prefix[..=dot]) {
                        continue;
                    }
                    written
                } else if written.contains('.') || d.kind == NameKind::Field {
                    continue;
                } else {
                    written
                };
                if seen.insert(label.clone()) {
                    out.push((label, d));
                }
            }
        }
        out
    }

    /// The files an INCLUDE (`include`) or INCBIN on line `line` can name,
    /// for the name typed from byte `start` to `cursor`: the SDK's own,
    /// and those in the file's folder and the include folders.
    fn files(&mut self, path: &Path, line: usize, text: &str, start: usize, cursor: usize, include: bool) -> Value {
        let source = |p: &Path| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("asm") || e.eq_ignore_ascii_case("inc"));
        let mut names: Vec<String> = Vec::new();
        if include {
            names.extend(juku_asm::HEADERS.iter().map(|h| h.0.to_string()));
        }
        let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for base in std::iter::once(folder).chain(self.project.include.iter().cloned()) {
            let mut found = Vec::new();
            list_files(&base, &base, 0, &mut found);
            for (rel, full) in found {
                if full != path && source(&full) == include {
                    names.push(rel);
                }
            }
        }
        text::sort(&mut names);
        names.dedup();
        let range = self.range(line, text, start, cursor);
        names
            .into_iter()
            .map(|n| obj([("label", n.as_str().into()), ("kind", FILE.into()), ("textEdit", obj([("range", range.clone()), ("newText", n.as_str().into())]))]))
            .collect::<Vec<_>>()
            .into()
    }

    /// `textDocument/signatureHelp`: the operands of the instruction or
    /// macro being written, and which one the cursor is in.
    pub fn signature_help(&mut self, params: &Value) -> Value {
        let Some((path, line, text, cursor)) = self.cursor(params) else { return Value::Null };
        let before = &text[..cursor];
        if text::strip_comment(before).len() < before.len() {
            return Value::Null;
        }
        let (Role::Operands(head), head_end) = role(&text, cursor.max(1)) else { return Value::Null };
        if !text[head_end..].starts_with(char::is_whitespace) || cursor <= head_end {
            return Value::Null;
        }
        let active = commas(&before[head_end..]);
        let macro_def = self.visible(&path, line, "").into_iter().find(|(w, d)| d.kind == NameKind::Macro && w == head).map(|(_, d)| d.clone());
        let operands: Vec<String> = match &macro_def {
            Some(d) => {
                let def_line = self.line(&d.path, d.line);
                let code = text::strip_comment(&def_line);
                let lower = code.to_lowercase();
                let at = lower.find("macro").map_or(code.len(), |i| i + 5);
                code[at..].split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
            }
            None => match operand_shape(head) {
                Some(shape) => shape.into_iter().map(str::to_string).collect(),
                None => return Value::Null,
            },
        };
        if operands.is_empty() {
            return Value::Null;
        }
        let mut label = format!("{head} ");
        let mut parameters = Vec::new();
        for (i, o) in operands.iter().enumerate() {
            if i > 0 {
                label += ", ";
            }
            let start = label.encode_utf16().count();
            label += o;
            parameters.push(obj([("label", vec![Value::from(start), Value::from(label.encode_utf16().count())].into())]));
        }
        let mut signature = obj([("label", label.as_str().into()), ("parameters", parameters.into())]);
        if let Some(d) = &macro_def {
            let doc = self.describe(d);
            signature.set("documentation", obj([("kind", "markdown".into()), ("value", doc.into())]));
        }
        obj([
            ("signatures", vec![signature].into()),
            ("activeSignature", 0.into()),
            ("activeParameter", active.min(operands.len() - 1).into()),
        ])
    }

    /// The definitions of `name` in `mains`, if it can be renamed: defined
    /// in the workspace, not in the SDK's include files, and not a field.
    fn renamable(&self, name: &str, mains: &[&std::path::PathBuf]) -> Result<Vec<Def>, (i64, String)> {
        let defs: Vec<Def> = self.defs(name, mains).into_iter().cloned().collect();
        let failed = |m: String| Err((REQUEST_FAILED, m));
        if defs.is_empty() {
            return failed(format!("{name} is not defined here"));
        }
        if let Some(dir) = &self.project.headers
            && defs.iter().any(|d| d.path.starts_with(dir))
        {
            return failed(format!("{name} is the SDK's: its include files are not changed"));
        }
        if defs.iter().any(|d| d.kind == NameKind::Field) {
            return failed(format!("{name} is a field: each label laid out with its struct has it too, so it is renamed by hand"));
        }
        Ok(defs)
    }

    /// `textDocument/prepareRename`: the part of the word at the cursor a
    /// rename changes, its last name: `loop` of `.loop`, `tto` of `rb.tto`.
    pub fn prepare_rename(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let Some(at) = self.at(params).filter(|a| a.kind == Word::Name) else { return Ok(Value::Null) };
        let Some((name, mains)) = self.resolve(&at) else { return Ok(Value::Null) };
        self.renamable(&name, &mains)?;
        let word = at.word();
        let start = at.start + word.rfind('.').map_or(0, |i| i + 1);
        Ok(obj([("range", self.range(at.line, &at.text, start, at.end)), ("placeholder", at.text[start..at.end].into())]))
    }

    /// `textDocument/rename`: the name's last part, in its definitions and
    /// every use, and in the names under it (`main.loop` with `main`).
    pub fn rename(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let failed = |m: String| Err((REQUEST_FAILED, m));
        let Some(at) = self.at(params).filter(|a| a.kind == Word::Name) else { return failed("no name here".to_string()) };
        let Some((name, mains)) = self.resolve(&at) else { return failed(format!("{} is not a name here", at.word())) };
        self.renamable(&name, &mains)?;
        let new = params["newName"].as_str().unwrap_or("").trim().trim_start_matches('.');
        if !is_new_name(new) {
            return failed(format!("{new} cannot be a name: a letter, _, ? or @, then those or digits, but not @ alone, and no mnemonic, directive or register"));
        }
        let old = name.rsplit('.').next().unwrap_or(&name).to_string();
        let renamed = format!("{}{new}", &name[..name.len() - old.len()]);
        for main in &mains {
            if self.project.names[*main].defs.iter().any(|d| d.name == renamed) {
                return failed(format!("{renamed} is defined already"));
            }
        }
        // The name, and those under it, each with what follows the part
        // renamed: `main` in `main.loop` has `.loop` after it.
        let mut names: Vec<(String, String)> = vec![(name.clone(), String::new())];
        for main in &mains {
            for d in &self.project.names[*main].defs {
                if let Some(rest) = d.name.strip_prefix(&format!("{name}.")) {
                    names.push((d.name.clone(), format!(".{rest}")));
                }
            }
        }
        text::sort(&mut names);
        names.dedup();
        let mut edits: BTreeMap<String, Vec<(usize, usize, usize, String)>> = BTreeMap::new();
        for (full, after) in &names {
            let mut lines: Vec<(std::path::PathBuf, usize)> = Vec::new();
            for main in &mains {
                let n = &self.project.names[*main];
                lines.extend(n.uses.iter().filter(|u| &u.name == full).map(|u| (u.path.clone(), u.line)));
                lines.extend(n.defs.iter().filter(|d| &d.name == full).map(|d| (d.path.clone(), d.line)));
            }
            text::sort(&mut lines);
            lines.dedup();
            for (path, line) in lines {
                let (text, found) = self.words_for(&path, line, full);
                for (_, end) in found {
                    let word_end = end - after.len();
                    let Some(start) = word_end.checked_sub(old.len()) else { continue };
                    if !text[..end].ends_with(&format!("{old}{after}")) || (start > 0 && !text.is_char_boundary(start)) {
                        continue;
                    }
                    // Only where the part is written: `.loop` has no `main`.
                    let before = &text[..start];
                    if !after.is_empty() && !before.ends_with(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == '.') && !before.is_empty() {
                        continue;
                    }
                    let uri = self.uri(&path);
                    edits.entry(uri).or_default().push((line, start, word_end, text.clone()));
                }
            }
        }
        let changes: Vec<(String, Value)> = edits
            .into_iter()
            .map(|(uri, mut list)| {
                text::sort(&mut list);
                list.dedup();
                let list: Vec<Value> =
                    list.iter().map(|(line, s, e, text)| obj([("range", self.range(*line, text, *s, *e)), ("newText", new.into())])).collect();
                (uri, list.into())
            })
            .collect();
        Ok(obj([("changes", Value::Object(changes))]))
    }

    /// `textDocument/foldingRange`: IF to ENDIF (each branch), MACRO, REPT
    /// and FOR to ENDM, STRUCT and UNION to ENDS, SAMEPAGE to ENDSAMEPAGE,
    /// and runs of comment lines.
    pub fn folding_ranges(&mut self, params: &Value) -> Value {
        let Some(path) = params["textDocument"]["uri"].as_str().and_then(uri::to_path) else { return Value::Null };
        let source = self.project.text(&path).unwrap_or_default();
        let lines = text::split_lines(&source);
        let mut open: Vec<(usize, &str)> = Vec::new();
        let mut out = Vec::new();
        let mut fold = |start: usize, end: usize, kind: &str| {
            if end > start {
                out.push(obj([("startLine", start.into()), ("endLine", end.into()), ("kind", kind.into())]));
            }
        };
        let mut comments: Option<usize> = None;
        for (n, line) in lines.iter().enumerate() {
            let is_comment = line.trim_start().starts_with(';');
            match (is_comment, comments) {
                (true, None) => comments = Some(n),
                (false, Some(s)) => {
                    fold(s, n - 1, "comment");
                    comments = None;
                }
                _ => {}
            }
            let head = match role(line, line.len() + 1).0 {
                Role::Operands(h) => h.to_uppercase(),
                _ => continue,
            };
            let closes = |open: &mut Vec<(usize, &str)>, kind: &str| match open.last() {
                Some(&(s, k)) if k == kind => {
                    open.pop();
                    Some(s)
                }
                _ => None,
            };
            match head.as_str() {
                "IF" => open.push((n, "IF")),
                "ELSEIF" | "ELSE" => {
                    if let Some(s) = closes(&mut open, "IF") {
                        fold(s, n - 1, "region");
                        open.push((n, "IF"));
                    }
                }
                "ENDIF" => {
                    if let Some(s) = closes(&mut open, "IF") {
                        fold(s, n - 1, "region");
                    }
                }
                "MACRO" | "REPT" | "FOR" => open.push((n, "ENDM")),
                "ENDM" => {
                    if let Some(s) = closes(&mut open, "ENDM") {
                        fold(s, n - 1, "region");
                    }
                }
                "SAMEPAGE" => open.push((n, "ENDSAMEPAGE")),
                "ENDSAMEPAGE" => {
                    if let Some(s) = closes(&mut open, "ENDSAMEPAGE") {
                        fold(s, n - 1, "region");
                    }
                }
                "STRUCT" | "UNION" => open.push((n, "ENDS")),
                "ENDS" => {
                    if let Some(s) = closes(&mut open, "ENDS") {
                        fold(s, n - 1, "region");
                    }
                }
                _ => {}
            }
        }
        if let Some(s) = comments {
            fold(s, lines.len() - 1, "comment");
        }
        out.into()
    }

    /// `textDocument/inlayHint`: after each line that places bytes, their
    /// address, and the cycles of its instructions or else how many bytes.
    pub fn inlay_hints(&mut self, params: &Value) -> Value {
        let Some(path) = params["textDocument"]["uri"].as_str().and_then(uri::to_path) else { return Value::Null };
        let first = params["range"]["start"]["line"].as_i64().unwrap_or(0).max(0) as usize;
        let last = params["range"]["end"]["line"].as_i64().unwrap_or(i64::MAX).max(0) as usize;
        let mut placed = BTreeMap::new();
        for names in self.programs(&path) {
            for p in names.placed.iter().filter(|p| p.path == path && (first..=last).contains(&p.line)) {
                placed.entry(p.line).or_insert((p.address, p.size, p.cycles));
            }
        }
        let mut out = Vec::new();
        for (line, (address, size, cycles)) in placed {
            let text = self.line(&path, line);
            let end = text.trim_end().len();
            let mut label = hex_digits(address & 0xFFFF, 4);
            match cycles {
                Some(c) => label += &format!(" · {c} cycles"),
                None if size == 1 => label += " · 1 byte",
                None if size > 1 => label += &format!(" · {size} bytes"),
                None => {}
            }
            out.push(obj([("position", self.position(line, &text, end)), ("label", label.into()), ("paddingLeft", true.into())]));
        }
        out.into()
    }
}

/// Adds the files in `folder` and its folders, two deep, to `out`: each
/// name from `base`, with `/`, and the path.
fn list_files(base: &Path, folder: &Path, depth: usize, out: &mut Vec<(String, std::path::PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = crate::uri::normal(&entry.path());
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || out.len() > 500 {
            continue;
        }
        match entry.file_type() {
            Ok(t) if t.is_dir() && depth < 2 && name != "target" && name != "node_modules" => list_files(base, &path, depth + 1, out),
            Ok(t) if t.is_file() => {
                if let Ok(rel) = path.strip_prefix(base) {
                    out.push((rel.to_string_lossy().replace('\\', "/"), path));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles() {
        let head = |line: &str, at: usize| match role(line, at).0 {
            Role::Label => "label".to_string(),
            Role::Head => "head".to_string(),
            Role::Operands(h) => format!("operands of {h}"),
        };
        assert_eq!(head("loop", 0), "label");
        assert_eq!(head("loop:   mv", 8), "head");
        assert_eq!(head("        mv", 8), "head");
        assert_eq!(head(":       mvi a, x", 8), "head");
        assert_eq!(head("loop:   mvi a, x", 15), "operands of mvi");
        assert_eq!(head("name    macro a, b", 14), "operands of macro");
        assert_eq!(head("        call ", 13), "operands of call");
    }

    #[test]
    fn names_and_commas() {
        assert!(is_new_name("draw") && is_new_name("_x?1"));
        assert!(!is_new_name("mov") && !is_new_name("A") && is_new_name("high") && !is_new_name("a.b") && !is_new_name("1x") && !is_new_name(""));
        assert_eq!(commas("a, 'x,y', (1, 2), [3, 4], b"), 4);
    }
}
