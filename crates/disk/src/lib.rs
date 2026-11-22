//! Files on EKDOS disk images.
//!
//! An image is a raw sector dump of the Juku's "D" disk: 80 cylinders of 10
//! sectors of 512 bytes, double-sided (800 KB, `.juk`) or single-sided (400
//! KB). The layout (juku-emu's docs/Juku_machine_description.md, §10.3):
//!
//! - The image holds the cylinders in order, side 0 before side 1. EKDOS
//!   numbers its tracks across the sides: logical track t is cylinder t,
//!   side 0, below 80, and cylinder t - 80, side 1, from 80 up.
//! - Within a logical track, logical sectors 0-9 are sector IDs 1, 3, 5, 7,
//!   9, 2, 4, 6, 8, 10: EKDOS's reading order. (The boot loader's 1, 3, 2,
//!   4, … reads the same directory sectors in another order, and scrambles
//!   files.)
//! - Two system tracks, then the blocks. Block 0, at logical track 2, is the
//!   directory: 4 KB, 128 entries of 32 bytes, as CP/M's, with 8-bit block
//!   numbers, 16 to an entry.
//! - EKDOS's double-sided disks have 4 KB blocks, 197 of them; single-sided
//!   ones 2 KB or 4 KB blocks, and the Juku's CP/M 2.2 has 2 KB blocks on
//!   double-sided disks. The size is taken as the one, of 1, 2, 4 and 8 KB,
//!   with which the most directory entries have the blocks their size
//!   needs; 4 KB where that does not decide, as on a disk of small files.
//!
//! Files are read from all of these, and written to EKDOS's double-sided
//! disks only.

use std::fmt;

const SECTOR: usize = 512;
const SECTORS: usize = 10;
const TRACK: usize = SECTOR * SECTORS;
const CYLINDERS: usize = 80;
const SKEW: [usize; SECTORS] = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];
const SYSTEM_TRACKS: usize = 2;
const DIR_ENTRIES: usize = 128;
const ENTRY: usize = 32;
const RECORD: usize = 128;
const EXTENT: usize = 16384;
/// An unused directory entry, and a freshly formatted disk's every byte.
const EMPTY: u8 = 0xE5;
/// Padding after the end of a file, in its last record.
const EOF: u8 = 0x1A;

/// The size of a double-sided image, and of a single-sided one.
pub const DOUBLE_SIDED: usize = 2 * CYLINDERS * TRACK;
pub const SINGLE_SIDED: usize = CYLINDERS * TRACK;

/// What went wrong.
#[derive(Debug, PartialEq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

fn error<T>(message: impl Into<String>) -> Result<T> {
    Err(Error(message.into()))
}

/// A file on a disk.
#[derive(Clone, Debug, PartialEq)]
pub struct File {
    /// The user number, 0-15.
    pub user: u8,
    /// `NAME.EXT`, or `NAME` with no extension.
    pub name: String,
    pub read_only: bool,
    pub system: bool,
    /// In bytes: whole records of 128.
    pub size: usize,
    /// Its directory entries, in extent order.
    entries: Vec<usize>,
}

impl File {
    /// The name with the user number before it, unless it is 0: `3_PROG.COM`.
    pub fn shown(&self) -> String {
        if self.user == 0 { self.name.clone() } else { format!("{}_{}", self.user, self.name) }
    }
}

/// A disk image in memory.
pub struct Disk {
    data: Vec<u8>,
    sides: usize,
    block: usize,
}

impl Disk {
    /// The disk whose image is `data`.
    pub fn new(data: Vec<u8>) -> Result<Disk> {
        let sides = match data.len() {
            DOUBLE_SIDED => 2,
            SINGLE_SIDED => 1,
            n => return error(format!("{n} bytes: not an 800 or 400 KB image")),
        };
        let mut disk = Disk { data, sides, block: 4096 };
        let mut best = disk.fitting(4096);
        for block in [2048, 1024, 8192] {
            let fits = disk.fitting(block);
            if fits > best {
                (disk.block, best) = (block, fits);
            }
        }
        Ok(disk)
    }

    /// A blank double-sided disk, formatted: no files, and no system on its
    /// system tracks.
    pub fn blank() -> Disk {
        Disk { data: vec![EMPTY; DOUBLE_SIDED], sides: 2, block: 4096 }
    }

    /// The image.
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn double_sided(&self) -> bool {
        self.sides == 2
    }

    /// Whether files can be put on the disk and removed: an EKDOS
    /// double-sided disk, with 4 KB blocks.
    pub fn writable(&self) -> bool {
        self.sides == 2 && self.block == 4096
    }

    fn check_writable(&self) -> Result<()> {
        if self.writable() { Ok(()) } else { error("only EKDOS's double-sided disks, with 4 KB blocks, can be written") }
    }

    /// The block size in bytes.
    pub fn block_size(&self) -> usize {
        self.block
    }

    /// Blocks for files, the directory's block 0 among them.
    fn blocks(&self) -> usize {
        ((CYLINDERS * self.sides - SYSTEM_TRACKS) * TRACK / self.block).min(256)
    }

    /// The image offset of logical sector `n`, counted from track 0.
    fn sector(&self, n: usize) -> Result<usize> {
        let (track, s) = (n / SECTORS, n % SECTORS);
        let (cylinder, side) = if track < CYLINDERS { (track, 0) } else { (track - CYLINDERS, 1) };
        if side >= self.sides {
            return error(format!("logical track {track} is past the disk"));
        }
        Ok(((cylinder * self.sides + side) * SECTORS + SKEW[s] - 1) * SECTOR)
    }

    /// The image offset of byte `pos` of the blocks, past the system tracks.
    fn offset(&self, pos: usize) -> Result<usize> {
        Ok(self.sector(SYSTEM_TRACKS * SECTORS + pos / SECTOR)? + pos % SECTOR)
    }

    fn read(&self, pos: usize, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        let mut at = pos;
        while at < pos + len {
            let n = (SECTOR - at % SECTOR).min(pos + len - at);
            let o = self.offset(at)?;
            out.extend_from_slice(&self.data[o..o + n]);
            at += n;
        }
        Ok(out)
    }

    fn write(&mut self, pos: usize, bytes: &[u8]) -> Result<()> {
        let mut done = 0;
        while done < bytes.len() {
            let at = pos + done;
            let n = (SECTOR - at % SECTOR).min(bytes.len() - done);
            let o = self.offset(at)?;
            self.data[o..o + n].copy_from_slice(&bytes[done..done + n]);
            done += n;
        }
        Ok(())
    }

    /// The directory's entries.
    fn entries(&self) -> Vec<[u8; ENTRY]> {
        let dir = self.read(0, DIR_ENTRIES * ENTRY).unwrap_or_default();
        dir.chunks(ENTRY).map(|e| e.try_into().unwrap()).collect()
    }

    fn set_entry(&mut self, i: usize, entry: &[u8; ENTRY]) -> Result<()> {
        self.write(i * ENTRY, entry)
    }

    /// The entries of files: a user number and a name that can be one.
    fn file_entries(&self) -> impl Iterator<Item = (usize, [u8; ENTRY])> {
        self.entries().into_iter().enumerate().filter(|(_, e)| e[0] < 16 && name_ok(e))
    }

    /// How many file entries have the blocks their size needs, with blocks
    /// of `block` bytes.
    fn fitting(&self, block: usize) -> usize {
        self.file_entries()
            .filter(|(_, e)| {
                let blocks = e[16..].iter().filter(|&&b| b != 0).count();
                let capacity = 16 * block;
                let extents = e[12] as usize * EXTENT;
                let in_entry = extents + e[15] as usize * RECORD - extents / capacity * capacity;
                blocks == in_entry.div_ceil(block)
            })
            .count()
    }

    /// The files, by user and name.
    pub fn files(&self) -> Vec<File> {
        let entries = self.entries();
        let mut files: Vec<File> = Vec::new();
        for (i, e) in self.file_entries() {
            let name = name_of(&e);
            match files.iter_mut().find(|f| f.user == e[0] && f.name == name) {
                Some(f) => f.entries.push(i),
                None => files.push(File { user: e[0], name, read_only: false, system: false, size: 0, entries: vec![i] }),
            }
        }
        for f in &mut files {
            f.entries.sort_by_key(|&i| entries[i][12]);
            let (first, last) = (&entries[f.entries[0]], &entries[*f.entries.last().unwrap()]);
            f.read_only = first[9] & 0x80 != 0;
            f.system = first[10] & 0x80 != 0;
            f.size = last[12] as usize * EXTENT + last[15] as usize * RECORD;
        }
        files.sort_by(|a, b| (a.user, raw_name(&entries[a.entries[0]])).cmp(&(b.user, raw_name(&entries[b.entries[0]]))));
        files
    }

    /// File `file`'s contents, whole records: a text file ends in ^Z padding.
    pub fn contents(&self, file: &File) -> Result<Vec<u8>> {
        let entries = self.entries();
        let exm = 16 * self.block / EXTENT - 1;
        let mut out = Vec::new();
        for &i in &file.entries {
            let e = &entries[i];
            let start = (e[12] as usize & !exm) * EXTENT;
            let mut data = Vec::new();
            for &b in e[16..].iter().filter(|&&b| b != 0) {
                if b as usize >= self.blocks() {
                    return error(format!("{}: block {b} is past the disk", file.shown()));
                }
                data.extend(self.read(b as usize * self.block, self.block)?);
            }
            if out.len() < start + data.len() {
                out.resize(start + data.len(), 0);
            }
            out[start..start + data.len()].copy_from_slice(&data);
        }
        out.truncate(file.size);
        Ok(out)
    }

    /// Puts `data` on the disk as file `name` (`NAME.EXT`) of user 0,
    /// replacing a file of that name.
    pub fn put(&mut self, name: &str, data: &[u8]) -> Result<()> {
        self.check_writable()?;
        let name = name83(name)?;
        let mut entries = self.entries();
        for e in entries.iter_mut() {
            if e[0] == 0 && e[1..12].iter().map(|c| c & 0x7F).eq(name.iter().copied()) {
                *e = [EMPTY; ENTRY];
            }
        }
        let mut used = vec![false; 256];
        used[0] = true;
        for e in entries.iter().filter(|e| e[0] != EMPTY) {
            for &b in &e[16..] {
                used[b as usize] = true;
            }
        }
        let records = data.len().div_ceil(RECORD).max(1);
        let count = (records * RECORD).div_ceil(self.block);
        let free: Vec<u8> = (1..self.blocks()).filter(|&b| !used[b]).map(|b| b as u8).take(count).collect();
        if free.len() < count {
            return error(format!("{}: the disk is full", shown83(&name)));
        }
        // One entry holds 16 blocks, 64 KB: four 16 KB extents.
        let records_a_block = self.block / RECORD;
        for first in (0..count).step_by(16) {
            let recs = (records - first * records_a_block).min(16 * records_a_block);
            let extent = first * self.block / EXTENT + (recs - 1) / (EXTENT / RECORD);
            let rc = recs - (recs - 1) / (EXTENT / RECORD) * (EXTENT / RECORD);
            let Some(slot) = entries.iter().position(|e| e[0] == EMPTY) else {
                return error(format!("{}: the directory is full", shown83(&name)));
            };
            let mut e = [0; ENTRY];
            e[1..12].copy_from_slice(&name);
            e[12] = extent as u8;
            e[15] = rc as u8;
            let blocks = &free[first..(first + 16).min(count)];
            e[16..16 + blocks.len()].copy_from_slice(blocks);
            entries[slot] = e;
        }
        for (i, &b) in free.iter().enumerate() {
            let chunk = &data[(i * self.block).min(data.len())..((i + 1) * self.block).min(data.len())];
            let mut block = chunk.to_vec();
            block.resize(self.block, EOF);
            self.write(b as usize * self.block, &block)?;
        }
        for (i, e) in entries.iter().enumerate() {
            self.set_entry(i, e)?;
        }
        Ok(())
    }

    /// Removes `file`: its entries are marked unused.
    pub fn remove(&mut self, file: &File) -> Result<()> {
        self.check_writable()?;
        let entries = self.entries();
        for &i in &file.entries {
            let mut e = entries[i];
            e[0] = EMPTY;
            self.set_entry(i, &e)?;
        }
        Ok(())
    }

    /// The bytes free for files.
    pub fn free(&self) -> usize {
        let mut used = vec![false; 256];
        for e in self.entries().iter().filter(|e| e[0] != EMPTY) {
            for &b in &e[16..] {
                used[b as usize] = true;
            }
        }
        (1..self.blocks()).filter(|&b| !used[b]).count() * self.block
    }

    /// Copies the system tracks of `from`: what EKDOS boots from.
    pub fn copy_system(&mut self, from: &Disk) -> Result<()> {
        for n in 0..SYSTEM_TRACKS * SECTORS {
            let (src, dst) = (from.sector(n)?, self.sector(n)?);
            self.data[dst..dst + SECTOR].copy_from_slice(&from.data[src..src + SECTOR]);
        }
        Ok(())
    }
}

fn name_ok(e: &[u8; ENTRY]) -> bool {
    e[1..12].iter().all(|c| (32..127).contains(&(c & 0x7F))) && e[1] & 0x7F != b' '
}

/// The 11 bytes of the name, without the attribute bits.
fn raw_name(e: &[u8; ENTRY]) -> Vec<u8> {
    e[1..12].iter().map(|c| c & 0x7F).collect()
}

/// `NAME.EXT` from an entry.
fn name_of(e: &[u8; ENTRY]) -> String {
    shown83(&raw_name(e).try_into().unwrap())
}

fn shown83(name: &[u8; 11]) -> String {
    let base = String::from_utf8_lossy(&name[..8]).trim_end().to_string();
    let ext = String::from_utf8_lossy(&name[8..]).trim_end().to_string();
    if ext.is_empty() { base } else { format!("{base}.{ext}") }
}

/// `name` as a directory entry has it: 8 + 3 bytes, upper case, spaces after.
fn name83(name: &str) -> Result<[u8; 11]> {
    let upper = name.to_ascii_uppercase();
    let (base, ext) = upper.split_once('.').unwrap_or((&upper, ""));
    let bad = |s: &str| s.bytes().any(|c| !(33..127).contains(&c) || b".,:;=*?<>[]|/\\\"".contains(&c));
    if base.is_empty() || base.len() > 8 || ext.len() > 3 || bad(base) || bad(ext) {
        return error(format!("{name}: not a file name EKDOS takes, NAME.EXT"));
    }
    let mut out = [b' '; 11];
    out[..base.len()].copy_from_slice(base.as_bytes());
    out[8..8 + ext.len()].copy_from_slice(ext.as_bytes());
    Ok(out)
}
