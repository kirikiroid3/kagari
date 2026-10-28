//! Format detection by content. Every read is bounds-checked: truncated data falls back to the
//! rule's "unrecognized" answer instead of failing.

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
        return AssetFormat::Bmp;
    }
    if bytes.starts_with(&[0xFF, 0x0A])
        || bytes.starts_with(&[
            0x00, 0x00, 0x00, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
        ])
    {
        return AssetFormat::Jxl;
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
        AssetKind::Audio | AssetKind::Voice => AssetFormat::Aac,
        _ => AssetFormat::Unknown,
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
        return Some(AssetFormat::Aac);
    }
    let b2 = *bytes.get(2)?;
    let mp3 = b1 & 0xE0 == 0xE0
        && (b1 >> 3) & 3 != 1
        && (b1 >> 1) & 3 == 1
        && (1..=14).contains(&(b2 >> 4))
        && (b2 >> 2) & 3 != 3;
    mp3.then_some(AssetFormat::Mp3)
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
