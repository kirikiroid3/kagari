//! Downloads the desktop runtime of this toolchain version from the download server, checked
//! against its `.sha256` file.

use kagari_project::TOOLCHAIN_VERSION;
use sha2::{Digest, Sha256};

use super::Platform;
use crate::failure::Failure;

/// The download server, where the release workflow uploads every build.
const DOWNLOAD_URL: &str = "https://kagari-download.suitsu31.club";
/// The environment variable that replaces [`DOWNLOAD_URL`] (as for `installer.sh`).
const DOWNLOAD_URL_VAR: &str = "KAGARI_DOWNLOAD_URL";
/// The largest runtime accepted.
const MAX_RUNTIME_SIZE: u64 = 1 << 30;
/// The largest `.sha256` file accepted.
const MAX_CHECKSUM_SIZE: u64 = 4096;

/// The bytes of the `platform` runtime of [`TOOLCHAIN_VERSION`].
pub fn runtime(platform: Platform) -> Result<Vec<u8>, Failure> {
    let base = std::env::var(DOWNLOAD_URL_VAR)
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DOWNLOAD_URL.to_owned());
    let url = format!(
        "{}/{}/{TOOLCHAIN_VERSION}/{}",
        base.trim_end_matches('/'),
        platform.bucket_dir(),
        platform.runtime_file(TOOLCHAIN_VERSION),
    );
    println!("downloading {url}");
    let agent = ureq::Agent::new_with_defaults();
    let runtime = get(&agent, &url, MAX_RUNTIME_SIZE, platform)?;
    let checksum_url = format!("{url}.sha256");
    let checksum = get(&agent, &checksum_url, MAX_CHECKSUM_SIZE, platform)?;
    let expected = std::str::from_utf8(&checksum)
        .ok()
        .and_then(|text| text.split_whitespace().next())
        .ok_or_else(|| Failure(format!("`{checksum_url}` is not a checksum file")))?;
    let actual: String = Sha256::digest(&runtime)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(Failure(format!(
            "the download of `{url}` does not match its checksum (expected {expected}, got {actual})"
        )));
    }
    Ok(runtime)
}

/// The body of `url`, at most `limit` bytes.
fn get(agent: &ureq::Agent, url: &str, limit: u64, platform: Platform) -> Result<Vec<u8>, Failure> {
    let failure = |error: ureq::Error| {
        Failure(match error {
            ureq::Error::StatusCode(404) => format!(
                "cannot download `{url}`: not found; the {} runtime of Kagari {TOOLCHAIN_VERSION} is not published",
                platform.as_str()
            ),
            error => format!("cannot download `{url}`: {error}"),
        })
    };
    agent
        .get(url)
        .call()
        .map_err(failure)?
        .into_body()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(failure)
}
