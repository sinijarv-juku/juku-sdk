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
in place. The options can come anywhere after the command, before IMAGE
too; after --, everything is a name.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && (args[0] == "--version" || args[0] == "-V") {
        println!("juku-disk {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let result = match args.first().map(String::as_str) {
        // Each command's switches, and its options that take a value.
        Some("ls") => Options::parse(&args[1..], &[], &[], (1, 1), "ls takes an image").and_then(|o| ls(&o)),
        Some("get") => Options::parse(&args[1..], &["-a", "-f"], &["-o"], (1, usize::MAX), "get takes an image").and_then(|o| get(&o)),
        Some("put") => Options::parse(&args[1..], &[], &[], (2, usize::MAX), "put takes an image and files").and_then(|o| put(&o)),
        Some("rm") => Options::parse(&args[1..], &[], &[], (2, usize::MAX), "rm takes an image and names").and_then(|o| rm(&o)),
        Some("new") => Options::parse(&args[1..], &["-f"], &["--system"], (1, 1), "new takes an image").and_then(|o| new(&o)),
        Some(other) => Err(format!("juku-disk: unknown command {other}\n\n{USAGE}")),
        None => Err(USAGE.to_string()),
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

/// A command's arguments: the image, the names or files after it, and the
/// options, which can come anywhere.
struct Options {
    image: String,
    names: Vec<String>,
    switches: Vec<&'static str>,
    values: Vec<(&'static str, String)>,
}

impl Options {
    /// `args` after the command, which takes the `switches`, the options
    /// with a value `valued`, and from `count.0` to `count.1` files and
    /// names, the image first, as `takes` says.
    fn parse(args: &[String], switches: &[&'static str], valued: &[&'static str], count: (usize, usize), takes: &str) -> Result<Options, String> {
        let usage = |e: String| format!("juku-disk: {e}\n\n{USAGE}");
        let mut out = Options { image: String::new(), names: Vec::new(), switches: Vec::new(), values: Vec::new() };
        let mut positional = Vec::new();
        let mut i = 0;
        let mut options = true;
        while i < args.len() {
            let arg = args[i].as_str();
            i += 1;
            if !options || !arg.starts_with('-') || arg == "-" {
                positional.push(arg.to_string());
            } else if arg == "--" {
                options = false;
            } else if let Some(&s) = switches.iter().find(|&&s| s == arg) {
                out.switches.push(s);
            } else if let Some(&v) = valued.iter().find(|&&v| v == arg) {
                let value = args.get(i).ok_or_else(|| usage(format!("{arg} takes a value")))?;
                out.values.push((v, value.clone()));
                i += 1;
            } else {
                return Err(usage(format!("unknown option {arg}")));
            }
        }
        if positional.len() < count.0 || positional.len() > count.1 {
            return Err(usage(takes.to_string()));
        }
        out.image = positional.remove(0);
        out.names = positional;
        Ok(out)
    }

    fn has(&self, switch: &str) -> bool {
        self.switches.contains(&switch)
    }

    fn value(&self, option: &str) -> Option<&String> {
        self.values.iter().rev().find(|v| v.0 == option).map(|v| &v.1)
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

fn ls(o: &Options) -> Result<bool, String> {
    let image = &o.image;
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

fn get(o: &Options) -> Result<bool, String> {
    let image = &o.image;
    let (all, force) = (o.has("-a"), o.has("-f"));
    let folder = o.value("-o").map_or_else(|| PathBuf::from("."), PathBuf::from);
    let patterns: Vec<&String> = o.names.iter().collect();
    if all == !patterns.is_empty() {
        let e = if all { "-a or names, not both" } else { "names, or -a for every file" };
        return Err(format!("juku-disk: get takes {e}\n\n{USAGE}"));
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

fn put(o: &Options) -> Result<bool, String> {
    let image = &o.image;
    let mut disk = open(image)?;
    for path in &o.names {
        let path = Path::new(path);
        let data = std::fs::read(path).map_err(|e| io_error(path, e))?;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        disk.put(&name, &data).map_err(|e| format!("{image}: {e}"))?;
        println!("{} {}", name.to_ascii_uppercase(), data.len());
    }
    save(image, &disk)?;
    Ok(true)
}

fn rm(o: &Options) -> Result<bool, String> {
    let image = &o.image;
    let mut disk = open(image)?;
    let files = disk.files();
    let (chosen, ok) = chosen(&files, &o.names.iter().collect::<Vec<_>>());
    for f in chosen {
        disk.remove(f).map_err(|e| format!("{image}: {e}"))?;
        println!("{}", f.shown());
    }
    save(image, &disk)?;
    Ok(ok)
}

fn new(o: &Options) -> Result<bool, String> {
    let image = &o.image;
    let (force, system) = (o.has("-f"), o.value("--system"));
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
