//! The command line: the outputs are named from the source, or from -o,
//! and the options and the source can come in any order.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A folder of the test's own, with `demo.asm` in it.
fn folder(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli").join(test);
    _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("demo.asm"), "        org 100h\nmain:   ret\n").unwrap();
    dir
}

/// Runs juku-asm in `dir` with `args`: whether it succeeded, and what it
/// printed and wrote to standard error.
fn run(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_juku-asm")).args(args).current_dir(dir).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into(), String::from_utf8_lossy(&out.stderr).into())
}

/// The files in `dir`, in order.
fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into()).collect();
    names.sort();
    names
}

#[test]
fn names_the_outputs_from_the_source() {
    let dir = folder("source");
    assert_eq!(run(&dir, &["demo.asm"]), (true, "demo.bin: 1 bytes at 0100h-0100h\n".into(), String::new()));
    assert_eq!(files(&dir), ["demo.asm", "demo.bin"]);
    assert_eq!(std::fs::read(dir.join("demo.bin")).unwrap(), [0xC9]);

    // The switches, together or apart, before or after the source.
    let dir = folder("switches");
    assert!(run(&dir, &["-lsg", "demo.asm"]).0);
    assert_eq!(files(&dir), ["demo.asm", "demo.bin", "demo.dbg", "demo.lst", "demo.sym"]);
    let dir = folder("after");
    assert!(run(&dir, &["demo.asm", "-g", "-l"]).0);
    assert_eq!(files(&dir), ["demo.asm", "demo.bin", "demo.dbg", "demo.lst"]);
    assert!(std::fs::read_to_string(dir.join("demo.dbg")).unwrap().contains(r#""image": "demo.bin""#));

    // Beside the source, wherever it is.
    let dir = folder("beside");
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::rename(dir.join("demo.asm"), dir.join("src/demo.asm")).unwrap();
    assert!(run(&dir, &["-s", "src/demo.asm"]).0);
    assert_eq!(files(&dir.join("src")), ["demo.asm", "demo.bin", "demo.sym"]);
}

#[test]
fn o_names_the_image_and_the_rest() {
    let dir = folder("o");
    std::fs::create_dir(dir.join("build")).unwrap();
    assert_eq!(run(&dir, &["-o", "build/game.com", "-lg", "demo.asm"]).1, "build/game.com: 1 bytes at 0100h-0100h\n");
    assert_eq!(files(&dir.join("build")), ["game.com", "game.dbg", "game.lst"]);
    let dbg = std::fs::read_to_string(dir.join("build/game.dbg")).unwrap();
    assert!(dbg.contains(r#""image": "game.com""#) && dbg.contains(r#""files": ["../demo.asm"]"#), "{dbg}");
}

#[test]
fn errors() {
    let dir = folder("errors");
    let error = |args: &[&str]| {
        let (ok, _, err) = run(&dir, args);
        assert!(!ok, "{args:?}");
        err.lines().next().unwrap_or("").to_string()
    };
    assert_eq!(error(&[]), "juku-asm: no source file");
    assert_eq!(error(&["demo.asm", "demo.bin"]), "juku-asm: one source file, not demo.asm and demo.bin: -o names the image");
    assert_eq!(error(&["-x", "demo.asm"]), "juku-asm: unknown option -x");
    assert_eq!(error(&["demo.asm", "-o"]), "juku-asm: -o takes a name");
    assert_eq!(error(&["-o", "demo.asm", "demo.asm"]), "demo.asm: would be written twice, or over the source: -o names the image");
    assert_eq!(error(&["-l", "-o", "demo.lst", "demo.asm"]), "demo.lst: would be written twice, or over the source: -o names the image");
    assert_eq!(error(&["-o", "./demo.asm", "demo.asm"]), "./demo.asm: would be written twice, or over the source: -o names the image");
    assert_eq!(files(&dir), ["demo.asm"], "nothing written");
    assert!(run(&dir, &["--help"]).1.starts_with("Usage: juku-asm [-l] [-s] [-g]"));
}
