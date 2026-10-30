<p align="center">
  <img src="docs/site/public/logo.svg" alt="Kagari logo" width="128" height="128">
</p>

# Kagari Visual Novel Engine

Kagari is a visual novel engine with its own Rust-like language. The story is written as
`pushdown fn` code over finite data, so the compiler can decide questions a general-purpose
script cannot: whether every step terminates, whether a choice can get stuck, which scenes are
unreachable, and whether the screen at each step is consistent. Presentation (`const fn`) and the
game system (`fn`) stay free to compute.

## Documentation

The documentation site is [kagari.suitsu31.club](https://kagari.suitsu31.club) (日本語) /
[kagari.suitsu31.club/en](https://kagari.suitsu31.club/en/) (English):

| Part | For | English | 日本語 |
| ---- | --- | ------- | ------ |
| Tutorial | Newcomers: install Kagari, make a first game | [en](https://kagari.suitsu31.club/en/tutorial/) | [ja](https://kagari.suitsu31.club/tutorial/) |
| Language Reference | Studios: concepts, language, CLI, runtime | [en](https://kagari.suitsu31.club/en/reference/) | [ja](https://kagari.suitsu31.club/reference/) |
| STD Reference | Studios: the standard library `std` | [en](https://kagari.suitsu31.club/en/std/) | [ja](https://kagari.suitsu31.club/std/) |
| Diagnostics | Studios: every compiler diagnostic | [en](https://kagari.suitsu31.club/en/diagnostics/) | [ja](https://kagari.suitsu31.club/diagnostics/) |
| Kagari Develop Guide | Contributors: design decisions, compiler, formats, runtime | [en](docs/dev/src/introduction.md) | — |

The site's sources are in [`docs/site`](docs/site) (Astro; `pnpm install && pnpm run dev` there). The
Develop Guide is an mdBook: `mdbook build docs/dev` writes it to `docs/_site/`.

## Status

| Component | Status |
| --------- | ------ |
| Compiler (`kagari-syntax`, `kagari-lang`): parse, resolve, type and kind check, const evaluation, story analysis, bytecode emission | ✅ |
| Bytecode (`kagari-bytecode`) and archives (`kagari-archive`) | ✅ |
| Standard library (`kagari-std` declarations, implemented by the engine) | ✅ except video playback and studio shaders |
| Tooling: `kagari` CLI, formatter, language server, tree-sitter grammar, Zed extension | ✅ |
| Desktop runtime, profile `native_v0` (the whole std except video and shaders) | ✅ |

## Installing

Linux (x86_64, glibc 2.35 or newer):

```sh
curl -fsSL https://kagari-download.suitsu31.club/installer.sh | bash
```

Windows has an installer and NixOS a flake (add `github:kirikiroid3/kagari` as an input);
see [Installing Kagari](https://kagari.suitsu31.club/en/tutorial/install/). macOS builds are not
available yet.

## Quick start from the repository

```sh
cargo run -p kagari-cli -- build --manifest-path example/test_game1/kagari.toml
cargo run --release -p kagari-desktop-runtime -- example/test_game1/target/v0
```

On NixOS, `direnv allow` (or `nix develop`) enters the dev shell; other platforms need the
packages listed in [The Runtime](https://kagari.suitsu31.club/en/reference/tools/runtime/). Runtime
options and the headless test mode are described there and in
[Headless Mode](https://kagari.suitsu31.club/en/reference/tools/headless/).

## Contributing

Read [`AGENTS.md`](AGENTS.md) and the [Develop Guide](docs/dev/src/introduction.md). Code changes
update the Develop Guide in the same change; user-visible changes also update the documentation
site in both English and Japanese.

## License

Apache-2.0. See [LICENSE](LICENSE).
