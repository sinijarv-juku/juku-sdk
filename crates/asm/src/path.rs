//! Paths, worked out as text: a relative one is from the current folder.

use std::io;
use std::path::{Component, MAIN_SEPARATOR_STR, Path, PathBuf};
use std::rc::Rc;

/// File `name` in the folder of file `path`, or `name` if it is absolute.
pub(crate) fn beside(path: &str, name: &str) -> Rc<str> {
    Path::new(path).parent().unwrap_or(Path::new("")).join(name).to_string_lossy().into()
}

/// `path` made absolute, with `.` and `..` taken out without looking at the
/// file system.
pub(crate) fn absolute(path: &Path) -> PathBuf {
    let path = std::env::current_dir().map_or_else(|_| path.to_path_buf(), |dir| dir.join(path));
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

/// Absolute `path` from absolute folder `from`; `path` itself if they have
/// no root in common, on another drive.
fn relative(path: &Path, from: &Path) -> PathBuf {
    let path: Vec<_> = path.components().collect();
    let from: Vec<_> = from.components().collect();
    let common = path.iter().zip(&from).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return path.iter().collect();
    }
    let up = std::iter::repeat_n(Component::ParentDir, from.len() - common);
    let out: PathBuf = up.chain(path[common..].iter().copied()).collect();
    if out.as_os_str().is_empty() { PathBuf::from(".") } else { out }
}

/// `path` as messages show it: from the current folder. A built-in include
/// file's is shown as it is.
pub(crate) fn shown(path: &str) -> String {
    if path.starts_with('<') {
        return path.to_string();
    }
    relative(&absolute(Path::new(path)), &absolute(Path::new(""))).display().to_string()
}

/// `path` as debug information names it: from folder `from`, absolute, with
/// `/` between names.
pub(crate) fn portable(path: &Path, from: &Path) -> String {
    relative(&absolute(path), from).to_string_lossy().replace(MAIN_SEPARATOR_STR, "/")
}

/// What went wrong, as the system says it, without "(os error n)".
pub(crate) fn strerror(e: &io::Error) -> String {
    let text = e.to_string();
    match text.find(" (os error ") {
        Some(i) => text[..i].to_string(),
        None => text,
    }
}
