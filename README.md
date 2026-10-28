# Kagari Visual Novel Engine

Kagari is a visual novel engine with its own Rust-like language. The story is written as
`pushdown fn` code over finite data, so the compiler can decide questions a general-purpose
script cannot: whether every step terminates, whether a choice can get stuck, which scenes are
unreachable, and whether the screen at each step is consistent. Presentation (`const fn`) and the
game system (`fn`) stay free to compute.

## Documentation

| Book | For | English | 日本語 |
| ---- | --- | ------- | ------ |
| The Kagari Reference | Studios: concepts, language, CLI, runtime, diagnostics | [en](docs/reference/en/src/introduction.md) | [ja](docs/reference/ja/src/introduction.md) |
| Kagari STD Reference | Studios: the standard library `std` | [en](docs/std/en/src/about.md) | [ja](docs/std/ja/src/about.md) |
| Kagari Develop Guide | Contributors: design decisions, compiler, formats, runtime | [en](docs/dev/src/introduction.md) | — |

Build a book with `mdbook build docs/reference/en` (likewise `docs/reference/ja`, `docs/std/en`,
`docs/std/ja`, `docs/dev`); the output goes to `docs/_site/`.

## Status

| Component | Status |
| --------- | ------ |
| Compiler (`kagari-syntax`, `kagari-lang`): parse, resolve, type and kind check, const evaluation, story analysis, bytecode emission | ✅ |
| Bytecode (`kagari-bytecode`) and archives (`kagari-archive`) | ✅ |
| Standard library (`kagari-std` declarations, implemented by the engine) | ✅ except video playback and studio shaders |
| Tooling: `kagari` CLI, formatter, language server, tree-sitter grammar, Zed extension | ✅ |
| Desktop runtime, profile `native_v0` (the whole std except video and shaders) | ✅ |

## Quick start

```sh
cargo run -p kagari-cli -- build --manifest-path example/test_game1/kagari.toml
cargo run --release -p kagari-desktop-runtime -- example/test_game1/target/v0
```

On NixOS, `direnv allow` (or `nix develop`) enters the dev shell; other platforms need the
packages listed in [The Runtime](docs/reference/en/src/tools/runtime.md). Runtime options and the
headless test mode are described there and in [Headless Mode](docs/reference/en/src/tools/headless.md).

## Contributing

Read [`AGENTS.md`](AGENTS.md) and the [Develop Guide](docs/dev/src/introduction.md). Code changes
update the Develop Guide in the same change; user-visible changes also update the Reference or STD
Reference in both English and Japanese.

## License

Apache-2.0. See [LICENSE](LICENSE).
