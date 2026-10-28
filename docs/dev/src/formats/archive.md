# Archives and Save Databases

`core/kagari-archive` defines the two other binary formats of Kagari: `.kiarchive`, the
immutable asset package that `kagari build` writes and the runtime reads, and `.kisave`, the
small paged database the engine uses for settings, meta data and saves. Both are version
`V000`, little-endian, and validated eagerly on open.

Where in the code:

- `core/kagari-archive/src/archive/`: `index.rs` (header, index parsing, `decode_block`, name
  rule), `build.rs` (`ArchiveBuilder`, `ArchiveOptions`), `read.rs` (`Archive` over
  `Read + Seek`), `chain.rs` (`ArchiveChain`, `PatchedArchive`, `open_chain`, `patch_path`),
  `codec.rs` (`Codec`), `error.rs`, `tests.rs`.
- `core/kagari-archive/src/save/`: `layout.rs` (constants, `ValueType`), `validate.rs`,
  `read.rs` (`TableRef`, `ArrayRef`, `ValueRef`), `write.rs` (`TableMut`, `ArrayMut`,
  `Scalar`), `compact.rs`, `error.rs`, `tests.rs`.
- `core/kagari-archive/src/le.rs`: bounds-checked little-endian accessors (the crate denies
  `unwrap`, `panic` and unchecked arithmetic).
- Writers and readers: `tooling/kagari-cli/src/command/build/archives.rs` (packs assets),
  `runtime/kagari-desktop-runtime/src/archives.rs` (opens chains),
  `runtime_libs/kagari-engine/src/store/` and `src/saves/` (`.kisave` content, see
  [Saves, Traces and Snapshots](../runtime/saves.md)).

## `.kiarchive` {#kiarchive}

**Decision.** One archive per top-level asset directory, a front index, per-block compression
and CRCs, and numbered patch archives instead of rewriting.

**Why.** A web or streaming runtime must find a file with three range requests (header, index,
one block) and verify what it got. Studios ship updates as small patches. Per-file blocks keep
every file independently fetchable; solid blocks trade that for better compression of many
small files.

**Alternatives rejected.** ZIP (central directory at the end, no streaming, per-entry
headers); one monolithic archive per game (a patch of one image would invalidate everything).

<!-- diagram: formats-archive-layout (mermaid-ish)
flowchart LR
  header["header 64 B"] ==> blocks["block table 32 B each"] ==> entries["entry table 32 B each, sorted by name"] ==> pool["name pool"] ==> data["blocks back to back (Store or LZ4)"]
  p2["cg.002.kiarchive"] ==> p1["cg.001.kiarchive"] ==> base["cg.kiarchive (sequence 0)"]
-->
{{#include ../diagrams/formats-archive-layout.svg}}

### Header {#kiarchive-header}

| Offset | Size | Field | Meaning |
| -----: | ---: | ----- | ------- |
| 0 | 16 | magic | `ARCHKAGARI\0\0V000` |
| 16 | 4 | `sequence` | 0 for a base archive, `n ≥ 1` for patch `n` |
| 20 | 4 | reserved | = 0 |
| 24 | 8 | `block_count` | at most `u32::MAX - 1` |
| 32 | 8 | `entry_count` | |
| 40 | 8 | `index_len` | length of the index region, which starts at offset 64 |
| 48 | 4 | `index_crc` | CRC32 (IEEE, `crc32fast`) of the index region |
| 52 | 4 | `parent_crc` | the parent layer's `index_crc`; 0 for a base |
| 56 | 8 | reserved | = 0 |

A magic whose first 13 bytes (`ARCHKAGARI\0\0V`) match but whose version digits differ is
`UnsupportedVersion`; anything else is `BadMagic`.

### Index region

Starts at offset 64 and is exactly `index_len` bytes: the block table, the entry table, then
the name pool (the rest of the region; the builder pads it to 8 bytes with zeros).

Block record (32 bytes, block id = position):

| Offset | Size | Field | Meaning |
| -----: | ---: | ----- | ------- |
| 0 | 8 | `offset` | absolute file offset of the stored bytes |
| 8 | 8 | `stored_len` | stored (possibly compressed) length |
| 16 | 8 | `raw_len` | decompressed length |
| 24 | 2 | `codec` | 0 Store, 1 LZ4 |
| 26 | 2 | reserved | = 0 |
| 28 | 4 | `crc` | CRC32 of the stored bytes |

Entry record (32 bytes):

| Offset | Size | Field | Meaning |
| -----: | ---: | ----- | ------- |
| 0 | 4 | `name_off` | offset of the name in the pool |
| 4 | 4 | `name_len` | name length in bytes |
| 8 | 4 | `block` | block id; `u32::MAX` = tombstone (deleted in this patch) |
| 12 | 4 | reserved | = 0 |
| 16 | 8 | `offset` | start of the file inside the block's **raw** bytes |
| 24 | 8 | `len` | file length |

### Blocks and codecs

Blocks follow the index back to back: block 0 starts at `64 + index_len`, each next block at
the end of the previous one, and the file ends exactly at the end of the last block
(`FileLength`). A block is either one file or a *solid* group of consecutive files (in name
order) that are decompressed together.

| Codec | Id | Stored bytes | Length rule |
| ----- | -: | ------------ | ----------- |
| Store | 0 | the raw bytes | `raw_len == stored_len` |
| LZ4 | 1 | one LZ4 *block-format* stream (`lz4_flex::block`, no frame) of the whole raw block | `raw_len ≤ 255 × stored_len` |

An unknown codec id still parses (so the index of a newer archive can be listed); decoding such
a block is `UnsupportedCodec`. The LZ4 ratio bound caps the allocation a hostile index can
request. The builder stores a block uncompressed whenever compression does not make it smaller.

`ArchiveOptions { codec, solid_block_size }`: the default is LZ4 with no solid blocks (one block
per file), which is what `kagari build` uses. With `solid_block_size = Some(n)`, a block takes
files until the next one would push its raw size past `n`; a larger file gets its own block.

### Names

Entry names are the asset paths of the bytecode (`characters/a/normal.png`,
`system-fonts/msgothic.ttc`). The name rule (`validate_name`): non-empty UTF-8, length fits
`u32`, no `\` or NUL, no leading or trailing `/`, no empty, `.` or `..` component. Entries are
sorted **strictly ascending by bytes**, so lookup is a binary search and duplicates are
impossible.

`archive_stem(path)` decides which archive holds a path: its first `/`-separated segment, or
`data` for a path without `/`. `kagari build` writes `<stem>.kiarchive` for every stem the
bytecode's asset table uses and deletes stale `.kiarchive` files from the output directory
first.

### Reading

The index module is I/O-free so a network reader can stream:

1. fetch bytes `0..64`, `Header::parse`;
2. fetch `Header::index_range()`, `ArchiveIndex::parse` (checks the CRC first);
3. `ArchiveIndex::get(name)`, fetch `BlockInfo::range()` of its block, `decode_block` (checks
   length and block CRC, decompresses to exactly `raw_len`);
4. slice `offset..offset + len` out of the raw block.

`Archive<R: Read + Seek>` does the same from a file and caches the last decoded block, so
sequential reads from one solid block decompress it once.

Validation on open: reserved fields zero; a base (`sequence == 0`) has `parent_crc == 0` and no
tombstones; block offsets contiguous; per-codec length rule; each entry's range lies inside its
block's raw length; a tombstone has `offset == len == 0`; the name pool contains every name.

### Patch chains

Patch `n` of `cg.kiarchive` is `cg.NNN.kiarchive` (`patch_path`, three digits, zero-padded:
`cg.001.kiarchive`). `open_chain(base)` opens the base and patches 1, 2, … up to the first
missing file. `ArchiveChain::new` requires layer `i` to have `sequence == i` and, for
`i ≥ 1`, `parent_crc` equal to layer `i - 1`'s `index_crc`, so a patch applied to the wrong
base is rejected (`ChainOrder`, `ChainParent`).

A lookup goes from the newest layer down: the first layer that mentions the name decides. A
file entry wins; a tombstone hides the name in every older layer. `ArchiveBuilder::patch(parent,
options)` builds patch `parent.sequence + 1`; `add` replaces, `delete` writes a tombstone
(`DeleteInBase` on a base builder).

The CLI does not build patches yet; the format and library support them.

## `.kisave` {#kisave}

**Decision.** A JSONB-like tree of tables and arrays in one byte buffer, edited in place in
memory and written back whole, with a bump allocator and an explicit `compact()`.

**Why.** Saves, settings and meta are small, read often and changed a little at a time.
Editing a validated buffer in place avoids re-serializing; whole-file writes keep the platform
interface trivial (`Platform::write_store(name, bytes)`) and work on the web.

**Consequences.** Edits never free space; replaced data becomes garbage until `compact()`.
The format is unaligned (cells are 10 bytes), so every access goes through the bounds-checked
`le` helpers.

### Header

| Offset | Size | Field | Meaning |
| -----: | ---: | ----- | ------- |
| 0 | 16 | magic | `SAVEKAGARI\0\0V000` |
| 16 | 8 | `root` | offset of the root table's head |
| 24 | 8 | `end` | bump pointer: the first byte never allocated |
| 32 | 16 | reserved | two `u64`, = 0 |
| 48 | | heap | table heads, array heads, names, out-of-line values |

The file length is a non-zero multiple of the page size, 16 384 bytes; allocation grows the
buffer by whole pages. `Save::new()` is one page with the root table at 48, capacity 128, and
`end = 3392` (48 + 16 + 128 × 26).

### Cells and nodes

A **cell** is 10 bytes: `type: u16`, `value: u64`.

| Type id | `ValueType` | `value` |
| ------: | ----------- | ------- |
| 0 | Null | inline, must be 0 |
| 1 | Bool | inline, 0 or 1 |
| 2 | I64 | inline, the `i64` bits |
| 3 | U64 | inline |
| 4 | I128 | offset of 16 bytes |
| 5 | U128 | offset of 16 bytes |
| 6 | Decimal | offset of 20 bytes: `mantissa: i128`, `scale: u32` (`mantissa / 10^scale`) |
| 7 | Rat | offset of 32 bytes: `num: i128`, `den: i128`, stored raw |
| 8 | String | offset of a head `(offset: u64, len: u64)` → UTF-8 bytes |
| 9 | Bytes | offset of a head `(offset: u64, len: u64)` → bytes |
| 10 | Table | offset of a table head |
| 11 | Array | offset of an array head |

| Node | Layout |
| ---- | ------ |
| table | head `count: u64`, `cap: u64`, then `cap` field slots of 26 bytes: `name_off: u64`, `name_len: u64`, cell |
| array | head `count: u64`, `cap: u64`, then `cap` cells of 10 bytes |

The API creates tables and arrays with capacity 8 (`DEFAULT_CAP`); a full node relocates to a
fresh region with double the capacity (at least 8) and the pointer that owns it (the header's
root pointer or the parent cell) is rewritten.

### Validation (`Save::from_bytes`)

Every `Save` value is well-formed: loading validates eagerly, and every edit preserves the
rules, so in-place edits are memory-safe.

- size, magic/version, reserved header fields zero, `48 ≤ root`, `48 ≤ end ≤ len`;
- every head, slot array, name and out-of-line value lies inside `[48, end)`; `count ≤ cap`;
- field names are non-empty UTF-8 and strictly ascending by bytes (lookup is a binary search);
  Strings are UTF-8; Null is 0, Bool is 0/1; type ids are known;
- nesting depth at most 128 (`MAX_DEPTH`, root = 0);
- the data is a tree: no node is reached twice and no two referenced regions overlap
  (`SharedNode`), because an in-place write through one would corrupt the other.

### Editing

- `TableMut::set` inserts at the sorted position (shifting later slots) or overwrites. A
  fixed-size value (I128, U128, Decimal, Rat) replacing one of the same type is rewritten in
  place; anything else is encoded afresh at the bump pointer.
- `remove` shifts later slots down and zeroes the freed slot; `ArrayMut::truncate` zeroes the
  cut cells. The referenced data becomes garbage.
- `table(name)` / `array(name)` return the child, creating it empty when missing; a type
  mismatch is `TypeMismatch`.
- `compact()` deep-copies the live tree into a fresh save with the root back at 48, keeping
  every node's capacity, field order and values. No engine code calls it yet.

### Users

The engine writes `settings.kisave` (the `Setting` statics), `meta.kisave` (the `Meta`
statics, read flags, seen sets, the next autosave slot) and `save-manual-N.kisave`,
`save-quick.kisave`, `save-auto-N.kisave` (story traces). How VM values map to cells is in
[Saves, Traces and Snapshots](../runtime/saves.md).

## Compatibility {#compatibility}

- Both formats carry their version in the last 4 magic bytes (`V000`). A reader reports another
  version as `UnsupportedVersion`, never as garbage.
- Reserved fields are zero today, so a future version can give them meaning.
- `.kiarchive`: new codecs get new ids (`Codec` is `#[non_exhaustive]`; add a variant, an id and
  an arm in `compress`/`decompress`). Old readers can still list such an archive.
- `.kisave`: new value types need new ids; an old reader rejects them (`UnknownType`).
- `.kisave` content is a compatibility contract for players' saves; its schema is owned by the
  engine and documented in [Saves, Traces and Snapshots](../runtime/saves.md).

## Invariants

- No panics, no unchecked arithmetic: every offset computation is checked (`le.rs`).
- `.kiarchive`: names sorted and unique; blocks contiguous; the file ends at the last block;
  CRCs cover the index and every stored block.
- `.kisave`: a loaded or edited `Save` always passes `validate`; the data is a tree.

## When you change this

- Update the tables in this chapter and the crate's `tests.rs` (round trips and every
  rejection path).
- A layout change is a version change: bump the magic's version digits and keep a reader for
  `V000` if released games or players' saves must still load.
- Load the `kagari-bytecode` skill (`.agents/skills/kagari-bytecode/SKILL.md`), which covers
  this crate too.
