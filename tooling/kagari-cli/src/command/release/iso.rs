//! A writer of ISO 9660 images with one flat root directory, readable on Windows (Joliet names)
//! and on Linux (Rock Ridge names and permissions, so an executable stays executable).
//!
//! Layout, in 2048-byte sectors: the system area (0–15), the primary and the Joliet volume
//! descriptor (16, 17), the set terminator (18), the path tables (19–22: primary L and M, Joliet
//! L and M), the Rock Ridge continuation area with the `ER` entry (23), the primary root
//! directory, the Joliet root directory, then each file's data from a sector boundary.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use jiff::tz::TimeZone;

const SECTOR: usize = 2048;
/// The system area before the first volume descriptor: 16 sectors.
const SYSTEM_AREA: usize = SECTOR * 16;
/// The sector of the first path table (after the two descriptors and the terminator).
const PATH_TABLES: u32 = 19;
/// The sector of the Rock Ridge continuation area (after the four path tables).
const CONTINUATION: u32 = 23;
/// The sector of the primary root directory.
const PRIMARY_ROOT: u32 = 24;
/// A path table with the root directory only: one 10-byte record.
const PATH_TABLE_SIZE: u32 = 10;
/// The longest Joliet file identifier, in UTF-16 code units (`;1` included).
const MAX_JOLIET_NAME: usize = 64;
/// The longest Rock Ridge name, in bytes: a directory record holds at most 254 bytes (an even
/// length below 256), of which 33 are fixed, 15 the level 1 identifier and its padding, 36 the
/// `PX` and 5 the `NM` entry header.
const MAX_ROCK_RIDGE_NAME: usize = 254 - 33 - 15 - 36 - 5;

const RRIP_ID: &[u8] = b"RRIP_1991A";
const RRIP_DESCRIPTOR: &[u8] =
    b"THE ROCK RIDGE INTERCHANGE PROTOCOL PROVIDES SUPPORT FOR POSIX FILE SYSTEM SEMANTICS";
const RRIP_SOURCE: &[u8] = b"PLEASE CONTACT DISC PUBLISHER FOR SPECIFICATION SOURCE.  SEE PUBLISHER IDENTIFIER IN PRIMARY VOLUME DESCRIPTOR FOR CONTACT INFORMATION.";
/// The length of the `ER` entry: an 8-byte header, then the three texts.
const ER_LEN: usize = 8 + RRIP_ID.len() + RRIP_DESCRIPTOR.len() + RRIP_SOURCE.len();
const _: () = assert!(ER_LEN <= 255);

const DIRECTORY_MODE: u32 = 0o040_555;
const EXECUTABLE_MODE: u32 = 0o100_555;
const FILE_MODE: u32 = 0o100_444;

/// A file of the image's root directory.
pub struct IsoFile {
    /// The file name players see (Joliet and Rock Ridge).
    pub name: String,
    pub source: Source,
    /// Whether Rock Ridge marks the file executable.
    pub executable: bool,
}

/// Where an [`IsoFile`]'s bytes come from.
pub enum Source {
    Bytes(Vec<u8>),
    Disk(PathBuf),
}

/// Why an image cannot be written.
pub enum IsoError {
    /// A file name the image cannot hold.
    Name(String),
    /// A file of 4 GiB or more, or an image of 8 TiB or more.
    TooLarge(String),
    /// A file to put in the image cannot be read, or changed its size while being read.
    Read(PathBuf, io::Error),
    /// The image cannot be written.
    Write(io::Error),
}

/// A file with its identifiers and place in the image.
struct Entry<'a> {
    file: &'a IsoFile,
    size: u32,
    /// The ISO 9660 level 1 identifier (`NAME.EXT;1`).
    primary: Vec<u8>,
    /// The Joliet identifier, UCS-2 big-endian (`name;1`).
    joliet: Vec<u8>,
    extent: u32,
}

/// Writes the image with the root directory `files` and the volume label `volume` to `out`.
pub fn write(out: &mut impl Write, volume: &str, files: &[IsoFile]) -> Result<(), IsoError> {
    let time = Time::now();
    let mut entries = entries(files)?;

    // Directory sizes depend only on the record lengths, so a first build with placeholder
    // extents sizes both directories; the file extents follow them.
    let primary_size = sectors_of(&primary_directory(&entries, 0, &time)?)?;
    let joliet_root = PRIMARY_ROOT
        .checked_add(primary_size)
        .ok_or_else(too_large)?;
    let joliet_size = sectors_of(&joliet_directory(&entries, joliet_root, 0, &time))?;
    let mut next = joliet_root.checked_add(joliet_size).ok_or_else(too_large)?;
    for entry in &mut entries {
        entry.extent = next;
        let sectors = entry.size.div_ceil(SECTOR as u32);
        next = next.checked_add(sectors).ok_or_else(too_large)?;
    }
    let primary = primary_directory(&entries, primary_size, &time)?;
    let joliet = joliet_directory(&entries, joliet_root, joliet_size, &time);
    let primary_root = directory_record(
        &[0],
        PRIMARY_ROOT,
        bytes_of(primary_size),
        2,
        &time.record,
        &[],
    );
    let joliet_root_record = directory_record(
        &[0],
        joliet_root,
        bytes_of(joliet_size),
        2,
        &time.record,
        &[],
    );

    let head = [
        vec![0; SYSTEM_AREA],
        volume_descriptor(1, volume, next, PATH_TABLES, &primary_root, &time),
        volume_descriptor(
            2,
            volume,
            next,
            PATH_TABLES.saturating_add(2),
            &joliet_root_record,
            &time,
        ),
        terminator(),
        path_table(PRIMARY_ROOT, false),
        path_table(PRIMARY_ROOT, true),
        path_table(joliet_root, false),
        path_table(joliet_root, true),
        padded(continuation_area()),
        primary,
        joliet,
    ];
    for part in head {
        out.write_all(&part).map_err(IsoError::Write)?;
    }
    for entry in &entries {
        match &entry.file.source {
            Source::Bytes(bytes) => out.write_all(bytes).map_err(IsoError::Write)?,
            Source::Disk(source) => copy_file(source, entry.size, out)?,
        }
        out.write_all(&vec![0; padding(entry.size)])
            .map_err(IsoError::Write)?;
    }
    Ok(())
}

/// Copies the `size` bytes of the file `source` to `out`.
fn copy_file(source: &Path, size: u32, out: &mut impl Write) -> Result<(), IsoError> {
    let read = |error| IsoError::Read(source.to_owned(), error);
    let file = File::open(source).map_err(read)?;
    let mut reader = file.take(u64::from(size));
    let mut buffer = vec![0; 1 << 16];
    let mut copied: u64 = 0;
    loop {
        let n = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(read(error)),
        };
        let chunk = buffer.get(..n).unwrap_or_default();
        out.write_all(chunk).map_err(IsoError::Write)?;
        copied = copied.saturating_add(u64::try_from(n).unwrap_or(u64::MAX));
    }
    if copied != u64::from(size) {
        return Err(read(io::Error::other("the file changed while being read")));
    }
    Ok(())
}

fn too_large() -> IsoError {
    IsoError::TooLarge("the image is too large".to_owned())
}

/// The entries of `files`, sorted by their primary identifier.
fn entries(files: &[IsoFile]) -> Result<Vec<Entry<'_>>, IsoError> {
    let mut taken = BTreeSet::new();
    let mut entries = Vec::with_capacity(files.len());
    for file in files {
        let size = match &file.source {
            Source::Bytes(bytes) => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            Source::Disk(path) => std::fs::metadata(path)
                .map_err(|error| IsoError::Read(path.clone(), error))?
                .len(),
        };
        let size = u32::try_from(size)
            .map_err(|_| IsoError::TooLarge(format!("`{}` is 4 GiB or larger", file.name)))?;
        entries.push(Entry {
            file,
            size,
            primary: primary_identifier(&file.name, &mut taken),
            joliet: joliet_identifier(&file.name)?,
            extent: 0,
        });
    }
    let mut joliet: Vec<&[u8]> = entries
        .iter()
        .map(|entry| entry.joliet.as_slice())
        .collect();
    joliet.sort_unstable();
    if let Some(pair) = joliet.windows(2).find(|pair| pair.first() == pair.get(1)) {
        let name = pair
            .first()
            .map(|id| String::from_utf16_lossy(&ucs2(id)))
            .unwrap_or_default();
        return Err(IsoError::Name(format!("two files are named `{name}`")));
    }
    entries.sort_by(|a, b| a.primary.cmp(&b.primary));
    Ok(entries)
}

/// A unique ISO 9660 level 1 identifier for `name`: up to 8 d-characters, a dot, up to 3, `;1`.
fn primary_identifier(name: &str, taken: &mut BTreeSet<Vec<u8>>) -> Vec<u8> {
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    let mut stem = d_characters(stem, 8);
    if stem.is_empty() {
        stem.push('_');
    }
    let extension = d_characters(extension, 3);
    let mut candidate = format!("{stem}.{extension};1").into_bytes();
    let mut number: u32 = 0;
    while taken.contains(&candidate) {
        number = number.saturating_add(1);
        let suffix = format!("_{number}");
        let kept: String = stem
            .chars()
            .take(8_usize.saturating_sub(suffix.len()))
            .collect();
        candidate = format!("{kept}{suffix}.{extension};1").into_bytes();
    }
    taken.insert(candidate.clone());
    candidate
}

/// The first `max` characters of `text` as d-characters (`A`–`Z`, `0`–`9`, `_`).
fn d_characters(text: &str, max: usize) -> String {
    text.chars()
        .map(|c| match c.to_ascii_uppercase() {
            c @ ('A'..='Z' | '0'..='9') => c,
            _ => '_',
        })
        .take(max)
        .collect()
}

/// `name;1` in UCS-2 big-endian (UTF-16, so names outside the BMP survive on Windows).
fn joliet_identifier(name: &str) -> Result<Vec<u8>, IsoError> {
    if let Some(c) = name
        .chars()
        .find(|&c| c.is_control() || matches!(c, '*' | '/' | ':' | ';' | '?' | '\\'))
    {
        return Err(IsoError::Name(format!(
            "the file name `{name}` contains {c:?}, which an ISO image cannot hold"
        )));
    }
    let units: Vec<u16> = format!("{name};1").encode_utf16().collect();
    if name.is_empty() || units.len() > MAX_JOLIET_NAME {
        return Err(IsoError::Name(format!(
            "the file name `{name}` must have 1 to {} UTF-16 code units",
            MAX_JOLIET_NAME.saturating_sub(2)
        )));
    }
    Ok(units.iter().flat_map(|unit| unit.to_be_bytes()).collect())
}

fn ucs2(bytes: &[u8]) -> Vec<u16> {
    let (pairs, _) = bytes.as_chunks::<2>();
    pairs.iter().map(|&pair| u16::from_be_bytes(pair)).collect()
}

/// The primary root directory, `size` sectors (0 while sizing), with Rock Ridge entries.
fn primary_directory(entries: &[Entry<'_>], size: u32, time: &Time) -> Result<Vec<u8>, IsoError> {
    let mut dot = Vec::new();
    // SP: SUSP is in use, no bytes to skip.
    dot.extend_from_slice(&[b'S', b'P', 7, 1, 0xBE, 0xEF, 0]);
    // CE: the ER entry, in the continuation area.
    let er_len = u32::try_from(ER_LEN).map_err(|_| too_large())?;
    dot.extend_from_slice(&[b'C', b'E', 28, 1]);
    dot.extend_from_slice(&both32(CONTINUATION));
    dot.extend_from_slice(&both32(0));
    dot.extend_from_slice(&both32(er_len));
    dot.extend_from_slice(&posix(DIRECTORY_MODE, 2));
    let mut records = vec![
        directory_record(&[0], PRIMARY_ROOT, bytes_of(size), 2, &time.record, &dot),
        directory_record(
            &[1],
            PRIMARY_ROOT,
            bytes_of(size),
            2,
            &time.record,
            &posix(DIRECTORY_MODE, 2),
        ),
    ];
    for entry in entries {
        let mode = if entry.file.executable {
            EXECUTABLE_MODE
        } else {
            FILE_MODE
        };
        let mut system_use = posix(mode, 1).to_vec();
        let name = entry.file.name.as_bytes();
        let nm_len = u8::try_from(name.len().saturating_add(5))
            .ok()
            .filter(|_| name.len() <= MAX_ROCK_RIDGE_NAME);
        let Some(nm_len) = nm_len else {
            return Err(IsoError::Name(format!(
                "the file name `{}` is longer than {MAX_ROCK_RIDGE_NAME} bytes",
                entry.file.name
            )));
        };
        system_use.extend_from_slice(&[b'N', b'M', nm_len, 1, 0]);
        system_use.extend_from_slice(name);
        records.push(directory_record(
            &entry.primary,
            entry.extent,
            entry.size,
            0,
            &time.record,
            &system_use,
        ));
    }
    Ok(pack(&records))
}

/// The Joliet root directory at sector `root`, `size` sectors (0 while sizing), sorted by name.
fn joliet_directory(entries: &[Entry<'_>], root: u32, size: u32, time: &Time) -> Vec<u8> {
    let mut sorted: Vec<&Entry<'_>> = entries.iter().collect();
    sorted.sort_by(|a, b| a.joliet.cmp(&b.joliet));
    let mut records = vec![
        directory_record(&[0], root, bytes_of(size), 2, &time.record, &[]),
        directory_record(&[1], root, bytes_of(size), 2, &time.record, &[]),
    ];
    for entry in sorted {
        records.push(directory_record(
            &entry.joliet,
            entry.extent,
            entry.size,
            0,
            &time.record,
            &[],
        ));
    }
    pack(&records)
}

/// A directory record (ECMA-119 9.1). The caller keeps it within 255 bytes.
fn directory_record(
    identifier: &[u8],
    extent: u32,
    size: u32,
    flags: u8,
    time: &[u8; 7],
    system_use: &[u8],
) -> Vec<u8> {
    let mut record = vec![0, 0];
    record.extend_from_slice(&both32(extent));
    record.extend_from_slice(&both32(size));
    record.extend_from_slice(time);
    record.extend_from_slice(&[flags, 0, 0]);
    record.extend_from_slice(&both16(1));
    record.push(u8::try_from(identifier.len()).unwrap_or(u8::MAX));
    record.extend_from_slice(identifier);
    if identifier.len().is_multiple_of(2) {
        record.push(0);
    }
    record.extend_from_slice(system_use);
    if !record.len().is_multiple_of(2) {
        record.push(0);
    }
    let length = u8::try_from(record.len()).unwrap_or(u8::MAX);
    if let Some(first) = record.first_mut() {
        *first = length;
    }
    record
}

/// `records` in whole sectors; a record never crosses a sector boundary.
fn pack(records: &[Vec<u8>]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for record in records {
        let used = out.len() % SECTOR;
        if used != 0 && SECTOR.saturating_sub(used) < record.len() {
            out.resize(out.len().next_multiple_of(SECTOR), 0);
        }
        out.extend_from_slice(record);
    }
    padded(out)
}

fn padded(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.resize(bytes.len().next_multiple_of(SECTOR).max(SECTOR), 0);
    bytes
}

fn sectors_of(bytes: &[u8]) -> Result<u32, IsoError> {
    u32::try_from(bytes.len() / SECTOR).map_err(|_| too_large())
}

/// `sectors` in bytes; directories are a few sectors, far below the `u32` limit.
fn bytes_of(sectors: u32) -> u32 {
    sectors.saturating_mul(SECTOR as u32)
}

/// The zero bytes after `size` bytes of file data up to the next sector boundary.
fn padding(size: u32) -> usize {
    let size = usize::try_from(size).unwrap_or(usize::MAX);
    size.next_multiple_of(SECTOR).saturating_sub(size)
}

/// The Rock Ridge `ER` entry naming RRIP 1991A.
fn continuation_area() -> Vec<u8> {
    let mut er = vec![
        b'E',
        b'R',
        ER_LEN as u8,
        1,
        RRIP_ID.len() as u8,
        RRIP_DESCRIPTOR.len() as u8,
        RRIP_SOURCE.len() as u8,
        1,
    ];
    er.extend_from_slice(RRIP_ID);
    er.extend_from_slice(RRIP_DESCRIPTOR);
    er.extend_from_slice(RRIP_SOURCE);
    er
}

/// The Rock Ridge `PX` entry (RRIP 1991A): `mode`, `links`, owner and group 0.
fn posix(mode: u32, links: u32) -> [u8; 36] {
    let mut px = [0; 36];
    px[..4].copy_from_slice(&[b'P', b'X', 36, 1]);
    px[4..12].copy_from_slice(&both32(mode));
    px[12..20].copy_from_slice(&both32(links));
    px[20..28].copy_from_slice(&both32(0));
    px[28..36].copy_from_slice(&both32(0));
    px
}

/// A primary (`kind` 1) or Joliet supplementary (`kind` 2) volume descriptor (ECMA-119 8.4).
fn volume_descriptor(
    kind: u8,
    volume: &str,
    sectors: u32,
    path_tables: u32,
    root: &[u8],
    time: &Time,
) -> Vec<u8> {
    let joliet = kind == 2;
    let text = |text: &str, len: usize| {
        if joliet {
            ucs2_field(text, len)
        } else {
            ascii_field(text, len)
        }
    };
    let mut vd = vec![kind];
    vd.extend_from_slice(b"CD001");
    vd.extend_from_slice(&[1, 0]);
    vd.extend_from_slice(&text("", 32));
    // Joliet identifiers exclude controls and `* / : ; ? \`; the label follows the same rule.
    let label = if joliet {
        volume
            .chars()
            .map(|c| {
                if c.is_control() || matches!(c, '*' | '/' | ':' | ';' | '?' | '\\') {
                    '_'
                } else {
                    c
                }
            })
            .collect()
    } else {
        d_characters(volume, 32)
    };
    vd.extend_from_slice(&text(&label, 32));
    vd.extend_from_slice(&[0; 8]);
    vd.extend_from_slice(&both32(sectors));
    let mut escapes = [0; 32];
    if joliet {
        // UCS-2 level 3.
        escapes[..3].copy_from_slice(b"%/E");
    }
    vd.extend_from_slice(&escapes);
    vd.extend_from_slice(&both16(1));
    vd.extend_from_slice(&both16(1));
    vd.extend_from_slice(&both16(SECTOR as u16));
    vd.extend_from_slice(&both32(PATH_TABLE_SIZE));
    vd.extend_from_slice(&path_tables.to_le_bytes());
    vd.extend_from_slice(&[0; 4]);
    vd.extend_from_slice(&path_tables.saturating_add(1).to_be_bytes());
    vd.extend_from_slice(&[0; 4]);
    vd.extend_from_slice(root);
    vd.extend_from_slice(&text(&label, 128));
    vd.extend_from_slice(&text("", 128));
    vd.extend_from_slice(&text("", 128));
    vd.extend_from_slice(&text(
        &format!("KAGARI {}", kagari_project::TOOLCHAIN_VERSION),
        128,
    ));
    for _ in 0..3 {
        vd.extend_from_slice(&text("", 37));
    }
    vd.extend_from_slice(&time.descriptor);
    vd.extend_from_slice(&time.descriptor);
    let mut unset = [b'0'; 17];
    unset[16] = 0;
    vd.extend_from_slice(&unset);
    vd.extend_from_slice(&unset);
    vd.extend_from_slice(&[1, 0]);
    padded(vd)
}

fn terminator() -> Vec<u8> {
    let mut vd = vec![255];
    vd.extend_from_slice(b"CD001");
    vd.push(1);
    padded(vd)
}

/// A path table with the root directory at `root`, little-endian (L) or big-endian (M).
fn path_table(root: u32, big_endian: bool) -> Vec<u8> {
    let mut table = vec![1, 0];
    if big_endian {
        table.extend_from_slice(&root.to_be_bytes());
        table.extend_from_slice(&1_u16.to_be_bytes());
    } else {
        table.extend_from_slice(&root.to_le_bytes());
        table.extend_from_slice(&1_u16.to_le_bytes());
    }
    table.extend_from_slice(&[0, 0]);
    padded(table)
}

/// `text` in a field of `len` bytes, printable ASCII, upper-cased, padded with spaces.
fn ascii_field(text: &str, len: usize) -> Vec<u8> {
    let mut field: Vec<u8> = text
        .bytes()
        .filter(|byte| *byte == b' ' || byte.is_ascii_graphic())
        .map(|byte| byte.to_ascii_uppercase())
        .take(len)
        .collect();
    field.resize(len, b' ');
    field
}

/// `text` in a field of `len` bytes, UCS-2 big-endian, padded with spaces; never splits a
/// surrogate pair.
fn ucs2_field(text: &str, len: usize) -> Vec<u8> {
    let mut field = Vec::with_capacity(len);
    for c in text.chars() {
        let mut units = [0; 2];
        let units = c.encode_utf16(&mut units);
        if field.len().saturating_add(units.len().saturating_mul(2)) > len {
            break;
        }
        for unit in units {
            field.extend_from_slice(&unit.to_be_bytes());
        }
    }
    while field.len().saturating_add(2) <= len {
        field.extend_from_slice(&[0, b' ']);
    }
    field.resize(len, 0);
    field
}

fn both16(value: u16) -> [u8; 4] {
    let [a, b] = value.to_le_bytes();
    let [c, d] = value.to_be_bytes();
    [a, b, c, d]
}

fn both32(value: u32) -> [u8; 8] {
    let mut bytes = [0; 8];
    bytes[..4].copy_from_slice(&value.to_le_bytes());
    bytes[4..].copy_from_slice(&value.to_be_bytes());
    bytes
}

/// The time the image is written, UTC, in both ISO 9660 forms.
struct Time {
    /// Directory records: years since 1900, month, day, hour, minute, second, offset.
    record: [u8; 7],
    /// Volume descriptors: `YYYYMMDDHHMMSScc` and the offset.
    descriptor: [u8; 17],
}

impl Time {
    fn now() -> Time {
        let now = Timestamp::now().to_zoned(TimeZone::UTC);
        let byte = |value: i8| u8::try_from(value).unwrap_or(0);
        let mut descriptor = [0; 17];
        let text = format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}00",
            now.year(),
            now.month(),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        );
        descriptor[..16].copy_from_slice(&text.as_bytes()[..16]);
        Time {
            record: [
                u8::try_from(now.year().saturating_sub(1900)).unwrap_or(0),
                byte(now.month()),
                byte(now.day()),
                byte(now.hour()),
                byte(now.minute()),
                byte(now.second()),
                0,
            ],
            descriptor,
        }
    }
}
