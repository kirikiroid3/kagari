//! Asset formats, their descriptions, and which profile supports which.

use kagari_bytecode::AssetKind;

use crate::RuntimeProfile;

/// The format of an asset file, as told by its content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetFormat {
    Png,
    JpegBaseline,
    JpegProgressive,
    /// Arithmetic, lossless, hierarchical or 12-bit coding.
    JpegUnsupported,
    WebpLossy,
    WebpLossless,
    /// `VP8X` without the animation flag.
    WebpExtended,
    WebpAnimated,
    Gif,
    Bmp,
    Avif,
    Jxl,
    OggVorbis,
    OggOpus,
    OggFlac,
    OggOther,
    Flac,
    WavPcm,
    WavFloat,
    WavOther,
    Mp3,
    Aac,
    TrueType,
    OpenTypeCff,
    FontCollection,
    Woff,
    Woff2,
    Wgsl,
    Unknown,
}

/// The name of `format` used in build errors.
pub fn describe(format: AssetFormat) -> &'static str {
    match format {
        AssetFormat::Png => "PNG",
        AssetFormat::JpegBaseline => "baseline JPEG",
        AssetFormat::JpegProgressive => "progressive JPEG",
        AssetFormat::JpegUnsupported => {
            "JPEG with arithmetic, lossless, hierarchical or 12-bit coding"
        }
        AssetFormat::WebpLossy => "lossy WebP",
        AssetFormat::WebpLossless => "lossless WebP",
        AssetFormat::WebpExtended => "WebP with alpha or metadata",
        AssetFormat::WebpAnimated => "animated WebP",
        AssetFormat::Gif => "GIF",
        AssetFormat::Bmp => "BMP",
        AssetFormat::Avif => "AVIF",
        AssetFormat::Jxl => "JPEG XL",
        AssetFormat::OggVorbis => "Ogg Vorbis",
        AssetFormat::OggOpus => "Ogg Opus",
        AssetFormat::OggFlac => "Ogg FLAC",
        AssetFormat::OggOther => "Ogg with an unknown codec",
        AssetFormat::Flac => "FLAC",
        AssetFormat::WavPcm => "WAV (PCM)",
        AssetFormat::WavFloat => "WAV (IEEE float)",
        AssetFormat::WavOther => "WAV (compressed or unknown encoding)",
        AssetFormat::Mp3 => "MP3",
        AssetFormat::Aac => "AAC/M4A",
        AssetFormat::TrueType => "TrueType font",
        AssetFormat::OpenTypeCff => "OpenType (CFF) font",
        AssetFormat::FontCollection => "TrueType/OpenType collection",
        AssetFormat::Woff => "WOFF font",
        AssetFormat::Woff2 => "WOFF2 font",
        AssetFormat::Wgsl => "WGSL shader",
        AssetFormat::Unknown => "unrecognized data",
    }
}

/// Whether `profile` supports assets of `kind` at all.
pub fn supports_kind(profile: RuntimeProfile, kind: AssetKind) -> bool {
    match profile {
        RuntimeProfile::NativeV0 => !matches!(kind, AssetKind::Video | AssetKind::Shader),
    }
}

/// Whether `profile` decodes an asset of `kind` stored as `format`.
pub fn supports(profile: RuntimeProfile, kind: AssetKind, format: AssetFormat) -> bool {
    use AssetFormat as F;
    match profile {
        RuntimeProfile::NativeV0 => match kind {
            AssetKind::Image => matches!(
                format,
                F::Png
                    | F::JpegBaseline
                    | F::JpegProgressive
                    | F::WebpLossy
                    | F::WebpLossless
                    | F::WebpExtended
            ),
            AssetKind::Audio | AssetKind::Voice => matches!(
                format,
                F::OggVorbis | F::Flac | F::WavPcm | F::WavFloat | F::Mp3
            ),
            AssetKind::Font => matches!(format, F::TrueType | F::OpenTypeCff | F::FontCollection),
            AssetKind::Video | AssetKind::Shader => false,
        },
    }
}
