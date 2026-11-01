//! `kagari release`: builds the project in the release profile and packages the game with the
//! desktop runtime of one platform, downloaded for this toolchain version: into the image
//! `<out-dir>/release/<platform>/<name>.iso` for Windows and Linux, into the app bundle
//! `<name>.app` in the archive `<out-dir>/release/macos/<name>.zip` for macOS.

mod download;
mod iso;
mod macos;
mod zip;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::cli::Profile;
use crate::command::build::build_project;
use crate::failure::{Failure, exit, io};
use crate::outputs::{game_files, release_dir, remove_packages};
use crate::workspace::{self, shown};
use iso::{IsoError, IsoFile, Source};
use macos::{App, AppError};

/// A platform a release is for: the runtime it ships.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
    MacOs,
}

impl Platform {
    pub const ALL: &[Platform] = &[Platform::Windows, Platform::Linux, Platform::MacOs];

    /// The name of the release directory and in messages.
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Windows => "windows",
            Platform::Linux => "linux",
            Platform::MacOs => "macos",
        }
    }

    /// The download server's directory of this platform's builds.
    fn bucket_dir(self) -> &'static str {
        match self {
            Platform::Windows => "windows_x86_installer",
            Platform::Linux => "linux_x86_bin",
            Platform::MacOs => "macos_arm64_bin",
        }
    }

    /// The runtime's file name on the download server.
    fn runtime_file(self, version: &str) -> String {
        match self {
            Platform::Windows => format!("kagari-desktop-runtime-{version}-x86_64-windows.exe"),
            Platform::Linux => format!("kagari-desktop-runtime-{version}-x86_64-linux"),
            Platform::MacOs => format!("kagari-desktop-runtime-{version}-aarch64-macos"),
        }
    }

    /// The extension of the package `kagari release` writes.
    fn package_extension(self) -> &'static str {
        match self {
            Platform::Windows | Platform::Linux => "iso",
            Platform::MacOs => "zip",
        }
    }

    /// The libraries the runtime needs and a player's system may lack, downloaded from the
    /// runtime's directory on the server and shipped next to it under the same name, and
    /// FFmpeg's licence (and on macOS the app's icon).
    fn libraries(self) -> &'static [&'static str] {
        match self {
            // The MSVC C runtime and FFmpeg: Windows loads DLLs from the executable's
            // directory first.
            Platform::Windows => &[
                "vcruntime140.dll",
                "avcodec-63.dll",
                "avformat-63.dll",
                "avutil-61.dll",
                "swscale-10.dll",
                "swresample-7.dll",
                "ffmpeg-LICENSE.txt",
            ],
            // FFmpeg: the runtime's RUNPATH is `$ORIGIN`.
            Platform::Linux => &[
                "libavcodec.so.63",
                "libavformat.so.63",
                "libavutil.so.61",
                "libswscale.so.10",
                "libswresample.so.7",
                "ffmpeg-LICENSE.txt",
            ],
            // FFmpeg, in the app's `Frameworks` (an rpath of the runtime); the rest goes to
            // `Resources`.
            Platform::MacOs => &[
                "libavcodec.63.dylib",
                "libavformat.63.dylib",
                "libavutil.61.dylib",
                "libswscale.10.dylib",
                "libswresample.7.dylib",
                "ffmpeg-LICENSE.txt",
                macos::ICON,
            ],
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
    let identifier = match platform {
        Platform::MacOs => Some(project.manifest.identifier.as_deref().ok_or_else(|| {
            Failure(
                "a macOS release needs `project.identifier` in kagari.toml (a reverse-DNS name such as `com.example.game`)"
                    .to_owned(),
            )
        })?),
        Platform::Windows | Platform::Linux => None,
    };
    if build_project(&project, &[target], Profile::Release)? {
        return Ok(exit(true));
    }

    let runtime = download::runtime(platform)?;
    let mut libraries = Vec::new();
    for &library in platform.libraries() {
        libraries.push((library, download::file(platform, library)?));
    }
    let game = game_files(&project.out_dir(target))?;

    let dir = release_dir(&project.build_dir(), platform.as_str());
    std::fs::create_dir_all(&dir).map_err(|error| io(&dir, error))?;
    remove_packages(&dir)?;
    let name = file_name(&project.manifest.name);
    let extension = platform.package_extension();
    let package = dir.join(format!("{name}.{extension}"));
    let partial = dir.join(format!("{name}.{extension}.part"));
    let written = match identifier {
        Some(identifier) => write_app(
            &partial,
            &App {
                file_name: &name,
                display_name: &project.manifest.name,
                identifier,
                version: &project.manifest.version,
                runtime: &runtime,
                shipped: &libraries,
                game: &game,
            },
        ),
        None => write_image(
            &partial,
            &project.manifest.name,
            &iso_files(platform == Platform::Windows, runtime, libraries, &game)?,
        ),
    }
    .and_then(|files| {
        std::fs::rename(&partial, &package).map_err(|error| io(&package, error))?;
        Ok(files)
    });
    let files = match written {
        Ok(files) => files,
        Err(failure) => {
            let _ = std::fs::remove_file(&partial);
            return Err(failure);
        }
    };
    println!("wrote {} ({files} files)", shown(&package, &project.root));
    Ok(ExitCode::SUCCESS)
}

/// The root directory of a Windows or Linux image: the runtime under the name players start
/// (`run.exe` on Windows), the libraries next to it, then the game files.
fn iso_files(
    windows: bool,
    runtime: Vec<u8>,
    libraries: Vec<(&str, Vec<u8>)>,
    game: &[PathBuf],
) -> Result<Vec<IsoFile>, Failure> {
    let runtime_name = if windows {
        "run.exe"
    } else {
        "kagari-desktop-runtime"
    };
    let mut files = vec![IsoFile {
        name: runtime_name.to_owned(),
        source: Source::Bytes(runtime),
        executable: true,
    }];
    for (library, bytes) in libraries {
        files.push(IsoFile {
            name: library.to_owned(),
            source: Source::Bytes(bytes),
            executable: false,
        });
    }
    for path in game {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| Failure(format!("{}: not a UTF-8 file name", path.display())))?
            .to_owned();
        files.push(IsoFile {
            name,
            source: Source::Disk(path.clone()),
            executable: false,
        });
    }
    Ok(files)
}

/// Writes the archive of `app` to `path`; returns how many files the app has.
fn write_app(path: &Path, app: &App<'_>) -> Result<usize, Failure> {
    let file = File::create(path).map_err(|error| io(path, error))?;
    let (mut out, files) =
        macos::write(BufWriter::new(file), app).map_err(|error| match error {
            AppError::Sign(name, why) => Failure(format!("cannot sign {name}: {why}")),
            AppError::Read(source, error) => io(&source, error),
            AppError::Write(error) => io(path, error),
        })?;
    out.flush().map_err(|error| io(path, error))?;
    out.get_ref().sync_all().map_err(|error| io(path, error))?;
    Ok(files)
}

/// Writes the image of `files`, labelled `volume`, to `path`; returns how many files it has.
fn write_image(path: &Path, volume: &str, files: &[IsoFile]) -> Result<usize, Failure> {
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
    out.get_ref().sync_all().map_err(|error| io(path, error))?;
    Ok(files.len())
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
