#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use kagari_bytecode::AssetKind;

use crate::{AssetFormat, RuntimeProfile, sniff, supports, supports_kind};

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
    assert_eq!(
        sniff(AssetKind::Audio, &[0xFF, 0xF1, 0x50, 0x80]),
        AssetFormat::Aac
    );
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
