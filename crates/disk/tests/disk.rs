//! Disk images made in the tests: blank ones, with files put on them, and a
//! single-sided one written by hand. The layout is checked byte by byte
//! where it matters: the directory entry and the sector order.

use std::path::{Path, PathBuf};
use std::process::Command;

use juku_disk::{DOUBLE_SIDED, Disk, SINGLE_SIDED};

/// Image offset of logical sector `s` (0-9) of cylinder `c`, side `h`, on a
/// double-sided disk: sector IDs 1, 3, 5, 7, 9, 2, 4, 6, 8, 10.
fn at(c: usize, h: usize, s: usize) -> usize {
    const SKEW: [usize; 10] = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];
    ((c * 2 + h) * 10 + SKEW[s] - 1) * 512
}

fn file(disk: &Disk, name: &str) -> Vec<u8> {
    let f = disk.files().into_iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no {name}"));
    disk.contents(&f).unwrap()
}

#[test]
fn blank() {
    let disk = Disk::blank();
    assert_eq!(disk.bytes().len(), DOUBLE_SIDED);
    assert!(disk.files().is_empty());
    assert_eq!((disk.block_size(), disk.free()), (4096, 196 * 4096));
    assert!(disk.writable());
}

#[test]
fn layout() {
    let mut disk = Disk::blank();
    let data: Vec<u8> = (0..5000).map(|i| i as u8).collect();
    disk.put("hello.txt", &data).unwrap();
    let image = disk.bytes();

    // The directory is block 0, at logical track 2: cylinder 2, side 0.
    let entry = &image[at(2, 0, 0)..at(2, 0, 0) + 32];
    let mut want = vec![0];
    want.extend(b"HELLO   TXT");
    want.extend([0, 0, 0, 40, 1, 2]);
    want.resize(32, 0);
    assert_eq!(entry, want, "user 0, the name, extent 0, 40 records, blocks 1 and 2");
    assert_eq!(image[at(2, 0, 0) + 32], 0xE5, "the next entry is free");

    // Block 1 is the next 4 KB: logical sectors 8 and 9 of track 2, then
    // track 3.
    assert_eq!(&image[at(2, 0, 8)..at(2, 0, 8) + 512], &data[..512]);
    assert_eq!(&image[at(2, 0, 9)..at(2, 0, 9) + 512], &data[512..1024]);
    assert_eq!(&image[at(3, 0, 0)..at(3, 0, 0) + 512], &data[1024..1536]);
    // Logical track 80 is cylinder 0, side 1.
    let mut far = Disk::blank();
    far.put("big.dat", &vec![7; 600 * 1024]).unwrap();
    assert_eq!(far.bytes()[at(0, 1, 0)], 7);

    // The last record is padded with ^Z.
    let back = file(&disk, "HELLO.TXT");
    assert_eq!(back.len(), 5120);
    assert_eq!(&back[..5000], &data[..]);
    assert!(back[5000..].iter().all(|&b| b == 0x1A));
}

#[test]
fn files_of_every_size() {
    let mut disk = Disk::blank();
    let big: Vec<u8> = (0..150_000u32).map(|i| (i * 7 + i / 256) as u8).collect();
    for (name, data) in [("EMPTY.TXT", vec![]), ("ONE.BIN", vec![1; 128]), ("BLOCK.BIN", vec![2; 4096]), ("BIG.DAT", big.clone())] {
        disk.put(name, &data).unwrap();
        let back = file(&disk, name);
        assert_eq!(&back[..data.len()], &data[..], "{name}");
        assert_eq!(back.len(), data.len().div_ceil(128).max(1) * 128, "{name}: whole records");
    }
    // 150 000 bytes need 37 blocks: three entries of up to 16.
    let names: Vec<String> = disk.files().iter().map(|f| f.name.clone()).collect();
    assert_eq!(names, ["BIG.DAT", "BLOCK.BIN", "EMPTY.TXT", "ONE.BIN"]);
    assert_eq!(disk.free(), (196 - 37 - 1 - 1 - 1) * 4096);

    // Put again, it replaces; removed, its blocks are free.
    disk.put("big.dat", b"short").unwrap();
    assert_eq!(&file(&disk, "BIG.DAT")[..5], b"short");
    assert_eq!(disk.files().len(), 4);
    let one = disk.files().into_iter().find(|f| f.name == "ONE.BIN").unwrap();
    disk.remove(&one).unwrap();
    assert_eq!(disk.files().len(), 3);
    assert_eq!(disk.free(), (196 - 3) * 4096);
}

#[test]
fn refused() {
    let mut disk = Disk::blank();
    for name in ["", "TOOLONGNAME.TXT", "A.TEXT", "A B.TXT", "A*.TXT", "A.B.C"] {
        assert!(disk.put(name, b"x").is_err(), "{name:?}");
    }
    assert!(disk.put("a.txt", &vec![0; 900 * 1024]).unwrap_err().0.contains("the disk is full"));
    for i in 0..128 {
        disk.put(&format!("F{i}.TXT"), b"x").unwrap();
    }
    assert!(disk.put("one.txt", b"x").unwrap_err().0.contains("the directory is full"));
    assert!(Disk::new(vec![0; 1000]).is_err());
}

/// A single-sided disk with 2 KB blocks, written as its own BIOS would:
/// read, and not written.
#[test]
fn single_sided() {
    let mut image = vec![0xE5; SINGLE_SIDED];
    // Logical track t is cylinder t; logical sector s is ID SKEW[s].
    let at1 = |c: usize, s: usize| (c * 10 + [1, 3, 5, 7, 9, 2, 4, 6, 8, 10][s] - 1) * 512;
    let mut entry = vec![0];
    entry.extend(b"NOTE    TXT");
    entry.extend([0, 0, 0, 24, 2, 3]); // 3 KB in two 2 KB blocks
    entry.resize(32, 0);
    image[at1(2, 0)..at1(2, 0) + 32].copy_from_slice(&entry);
    // Block 2 starts 4 KB into the data: logical sector 8 of track 2.
    image[at1(2, 8)..at1(2, 8) + 512].fill(b'a');
    image[at1(2, 9)..at1(2, 9) + 512].fill(b'b');
    image[at1(3, 0)..at1(3, 0) + 512].fill(b'c');
    let mut disk = Disk::new(image).unwrap();
    assert_eq!((disk.double_sided(), disk.block_size()), (false, 2048));
    let text = file(&disk, "NOTE.TXT");
    assert_eq!(text.len(), 3072);
    assert_eq!((text[0], text[512], text[1024]), (b'a', b'b', b'c'));
    assert!(disk.put("x.txt", b"x").is_err());
}

#[test]
fn system_tracks() {
    let mut from = Disk::blank();
    from.put("x.txt", b"x").unwrap();
    let mut source = from.bytes().to_vec();
    source[at(0, 0, 0)..at(1, 0, 9) + 512].iter_mut().enumerate().for_each(|(i, b)| *b = i as u8);
    let from = Disk::new(source.clone()).unwrap();
    let mut disk = Disk::blank();
    disk.copy_system(&from).unwrap();
    assert_eq!(&disk.bytes()[at(0, 0, 0)..at(0, 0, 0) + 10 * 512], &source[at(0, 0, 0)..at(0, 0, 0) + 10 * 512]);
    assert_eq!(&disk.bytes()[at(1, 0, 0)..at(1, 0, 0) + 10 * 512], &source[at(1, 0, 0)..at(1, 0, 0) + 10 * 512]);
    assert!(disk.files().is_empty(), "the system only, not the files");
}

/// The command, on a disk of its own.
#[test]
fn command() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("command");
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_juku-disk")).current_dir(&dir).args(args).output().unwrap();
        (out.status.success(), String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
    };
    std::fs::write(dir.join("prog.com"), [0xC9; 300]).unwrap();
    std::fs::write(dir.join("read.me"), b"text").unwrap();
    assert!(run(&["new", "d.juk", "-f"]).0);
    assert!(!run(&["new", "d.juk"]).0, "d.juk exists");
    assert_eq!(run(&["put", "d.juk", "prog.com", "read.me"]).1, "PROG.COM 300\nREAD.ME 4\n");
    let (_, ls, _) = run(&["ls", "d.juk"]);
    assert_eq!(ls, "d.juk: 2 files, 4 KB blocks, 776 KB free\nPROG.COM             384\nREAD.ME              128\n");
    let out: PathBuf = dir.join("out");
    let (ok, text, _) = run(&["get", "d.juk", "*.com", "-o", "out", "-f"]);
    assert!(ok, "{text}");
    assert_eq!(std::fs::read(out.join("PROG.COM")).unwrap().len(), 384);
    let (ok, _, err) = run(&["get", "d.juk", "nosuch.txt", "-o", "out"]);
    assert!(!ok && err.contains("nosuch.txt: no such file"), "{err}");
    assert_eq!(run(&["rm", "d.juk", "read.me"]).1, "READ.ME\n");
    assert!(run(&["ls", "d.juk"]).1.starts_with("d.juk: 1 file,"));
}
