//! The assembler on the demos in `tests/demos/`, and on small programs
//! written for each test.
//!
//! Each demo's `.bin`, `.lst`, `.sym` and `.dbg` beside it are what it must
//! assemble to. After a change meant to change them,
//! `JUKU_ASM_BLESS=1 cargo test` writes them anew: look at the diff.

use std::path::{Path, PathBuf};

use juku_asm::{HEADERS, Options, Program, assemble, assemble_with};

const DEMOS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/demos");

/// Writes `files`, (name, text), into a folder of the test's own.
fn write(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    for (name, text) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn program(test: &str, source: &str) -> Program {
    let dir = write(test, &[("p.asm", source)]);
    assemble(&dir.join("p.asm")).unwrap()
}

/// The error assembling `source`, from the file name on.
fn error(test: &str, source: &str) -> String {
    let dir = write(test, &[("p.asm", source)]);
    let text = assemble(&dir.join("p.asm")).err().expect("assembled").to_string();
    text[text.find("p.asm").unwrap()..].to_string()
}

/// Demo `name`'s outputs: as they should be, or written anew when blessing.
fn check_demo(name: &str) -> Program {
    let demo = |ext: &str| Path::new(DEMOS).join(format!("{name}.{ext}"));
    let program = assemble(&demo("asm")).unwrap();
    let outputs = [
        ("bin", program.image.clone()),
        ("lst", program.listing().into_bytes()),
        ("sym", program.symbols().into_bytes()),
        ("dbg", program.debug_info(&demo("dbg"), &demo("bin")).into_bytes()),
    ];
    for (ext, bytes) in outputs {
        if std::env::var_os("JUKU_ASM_BLESS").is_some() {
            std::fs::write(demo(ext), bytes).unwrap();
        } else if std::fs::read(demo(ext)).unwrap() != bytes {
            panic!("{name}.{ext} is not what {name}.asm assembles to");
        }
    }
    program
}

#[test]
fn hello() {
    let program = check_demo("hello");
    assert_eq!((program.load, program.image.len()), (0x100, 61));

    // Paths in the debug information are from its own folder.
    let info = program.debug_info(&Path::new(DEMOS).join("sub/hello.dbg"), &Path::new(DEMOS).join("hello.bin"));
    assert!(info.contains(r#""image": "../hello.bin""#) && info.contains(r#""files": ["../hello.asm"]"#), "{info}");
}

#[test]
fn directives() {
    let program = check_demo("directives");
    assert_eq!((program.load, program.image.len()), (0x100, 0x303 - 0x100), "up to the end of the data, not of BSS");
}

/// Every documented opcode once, in order, with its operands.
#[test]
fn opcodes() {
    let program = check_demo("opcodes");
    let undocumented = [0x08, 0x10, 0x18, 0x20, 0x28, 0x30, 0x38, 0xCB, 0xD9, 0xDD, 0xED, 0xFD];
    let want: Vec<u8> = (0..=255).filter(|op| !undocumented.contains(op)).collect();
    let mut got = Vec::new();
    let mut i = 0;
    while i < program.image.len() {
        let op = program.image[i];
        let len = length(op);
        let operand = &program.image[i + 1..i + len];
        assert!(operand.is_empty() || operand == [0x34] || operand == [0x34, 0x12], "{op:02X}h: {operand:02X?}");
        got.push(op);
        i += len;
    }
    assert_eq!(got, want);
}

/// The length of the instruction with opcode `op`.
fn length(op: u8) -> usize {
    match op {
        0x01 | 0x11 | 0x21 | 0x31 | 0x22 | 0x2A | 0x32 | 0x3A | 0xC3 | 0xCD => 3,
        _ if op & 0xC7 == 0xC2 || op & 0xC7 == 0xC4 => 3, // Jcc, Ccc
        _ if op & 0xC7 == 0x06 || op & 0xC7 == 0xC6 => 2, // MVI, ADI to CPI
        0xD3 | 0xDB => 2,                                 // OUT, IN
        _ => 1,
    }
}

#[test]
fn expressions() {
    let cases: &[(&str, u16)] = &[
        ("-7/2", 0xFFFC),
        ("-7%2", 1),
        ("7/-2", 0xFFFC),
        ("7%-2", 0xFFFF),
        ("7/0", 0),
        ("7%0", 0),
        ("1<<15", 0x8000),
        ("-256>>4", 0xFFF0),
        ("~0", 0xFFFF),
        ("!5", 0),
        ("!0", 1),
        ("1|2^3&4", 3),
        ("2+3*4", 14),
        ("(2+3)*4", 20),
        ("1==1", 1),
        ("2<=1", 0),
        ("3 > 2", 1),
        ("high(1234h)", 0x12),
        ("HIGH(1234h)", 0x12),
        ("low(1234h)", 0x34),
        ("LOW(1234h)", 0x34),
        ("0ffh", 0xFF),
        ("1010b", 10),
        ("12d", 12),
        ("'a'", 97),
        ("''''", 39),
        ("$", 0x134),
        ("later-$", 4),
        ("main.local", 0x100),
    ];
    let mut source = String::from("        org 100h\nmain:\n.local:\n");
    for (expr, _) in cases {
        source += &format!("        dw {expr}\n");
    }
    source += "later:\n";
    let program = program("expressions", &source);
    for (i, &(expr, want)) in cases.iter().enumerate() {
        let got = u16::from_le_bytes([program.image[2 * i], program.image[2 * i + 1]]);
        assert_eq!(got, want, "{expr}");
    }
}

/// The SDK's include files: all three at once, with no clash.
#[test]
fn headers() {
    let source = "        include 'rombios.inc'
        include 'ekdos.inc'
        include 'ports.inc'
        org 100h
        call TTO
        out T2_1
        jmp WBOOT
";
    let program = program("headers", source);
    assert_eq!(program.image, [0xCD, 0xD9, 0xFF, 0xD3, 0x15, 0xC3, 0x00, 0x00]);
    assert!(program.listing().contains("TTO       equ 0ffd9h"), "the header's lines are listed");
    let names: Vec<&str> = HEADERS.iter().map(|h| h.0).collect();
    assert_eq!(names, ["ekdos.inc", "ports.inc", "rombios.inc"]);
}

/// INCLUDE and INCBIN look beside the including file, then in the include
/// folders in order, and INCLUDE then in the SDK's include files.
#[test]
fn include_folders() {
    let dir = write(
        "include_folders",
        &[
            ("src/p.asm", "        include 'local.inc'\n        include 'one.inc'\n        include 'two.inc'\n        include 'rombios.inc'\n        db LOCAL, ONE, TWO, low(TTO)\n        incbin 'data.bin'\n"),
            ("src/local.inc", "LOCAL   equ 1\n"),
            ("inc1/one.inc", "ONE     equ 2\n"),
            ("inc1/local.inc", "LOCAL   equ 99\n"),
            ("inc2/one.inc", "ONE     equ 99\n"),
            ("inc2/two.inc", "TWO     equ 3\n"),
            ("inc2/rombios.inc", "TTO     equ 4\n"),
            ("inc2/data.bin", "\u{5}"),
        ],
    );
    let options = Options { include: vec![dir.join("inc1"), dir.join("inc2")] };
    let program = assemble_with(&dir.join("src/p.asm"), &options).unwrap();
    assert_eq!(program.image, [1, 2, 3, 4, 5]);

    // Without the folders, one.inc is nowhere. The system words the error
    // and writes the path its own way: Windows with backslashes.
    let e = assemble(&dir.join("src/p.asm")).err().unwrap().to_string().replace('\\', "/");
    assert!(e.contains("p.asm:2: ") && e.contains("src/one.inc"), "{e}");
}

/// Names are case-sensitive; mnemonics, registers and directives are not.
#[test]
fn case() {
    let program = program("case", "COUNT   equ 3\n        ORG 100h\ncount:  MVI A, COUNT\n        Jmp count\n");
    assert_eq!(program.image, [0x3E, 3, 0xC3, 0x00, 0x01]);
    assert_eq!(program.symbols(), "COUNT 0003\ncount 0100\n");
}

#[test]
fn errors() {
    assert_eq!(error("undefined", "        org 100h\n        jmp nowhere\n"), "p.asm:2: undefined symbol nowhere\n          jmp nowhere");
    assert_eq!(error("byte", "        mvi a, 256\n"), "p.asm:1: 256 does not fit in a byte\n          mvi a, 256");
    assert_eq!(error("twice", "x:      nop\nx:      nop\n"), "p.asm:2: x defined twice\n  x:      nop");
    // Names are case-sensitive: a local label is under its scope as written.
    assert_eq!(
        error("case", "        org 100h\nmain:\n.local: jmp MAIN.local\n"),
        "p.asm:3: undefined symbol MAIN.local\n  .local: jmp MAIN.local"
    );
    assert_eq!(error("operands", "        mov a\n"), "p.asm:1: MOV takes 2 operand(s)\n          mov a");
    assert_eq!(error("mnemonic", "        ld a, b\n"), "p.asm:1: unknown mnemonic LD\n          ld a, b");
    assert_eq!(error("register", "        mov a, x\n"), "p.asm:1: not a register: x\n          mov a, x");
    assert_eq!(
        error("assert", "        nop\n        assert $ < 1, 'too big'\n"),
        "p.asm:2: assertion failed: too big\n          assert $ < 1, 'too big'"
    );
    assert_eq!(error("bss", "        bss\n        db 1\n"), "p.asm:2: only DS after BSS\n          db 1");
    assert_eq!(error("if", "        if 1\n        nop\n"), "p.asm: IF without ENDIF");
    assert_eq!(error("rept", "        rept 2\n        nop\n"), "p.asm:1: REPT without ENDM\n          rept 2");
    assert_eq!(error("empty", "; nothing\n"), "p.asm: no code");

    let e = error("missing", "        include 'no.asm'\n").replace('\\', "/");
    assert!(e.starts_with("p.asm:1: ") && e.ends_with("missing/no.asm\n          include 'no.asm'"), "{e}");

    // An error in an included file is on its line there.
    let dir = write("included", &[("p.asm", "        nop\n        include 'sub/q.asm'\n"), ("sub/q.asm", "        nop\n        foo\n")]);
    let e = assemble(&dir.join("p.asm")).err().unwrap().to_string().replace('\\', "/");
    assert!(e.ends_with("included/sub/q.asm:2: unknown mnemonic FOO\n          foo"), "{e}");
}
