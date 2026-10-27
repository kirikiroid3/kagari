use std::path::{Path, PathBuf};

use kagari_lang::{CompileOptions, DEFAULT_RECURSION_DEPTH, DEFAULT_STATE_CAP};
use serde::Deserialize;

use crate::target::Target;

/// A validated `kagari.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub version: semver::Version,
    /// The root source file, relative to the project root.
    pub entry: PathBuf,
    pub toolchain: semver::VersionReq,
    /// The asset directory, relative to the project root.
    pub asset_dir: PathBuf,
    /// The targets `kagari build` builds by default; non-empty, without duplicates.
    pub targets: Vec<Target>,
    /// The output directory, relative to the project root.
    pub out_dir: PathBuf,
    pub options: CompileOptions,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawManifest {
    project: RawProject,
    toolchain: RawToolchain,
    #[serde(default)]
    assets: RawAssets,
    #[serde(default)]
    build: RawBuild,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawProject {
    name: String,
    version: String,
    entry: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawToolchain {
    kagari: String,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawAssets {
    dir: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawBuild {
    targets: Option<Vec<String>>,
    out_dir: Option<String>,
    state_cap: Option<u32>,
    recursion_depth: Option<u32>,
    keyframe_check: Option<bool>,
}

/// Parses and validates a manifest. The error message does not name the file.
pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let raw: RawManifest = toml::from_str(text).map_err(|error| error.to_string())?;
    if raw.project.name.is_empty() {
        return Err("`project.name` must not be empty".to_owned());
    }
    let version = semver::Version::parse(&raw.project.version)
        .map_err(|error| format!("`project.version`: {error}"))?;
    let toolchain = semver::VersionReq::parse(&raw.toolchain.kagari)
        .map_err(|error| format!("`toolchain.kagari`: {error}"))?;

    let entry = relative(raw.project.entry.as_deref().unwrap_or("src/main.kis"), "project.entry")?;
    if !matches!(
        entry.extension().and_then(|ext| ext.to_str()),
        Some("kis" | "kix")
    ) {
        return Err("`project.entry` must name a `.kis` or `.kix` file".to_owned());
    }
    let asset_dir = relative(raw.assets.dir.as_deref().unwrap_or("assets"), "assets.dir")?;
    let out_dir = relative(raw.build.out_dir.as_deref().unwrap_or("target"), "build.out-dir")?;

    let targets = match raw.build.targets {
        None => vec![Target::V0],
        Some(names) => {
            if names.is_empty() {
                return Err("`build.targets` must name at least one target".to_owned());
            }
            let mut targets = Vec::new();
            for name in names {
                let target = Target::parse(&name).ok_or_else(|| {
                    format!("unknown target `{name}`; supported targets: {}", supported())
                })?;
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
            targets
        }
    };

    Ok(Manifest {
        name: raw.project.name,
        version,
        entry,
        toolchain,
        asset_dir,
        targets,
        out_dir,
        options: CompileOptions {
            state_cap: raw.build.state_cap.unwrap_or(DEFAULT_STATE_CAP),
            recursion_depth: raw.build.recursion_depth.unwrap_or(DEFAULT_RECURSION_DEPTH),
            keyframe_check: raw.build.keyframe_check.unwrap_or(false),
        },
    })
}

fn relative(path: &str, key: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if path.is_absolute() || path.has_root() {
        return Err(format!("`{key}` must be a relative path"));
    }
    Ok(path.to_path_buf())
}

fn supported() -> String {
    Target::ALL
        .iter()
        .map(|target| target.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
