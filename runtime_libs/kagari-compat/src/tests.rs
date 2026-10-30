#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use std::io::Cursor;

use kagari_bytecode::AssetKind;

use crate::{AssetFormat, RuntimeProfile, read_iso_bmff_head, sniff, supports, supports_kind};

/// A JFIF header followed by a frame header with `sof` and `precision`.
fn jpeg(sof: u8, precision: u8) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    bytes.extend_from_slice(b"JFIF\0");
    bytes.extend_from_slice(&[0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00]);
    bytes.extend_from_slice(&[0xFF, sof, 0x00, 0x11, precision, 0x00, 0x10, 0x00, 0x10]);
    bytes
}

#[test]
fn jpeg_frames() {
    let image = |bytes: Vec<u8>| sniff(AssetKind::Image, &bytes);
    assert_eq!(image(jpeg(0xC0, 8)), AssetFormat::JpegBaseline);
    assert_eq!(image(jpeg(0xC2, 8)), AssetFormat::JpegProgressive);
    assert_eq!(image(jpeg(0xC9, 8)), AssetFormat::JpegUnsupported);
    assert_eq!(image(jpeg(0xC1, 0x0C)), AssetFormat::JpegUnsupported);
}

fn webp_vp8x(flags: u8) -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(&[flags, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

#[test]
fn webp_extended() {
    assert_eq!(
        sniff(AssetKind::Image, &webp_vp8x(0x02)),
        AssetFormat::WebpAnimated
    );
    assert_eq!(
        sniff(AssetKind::Image, &webp_vp8x(0x10)),
        AssetFormat::WebpExtended
    );
}

/// A BMP file header and the start of a DIB header of `size` bytes.
fn bmp(size: u32, height: i32, bits: u16, compression: u32) -> Vec<u8> {
    let mut bytes = b"BM\0\0\0\0\0\0\0\0\0\0\0\0".to_vec();
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(&2i32.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(&compression.to_le_bytes());
    bytes
}

#[test]
fn bmp_layouts() {
    let image = |bytes: Vec<u8>| sniff(AssetKind::Image, &bytes);
    assert_eq!(image(bmp(40, 2, 24, 0)), AssetFormat::Bmp);
    assert_eq!(image(bmp(124, 2, 32, 3)), AssetFormat::Bmp);
    assert_eq!(image(bmp(40, 2, 8, 1)), AssetFormat::Bmp);
    assert_eq!(image(bmp(40, -2, 24, 0)), AssetFormat::Bmp);
    // BITMAPCOREHEADER: u16 width, height, planes, bit count.
    let core = b"BM\0\0\0\0\0\0\0\0\0\0\0\0\x0C\0\0\0\x02\0\x02\0\x01\0\x18\0".to_vec();
    assert_eq!(image(core), AssetFormat::Bmp);

    assert_eq!(image(bmp(40, 2, 24, 4)), AssetFormat::BmpUnsupported);
    assert_eq!(image(bmp(40, 2, 24, 5)), AssetFormat::BmpUnsupported);
    assert_eq!(image(bmp(40, 2, 32, 11)), AssetFormat::BmpUnsupported);
    assert_eq!(image(bmp(40, -2, 8, 1)), AssetFormat::BmpUnsupported);
    assert_eq!(image(bmp(40, 2, 24, 1)), AssetFormat::BmpUnsupported);
    assert_eq!(image(bmp(64, 2, 24, 0)), AssetFormat::BmpUnsupported);
    assert_eq!(image(b"BM\0\0".to_vec()), AssetFormat::BmpUnsupported);
}

#[test]
fn jxl_headers() {
    let still = include_bytes!("../tests/fixtures/still.jxl");
    let animated = include_bytes!("../tests/fixtures/animated.jxl");
    assert_eq!(sniff(AssetKind::Image, still), AssetFormat::Jxl);
    assert_eq!(
        sniff(AssetKind::Image, animated),
        AssetFormat::JxlUnsupported
    );
    assert_eq!(
        sniff(AssetKind::Image, &still[..24]),
        AssetFormat::JxlUnsupported
    );
    assert_eq!(
        sniff(AssetKind::Image, &[0xFF, 0x0A, 0x00, 0x00]),
        AssetFormat::JxlUnsupported
    );
}

/// A WAV whose `fmt ` chunk has `size` bytes and starts with `tag`; for `0xFFFE`, the
/// sub-format at data+24 is `sub`.
fn wav(tag: u16, size: u32, sub: u16) -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
    bytes.extend_from_slice(b"LIST");
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&size.to_le_bytes());
    let mut data = vec![0u8; usize::try_from(size).unwrap()];
    data[..2].copy_from_slice(&tag.to_le_bytes());
    if size >= 26 {
        data[24..26].copy_from_slice(&sub.to_le_bytes());
    }
    bytes.extend_from_slice(&data);
    bytes
}

#[test]
fn wav_encodings() {
    let audio = |bytes: Vec<u8>| sniff(AssetKind::Audio, &bytes);
    assert_eq!(audio(wav(3, 16, 0)), AssetFormat::WavFloat);
    assert_eq!(audio(wav(0xFFFE, 40, 1)), AssetFormat::WavPcm);
    assert_eq!(audio(wav(2, 16, 0)), AssetFormat::WavOther);
}

#[test]
fn template_wav_is_pcm() {
    assert_eq!(
        sniff(
            AssetKind::Voice,
            include_bytes!(
                "../../../tooling/kagari-cli/template/assets/characters/character1/voice/0001.wav"
            )
        ),
        AssetFormat::WavPcm
    );
}

/// The first Ogg page with one segment holding `packet`.
fn ogg(packet: &[u8]) -> Vec<u8> {
    let mut bytes = b"OggS\x00\x02".to_vec();
    bytes.extend_from_slice(&[0; 20]);
    bytes.extend_from_slice(&[0x01, 0x13]);
    bytes.extend_from_slice(packet);
    bytes
}

#[test]
fn ogg_codecs() {
    let opus = ogg(b"OpusHead\x01\x02\x38\x01\x80\xBB\x00\x00\x00\x00\x00");
    assert_eq!(sniff(AssetKind::Audio, &opus), AssetFormat::OggOpus);
    let vorbis = ogg(b"\x01vorbis\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00");
    assert_eq!(sniff(AssetKind::Audio, &vorbis), AssetFormat::OggVorbis);
    let flac = ogg(b"\x7FFLAC\x01\x00\x00\x01fLaC\x00\x00\x00\x22");
    assert_eq!(sniff(AssetKind::Audio, &flac), AssetFormat::OggFlac);
}

#[test]
fn mpeg_streams() {
    assert_eq!(
        sniff(
            AssetKind::Audio,
            b"ID3\x03\x00\x00\x00\x00\x00\x00\xFF\xFB\x90\x64"
        ),
        AssetFormat::Mp3
    );
}

/// An ADTS header: `b2` holds profile, sample rate index and the high channel bit, `b3` the low
/// channel bits, `b6` the raw data block count.
fn adts(b2: u8, b3: u8, b6: u8) -> Vec<u8> {
    vec![0xFF, 0xF1, b2, b3, 0x00, 0x1F, b6]
}

#[test]
fn adts_profiles() {
    let audio = |bytes: Vec<u8>| sniff(AssetKind::Audio, &bytes);
    // LC, 44.1 kHz, stereo, one block.
    assert_eq!(audio(adts(0x50, 0x80, 0xFC)), AssetFormat::AdtsAac);
    // LC, 48 kHz, mono.
    assert_eq!(audio(adts(0x4C, 0x40, 0xFC)), AssetFormat::AdtsAac);
    // Main profile.
    assert_eq!(audio(adts(0x10, 0x80, 0xFC)), AssetFormat::AacUnsupported);
    // 5.1 channels.
    assert_eq!(audio(adts(0x51, 0x80, 0xFC)), AssetFormat::AacUnsupported);
    // Channel configuration 0 (program config element).
    assert_eq!(audio(adts(0x50, 0x00, 0xFC)), AssetFormat::AacUnsupported);
    // Two raw data blocks per frame.
    assert_eq!(audio(adts(0x50, 0x80, 0xFD)), AssetFormat::AacUnsupported);
    // Truncated header.
    assert_eq!(
        audio(vec![0xFF, 0xF1, 0x50, 0x80]),
        AssetFormat::AacUnsupported
    );
}

/// An ISO BMFF box.
fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut bytes = u32::try_from(body.len() + 8)
        .unwrap()
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(body);
    bytes
}

/// An M4A file whose one track has `handler` and a version 0 sample entry `entry` with `config`
/// as its child; `mdat` holds `media` bytes and comes after `moov` if `moov_first`.
fn m4a(
    handler: &[u8; 4],
    entry: &[u8; 4],
    config: &[u8],
    media: usize,
    moov_first: bool,
) -> Vec<u8> {
    let mut fields = vec![0u8; 28];
    fields.extend_from_slice(config);
    let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
    stsd.extend(mp4_box(entry, &fields));
    let minf = mp4_box(b"minf", &mp4_box(b"stbl", &mp4_box(b"stsd", &stsd)));
    let mut hdlr = vec![0u8; 8];
    hdlr.extend_from_slice(handler);
    hdlr.extend_from_slice(&[0; 13]);
    let mdia = mp4_box(b"mdia", &[mp4_box(b"hdlr", &hdlr), minf].concat());
    let moov = mp4_box(b"moov", &mp4_box(b"trak", &mdia));
    let ftyp = mp4_box(b"ftyp", b"M4A \0\0\x02\0M4A isomiso2");
    let mdat = mp4_box(b"mdat", &vec![0xAA; media]);
    if moov_first {
        [ftyp, moov, mdat].concat()
    } else {
        [ftyp, mdat, moov].concat()
    }
}

/// An MPEG-4 descriptor with its length in the four-byte form encoders write.
fn descriptor(tag: u8, body: &[u8]) -> Vec<u8> {
    let len = u8::try_from(body.len()).unwrap();
    [&[tag, 0x80, 0x80, 0x80, len][..], body].concat()
}

/// An `esds` box with object type indication `oti` and AudioSpecificConfig `asc`.
fn esds(oti: u8, asc: &[u8]) -> Vec<u8> {
    let mut config = vec![oti, 0x15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    config.extend(descriptor(5, asc));
    let mut es = vec![0, 1, 0];
    es.extend(descriptor(4, &config));
    es.extend(descriptor(6, &[2]));
    mp4_box(b"esds", &[&[0, 0, 0, 0][..], &descriptor(3, &es)].concat())
}

/// An `alac` box with a 24-byte magic cookie for `channels` channels of 16 bits.
fn alac(channels: u8) -> Vec<u8> {
    let mut cookie = vec![
        0, 0, 0, 0, 0x00, 0x00, 0x10, 0x00, 0, 16, 40, 10, 14, channels,
    ];
    cookie.extend_from_slice(&[0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xAC, 0x44]);
    mp4_box(b"alac", &cookie)
}

#[test]
fn mp4_codecs() {
    let audio = |entry: &[u8; 4], config: Vec<u8>| {
        sniff(AssetKind::Audio, &m4a(b"soun", entry, &config, 16, true))
    };
    // AAC-LC, 44.1 kHz, stereo.
    assert_eq!(
        audio(b"mp4a", esds(0x40, &[0x12, 0x10])),
        AssetFormat::Mp4Aac
    );
    // MPEG-2 AAC-LC object type, mono.
    assert_eq!(
        audio(b"mp4a", esds(0x67, &[0x12, 0x08])),
        AssetFormat::Mp4Aac
    );
    // HE-AAC (explicit SBR, object type 5).
    assert_eq!(
        audio(b"mp4a", esds(0x40, &[0x2B, 0x11, 0x88, 0x00])),
        AssetFormat::AacUnsupported
    );
    // AAC-LC 5.1.
    assert_eq!(
        audio(b"mp4a", esds(0x40, &[0x12, 0x30])),
        AssetFormat::AacUnsupported
    );
    // 960-sample frames.
    assert_eq!(
        audio(b"mp4a", esds(0x40, &[0x12, 0x14])),
        AssetFormat::AacUnsupported
    );
    // MP3 in MP4.
    assert_eq!(audio(b"mp4a", esds(0x6B, &[])), AssetFormat::Mp4Other);
    assert_eq!(audio(b"alac", alac(2)), AssetFormat::Mp4Alac);
    assert_eq!(audio(b"alac", alac(6)), AssetFormat::Mp4Other);
    assert_eq!(
        audio(b"Opus", mp4_box(b"dOps", &[0; 11])),
        AssetFormat::Mp4Other
    );
    // No sound track.
    let video = m4a(b"vide", b"mp4a", &esds(0x40, &[0x12, 0x10]), 16, true);
    assert_eq!(sniff(AssetKind::Audio, &video), AssetFormat::Mp4Other);
}

#[test]
fn mp4_moov_after_media() {
    let file = m4a(b"soun", b"mp4a", &esds(0x40, &[0x12, 0x10]), 2 << 20, false);
    assert_eq!(
        sniff(AssetKind::Audio, &file[..1 << 20]),
        AssetFormat::Mp4Other
    );
    let head = read_iso_bmff_head(&mut Cursor::new(&file), 1 << 20).unwrap();
    assert!(head.len() < 1024);
    assert_eq!(sniff(AssetKind::Audio, &head), AssetFormat::Mp4Aac);
    // A `moov` larger than the limit is not read.
    let head = read_iso_bmff_head(&mut Cursor::new(&file), 64).unwrap();
    assert_eq!(sniff(AssetKind::Audio, &head), AssetFormat::Mp4Other);
}

/// Files ffmpeg wrote (0.1 s, 16 kHz mono; the M4A files with `moov` after `mdat`). The desktop
/// runtime's tests decode the same files.
#[test]
fn encoder_output_is_supported() {
    let fixtures: [(&[u8], AssetFormat); 4] = [
        (
            include_bytes!("../tests/fixtures/flac.oga"),
            AssetFormat::OggFlac,
        ),
        (
            include_bytes!("../tests/fixtures/aac-lc.aac"),
            AssetFormat::AdtsAac,
        ),
        (
            include_bytes!("../tests/fixtures/aac-lc.m4a"),
            AssetFormat::Mp4Aac,
        ),
        (
            include_bytes!("../tests/fixtures/alac.m4a"),
            AssetFormat::Mp4Alac,
        ),
    ];
    for (bytes, expected) in fixtures {
        let head = if bytes.get(4..8) == Some(b"ftyp") {
            read_iso_bmff_head(&mut Cursor::new(bytes), 1 << 20).unwrap()
        } else {
            bytes.to_vec()
        };
        assert_eq!(sniff(AssetKind::Voice, &head), expected);
        assert!(supports(
            RuntimeProfile::NativeV0,
            AssetKind::Voice,
            expected
        ));
    }
}

#[test]
fn fonts() {
    assert_eq!(sniff(AssetKind::Font, b"OTTO"), AssetFormat::OpenTypeCff);
    assert_eq!(sniff(AssetKind::Font, b"wOF2"), AssetFormat::Woff2);
}

#[test]
fn native_v0_support() {
    let p = RuntimeProfile::NativeV0;
    assert!(!supports(p, AssetKind::Image, AssetFormat::WebpAnimated));
    assert!(supports(p, AssetKind::Image, AssetFormat::Bmp));
    assert!(!supports(p, AssetKind::Image, AssetFormat::BmpUnsupported));
    assert!(supports(p, AssetKind::Image, AssetFormat::Jxl));
    assert!(!supports(p, AssetKind::Image, AssetFormat::JxlUnsupported));
    assert!(supports(p, AssetKind::Voice, AssetFormat::Mp3));
    assert!(supports(p, AssetKind::Audio, AssetFormat::OggFlac));
    assert!(!supports(p, AssetKind::Audio, AssetFormat::OggOpus));
    assert!(supports(p, AssetKind::Voice, AssetFormat::AdtsAac));
    assert!(supports(p, AssetKind::Audio, AssetFormat::Mp4Aac));
    assert!(supports(p, AssetKind::Audio, AssetFormat::Mp4Alac));
    assert!(!supports(p, AssetKind::Audio, AssetFormat::AacUnsupported));
    assert!(!supports(p, AssetKind::Audio, AssetFormat::Mp4Other));
    assert!(supports(p, AssetKind::Font, AssetFormat::FontCollection));
    assert!(!supports(p, AssetKind::Video, AssetFormat::Unknown));
    assert!(supports_kind(p, AssetKind::Shader));
    assert!(!supports_kind(p, AssetKind::Video));
    assert!(supports(p, AssetKind::Shader, AssetFormat::Wgsl));
    assert!(!supports(p, AssetKind::Shader, AssetFormat::Unknown));
}

#[test]
fn wgsl_sniff() {
    let src = b"fn effect(uv: vec2<f32>) -> vec4<f32> { return vec4(1.0); }";
    assert_eq!(sniff(AssetKind::Shader, src), AssetFormat::Wgsl);
    assert_eq!(
        sniff(AssetKind::Shader, &[0xFF, 0x00]),
        AssetFormat::Unknown
    );
}
