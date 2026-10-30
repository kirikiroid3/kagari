---
name: kagari-bytecode
description: Load when changing core/kagari-bytecode (the .kagari format, opcodes, verifier, disassembler), runtime_libs/kagari-vm (interpreter, values, traps, budgets, story fiber, PRNG), core/kagari-archive (.kiarchive/.kisave), core/kagari-debug (.kagari-cache files), or the shape of what core/kagari-lang/src/emit produces.
---

# Kagari bytecode, VM and binary formats

## Scope

| Path | What |
| ---- | ---- |
| `core/kagari-bytecode/src/module.rs` | `Module`, tables, enum codes, `MAGIC` (`V000`), `NONE` |
| `core/kagari-bytecode/src/instr.rs` | `Instr`, opcode/mnemonic/words/registers/range, encode/decode of one instruction |
| `core/kagari-bytecode/src/{encode,decode,verify,disasm}.rs` | file encoding, decoding (`DecodeError`), verifier (`VerifyErrorKind`), `kagari disasm` |
| `runtime_libs/kagari-vm/src/` | `Program::load`, `Vm`, `exec/` loop, `refs`, `int`/`ops`, `story` (fiber), `rng`, `host` (`Host` trait), `trap` |
| `core/kagari-archive/src/{archive,save}/` | `.kiarchive` (blocks, index, patches), `.kisave` (paged tree, `compact`) |
| `core/kagari-debug/src/` | `.kagari-cache` files over the `.kisave` container (uncompressed): `StoryGraph` (`graph.rs`), `DebugInfo` (`debug_info.rs`), `AnalysisCache` / `DebugArtifact` (`file.rs`), `codec.rs`; `SCHEMA`, `ANALYSIS_FILE`, `DEBUG_FILE` in `lib.rs` |
| `core/kagari-lang/src/emit/` | producer of `Module` (intrinsics vs imports in `codegen/call.rs`) |

## Read first

- `docs/dev/src/formats/kagari.md` — byte layout, opcode table, verifier rules, panics, PRNG,
  compatibility policy.
- `docs/dev/src/runtime/vm.md` — load, frames, fiber, budgets, host interface, verifier reliance.
- `docs/dev/src/formats/archive.md` — `.kiarchive` and `.kisave` layouts and invariants.
- `docs/dev/src/formats/cache.md` — `.kagari-cache` files: analysis cache, debug artifact, schema.
- `docs/dev/src/compiler/emit.md` — how MIR becomes bytecode.

## Invariants

- **Frozen opcodes.** Never renumber, repurpose or change the operand layout of an existing
  opcode. New behaviour = new opcode in a free slot.
- **Canonical encoding.** Little-endian; every record a multiple of 8 bytes; zero padding and
  reserved fields; unused operand fields zero (decode re-encodes and compares). `decode(encode(m))
  == m`.
- **Sections.** Strictly ascending ids; `0x01..=0x7F` critical (unknown = reject, all ten
  required), `0x80..` ancillary (unknown = skip). Optional new data goes in an ancillary section.
- **Version.** The magic's last 4 bytes (`V000`) change only on a breaking change once the format
  is declared stable; pre-1.0 in-place changes must update encoder, decoder, verifier, VM,
  emitter, tests and docs together. Same rule for `ARCHKAGARI\0\0V000` / `SAVEKAGARI\0\0V000`.
- **Decoder vs verifier.** Decoder: container and record encoding only, never panics. Verifier:
  indices, constants order, `main`/entry, register ranges, fall-through, arity, kind rules
  (`FnKind::may_call`), STEP/RAND scene-only, REFSTAT story-only. Keep checks in their layer.
- **Verifier soundness.** Anything the VM treats as `fault()` must be excluded by `verify`. Value
  types stay dynamic (tagged values, no raw memory); do not add static type checks to `verify`.
- **No panics anywhere.** These crates `#![deny(clippy::unwrap_used, clippy::panic,
  clippy::expect_used, clippy::arithmetic_side_effects)]`. Use checked arithmetic and `get`.
- **Determinism.** No floats, no hash-order-dependent semantics; PRNG is xoroshiro128++ seeded by
  two SplitMix64 outputs (seed 0 → first `next()` = `0x6F68E1E7E2646EE1`).
- **Value semantics.** Only `Ref` aliases; aggregates and host objects copy on write.
- `.kisave`: every `Save` is valid (eager `validate`, edits preserve it); data is a tree.
  `.kiarchive`: names sorted/unique, blocks contiguous, file ends at the last block, CRC32 on
  index and every block.
- `.kagari-cache`: a reader rejects any other `SCHEMA`; a change to the stored shape bumps
  `kagari_debug::SCHEMA`. The debug info never changes the bytecode (`game.kagari` is the same in
  the debug and release profiles); the debug artifact carries the SHA-256 of its `game.kagari`.

## Workflows

### Add an opcode
1. `instr.rs`: variant with doc comment `/// 0xNN \`MNEM operands\``; arms in `opcode`, `mnemonic`,
   `words`, `registers`, `range` (if it reads a register window), `encode`, `decode`.
2. `verify.rs`: index/kind/arity rules for its operands. `disasm.rs`: operand printing.
3. `tests.rs`: use it in `full()` / `main_code()` so `full_module_uses_every_opcode_and_tag`
   passes; add a rejection test for each new verifier rule.
4. VM: semantics in `exec/mod.rs` (control flow, calls, story) or `exec/data.rs`; tests in
   `runtime_libs/kagari-vm/tests/vm.rs`.
5. Emitter: produce it in `core/kagari-lang/src/emit/codegen/`.
6. Docs: regenerate the opcode table in `docs/dev/src/formats/kagari.md` from `instr.rs`
   (`Instr::opcode` + `mnemonic` + `words` + variant docs), update Verification/Panics.

### Add a section, constant tag or table field
1. `module.rs` model; `encode.rs` (id in `mod id`, and in `REQUIRED` if critical); `decode.rs`;
   `verify.rs` index checks; `full()` test module.
2. VM `Program::load` if the runtime needs it.
3. Docs: layout tables in `formats/kagari.md`.

### Change VM limits or the host interface
1. Budgets: `FUEL_PER_RUN`, `max_call_depth` handling in `exec/mod.rs` and `story.rs`.
2. `Host` changes: update every impl (`kagari-engine/src/host/mod.rs`,
   `kagari-engine/src/test_support.rs`, `kagari-vm/tests/common/mod.rs`).
3. Docs: `runtime/vm.md`; user-visible limits also in the Reference runtime pages (en + ja).

### Change `.kiarchive` / `.kisave`
1. Layout constants (`archive/index.rs`, `save/layout.rs`), writer, reader, validator together.
2. Tests in `archive/tests.rs` / `save/tests.rs`: round trip and every rejection path.
3. Docs: tables in `formats/archive.md`. A `.kisave` change affects players' saves: coordinate
   with `docs/dev/src/runtime/saves.md`.

## Tests

```sh
cargo test -p kagari-bytecode          # round trip, rejections, disasm, corruption fuzz
cargo test -p kagari-vm                # interpreter, story fiber, PRNG
cargo test -p kagari-archive           # archives, patches, saves, compact
cargo test -p kagari-debug             # .kagari-cache round trips and rejections
cargo test -p kagari-lang emit::       # emitter output verifies
cargo test -p kagari-engine            # engine over real compiled programs
cargo clippy --workspace --all-targets
```

Inspect output: `target/debug/kagari build` in a project, then
`target/debug/kagari disasm target/game.kagari` (`--all` includes std functions).

## Docs to update

- `docs/dev/src/formats/kagari.md`, `docs/dev/src/formats/archive.md`, `docs/dev/src/formats/cache.md`,
  `docs/dev/src/runtime/vm.md`.
- `docs/dev/src/compiler/emit.md` when emission changes.
- Reference (en + ja) only for user-visible effects (limits, panic messages); never opcode detail.
