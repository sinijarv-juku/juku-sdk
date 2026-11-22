//! juku-disk: lists, copies off, puts on and removes the files on EKDOS disk
//! images, and makes blank ones.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use juku_disk::{Disk, File};

const USAGE: &str = "\
Usage: juku-disk ls IMAGE                       list the files
       juku-disk get IMAGE NAME... [-o FOLDER] [-f]
                                             copy files off; NAME may use * and ?
       juku-disk get IMAGE -a [-o FOLDER] [-f]  copy every file off
       juku-disk put IMAGE FILE...              put files on, replacing any of the name
       juku-disk rm IMAGE NAME...               remove files; NAME may use * and ?
       juku-disk new IMAGE [--system FROM] [-f] a blank formatted disk, with the
                                             system tracks of FROM to boot from
       juku-disk --version

IMAGE is an EKDOS disk: 800 KB double-sided or 400 KB single-sided; files
are put on and removed from EKDOS's double-sided ones only. A file of user n other
than 0 is shown and copied off as n_NAME.EXT. Files copied off keep their
whole last record: a text file ends in ^Z padding. put and rm change IMAGE
in place.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && (args[0] == "--version" || args[0] == "-V") {
        println!("juku-disk {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let result = match args.first().map(String::as_str) {
        Some("ls") if args.len() == 2 => ls(&args[1]),
        Some("get") if args.len() >= 3 => get(&args[1], &args[2..]),
        Some("put") if args.len() >= 3 => put(&args[1], &args[2..]),
        Some("rm") if args.len() >= 3 => rm(&args[1], &args[2..]),
        Some("new") if args.len() >= 2 => new(&args[1], &args[2..]),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// What went wrong, as the system says it, without "(os error n)".
fn io_error(path: &Path, e: std::io::Error) -> String {
    let text = e.to_string();
    let text = text.find(" (os error ").map_or(text.as_str(), |i| &text[..i]);
    format!("{}: {text}", path.display())
}

fn open(image: &str) -> Result<Disk, String> {
    let data = std::fs::read(image).map_err(|e| io_error(Path::new(image), e))?;
    Disk::new(data).map_err(|e| format!("{image}: {e}"))
}

fn save(image: &str, disk: &Disk) -> Result<(), String> {
    // Written beside it and renamed over it, so that a failed write leaves
    // the image as it was.
    let path = Path::new(image);
    let temp = path.with_extension("juk-new");
    std::fs::write(&temp, disk.bytes()).map_err(|e| io_error(&temp, e))?;
    std::fs::rename(&temp, path).map_err(|e| io_error(path, e))
}

fn ls(image: &str) -> Result<bool, String> {
    let disk = open(image)?;
    let files = disk.files();
    println!(
        "{image}: {} {}, {} KB blocks, {} KB free",
        files.len(),
        if files.len() == 1 { "file" } else { "files" },
        disk.block_size() / 1024,
        disk.free() / 1024
    );
    for f in &files {
        let attributes = [(f.read_only, "R/O"), (f.system, "SYS")].iter().filter(|a| a.0).map(|a| a.1).collect::<Vec<_>>();
        let line = format!("{:16} {:7} {}", f.shown(), f.size, attributes.join(" "));
        println!("{}", line.trim_end());
    }
    Ok(true)
}

/// Whether `name` matches `pattern`, ignoring case: `*` is any run of
/// characters, `?` any one.
fn matches(pattern: &str, name: &str) -> bool {
    fn go(p: &[u8], n: &[u8]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some(b'*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some(b'?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b) && go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    go(pattern.as_bytes(), name.as_bytes())
}

/// The files `patterns` name, by their shown name or, for any user, their
/// own; false for a pattern that names none.
fn chosen<'a>(files: &'a [File], patterns: &[&String]) -> (Vec<&'a File>, bool) {
    let mut found = vec![false; patterns.len()];
    let mut out = Vec::new();
    for f in files {
        let mut hit = false;
        for (i, p) in patterns.iter().enumerate() {
            if matches(p, &f.shown()) || matches(p, &f.name) {
                found[i] = true;
                hit = true;
            }
        }
        if hit {
            out.push(f);
        }
    }
    for (i, p) in patterns.iter().enumerate() {
        if !found[i] {
            eprintln!("{p}: no such file");
        }
    }
    (out, found.iter().all(|&f| f))
}

fn get(image: &str, args: &[String]) -> Result<bool, String> {
    let (mut all, mut force, mut folder) = (false, false, PathBuf::from("."));
    let mut patterns = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-a" => all = true,
            "-f" => force = true,
            "-o" if i + 1 < args.len() => {
                folder = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            _ => patterns.push(&args[i]),
        }
        i += 1;
    }
    if all == !patterns.is_empty() {
        return Err(USAGE.to_string());
    }
    let disk = open(image)?;
    let files = disk.files();
    let (chosen, mut ok) = if all { (files.iter().collect(), true) } else { chosen(&files, &patterns) };
    std::fs::create_dir_all(&folder).map_err(|e| io_error(&folder, e))?;
    for f in chosen {
        let path = folder.join(f.shown().replace('/', "_"));
        if path.exists() && !force {
            eprintln!("{} exists; skipped (-f writes over it)", path.display());
            ok = false;
            continue;
        }
        let data = match disk.contents(f) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("{image}: {e}");
                ok = false;
                continue;
            }
        };
        std::fs::write(&path, &data).map_err(|e| io_error(&path, e))?;
        println!("{} {}", path.display(), data.len());
    }
    Ok(ok)
}

fn put(image: &str, files: &[String]) -> Result<bool, String> {
    let mut disk = open(image)?;
    for path in files {
        let path = Path::new(path);
        let data = std::fs::read(path).map_err(|e| io_error(path, e))?;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        disk.put(&name, &data).map_err(|e| format!("{image}: {e}"))?;
        println!("{} {}", name.to_ascii_uppercase(), data.len());
    }
    save(image, &disk)?;
    Ok(true)
}

fn rm(image: &str, patterns: &[String]) -> Result<bool, String> {
    let mut disk = open(image)?;
    let files = disk.files();
    let (chosen, ok) = chosen(&files, &patterns.iter().collect::<Vec<_>>());
    for f in chosen {
        disk.remove(f).map_err(|e| format!("{image}: {e}"))?;
        println!("{}", f.shown());
    }
    save(image, &disk)?;
    Ok(ok)
}

fn new(image: &str, args: &[String]) -> Result<bool, String> {
    let (mut force, mut system) = (false, None);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-f" => force = true,
            "--system" if i + 1 < args.len() => {
                system = Some(&args[i + 1]);
                i += 1;
            }
            _ => return Err(USAGE.to_string()),
        }
        i += 1;
    }
    if Path::new(image).exists() && !force {
        return Err(format!("{image} exists (-f writes over it)"));
    }
    let mut disk = Disk::blank();
    if let Some(from) = system {
        disk.copy_system(&open(from)?).map_err(|e| format!("{from}: {e}"))?;
    }
    std::fs::write(image, disk.bytes()).map_err(|e| io_error(Path::new(image), e))?;
    Ok(true)
}
