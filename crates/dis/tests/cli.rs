//! The command line: -r gives the range, and the options and the image can
//! come in any order.

use std::process::Command;

/// Runs juku-dis on the assembler's demo hello.bin with `args`: whether it
/// succeeded, its output and the first line of its errors.
fn run(args: &[&str]) -> (bool, String, String) {
    let demos = concat!(env!("CARGO_MANIFEST_DIR"), "/../asm/tests/demos");
    let out = Command::new(env!("CARGO_BIN_EXE_juku-dis")).current_dir(demos).args(args).output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("").to_string();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into(), err)
}

#[test]
fn ranges_and_order() {
    let lines = |args: &[&str]| run(args).1.lines().map(|l| l[..4].to_string()).collect::<Vec<_>>();
    assert_eq!(lines(&["-r", "100-10c", "hello.bin"]), ["0100", "0103", "0106", "0108", "0109"]);
    assert_eq!(lines(&["hello.bin", "-r", "0100h-0x106"]), ["0100", "0103"]);
    assert_eq!(lines(&["-r", "-103", "hello.bin"]), ["0100"]);
    assert_eq!(lines(&["-r", "13a-", "hello.bin"]), lines(&["-r", "13a", "hello.bin"]));
    assert_eq!(lines(&["-b", "0", "-r", "-3", "hello.bin"]), lines(&["--base", "0", "--range", "-3", "hello.bin"]));
    assert_eq!(run(&["-b", "0", "-r", "-3", "hello.bin"]).1, "0000  21 2B 01  lxi h, 012bh\n");
    let source = run(&["hello.bin", "-s", "-p", "-i", "rombios.inc", "-r", "100-10c"]).1;
    assert!(source.contains("include 'ports.inc' as io"), "{source}");
    assert_eq!(source, run(&["hello.bin", "-s", "--ports", "-i", "rombios.inc", "-r", "100-10c"]).1);
    assert!(source.contains("include 'rombios.inc' as rb") && source.contains("call rb.tto"), "{source}");
}

#[test]
fn errors() {
    assert_eq!(run(&[]).2, "juku-dis: no image");
    assert_eq!(run(&["hello.bin", "100"]).2, "juku-dis: one image, not hello.bin and 100: -r gives the range");
    assert_eq!(run(&["-x", "hello.bin"]).2, "juku-dis: unknown option -x");
    assert_eq!(run(&["hello.bin", "-r"]).2, "juku-dis: -r takes a value");
    assert_eq!(run(&["-r", "10c-100", "hello.bin"]).2, "10c-100: the end is not after the start");
    assert_eq!(run(&["-r", "zz", "hello.bin"]).2, "zz: not an address");
    assert_eq!(run(&["-b", "ffff", "hello.bin"]).2, "hello.bin: 61 bytes do not fit from 0ffffh to the end of memory");
}
