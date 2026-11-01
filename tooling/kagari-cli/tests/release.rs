#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};

use common::{font_home, game, init_project, kagari, kagari_env, stderr, stdout};
use sha1::Sha1;
use sha2::{Digest, Sha256};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const RUNTIME: &[u8] = b"\x7fELF not really a runtime";
const SECTOR: usize = 2048;

/// A download server on localhost serving `files` (path → body); the paths it was asked for.
struct Server {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

fn serve(files: HashMap<String, Vec<u8>>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let path = line.split(' ').nth(1).unwrap_or_default().to_owned();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap() == 0 || header == "\r\n" {
                    break;
                }
            }
            log.lock().unwrap().push(path.clone());
            let (status, body) = match files.get(&path) {
                Some(body) => ("200 OK", body.as_slice()),
                None => ("404 Not Found", &b""[..]),
            };
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        }
    });
    Server { url, requests }
}

/// The server files of `file` (`bytes`) for `bucket`, with a `.sha256` of `checksum`.
fn runtime_files(
    bucket: &str,
    file: &str,
    bytes: &[u8],
    checksum: &[u8],
) -> HashMap<String, Vec<u8>> {
    let path = format!("/{bucket}/{VERSION}/{file}");
    let hex: String = Sha256::digest(checksum)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    HashMap::from([
        (
            format!("{path}.sha256"),
            format!("{hex}  {file}\n").into_bytes(),
        ),
        (path, bytes.to_vec()),
    ])
}

/// The libraries `kagari release --linux` ships next to the runtime.
const LINUX_LIBRARIES: &[&str] = &[
    "libavcodec.so.63",
    "libavformat.so.63",
    "libavutil.so.61",
    "libswscale.so.10",
    "libswresample.so.7",
    "ffmpeg-LICENSE.txt",
];
/// The libraries `kagari release --windows` ships next to the runtime.
const WINDOWS_LIBRARIES: &[&str] = &[
    "vcruntime140.dll",
    "avcodec-63.dll",
    "avformat-63.dll",
    "avutil-61.dll",
    "swscale-10.dll",
    "swresample-7.dll",
    "ffmpeg-LICENSE.txt",
];

/// The served contents of the library `name`.
fn library(name: &str) -> Vec<u8> {
    format!("not really {name}").into_bytes()
}

/// The server files of `libraries` for `bucket`, with matching checksums.
fn library_files(bucket: &str, libraries: &[&str]) -> HashMap<String, Vec<u8>> {
    let mut files = HashMap::new();
    for name in libraries {
        let bytes = library(name);
        files.extend(runtime_files(bucket, name, &bytes, &bytes));
    }
    files
}

fn linux_runtime() -> HashMap<String, Vec<u8>> {
    let file = format!("kagari-desktop-runtime-{VERSION}-x86_64-linux");
    let mut files = runtime_files("linux_x86_bin", &file, RUNTIME, RUNTIME);
    files.extend(library_files("linux_x86_bin", LINUX_LIBRARIES));
    files
}

fn release(dir: &Path, platform: &str, server: &Server) -> std::process::Output {
    kagari_env(
        &["release", platform],
        dir,
        &font_home(),
        &[("KAGARI_DOWNLOAD_URL", &server.url)],
    )
}

/// A directory record: identifier, system use area, extent, size.
struct Record {
    id: Vec<u8>,
    system_use: Vec<u8>,
    extent: usize,
    size: usize,
}

fn le32(bytes: &[u8]) -> usize {
    u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize
}

fn record(bytes: &[u8]) -> Record {
    let id_len = bytes[32] as usize;
    let id = bytes[33..33 + id_len].to_vec();
    let su_start = 33 + id_len + usize::from(id_len.is_multiple_of(2));
    Record {
        id,
        system_use: bytes[su_start..bytes[0] as usize].to_vec(),
        extent: le32(&bytes[2..]),
        size: le32(&bytes[10..]),
    }
}

/// The records of the directory `dir`, `.` and `..` first.
fn directory(iso: &[u8], dir: &Record) -> Vec<Record> {
    let data = &iso[dir.extent * SECTOR..dir.extent * SECTOR + dir.size];
    let mut records = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let len = data[at] as usize;
        if len == 0 {
            at = (at / SECTOR + 1) * SECTOR;
            continue;
        }
        records.push(record(&data[at..at + len]));
        at += len;
    }
    records
}

fn data<'a>(iso: &'a [u8], record: &Record) -> &'a [u8] {
    &iso[record.extent * SECTOR..record.extent * SECTOR + record.size]
}

/// The root directory of the Joliet tree: name → contents.
fn joliet_files(iso: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let svd = &iso[17 * SECTOR..18 * SECTOR];
    assert_eq!(&svd[..6], b"\x02CD001");
    assert_eq!(&svd[88..91], b"%/E");
    let root = record(&svd[156..190]);
    directory(iso, &root)
        .into_iter()
        .skip(2)
        .map(|file| {
            let units: Vec<u16> = file
                .id
                .chunks(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect();
            let name = String::from_utf16(&units).unwrap();
            let name = name.strip_suffix(";1").unwrap().to_owned();
            (name, data(iso, &file).to_vec())
        })
        .collect()
}

/// The root directory of the primary tree with its Rock Ridge names: name → (mode, contents).
fn rock_ridge_files(iso: &[u8]) -> BTreeMap<String, (u32, Vec<u8>)> {
    let pvd = &iso[16 * SECTOR..17 * SECTOR];
    assert_eq!(&pvd[..6], b"\x01CD001");
    let records = directory(iso, &record(&pvd[156..190]));
    assert_eq!(&records[0].system_use[..2], b"SP", "SUSP is announced");
    records
        .iter()
        .skip(2)
        .map(|file| {
            let (mut name, mut mode) = (None, None);
            let mut su = file.system_use.as_slice();
            while su.len() >= 4 {
                let len = su[2] as usize;
                match &su[..2] {
                    b"NM" => name = Some(String::from_utf8(su[5..len].to_vec()).unwrap()),
                    b"PX" => mode = Some(le32(&su[4..]) as u32),
                    _ => {}
                }
                su = &su[len..];
            }
            (name.unwrap(), (mode.unwrap(), data(iso, file).to_vec()))
        })
        .collect()
}

/// The runtime under `runtime` and the game files of `target/v0`.
fn expected_files(dir: &Path, runtime: &str) -> BTreeMap<String, Vec<u8>> {
    let mut files: BTreeMap<String, Vec<u8>> = std::fs::read_dir(dir.join("target/v0"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            (name, std::fs::read(entry.path()).unwrap())
        })
        .collect();
    files.insert(runtime.to_owned(), RUNTIME.to_vec());
    files
}

#[test]
fn linux_release_packages_the_game_an_executable_runtime_and_ffmpeg() {
    let tmp = init_project();
    let dir = game(&tmp);
    let server = serve(linux_runtime());
    let output = release(&dir, "--linux", &server);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("wrote target/release/linux/game.iso (10 files)"),
        "{}",
        stdout(&output)
    );

    let iso = std::fs::read(dir.join("target/release/linux/game.iso")).unwrap();
    let mut expected = expected_files(&dir, "kagari-desktop-runtime");
    for name in LINUX_LIBRARIES {
        expected.insert((*name).to_owned(), library(name));
    }
    assert_eq!(
        expected.keys().collect::<Vec<_>>(),
        [
            "characters.kiarchive",
            "ffmpeg-LICENSE.txt",
            "game.kagari",
            "kagari-desktop-runtime",
            "libavcodec.so.63",
            "libavformat.so.63",
            "libavutil.so.61",
            "libswresample.so.7",
            "libswscale.so.10",
            "system-fonts.kiarchive"
        ]
    );
    assert_eq!(joliet_files(&iso), expected);
    let rock_ridge = rock_ridge_files(&iso);
    for (name, bytes) in &expected {
        let (mode, data) = &rock_ridge[name];
        assert_eq!(data, bytes, "{name}");
        let expected_mode = if name == "kagari-desktop-runtime" {
            0o100_555
        } else {
            0o100_444
        };
        assert_eq!(*mode, expected_mode, "{name}");
    }
    assert_eq!(rock_ridge.len(), expected.len());
}

#[test]
fn windows_release_ships_run_exe_with_the_c_runtime_and_ffmpeg_and_clean_removes_it() {
    let tmp = init_project();
    let dir = game(&tmp);
    let file = format!("kagari-desktop-runtime-{VERSION}-x86_64-windows.exe");
    let mut files = runtime_files("windows_x86_installer", &file, RUNTIME, RUNTIME);
    files.extend(library_files("windows_x86_installer", WINDOWS_LIBRARIES));
    let server = serve(files);
    let output = release(&dir, "--windows", &server);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("wrote target/release/windows/game.iso (11 files)"),
        "{}",
        stdout(&output)
    );

    let iso = std::fs::read(dir.join("target/release/windows/game.iso")).unwrap();
    let mut expected = expected_files(&dir, "run.exe");
    for name in WINDOWS_LIBRARIES {
        expected.insert((*name).to_owned(), library(name));
    }
    assert_eq!(joliet_files(&iso), expected);

    let output = kagari(&["clean"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(!dir.join("target").exists());
}

#[test]
fn an_unpublished_c_runtime_writes_no_image() {
    let tmp = init_project();
    let dir = game(&tmp);
    let file = format!("kagari-desktop-runtime-{VERSION}-x86_64-windows.exe");
    let server = serve(runtime_files(
        "windows_x86_installer",
        &file,
        RUNTIME,
        RUNTIME,
    ));
    let output = release(&dir, "--windows", &server);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains(&format!(
            "/windows_x86_installer/{VERSION}/vcruntime140.dll`: not found"
        )),
        "{}",
        stderr(&output)
    );
    assert!(!dir.join("target/release").exists());
}

#[test]
fn an_unpublished_runtime_writes_no_image() {
    let tmp = init_project();
    let dir = game(&tmp);
    let server = serve(HashMap::new());
    let output = release(&dir, "--linux", &server);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains(&format!(
            "not found; the linux runtime of Kagari {VERSION} is not published"
        )),
        "{}",
        stderr(&output)
    );
    assert!(!dir.join("target/release").exists());
}

#[test]
fn a_runtime_that_fails_its_checksum_is_rejected() {
    let tmp = init_project();
    let dir = game(&tmp);
    let file = format!("kagari-desktop-runtime-{VERSION}-x86_64-linux");
    let server = serve(runtime_files(
        "linux_x86_bin",
        &file,
        RUNTIME,
        b"other bytes",
    ));
    let output = release(&dir, "--linux", &server);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("does not match its checksum"),
        "{}",
        stderr(&output)
    );
    assert!(!dir.join("target/release").exists());
}

#[test]
fn build_errors_stop_before_the_download() {
    let tmp = init_project();
    let dir = game(&tmp);
    std::fs::write(dir.join("src/main.kis"), "fn main() -> System { nope }\n").unwrap();
    let server = serve(linux_runtime());
    let output = release(&dir, "--linux", &server);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(server.requests.lock().unwrap().is_empty());
    assert!(!dir.join("target/release").exists());
}

/// The files `kagari release --macos` ships with the runtime.
const MACOS_LIBRARIES: &[&str] = &[
    "libavcodec.63.dylib",
    "libavformat.63.dylib",
    "libavutil.61.dylib",
    "libswscale.10.dylib",
    "libswresample.7.dylib",
    "ffmpeg-LICENSE.txt",
    "kagari.icns",
];
const IDENTIFIER: &str = "com.example.game";

/// A thin arm64 Mach-O file of `filetype` (2 executable, 6 dylib) as Apple's linker writes it:
/// `__TEXT` from the start, then `__LINKEDIT` ending in a placeholder ad-hoc signature.
fn macho(filetype: u32, seed: u8) -> Vec<u8> {
    const TEXT: u64 = 0x5000;
    const LINKEDIT_DATA: u64 = 0x100;
    const SIGNATURE: u64 = 0x20;
    let mut file = Vec::new();
    let le32 = |file: &mut Vec<u8>, v: u32| file.extend_from_slice(&v.to_le_bytes());
    let le64 = |file: &mut Vec<u8>, v: u64| file.extend_from_slice(&v.to_le_bytes());
    for word in [0xfeed_facf, 0x0100_000c, 0, filetype, 3, 72 + 72 + 16, 0, 0] {
        le32(&mut file, word);
    }
    let segment = |file: &mut Vec<u8>, name: &[u8], vmaddr: u64, fileoff: u64, size: u64| {
        le32(file, 0x19);
        le32(file, 72);
        let mut segname = [0u8; 16];
        segname[..name.len()].copy_from_slice(name);
        file.extend_from_slice(&segname);
        for value in [
            vmaddr,
            0x4000_u64.max(size.next_multiple_of(0x4000)),
            fileoff,
            size,
        ] {
            le64(file, value);
        }
        for value in [5, 5, 0, 0] {
            le32(file, value);
        }
    };
    segment(&mut file, b"__TEXT", 0, 0, TEXT);
    segment(
        &mut file,
        b"__LINKEDIT",
        0x8000,
        TEXT,
        LINKEDIT_DATA + SIGNATURE,
    );
    for word in [0x1d, 16, (TEXT + LINKEDIT_DATA) as u32, SIGNATURE as u32] {
        le32(&mut file, word);
    }
    while file.len() < (TEXT + LINKEDIT_DATA) as usize {
        file.push(seed.wrapping_add(file.len() as u8));
    }
    file.resize((TEXT + LINKEDIT_DATA + SIGNATURE) as usize, 0xcc);
    file
}

fn macos_runtime() -> HashMap<String, Vec<u8>> {
    let bucket = "macos_arm64_bin";
    let file = format!("kagari-desktop-runtime-{VERSION}-aarch64-macos");
    let runtime = macho(2, 1);
    let mut files = runtime_files(bucket, &file, &runtime, &runtime);
    for (seed, name) in MACOS_LIBRARIES.iter().enumerate() {
        let bytes = if name.ends_with(".dylib") {
            macho(6, seed as u8 + 2)
        } else {
            library(name)
        };
        files.extend(runtime_files(bucket, name, &bytes, &bytes));
    }
    files
}

fn set_identifier(dir: &Path) {
    let manifest = dir.join("kagari.toml");
    let text = std::fs::read_to_string(&manifest).unwrap().replace(
        "version = \"0.1.0\"",
        &format!("version = \"0.1.0\"\nidentifier = \"{IDENTIFIER}\""),
    );
    std::fs::write(manifest, text).unwrap();
}

/// The entries of a ZIP archive: name → (Unix mode, contents). Every local header must sit at
/// the offset the central directory records, with the same name, and every CRC must match.
fn zip_entries(zip: &[u8]) -> BTreeMap<String, (u32, Vec<u8>)> {
    let le16 = |at: usize| u16::from_le_bytes(zip[at..at + 2].try_into().unwrap()) as usize;
    let eocd = zip.len() - 22;
    assert_eq!(&zip[eocd..eocd + 4], b"PK\x05\x06");
    let count = le16(eocd + 10);
    let mut at = le32(&zip[eocd + 16..]);
    assert_eq!(
        at + le32(&zip[eocd + 12..]),
        eocd,
        "the central directory ends at the end record"
    );
    let central = at;
    let mut entries = BTreeMap::new();
    // Where each local entry starts and ends.
    let mut spans = Vec::new();
    for _ in 0..count {
        assert_eq!(&zip[at..at + 4], b"PK\x01\x02");
        let method = le16(at + 10);
        let crc = le32(&zip[at + 16..]) as u32;
        let compressed = le32(&zip[at + 20..]);
        let size = le32(&zip[at + 24..]);
        let name_len = le16(at + 28);
        let next = at + 46 + name_len + le16(at + 30) + le16(at + 32);
        let mode = (le32(&zip[at + 38..]) >> 16) as u32;
        let local = le32(&zip[at + 42..]);
        let name = std::str::from_utf8(&zip[at + 46..at + 46 + name_len]).unwrap();
        assert_eq!(&zip[local..local + 4], b"PK\x03\x04", "{name}");
        assert_eq!(&zip[local + 30..local + 30 + name_len], name.as_bytes());
        let start = local + 30 + le16(local + 26) + le16(local + 28);
        let stored = &zip[start..start + compressed];
        spans.push((local, start + compressed));
        let data = match method {
            0 => stored.to_vec(),
            8 => {
                let mut data = Vec::new();
                flate2::read::DeflateDecoder::new(stored)
                    .read_to_end(&mut data)
                    .unwrap();
                data
            }
            other => panic!("{name}: method {other}"),
        };
        assert_eq!(data.len(), size, "{name}");
        assert_eq!(crc32fast::hash(&data), crc, "{name}");
        entries.insert(name.to_owned(), (mode, data));
        at = next;
    }
    spans.sort();
    let mut expected_start = 0;
    for (start, end) in spans {
        assert_eq!(
            start, expected_start,
            "a gap or an overlap before offset {start}"
        );
        expected_start = end;
    }
    assert_eq!(
        expected_start, central,
        "the central directory follows the last entry"
    );
    entries
}

fn be32(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}

/// The code directory of the signed Mach-O `file`, after checking that the signature is the
/// last thing in the file and in `__LINKEDIT`.
fn code_directory(file: &[u8]) -> &[u8] {
    let linkedit = 32 + 72;
    let linkedit_end = le32(&file[linkedit + 40..]) + le32(&file[linkedit + 48..]);
    let signature = 32 + 72 + 72;
    let (offset, size) = (le32(&file[signature + 8..]), le32(&file[signature + 12..]));
    assert_eq!(offset + size, file.len());
    assert_eq!(linkedit_end, file.len());
    let superblob = &file[offset..];
    assert_eq!(be32(superblob, 0), 0xfade_0cc0);
    assert_eq!((be32(superblob, 12), be32(superblob, 20)), (0, 2));
    let cd = &superblob[be32(superblob, 16)..];
    assert_eq!(be32(cd, 0), 0xfade_0c02);
    &cd[..be32(cd, 4)]
}

/// Checks the ad-hoc code directory `cd` of `file`: its identifier, every page hash, and the
/// special slots `special` (slot 1 first).
fn check_signature(file: &[u8], cd: &[u8], identifier: &str, special: &[[u8; 32]]) {
    assert_eq!(be32(cd, 12) & 0x2, 0x2, "ad hoc");
    let (hash_offset, ident_offset) = (be32(cd, 16), be32(cd, 20));
    let (special_slots, code_slots, code_limit) = (be32(cd, 24), be32(cd, 28), be32(cd, 32));
    let ident_end = cd[ident_offset..].iter().position(|&b| b == 0).unwrap();
    assert_eq!(
        &cd[ident_offset..ident_offset + ident_end],
        identifier.as_bytes()
    );
    assert_eq!(code_limit, le32(&file[32 + 72 + 72 + 8..]));
    assert_eq!(code_slots, code_limit.div_ceil(4096));
    for (i, page) in file[..code_limit].chunks(4096).enumerate() {
        let at = hash_offset + i * 32;
        assert_eq!(
            &cd[at..at + 32],
            Sha256::digest(page).as_slice(),
            "page {i}"
        );
    }
    assert_eq!(special_slots, special.len());
    for (i, hash) in special.iter().enumerate() {
        let at = hash_offset - (i + 1) * 32;
        assert_eq!(&cd[at..at + 32], hash, "special slot {}", i + 1);
    }
}

/// The `<data>` after `<key>{key}</key>` (and, in a dict, after `<key>{field}</key>`), decoded.
fn plist_data(plist: &str, key: &str, field: Option<&str>) -> Vec<u8> {
    let mut rest = &plist[plist.find(&format!("<key>{key}</key>")).unwrap()..];
    if let Some(field) = field {
        rest = &rest[rest.find(&format!("<key>{field}</key>")).unwrap()..];
    }
    let start = rest.find("<data>").unwrap() + "<data>".len();
    let end = rest.find("</data>").unwrap();
    base64_decode(rest[start..end].trim())
}

fn base64_decode(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic!("not base64: {c}"),
    };
    let mut out = Vec::new();
    for chunk in text.as_bytes().chunks(4) {
        let digits: Vec<u8> = chunk
            .iter()
            .filter(|&&c| c != b'=')
            .map(|&c| value(c))
            .collect();
        let n = digits
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &d)| n | (u32::from(d) << (18 - 6 * i)));
        out.extend_from_slice(&n.to_be_bytes()[1..digits.len()]);
    }
    out
}

#[test]
fn macos_release_packages_a_signed_app_in_a_zip_and_clean_removes_it() {
    let tmp = init_project();
    let dir = game(&tmp);
    set_identifier(&dir);
    let server = serve(macos_runtime());
    let output = release(&dir, "--macos", &server);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("wrote target/release/macos/game.zip (13 files)"),
        "{}",
        stdout(&output)
    );

    let entries = zip_entries(&std::fs::read(dir.join("target/release/macos/game.zip")).unwrap());
    let contents = "game.app/Contents/";
    let mut expected_resources = expected_files(&dir, "unused");
    expected_resources.remove("unused");
    for name in ["ffmpeg-LICENSE.txt", "kagari.icns"] {
        expected_resources.insert(name.to_owned(), library(name));
    }
    let mut names: Vec<&str> = entries.keys().map(String::as_str).collect();
    names.retain(|name| !name.ends_with('/'));
    let mut expected_names: Vec<String> = expected_resources
        .keys()
        .map(|name| format!("{contents}Resources/{name}"))
        .collect();
    for name in &MACOS_LIBRARIES[..5] {
        expected_names.push(format!("{contents}Frameworks/{name}"));
    }
    for name in ["Info.plist", "MacOS/game", "_CodeSignature/CodeResources"] {
        expected_names.push(format!("{contents}{name}"));
    }
    expected_names.sort();
    assert_eq!(names, expected_names);
    for dir in [
        "game.app/",
        "game.app/Contents/",
        "game.app/Contents/MacOS/",
    ] {
        assert_eq!(entries[dir].0, 0o040_755, "{dir}");
    }
    for (name, bytes) in &expected_resources {
        let (mode, data) = &entries[&format!("{contents}Resources/{name}")];
        assert_eq!((*mode, data), (0o100_644, bytes), "{name}");
    }

    let info = &entries[&format!("{contents}Info.plist")].1;
    let info_text = std::str::from_utf8(info).unwrap();
    for pair in [
        ("CFBundleExecutable", "game"),
        ("CFBundleIdentifier", IDENTIFIER),
        ("CFBundleName", "game"),
        ("CFBundleShortVersionString", "0.1.0"),
        ("CFBundleIconFile", "kagari.icns"),
    ] {
        let entry = format!("<key>{}</key>\n\t<string>{}</string>", pair.0, pair.1);
        assert!(info_text.contains(&entry), "{entry}\n{info_text}");
    }
    let code_resources = &entries[&format!("{contents}_CodeSignature/CodeResources")].1;
    let resources_text = std::str::from_utf8(code_resources).unwrap();
    let files2 = &resources_text[resources_text.find("<key>files2</key>").unwrap()..];
    for (name, bytes) in &expected_resources {
        let key = format!("Resources/{name}");
        assert_eq!(
            plist_data(resources_text, &key, None),
            Sha1::digest(bytes).as_slice()
        );
        assert_eq!(
            plist_data(files2, &key, Some("hash2")),
            Sha256::digest(bytes).as_slice()
        );
    }

    // Each dylib is signed under its name; CodeResources records its cdhash.
    for name in &MACOS_LIBRARIES[..5] {
        let (mode, dylib) = &entries[&format!("{contents}Frameworks/{name}")];
        assert_eq!(*mode, 0o100_755);
        let cd = code_directory(dylib);
        let requirements = [0xfa, 0xde, 0x0c, 0x01, 0, 0, 0, 12, 0, 0, 0, 0];
        let empty = [0; 32];
        let identifier = name.trim_end_matches(".dylib");
        check_signature(
            dylib,
            cd,
            identifier,
            &[empty, Sha256::digest(requirements).into()],
        );
        let cdhash = plist_data(files2, &format!("Frameworks/{name}"), Some("cdhash"));
        assert_eq!(cdhash, &Sha256::digest(cd)[..20], "{name}");
    }

    // The executable binds Info.plist and CodeResources.
    let (mode, runtime) = &entries[&format!("{contents}MacOS/game")];
    assert_eq!(*mode, 0o100_755);
    let cd = code_directory(runtime);
    let requirements = [0xfa, 0xde, 0x0c, 0x01, 0, 0, 0, 12, 0, 0, 0, 0];
    check_signature(
        runtime,
        cd,
        IDENTIFIER,
        &[
            Sha256::digest(info).into(),
            Sha256::digest(requirements).into(),
            Sha256::digest(code_resources).into(),
        ],
    );
    let original = macho(2, 1);
    let code_limit = le32(&original[32 + 72 + 72 + 8..]);
    assert_eq!(runtime[0x200..code_limit], original[0x200..code_limit]);

    let output = kagari(&["clean"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(!dir.join("target").exists());
}

#[test]
fn a_macos_release_without_an_identifier_downloads_nothing() {
    let tmp = init_project();
    let dir = game(&tmp);
    let server = serve(macos_runtime());
    let output = release(&dir, "--macos", &server);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("needs `project.identifier`"),
        "{}",
        stderr(&output)
    );
    assert!(server.requests.lock().unwrap().is_empty());
    assert!(!dir.join("target/release").exists());
}

#[test]
fn a_macos_runtime_that_is_not_mach_o_writes_no_archive() {
    let tmp = init_project();
    let dir = game(&tmp);
    set_identifier(&dir);
    let mut files = macos_runtime();
    files.extend(runtime_files(
        "macos_arm64_bin",
        &format!("kagari-desktop-runtime-{VERSION}-aarch64-macos"),
        RUNTIME,
        RUNTIME,
    ));
    let server = serve(files);
    let output = release(&dir, "--macos", &server);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("cannot sign the runtime: not a thin arm64 Mach-O file"),
        "{}",
        stderr(&output)
    );
    let release_dir = dir.join("target/release/macos");
    assert_eq!(std::fs::read_dir(release_dir).unwrap().count(), 0);
}
