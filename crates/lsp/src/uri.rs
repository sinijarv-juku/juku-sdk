//! `file:` URIs, as LSP names files, and paths.
//!
//! A path is absolute, with `.` and `..` taken out; on Windows with `\`
//! between names and the drive letter in upper case, since editors send it
//! in either case (VS Code: `file:///d%3A/x`).

use std::path::{Component, Path, PathBuf};

/// The path a `file:` URI names; none for another scheme or a relative one.
pub fn to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://").or_else(|| uri.strip_prefix("FILE://"))?;
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let decoded = decode(rest)?;
    // file:///path, or file://host/path for a share.
    let i = decoded.find('/')?;
    let (host, path) = (&decoded[..i], &decoded[i..]);
    let path = if cfg!(windows) {
        let name = path.trim_start_matches('/');
        let drive = name.as_bytes();
        let text = if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
            format!(r"\\{host}\{name}")
        } else if drive.len() >= 2 && drive[0].is_ascii_alphabetic() && drive[1] == b':' {
            name.to_string()
        } else {
            return None;
        };
        PathBuf::from(text.replace('/', "\\"))
    } else {
        if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
            return None;
        }
        PathBuf::from(path)
    };
    path.is_absolute().then(|| normal(&path))
}

/// The `file:` URI of absolute path `path`.
pub fn from_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    let (share, name) = match text.strip_prefix("//") {
        Some(share) => (true, share),
        None => (false, text.as_str()),
    };
    if !share && !name.starts_with('/') {
        out.push('/');
    }
    for (i, b) in name.bytes().enumerate() {
        let drive_colon = !share && b == b':' && i == 1;
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) || drive_colon {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `path`, absolute, with `.` and `..` taken out without looking at the
/// file system, and on Windows the drive letter in upper case.
pub fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Prefix(p) => {
                let text = p.as_os_str().to_string_lossy();
                let b = text.as_bytes();
                if b.len() == 2 && b[1] == b':' {
                    out.push(text.to_uppercase());
                } else {
                    out.push(p.as_os_str());
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// `text` with each `%XX` the byte it stands for, if that is UTF-8.
fn decode(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows() {
        assert_eq!(to_path("file:///d%3A/Juku/a%20b.asm"), Some(PathBuf::from(r"D:\Juku\a b.asm")));
        assert_eq!(to_path("file:///D:/Juku/x/../y.asm"), Some(PathBuf::from(r"D:\Juku\y.asm")));
        assert_eq!(to_path("file://server/share/y.asm"), Some(PathBuf::from(r"\\server\share\y.asm")));
        assert_eq!(from_path(Path::new(r"D:\Juku\a b.asm")), "file:///D:/Juku/a%20b.asm");
        assert_eq!(from_path(Path::new(r"\\server\share\y.asm")), "file://server/share/y.asm");
        assert_eq!(to_path("file:///juku.asm"), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn unix() {
        assert_eq!(to_path("file:///home/me/a%20b.asm"), Some(PathBuf::from("/home/me/a b.asm")));
        assert_eq!(to_path("file://localhost/x/./y/../z.asm"), Some(PathBuf::from("/x/z.asm")));
        assert_eq!(from_path(Path::new("/home/me/a b.asm")), "file:///home/me/a%20b.asm");
        assert_eq!(to_path("file://server/x.asm"), None);
    }

    #[test]
    fn round_trip() {
        let path = normal(&std::env::temp_dir().join("ü ä").join("x#1.asm"));
        assert_eq!(to_path(&from_path(&path)), Some(path));
        assert_eq!(to_path("untitled:Untitled-1"), None);
        assert_eq!(to_path("file:///a%zz"), None);
    }
}
