#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use crate::{Accel, AudioDecoder, MediaInfo, VideoDecoder, probe};

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../kagari-compat/tests/fixtures"
);

fn fixture(name: &str) -> Arc<[u8]> {
    std::fs::read(format!("{FIXTURES}/{name}"))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .into()
}

/// The accepted encodings, with whether each has sound.
const ACCEPTED: [(&str, bool); 3] = [
    ("vp9-opus.webm", true),
    ("av1.webm", false),
    ("h264-aac.mp4", true),
];

#[test]
fn decoders_are_send() {
    fn send<T: Send>() {}
    send::<VideoDecoder>();
    send::<AudioDecoder>();
}

#[test]
fn supported_encodings_probe() {
    for (name, has_audio) in ACCEPTED {
        let MediaInfo {
            width,
            height,
            duration_ms,
            has_audio: audio,
        } = probe(fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!((width, height), (64, 64), "{name}");
        assert!(
            (450..=550).contains(&duration_ms),
            "{name}: {duration_ms} ms"
        );
        assert_eq!(audio, has_audio, "{name}");
    }
}

#[test]
fn probe_rejects_garbage() {
    assert!(probe(Arc::from(&b"not a movie"[..])).is_err());
}

#[test]
fn supported_encodings_decode() {
    for (name, _) in ACCEPTED {
        let mut dec = VideoDecoder::open(fixture(name), Accel::Software)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(dec.backend(), "software");
        let at = |dec: &mut VideoDecoder, ms: u64| {
            dec.frame_at(ms)
                .unwrap_or_else(|e| panic!("{name} at {ms}: {e}"))
                .unwrap_or_else(|| panic!("{name} at {ms}: no frame"))
        };
        let first = at(&mut dec, 0);
        assert_eq!(first.pts_ms, 0, "{name}");
        assert_eq!((first.width, first.height), (64, 64), "{name}");
        assert_eq!(first.rgba.len(), 64 * 64 * 4, "{name}");
        assert!(
            first.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255),
            "{name}"
        );
        assert!(
            Arc::ptr_eq(&first, &at(&mut dec, 50)),
            "{name}: same frame reused"
        );
        assert_eq!(at(&mut dec, 250).pts_ms, 200, "{name}");
        assert_eq!(at(&mut dec, 400).pts_ms, 400, "{name}");
        assert_eq!(at(&mut dec, 0).pts_ms, 0, "{name}: seek back");
        assert_eq!(at(&mut dec, 10_000).pts_ms, 400, "{name}: past the end");
    }
}

#[test]
fn movie_audio_decodes_and_seeks() {
    let mut dec = AudioDecoder::open(fixture("vp9-opus.webm"))
        .unwrap()
        .unwrap();
    assert_eq!(dec.sample_rate(), 48_000);
    let total = dec.num_frames();
    assert!((21_600..=26_400).contains(&total), "{total} frames");
    let mut peak = [0.0f32; 2];
    let mut decoded = 0usize;
    while decoded < total {
        let chunk = dec.decode().unwrap();
        assert!(!chunk.is_empty());
        for [l, r] in &chunk {
            peak = [peak[0].max(l.abs()), peak[1].max(r.abs())];
        }
        decoded += chunk.len();
    }
    // lavfi's sine has amplitude 1/8; mono is upmixed to both channels
    assert!(peak[0] > 0.05 && peak[1] > 0.05, "peak {peak:?}");
    // past the end: silence, never empty
    for _ in 0..4 {
        assert!(!dec.decode().unwrap().is_empty());
    }
    assert_eq!(dec.seek(24_000).unwrap(), 24_000);
    assert_eq!(dec.seek(4_800).unwrap(), 4_800);
    assert!(!dec.decode().unwrap().is_empty());
}

#[test]
fn silent_movie_has_no_audio_decoder() {
    assert!(AudioDecoder::open(fixture("av1.webm")).unwrap().is_none());
}
