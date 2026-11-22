//! juku-dis: disassembles a Juku program or ROM image into a listing, or
//! into source juku-asm assembles back to the same bytes.

use std::path::Path;
use std::process::ExitCode;

use juku_dis::{Names, disassemble, listing, source};

const USAGE: &str = "\
Usage: juku-dis IMAGE [START [END]] [--base ADDRESS] [-s] [-i FILE]... [--ports]
       juku-dis --version

Disassembles IMAGE, whose first byte is at ADDRESS (100h, a COM file's, if
not given; 0 for a ROM image), from START (the first byte) up to END (not
included; the end of the image). Addresses are hex: 100, 0100h or 0x100.

  -s         write source that juku-asm assembles back to the same bytes,
             with a label for each operand that is the address of an
             instruction; otherwise a listing of addresses and bytes
  -i FILE    name addresses by the names an include file defines: the
             SDK's rombios.inc and ekdos.inc are built in
  --ports    name the ports of IN and OUT as the SDK's ports.inc does";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// An address: hex, with or without `h` or `0x`.
fn address(text: &str) -> Result<u32, String> {
    let digits = text.strip_prefix("0x").or_else(|| text.strip_suffix(['h', 'H'])).unwrap_or(text);
    u32::from_str_radix(digits, 16).ok().filter(|&a| a <= 0xFFFF).ok_or_else(|| format!("{text}: not an address"))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && (args[0] == "--version" || args[0] == "-V") {
        println!("juku-dis {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let (mut base, mut as_source, mut ports) = (0x100, false, false);
    let (mut includes, mut positional) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < args.len() {
        match (args[i].as_str(), args.get(i + 1)) {
            ("--base", Some(v)) => {
                base = address(v)?;
                i += 1;
            }
            ("-i", Some(v)) => {
                includes.push(v.clone());
                i += 1;
            }
            ("-s", _) => as_source = true,
            ("--ports", _) => ports = true,
            _ => positional.push(&args[i]),
        }
        i += 1;
    }
    let [image_path, rest @ ..] = positional.as_slice() else {
        return Err(USAGE.to_string());
    };
    if rest.len() > 2 {
        return Err(USAGE.to_string());
    }
    let image = std::fs::read(image_path).map_err(|e| {
        let text = e.to_string();
        format!("{image_path}: {}", text.find(" (os error ").map_or(text.as_str(), |i| &text[..i]))
    })?;
    let start = rest.first().map(|a| address(a)).transpose()?.unwrap_or(base);
    let end = rest.get(1).map(|a| address(a)).transpose()?.unwrap_or(base + image.len() as u32);
    if start < base || start >= base + image.len() as u32 {
        return Err(format!("{}: outside the image, {}-{}", juku_dis::hex(start, 4), juku_dis::hex(base, 4), juku_dis::hex(base + image.len() as u32 - 1, 4)));
    }

    let mut names = Names::default();
    let options = juku_asm::Options::default();
    for file in &includes {
        for (name, value) in juku_asm::names(file, &options).map_err(|e| e.to_string())? {
            names.word(value as u16, &name);
        }
    }
    if ports {
        for (name, value) in juku_asm::names("ports.inc", &options).map_err(|e| e.to_string())? {
            names.port(value as u8, &name);
        }
    }

    let lines = disassemble(&image, base, start, end, &names, as_source);
    if as_source {
        let shown = Path::new(image_path).file_name().map_or(image_path.to_string(), |n| n.to_string_lossy().to_string());
        let mut head = format!("; {shown}, disassembled by juku-dis {}\n\n", env!("CARGO_PKG_VERSION"));
        for file in includes.iter().chain(ports.then(|| "ports.inc".to_string()).iter()) {
            head += &format!("        include '{file}'\n");
        }
        if !includes.is_empty() || ports {
            head.push('\n');
        }
        print!("{}", source(&lines, &head));
    } else {
        print!("{}", listing(&lines));
    }
    Ok(())
}
