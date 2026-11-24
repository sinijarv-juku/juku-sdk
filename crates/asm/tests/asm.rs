//! The assembler on the demos in `tests/demos/`, and on small programs
//! written for each test.
//!
//! Each demo's `.bin`, `.lst`, `.sym` and `.dbg` beside it are what it must
//! assemble to. After a change meant to change them,
//! `JUKU_ASM_BLESS=1 cargo test` writes them anew: look at the diff.

use std::path::{Path, PathBuf};

use juku_asm::{HEADERS, Index, NameKind, Options, Program, assemble, assemble_with};

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
    assert_eq!((program.load, program.image.len()), (0x100, 0x30D - 0x100), "up to the end of DATA, not of BSS");
    assert_eq!(program.image[5..7], [0x0D, 0x02], "dw end(data) - 100h");
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

/// `db` takes an expression of characters as a number; tabs separate as
/// spaces do.
#[test]
fn quotes_and_tabs() {
    let p = program("db_expr", "        db 'a'+'b', 'a' | 'b', 'ab', ''''\n");
    assert_eq!(p.image, [0xC3, 0x63, b'a', b'b', b'\'']);
    let p = program("tabs", "\torg\t100h\nfoo\tmvi\ta, 1\n\tjmp\tfoo\n");
    assert_eq!(p.image, [0x3E, 1, 0xC3, 0x00, 0x01]);
}

/// The SDK's include files: all three at once, with no clash.
#[test]
fn headers() {
    let source = "        include 'rombios.inc'
        include 'ekdos.inc'
        include 'ports.inc'
        org 100h
        call tto
        out t2_1
        jmp wboot
";
    let program = program("headers", source);
    assert_eq!(program.image, [0xCD, 0xD9, 0xFF, 0xD3, 0x15, 0xC3, 0x00, 0x00]);
    assert!(program.listing().contains("tto       equ 0ffd9h"), "the header's lines are listed");
    let names: Vec<&str> = HEADERS.iter().map(|h| h.0).collect();
    assert_eq!(names, ["ekdos.inc", "macros.inc", "ports.inc", "rombios.inc"]);
}

/// INCLUDE with AS puts the file's names under a prefix, and with a list
/// defines those as they are: the program's own names can be the same.
#[test]
fn namespaces() {
    let source = "tto     equ 1
fill    equ 2
        include 'rombios.inc' as bios
        include 'rombios.inc' (ttcon, crlf)
        db tto, fill, low(bios.tto), low(crlf), low(rombios.fill), low(ttcon)
";
    let program = program("namespaces", source);
    assert_eq!(program.image, [1, 2, 0xD9, 0xCA, 0xFA, 0xCD]);
    let symbols = program.symbols();
    for line in ["tto 0001\n", "bios.tto FFD9\n", "crlf FFCA\n", "ttcon FFCD\n", "rombios.tto FFD9\n"] {
        assert!(symbols.contains(line), "{line}{symbols}");
    }
    assert!(!symbols.contains("rombios.crlf"), "{symbols}");

    // A file of code: a name in it is its own if it defines it, else the
    // includer's. Its local labels go with their label, it leaves the
    // includer's scope alone, and a file it includes is under its prefix.
    let dir = write(
        "namespaces_code",
        &[
            (
                "p.asm",
                "COUNT   equ 7
        org 100h
main:   call gfx.draw
        include 'lib/gfx.asm'  as gfx
.next:  jmp gfx.draw.loop
        db COUNT, gfx.COUNT, gfx.SIZE
        dw main.next, helper
        include 'lib/gfx.asm' AS g2 (draw)
        dw draw.loop, g2.helper
helper: ret
",
            ),
            (
                "lib/gfx.asm",
                "COUNT   equ 2
draw:   mvi b, COUNT + SIZE
.loop:  dcr b
        jnz .loop
        call helper
        ret
helper: lxi h, main
        ret
        include 'size.inc'
",
            ),
            ("lib/size.inc", "SIZE    equ 3\n"),
        ],
    );
    let program = assemble(&dir.join("p.asm")).unwrap();
    let symbols = program.symbols();
    let value = |name: &str| {
        let line = symbols.lines().find(|l| l.split(' ').next() == Some(name)).unwrap_or_else(|| panic!("{name}\n{symbols}"));
        u16::from_str_radix(&line[name.len() + 1..], 16).unwrap()
    };
    let names = ["gfx.draw", "gfx.draw.loop", "gfx.helper", "gfx.SIZE", "main.next", "draw", "draw.loop", "g2.helper", "helper"];
    let values: Vec<u16> = names.iter().map(|n| value(n)).collect();
    assert_eq!(values, [0x103, 0x105, 0x10D, 3, 0x111, 0x11B, 0x11D, 0x125, 0x12D]);
    let image = &program.image;
    assert_eq!(image[..3], [0xCD, 0x03, 0x01], "call gfx.draw");
    assert_eq!(image[3..5], [0x06, 5], "mvi b, COUNT + SIZE: the file's own");
    assert_eq!(image[9..12], [0xCD, 0x0D, 0x01], "call helper: the file's own");
    assert_eq!(image[0x0D..0x10], [0x21, 0x00, 0x01], "lxi h, main: the includer's");
    assert_eq!(image[0x11..0x1B], [0xC3, 0x05, 0x01, 7, 2, 3, 0x11, 0x01, 0x2D, 0x01], "jmp, db, dw");
    assert_eq!(image[0x29..0x2D], [0x1D, 0x01, 0x25, 0x01], "dw draw.loop, g2.helper");

    let e = error("listed_missing", "        include 'rombios.inc' (tto, nope)\n");
    assert_eq!(e, "p.asm:1: rombios.inc does not define nope\n          include 'rombios.inc' (tto, nope)");
    // A listed name the program has is defined twice, in the include file.
    let dir = write("listed_twice", &[("p.asm", "fill    equ 1\n        include 'rombios.inc' (fill)\n")]);
    let e = assemble(&dir.join("p.asm")).err().unwrap().to_string();
    assert!(e.starts_with("<juku-sdk>/rombios.inc:") && e.contains(": fill defined twice\n"), "{e}");
    let e = error("as_name", "        include 'rombios.inc' as .x\n");
    assert_eq!(e, "p.asm:1: AS takes a name\n          include 'rombios.inc' as .x");
    let e = error("stem", "        include 'my-lib.inc' (x)\n");
    assert_eq!(e, "p.asm:1: my-lib is not a name: give one with AS\n          include 'my-lib.inc' (x)");
    let e = error("list", "        include 'rombios.inc' (tto,)\n");
    assert_eq!(e, "p.asm:1: not a name to include: \n          include 'rombios.inc' (tto,)");
    let e = error("after", "        include 'rombios.inc' bios\n");
    assert_eq!(e, "p.asm:1: unexpected text after INCLUDE's file\n          include 'rombios.inc' bios");
}

/// Macros: arguments, a list of the rest and FOR over it, local labels of
/// each expansion's own, and MOV r, r left out of an expansion.
#[test]
fn macros() {
    let source = "load    macro dst, src
        mov dst, src
        endm
pushr   macro regs...
        for r, regs
        push r
        endm
        endm
popr    macro regs...
        for r, reverse(regs)
        pop r
        endm
        endm
wait    macro n
.loop:  lda n
        ora a
        jnz .loop
        endm
skip    macro
:       nop
        jmp :-
        endm
        org 100h
main:   pushr b, d, h
        load a, a
        load a, b
        mov a, a
        wait flag
        wait .x
        jmp :+
        skip
:       popr b, d, h
.x:     db 0
flag:   db 0
";
    let p = program("macros", source);
    #[rustfmt::skip]
    let want = [
        0xC5, 0xD5, 0xE5, 0x78, 0x7F,
        0x3A, 0x1E, 0x01, 0xB7, 0xC2, 0x05, 0x01,
        0x3A, 0x1D, 0x01, 0xB7, 0xC2, 0x0C, 0x01,
        0xC3, 0x1A, 0x01, 0x00, 0xC3, 0x16, 0x01,
        0xE1, 0xD1, 0xC1, 0, 0,
    ];
    assert_eq!(p.image, want);
    // An expansion's own labels are not symbols.
    assert_eq!(p.symbols(), "flag 011E\nmain 0100\nmain.x 011D\n");
    let listing = p.listing();
    assert!(listing.contains("0103  (left out)                  mov a, a"), "{listing}");
    assert!(listing.contains("0100                  33  main:   pushr b, d, h"), "the call's cycles: {listing}");
    // The expansion's code is on the call's line.
    let info = p.debug_info(Path::new("p.dbg"), Path::new("p.bin"));
    assert!(info.contains("[256,0,24,1,0,1,0,1,2,1,1,1,1,3,0,1,0,3,"), "{info}");

    // The functions of an argument's text, defaults, and CSTR(): each
    // string once, at the end of CONST, and one that ends another in it.
    let source = "put     macro r, v=0
        if const(v) & same(v, 0) & same(r, a)
        xra a
        else
        mvi r, v
        endif
        endm
        org 100h
        put a
        put b
        put A, 5
        lxi h, cstr('Hello')
        lxi h, cstr('lo')
        lxi d, cstr('ab', 13, 10)
        dw count(1, 2, 3), blank(), blank(x), reg(m), pair(psw), same(A, a), same(Count, count)
        lxi h, cstr('Hello')
";
    let p = program("macro_functions", source);
    #[rustfmt::skip]
    let want = [
        0xAF, 0x06, 0x00, 0x3E, 0x05,
        0x21, 0x1F, 0x01, 0x21, 0x22, 0x01, 0x11, 0x25, 0x01,
        3, 0, 1, 0, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0,
        0x21, 0x1F, 0x01,
        b'H', b'e', b'l', b'l', b'o', 0, b'a', b'b', 13, 10, 0,
    ];
    assert_eq!(p.image, want);
    assert!(p.listing().contains("011F  48 65 6C 6C …               db 'Hello', 0   ; cstr"), "{}", p.listing());
    // CONST() of a name defined further on is 0, in every pass.
    let p = program("macro_const", "        db const(later), const(early)\nearly   equ 1\nlater   equ 2\n        db const(early), const(a)\n");
    assert_eq!(p.image, [0, 0, 1, 0]);

    // A macro of a namespaced include, which calls another; FOR outside a
    // macro; a macro and a FOR in an IF that is not taken.
    let dir = write(
        "macro_files",
        &[
            ("p.asm", "        include 'tx.inc' as tx\n        org 100h\n        tx.say 'J'\n        for v, 1, 2\n        db v\n        endm\n        if 0\n        for v, [3]\n        db v\n        endm\nbad     macro\n        endm\n        endif\n"),
            ("tx.inc", "say     macro ch\n        mvi a, ch\n        out1\n        endm\nout1    macro\n        call 0ffd9h\n        endm\n"),
        ],
    );
    let p = assemble(&dir.join("p.asm")).unwrap();
    assert_eq!(p.image, [0x3E, b'J', 0xCD, 0xD9, 0xFF, 1, 2]);

    let errors = [
        ("bad     macro x\n        foo\n        endm\n        bad 1\n", "p.asm:4: in bad: p.asm:2: unknown mnemonic FOO\n          foo"),
        ("        late 1\nlate    macro x\n        endm\n", "p.asm:1: late is used before its definition\n          late 1"),
        ("two     macro x\n        endm\n        two 1, 2\n", "p.asm:3: two takes 1 argument(s)\n          two 1, 2"),
        ("mov     macro\n        endm\n", "p.asm:1: mov is a mnemonic or a directive\n  mov     macro"),
        ("m       macro\n        nop\n", "p.asm:1: MACRO without ENDM\n  m       macro"),
        ("m       macro a\n        endm\n", "p.asm:1: not a parameter name: a\n  m       macro a"),
        ("m       macro\n        endm\nm       macro\n        endm\n", "p.asm:3: m defined twice\n  m       macro"),
        ("        for x\n        endm\n", "p.asm:1: FOR takes a name and a list\n          for x"),
        ("        for x, 1\n        nop\n", "p.asm:1: FOR without ENDM\n          for x, 1"),
        ("m       macro x\n        jmp x\n        endm\n:       m :-\n", "p.asm:4: an anonymous label cannot be a macro's argument\n  :       m :-"),
        ("r       macro\n        r\n        endm\n        r\n", "p.asm:4: in r: p.asm:2: macros nested more than 64 deep\n          r"),
        ("g       macro\nhere:   nop\n        endm\n        g\n        g\n", "p.asm:5: in g: p.asm:2: here defined twice\n  here:   nop"),
        ("        opt maybe\n", "p.asm:1: OPT takes ON or OFF\n          opt maybe"),
    ];
    for (i, (source, want)) in errors.iter().enumerate() {
        // An expansion's line is shown with its file's path: the name only.
        let e = error(&format!("macro_error_{i}"), source);
        let e: Vec<&str> = e.split(' ').map(|w| w.rsplit_once(['/', '\\']).map_or(w, |(_, name)| name)).collect();
        assert_eq!(e.join(" "), *want);
    }
}

/// {text} joins an argument to the names around it; upper() and lower()
/// change its case.
#[test]
fn join_and_case() {
    let source = "sfx     macro name
        lxi h, sfx_{name}
        endm
dirty   macro part
        mvi a, S_{upper(part)}
        endm
say     macro text
        db {upper(text)}, 0
        endm
        org 100h
        sfx jump
        dirty oil
        say 'hi'
        for k, [a, b]
        dw tab_{k}
        endm
        dw tab_{lower(B)}
S_OIL   equ 4
sfx_jump:
tab_a:  nop
tab_b:  nop
";
    let p = program("join", source);
    assert_eq!(p.image, [0x21, 0x0E, 0x01, 0x3E, 4, b'H', b'I', 0, 0x0E, 0x01, 0x0F, 0x01, 0x0F, 0x01, 0, 0]);
    assert_eq!(error("join_open", "        db {x\n"), "p.asm:1: missing }\n          db {x");
}

/// IF, ELSEIF and ELSE: the first branch whose condition holds, in a macro
/// and out of one, and none in an IF that is not taken.
#[test]
fn elseif() {
    let source = "pick    macro r
        if same(r, a)
        db 1
        elseif same(r, b)
        db 2
        elseif same(r, c)
        db 3
        else
        db 4
        endif
        endm
        pick a
        pick b
        pick c
        pick d
        if 0
        if 1
        db 9
        elseif 1
        db 9
        endif
        elseif 1
        db 5
        elseif 1
        db 9
        endif
k       set [x, y]
        if 0
        db 9
        elseif count(k) == 2
        db 6
        endif
";
    assert_eq!(program("elseif", source).image, [1, 2, 3, 4, 5, 6]);
}

/// SET, a value that changes: a number, a string or a list; and the SDK's
/// macros.inc: pushr and popr, and save, which restore pops at each exit.
#[test]
fn set_and_save() {
    let p = program("set", "x       set 1\n        db x\nx       set x + 1\n        db x\n");
    assert_eq!((&p.image[..], p.symbols()), (&[1, 2][..], String::new()), "a SET is not a symbol");

    // A string or a list is its text where its name is; a name can change
    // its type, and DEFINED() asks about the name itself.
    let source = "msg     set 'It''s'
        db msg, 0
        lxi h, cstr(msg)
regs    set [b, d, h]
        for r, reverse(regs)
        push r
        endm
        db count(regs), blank([]), blank(regs), defined(regs), defined(nothing), defined(msg)
regs    set 7
        db regs
";
    let p = program("set_types", source);
    #[rustfmt::skip]
    let want = [b'I', b't', b'\'', b's', 0, 0x21, 0x12, 0x00, 0xE5, 0xD5, 0xC5, 3, 1, 0, 1, 0, 1, 7, b'I', b't', b'\'', b's', 0];
    assert_eq!(p.image, want);
    let e = error("set_list", "regs    set [b]\n        db regs + 1\n");
    assert_eq!(e, "p.asm:2: a list is not a number\n          db regs + 1");
    assert_eq!(error("set_equ", "y       equ 3\ny       set 4\n"), "p.asm:2: y defined twice\n  y       set 4");
    assert_eq!(error("set_late", "        db z\nz       set 1\n"), "p.asm:1: z is used before it is set\n          db z");

    let source = "        include 'macros.inc'
        org 100h
work:   save b, d, h, psw
        ora a
        jz .out
        restore
        ret
.out:   restore
        ret
other:  save h
        pushr b, d
        popr b, d
        restore
        ret
";
    let p = program("save", source);
    #[rustfmt::skip]
    let want = [
        0xC5, 0xD5, 0xE5, 0xF5, 0xB7, 0xCA, 0x0D, 0x01,
        0xF1, 0xE1, 0xD1, 0xC1, 0xC9,
        0xF1, 0xE1, 0xD1, 0xC1, 0xC9,
        0xE5, 0xC5, 0xD5, 0xD1, 0xC1, 0xE1, 0xC9,
    ];
    assert_eq!(p.image, want);
    assert_eq!(p.symbols(), "other 0112\nwork 0100\nwork.out 010D\n");
    // An expansion lists only what it places: not its IF, FOR and SET lines.
    assert_eq!(p.listing().matches("saved_pairs set pairs").count(), 1, "only in the definition: {}", p.listing());
    let e = error("restore_alone", "        include 'macros.inc'\n        restore\n");
    assert!(e.starts_with("p.asm:2: in restore: <juku-sdk>/macros.inc:") && e.contains("restore without a save in its routine"), "{e}");
    // A restore is its routine's: not another one's save.
    let e = error("restore_other", "        include 'macros.inc'\none:    save b\n        restore\n        ret\ntwo:    restore\n        ret\n");
    assert!(e.starts_with("p.asm:5: in restore: ") && e.contains("restore without a save in its routine"), "{e}");

    // ..name in a macro is the calling routine's local, in each routine.
    let source = "exit    macro
        jmp ..done
        endm
here    macro
..done:
        endm
        org 100h
one:    exit
        nop
        here
        ret
two:    exit
        here
        ret
";
    let p = program("caller_local", source);
    assert_eq!(p.image, [0xC3, 0x04, 0x01, 0x00, 0xC9, 0xC3, 0x08, 0x01, 0xC9]);
    assert_eq!(p.symbols(), "one 0100\none.done 0104\ntwo 0105\ntwo.done 0108\n");
    assert_eq!(error("caller_outside", "        jmp ..done\n"), "p.asm:1: ..done is a macro's caller's local: only in a macro\n          jmp ..done");
}

/// A macro's argument means what it does where the macro is used, also
/// when the macro's file has a name of the same spelling; an include list
/// names macros and structs; no include changes the includer's scope.
#[test]
fn includes_and_arguments() {
    let dir = write(
        "includes_arguments",
        &[
            (
                "p.asm",
                "        include 'lib.inc' as lib
        include 'lib.inc' as l2 (m, pt)
foo     equ 2
        org 100h
main:   lib.m foo
        m foo
        lib.m lib.foo
        db size(pt)
        include 'code.inc'
.next:  jmp main.next
",
            ),
            ("lib.inc", "foo     equ 1\nm       macro x\n        mvi a, x\n        endm\npt      struct\na       db 0\n        ends\n"),
            ("code.inc", "other:  nop\n"),
        ],
    );
    let p = assemble(&dir.join("p.asm")).unwrap();
    assert_eq!(p.image, [0x3E, 2, 0x3E, 2, 0x3E, 1, 1, 0, 0xC3, 0x08, 0x01]);
    assert_eq!(value(&p, "main.next"), 0x108);
}

/// STRUCT: its fields' offsets are symbols, and its name lays it out with
/// values, in order or by name, under a label that names the fields.
#[test]
fn structs() {
    let source = "        org 100h
        lxi h, hero.pos.y
        lxi d, sprite.pos.y
        mvi a, size(sprite)
        db size(point)
        lda spare.flags
point   struct
x       db 0
y       db 0
        ends
sprite  struct
pos     point 1, 2              ; a struct in a struct, with values
name    ds 4, ' '
flags   dw 1234h
        ds 1                    ; no name: a value cannot be given
        ends
        data
hero    sprite [5], 'AB', flags=7
        sprite name='X', pos=[, 3]
        bss
spare   sprite
";
    let p = program("structs", source);
    let hero = [5, 2, b'A', b'B', b' ', b' ', 7, 0, 0];
    let other = [1, 3, b'X', b' ', b' ', b' ', 0x34, 0x12, 0];
    let mut image = vec![0x21, 0x0D, 0x01, 0x11, 0x01, 0x00, 0x3E, 9, 2, 0x3A, 0x24, 0x01];
    image.extend(hero.iter().chain(&other));
    assert_eq!(p.image, image);
    for (name, v) in [("point.y", 1), ("sprite.pos.y", 1), ("sprite.flags", 6), ("hero", 0x10C), ("hero.name", 0x10E), ("spare.pos.x", 0x11E)] {
        assert_eq!(value(&p, name), v, "{name}");
    }
    assert!(!p.symbols().lines().any(|l| l.starts_with("point ") || l.starts_with("sprite ")), "a struct's name is not a symbol");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("structs");
    let dbg = p.debug_info(&dir.join("p.dbg"), &dir.join("p.bin"));
    for row in ["[\"hero\",268,9]", "[\"hero.pos.y\",269,1]", "[\"hero.name\",270,4]", "[\"spare\",286,9]", "[\"sprite.name\",2]"] {
        assert!(dbg.contains(row), "{row}\n{dbg}");
    }

    // Used above its definition, and in a namespace.
    let dir = write(
        "structs_namespace",
        &[
            ("p.asm", "        org 100h\nmine    t.pair 1, 2\n        db size(t.pair), t.pair.b\n        include 'types.inc' as t\n"),
            ("types.inc", "pair    struct\na       db 0\nb       dw 0\n        ends\n"),
        ],
    );
    let p = assemble(&dir.join("p.asm")).unwrap();
    assert_eq!(p.image, [1, 2, 0, 3, 1]);
    assert_eq!(value(&p, "mine.b"), 0x101);

    let s = "pt      struct\nx       db 0\nname    ds 2\n        ends\n";
    let e = |test: &str, line: &str| error(test, &format!("{s}{line}\n"));
    assert_eq!(e("struct_field", "        pt z=1"), "p.asm:5: pt has no field z\n          pt z=1");
    assert_eq!(e("struct_many", "        pt 1, 2, 3"), "p.asm:5: pt has 2 fields\n          pt 1, 2, 3");
    assert_eq!(e("struct_long", "        pt , 'abc'"), "p.asm:5: 'abc' is longer than name, 2 bytes\n          pt , 'abc'");
    assert_eq!(e("struct_twice", "        pt x=1, x=2"), "p.asm:5: x given twice\n          pt x=1, x=2");
    assert_eq!(e("struct_order", "        pt x=1, 2"), "p.asm:5: a value without its field's name after one with it\n          pt x=1, 2");
    assert_eq!(e("struct_bss", "        bss\n        pt 1"), "p.asm:6: a struct in BSS takes no values: it is reserved, not set\n          pt 1");
    assert_eq!(e("struct_number", "        db pt"), "p.asm:5: pt is a struct, not a number: size(pt) is its size\n          db pt");
    assert_eq!(e("struct_size", "        db size(qq)"), "p.asm:5: size() takes a section (code, const, data, bss) or a struct: qq\n          db size(qq)");
    let e = error("struct_kind", "pt      struct\nx       dd 0\n        ends\n");
    assert_eq!(e, "p.asm:2: a field is DB, DW, DS or a struct: dd\n  x       dd 0");
    let e = error("struct_ends", "pt      struct\nx       db 0\n");
    assert_eq!(e, "p.asm:1: STRUCT without ENDS\n  pt      struct");
    let e = error("struct_dot", "pt      struct\n.x      db 0\n        ends\n");
    assert_eq!(e, "p.asm:2: a field's name has no dot: .x\n  .x      db 0");
}

/// STRUCT with a size pads to it, and UNION's fields all start at 0: a
/// value is given to one of them.
#[test]
fn unions_and_sizes() {
    let source = "        org 100h
        db obj.k.lift.top, obj.k.gas.off, obj.k.drip, size(params), size(obj), obj.by
lift    struct
top     db 1
bottom  db 2
wait    db 3
        ends
gas     struct
on      db 7
off     db 8
        ends
params  union                   ; where fields overlap, the first one's bytes
gas     gas
lift    lift
drip    db 0
        ends
obj     struct 8
kind    db 0
k       params
by      db 0
        ends
        data
a       obj 5, [lift=[10, 40]]
        obj k=[drip=9]
        obj
";
    let p = program("unions", source);
    let mut image = vec![1, 2, 1, 3, 8, 4];
    image.extend([5, 10, 40, 3, 0, 0, 0, 0, 0, 9, 8, 3, 0, 0, 0, 0, 0, 7, 8, 3, 0, 0, 0, 0]);
    assert_eq!(p.image, image);
    assert_eq!(value(&p, "a.k.lift.bottom"), 0x108);
    assert_eq!(value(&p, "a.k.gas.on"), 0x107);

    let e = error("struct_over", "x       struct 2\na       dw 0\nb       db 0\n        ends\n");
    assert_eq!(e, "p.asm:1: x's fields are 3 bytes, more than its 2\n  x       struct 2");
    let e = error("union_two", "p       union\na       db 0\nb       db 0\n        ends\n        p a=1, b=2\n");
    assert_eq!(e, "p.asm:5: a and b are the same bytes of p: give only one a value\n          p a=1, b=2");
    let e = error("union_ends", "p       union\na       db 0\n");
    assert_eq!(e, "p.asm:1: UNION without ENDS\n  p       union");
}

/// The value of symbol `name` in `program`'s symbols.
fn value(program: &Program, name: &str) -> u16 {
    let symbols = program.symbols();
    let line = symbols.lines().find(|l| l.split(' ').next() == Some(name)).unwrap_or_else(|| panic!("{name}\n{symbols}"));
    u16::from_str_radix(&line[name.len() + 1..], 16).unwrap()
}

/// CODE, CONST, DATA and BSS: laid out in that order, each where the one
/// before ends, and an INCLUDE leaves the includer in its section.
#[test]
fn sections() {
    let dir = write(
        "sections",
        &[
            (
                "p.asm",
                "        org 100h
main:   lxi h, hello
        lda count
        ret
        const
hello:  db 'hi', 0
        data
count:  db 7
        align 4
table:  ds 2, 0aah
        bss
buf:    ds 16
        code
        dw start(const), end(code), size(data), start(bss), SIZE(BSS), end(bss)
        include 'lib.asm'
after:  nop
",
            ),
            ("lib.asm", "        const\nlib_s:  db 'x'\n"),
        ],
    );
    let p = assemble(&dir.join("p.asm")).unwrap();
    #[rustfmt::skip]
    let want = [
        0x21, 0x14, 0x01, 0x3A, 0x18, 0x01, 0xC9,
        0x14, 0x01, 0x14, 0x01, 0x06, 0x00, 0x1E, 0x01, 0x10, 0x00, 0x2E, 0x01,
        0x00,
        b'h', b'i', 0, b'x',
        7, 0, 0, 0, 0xAA, 0xAA,
    ];
    assert_eq!((p.load, &p.image[..]), (0x100, &want[..]));
    let names = ["after", "hello", "lib_s", "count", "table", "buf"];
    let values: Vec<u16> = names.iter().map(|n| value(&p, n)).collect();
    assert_eq!(values, [0x113, 0x114, 0x117, 0x118, 0x11C, 0x11E]);

    // An ORG places a section: BSS at a fixed address, which is not the
    // load address; DATA elsewhere, with BSS after it.
    let p = program("sections_org", "        org 100h\n        lxi h, buf\n        bss\n        org 0c000h\nbuf:    ds 4\n        data\nv:      db 1\n");
    assert_eq!((p.load, &p.image[..]), (0x100, &[0x21, 0x00, 0xC0, 1][..]));
    let p = program("sections_far", "        org 100h\n        nop\n        data\n        org 200h\nx:      db 1\n        bss\ny:      ds 1\n");
    assert_eq!((value(&p, "x"), value(&p, "y"), p.image.len()), (0x200, 0x201, 0x101));

    assert_eq!(error("section_operands", "        data 5\n"), "p.asm:1: DATA takes no operands\n          data 5");
    let e = error("section_name", "        dw size(text)\n");
    assert_eq!(e, "p.asm:1: size() takes a section (code, const, data, bss) or a struct: text\n          dw size(text)");
}

/// Anonymous labels: `:`, found by `:-` and `:+`, one sign more for each
/// label further; in a REPT body, each time round has its own.
#[test]
fn anonymous() {
    let source = "        org 100h
main:   mvi b, 8
:       rlc
        jnc :+
        inr c
:       dcr b
        jnz :--
        jmp :+
        rept 2
:       dcr c
        jnz :-
        endm
:
.x:     ret
";
    let program = program("anonymous", source);
    #[rustfmt::skip]
    let want = [
        0x06, 8, 0x07, 0xD2, 0x07, 0x01, 0x0C, 0x05, 0xC2, 0x02, 0x01, 0xC3, 0x0E, 0x01,
        0x0D, 0xC2, 0x0E, 0x01, 0x0D, 0xC2, 0x12, 0x01, 0xC9,
    ];
    assert_eq!(program.image, want);
    // They are not symbols, and do not start a scope.
    assert_eq!(program.symbols(), "main 0100\nmain.x 0116\n");

    // A reference stays in its label's scope, and in its file.
    let e = error("anonymous_back", "main:\n:       nop\nother:  jmp :-\n");
    assert_eq!(e, "p.asm:3: :-: not that many anonymous labels before it in its scope\n  other:  jmp :-");
    let e = error("anonymous_on", "main:   jmp :++\n:       nop\nother:\n:       nop\n");
    assert_eq!(e, "p.asm:1: :++: not that many anonymous labels after it in its scope\n  main:   jmp :++");
    let dir = write("anonymous_file", &[("p.asm", ":       nop\n        include 'q.asm'\n"), ("q.asm", "        jmp :-\n")]);
    let e = assemble(&dir.join("p.asm")).err().unwrap().to_string();
    assert!(e.ends_with("q.asm:1: :-: not that many anonymous labels before it in its scope\n          jmp :-"), "{e}");
    assert_eq!(error("anonymous_bare", "        jmp :\n"), "p.asm:1: expected :+ or :-\n          jmp :");
}

/// INCLUDE and INCBIN look beside the including file, then in the include
/// folders in order, and INCLUDE then in the SDK's include files.
#[test]
fn include_folders() {
    let dir = write(
        "include_folders",
        &[
            ("src/p.asm", "        include 'local.inc'\n        include 'one.inc'\n        include 'two.inc'\n        include 'rombios.inc'\n        db LOCAL, ONE, TWO, low(tto)\n        incbin 'data.bin'\n"),
            ("src/local.inc", "LOCAL   equ 1\n"),
            ("inc1/one.inc", "ONE     equ 2\n"),
            ("inc1/local.inc", "LOCAL   equ 99\n"),
            ("inc2/one.inc", "ONE     equ 99\n"),
            ("inc2/two.inc", "TWO     equ 3\n"),
            ("inc2/rombios.inc", "tto     equ 4\n"),
            ("inc2/data.bin", "\u{5}"),
        ],
    );
    let options = Options { include: vec![dir.join("inc1"), dir.join("inc2")], ..Options::default() };
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
        error("case_error", "        org 100h\nmain:\n.local: jmp MAIN.local\n"),
        "p.asm:3: undefined symbol MAIN.local\n  .local: jmp MAIN.local"
    );
    assert_eq!(error("operands", "        mov a\n"), "p.asm:1: MOV takes 2 operand(s)\n          mov a");
    assert_eq!(error("mnemonic", "        ld a, b\n"), "p.asm:1: unknown mnemonic LD\n          ld a, b");
    assert_eq!(error("register", "        mov a, x\n"), "p.asm:1: not a register: x\n          mov a, x");
    assert_eq!(
        error("assert", "        nop\n        assert $ < 1, 'too big'\n"),
        "p.asm:2: assertion failed: too big\n          assert $ < 1, 'too big'"
    );
    assert_eq!(error("bss", "        bss\n        db 1\n"), "p.asm:2: only DS, ALIGN and structs in BSS\n          db 1");
    assert_eq!(error("bss_db0", "        bss\n        db 0\n"), "p.asm:2: only DS, ALIGN and structs in BSS\n          db 0");
    assert_eq!(error("bss_nop", "        bss\n        nop\n"), "p.asm:2: only DS, ALIGN and structs in BSS\n          nop");
    let e = error("bss_fill", "        bss\n        ds 4, 0ffh\n");
    assert_eq!(e, "p.asm:2: BSS is reserved, not set: DS takes no fill there\n          ds 4, 0ffh");
    // Memory ends at FFFFh, and counts past it are errors, not allocations.
    let e = error("past_end", "        org 0fff0h\n        ds 20h, 0ffh\n");
    assert_eq!(e, "p.asm:2: past FFFFh, the end of memory: 32 bytes at FFF0h\n          ds 20h, 0ffh");
    let e = error("huge_ds", "        bss\n        ds 1000000000000\n");
    assert_eq!(e, "p.asm:2: 1000000000000 bytes: more than memory holds\n          ds 1000000000000");
    assert!(error("huge_align", "        nop\n        align 1000000000000\n").contains("more than memory holds"));
    assert!(error("huge_struct", "big     struct 1000000000000\n        ends\n").contains("more than memory holds"));
    assert!(error("huge_rept", "        rept 1000000000000\n        endm\n").contains("more times than memory has bytes"));
    // A label is a name; a string's characters are bytes.
    assert_eq!(error("label_name", "123     nop\n"), "p.asm:1: not a name: 123\n  123     nop");
    assert_eq!(error("wide", "        db 'ĀĀ'\n"), "p.asm:1: Ā is not a byte: a string's characters go up to U+00FF\n          db 'ĀĀ'");
    // A file that includes itself.
    let e = error("recursive", "        include 'p.asm'\n");
    assert!(e.contains("includes nested more than 32 deep"), "{e}");
    // A struct whose definition has an error, used above it.
    let e = error("struct_broken", "        pt 1\npt      struct\nx       dq 1\n        ends\n");
    assert_eq!(e, "p.asm:1: pt is a struct with an error in its definition\n          pt 1");
    // A name defined only in an early pass is not defined.
    let e = error("stale", "        org 100h\n        if FLAG == 0\nfoo:    nop\n        endif\n        lxi h, foo\nFLAG    equ 1\n");
    assert_eq!(e, "p.asm:5: undefined symbol foo\n          lxi h, foo");
    // The IF that is not closed: the innermost one left open.
    assert_eq!(error("if", "        if 1\n        nop\n"), "p.asm:1: IF without ENDIF\n          if 1");
    let e = error("if_inner", "        if 1\n        if 0\n        endif\n        if 2\n        nop\n");
    assert_eq!(e, "p.asm:4: IF without ENDIF\n          if 2");
    assert_eq!(error("elseif", "        elseif 1\n"), "p.asm:1: ELSEIF without IF\n          elseif 1");
    let e = error("elseif_else", "        if 1\n        else\n        elseif 1\n        endif\n");
    assert_eq!(e, "p.asm:3: ELSEIF after ELSE\n          elseif 1");
    let e = error("else_twice", "        if 1\n        else\n        else\n        endif\n");
    assert_eq!(e, "p.asm:3: ELSE twice\n          else");
    assert_eq!(error("rept", "        rept 2\n        nop\n"), "p.asm:1: REPT without ENDM\n          rept 2");
    assert_eq!(error("empty", "; nothing\n"), "p.asm: no code");

    // Errors are the last pass's: the first in the source is the one
    // reported, and one only an early pass has, before a value is known,
    // is none.
    assert_eq!(error("first", "        mvi a, big\n        foo\nbig     equ 300\n"), "p.asm:1: 300 does not fit in a byte\n          mvi a, big");
    let p = program("early", "        if later == 0\n        foo\n        endif\n        nop\nlater   equ 1\n");
    assert_eq!(p.image, [0]);
    // A value that depends on itself has none.
    let e = error("settle", "        org 100h\n        if a == 100h\n        nop\n        endif\na:      nop\n");
    assert_eq!(e, "p.asm:5: a never settles: its value depends on itself\n  a:      nop");

    let e = error("missing", "        include 'no.asm'\n").replace('\\', "/");
    assert!(e.starts_with("p.asm:1: ") && e.ends_with("missing/no.asm\n          include 'no.asm'"), "{e}");

    // An error in an included file is on its line there.
    let dir = write("included", &[("p.asm", "        nop\n        include 'sub/q.asm'\n"), ("sub/q.asm", "        nop\n        foo\n")]);
    let e = assemble(&dir.join("p.asm")).err().unwrap().to_string().replace('\\', "/");
    assert!(e.ends_with("included/sub/q.asm:2: unknown mnemonic FOO\n          foo"), "{e}");
}

/// The lines that define `name` in `index`, as (kind, file's name, line).
fn defined(index: &Index, name: &str) -> Vec<(NameKind, String, usize)> {
    let file = |f: &str| Path::new(f).file_name().unwrap().to_string_lossy().into_owned();
    index.definitions.iter().filter(|d| d.name == name).map(|d| (d.kind, file(&d.file), d.line)).collect()
}

/// The lines of `file` that use `name` in `index`.
fn used(index: &Index, name: &str, file: &str) -> Vec<usize> {
    let mut lines: Vec<usize> = index.uses.iter().filter(|u| u.name == name && u.file.ends_with(file)).map(|u| u.line).collect();
    lines.sort();
    lines
}

#[test]
fn index_of_names() {
    let main = "        include 'rombios.inc' as rb
        include 'lib.asm'
COUNT   equ 3
n       set 1
n       set n + 1
point   struct
x       db 0
y       db 0
        ends
twice   macro what
        call what
        call rb.tto
        endm
        org 100h
main:   mvi a, COUNT
.loop:  dcr a
        jnz .loop
        twice helper
        twice .loop
        rept 2
        call helper
        endm
        lxi h, here.y
        db size(point), n
here    point
";
    let dir = write("index", &[("p.asm", main), ("lib.asm", "helper: ret\n")]);
    let index = juku_asm::index(&dir.join("p.asm"), &Options::default());
    let program = index.program.as_ref().unwrap();
    assert_eq!(program.symbols(), assemble(&dir.join("p.asm")).unwrap().symbols());

    use NameKind::*;
    let p = |line| ("p.asm".to_string(), line);
    let with = |kind, (file, line): (String, usize)| vec![(kind, file, line)];
    assert_eq!(defined(&index, "COUNT"), with(Equ, p(3)));
    assert_eq!(defined(&index, "n"), vec![(Set, "p.asm".into(), 4), (Set, "p.asm".into(), 5)]);
    assert_eq!(defined(&index, "point"), with(Struct, p(6)));
    assert_eq!(defined(&index, "point.y"), with(Field, p(8)), "a field on its own line");
    assert_eq!(defined(&index, "twice"), with(Macro, p(10)));
    assert_eq!(defined(&index, "main.loop"), with(Label, p(16)));
    assert_eq!(defined(&index, "here"), with(Label, p(25)));
    assert_eq!(defined(&index, "here.y"), with(Label, p(25)), "an instance's fields on its line");
    assert_eq!(defined(&index, "helper"), with(Label, ("lib.asm".into(), 1)));
    assert_eq!(defined(&index, "rb.tto"), with(Equ, ("rombios.inc".into(), 20)));
    let tto = index.definitions.iter().find(|d| d.name == "rb.tto").unwrap();
    assert_eq!(tto.file, format!("{}rombios.inc", juku_asm::HEADER_PREFIX), "a built-in file");
    let value = |name: &str| index.definitions.iter().find(|d| d.name == name).unwrap().value;
    assert_eq!((value("main.loop"), value("n"), value("point"), value("twice")), (Some(0x102), Some(2), Some(2), None));

    assert_eq!(used(&index, "COUNT", "p.asm"), [15]);
    assert_eq!(used(&index, "main.loop", "p.asm"), [11, 17, 19], "in the macro, on its line and the call's");
    assert_eq!(used(&index, "helper", "p.asm"), [11, 18, 21], "once for both of REPT's times");
    assert_eq!(used(&index, "rb.tto", "p.asm"), [12, 18, 19]);
    assert_eq!(used(&index, "twice", "p.asm"), [18, 19]);
    assert_eq!(used(&index, "point", "p.asm"), [24, 25]);
    assert_eq!(used(&index, "here.y", "p.asm"), [23]);
    assert_eq!(used(&index, "n", "p.asm"), [5, 24]);

    let includes: Vec<_> = index.includes.iter().map(|i| (i.line, i.path.rsplit(['/', '\\']).next().unwrap().to_string(), i.namespace.clone())).collect();
    assert_eq!(includes, [(1, "rombios.inc".into(), Some("rb".into())), (2, "lib.asm".into(), None)]);
    assert!(index.definitions.iter().all(|d| !d.name.contains('#')), "no expansion's own labels");
}

/// With an error, the names are still all there: those below it too.
#[test]
fn index_with_an_error() {
    let dir = write("index_error", &[("p.asm", "        org 100h\nstart:  jmp later\n        foo\nlater:  call start\n")]);
    let index = juku_asm::index(&dir.join("p.asm"), &Options::default());
    let e = index.program.as_ref().err().unwrap();
    assert_eq!((e.place().map(|(f, l)| (f.ends_with("p.asm"), l)), e.message()), (Some((true, 3)), "unknown mnemonic FOO"));
    assert_eq!(defined(&index, "later"), vec![(NameKind::Label, "p.asm".into(), 4)]);
    assert_eq!(used(&index, "start", "p.asm"), [4]);
    assert_eq!(used(&index, "later", "p.asm"), [2]);

    // In a macro, the error is at the call, and says where in the macro.
    let dir = write("index_error_macro", &[("p.asm", "m       macro\n        foo\n        endm\n        nop\n        m\n")]);
    let e = juku_asm::index(&dir.join("p.asm"), &Options::default()).program.err().unwrap();
    assert_eq!(e.place().map(|(_, l)| l), Some(5));
    assert!(e.message().starts_with("in m: ") && e.message().contains("p.asm:2: unknown mnemonic FOO"), "{}", e.message());
    // One in no line.
    let dir = write("index_error_none", &[("p.asm", "; nothing\n")]);
    let e = juku_asm::index(&dir.join("p.asm"), &Options::default()).program.err().unwrap();
    assert_eq!(e.place(), None);
}

/// An editor's text is taken in place of the file's, and is a file to
/// include though it is not saved.
#[test]
fn sources() {
    let dir = write("sources", &[("p.asm", "        nop\n")]);
    let mut options = Options::default();
    options.sources.insert(dir.join("p.asm"), "        include 'new.asm'\n        mvi a, 1\n".into());
    options.sources.insert(dir.join("sub/../new.asm"), "        mvi a, 2\n".into());
    assert_eq!(assemble_with(&dir.join("p.asm"), &options).unwrap().image, [0x3E, 2, 0x3E, 1]);
    assert_eq!(juku_asm::index(&dir.join("p.asm"), &options).includes.len(), 1);
    assert_eq!(assemble(&dir.join("p.asm")).unwrap().image, [0], "the disk's own");
}

/// What each line places: in a macro's call its expansion's, in a REPT
/// every time's.
#[test]
fn index_of_placed() {
    let source = "two     macro
        nop
        mvi a, 1
        endm
        org 100h
        two
        rept 3
        inx h
        endm
        db 1, 2
        bss
buf     ds 4
";
    let dir = write("index_placed", &[("p.asm", source)]);
    let index = juku_asm::index(&dir.join("p.asm"), &Options::default());
    let placed: Vec<_> = index.placed.iter().map(|p| (p.line, p.address, p.size, p.cycles)).collect();
    assert_eq!(placed, [(6, 0x100, 3, Some(11)), (8, 0x103, 3, Some(15)), (10, 0x106, 2, None), (12, 0x108, 4, None)]);
}
