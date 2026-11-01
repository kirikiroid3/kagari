//! `kagari release --macos`: the game as an app bundle `<name>.app`, signed ad hoc, in a ZIP
//! archive.
//!
//! ```text
//! <name>.app/Contents/
//!   Info.plist                   the bundle's identity; the executable binds its hash
//!   MacOS/<name>                 the runtime, signed with the bundle bound to it
//!   Frameworks/*.dylib           FFmpeg, each signed (nested code)
//!   Resources/                   game.kagari, *.kiarchive, the icon, FFmpeg's licence
//!   _CodeSignature/CodeResources the hashes of Resources/ and the cdhashes of Frameworks/
//! ```
//!
//! The runtime finds the game in `Contents/Resources` and FFmpeg through its rpath
//! `@executable_path/../Frameworks`.

mod sign;

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

use sha1::Sha1;
use sha2::{Digest, Sha256};

use super::zip::{Method, ZipWriter};
use sign::{Bundle, SignError};

/// The icon file shipped with the runtime; `Info.plist` names it.
pub const ICON: &str = "kagari.icns";
/// The oldest macOS the runtime runs on: the first with Apple silicon.
const MINIMUM_SYSTEM_VERSION: &str = "11.0";
const EXECUTABLE_MODE: u32 = 0o755;
const FILE_MODE: u32 = 0o644;

/// What goes into the app.
pub struct App<'a> {
    /// The bundle's name without `.app`, also the executable's.
    pub file_name: &'a str,
    /// The name players see (`CFBundleName`).
    pub display_name: &'a str,
    /// `CFBundleIdentifier`, also the executable's signing identifier.
    pub identifier: &'a str,
    pub version: &'a semver::Version,
    pub runtime: &'a [u8],
    /// The files shipped with the runtime: dylibs go to `Frameworks`, the others to `Resources`.
    pub shipped: &'a [(&'a str, Vec<u8>)],
    /// The game files, which go to `Resources`.
    pub game: &'a [PathBuf],
}

/// Why the archive cannot be written.
pub enum AppError {
    /// A shipped binary cannot be signed: its name and why.
    Sign(String, String),
    /// A game file cannot be read.
    Read(PathBuf, io::Error),
    /// The archive cannot be written.
    Write(io::Error),
}

/// Writes the archive holding the app to `out`; returns how many files the app has.
pub fn write<W: Write + Seek>(out: W, app: &App<'_>) -> Result<(W, usize), AppError> {
    let root = format!("{}.app/Contents/", app.file_name);
    let mut zip = ZipWriter::new(out);
    for dir in [
        format!("{}.app/", app.file_name),
        root.clone(),
        format!("{root}MacOS/"),
        format!("{root}Frameworks/"),
        format!("{root}Resources/"),
        format!("{root}_CodeSignature/"),
    ] {
        zip.directory(&dir).map_err(AppError::Write)?;
    }

    let mut resources = Vec::new();
    for path in app.game {
        let name = file_name(path);
        let file = File::open(path).map_err(|error| AppError::Read(path.clone(), error))?;
        let size = file
            .metadata()
            .map_err(|error| AppError::Read(path.clone(), error))?
            .len();
        // Asset archives are compressed already.
        let method = if path
            .extension()
            .is_some_and(|ext| ext == kagari_archive::archive::ARCHIVE_EXTENSION)
        {
            Method::Stored
        } else {
            Method::Deflated
        };
        let mut reader = Hashing::new(file);
        let written = zip.file(
            &format!("{root}Resources/{name}"),
            FILE_MODE,
            method,
            size,
            &mut reader,
        );
        if let Err(error) = written {
            return Err(if reader.failed {
                AppError::Read(path.clone(), error)
            } else {
                AppError::Write(error)
            });
        }
        resources.push(reader.finish(format!("Resources/{name}")));
    }

    let mut nested = Vec::new();
    for (name, bytes) in app.shipped {
        if let Some(stem) = name.strip_suffix(".dylib") {
            let signed = sign::sign(bytes, stem, None)
                .map_err(|SignError(why)| AppError::Sign((*name).to_owned(), why))?;
            add_bytes(
                &mut zip,
                &format!("{root}Frameworks/{name}"),
                EXECUTABLE_MODE,
                &signed.bytes,
            )?;
            nested.push((format!("Frameworks/{name}"), signed.cdhash));
        } else {
            add_bytes(
                &mut zip,
                &format!("{root}Resources/{name}"),
                FILE_MODE,
                bytes,
            )?;
            let mut hashing = Hashing::new(bytes.as_slice());
            io::copy(&mut hashing, &mut io::sink()).map_err(AppError::Write)?;
            resources.push(hashing.finish(format!("Resources/{name}")));
        }
    }

    let code_resources = code_resources(&mut resources, &mut nested).into_bytes();
    let info_plist = info_plist(app).into_bytes();
    let signed = sign::sign(
        app.runtime,
        app.identifier,
        Some(&Bundle {
            info_plist: &info_plist,
            code_resources: &code_resources,
        }),
    )
    .map_err(|SignError(why)| AppError::Sign("the runtime".to_owned(), why))?;
    add_bytes(
        &mut zip,
        &format!("{root}Info.plist"),
        FILE_MODE,
        &info_plist,
    )?;
    add_bytes(
        &mut zip,
        &format!("{root}_CodeSignature/CodeResources"),
        FILE_MODE,
        &code_resources,
    )?;
    add_bytes(
        &mut zip,
        &format!("{root}MacOS/{}", app.file_name),
        EXECUTABLE_MODE,
        &signed.bytes,
    )?;
    let files = app
        .game
        .len()
        .saturating_add(app.shipped.len())
        .saturating_add(3);
    let out = zip.finish().map_err(AppError::Write)?;
    Ok((out, files))
}

fn add_bytes<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    mode: u32,
    bytes: &[u8],
) -> Result<(), AppError> {
    zip.file(
        name,
        mode,
        Method::Deflated,
        bytes.len() as u64,
        &mut &bytes[..],
    )
    .map_err(AppError::Write)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A resource's path in `Contents` with its SHA-1 (`CodeResources` version 1) and SHA-256
/// (version 2).
struct Resource {
    path: String,
    sha1: [u8; 20],
    sha256: [u8; 32],
}

/// A reader that hashes what it reads.
struct Hashing<R> {
    inner: R,
    sha1: Sha1,
    sha256: Sha256,
    /// Whether reading `inner` failed.
    failed: bool,
}

impl<R: Read> Hashing<R> {
    fn new(inner: R) -> Self {
        Hashing {
            inner,
            sha1: Sha1::new(),
            sha256: Sha256::new(),
            failed: false,
        }
    }

    fn finish(self, path: String) -> Resource {
        Resource {
            path,
            sha1: self.sha1.finalize().into(),
            sha256: self.sha256.finalize().into(),
        }
    }
}

impl<R: Read> Read for Hashing<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let n = self
            .inner
            .read(buffer)
            .inspect_err(|_| self.failed = true)?;
        if let Some(read) = buffer.get(..n) {
            self.sha1.update(read);
            self.sha256.update(read);
        }
        Ok(n)
    }
}

/// `Contents/Info.plist`.
fn info_plist(app: &App<'_>) -> String {
    let version = format!(
        "{}.{}.{}",
        app.version.major, app.version.minor, app.version.patch
    );
    let mut plist = String::from(PLIST_HEADER);
    plist.push_str("<dict>\n");
    for (key, value) in [
        ("CFBundleDevelopmentRegion", "en"),
        ("CFBundleDisplayName", app.display_name),
        ("CFBundleExecutable", app.file_name),
        ("CFBundleIconFile", ICON),
        ("CFBundleIdentifier", app.identifier),
        ("CFBundleInfoDictionaryVersion", "6.0"),
        ("CFBundleName", app.display_name),
        ("CFBundlePackageType", "APPL"),
        ("CFBundleShortVersionString", &version),
        ("CFBundleVersion", &version),
        ("LSApplicationCategoryType", "public.app-category.games"),
        ("LSMinimumSystemVersion", MINIMUM_SYSTEM_VERSION),
    ] {
        let _ = writeln!(
            plist,
            "\t<key>{key}</key>\n\t<string>{}</string>",
            escape(value)
        );
    }
    plist.push_str("\t<key>NSHighResolutionCapable</key>\n\t<true/>\n");
    plist.push_str("</dict>\n</plist>\n");
    plist
}

/// `Contents/_CodeSignature/CodeResources`, as `codesign` writes it: version 1 (`files`,
/// `rules`) hashes `Resources` with SHA-1; version 2 (`files2`, `rules2`) hashes it with
/// SHA-256 and records each nested binary's cdhash, whose designated requirement (ad hoc) is
/// that cdhash.
fn code_resources(resources: &mut [Resource], nested: &mut [(String, [u8; 20])]) -> String {
    resources.sort_by(|a, b| a.path.cmp(&b.path));
    nested.sort_by(|a, b| a.0.cmp(&b.0));
    let mut plist = String::from(PLIST_HEADER);
    plist.push_str("<dict>\n\t<key>files</key>\n\t<dict>\n");
    for resource in resources.iter() {
        let _ = writeln!(
            plist,
            "\t\t<key>{}</key>\n\t\t<data>\n\t\t{}\n\t\t</data>",
            escape(&resource.path),
            base64(&resource.sha1)
        );
    }
    plist.push_str("\t</dict>\n\t<key>files2</key>\n\t<dict>\n");
    // Both lists are sorted; files2 interleaves them in key order.
    let mut entries: Vec<(&str, String)> = Vec::new();
    for (path, cdhash) in nested.iter() {
        let hex: String = cdhash.iter().map(|byte| format!("{byte:02x}")).collect();
        entries.push((
            path,
            format!(
                "\t\t<dict>\n\t\t\t<key>cdhash</key>\n\t\t\t<data>\n\t\t\t{}\n\t\t\t</data>\n\t\t\t<key>requirement</key>\n\t\t\t<string>cdhash H\"{hex}\"</string>\n\t\t</dict>\n",
                base64(cdhash)
            ),
        ));
    }
    for resource in resources.iter() {
        entries.push((
            &resource.path,
            format!(
                "\t\t<dict>\n\t\t\t<key>hash2</key>\n\t\t\t<data>\n\t\t\t{}\n\t\t\t</data>\n\t\t</dict>\n",
                base64(&resource.sha256)
            ),
        ));
    }
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (path, value) in entries {
        let _ = write!(plist, "\t\t<key>{}</key>\n{value}", escape(path));
    }
    plist.push_str("\t</dict>\n");
    plist.push_str(RULES);
    plist.push_str("</dict>\n</plist>\n");
    plist
}

const PLIST_HEADER: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
    "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
    "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
    "<plist version=\"1.0\">\n",
);

/// `codesign`'s default resource rules for an app bundle.
const RULES: &str = r#"	<key>rules</key>
	<dict>
		<key>^Resources/</key>
		<true/>
		<key>^Resources/.*\.lproj/</key>
		<dict>
			<key>optional</key>
			<true/>
			<key>weight</key>
			<real>1000</real>
		</dict>
		<key>^Resources/.*\.lproj/locversion.plist$</key>
		<dict>
			<key>omit</key>
			<true/>
			<key>weight</key>
			<real>1100</real>
		</dict>
		<key>^Resources/Base\.lproj/</key>
		<dict>
			<key>weight</key>
			<real>1010</real>
		</dict>
		<key>^version.plist$</key>
		<true/>
	</dict>
	<key>rules2</key>
	<dict>
		<key>.*\.dSYM($|/)</key>
		<dict>
			<key>weight</key>
			<real>11</real>
		</dict>
		<key>^(.*/)?\.DS_Store$</key>
		<dict>
			<key>omit</key>
			<true/>
			<key>weight</key>
			<real>2000</real>
		</dict>
		<key>^(Frameworks|SharedFrameworks|PlugIns|Plug-ins|XPCServices|Helpers|MacOS|Library/(Automator|Spotlight|LoginItems))/</key>
		<dict>
			<key>nested</key>
			<true/>
			<key>weight</key>
			<real>10</real>
		</dict>
		<key>^.*</key>
		<true/>
		<key>^Info\.plist$</key>
		<dict>
			<key>omit</key>
			<true/>
			<key>weight</key>
			<real>20</real>
		</dict>
		<key>^PkgInfo$</key>
		<dict>
			<key>omit</key>
			<true/>
			<key>weight</key>
			<real>20</real>
		</dict>
		<key>^Resources/</key>
		<dict>
			<key>weight</key>
			<real>20</real>
		</dict>
		<key>^Resources/.*\.lproj/</key>
		<dict>
			<key>optional</key>
			<true/>
			<key>weight</key>
			<real>1000</real>
		</dict>
		<key>^Resources/.*\.lproj/locversion.plist$</key>
		<dict>
			<key>omit</key>
			<true/>
			<key>weight</key>
			<real>1100</real>
		</dict>
		<key>^Resources/Base\.lproj/</key>
		<dict>
			<key>weight</key>
			<real>1010</real>
		</dict>
		<key>^[^/]+$</key>
		<dict>
			<key>nested</key>
			<true/>
			<key>weight</key>
			<real>10</real>
		</dict>
		<key>^embedded\.provisionprofile$</key>
		<dict>
			<key>weight</key>
			<real>20</real>
		</dict>
		<key>^version\.plist$</key>
		<dict>
			<key>weight</key>
			<real>20</real>
		</dict>
	</dict>
"#;

/// `text` with the XML special characters escaped.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            c => escaped.push(c),
        }
    }
    escaped
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
    for chunk in bytes.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                let index = ((n >> shift) & 0x3f) as usize;
                out.push(char::from(ALPHABET.get(index).copied().unwrap_or(b'=')));
            } else {
                out.push('=');
            }
        }
    }
    out
}
