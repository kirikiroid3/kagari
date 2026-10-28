#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::arithmetic_side_effects)]

use std::fs::{self, File};
use std::io::Cursor;
use std::num::NonZeroU64;
use std::path::Path;

use super::*;

fn build(builder: ArchiveBuilder) -> (Header, Vec<u8>) {
    let mut out = Cursor::new(Vec::new());
    let header = builder.finish(&mut out).unwrap();
    (header, out.into_inner())
}

fn open(bytes: Vec<u8>) -> Result<Archive<Cursor<Vec<u8>>>, Error> {
    Archive::open(Cursor::new(bytes))
}

fn text(len: usize) -> Vec<u8> {
    b"the quick brown fox jumps over the lazy dog. "
        .iter()
        .copied()
        .cycle()
        .take(len)
        .collect()
}

/// Incompressible bytes from a fixed LCG.
fn noise(len: usize) -> Vec<u8> {
    let mut state: u32 = 0x1234_5678;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

fn solid(limit: u64) -> ArchiveOptions {
    ArchiveOptions {
        codec: Codec::Lz4,
        solid_block_size: NonZeroU64::new(limit),
    }
}

fn loc(index: &ArchiveIndex, name: &str) -> FileLoc {
    match index.get(name) {
        Some(Lookup::File(loc)) => loc,
        other => panic!("{name}: {other:?}"),
    }
}

fn base(files: &[(&str, &[u8])]) -> (Header, Vec<u8>) {
    let mut b = ArchiveBuilder::base(ArchiveOptions::default());
    for (name, data) in files {
        b.add(name, data.to_vec()).unwrap();
    }
    build(b)
}

#[test]
fn archive_stems() {
    assert_eq!(
        archive_stem("characters/character1/image/normal.png"),
        "characters"
    );
    assert_eq!(archive_stem("bg.png"), "data");
    assert_eq!(archive_stem("system-fonts/msgothic.ttc"), "system-fonts");
}

#[test]
fn non_solid_round_trip() {
    let mut b = ArchiveBuilder::base(ArchiveOptions::default());
    b.add("a.txt", text(10 * 1024)).unwrap();
    b.add("empty", Vec::new()).unwrap();
    b.add("noise.bin", noise(4096)).unwrap();
    let (header, bytes) = build(b);

    assert_eq!(bytes[0..16], *b"ARCHKAGARI\0\0V000");
    assert_eq!(header.sequence, 0);
    assert_eq!(header.block_count, 3);
    assert_eq!(header.entry_count, 3);
    assert_eq!(Header::parse(&bytes).unwrap(), header);

    let mut archive = open(bytes).unwrap();
    assert_eq!(archive.read("a.txt").unwrap(), Some(text(10 * 1024)));
    assert_eq!(archive.read("empty").unwrap(), Some(Vec::new()));
    assert_eq!(archive.read("noise.bin").unwrap(), Some(noise(4096)));
    assert_eq!(archive.read("missing").unwrap(), None);

    let index = archive.index();
    let noise_block = index.block(loc(index, "noise.bin").block).unwrap();
    assert_eq!(noise_block.codec, 0);
    let text_block = index.block(loc(index, "a.txt").block).unwrap();
    assert_eq!(text_block.codec, 1);
    assert!(text_block.stored_len < text_block.raw_len);
    let names: Vec<_> = index.entries().map(|(n, _)| n).collect();
    assert_eq!(names, ["a.txt", "empty", "noise.bin"]);
}

#[test]
fn solid_blocks() {
    let files = [("a", noise(1024)), ("b", text(1024)), ("c", noise(1024))];
    for (limit, blocks) in [(1 << 20, 1), (1500, 3)] {
        let mut b = ArchiveBuilder::base(solid(limit));
        for (name, data) in &files {
            b.add(name, data.clone()).unwrap();
        }
        let (header, bytes) = build(b);
        assert_eq!(header.block_count, blocks, "limit {limit}");
        let mut archive = open(bytes).unwrap();
        for (name, data) in &files {
            assert_eq!(archive.read(name).unwrap().as_ref(), Some(data));
        }
    }

    // A file larger than the limit gets a block of its own.
    let mut b = ArchiveBuilder::base(solid(1500));
    b.add("a", text(1024)).unwrap();
    b.add("b", text(1024)).unwrap();
    b.add("b2", noise(4096)).unwrap();
    b.add("c", text(1024)).unwrap();
    let (header, bytes) = build(b);
    assert_eq!(header.block_count, 4);
    let mut archive = open(bytes).unwrap();
    let big = loc(archive.index(), "b2");
    assert_eq!(big.offset, 0);
    assert_eq!(archive.index().block(big.block).unwrap().raw_len, 4096);
    assert_eq!(archive.read("b2").unwrap(), Some(noise(4096)));
    assert_eq!(archive.read("c").unwrap(), Some(text(1024)));
}

#[test]
fn streaming_protocol() {
    let mut b = ArchiveBuilder::base(solid(1 << 20));
    b.add("a.txt", text(3000)).unwrap();
    b.add("z.txt", text(500)).unwrap();
    let (_, bytes) = build(b);

    let header = Header::parse(&bytes[..64]).unwrap();
    let range = header.index_range();
    let index =
        ArchiveIndex::parse(header, &bytes[range.start as usize..range.end as usize]).unwrap();
    let file = loc(&index, "a.txt");
    let block = index.block(file.block).unwrap();
    let range = block.range();
    let raw = decode_block(
        file.block,
        block,
        &bytes[range.start as usize..range.end as usize],
    )
    .unwrap();
    let data = &raw[file.offset as usize..(file.offset + file.len) as usize];
    assert_eq!(data, text(3000));
}

#[test]
fn unknown_codec_fails_per_block() {
    let (header, mut bytes) = base(&[("a", &text(2000)), ("b", &text(100))]);
    assert_eq!(header.block_count, 2);
    // Block 0's record starts the index; its codec is at +24.
    bytes[64 + 24..64 + 26].copy_from_slice(&99u16.to_le_bytes());
    let index_end = 64 + header.index_len as usize;
    let crc = crc32fast::hash(&bytes[64..index_end]);
    bytes[48..52].copy_from_slice(&crc.to_le_bytes());

    let mut archive = open(bytes).unwrap();
    assert!(matches!(
        archive.read("a"),
        Err(Error::UnsupportedCodec(99))
    ));
    assert_eq!(archive.read("b").unwrap(), Some(text(100)));
}

#[test]
fn corruption() {
    let (_, bytes) = base(&[("a", &text(2000)), ("b", &noise(300))]);
    assert!(open(bytes.clone()).is_ok());

    let mut data = bytes.clone();
    *data.last_mut().unwrap() ^= 1;
    let mut archive = open(data).unwrap();
    assert!(matches!(
        archive.read("b"),
        Err(Error::BlockCrc { block: 1 })
    ));
    assert_eq!(archive.read("a").unwrap(), Some(text(2000)));

    let mut index = bytes.clone();
    index[64] ^= 1;
    assert!(matches!(open(index), Err(Error::IndexCrc)));

    let mut magic = bytes.clone();
    magic[0] = b'X';
    assert!(matches!(open(magic), Err(Error::BadMagic)));

    let mut version = bytes.clone();
    version[13..16].copy_from_slice(b"001");
    assert!(matches!(open(version), Err(Error::UnsupportedVersion)));

    let short = bytes[..bytes.len() - 1].to_vec();
    assert!(matches!(
        open(short),
        Err(Error::FileLength { expected, found }) if expected == found + 1
    ));

    assert!(matches!(open(bytes[..10].to_vec()), Err(Error::Truncated)));
    assert!(matches!(Header::parse(&bytes[..10]), Err(Error::Truncated)));
}

#[test]
fn names() {
    let mut b = ArchiveBuilder::base(ArchiveOptions::default());
    for name in ["", "/a", "a/", "a//b", "./a", "a/../b", "a\\b", "a\0b"] {
        assert!(
            matches!(b.add(name, Vec::new()), Err(Error::BadName(n)) if n == name),
            "{name:?}"
        );
    }
    b.add("dir/x.png", Vec::new()).unwrap();
    b.add("x", Vec::new()).unwrap();
    assert!(matches!(b.add("x", Vec::new()), Err(Error::DuplicateName(n)) if n == "x"));
    assert!(matches!(b.delete("y"), Err(Error::DeleteInBase)));
}

/// Base `{a: 1, b: 2, c: 3}`; patch 1 deletes `b`, replaces `a` and adds `d`; patch 2 adds `b`
/// back.
fn chain_layers() -> [(Header, Vec<u8>); 3] {
    let (h0, base) = base(&[("a", b"1"), ("b", b"2"), ("c", b"3")]);
    let mut p1 = ArchiveBuilder::patch(&h0, ArchiveOptions::default()).unwrap();
    p1.delete("b").unwrap();
    p1.add("a", b"1b".to_vec()).unwrap();
    p1.add("d", b"4".to_vec()).unwrap();
    let (h1, p1) = build(p1);
    let mut p2 = ArchiveBuilder::patch(&h1, ArchiveOptions::default()).unwrap();
    p2.add("b", b"2b".to_vec()).unwrap();
    let (h2, p2) = build(p2);
    assert_eq!((h1.sequence, h1.parent_crc), (1, h0.index_crc));
    assert_eq!((h2.sequence, h2.parent_crc), (2, h1.index_crc));
    [(h0, base), (h1, p1), (h2, p2)]
}

fn patched(layers: &[&Vec<u8>]) -> Result<PatchedArchive<Cursor<Vec<u8>>>, Error> {
    PatchedArchive::new(
        layers
            .iter()
            .map(|bytes| open((*bytes).clone()).unwrap())
            .collect(),
    )
}

#[test]
fn chain_semantics() {
    let [(_, base), (_, p1), (_, p2)] = chain_layers();

    let mut two = patched(&[&base, &p1]).unwrap();
    assert_eq!(two.read("a").unwrap(), Some(b"1b".to_vec()));
    assert_eq!(two.read("b").unwrap(), None);
    assert_eq!(two.read("c").unwrap(), Some(b"3".to_vec()));
    assert_eq!(two.chain().names(), ["a", "c", "d"]);

    let mut three = patched(&[&base, &p1, &p2]).unwrap();
    assert_eq!(three.read("b").unwrap(), Some(b"2b".to_vec()));
    assert_eq!(three.read("a").unwrap(), Some(b"1b".to_vec()));
    assert_eq!(three.chain().names(), ["a", "b", "c", "d"]);
}

#[test]
fn chain_validation() {
    let [(_, base), (_, _), (_, p2)] = chain_layers();
    let (other, _) = self::base(&[("z", b"9")]);
    let mut stray = ArchiveBuilder::patch(&other, ArchiveOptions::default()).unwrap();
    stray.add("a", b"x".to_vec()).unwrap();
    let (_, stray) = build(stray);

    assert!(matches!(
        patched(&[&base, &stray]).err(),
        Some(Error::ChainParent { sequence: 1 })
    ));
    assert!(matches!(
        patched(&[&base, &p2]).err(),
        Some(Error::ChainOrder {
            expected: 1,
            found: 2
        })
    ));
    assert!(matches!(
        ArchiveChain::new(Vec::new()),
        Err(Error::EmptyChain)
    ));
}

fn write_layer(path: &Path, builder: ArchiveBuilder) -> Header {
    builder.finish(File::create(path).unwrap()).unwrap()
}

#[test]
fn filesystem_discovery() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("cg.kiarchive");
    let source = dir.path().join("a.src");
    fs::write(&source, b"1").unwrap();

    let mut b = ArchiveBuilder::base(ArchiveOptions::default());
    b.add_file("a", &source).unwrap();
    b.add("keep", b"k".to_vec()).unwrap();
    let mut header = write_layer(&base, b);
    for (n, data) in [(1, "2"), (2, "3"), (4, "stray")] {
        if n == 4 {
            // Patch 3 is missing, so patch 4 is never opened.
            header.sequence = 3;
        }
        let mut p = ArchiveBuilder::patch(&header, ArchiveOptions::default()).unwrap();
        p.add("a", data.as_bytes().to_vec()).unwrap();
        header = write_layer(&patch_path(&base, n).unwrap(), p);
    }
    assert!(dir.path().join("cg.004.kiarchive").exists());

    let mut chain = open_chain(&base).unwrap();
    assert!(chain.chain().layer(2).is_some());
    assert!(chain.chain().layer(3).is_none());
    assert_eq!(chain.read("a").unwrap(), Some(b"3".to_vec()));
    assert_eq!(chain.read("keep").unwrap(), Some(b"k".to_vec()));

    assert_eq!(
        patch_path(Path::new("x/cg.kiarchive"), 1000).unwrap(),
        Path::new("x/cg.1000.kiarchive")
    );
    assert_eq!(patch_path(Path::new("x/cg.zip"), 1), None);
    assert!(matches!(
        open_chain(&dir.path().join("cg.zip")),
        Err(Error::BadName(_))
    ));
}

#[test]
fn source_changed() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.src");
    fs::write(&source, b"short").unwrap();
    let mut b = ArchiveBuilder::base(ArchiveOptions::default());
    b.add_file("a", &source).unwrap();
    fs::write(&source, b"longer now").unwrap();
    assert!(matches!(
        b.finish(Cursor::new(Vec::new())),
        Err(Error::SourceChanged(path)) if path == source
    ));
}
