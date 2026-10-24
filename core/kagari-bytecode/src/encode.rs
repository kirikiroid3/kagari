//! The binary encoding of a [`Module`]: the magic, then one section per table in ascending id
//! order. Every record is a multiple of 8 bytes and every integer is little-endian.

use crate::instr::{Instr, word_offsets};
use crate::module::{ConstId, Constant, MAGIC, Module, NONE};

/// Section ids.
pub(crate) mod id {
    pub const STRINGS: u16 = 0x01;
    pub const FEATURES: u16 = 0x02;
    pub const TYPES: u16 = 0x03;
    pub const INT_TYPES: u16 = 0x04;
    pub const IMPORTS: u16 = 0x05;
    pub const ASSETS: u16 = 0x06;
    pub const CONSTANTS: u16 = 0x07;
    pub const STATICS: u16 = 0x08;
    pub const FUNCTIONS: u16 = 0x09;
    pub const OPTIONS: u16 = 0x0A;
    pub const DEBUG: u16 = 0x80;
    /// The critical sections every module has, in order.
    pub const REQUIRED: [u16; 10] = [
        STRINGS, FEATURES, TYPES, INT_TYPES, IMPORTS, ASSETS, CONSTANTS, STATICS, FUNCTIONS,
        OPTIONS,
    ];
}

/// A little-endian byte buffer.
#[derive(Default)]
struct Writer {
    out: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn i128(&mut self, v: i128) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    /// A length or count; tables never exceed `u64`.
    fn len(&mut self, n: usize) {
        self.u64(u64::try_from(n).unwrap_or(u64::MAX));
    }
    /// A count that the format stores as `u32`.
    fn len32(&mut self, n: usize) {
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }
    fn ids(&mut self, ids: &[ConstId]) {
        for c in ids {
            self.u32(c.0);
        }
    }
    /// Zero bytes up to the next multiple of 8.
    fn pad(&mut self) {
        while self.out.len() & 7 != 0 {
            self.out.push(0);
        }
    }
}

/// Encodes `module`. Every critical section is written; Debug only when `module.debug` is set.
pub fn encode(module: &Module) -> Vec<u8> {
    let mut file = Writer::default();
    file.out.extend_from_slice(&MAGIC);
    let mut section = |id: u16, body: &dyn Fn(&mut Writer)| {
        let mut payload = Writer::default();
        body(&mut payload);
        file.u16(id);
        file.u16(0);
        file.u32(0);
        file.len(payload.out.len());
        file.out.extend_from_slice(&payload.out);
        file.pad();
    };

    section(id::STRINGS, &|w| {
        w.len(module.strings.len());
        for s in &module.strings {
            w.len(s.len());
            w.out.extend_from_slice(s.as_bytes());
            w.pad();
        }
    });
    section(id::FEATURES, &|w| {
        w.len(module.features.len());
        for f in &module.features {
            w.u32(f.name.0);
            w.u32(u32::from(f.required));
        }
    });
    section(id::TYPES, &|w| {
        w.len(module.types.len());
        for t in &module.types {
            w.u32(t.path.0);
            w.u32(u32::from(t.is_enum));
            w.len(t.variants.len());
            for v in &t.variants {
                w.u32(v.name.0);
                w.len32(v.fields.len());
                for f in &v.fields {
                    w.u32(f.map_or(NONE, |s| s.0));
                }
                w.pad();
            }
        }
    });
    section(id::INT_TYPES, &|w| {
        w.len(module.int_types.len());
        for t in &module.int_types {
            w.i128(t.lo);
            w.i128(t.hi);
            w.u32(t.bits);
            w.u32(u32::from(t.signed));
        }
    });
    section(id::IMPORTS, &|w| {
        w.len(module.imports.len());
        for i in &module.imports {
            w.u32(i.path.0);
            w.u16(u16::from(i.kind.code()));
            w.u16(i.params);
        }
    });
    section(id::ASSETS, &|w| {
        w.len(module.assets.len());
        for a in &module.assets {
            w.u32(a.kind.code());
            w.u32(a.path.0);
        }
    });
    section(id::CONSTANTS, &|w| {
        w.len(module.constants.len());
        for c in &module.constants {
            constant(w, c);
        }
    });
    section(id::STATICS, &|w| {
        w.len(module.statics.len());
        for s in &module.statics {
            w.u32(s.path.0);
            w.u32(s.kind.code());
            w.u32(s.init.0);
            w.u32(0);
        }
    });
    section(id::FUNCTIONS, &|w| {
        w.len(module.functions.len());
        for f in &module.functions {
            let offsets = word_offsets(&f.code);
            w.u32(f.path.0);
            w.u8(f.kind.code());
            w.u8(f.flags.bits());
            w.u16(f.params);
            w.u16(f.regs);
            w.u16(0);
            w.u32(offsets.last().copied().unwrap_or(0));
            code(w, &f.code, &offsets);
        }
    });
    section(id::OPTIONS, &|w| {
        w.u32(module.options.max_call_depth);
        w.u32(0);
    });
    if let Some(debug) = &module.debug {
        section(id::DEBUG, &|w| {
            w.len(debug.files.len());
            for f in &debug.files {
                w.u32(f.0);
                w.u32(0);
            }
            for f in &debug.functions {
                w.u32(f.file.unwrap_or(NONE));
                w.len32(f.lines.len());
                for &(word, offset) in &f.lines {
                    w.u32(word);
                    w.u32(offset);
                }
            }
        });
    }
    file.out
}

fn constant(w: &mut Writer, c: &Constant) {
    w.u32(c.tag());
    match c {
        Constant::Bool(b) => w.u32(u32::from(*b)),
        Constant::Int(v) => {
            w.u32(0);
            w.i128(*v);
        }
        Constant::Decimal { mantissa, scale } => {
            w.u32(*scale);
            w.i128(*mantissa);
        }
        Constant::Rat { num, den } => {
            w.u32(0);
            w.i128(*num);
            w.i128(*den);
        }
        Constant::String(s) | Constant::Text(s) => w.u32(s.0),
        Constant::Ruby { base, reading } => {
            w.u32(0);
            w.u32(base.0);
            w.u32(reading.0);
        }
        Constant::MonoRuby { base, readings } => {
            w.len32(readings.len());
            w.u32(base.0);
            w.ids(readings);
            w.pad();
        }
        Constant::Effect { base, effect } => {
            w.u32(0);
            w.u32(base.0);
            w.u32(effect.0);
        }
        Constant::Concat(items)
        | Constant::Tuple(items)
        | Constant::Array(items)
        | Constant::Vec(items) => {
            w.len32(items.len());
            w.ids(items);
            w.pad();
        }
        Constant::Adt {
            ty,
            variant,
            fields,
        } => {
            w.len32(fields.len());
            w.u32(ty.0);
            w.u32(*variant);
            w.ids(fields);
            w.pad();
        }
        Constant::Asset(a) => w.u32(a.0),
        Constant::Function(f) => w.u32(f.0),
        Constant::Handle(s) => w.u32(s.0),
    }
}

/// The code words of `code`; jump labels become the word offsets in `offsets`.
fn code(w: &mut Writer, code: &[Instr], offsets: &[u32]) {
    let target_word = |label: crate::instr::Label| {
        usize::try_from(label.0)
            .ok()
            .and_then(|index| offsets.get(index))
            .copied()
            .unwrap_or(NONE)
    };
    for instr in code {
        let (first, x) = instr.encode(target_word);
        w.u64(first);
        if let Some(x) = x {
            w.u64(x);
        }
    }
}
