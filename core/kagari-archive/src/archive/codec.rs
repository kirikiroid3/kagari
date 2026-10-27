//! Block codecs. Adding one takes a variant, an id and an arm in each function below.

/// How a block's raw bytes are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Codec {
    /// The raw bytes, unchanged.
    Store,
    /// One LZ4 block-format stream of the whole raw block.
    Lz4,
}

impl Codec {
    pub const fn id(self) -> u16 {
        match self {
            Codec::Store => 0,
            Codec::Lz4 => 1,
        }
    }

    pub const fn from_id(id: u16) -> Option<Codec> {
        match id {
            0 => Some(Codec::Store),
            1 => Some(Codec::Lz4),
            _ => None,
        }
    }
}

pub(crate) fn compress(codec: Codec, raw: &[u8]) -> Vec<u8> {
    match codec {
        Codec::Store => raw.to_vec(),
        Codec::Lz4 => lz4_flex::block::compress(raw),
    }
}

/// The raw bytes, or `None` when `stored` does not decode to exactly `raw_len` bytes.
pub(crate) fn decompress(codec: Codec, stored: &[u8], raw_len: usize) -> Option<Vec<u8>> {
    match codec {
        Codec::Store => (stored.len() == raw_len).then(|| stored.to_vec()),
        Codec::Lz4 => {
            let mut raw = vec![0; raw_len];
            let n = lz4_flex::block::decompress_into(stored, &mut raw).ok()?;
            (n == raw_len).then_some(raw)
        }
    }
}
