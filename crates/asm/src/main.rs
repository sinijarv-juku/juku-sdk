//! juku-asm: assembles a Juku program into its image, and its listing,
//! symbols and debug information if asked.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
Usage: juku-asm [-l] [-s] [-g] [-o image.bin] [-I folder]... main.asm
       juku-asm --version

Assembles Intel 8080 source into the image of a Juku program, written
beside the source with the extension .bin (demo.asm gives demo.bin), or
as -o names it. The options and the source can come in any order.

  -l         also write a listing with cycle counts (demo.lst)
  -s         also write the symbols, as name 1234 in hex (demo.sym)
  -g         also write debug information for a debugger, in JSON
             (demo.dbg)
  -o FILE    the image's name; the other files are named as it is
  -I FOLDER  a folder INCLUDE and INCBIN look in

-l, -s and -g can be given together: -lsg. INCLUDE and INCBIN look for a
file beside the including file, then in each -I folder in turn; INCLUDE
then among the SDK's own include files: rombios.inc, ekdos.inc, ports.inc,
macros.inc.";

/// What the command line asks for.
struct Request {
    source: PathBuf,
    image: Option<PathBuf>,
    listing: bool,
    symbols: bool,
    debug: bool,
    options: juku_asm::Options,
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--version" || args[0] == "-V") {
        println!("juku-asm {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match parse(&args).and_then(|request| run(&request)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// The request `args` make, or what is wrong with them, with the usage.
fn parse(args: &[OsString]) -> Result<Request, String> {
    let usage = |e: String| format!("juku-asm: {e}\n\n{USAGE}");
    let mut request = Request {
        source: PathBuf::new(),
        image: None,
        listing: false,
        symbols: false,
        debug: false,
        options: juku_asm::Options::default(),
    };
    let mut sources = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        let text = arg.to_str().unwrap_or("");
        if text == "-o" || text == "-I" {
            let value = args.get(i).map(PathBuf::from).ok_or_else(|| usage(format!("{text} takes a name")))?;
            i += 1;
            if text == "-o" {
                request.image = Some(value);
            } else {
                request.options.include.push(value);
            }
        } else if let Some(flags) = text.strip_prefix('-').filter(|f| !f.is_empty()) {
            for flag in flags.chars() {
                match flag {
                    'l' => request.listing = true,
                    's' => request.symbols = true,
                    'g' => request.debug = true,
                    _ => return Err(usage(format!("unknown option {text}"))),
                }
            }
        } else {
            sources.push(PathBuf::from(arg));
        }
    }
    match sources.as_slice() {
        [source] => request.source = source.clone(),
        [] => return Err(usage("no source file".to_string())),
        [a, b, ..] => return Err(usage(format!("one source file, not {} and {}: -o names the image", a.display(), b.display()))),
    }
    Ok(request)
}

/// Assembles the source and writes the files `request` asks for.
fn run(request: &Request) -> Result<(), String> {
    let image = request.image.clone().unwrap_or_else(|| request.source.with_extension("bin"));
    let named = |asked: bool, ext: &str| asked.then(|| image.with_extension(ext));
    let (listing, symbols, debug) = (named(request.listing, "lst"), named(request.symbols, "sym"), named(request.debug, "dbg"));
    // No file is written over the source or over another one.
    let outputs: Vec<&PathBuf> = [Some(&image), listing.as_ref(), symbols.as_ref(), debug.as_ref()].into_iter().flatten().collect();
    for (n, path) in outputs.iter().enumerate() {
        if same_file(path, &request.source) || outputs[..n].contains(path) {
            return Err(format!("{}: would be written twice, or over the source: -o names the image", path.display()));
        }
    }

    let program = juku_asm::assemble_with(&request.source, &request.options).map_err(|e| e.to_string())?;
    write(&image, &program.image)?;
    if let Some(path) = &listing {
        write(path, program.listing().as_bytes())?;
    }
    if let Some(path) = &symbols {
        write(path, program.symbols().as_bytes())?;
    }
    if let Some(path) = &debug {
        write(path, program.debug_info(path, &image).as_bytes())?;
    }
    let (load, size) = (program.load, program.image.len() as i64);
    println!("{}: {size} bytes at {load:04X}h-{:04X}h", image.display(), load + size - 1);
    Ok(())
}

/// `a` and `b` name the same file: written the same, or, when both exist,
/// one file by another name (`./demo.asm`, or `DEMO.ASM` where case does
/// not matter).
fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        #[cfg(unix)]
        (Ok(x), Ok(y)) => {
            use std::os::unix::fs::MetadataExt;
            (x.dev(), x.ino()) == (y.dev(), y.ino())
        }
        #[cfg(not(unix))]
        (Ok(_), Ok(_)) => match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(x), Ok(y)) => x.to_string_lossy().to_lowercase() == y.to_string_lossy().to_lowercase(),
            _ => false,
        },
        _ => false,
    }
}

/// Writes `bytes` to `path`, or says what went wrong, as the system says it
/// without "(os error n)".
fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e: io::Error| {
        let text = e.to_string();
        let text = text.find(" (os error ").map_or(text.as_str(), |i| &text[..i]);
        format!("{}: {text}", path.display())
    })
}
