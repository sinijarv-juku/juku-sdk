//! juku-asm: assembles a Juku program into its image, and its listing,
//! symbols and debug information if asked.

use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
Usage: juku-asm <main.asm> <output.bin> [-l listing.lst] [-s symbols.sym]
                [-g debug.dbg] [-I folder]...
       juku-asm --version

Assembles Intel 8080 source into the image of a Juku program. -l writes a
listing with cycle counts, -s the symbols as name 1234 in hex, and -g
debug information for a debugger, in JSON. INCLUDE and INCBIN look for a
file beside the including file, then in each -I folder in turn; INCLUDE
then in the SDK's own include files: rombios.inc, ekdos.inc, ports.inc.";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--version" || args[0] == "-V") {
        println!("juku-asm {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let (mut listing, mut symbols, mut debug) = (None, None, None);
    let mut options = juku_asm::Options::default();
    let mut paths = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let value = args.get(i + 1).map(PathBuf::from);
        match (args[i].to_str(), value) {
            (Some("-l"), Some(path)) => listing = Some(path),
            (Some("-s"), Some(path)) => symbols = Some(path),
            (Some("-g"), Some(path)) => debug = Some(path),
            (Some("-I"), Some(dir)) => options.include.push(dir),
            _ => {
                paths.push(PathBuf::from(&args[i]));
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    let [source, image] = paths.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let program = match juku_asm::assemble_with(source, &options) {
        Ok(program) => program,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let mut written = write(image, &program.image);
    if let Some(path) = &listing {
        written = written.and_then(|()| write(path, program.listing().as_bytes()));
    }
    if let Some(path) = &symbols {
        written = written.and_then(|()| write(path, program.symbols().as_bytes()));
    }
    if let Some(path) = &debug {
        written = written.and_then(|()| write(path, program.debug_info(path, image).as_bytes()));
    }
    if let Err(e) = written {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }
    let (load, size) = (program.load, program.image.len() as i64);
    println!("{}: {size} bytes at {load:04X}h-{:04X}h", image.display(), load + size - 1);
    ExitCode::SUCCESS
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
