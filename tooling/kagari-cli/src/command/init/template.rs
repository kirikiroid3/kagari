//! The files of a new project, embedded from `tooling/kagari-cli/template/`.

use kagari_project::TOOLCHAIN_VERSION;

const MANIFEST: &str = include_str!("../../../template/kagari.toml");
const MAIN: &str = include_str!("../../../template/src/main.kis");

/// `(output path, contents)`; the manifest and `src/main.kis` are rendered by [`render`].
pub const FILES: &[(&str, &[u8])] = &[
    ("kagari.toml", MANIFEST.as_bytes()),
    (".gitignore", include_bytes!("../../../template/_gitignore")),
    ("src/main.kis", MAIN.as_bytes()),
    (
        "src/scenario/mod.kix",
        include_bytes!("../../../template/src/scenario/mod.kix"),
    ),
    (
        "src/scripts/mod.kis",
        include_bytes!("../../../template/src/scripts/mod.kis"),
    ),
    (
        "src/scripts/characters.kis",
        include_bytes!("../../../template/src/scripts/characters.kis"),
    ),
    (
        "src/system/mod.kix",
        include_bytes!("../../../template/src/system/mod.kix"),
    ),
    (
        "src/effect/mod.kis",
        include_bytes!("../../../template/src/effect/mod.kis"),
    ),
    (
        "assets/bgimage/.gitkeep",
        include_bytes!("../../../template/assets/bgimage/.gitkeep"),
    ),
    (
        "assets/sound/.gitkeep",
        include_bytes!("../../../template/assets/sound/.gitkeep"),
    ),
    (
        "assets/music/.gitkeep",
        include_bytes!("../../../template/assets/music/.gitkeep"),
    ),
    (
        "assets/characters/character1/image/normal.png",
        include_bytes!("../../../template/assets/characters/character1/image/normal.png"),
    ),
    (
        "assets/characters/character1/voice/0001.wav",
        include_bytes!("../../../template/assets/characters/character1/voice/0001.wav"),
    ),
];

/// The contents of `path` for a project named `name`.
pub fn render(path: &str, contents: &[u8], name: &str) -> Vec<u8> {
    match path {
        "kagari.toml" => MANIFEST
            .replace("\"{{name}}\"", &toml_string(name))
            .replace("{{toolchain}}", &toolchain_req())
            .into_bytes(),
        "src/main.kis" => MAIN
            .replace("\"{{name}}\"", &kagari_string(name))
            .into_bytes(),
        _ => contents.to_vec(),
    }
}

/// `major.minor` of this toolchain.
fn toolchain_req() -> String {
    TOOLCHAIN_VERSION
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join(".")
}

/// A TOML basic string.
fn toml_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(ch))),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// A Kagari string literal.
fn kagari_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{{{:X}}}", u32::from(ch))),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
