//! Decoding a `.kagari` file into a [`Module`]. The decoder checks the container (magic,
//! section order, lengths, padding) and the encoding of every record; index ranges and the
//! other semantic rules are the verifier's job. It never panics on any input.

use std::fmt;

use crate::encode::id;
use crate::instr::{Instr, InstrError, Label};
use crate::module::{
    AdtType, Asset, AssetId, AssetKind, ConstId, Constant, Debug, Feature, FnDebug, FnFlags,
    FnKind, FuncId, Function, Import, IntType, MAGIC, Module, NONE, Options, Static, StaticId,
    StaticKind, StrId, TypeId, Variant,
};

/// Why a file does not decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The file does not start with the `.kagari` magic.
    BadMagic,
    /// The magic matches except for the version digits.
    UnsupportedVersion,
    /// The file or a record ends early.
    Truncated,
    /// Section ids are not strictly ascending.
    SectionOrder,
    /// A critical section (`0x0001..=0x007F`) this decoder does not know.
    UnknownCriticalSection(u16),
    /// A required critical section is absent.
    MissingSection(u16),
    /// Padding, or a reserved field (section `flags`/`reserved`, a record's `reserved` or an
    /// unused `aux`), is not zero.
    NonZeroPadding,
    /// A string is not UTF-8.
    BadUtf8,
    UnknownOpcode(u16),
    /// A jump's word offset is not the start of an instruction.
    BadJumpTarget,
    /// An enum code, a boolean or flag bits, a constant tag or an instruction operand that has
    /// no meaning.
    BadTag,
    /// A section payload is longer than its records, or bytes follow the last section.
    TrailingBytes,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::BadMagic => f.write_str("not a .kagari file (bad magic)"),
            DecodeError::UnsupportedVersion => {
                f.write_str("unsupported .kagari format version (expected V000)")
            }
            DecodeError::Truncated => f.write_str("the file is truncated"),
            DecodeError::SectionOrder => f.write_str("sections are not in ascending id order"),
            DecodeError::UnknownCriticalSection(id) => {
                write!(f, "unknown critical section 0x{id:04X}")
            }
            DecodeError::MissingSection(id) => write!(f, "missing section 0x{id:04X}"),
            DecodeError::NonZeroPadding => f.write_str("padding or a reserved field is not zero"),
            DecodeError::BadUtf8 => f.write_str("a string is not valid UTF-8"),
            DecodeError::UnknownOpcode(op) => write!(f, "unknown opcode 0x{op:04X}"),
            DecodeError::BadJumpTarget => {
                f.write_str("a jump target is not the start of an instruction")
            }
            DecodeError::BadTag => f.write_str("an invalid tag, kind, flag or operand"),
            DecodeError::TrailingBytes => f.write_str("unexpected trailing bytes"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<InstrError> for DecodeError {
    fn from(error: InstrError) -> DecodeError {
        match error {
            InstrError::UnknownOpcode(op) => DecodeError::UnknownOpcode(op),
            InstrError::BadOperand => DecodeError::BadTag,
            InstrError::Truncated => DecodeError::Truncated,
        }
    }
}

/// A cursor over little-endian bytes.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated)?;
        let out = self
            .bytes
            .get(self.pos..end)
            .ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(u8::from_le_bytes(self.array()?))
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.array()?))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn i128(&mut self) -> Result<i128, DecodeError> {
        Ok(i128::from_le_bytes(self.array()?))
    }

    /// A reserved `u32` that must be zero.
    fn zero32(&mut self) -> Result<(), DecodeError> {
        match self.u32()? {
            0 => Ok(()),
            _ => Err(DecodeError::NonZeroPadding),
        }
    }

    fn bool32(&mut self) -> Result<bool, DecodeError> {
        match self.u32()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::BadTag),
        }
    }

    /// A `u64` count or length. A count larger than the address space cannot be satisfied
    /// by the remaining bytes, so it is truncation.
    fn count(&mut self) -> Result<usize, DecodeError> {
        usize::try_from(self.u64()?).map_err(|_| DecodeError::Truncated)
    }

    fn count32(&mut self) -> Result<usize, DecodeError> {
        usize::try_from(self.u32()?).map_err(|_| DecodeError::Truncated)
    }

    fn ids(&mut self, n: usize) -> Result<Vec<ConstId>, DecodeError> {
        let mut out = Vec::new();
        for _ in 0..n {
            out.push(ConstId(self.u32()?));
        }
        Ok(out)
    }

    /// Zero bytes up to the next multiple of 8 (relative to the payload start, which is
    /// 8-aligned in the file).
    fn pad(&mut self) -> Result<(), DecodeError> {
        let n = self.pos.wrapping_neg() & 7;
        if self.take(n)?.iter().any(|&b| b != 0) {
            return Err(DecodeError::NonZeroPadding);
        }
        Ok(())
    }

    fn finish(&self) -> Result<(), DecodeError> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(DecodeError::TrailingBytes)
        }
    }
}

/// Decodes a `.kagari` file. Unknown ancillary sections are skipped and their data dropped.
pub fn decode(bytes: &[u8]) -> Result<Module, DecodeError> {
    let prefix = bytes.get(..12).unwrap_or(bytes);
    if MAGIC.get(..prefix.len()) != Some(prefix) {
        return Err(DecodeError::BadMagic);
    }
    let header = bytes.get(..16).ok_or(DecodeError::Truncated)?;
    if header != MAGIC {
        return Err(DecodeError::UnsupportedVersion);
    }

    // Split the file into sections.
    let mut r = Reader::new(bytes);
    r.take(16)?;
    let mut sections: Vec<(u16, &[u8])> = Vec::new();
    while r.pos < bytes.len() {
        let id = r.u16()?;
        let flags = r.u16()?;
        let reserved = r.u32()?;
        let len = r.count()?;
        let payload = r.take(len)?;
        r.pad()?;
        if sections.last().is_some_and(|&(last, _)| id <= last) {
            return Err(DecodeError::SectionOrder);
        }
        let known = id::REQUIRED.contains(&id) || id == id::DEBUG;
        if !known && id < 0x80 {
            return Err(DecodeError::UnknownCriticalSection(id));
        }
        // Section flags and the reserved word must be zero in this version.
        if flags != 0 || reserved != 0 {
            return Err(DecodeError::NonZeroPadding);
        }
        if known {
            sections.push((id, payload));
        }
    }
    let section = |want: u16| {
        sections
            .iter()
            .find(|&&(id, _)| id == want)
            .map(|&(_, payload)| Reader::new(payload))
    };
    for want in id::REQUIRED {
        if section(want).is_none() {
            return Err(DecodeError::MissingSection(want));
        }
    }
    let required = |want: u16| section(want).ok_or(DecodeError::MissingSection(want));

    let mut module = Module {
        strings: strings(required(id::STRINGS)?)?,
        features: features(required(id::FEATURES)?)?,
        types: types(required(id::TYPES)?)?,
        int_types: int_types(required(id::INT_TYPES)?)?,
        imports: imports(required(id::IMPORTS)?)?,
        assets: assets(required(id::ASSETS)?)?,
        constants: constants(required(id::CONSTANTS)?)?,
        statics: statics(required(id::STATICS)?)?,
        functions: functions(required(id::FUNCTIONS)?)?,
        options: options(required(id::OPTIONS)?)?,
        debug: None,
    };
    if let Some(r) = section(id::DEBUG) {
        module.debug = Some(debug(r, module.functions.len())?);
    }
    Ok(module)
}

fn strings(mut r: Reader<'_>) -> Result<Vec<String>, DecodeError> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        let len = r.count()?;
        let bytes = r.take(len)?;
        let s = std::str::from_utf8(bytes).map_err(|_| DecodeError::BadUtf8)?;
        out.push(s.to_owned());
        r.pad()?;
    }
    r.finish()?;
    Ok(out)
}

fn features(mut r: Reader<'_>) -> Result<Vec<Feature>, DecodeError> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        let name = StrId(r.u32()?);
        let required = r.bool32()?;
        out.push(Feature { name, required });
    }
    r.finish()?;
    Ok(out)
}

fn types(mut r: Reader<'_>) -> Result<Vec<AdtType>, DecodeError> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        let path = StrId(r.u32()?);
        let is_enum = r.bool32()?;
        let mut variants = Vec::new();
        for _ in 0..r.count()? {
            let name = StrId(r.u32()?);
            let mut fields = Vec::new();
            for _ in 0..r.count32()? {
                let field = r.u32()?;
                fields.push((field != NONE).then_some(StrId(field)));
            }
            r.pad()?;
            variants.push(Variant { name, fields });
        }
        out.push(AdtType {
            path,
            is_enum,
            variants,
        });
    }
    r.finish()?;
    Ok(out)
}

fn int_types(mut r: Reader<'_>) -> Result<Vec<IntType>, DecodeError> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        let lo = r.i128()?;
        let hi = r.i128()?;
        let bits = r.u32()?;
        let signed = r.bool32()?;
        out.push(IntType {
            lo,
            hi,
            bits,
            signed,
        });
    }
    r.finish()?;
    Ok(out)
}

fn fn_kind(code: u64) -> Result<FnKind, DecodeError> {
    u8::try_from(code)
        .ok()
        .and_then(FnKind::from_code)
        .ok_or(DecodeError::BadTag)
}

fn imports(mut r: Reader<'_>) -> Result<Vec<Import>, DecodeError> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        let path = StrId(r.u32()?);
        let kind = fn_kind(u64::from(r.u16()?))?;
        let params = r.u16()?;
        out.push(Import { path, kind, params });
    }
    r.finish()?;
    Ok(out)
}

fn assets(mut r: Reader<'_>) -> Result<Vec<Asset>, DecodeError> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        let kind = AssetKind::from_code(r.u32()?).ok_or(DecodeError::BadTag)?;
        let path = StrId(r.u32()?);
        out.push(Asset { kind, path });
    }
    r.finish()?;
    Ok(out)
}

fn constants(mut r: Reader<'_>) -> Result<Vec<Constant>, DecodeError> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        out.push(constant(&mut r)?);
    }
    r.finish()?;
    Ok(out)
}

fn constant(r: &mut Reader<'_>) -> Result<Constant, DecodeError> {
    let tag = r.u32()?;
    let aux = r.u32()?;
    // Tags whose `aux` is documented as 0 reject anything else.
    let zero = |aux: u32| {
        if aux == 0 {
            Ok(())
        } else {
            Err(DecodeError::NonZeroPadding)
        }
    };
    let count = usize::try_from(aux).map_err(|_| DecodeError::Truncated);
    Ok(match tag {
        0 => Constant::Bool(match aux {
            0 => false,
            1 => true,
            _ => return Err(DecodeError::BadTag),
        }),
        1 => {
            zero(aux)?;
            Constant::Int(r.i128()?)
        }
        2 => Constant::Decimal {
            mantissa: r.i128()?,
            scale: aux,
        },
        3 => {
            zero(aux)?;
            Constant::Rat {
                num: r.i128()?,
                den: r.i128()?,
            }
        }
        4 => Constant::String(StrId(aux)),
        5 => Constant::Text(StrId(aux)),
        6 => {
            zero(aux)?;
            Constant::Ruby {
                base: ConstId(r.u32()?),
                reading: ConstId(r.u32()?),
            }
        }
        7 => {
            let base = ConstId(r.u32()?);
            let readings = r.ids(count?)?;
            r.pad()?;
            Constant::MonoRuby { base, readings }
        }
        8 => {
            zero(aux)?;
            Constant::Effect {
                base: ConstId(r.u32()?),
                effect: ConstId(r.u32()?),
            }
        }
        9..=12 => {
            let items = r.ids(count?)?;
            r.pad()?;
            match tag {
                9 => Constant::Concat(items),
                10 => Constant::Tuple(items),
                11 => Constant::Array(items),
                _ => Constant::Vec(items),
            }
        }
        13 => {
            let ty = TypeId(r.u32()?);
            let variant = r.u32()?;
            let fields = r.ids(count?)?;
            r.pad()?;
            Constant::Adt {
                ty,
                variant,
                fields,
            }
        }
        14 => Constant::Asset(AssetId(aux)),
        15 => Constant::Function(FuncId(aux)),
        16 => Constant::Handle(StaticId(aux)),
        _ => return Err(DecodeError::BadTag),
    })
}

fn statics(mut r: Reader<'_>) -> Result<Vec<Static>, DecodeError> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        let path = StrId(r.u32()?);
        let kind = StaticKind::from_code(r.u32()?).ok_or(DecodeError::BadTag)?;
        let init = ConstId(r.u32()?);
        r.zero32()?;
        out.push(Static { path, kind, init });
    }
    r.finish()?;
    Ok(out)
}

fn functions(mut r: Reader<'_>) -> Result<Vec<Function>, DecodeError> {
    let mut out = Vec::new();
    for _ in 0..r.count()? {
        let path = StrId(r.u32()?);
        let kind = fn_kind(u64::from(r.u8()?))?;
        let flags = FnFlags::from_bits(r.u8()?).ok_or(DecodeError::BadTag)?;
        let params = r.u16()?;
        let regs = r.u16()?;
        if r.u16()? != 0 {
            return Err(DecodeError::NonZeroPadding);
        }
        let words = r.count32()?;
        let mut code_words = Vec::new();
        for _ in 0..words {
            code_words.push(r.u64()?);
        }
        out.push(Function {
            path,
            kind,
            flags,
            params,
            regs,
            code: code(&code_words)?,
        });
    }
    r.finish()?;
    Ok(out)
}

/// Decodes code words; jump targets are mapped from word offsets to instruction indices.
fn code(words: &[u64]) -> Result<Vec<Instr>, DecodeError> {
    let mut code = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut pos = 0;
    while let Some(rest) = words.get(pos..).filter(|rest| !rest.is_empty()) {
        let (instr, used) = Instr::decode(rest)?;
        code.push(instr);
        offsets.push(pos);
        pos = pos.checked_add(used).ok_or(DecodeError::Truncated)?;
    }
    let index = |label: Label| -> Result<Label, DecodeError> {
        let word = usize::try_from(label.0).map_err(|_| DecodeError::BadJumpTarget)?;
        let index = offsets
            .binary_search(&word)
            .map_err(|_| DecodeError::BadJumpTarget)?;
        u32::try_from(index)
            .map(Label)
            .map_err(|_| DecodeError::BadJumpTarget)
    };
    for instr in &mut code {
        match instr {
            Instr::Jmp { target } | Instr::JmpIf { target, .. } | Instr::JmpNot { target, .. } => {
                *target = index(*target)?
            }
            _ => {}
        }
    }
    Ok(code)
}

fn options(mut r: Reader<'_>) -> Result<Options, DecodeError> {
    let max_call_depth = r.u32()?;
    r.zero32()?;
    r.finish()?;
    Ok(Options { max_call_depth })
}

fn debug(mut r: Reader<'_>, functions: usize) -> Result<Debug, DecodeError> {
    let mut files = Vec::new();
    for _ in 0..r.count()? {
        files.push(StrId(r.u32()?));
        r.zero32()?;
    }
    let mut out = Vec::new();
    for _ in 0..functions {
        let file = r.u32()?;
        let mut lines = Vec::new();
        for _ in 0..r.count32()? {
            let word = r.u32()?;
            let offset = r.u32()?;
            lines.push((word, offset));
        }
        out.push(FnDebug {
            file: (file != NONE).then_some(file),
            lines,
        });
    }
    r.finish()?;
    Ok(Debug {
        files,
        functions: out,
    })
}
