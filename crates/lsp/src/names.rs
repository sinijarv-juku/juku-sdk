//! A main file's names, from its index, with files as paths and lines from
//! 0; and the SDK's built-in include files, written out so that an editor
//! can open them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use juku_asm::{HEADER_PREFIX, Index, NameKind};

use crate::uri::normal;

/// A name and where it is defined.
#[derive(Clone, Debug)]
pub struct Def {
    pub name: String,
    pub kind: NameKind,
    pub value: Option<i64>,
    pub path: PathBuf,
    pub line: usize,
}

/// A line that uses a name.
#[derive(Clone, Debug)]
pub struct Ref {
    pub name: String,
    pub path: PathBuf,
    pub line: usize,
}

/// A main file's names.
#[derive(Default)]
pub struct Names {
    pub defs: Vec<Def>,
    pub uses: Vec<Ref>,
    /// The namespace each file's names are under, for the files that have
    /// one: `rb` for `include 'rombios.inc' as rb`.
    pub namespaces: HashMap<PathBuf, String>,
    /// What each line places: its file and line, the address, the bytes,
    /// the cycles.
    pub placed: Vec<Placed>,
}

/// What a line places in memory.
#[derive(Clone, Debug)]
pub struct Placed {
    pub path: PathBuf,
    pub line: usize,
    pub address: i64,
    pub size: i64,
    pub cycles: Option<u32>,
}

impl Names {
    /// `index`'s names, its built-in include files in `headers`.
    pub fn new(main: &Path, index: &Index, headers: Option<&Path>) -> Names {
        let path = |file: &str| match file.strip_prefix(HEADER_PREFIX) {
            Some(name) => headers.map_or_else(|| PathBuf::from(file), |dir| dir.join(name)),
            None => normal(Path::new(file)),
        };
        let defs = index
            .definitions
            .iter()
            .map(|d| Def { name: d.name.clone(), kind: d.kind, value: d.value, path: path(&d.file), line: d.line - 1 })
            .collect();
        let uses = index.uses.iter().map(|u| Ref { name: u.name.clone(), path: path(&u.file), line: u.line - 1 }).collect();
        // A file's prefix is its includer's, and the INCLUDE's namespace.
        let mut namespaces: HashMap<PathBuf, String> = HashMap::new();
        let mut prefixes = HashMap::from([(normal(main), String::new())]);
        for inc in &index.includes {
            let Some(outer) = prefixes.get(&path(&inc.file)).cloned() else { continue };
            let inner = match (&inc.namespace, outer.is_empty()) {
                (Some(ns), true) => ns.clone(),
                (Some(ns), false) => format!("{outer}.{ns}"),
                (None, _) => outer,
            };
            let included = path(&inc.path);
            if !inner.is_empty() {
                namespaces.entry(included.clone()).or_insert_with(|| inner.clone());
            }
            prefixes.entry(included).or_insert(inner);
        }
        let placed = index
            .placed
            .iter()
            .map(|p| Placed { path: path(&p.file), line: p.line - 1, address: p.address, size: p.size, cycles: p.cycles })
            .collect();
        Names { defs, uses, namespaces, placed }
    }

    /// `name` as file `path` writes it: without the file's namespace.
    pub fn shown<'a>(&self, name: &'a str, path: &Path) -> &'a str {
        match self.namespaces.get(path) {
            Some(ns) => name.strip_prefix(ns.as_str()).and_then(|n| n.strip_prefix('.')).unwrap_or(name),
            None => name,
        }
    }
}

/// Whether the word `word` can be a use of the full name `name`: the
/// name itself, or its end after a dot (`.loop` of `main.loop`, `tto` of
/// `rb.tto`).
pub fn refers(name: &str, word: &str) -> bool {
    let word = word.trim_start_matches('.');
    !word.is_empty() && (name == word || name.strip_suffix(word).is_some_and(|rest| rest.ends_with('.')))
}

/// The folder the SDK's built-in include files are written to, read-only,
/// so that an editor can open them: one for each SDK version, in the
/// system's temporary folder. None if they cannot be written.
pub fn write_headers() -> Option<PathBuf> {
    let dir = normal(&std::env::temp_dir().join(format!("juku-sdk-{}", env!("CARGO_PKG_VERSION"))));
    std::fs::create_dir_all(&dir).ok()?;
    for (name, text) in juku_asm::HEADERS {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).is_ok_and(|t| t == text) {
            continue;
        }
        if let Ok(meta) = std::fs::metadata(&path) {
            let mut perms = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(&path, perms);
        }
        std::fs::write(&path, text).ok()?;
        if let Ok(meta) = std::fs::metadata(&path) {
            let mut perms = meta.permissions();
            perms.set_readonly(true);
            let _ = std::fs::set_permissions(&path, perms);
        }
    }
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_refer() {
        assert!(refers("main.loop", ".loop"));
        assert!(refers("rb.tto", "tto") && refers("rb.tto", "rb.tto") && refers("x.rb.tto", "rb.tto"));
        assert!(refers("draw.x", "..x"), "a caller's local in a macro");
        assert!(!refers("rb.tto", "b.tto") && !refers("tto", "rb.tto") && !refers("x", "."));
    }
}
