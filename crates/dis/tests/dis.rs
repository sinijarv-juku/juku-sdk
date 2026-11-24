//! The disassembler: every opcode, names from the include files, and source
//! that assembles back to the same bytes.

use std::path::Path;

use juku_dis::{Names, disassemble, hex, include_line, listing, qualified, source};

/// juku-asm's demos, assembled.
const DEMOS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../asm/tests/demos");

/// `image`'s disassembly as source, assembled again.
fn round_trip(test: &str, image: &[u8], names: &Names, head: &str) -> Vec<u8> {
    let lines = disassemble(image, 0x100, 0x100, 0x100 + image.len() as u32, names, true);
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{test}.asm"));
    std::fs::write(&path, source(&lines, head)).unwrap();
    juku_asm::assemble(&path).unwrap().image
}

#[test]
fn every_opcode() {
    let image: Vec<u8> = (0..=255u8).flat_map(|op| [op, 0x34, 0x12]).collect();
    let mut texts = Vec::new();
    for op in 0..=255u32 {
        let at = 0x100 + 3 * op;
        let line = &disassemble(&image, 0x100, at, at + 1, &Names::default(), false)[0];
        texts.push((line.text.clone(), line.comment.clone()));
    }
    let text = |op: usize| texts[op].0.as_str();
    assert_eq!(text(0x00), "nop");
    assert_eq!(text(0x21), "lxi h, 1234h");
    assert_eq!(text(0x3E), "mvi a, 34h");
    assert_eq!(text(0x7E), "mov a, m");
    assert_eq!(text(0xC2), "jnz 1234h");
    assert_eq!(text(0xDB), "in 34h");
    assert_eq!(text(0xF5), "push psw");
    assert_eq!(text(0xFF), "rst 7");
    assert_eq!(texts[0xCB], ("db 0cbh, 34h, 12h".to_string(), Some("undocumented: jmp 1234h".to_string())));
    assert_eq!(texts[0x08], ("db 08h".to_string(), Some("undocumented: nop".to_string())));
    assert_eq!(texts.iter().filter(|t| t.1.is_some()).count(), 12);
}

#[test]
fn hex_as_the_assembler_reads_it() {
    assert_eq!((hex(5, 2), hex(0xAB, 2), hex(0x103, 4), hex(0xFFD9, 4)), ("05h".into(), "0abh".into(), "0103h".into(), "0ffd9h".into()));
}

#[test]
fn names() {
    let image = [0xCD, 0xD9, 0xFF, 0xD3, 0x15, 0x3A, 0xBB, 0xD4, 0xC3, 0x00, 0x01];
    let mut names = Names::default();
    for (name, value) in juku_asm::names("rombios.inc", &juku_asm::Options::default()).unwrap() {
        names.word(value as u16, &qualified("rombios.inc", &name));
    }
    for (name, value) in juku_asm::names("ports.inc", &juku_asm::Options::default()).unwrap() {
        names.port(value as u8, &qualified("ports.inc", &name));
    }
    let lines = disassemble(&image, 0x100, 0x100, 0x10B, &names, true);
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["call rb.tto", "out io.t2_1", "lda rb.conch", "jmp l0100"]);
    assert_eq!(lines[0].label.as_deref(), Some("l0100"));
    let head = include_line("rombios.inc") + &include_line("ports.inc");
    assert_eq!(head, "        include 'rombios.inc' as rb\n        include 'ports.inc' as io\n");
    assert_eq!(qualified("ekdos.inc", "bdos"), "dos.bdos");
    assert_eq!(include_line("ekdos.inc"), "        include 'ekdos.inc' as dos\n");
    assert_eq!(round_trip("names", &image, &names, &head), image);
    assert_eq!(
        listing(&lines),
        "0100  CD D9 FF  call rb.tto\n0103  D3 15     out io.t2_1\n0105  3A BB D4  lda rb.conch\n0108  C3 00 01  jmp l0100\n"
    );
}

#[test]
fn cut_off() {
    let lines = disassemble(&[0x00, 0xC3, 0x00], 0x100, 0x100, 0x103, &Names::default(), true);
    assert_eq!(lines[1].text, "db 0c3h, 00h");
    assert_eq!(lines[1].comment.as_deref(), Some("cut off by the end of the image"));
    assert_eq!(round_trip("cut_off", &[0x00, 0xC3, 0x00], &Names::default(), ""), [0x00, 0xC3, 0x00]);
}

/// juku-asm's demos, code and data, back to their bytes: undocumented
/// opcodes and labels in the middle of instructions among them.
#[test]
fn demos_round_trip() {
    for name in ["hello", "opcodes", "directives"] {
        let image = std::fs::read(format!("{DEMOS}/{name}.bin")).unwrap();
        assert!(round_trip(name, &image, &Names::default(), "") == image, "{name}");
    }
    let every: Vec<u8> = (0..=255u8).flat_map(|op| [op, 0x34, 0x12]).collect();
    assert!(round_trip("every", &every, &Names::default(), "") == every);
}
