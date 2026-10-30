#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use kagari_engine::Key;
use winit::keyboard::KeyCode;

use crate::audio::{Span, clamp_span};
use crate::exit::Failure;
use crate::headless::script::{Command, parse};
use crate::input::{key_code, map_key};
use crate::sounds::SoundCache;
use crate::store::{Store, sanitize_title};

#[test]
fn every_key_maps_back() {
    assert_eq!(Key::ALL.len(), 64);
    for k in Key::ALL {
        assert_eq!(map_key(key_code(k)), Some(k), "{k:?}");
    }
    assert_eq!(map_key(KeyCode::ShiftRight), Some(Key::Shift));
    assert_eq!(map_key(KeyCode::NumpadEnter), Some(Key::Enter));
}

#[test]
fn script_parses() {
    let script = parse("wait 100\nkey Enter\n  # note\n\nclick 640 360\ncapture title").unwrap();
    assert_eq!(
        script,
        vec![
            (1, Command::Wait(100)),
            (2, Command::Key(Key::Enter)),
            (5, Command::Click(640.0, 360.0)),
            (6, Command::Capture("title".to_owned())),
        ]
    );
}

#[test]
fn script_rejects_unknown_command() {
    let err = parse("wait 1\njump 3").unwrap_err();
    assert_eq!(err.to_string(), "script:2: unknown command `jump`");
    assert_eq!(Failure::from(err).code, 5);
}

#[test]
fn script_errors() {
    let message = |text: &str| parse(text).unwrap_err().to_string();
    assert_eq!(message("key Foo"), "script:1: unknown key `Foo`");
    assert_eq!(
        message("wait -5"),
        "script:1: `-5` is not a valid duration in ms"
    );
    assert_eq!(message("wait"), "script:1: `wait` takes 1 argument(s)");
    assert_eq!(message("capture ../x"), "script:1: bad capture name `../x`");
}

#[test]
fn sanitize_title_replaces_reserved() {
    assert_eq!(sanitize_title(" a/b: c? "), "a_b_ c_");
    assert_eq!(
        sanitize_title("ティラノスクリプト解説"),
        "ティラノスクリプト解説"
    );
    assert_eq!(sanitize_title("..."), "Kagari Game");
}

#[test]
fn store_writes_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::fixed(dir.path().to_path_buf());
    store.write_store("save-quick.kisave", b"x").unwrap();
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["save-quick.kisave"]);
    assert_eq!(
        std::fs::read(dir.path().join("save-quick.kisave")).unwrap(),
        b"x"
    );
    let err = store.write_store("../evil", b"").unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn sound_spans_stay_inside_the_audio() {
    // 2 s at 1000 Hz: one frame per ms.
    let span = |start, end, looped| clamp_span("a.ogg", 2000, 1000, start, end, looped);
    let plain = |start, end| Span {
        start,
        end,
        loop_region: None,
    };
    assert_eq!(span(100, Some(1500), None), plain(100, Some(1500)));
    assert_eq!(span(0, Some(2001), None), plain(0, Some(2000)));
    assert_eq!(span(1500, Some(1000), None), plain(1500, None));
    assert_eq!(span(3000, None, None), plain(2000, None));
    assert_eq!(span(3000, Some(3500), None), plain(2000, Some(2000)));
    let looped = |start, ls, le| Span {
        start,
        end: None,
        loop_region: Some((ls, le)),
    };
    assert_eq!(
        span(100, Some(50), Some((500, Some(1500)))),
        looped(100, 500, 1500)
    );
    assert_eq!(span(0, None, Some((500, None))), looped(0, 500, 2000));
    assert_eq!(span(0, None, Some((500, Some(2500)))), looped(0, 500, 2000));
    assert_eq!(span(0, None, Some((500, Some(500)))), plain(0, None));
    assert_eq!(span(0, None, Some((800, Some(500)))), plain(0, None));
    assert_eq!(span(0, None, Some((2000, None))), plain(0, None));
    assert_eq!(span(0, None, Some((2500, Some(3000)))), plain(0, None));
    // 44.1 kHz rounds to the nearest frame.
    assert_eq!(clamp_span("a.ogg", 44_100, 44_100, 1, None, None).start, 44);
}

/// A 16-bit stereo PCM WAV at 44.1 kHz with a `LIST` chunk before `data`, as ffmpeg writes it;
/// the header declares `riff_size` and `data_size`, the data holds `frames` frames.
fn wav(riff_size: u32, data_size: u32, frames: usize) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&riff_size.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    for field in [1u16, 2] {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(&44_100u32.to_le_bytes());
    bytes.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    for field in [4u16, 16] {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(b"LIST\x0e\0\0\0INFOISFT\x02\0\0\0x\0");
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    bytes.resize(bytes.len() + frames * 4, 0);
    bytes
}

#[test]
fn wav_sizes_past_the_end_mean_until_the_end() {
    let frames = |bytes: Vec<u8>| {
        let mut cache = SoundCache::default();
        cache.decode("a.wav", &bytes).map(|data| data.frames.len())
    };
    let exact = wav(0, 0, 1000);
    let (riff, data) = (exact.len() as u32 - 8, 4000);
    assert_eq!(frames(wav(riff, data, 1000)), Some(1000));
    // Streaming encoders (ffmpeg to a pipe, TTS services) write the placeholder 0xFFFFFFFF.
    assert_eq!(frames(wav(u32::MAX, u32::MAX, 1000)), Some(1000));
    assert_eq!(frames(wav(riff, u32::MAX, 1000)), Some(1000));
    // A truncated file plays what it has.
    assert_eq!(frames(wav(riff + 4000, data + 4000, 1000)), Some(1000));
    // A trailing partial frame is dropped.
    let mut odd = wav(u32::MAX, u32::MAX, 1000);
    odd.push(0);
    assert_eq!(frames(odd), Some(1000));
}

/// The encoder output `kagari-compat` accepts (0.1 s at 16 kHz) decodes. AAC keeps the encoder's
/// priming and padding: symphonia applies neither the MP4 edit list nor gapless metadata.
#[test]
fn supported_encodings_decode() {
    let fixtures: [(&str, &[u8], usize); 4] = [
        (
            "flac.oga",
            include_bytes!("../../../runtime_libs/kagari-compat/tests/fixtures/flac.oga"),
            1600,
        ),
        (
            "aac-lc.aac",
            include_bytes!("../../../runtime_libs/kagari-compat/tests/fixtures/aac-lc.aac"),
            3072,
        ),
        (
            "aac-lc.m4a",
            include_bytes!("../../../runtime_libs/kagari-compat/tests/fixtures/aac-lc.m4a"),
            3072,
        ),
        (
            "alac.m4a",
            include_bytes!("../../../runtime_libs/kagari-compat/tests/fixtures/alac.m4a"),
            1600,
        ),
    ];
    for (path, bytes, frames) in fixtures {
        let mut cache = SoundCache::default();
        let data = cache
            .decode(path, bytes)
            .unwrap_or_else(|| panic!("{path} decodes"));
        assert_eq!(
            (path, data.sample_rate, data.frames.len()),
            (path, 16_000, frames)
        );
    }
}
