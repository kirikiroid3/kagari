#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};

use common::{font_home, game, init_project, kagari, kagari_env, stderr, stdout};
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

fn linux_runtime() -> HashMap<String, Vec<u8>> {
    let file = format!("kagari-desktop-runtime-{VERSION}-x86_64-linux");
    runtime_files("linux_x86_bin", &file, RUNTIME, RUNTIME)
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
fn linux_release_packages_the_game_and_an_executable_runtime() {
    let tmp = init_project();
    let dir = game(&tmp);
    let server = serve(linux_runtime());
    let output = release(&dir, "--linux", &server);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("wrote target/release/linux/game.iso (4 files)"),
        "{}",
        stdout(&output)
    );

    let iso = std::fs::read(dir.join("target/release/linux/game.iso")).unwrap();
    let expected = expected_files(&dir, "kagari-desktop-runtime");
    assert_eq!(
        expected.keys().collect::<Vec<_>>(),
        [
            "characters.kiarchive",
            "game.kagari",
            "kagari-desktop-runtime",
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
fn windows_release_ships_run_exe_with_the_c_runtime_and_clean_removes_it() {
    let tmp = init_project();
    let dir = game(&tmp);
    let dll = b"MZ not really vcruntime140.dll";
    let file = format!("kagari-desktop-runtime-{VERSION}-x86_64-windows.exe");
    let mut files = runtime_files("windows_x86_installer", &file, RUNTIME, RUNTIME);
    files.extend(runtime_files(
        "windows_x86_installer",
        "vcruntime140.dll",
        dll,
        dll,
    ));
    let server = serve(files);
    let output = release(&dir, "--windows", &server);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("wrote target/release/windows/game.iso (5 files)"),
        "{}",
        stdout(&output)
    );

    let iso = std::fs::read(dir.join("target/release/windows/game.iso")).unwrap();
    let mut expected = expected_files(&dir, "run.exe");
    expected.insert("vcruntime140.dll".to_owned(), dll.to_vec());
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
