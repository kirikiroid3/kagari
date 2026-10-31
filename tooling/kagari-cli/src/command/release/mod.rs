//! `kagari release`: builds the project in the release profile and packages the game with the
//! desktop runtime of one platform, downloaded for this toolchain version, into the image
//! `<out-dir>/release/<platform>/<name>.iso`.

mod download;
mod iso;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;

use crate::cli::Profile;
use crate::command::build::build_project;
use crate::failure::{Failure, exit, io};
use crate::outputs::{IMAGE_EXTENSION, game_files, release_dir, remove_images};
use crate::workspace::{self, shown};
use iso::{IsoError, IsoFile, Source};

/// A platform a release is for: the runtime it ships.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
}

impl Platform {
    pub const ALL: &[Platform] = &[Platform::Windows, Platform::Linux];

    /// The name of the release directory and in messages.
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Windows => "windows",
            Platform::Linux => "linux",
        }
    }

    /// The download server's directory of this platform's builds.
    fn bucket_dir(self) -> &'static str {
        match self {
            Platform::Windows => "windows_x86_installer",
            Platform::Linux => "linux_x86_bin",
        }
    }

    /// The runtime's file name on the download server.
    fn runtime_file(self, version: &str) -> String {
        match self {
            Platform::Windows => format!("kagari-desktop-runtime-{version}-x86_64-windows.exe"),
            Platform::Linux => format!("kagari-desktop-runtime-{version}-x86_64-linux"),
        }
    }

    /// The runtime's file name in the image: what a player starts.
    fn runtime_name(self) -> &'static str {
        match self {
            Platform::Windows => "run.exe",
            Platform::Linux => "kagari-desktop-runtime",
        }
    }

    /// The libraries the runtime needs and a player's system may lack, downloaded from the
    /// runtime's directory on the server and shipped next to it under the same name.
    fn libraries(self) -> &'static [&'static str] {
        match self {
            // The MSVC C runtime: Windows loads it from the executable's directory first.
            Platform::Windows => &["vcruntime140.dll"],
            Platform::Linux => &[],
        }
    }
}

pub fn run(manifest_path: Option<&Path>, platform: Platform) -> Result<ExitCode, Failure> {
    let project = workspace::load(manifest_path)?;
    let target = *project
        .manifest
        .targets
        .first()
        .ok_or_else(|| Failure("`build.targets` is empty".to_owned()))?;
    if build_project(&project, &[target], Profile::Release)? {
        return Ok(exit(true));
    }

    let runtime = download::runtime(platform)?;
    let mut files = vec![IsoFile {
        name: platform.runtime_name().to_owned(),
        source: Source::Bytes(runtime),
        executable: true,
    }];
    for &library in platform.libraries() {
        files.push(IsoFile {
            name: library.to_owned(),
            source: Source::Bytes(download::file(platform, library)?),
            executable: false,
        });
    }
    for path in game_files(&project.out_dir(target))? {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Failure(format!("{}: not a UTF-8 file name", path.display())))?
            .to_owned();
        files.push(IsoFile {
            name,
            source: Source::Disk(path),
            executable: false,
        });
    }

    let dir = release_dir(&project.build_dir(), platform.as_str());
    std::fs::create_dir_all(&dir).map_err(|error| io(&dir, error))?;
    remove_images(&dir)?;
    let name = file_name(&project.manifest.name);
    let image = dir.join(format!("{name}.{IMAGE_EXTENSION}"));
    let partial = dir.join(format!("{name}.{IMAGE_EXTENSION}.part"));
    let written = write_image(&partial, &project.manifest.name, &files)
        .and_then(|()| std::fs::rename(&partial, &image).map_err(|error| io(&image, error)));
    if let Err(failure) = written {
        let _ = std::fs::remove_file(&partial);
        return Err(failure);
    }
    println!(
        "wrote {} ({} files)",
        shown(&image, &project.root),
        files.len()
    );
    Ok(ExitCode::SUCCESS)
}

/// Writes the image of `files`, labelled `volume`, to `path`.
fn write_image(path: &Path, volume: &str, files: &[IsoFile]) -> Result<(), Failure> {
    let file = File::create(path).map_err(|error| io(path, error))?;
    let mut out = BufWriter::new(file);
    iso::write(&mut out, volume, files).map_err(|error| match error {
        IsoError::Name(message) | IsoError::TooLarge(message) => {
            Failure(format!("cannot write the image: {message}"))
        }
        IsoError::Read(source, error) => io(&source, error),
        IsoError::Write(error) => io(path, error),
    })?;
    out.flush().map_err(|error| io(path, error))?;
    out.get_ref().sync_all().map_err(|error| io(path, error))
}

/// The project name as a file name on every platform: path and reserved characters and
/// controls become `_`, surrounding whitespace and trailing dots go, and an empty result is
/// `Kagari Game` (the rule of the runtime's data directory).
fn file_name(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = replaced.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "Kagari Game".to_owned()
    } else {
        trimmed.to_owned()
    }
}
