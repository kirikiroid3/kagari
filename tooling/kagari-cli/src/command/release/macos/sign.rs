//! Ad-hoc code signing of thin arm64 Mach-O files, as `codesign --sign -` does: no identity, no
//! certificate, the code directory's hash (cdhash) is the signature. A file must already carry a
//! signature (Apple's linker signs every arm64 executable and dylib ad hoc), which is replaced:
//! its `LC_CODE_SIGNATURE` data is the last thing in the file and in `__LINKEDIT`, so the new
//! signature takes its place and both grow to fit.
//!
//! The embedded signature is a superblob (all fields big-endian) holding a SHA-256 code
//! directory (version 0x20400, flag `adhoc`), an empty requirement set and an empty CMS blob.
//! The code directory hashes each 4096-byte page of the file up to the signature and, in its
//! special slots, the requirement set and, for an app's main executable, the bundle's
//! `Info.plist` and `_CodeSignature/CodeResources`.

use sha2::{Digest, Sha256};

const MH_MAGIC_64: u32 = 0xfeed_facf;
const CPU_TYPE_ARM64: u32 = 0x0100_000c;
const MH_EXECUTE: u32 = 2;
const MH_DYLIB: u32 = 6;
const LC_SEGMENT_64: u32 = 0x19;
const LC_CODE_SIGNATURE: u32 = 0x1d;
/// The Mach-O header of a 64-bit file.
const HEADER_SIZE: usize = 32;

const CSMAGIC_EMBEDDED_SIGNATURE: u32 = 0xfade_0cc0;
const CSMAGIC_CODEDIRECTORY: u32 = 0xfade_0c02;
const CSMAGIC_REQUIREMENTS: u32 = 0xfade_0c01;
const CSMAGIC_BLOBWRAPPER: u32 = 0xfade_0b01;
const CSSLOT_CODEDIRECTORY: u32 = 0;
const CSSLOT_REQUIREMENTS: u32 = 2;
const CSSLOT_SIGNATURESLOT: u32 = 0x10000;
/// The code directory version with the executable segment fields.
const CD_VERSION: u32 = 0x20400;
const CS_ADHOC: u32 = 0x2;
const CS_EXECSEG_MAIN_BINARY: u64 = 0x1;
const CS_HASHTYPE_SHA256: u8 = 2;
/// The fixed part of a version 0x20400 code directory.
const CD_HEADER_SIZE: usize = 88;
const HASH_SIZE: usize = 32;
const PAGE_SHIFT: u8 = 12;
const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
/// The superblob header (magic, length, count) and its three index entries.
const SUPERBLOB_HEADER_SIZE: usize = 12 + 3 * 8;
/// An empty requirement set: magic, length, count 0. An ad-hoc signature's designated
/// requirement is implicitly its cdhash.
const EMPTY_REQUIREMENTS: [u8; 12] = blob(CSMAGIC_REQUIREMENTS, 12);
/// An empty CMS blob wrapper: an ad-hoc signature has no CMS signature.
const EMPTY_CMS: [u8; 8] = blob(CSMAGIC_BLOBWRAPPER, 8);
/// arm64 virtual memory pages: `__LINKEDIT`'s `vmsize` is a multiple.
const VM_PAGE: u64 = 0x4000;

/// The bundle files the main executable of an app binds into its signature.
pub struct Bundle<'a> {
    pub info_plist: &'a [u8],
    pub code_resources: &'a [u8],
}

/// A signed file.
pub struct Signed {
    pub bytes: Vec<u8>,
    /// The first 20 bytes of the code directory's SHA-256: what a bundle's `CodeResources`
    /// records for nested code.
    pub cdhash: [u8; 20],
}

/// Why a file cannot be signed.
#[derive(Debug)]
pub struct SignError(pub String);

/// `__TEXT` or `__LINKEDIT`: where its load command is and what it covers.
struct Segment {
    command: usize,
    fileoff: u64,
    filesize: u64,
    vmsize: u64,
}

/// The parts of a Mach-O file signing needs.
struct MachO {
    filetype: u32,
    text: Segment,
    linkedit: Segment,
    /// The `LC_CODE_SIGNATURE` load command's offset.
    signature_command: usize,
    /// The start of the signature data: the end of what the code directory hashes.
    code_limit: usize,
}

/// Signs the thin arm64 executable or dylib `binary` ad hoc under `identifier`; `bundle` binds
/// an app's main executable to its bundle.
pub fn sign(
    binary: &[u8],
    identifier: &str,
    bundle: Option<&Bundle<'_>>,
) -> Result<Signed, SignError> {
    let macho = parse(binary)?;
    // Slots 1 (Info.plist) to 3 (CodeResources) for an app's executable, else 1 and 2.
    let special: Vec<[u8; HASH_SIZE]> = match bundle {
        Some(bundle) => vec![
            Sha256::digest(bundle.info_plist).into(),
            Sha256::digest(EMPTY_REQUIREMENTS).into(),
            Sha256::digest(bundle.code_resources).into(),
        ],
        None => vec![[0; HASH_SIZE], Sha256::digest(EMPTY_REQUIREMENTS).into()],
    };
    let special_slots = special.len();
    let code_slots = macho.code_limit.div_ceil(PAGE_SIZE);
    let cd_size = CD_HEADER_SIZE
        .checked_add(identifier.len())
        .and_then(|n| n.checked_add(1))
        .and_then(|n| {
            n.checked_add(
                special_slots
                    .checked_add(code_slots)?
                    .checked_mul(HASH_SIZE)?,
            )
        })
        .ok_or_else(too_large)?;
    let blob_size = SUPERBLOB_HEADER_SIZE
        .checked_add(cd_size)
        .and_then(|n| n.checked_add(EMPTY_REQUIREMENTS.len()))
        .and_then(|n| n.checked_add(EMPTY_CMS.len()))
        .ok_or_else(too_large)?;
    // codesign pads the signature to 16 bytes; the space is reserved, so any length works.
    let data_size = blob_size
        .checked_next_multiple_of(16)
        .ok_or_else(too_large)?;
    let end = macho
        .code_limit
        .checked_add(data_size)
        .ok_or_else(too_large)?;

    let mut out = Vec::with_capacity(end);
    out.extend_from_slice(binary.get(..macho.code_limit).ok_or_else(truncated)?);
    let data_size32 = u32::try_from(data_size).map_err(|_| too_large())?;
    put_u32_le(&mut out, field(macho.signature_command, 12), data_size32);
    let linkedit_size = (end as u64)
        .checked_sub(macho.linkedit.fileoff)
        .ok_or_else(|| SignError("`__LINKEDIT` starts after the signature".to_owned()))?;
    let linkedit_vmsize = macho.linkedit.vmsize.max(
        linkedit_size
            .checked_next_multiple_of(VM_PAGE)
            .ok_or_else(too_large)?,
    );
    put_u64_le(&mut out, field(macho.linkedit.command, 32), linkedit_vmsize);
    put_u64_le(&mut out, field(macho.linkedit.command, 48), linkedit_size);

    let exec_seg_flags = if macho.filetype == MH_EXECUTE {
        CS_EXECSEG_MAIN_BINARY
    } else {
        0
    };
    let cd = code_directory(
        &out,
        identifier,
        &special,
        &macho.text,
        exec_seg_flags,
        cd_size,
    )?;
    let mut cdhash = [0u8; 20];
    cdhash.copy_from_slice(&Sha256::digest(&cd)[..20]);

    let blob_size32 = u32::try_from(blob_size).map_err(|_| too_large())?;
    let cd_offset = SUPERBLOB_HEADER_SIZE as u32;
    let requirements_offset = cd_offset.saturating_add(cd.len() as u32);
    let cms_offset = requirements_offset.saturating_add(EMPTY_REQUIREMENTS.len() as u32);
    for word in [
        CSMAGIC_EMBEDDED_SIGNATURE,
        blob_size32,
        3,
        CSSLOT_CODEDIRECTORY,
        cd_offset,
        CSSLOT_REQUIREMENTS,
        requirements_offset,
        CSSLOT_SIGNATURESLOT,
        cms_offset,
    ] {
        out.extend_from_slice(&word.to_be_bytes());
    }
    out.extend_from_slice(&cd);
    out.extend_from_slice(&EMPTY_REQUIREMENTS);
    out.extend_from_slice(&EMPTY_CMS);
    out.resize(end, 0);
    Ok(Signed { bytes: out, cdhash })
}

/// The code directory over `file` (the signed file up to its signature).
fn code_directory(
    file: &[u8],
    identifier: &str,
    special: &[[u8; HASH_SIZE]],
    text: &Segment,
    exec_seg_flags: u64,
    size: usize,
) -> Result<Vec<u8>, SignError> {
    let code_slots = file.len().div_ceil(PAGE_SIZE);
    let ident_offset = CD_HEADER_SIZE;
    let hash_offset = ident_offset
        .saturating_add(identifier.len())
        .saturating_add(1)
        .saturating_add(special.len().saturating_mul(HASH_SIZE));
    let u32_of = |n: usize| u32::try_from(n).map_err(|_| too_large());

    let mut cd = Vec::with_capacity(size);
    for word in [
        CSMAGIC_CODEDIRECTORY,
        u32_of(size)?,
        CD_VERSION,
        CS_ADHOC,
        u32_of(hash_offset)?,
        u32_of(ident_offset)?,
        u32_of(special.len())?,
        u32_of(code_slots)?,
        u32_of(file.len())?,
    ] {
        cd.extend_from_slice(&word.to_be_bytes());
    }
    // hashSize, hashType, platform, pageSize.
    cd.extend_from_slice(&[HASH_SIZE as u8, CS_HASHTYPE_SHA256, 0, PAGE_SHIFT]);
    // spare2, scatterOffset, teamOffset, spare3.
    cd.extend_from_slice(&[0; 16]);
    // codeLimit64 (unused: the limit fits in 32 bits), execSegBase, execSegLimit, execSegFlags.
    for word in [0, text.fileoff, text.filesize, exec_seg_flags] {
        cd.extend_from_slice(&u64::to_be_bytes(word));
    }
    cd.extend_from_slice(identifier.as_bytes());
    cd.push(0);
    for hash in special.iter().rev() {
        cd.extend_from_slice(hash);
    }
    for page in file.chunks(PAGE_SIZE) {
        cd.extend_from_slice(&Sha256::digest(page));
    }
    debug_assert_eq!(cd.len(), size);
    Ok(cd)
}

fn parse(binary: &[u8]) -> Result<MachO, SignError> {
    let not_macho = || SignError("not a thin arm64 Mach-O file".to_owned());
    if binary.len() < HEADER_SIZE
        || u32_le(binary, 0)? != MH_MAGIC_64
        || u32_le(binary, 4)? != CPU_TYPE_ARM64
    {
        return Err(not_macho());
    }
    let filetype = u32_le(binary, 12)?;
    if filetype != MH_EXECUTE && filetype != MH_DYLIB {
        return Err(SignError(
            "not an executable or a dynamic library".to_owned(),
        ));
    }
    let ncmds = u32_le(binary, 16)?;
    let commands_end = HEADER_SIZE
        .checked_add(u32_le(binary, 20)? as usize)
        .filter(|&end| end <= binary.len())
        .ok_or_else(truncated)?;

    let mut text = None;
    let mut linkedit = None;
    let mut signature = None;
    let mut at = HEADER_SIZE;
    for _ in 0..ncmds {
        let cmd = u32_le(binary, at)?;
        let size = u32_le(binary, at.saturating_add(4))? as usize;
        let next = at
            .checked_add(size)
            .filter(|&next| size >= 8 && next <= commands_end)
            .ok_or_else(|| SignError("a load command runs past the load commands".to_owned()))?;
        match cmd {
            LC_SEGMENT_64 => {
                let name = binary
                    .get(field(at, 8)..field(at, 24))
                    .ok_or_else(truncated)?;
                let segment = Segment {
                    command: at,
                    vmsize: u64_le(binary, field(at, 32))?,
                    fileoff: u64_le(binary, field(at, 40))?,
                    filesize: u64_le(binary, field(at, 48))?,
                };
                if name.starts_with(b"__TEXT\0") {
                    text = Some(segment);
                } else if name.starts_with(b"__LINKEDIT\0") {
                    linkedit = Some(segment);
                }
            }
            LC_CODE_SIGNATURE => signature = Some(at),
            _ => {}
        }
        at = next;
    }
    let text = text.ok_or_else(|| SignError("no `__TEXT` segment".to_owned()))?;
    let linkedit = linkedit.ok_or_else(|| SignError("no `__LINKEDIT` segment".to_owned()))?;
    let signature_command = signature.ok_or_else(|| {
        SignError("no code signature to replace (link it with Apple's linker)".to_owned())
    })?;
    let data_offset = u32_le(binary, field(signature_command, 8))? as usize;
    let data_size = u32_le(binary, field(signature_command, 12))? as usize;
    let linkedit_end = linkedit
        .fileoff
        .checked_add(linkedit.filesize)
        .ok_or_else(truncated)?;
    let signature_end = data_offset.checked_add(data_size).ok_or_else(truncated)?;
    if signature_end != binary.len()
        || linkedit_end != binary.len() as u64
        || (data_offset as u64) < linkedit.fileoff
    {
        return Err(SignError(
            "the code signature is not at the end of the file and of `__LINKEDIT`".to_owned(),
        ));
    }
    Ok(MachO {
        filetype,
        text,
        linkedit,
        signature_command,
        code_limit: data_offset,
    })
}

/// The offset of a load command's field: `offset` bytes into the command at `base`, which
/// lies in the load commands (so neither overflows).
fn field(base: usize, offset: usize) -> usize {
    base.saturating_add(offset)
}

/// A blob of `LEN` bytes holding only its header (magic, length) and zeros.
const fn blob<const LEN: usize>(magic: u32, len: u32) -> [u8; LEN] {
    let [m0, m1, m2, m3] = magic.to_be_bytes();
    let [l0, l1, l2, l3] = len.to_be_bytes();
    let mut bytes = [0; LEN];
    bytes[0] = m0;
    bytes[1] = m1;
    bytes[2] = m2;
    bytes[3] = m3;
    bytes[4] = l0;
    bytes[5] = l1;
    bytes[6] = l2;
    bytes[7] = l3;
    bytes
}

fn u32_le(bytes: &[u8], at: usize) -> Result<u32, SignError> {
    let field = bytes.get(at..at.saturating_add(4)).ok_or_else(truncated)?;
    let mut word = [0; 4];
    word.copy_from_slice(field);
    Ok(u32::from_le_bytes(word))
}

fn u64_le(bytes: &[u8], at: usize) -> Result<u64, SignError> {
    let field = bytes.get(at..at.saturating_add(8)).ok_or_else(truncated)?;
    let mut word = [0; 8];
    word.copy_from_slice(field);
    Ok(u64::from_le_bytes(word))
}

/// Overwrites the little-endian field at `at`, which [`parse`] has bounds-checked.
fn put_u32_le(bytes: &mut [u8], at: usize, value: u32) {
    if let Some(field) = bytes.get_mut(at..at.saturating_add(4)) {
        field.copy_from_slice(&value.to_le_bytes());
    }
}

/// Overwrites the little-endian field at `at`, which [`parse`] has bounds-checked.
fn put_u64_le(bytes: &mut [u8], at: usize, value: u64) {
    if let Some(field) = bytes.get_mut(at..at.saturating_add(8)) {
        field.copy_from_slice(&value.to_le_bytes());
    }
}

fn truncated() -> SignError {
    SignError("the file is truncated".to_owned())
}

fn too_large() -> SignError {
    SignError("the file is too large to sign".to_owned())
}
