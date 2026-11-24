//! The workspace: its source files, the documents open in the editor, which
//! file includes which, the main files, and assembling them.
//!
//! A main file is one assembled on its own: the settings' `mainFiles`, or
//! else each `.asm` file in the workspace that no other file includes. Which
//! file includes which is read from the text, the INCLUDE lines, so that
//! nothing needs assembling to know; assembling then says exactly which
//! files each main file takes. A file open in the editor that no main file
//! includes is assembled as one.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use juku_asm::{Index, Options};

use crate::names::Names;
use crate::text;
use crate::uri::normal;

/// Most source files read from the workspace's folders.
const MAX_FILES: usize = 5000;
/// Deepest folder looked in.
const MAX_DEPTH: usize = 16;

/// A document the editor has open.
pub struct Document {
    /// Its URI as the editor gave it, which messages about it use.
    pub uri: String,
    pub text: String,
}

/// An error to show: its line, from 0, and what is wrong.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Problem {
    pub line: usize,
    pub message: String,
}

#[derive(Default)]
pub struct Project {
    /// The workspace's folders.
    pub folders: Vec<PathBuf>,
    /// The include folders, and the main files if the settings name them.
    pub include: Vec<PathBuf>,
    pub main_files: Vec<PathBuf>,
    pub open: HashMap<PathBuf, Document>,
    /// The files each source file's INCLUDE lines name, as found.
    includes: HashMap<PathBuf, Vec<PathBuf>>,
    /// The files changed since the last assembling.
    dirty: HashSet<PathBuf>,
    /// Each main file's index, from its last assembling, and its names.
    pub indexes: HashMap<PathBuf, Index>,
    pub names: HashMap<PathBuf, Names>,
    /// Where the SDK's built-in include files are written out.
    pub headers: Option<PathBuf>,
    /// The problems last sent for each file, none for no file.
    shown: HashMap<PathBuf, Vec<Problem>>,
}

impl Project {
    /// Reads the workspace's source files, and marks every one changed.
    pub fn scan(&mut self) {
        self.includes.clear();
        let mut files = Vec::new();
        for folder in self.folders.clone() {
            walk(&folder, 0, &mut files);
        }
        for file in files {
            self.read(&file);
        }
        for file in self.open.keys().cloned().collect::<Vec<_>>() {
            self.read(&file);
        }
        self.dirty.extend(self.includes.keys().cloned());
    }

    /// Something has changed since the last assembling.
    pub fn waiting(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// The editor has opened `path`, or changed its text.
    pub fn set_text(&mut self, path: PathBuf, uri: &str, text: String) {
        self.open.insert(path.clone(), Document { uri: uri.to_string(), text });
        self.read(&path);
        self.dirty.insert(path);
    }

    /// The editor has closed `path`: its text is the disk's again.
    pub fn close(&mut self, path: &Path) {
        self.open.remove(path);
        self.changed(path);
    }

    /// `path` has changed on disk, or been made or deleted.
    pub fn changed(&mut self, path: &Path) {
        if !self.open.contains_key(path) {
            self.read(path);
        }
        self.dirty.insert(path.to_path_buf());
    }

    /// Marks every file changed: after the settings change.
    pub fn changed_all(&mut self) {
        self.dirty.extend(self.includes.keys().cloned());
        self.dirty.extend(self.indexes.keys().cloned());
    }

    /// The text of `path`: the editor's if it is open, else the disk's.
    pub fn text(&self, path: &Path) -> Option<String> {
        match self.open.get(path) {
            Some(doc) => Some(doc.text.clone()),
            None => std::fs::read_to_string(path).ok(),
        }
    }

    /// Notes the files `path`'s INCLUDE lines name; forgets it if it is
    /// gone or not a source file.
    fn read(&mut self, path: &Path) {
        match self.text(path).filter(|_| is_source(path)) {
            Some(text) => {
                let named = included(&text, path, &self.include, &|p| p.is_file() || self.open.contains_key(p));
                self.includes.insert(path.to_path_buf(), named);
            }
            None => {
                self.includes.remove(path);
            }
        }
    }

    /// The main files: the settings', or each `.asm` file no other file
    /// includes.
    pub fn mains(&self) -> Vec<PathBuf> {
        if !self.main_files.is_empty() {
            return self.main_files.clone();
        }
        let included: HashSet<&PathBuf> = self.includes.values().flatten().collect();
        let mut mains: Vec<PathBuf> = self.includes.keys().filter(|f| has_extension(f, "asm") && !included.contains(f)).cloned().collect();
        text::sort(&mut mains);
        mains
    }

    /// The main files whose programs have `path` in them; `path` itself if
    /// it is an `.asm` file in none, which is assembled on its own.
    pub fn mains_of(&self, path: &Path) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self
            .mains()
            .into_iter()
            .filter(|m| {
                let included = self.indexes.get(m).is_some_and(|i| i.includes.iter().any(|inc| normal(Path::new(&inc.path)) == path));
                included || self.closure(m).contains(path)
            })
            .collect();
        if out.is_empty() && has_extension(path, "asm") {
            out.push(path.to_path_buf());
        }
        out
    }

    /// `main` and the files it includes, and those they include.
    fn closure(&self, main: &Path) -> HashSet<PathBuf> {
        let mut seen = HashSet::new();
        let mut todo = vec![main.to_path_buf()];
        while let Some(file) = todo.pop() {
            if let Some(named) = self.includes.get(&file) {
                todo.extend(named.iter().filter(|n| !seen.contains(*n)).cloned());
            }
            seen.insert(file);
        }
        seen
    }

    /// Assembles each main file a changed file is in, and an open file in
    /// none on its own. Returns the problems of each file whose problems
    /// are no longer the ones last returned for it.
    pub fn assemble(&mut self) -> Vec<(PathBuf, Vec<Problem>)> {
        let mains = self.mains();
        let closures: Vec<(PathBuf, HashSet<PathBuf>)> = mains.iter().map(|m| (m.clone(), self.closure(m))).collect();
        let mut wanted: Vec<PathBuf> = mains.clone();
        for file in self.open.keys() {
            if !closures.iter().any(|(_, c)| c.contains(file)) {
                wanted.push(file.clone());
            }
        }
        let dirty = std::mem::take(&mut self.dirty);
        let options = Options {
            include: self.include.clone(),
            sources: self.open.iter().map(|(p, d)| (p.clone(), d.text.clone())).collect(),
        };
        self.indexes.retain(|main, _| wanted.contains(main));
        self.names.retain(|main, _| wanted.contains(main));
        for main in &wanted {
            let touched = match closures.iter().find(|(m, _)| m == main) {
                Some((_, closure)) => closure.iter().any(|f| dirty.contains(f)),
                None => dirty.contains(main),
            };
            // A main file assembled before, after the change to files its
            // INCLUDE lines do not name, is as it was.
            let stale = self.indexes.get(main).is_some_and(|i| i.includes.iter().any(|inc| dirty.contains(&normal(Path::new(&inc.path)))));
            if touched || stale || !self.indexes.contains_key(main) {
                let index = juku_asm::index(main, &options);
                self.names.insert(main.clone(), Names::new(main, &index, self.headers.as_deref()));
                self.indexes.insert(main.clone(), index);
            }
        }
        let problems = self.problems();
        let mut out = Vec::new();
        let files: HashSet<PathBuf> = problems.keys().chain(self.shown.keys()).cloned().collect();
        for file in files {
            let now = problems.get(&file).cloned().unwrap_or_default();
            if self.shown.get(&file).cloned().unwrap_or_default() != now {
                out.push((file.clone(), now.clone()));
            }
            if now.is_empty() {
                self.shown.remove(&file);
            } else {
                self.shown.insert(file, now);
            }
        }
        text::sort_by(&mut out, |a, b| a.0.cmp(&b.0));
        out
    }

    /// Each file's problems by the last assembling of each main file, and
    /// none for the other files it takes.
    fn problems(&self) -> HashMap<PathBuf, Vec<Problem>> {
        let mut out: HashMap<PathBuf, Vec<Problem>> = HashMap::new();
        for (main, index) in &self.indexes {
            out.entry(main.clone()).or_default();
            for inc in index.includes.iter().filter(|i| !is_header(&i.path)) {
                out.entry(normal(Path::new(&inc.path))).or_default();
            }
            let Err(e) = &index.program else { continue };
            let (file, problem) = match e.place() {
                Some((file, line)) if !is_header(file) => (normal(Path::new(file)), Problem { line: line - 1, message: e.message().to_string() }),
                Some((file, line)) => (main.clone(), Problem { line: 0, message: format!("{file}:{line}: {}", e.message()) }),
                None => {
                    let message = unprefixed(e.message(), main);
                    // An include file opened on its own has no code.
                    if !has_extension(main, "asm") && message == "no code" {
                        continue;
                    }
                    (main.clone(), Problem { line: 0, message })
                }
            };
            let list = out.entry(file).or_default();
            if !list.contains(&problem) {
                list.push(problem);
            }
        }
        out
    }
}

/// `message` without the main file's name it starts with: `p.asm: no
/// code` is `no code`.
fn unprefixed(message: &str, main: &Path) -> String {
    let name = main.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match message.split_once(": ") {
        Some((file, rest)) if !name.is_empty() && file.ends_with(&name) => rest.to_string(),
        _ => message.to_string(),
    }
}

/// `path` names one of the SDK's built-in include files.
pub fn is_header(path: &str) -> bool {
    path.starts_with(juku_asm::HEADER_PREFIX)
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// A file of source: `.asm` or `.inc`.
fn is_source(path: &Path) -> bool {
    has_extension(path, "asm") || has_extension(path, "inc")
}

/// Adds the source files in `folder` and the folders in it to `files`, but
/// not in hidden folders or `target` and `node_modules`.
fn walk(folder: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    text::sort_by_key(&mut entries, |e| e.file_name());
    for entry in entries {
        if files.len() >= MAX_FILES {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else { continue };
        let path = normal(&entry.path());
        if kind.is_dir() {
            if depth < MAX_DEPTH && !name.starts_with('.') && name != "target" && name != "node_modules" {
                walk(&path, depth + 1, files);
            }
        } else if is_source(&path) {
            files.push(path);
        }
    }
}

/// The files the INCLUDE lines of `text`, file `path`'s, name: each beside
/// `path`, else in the first include folder that has it, as juku-asm finds
/// them; not the SDK's own. `exists` says whether a file is there.
pub fn included(text: &str, path: &Path, include: &[PathBuf], exists: &dyn Fn(&Path) -> bool) -> Vec<PathBuf> {
    let folder = path.parent().unwrap_or(Path::new(""));
    let mut out = Vec::new();
    for line in text.split(['\n', '\r']) {
        let Some(name) = include_name(line) else { continue };
        let beside = normal(&folder.join(name));
        let found = if exists(&beside) {
            Some(beside)
        } else {
            include.iter().map(|dir| normal(&dir.join(name))).find(|p| exists(p)).or_else(|| juku_asm::header(name).is_none().then_some(beside))
        };
        if let Some(file) = found
            && !out.contains(&file)
        {
            out.push(file);
        }
    }
    out
}

/// The file an INCLUDE line names: `[label[:]] include 'name' ...`.
fn include_name(line: &str) -> Option<&str> {
    let code = line.split(';').next().unwrap_or("");
    let mut words = code.split_whitespace();
    let mut word = words.next()?;
    // A label in the first column, or one ending in a colon.
    if !code.starts_with(char::is_whitespace) || word.ends_with(':') {
        word = words.next()?;
    }
    if !word.eq_ignore_ascii_case("include") {
        return None;
    }
    let rest = code[code.find(word)? + word.len()..].trim_start();
    let name = rest.strip_prefix('\'')?;
    name.split_once('\'').map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_lines() {
        assert_eq!(include_name("        include 'a.asm'"), Some("a.asm"));
        assert_eq!(include_name("        INCLUDE 'rombios.inc' as rb ; names"), Some("rombios.inc"));
        assert_eq!(include_name("x:      include 'b.inc' (tto)"), Some("b.inc"));
        assert_eq!(include_name("        ; include 'c.asm'"), None);
        assert_eq!(include_name("include 'd.asm'"), None, "a label named include");
        assert_eq!(include_name("        db 'include'"), None);
        assert_eq!(include_name("        include"), None);
    }

    #[test]
    fn files_included() {
        let dir = normal(&std::env::temp_dir().join("juku-lsp-included"));
        let there = |p: &Path| p.ends_with("lib/b.asm") || p.ends_with("inc/c.inc");
        let text = "        include 'a.asm'\n        include 'lib/b.asm'\n        include 'c.inc'\n        include 'rombios.inc' as rb\n        include 'a.asm'\n";
        let found = included(text, &dir.join("p.asm"), &[dir.join("inc")], &there);
        assert_eq!(found, [dir.join("a.asm"), dir.join("lib").join("b.asm"), dir.join("inc").join("c.inc")]);
    }

    #[test]
    fn prefixes() {
        assert_eq!(unprefixed("/x/p.asm: no code", Path::new("/x/p.asm")), "no code");
        assert_eq!(unprefixed("the sections never settle", Path::new("p.asm")), "the sections never settle");
    }
}
