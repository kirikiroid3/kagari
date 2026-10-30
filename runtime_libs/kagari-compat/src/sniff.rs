//! Format detection by content. Every read is bounds-checked: truncated data falls back to the
//! rule's "unrecognized" answer instead of failing.

use std::io::{self, Read, Seek, SeekFrom};

use kagari_bytecode::AssetKind;

use crate::AssetFormat;

/// The format of `bytes`, an asset of `kind` (or its first bytes). The first matching rule wins.
pub fn sniff(kind: AssetKind, bytes: &[u8]) -> AssetFormat {
    if kind == AssetKind::Shader {
        return if std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0) {
            AssetFormat::Wgsl
        } else {
            AssetFormat::Unknown
        };
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return AssetFormat::Png;
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return jpeg(bytes);
    }
    if bytes.starts_with(b"RIFF") {
        return riff(bytes);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return AssetFormat::Gif;
    }
    if kind == AssetKind::Image && bytes.starts_with(b"BM") {
        return bmp(bytes);
    }
    if bytes.starts_with(&[0xFF, 0x0A])
        || bytes.starts_with(&[
            0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
        ])
    {
        return jxl(bytes);
    }
    if bytes.get(4..8) == Some(b"ftyp") {
        return iso_bmff(kind, bytes);
    }
    if bytes.starts_with(b"OggS") {
        return ogg(bytes);
    }
    if let Some(format) = audio_stream(bytes) {
        return format;
    }
    if bytes.starts_with(b"ID3") {
        return id3(bytes);
    }
    font(bytes).unwrap_or(AssetFormat::Unknown)
}

fn u16_be(bytes: &[u8], at: usize) -> Option<u16> {
    let b = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([*b.first()?, *b.get(1)?]))
}

fn u16_le(bytes: &[u8], at: usize) -> Option<u16> {
    let b = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*b.first()?, *b.get(1)?]))
}

fn u32_le(bytes: &[u8], at: usize) -> Option<u32> {
    let b: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(b))
}

fn u32_be(bytes: &[u8], at: usize) -> Option<u32> {
    let b: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_be_bytes(b))
}

fn u64_be(bytes: &[u8], at: usize) -> Option<u64> {
    let b: [u8; 8] = bytes.get(at..at.checked_add(8)?)?.try_into().ok()?;
    Some(u64::from_be_bytes(b))
}

/// Walks the JPEG markers up to the first frame header.
fn jpeg(bytes: &[u8]) -> AssetFormat {
    jpeg_frame(bytes).unwrap_or(AssetFormat::JpegUnsupported)
}

fn jpeg_frame(bytes: &[u8]) -> Option<AssetFormat> {
    let mut i: usize = 2;
    loop {
        if *bytes.get(i)? != 0xFF {
            return None;
        }
        while *bytes.get(i)? == 0xFF {
            i = i.checked_add(1)?;
        }
        let marker = *bytes.get(i)?;
        i = i.checked_add(1)?;
        match marker {
            0xD0..=0xD7 | 0x01 => continue,
            0xD9 | 0xDA => return None,
            _ => {}
        }
        let len = usize::from(u16_be(bytes, i)?);
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let precision = *bytes.get(i.checked_add(2)?)?;
            return Some(match (marker, precision) {
                (0xC0 | 0xC1, 8) => AssetFormat::JpegBaseline,
                (0xC2, 8) => AssetFormat::JpegProgressive,
                _ => AssetFormat::JpegUnsupported,
            });
        }
        i = i.checked_add(len)?;
    }
}

/// Mirrors what the `image` BMP decoder accepts: a known DIB header size, one plane, and an
/// uncompressed, RLE or bitfields pixel layout (top-down images only uncompressed or bitfields).
/// It refuses embedded JPEG or PNG and CMYK.
fn bmp(bytes: &[u8]) -> AssetFormat {
    if bmp_decodable(bytes) == Some(true) {
        AssetFormat::Bmp
    } else {
        AssetFormat::BmpUnsupported
    }
}

fn bmp_decodable(bytes: &[u8]) -> Option<bool> {
    let header = u32_le(bytes, 14)?;
    if header == 12 {
        let bits = u16_le(bytes, 24)?;
        return Some(u16_le(bytes, 22)? == 1 && matches!(bits, 1 | 4 | 8 | 24));
    }
    if !matches!(header, 40 | 52 | 56 | 108 | 124) {
        return Some(false);
    }
    let top_down = u32_le(bytes, 22)? >> 31 == 1;
    let bits = u16_le(bytes, 28)?;
    let compression = u32_le(bytes, 30)?;
    let layout = match compression {
        0 => matches!(bits, 1 | 2 | 4 | 8 | 16 | 24 | 32),
        1 => bits == 8 && !top_down,
        2 => bits == 4 && !top_down,
        3 => matches!(bits, 16 | 32),
        _ => false,
    };
    Some(u16_le(bytes, 26)? == 1 && layout)
}

/// Parses the image header (container boxes, size, metadata, ICC profile) with the same decoder
/// the runtime uses. The runtime renders only the first frame and has no colour management to
/// turn CMYK into RGB, so animations and CMYK are refused, as is a header that does not parse
/// within `bytes`.
fn jxl(bytes: &[u8]) -> AssetFormat {
    let decodable = (|| {
        let mut uninit = jxl_oxide::JxlImage::builder().build_uninit();
        uninit.feed_bytes(bytes).ok()?;
        match uninit.try_init().ok()? {
            jxl_oxide::InitializeResult::Initialized(image) => Some(
                image.image_header().metadata.animation.is_none()
                    && !image.pixel_format().has_black(),
            ),
            jxl_oxide::InitializeResult::NeedMoreData(_) => None,
        }
    })();
    if decodable == Some(true) {
        AssetFormat::Jxl
    } else {
        AssetFormat::JxlUnsupported
    }
}

fn riff(bytes: &[u8]) -> AssetFormat {
    match bytes.get(8..12) {
        Some(b"WEBP") => match bytes.get(12..16) {
            Some(b"VP8 ") => AssetFormat::WebpLossy,
            Some(b"VP8L") => AssetFormat::WebpLossless,
            Some(b"VP8X") => match bytes.get(20) {
                Some(flags) if flags & 0x02 != 0 => AssetFormat::WebpAnimated,
                Some(_) => AssetFormat::WebpExtended,
                None => AssetFormat::Unknown,
            },
            _ => AssetFormat::Unknown,
        },
        Some(b"WAVE") => wav(bytes).unwrap_or(AssetFormat::WavOther),
        _ => AssetFormat::Unknown,
    }
}

/// Finds the `fmt ` chunk and reads its format tag.
fn wav(bytes: &[u8]) -> Option<AssetFormat> {
    let mut at: usize = 12;
    loop {
        let id = bytes.get(at..at.checked_add(4)?)?;
        let size = usize::try_from(u32_le(bytes, at.checked_add(4)?)?).ok()?;
        let data = at.checked_add(8)?;
        if id == b"fmt " {
            let tag = match u16_le(bytes, data)? {
                0xFFFE if size >= 40 => u16_le(bytes, data.checked_add(24)?)?,
                tag => tag,
            };
            return match tag {
                1 => Some(AssetFormat::WavPcm),
                3 => Some(AssetFormat::WavFloat),
                _ => None,
            };
        }
        at = data.checked_add(size)?.checked_add(size & 1)?;
    }
}

fn iso_bmff(kind: AssetKind, bytes: &[u8]) -> AssetFormat {
    match kind {
        AssetKind::Image => {
            let size = u32_be(bytes, 0)
                .and_then(|size| usize::try_from(size).ok())
                .unwrap_or(0);
            let compatible = (16..size)
                .step_by(4)
                .map_while(|at| bytes.get(at..at.checked_add(4)?));
            let mut brands = bytes.get(8..12).into_iter().chain(compatible);
            if brands.any(|brand| brand == b"avif" || brand == b"avis") {
                AssetFormat::Avif
            } else {
                AssetFormat::Unknown
            }
        }
        AssetKind::Audio | AssetKind::Voice => mp4_audio(bytes).unwrap_or(AssetFormat::Mp4Other),
        _ => AssetFormat::Unknown,
    }
}

/// Splits the first box off `bytes`: its type, its payload and the bytes after it.
fn split_box(bytes: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    let kind = bytes.get(4..8)?;
    let (header, size) = match u32_be(bytes, 0)? {
        0 => (8, bytes.len()),
        1 => (16, usize::try_from(u64_be(bytes, 8)?).ok()?),
        size => (8, usize::try_from(size).ok()?),
    };
    Some((kind, bytes.get(header..size)?, bytes.get(size..)?))
}

/// The boxes in `bytes` as (type, payload), up to the first that does not fit.
fn boxes(mut bytes: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    std::iter::from_fn(move || {
        let (kind, body, rest) = split_box(bytes)?;
        bytes = rest;
        Some((kind, body))
    })
}

/// The payload of the first box of type `kind` in `bytes`.
fn child<'a>(bytes: &'a [u8], kind: &[u8]) -> Option<&'a [u8]> {
    boxes(bytes).find_map(|(k, body)| (k == kind).then_some(body))
}

/// The first sample description of the first audio (`soun`) track, the track the runtime plays.
/// Its children are read like the `symphonia` MP4 reader does: AAC from `esds` (directly or in a
/// QuickTime `wave`), ALAC from the `alac` magic cookie.
fn mp4_audio(bytes: &[u8]) -> Option<AssetFormat> {
    let moov = child(bytes, b"moov")?;
    let stbl = boxes(moov)
        .filter(|(kind, _)| *kind == b"trak")
        .find_map(|(_, trak)| {
            let mdia = child(trak, b"mdia")?;
            (child(mdia, b"hdlr")?.get(8..12)? == b"soun").then_some(())?;
            child(child(mdia, b"minf")?, b"stbl")
        })?;
    let (kind, entry, _) = split_box(child(stbl, b"stsd")?.get(8..)?)?;
    let fields = match u16_be(entry, 8)? {
        0 => 28,
        1 => 44,
        2 => 64,
        _ => return None,
    };
    let children = entry.get(fields..)?;
    let config =
        |kind: &[u8]| child(children, kind).or_else(|| child(child(children, b"wave")?, kind));
    match kind {
        b"mp4a" => esds(config(b"esds")?),
        b"alac" => alac(config(b"alac")?),
        _ => None,
    }
}

/// An MPEG-4 descriptor at the start of `bytes`: its tag, payload and the bytes after it.
fn descriptor(bytes: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let tag = *bytes.first()?;
    let mut len: usize = 0;
    let mut at: usize = 1;
    for _ in 0..4 {
        let byte = *bytes.get(at)?;
        at = at.checked_add(1)?;
        len = len
            .checked_mul(128)?
            .checked_add(usize::from(byte & 0x7F))?;
        if byte & 0x80 == 0 {
            break;
        }
    }
    let end = at.checked_add(len)?;
    Some((tag, bytes.get(at..end)?, bytes.get(end..)?))
}

/// The descriptor with `tag` among those in `bytes`.
fn find_descriptor(mut bytes: &[u8], tag: u8) -> Option<&[u8]> {
    loop {
        let (found, body, rest) = descriptor(bytes)?;
        if found == tag {
            return Some(body);
        }
        bytes = rest;
    }
}

/// The `esds` payload (after its version and flags): an ES descriptor whose decoder config
/// names MPEG-4 or MPEG-2 Main/LC AAC and carries the AudioSpecificConfig.
fn esds(esds: &[u8]) -> Option<AssetFormat> {
    let (tag, es, _) = descriptor(esds.get(4..)?)?;
    if tag != 3 {
        return None;
    }
    let flags = *es.get(2)?;
    let mut at: usize = 3;
    if flags & 0x80 != 0 {
        at = at.checked_add(2)?;
    }
    if flags & 0x40 != 0 {
        at = at.checked_add(usize::from(*es.get(at)?))?.checked_add(1)?;
    }
    if flags & 0x20 != 0 {
        at = at.checked_add(2)?;
    }
    let config = find_descriptor(es.get(at..)?, 4)?;
    if !matches!(config.first()?, 0x40 | 0x66 | 0x67) {
        return None;
    }
    let asc = find_descriptor(config.get(13..)?, 5)?;
    Some(if aac_lc(asc) == Some(true) {
        AssetFormat::Mp4Aac
    } else {
        AssetFormat::AacUnsupported
    })
}

/// Mirrors what the `symphonia` AAC decoder accepts from an AudioSpecificConfig: object type
/// AAC-LC (so no explicit SBR or PS), a valid sample rate, one or two channels (no program config
/// element), 1024-sample frames and no version 3 extensions.
fn aac_lc(asc: &[u8]) -> Option<bool> {
    let mut bits = Bits { bytes: asc, at: 0 };
    let object_type = match bits.read(5)? {
        31 => bits.read(6)?.checked_add(32)?,
        object_type => object_type,
    };
    let rate_ok = match bits.read(4)? {
        15 => bits.read(24)? != 0,
        index => index < 13,
    };
    let channels = bits.read(4)?;
    if object_type != 2 || !rate_ok || !matches!(channels, 1 | 2) {
        return Some(false);
    }
    let short_frame = bits.read(1)? == 1;
    if bits.read(1)? == 1 {
        bits.read(14)?;
    }
    let version3 = bits.read(1)? == 1 && bits.read(1)? == 1;
    Some(!short_frame && !version3)
}

/// A most-significant-bit-first reader.
struct Bits<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Bits<'_> {
    fn read(&mut self, count: u32) -> Option<u32> {
        let mut value: u32 = 0;
        for _ in 0..count {
            let byte = *self.bytes.get(self.at / 8)?;
            let bit = byte.checked_shl(u32::try_from(self.at % 8).ok()?)? >> 7;
            value = value.checked_mul(2)?.checked_add(u32::from(bit))?;
            self.at = self.at.checked_add(1)?;
        }
        Some(value)
    }
}

/// Mirrors the `symphonia` ALAC reader and decoder: an `alac` box (version and flags 0) holding a
/// 24- or 48-byte magic cookie of version 0, at most 64 Ki frames per packet and a bit depth up to
/// 32. More than two channels decode but cannot be played.
fn alac(config: &[u8]) -> Option<AssetFormat> {
    let cookie = config.get(4..)?;
    let decodable = config.get(..4)? == [0, 0, 0, 0]
        && matches!(cookie.len(), 24 | 48)
        && u32_be(cookie, 0)? <= 4096 * 16
        && *cookie.get(4)? == 0
        && *cookie.get(5)? <= 32
        && matches!(cookie.get(9)?, 1 | 2);
    decodable.then_some(AssetFormat::Mp4Alac)
}

/// Reads the top-level boxes of the ISO BMFF file in `reader`, except media data and free space,
/// into one buffer of at most `limit` bytes. `sniff` needs the `moov` box, which encoders often
/// write after `mdat`, far past any fixed-size head of the file.
pub fn read_iso_bmff_head<R: Read + Seek>(reader: &mut R, limit: u64) -> io::Result<Vec<u8>> {
    let mut head = Vec::new();
    loop {
        let mut header = [0u8; 16];
        if !read_or_eof(reader, &mut header[..8])? {
            break;
        }
        let mut len: usize = 8;
        let size = match u32::from_be_bytes([header[0], header[1], header[2], header[3]]) {
            // The box runs to the end of the file.
            0 => None,
            1 => {
                if !read_or_eof(reader, &mut header[8..])? {
                    break;
                }
                len = 16;
                let [.., a, b, c, d, e, f, g, h] = header;
                Some(u64::from_be_bytes([a, b, c, d, e, f, g, h]))
            }
            size => Some(u64::from(size)),
        };
        let body = match size.map(|size| size.checked_sub(len as u64)) {
            Some(None) => break,
            Some(Some(body)) => Some(body),
            None => None,
        };
        if matches!(&header[4..8], b"mdat" | b"free" | b"skip" | b"wide") {
            match body.map(i64::try_from) {
                Some(Ok(body)) => reader.seek(SeekFrom::Current(body))?,
                _ => break,
            };
            continue;
        }
        let room = limit.saturating_sub(head.len().saturating_add(len) as u64);
        if body.is_some_and(|body| body > room) {
            break;
        }
        head.extend_from_slice(&header[..len]);
        let wanted = body.unwrap_or(room);
        let read = reader.by_ref().take(wanted).read_to_end(&mut head)?;
        if body.is_none() || (read as u64) < wanted {
            break;
        }
    }
    Ok(head)
}

/// Fills `buf`; `false` if the reader ends first.
fn read_or_eof(reader: &mut impl Read, buf: &mut [u8]) -> io::Result<bool> {
    match reader.read_exact(buf) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(error),
    }
}

fn ogg(bytes: &[u8]) -> AssetFormat {
    let packet = bytes
        .get(26)
        .and_then(|&n| 27usize.checked_add(usize::from(n)))
        .and_then(|start| bytes.get(start..))
        .unwrap_or(&[]);
    if packet.starts_with(b"\x01vorbis") {
        AssetFormat::OggVorbis
    } else if packet.starts_with(b"OpusHead") {
        AssetFormat::OggOpus
    } else if packet.starts_with(b"\x7FFLAC") {
        AssetFormat::OggFlac
    } else {
        AssetFormat::OggOther
    }
}

/// Rules 10, 12 and 13: FLAC, AAC ADTS and MPEG Layer III.
fn audio_stream(bytes: &[u8]) -> Option<AssetFormat> {
    if bytes.starts_with(b"fLaC") {
        return Some(AssetFormat::Flac);
    }
    let (&b0, &b1) = (bytes.first()?, bytes.get(1)?);
    if b0 != 0xFF {
        return None;
    }
    if b1 & 0xF6 == 0xF0 {
        return Some(adts(bytes));
    }
    let b2 = *bytes.get(2)?;
    let mp3 = b1 & 0xE0 == 0xE0
        && (b1 >> 3) & 3 != 1
        && (b1 >> 1) & 3 == 1
        && (1..=14).contains(&(b2 >> 4))
        && (b2 >> 2) & 3 != 3;
    mp3.then_some(AssetFormat::Mp3)
}

/// Mirrors the `symphonia` ADTS reader, which decodes every stream as AAC-LC: profile LC, a sample
/// rate index below 13, one or two channels (configuration 0 needs a program config element) and
/// one raw data block per frame.
fn adts(bytes: &[u8]) -> AssetFormat {
    let decodable = (|| {
        let (b2, b3, b6) = (*bytes.get(2)?, *bytes.get(3)?, *bytes.get(6)?);
        let channels = (b2 & 1) << 2 | b3 >> 6;
        Some(b2 >> 6 == 1 && (b2 >> 2) & 0xF < 13 && matches!(channels, 1 | 2) && b6 & 3 == 0)
    })();
    if decodable == Some(true) {
        AssetFormat::AdtsAac
    } else {
        AssetFormat::AacUnsupported
    }
}

/// Skips the ID3v2 tag and its padding, then sniffs the audio stream after it.
fn id3(bytes: &[u8]) -> AssetFormat {
    let rest = (|| {
        let size = bytes.get(6..10)?.iter().fold(0usize, |acc, &b| {
            acc.wrapping_shl(7) | usize::from(b & 0x7F)
        });
        let mut skip = size.checked_add(10)?;
        if bytes.get(5)? & 0x10 != 0 {
            skip = skip.checked_add(10)?;
        }
        let rest = bytes.get(skip..)?;
        let start = rest.iter().position(|&b| b != 0)?;
        rest.get(start..)
    })();
    rest.and_then(audio_stream).unwrap_or(AssetFormat::Unknown)
}

fn font(bytes: &[u8]) -> Option<AssetFormat> {
    Some(match bytes.get(..4)? {
        [0x00, 0x01, 0x00, 0x00] | b"true" => AssetFormat::TrueType,
        b"OTTO" => AssetFormat::OpenTypeCff,
        b"ttcf" => AssetFormat::FontCollection,
        b"wOFF" => AssetFormat::Woff,
        b"wOF2" => AssetFormat::Woff2,
        _ => return None,
    })
}
