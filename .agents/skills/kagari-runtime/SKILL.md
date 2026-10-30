---
name: kagari-runtime
description: Load when changing the Kagari engine, text layout, renderer, runtime profiles, story debugger or desktop/headless runtime (runtime_libs/kagari-engine, kagari-text, kagari-render, kagari-compat, kagari-debugger, runtime/kagari-desktop-runtime) — not for the VM core (kagari-vm), which is kagari-bytecode.
---

# Kagari runtime

## Scope

| Path | What |
| ---- | ---- |
| `runtime_libs/kagari-engine` | host imports, mode stack, reactive UI, story driver, screen and timeline, audio model, input, stores, saves; `debug.rs` (debugger access) |
| `runtime_libs/kagari-text` | fonts, shaping, line breaking, ruby, rasterization |
| `runtime_libs/kagari-render` | the wgpu compositor for `Frame` |
| `runtime_libs/kagari-compat` | runtime profiles (`native_v0`) and asset sniffing, used by `kagari build` |
| `runtime_libs/kagari-debugger` | query engine (`session/`), debug protocol server, instruction hook, headless platform; features `client` (front-end helpers) and `dap` (the debug adapter) |
| `runtime/kagari-desktop-runtime` | winit/wgpu/kira/gilrs shell, `Platform`, headless mode |

The VM (`runtime_libs/kagari-vm`) and the bytecode format belong to the `kagari-bytecode` skill.

## Read first

Develop Guide (`docs/dev/src/`):

- `runtime/engine.md` — structure, boot, frame loop, host dispatch, modes, reactivity, story
  driver, screen/timeline, audio, input, stores, errors.
- `runtime/saves.md` — trace saves, choice IDs, replay, divergence, determinism rules.
- `runtime/text-render.md` — text pipeline, reveal, `Frame`, renderer, design resolution.
- `runtime/desktop.md` — the binary, exit statuses, headless script, platform deps.
- `runtime/debugger.md` — query engine, debug protocol, instruction hook, `nav`, breakpoints.
- `runtime/vm.md` and `std/builtins.md` when an import's ABI is involved.

## Invariants (do not break)

- **Determinism.** Story-visible behaviour depends only on bytecode, trace (entry, args, seed,
  choice IDs, step count) and inputs. The engine never reads a clock, RNG or file directly: only
  `now_ms` and `Platform`. Nothing reachable from `pushdown fn`/`const fn` may observe time,
  settings, meta reads or platform state. Floats only in presentation.
- **Save compatibility.** Saves are traces, never state dumps. Choice IDs are
  `<fn path>/<choice id>/<enum variant path or #index>`; types, variants, assets and statics are
  stored by path. Replay must use the same `driver::select`/`next_step` as live play. While
  `SystemState::replaying`, nothing persistent or audible happens. Layout change → bump the save
  `version` and keep a `Divergence` for old versions.
- **Trust boundary.** Bytecode is untrusted. Imports are bound by std path at boot (unknown path →
  `BootError::UnsupportedImport`); no native code from game data. No Rust panic on any input: the
  crates deny `unwrap`, `expect`, `panic!` and unchecked arithmetic. Game errors are `Trap`s →
  Crashed state; expected failures (divergence, unreadable store, missing archive) are values or
  logged fallbacks.
- **ABI tables.** `kagari-engine/src/values/table.rs` mirrors std `.kis` field order; every std
  `#[builtin]` import has an `import_table!` entry. The engine screen model mirrors the compiler's
  `core/kagari-lang/src/eval/machine/screen.rs`.
- **Navigation and story input are deferred** (`pending_nav`, `pending_input`) and applied only by
  `settle`.
- `kagari_text::units` defines reveal units for both layout and reveal.
- `kagari-compat::supports` must match the decoder features in the workspace `Cargo.toml`
  (`image`: png/jpeg/webp/bmp; `jxl-oxide` in the engine; `kira`:
  ogg/vorbis/flac/wav/pcm/mp3/aac/isomp4/alac). The desktop runtime's
  `supported_encodings_decode` test decodes the `kagari-compat` audio fixtures.

## Workflows

**New or changed std builtin (engine side)**
1. Declaration in `core/kagari-std/kis/std/**.kis`.
2. Variant in the module's `import_table!` and its `call` arm; decode args with `values::arg` and
   `values/decode.rs`, build results with `StdTypes::make`.
3. Types changed → `values/table.rs` and `tests/fixtures/std_types/main.kix`.
4. Reads that UI can observe: `ui.track(SourceKey::…)`; writes: `ui.invalidate(…)`.
5. Callable from the story? Check determinism and the replay flag.
6. Tests in the module's `tests.rs` or `src/tests/` (fixtures compiled via `test_support::compile`).

**Screen / animation change**: engine `screen/` + compiler `eval/machine/screen.rs` together;
drawing in `screen/draw.rs`; renderer only if a new `Node` or pipeline is needed.

**Save / replay change**: `saves/`, `story/driver.rs`; extend `src/tests/saves.rs` with a
round trip and a divergence case.

**Text / rendering change**: `kagari-text` tests use `tests/fonts/kagari-test-cjk.otf`; compare
headless captures before and after.

**New asset format or profile**: `kagari-compat` (`format.rs`, `sniff.rs`, `profile.rs`, tests),
decoder features, docs listed below.

## Verify

```sh
cargo test -p kagari-engine -p kagari-text -p kagari-render -p kagari-compat -p kagari-desktop-runtime
cargo test -p kagari-debugger --features dap
cargo clippy --workspace --all-targets
```

Smoke test with the example game (building it needs Noto Sans CJK installed as
`opentype/noto-cjk/NotoSansCJK-VF.otf.ttc` in a system font directory):

```sh
cargo run -p kagari-cli -- build --manifest-path example/test_game1/kagari.toml
cat > /tmp/kagari-smoke.txt <<'EOF'
wait 1000
capture title
key Enter
wait 2000
capture first-step
key Space
wait 1500
capture second-step
assert-no-crash
EOF
RUST_LOG=kagari=info cargo run -p kagari-desktop-runtime -- example/test_game1/target/v0 \
  --headless --script /tmp/kagari-smoke.txt --out /tmp/kagari-smoke
echo "exit: $?"   # 0 ok, 2 boot/usage, 3 trap, 4 GPU, 5 script
```

- Inspect `/tmp/kagari-smoke/*.png` (design resolution, 1280×720 for test_game1) by reading the
  images; saves and settings land in `/tmp/kagari-smoke/data/`.
- Read the log: `kagari::system` `modes: [...]`, `kagari::story` `step <fn>#<n> say|choice`,
  `chose …`, `kagari::saves` `saved`/`loaded`.
- Headless is deterministic (seed 1 unless `--seed`, fixed save time, time only via `wait`);
  rerunning the same script must give the same log and, on the same machine, the same PNGs.
- Adjust the script to the screen (`click X Y` in design pixels, `key Escape`, `wheel 1`, …); see
  `runtime/desktop.md` for all commands. Use `expect-crash` to test trap paths.
- The windowed runtime: `cargo run --release -p kagari-desktop-runtime -- example/test_game1/target/v0`.

## Docs to update

- Develop Guide chapters above (same change).
- Reference (en **and** ja): `tools/runtime.md`, `tools/headless.md`, `tools/debugger.md`, `concepts/system.md`,
  `concepts/saves.md`, `tools/projects.md` (profiles).
- STD Reference (en **and** ja): the module page of every changed std item; `appendix/profiles.md`.
