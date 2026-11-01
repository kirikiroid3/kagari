//! A writer of ZIP archives that keep Unix permissions (macOS's Archive Utility and `ditto`
//! restore them, so an executable stays executable), with names in UTF-8 and ZIP64 records for
//! entries, offsets and archives of 4 GiB or more.
//!
//! Each entry's local header is written with placeholder sizes and CRC, then patched once its
//! data is written, so the data is read once and no data descriptors are needed.

use std::io::{self, Read, Seek, SeekFrom, Write};

use flate2::Compression;
use flate2::write::DeflateEncoder;
use jiff::Zoned;

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
const ZIP64_END_OF_CENTRAL_DIRECTORY: u32 = 0x0606_4b50;
const ZIP64_LOCATOR: u32 = 0x0706_4b50;
const ZIP64_EXTRA: u16 = 0x0001;
/// Version made by: Unix (3) in the high byte, so readers take the mode from the external
/// attributes; ZIP 4.5 (ZIP64) in the low byte.
const MADE_BY: u16 = (3 << 8) | 45;
const VERSION_DEFAULT: u16 = 20;
const VERSION_ZIP64: u16 = 45;
/// General purpose flag 11: the name is UTF-8.
const FLAG_UTF8: u16 = 1 << 11;
const STORED: u16 = 0;
const DEFLATED: u16 = 8;
/// The MS-DOS directory attribute, next to the Unix mode in the external attributes.
const DOS_DIRECTORY: u32 = 0x10;
const DIRECTORY_MODE: u32 = 0o040_755;
/// Above this size an entry's local header gets ZIP64 sizes up front: deflate may grow
/// incompressible data a little, so the margin keeps a smaller entry below 4 GiB.
const ZIP64_LOCAL_THRESHOLD: u64 = 0xF000_0000;
const U32_MAX: u64 = u32::MAX as u64;

/// How an entry's data is stored.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Stored,
    Deflated,
}

/// An entry of the central directory.
struct Central {
    name: String,
    method: u16,
    external: u32,
    crc: u32,
    compressed: u64,
    uncompressed: u64,
    offset: u64,
}

/// A ZIP archive being written to `out`.
pub struct ZipWriter<W: Write + Seek> {
    out: W,
    /// Where the next local header goes: the bytes written so far.
    position: u64,
    entries: Vec<Central>,
    time: u16,
    date: u16,
}

impl<W: Write + Seek> ZipWriter<W> {
    /// A writer at the start of `out`, dating every entry now (local time).
    pub fn new(out: W) -> Self {
        let (time, date) = dos_time(&Zoned::now());
        ZipWriter {
            out,
            position: 0,
            entries: Vec::new(),
            time,
            date,
        }
    }

    /// Adds the directory `name` (ending in `/`).
    pub fn directory(&mut self, name: &str) -> io::Result<()> {
        let offset = self.position;
        let header = local_header(name, STORED, 0, false, self.time, self.date);
        self.write(&header)?;
        self.entries.push(Central {
            name: name.to_owned(),
            method: STORED,
            external: (DIRECTORY_MODE << 16) | DOS_DIRECTORY,
            crc: 0,
            compressed: 0,
            uncompressed: 0,
            offset,
        });
        Ok(())
    }

    /// Adds the file `name` with the Unix permission bits `mode`, reading `data` to its end;
    /// `size` is its length, which `data` must deliver.
    pub fn file(
        &mut self,
        name: &str,
        mode: u32,
        method: Method,
        size: u64,
        data: &mut impl Read,
    ) -> io::Result<()> {
        let offset = self.position;
        let zip64 = size > ZIP64_LOCAL_THRESHOLD;
        let code = match method {
            Method::Stored => STORED,
            Method::Deflated => DEFLATED,
        };
        let header = local_header(name, code, 0, zip64, self.time, self.date);
        self.write(&header)?;

        let mut crc = crc32fast::Hasher::new();
        let mut counter = Counter {
            out: &mut self.out,
            written: 0,
        };
        let mut read: u64 = 0;
        let mut buffer = vec![0; 1 << 16];
        match method {
            Method::Stored => loop {
                let n = data.read(&mut buffer)?;
                let Some(chunk) = buffer.get(..n).filter(|chunk| !chunk.is_empty()) else {
                    break;
                };
                crc.update(chunk);
                counter.write_all(chunk)?;
                read = read.saturating_add(n as u64);
            },
            Method::Deflated => {
                let mut encoder = DeflateEncoder::new(&mut counter, Compression::default());
                loop {
                    let n = data.read(&mut buffer)?;
                    let Some(chunk) = buffer.get(..n).filter(|chunk| !chunk.is_empty()) else {
                        break;
                    };
                    crc.update(chunk);
                    encoder.write_all(chunk)?;
                    read = read.saturating_add(n as u64);
                }
                encoder.finish()?;
            }
        }
        let compressed = counter.written;
        if read != size {
            return Err(io::Error::other(format!(
                "{name}: read {read} bytes, expected {size}"
            )));
        }
        if !zip64 && compressed >= U32_MAX {
            return Err(io::Error::other(format!(
                "{name}: deflating grew it past 4 GiB"
            )));
        }
        let crc = crc.finalize();

        // Patch the CRC and the sizes, in the fixed fields or in the ZIP64 extra field.
        // `write` advanced `position` past the header already.
        let end = self
            .position
            .checked_add(compressed)
            .ok_or_else(too_large)?;
        self.out.seek(SeekFrom::Start(offset.saturating_add(14)))?;
        self.out.write_all(&crc.to_le_bytes())?;
        if zip64 {
            let extra = offset
                .saturating_add(30)
                .saturating_add(name.len() as u64)
                .saturating_add(4);
            self.out.seek(SeekFrom::Start(extra))?;
            self.out.write_all(&size.to_le_bytes())?;
            self.out.write_all(&compressed.to_le_bytes())?;
        } else {
            self.out.write_all(&(compressed as u32).to_le_bytes())?;
            self.out.write_all(&(size as u32).to_le_bytes())?;
        }
        self.out.seek(SeekFrom::Start(end))?;
        self.position = end;
        self.entries.push(Central {
            name: name.to_owned(),
            method: code,
            external: (0o100_000 | (mode & 0o7777)) << 16,
            crc,
            compressed,
            uncompressed: size,
            offset,
        });
        Ok(())
    }

    /// Writes the central directory and the end records; returns `out`.
    pub fn finish(mut self) -> io::Result<W> {
        let start = self.position;
        let entries = std::mem::take(&mut self.entries);
        for entry in &entries {
            let record = central_header(entry, self.time, self.date);
            self.write(&record)?;
        }
        let size = self.position.saturating_sub(start);
        let count = entries.len() as u64;
        let zip64 = count >= 0xFFFF || size >= U32_MAX || start >= U32_MAX;
        if zip64 {
            let record_offset = self.position;
            let mut record = Vec::with_capacity(56);
            put32(&mut record, ZIP64_END_OF_CENTRAL_DIRECTORY);
            put64(&mut record, 44);
            put16(&mut record, MADE_BY);
            put16(&mut record, VERSION_ZIP64);
            put32(&mut record, 0);
            put32(&mut record, 0);
            put64(&mut record, count);
            put64(&mut record, count);
            put64(&mut record, size);
            put64(&mut record, start);
            put32(&mut record, ZIP64_LOCATOR);
            put32(&mut record, 0);
            put64(&mut record, record_offset);
            put32(&mut record, 1);
            self.write(&record)?;
        }
        let mut end = Vec::with_capacity(22);
        put32(&mut end, END_OF_CENTRAL_DIRECTORY);
        put16(&mut end, 0);
        put16(&mut end, 0);
        put16(&mut end, count.min(0xFFFF) as u16);
        put16(&mut end, count.min(0xFFFF) as u16);
        put32(&mut end, size.min(U32_MAX) as u32);
        put32(&mut end, start.min(U32_MAX) as u32);
        put16(&mut end, 0);
        self.write(&end)?;
        Ok(self.out)
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.out.write_all(bytes)?;
        self.position = self
            .position
            .checked_add(bytes.len() as u64)
            .ok_or_else(too_large)?;
        Ok(())
    }
}

/// A writer that counts what passes through it.
struct Counter<'a, W: Write> {
    out: &'a mut W,
    written: u64,
}

impl<W: Write> Write for Counter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.out.write(bytes)?;
        self.written = self.written.saturating_add(n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// A local file header; a `zip64` one carries the sizes in a ZIP64 extra field.
fn local_header(name: &str, method: u16, crc: u32, zip64: bool, time: u16, date: u16) -> Vec<u8> {
    let mut header = Vec::with_capacity(30usize.saturating_add(name.len()).saturating_add(20));
    put32(&mut header, LOCAL_HEADER);
    put16(
        &mut header,
        if zip64 {
            VERSION_ZIP64
        } else {
            VERSION_DEFAULT
        },
    );
    put16(&mut header, FLAG_UTF8);
    put16(&mut header, method);
    put16(&mut header, time);
    put16(&mut header, date);
    put32(&mut header, crc);
    let placeholder = if zip64 { u32::MAX } else { 0 };
    put32(&mut header, placeholder);
    put32(&mut header, placeholder);
    put16(&mut header, name.len() as u16);
    put16(&mut header, if zip64 { 20 } else { 0 });
    header.extend_from_slice(name.as_bytes());
    if zip64 {
        put16(&mut header, ZIP64_EXTRA);
        put16(&mut header, 16);
        put64(&mut header, 0);
        put64(&mut header, 0);
    }
    header
}

/// The central directory header of `entry`: sizes or an offset of 4 GiB or more move all three
/// into a ZIP64 extra field.
fn central_header(entry: &Central, time: u16, date: u16) -> Vec<u8> {
    let zip64 =
        entry.compressed >= U32_MAX || entry.uncompressed >= U32_MAX || entry.offset >= U32_MAX;
    let mut header =
        Vec::with_capacity(46usize.saturating_add(entry.name.len()).saturating_add(28));
    put32(&mut header, CENTRAL_HEADER);
    put16(&mut header, MADE_BY);
    put16(
        &mut header,
        if zip64 {
            VERSION_ZIP64
        } else {
            VERSION_DEFAULT
        },
    );
    put16(&mut header, FLAG_UTF8);
    put16(&mut header, entry.method);
    put16(&mut header, time);
    put16(&mut header, date);
    put32(&mut header, entry.crc);
    if zip64 {
        put32(&mut header, u32::MAX);
        put32(&mut header, u32::MAX);
    } else {
        put32(&mut header, entry.compressed as u32);
        put32(&mut header, entry.uncompressed as u32);
    }
    put16(&mut header, entry.name.len() as u16);
    put16(&mut header, if zip64 { 28 } else { 0 });
    put16(&mut header, 0); // comment length
    put16(&mut header, 0); // disk number
    put16(&mut header, 0); // internal attributes
    put32(&mut header, entry.external);
    put32(
        &mut header,
        if zip64 { u32::MAX } else { entry.offset as u32 },
    );
    header.extend_from_slice(entry.name.as_bytes());
    if zip64 {
        put16(&mut header, ZIP64_EXTRA);
        put16(&mut header, 24);
        put64(&mut header, entry.uncompressed);
        put64(&mut header, entry.compressed);
        put64(&mut header, entry.offset);
    }
    header
}

/// The MS-DOS time and date of `now`; before 1980 (which DOS dates cannot hold) is 1980-01-01.
fn dos_time(now: &Zoned) -> (u16, u16) {
    let year = now.year();
    if year < 1980 {
        return (0, (1 << 5) | 1);
    }
    let years = year.saturating_sub(1980).min(127) as u16;
    let time =
        ((now.hour() as u16) << 11) | ((now.minute() as u16) << 5) | (now.second() as u16 / 2);
    let date = (years << 9) | ((now.month() as u16) << 5) | now.day() as u16;
    (time, date)
}

fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn too_large() -> io::Error {
    io::Error::other("the archive is too large")
}
