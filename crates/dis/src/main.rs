//! juku-dis: disassembles a Juku program or ROM image into a listing, or
//! into source juku-asm assembles back to the same bytes.

use std::path::Path;
use std::process::ExitCode;

use juku_dis::{Names, disassemble, include_line, listing, qualified, source};

const USAGE: &str = "\
Usage: juku-dis [-s] [-i FILE]... [-p] [-b ADDRESS] [-r START-END] IMAGE
       juku-dis --version

Disassembles IMAGE. The options and IMAGE can come in any order.
Addresses are hex: 100, 0100h or 0x100.

  -b, --base ADDRESS
             where IMAGE's first byte is: 100h, a COM file's, if not
             given; 0 for a ROM image
  -r, --range START-END
             disassemble from START up to END, not included; either can
             be left out: -r 100-10c, -r 120- (to the end), -r -10c (from
             the first byte), and -r 120 is -r 120-
  -s         write source that juku-asm assembles back to the same bytes,
             with a label for each operand that is the address of an
             instruction; otherwise a listing of addresses and bytes
  -i FILE    name addresses by the names an include file defines, but
             those in upper case only, constants: the SDK's rombios.inc
             and ekdos.inc are built in, their names under rb and dos
             (rb.tto, dos.bdos), as -s includes them
  -p, --ports
             name the ports of IN and OUT as the SDK's ports.inc does,
             under io (io.t2_1)";

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

/// `-r`'s START-END: either address, or both.
fn range(text: &str) -> Result<(Option<u32>, Option<u32>), String> {
    let (start, end) = text.split_once('-').unwrap_or((text, ""));
    let at = |a: &str| if a.is_empty() { Ok(None) } else { address(a).map(Some) };
    let (start, end) = (at(start)?, at(end)?);
    if let (Some(s), Some(e)) = (start, end)
        && e <= s
    {
        return Err(format!("{text}: the end is not after the start"));
    }
    Ok((start, end))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && (args[0] == "--version" || args[0] == "-V") {
        println!("juku-dis {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let usage = |e: String| format!("juku-dis: {e}\n\n{USAGE}");
    let (mut base, mut as_source, mut ports, mut span) = (0x100, false, false, (None, None));
    let (mut includes, mut images) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        i += 1;
        if matches!(arg, "-b" | "--base" | "-i" | "-r" | "--range") {
            let value = args.get(i).ok_or_else(|| usage(format!("{arg} takes a value")))?;
            i += 1;
            match arg {
                "-b" | "--base" => base = address(value)?,
                "-i" => includes.push(value.clone()),
                _ => span = range(value)?,
            }
        } else if arg == "-s" {
            as_source = true;
        } else if arg == "-p" || arg == "--ports" {
            ports = true;
        } else if arg.starts_with('-') && arg.len() > 1 {
            return Err(usage(format!("unknown option {arg}")));
        } else {
            images.push(arg);
        }
    }
    let image_path = match images.as_slice() {
        [image] => *image,
        [] => return Err(usage("no image".to_string())),
        [a, b, ..] => return Err(usage(format!("one image, not {a} and {b}: -r gives the range"))),
    };
    let image = std::fs::read(image_path).map_err(|e| {
        let text = e.to_string();
        format!("{image_path}: {}", text.find(" (os error ").map_or(text.as_str(), |i| &text[..i]))
    })?;
    if image.is_empty() {
        return Err(format!("{image_path}: empty"));
    }
    if base + image.len() as u32 > 0x10000 {
        return Err(format!("{image_path}: {} bytes do not fit from {} to the end of memory", image.len(), juku_dis::hex(base, 4)));
    }
    let start = span.0.unwrap_or(base);
    let end = span.1.unwrap_or(base + image.len() as u32);
    if start < base || start >= base + image.len() as u32 {
        return Err(format!("{}: outside the image, {}-{}", juku_dis::hex(start, 4), juku_dis::hex(base, 4), juku_dis::hex(base + image.len() as u32 - 1, 4)));
    }

    let mut names = Names::default();
    let options = juku_asm::Options::default();
    for file in &includes {
        // A name in upper case only is a constant, not an address.
        for (name, value) in juku_asm::names(file, &options).map_err(|e| e.to_string())? {
            if name.chars().any(|c| c.is_ascii_lowercase()) {
                names.word(value as u16, &qualified(file, &name));
            }
        }
    }
    if ports {
        for (name, value) in juku_asm::names("ports.inc", &options).map_err(|e| e.to_string())? {
            names.port(value as u8, &qualified("ports.inc", &name));
        }
    }

    let lines = disassemble(&image, base, start, end, &names, as_source);
    if as_source {
        let shown = Path::new(image_path).file_name().map_or(image_path.to_string(), |n| n.to_string_lossy().to_string());
        let mut head = format!("; {shown}, disassembled by juku-dis {}\n\n", env!("CARGO_PKG_VERSION"));
        for file in includes.iter().chain(ports.then(|| "ports.inc".to_string()).iter()) {
            head += &include_line(file);
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
