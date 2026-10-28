# The `.kagari` Format

A game compiles to one `.kagari` file. This chapter specifies format version `V000` byte by
byte: the container, the tables, the instruction set, the load-time verifier and the story PRNG.
The Reference only says "the `.kagari` file"; everything below is internal.

Where in the code:

- `core/kagari-bytecode/src/module.rs`: the in-memory `Module`, one field per section, the id
  newtypes, `MAGIC`, `NONE`, the enum codes (`FnKind`, `AssetKind`, `StaticKind`, `FnFlags`).
- `core/kagari-bytecode/src/encode.rs`: `encode`, section ids (`mod id`).
- `core/kagari-bytecode/src/decode.rs`: `decode` and `DecodeError`.
- `core/kagari-bytecode/src/instr.rs`: `Instr`, opcodes, mnemonics, word counts, per-instruction
  encode/decode.
- `core/kagari-bytecode/src/verify.rs`: `verify`, `verify_code` and `VerifyErrorKind`.
- `core/kagari-bytecode/src/disasm.rs`: `disassemble` (behind `kagari disasm`).
- `core/kagari-bytecode/src/tests.rs`: a module using every section, constant tag and opcode,
  round trips, corruption fuzzing.
- Producer: `core/kagari-lang/src/emit/` (see [Emitting Bytecode](../compiler/emit.md)).
  Consumer: `runtime_libs/kagari-vm` (see [The VM](../runtime/vm.md)).

## Pipeline position

`kagari build` runs the checks, emits a `Module`, **verifies it** (`verify`, or `verify_code`
when the root has no `main`), and only then encodes it. A verification failure at build time is
a compiler bug and is reported as `E7003` ("internal compiler error: the emitted bytecode fails
verification"). A missing `fn main()` is `E7001`; a function that needs more than 65 535
registers is `E7002`. At run time the VM decodes and verifies again (`Program::load`): the file
is untrusted input.

The crate is `#![deny(clippy::unwrap_used, clippy::panic, clippy::expect_used,
clippy::arithmetic_side_effects)]`. The decoder never panics on any input; a test flips bytes
across a whole encoded module to check that.

## Conventions

- Every multi-byte integer is little-endian.
- Every record is a multiple of 8 bytes. *pad* means zero bytes up to the next 8-byte
  boundary, measured from the payload start (which is itself 8-aligned in the file). Nonzero
  padding is a decode error.
- References to strings, types, int types, imports, assets, constants, statics and functions
  are `u32` indices into their table. `NONE` = `u32::MAX` marks an absent index.
- Every reserved field must be zero, so that every module has exactly one encoding.
- Counts are `u64` unless a table below says `u32`.

## Header {#header}

Bytes `0..16` are the magic:

| Offset | Size | Content | Meaning |
| -----: | ---: | ------- | ------- |
| 0 | 4 | `00 00 00 00` | zero bytes |
| 4 | 6 | `4B 41 47 41 52 49` | ASCII `KAGARI` |
| 10 | 2 | `00 00` | zero bytes |
| 12 | 4 | `56 30 30 30` | ASCII `V000`: `V` and three decimal digits |

`MAGIC = *b"\0\0\0\0KAGARI\0\0V000"`. The decoder first compares bytes `0..12`: a mismatch is
`BadMagic`. If those match but bytes `12..16` differ, the error is `UnsupportedVersion`, so an
old loader reports a newer file as "unsupported version" rather than "not a `.kagari` file".

## Layout

<!-- diagram: formats-kagari-layout (mermaid-ish)
flowchart LR
  magic["magic 16 B V000"] ==> s01["0x01 Strings"] ==> s02["0x02 Features"] ==> s0308["0x03..0x08 Types IntTypes Imports Assets Constants Statics"] ==> s09["0x09 Functions"] ==> s0a["0x0A Options"] ==> s80["0x80 Debug (ancillary, optional)"]
  subgraph section["every section"]
    id["id u16 +0"] ==> flags["flags u16 +2 = 0"] ==> rsv["reserved u32 +4 = 0"] ==> len["len u64 +8"] ==> payload["payload +16, len bytes"] ==> pad["zero pad to 8"]
  end
  subgraph function["function record in 0x09"]
    path["path u32"] ==> kind["kind u8"] ==> fl["flags u8"] ==> params["params u16"] ==> regs["regs u16"] ==> r2["reserved u16"] ==> words["words u32"] ==> code["words x u64 code"]
  end
-->
{{#include ../diagrams/formats-kagari-layout.svg}}

Sections follow at offset 16. Each section is a 16-byte header, the payload, then pad:

| Offset | Size | Field | Meaning |
| -----: | ---: | ----- | ------- |
| 0 | 2 | `id` | section id |
| 2 | 2 | `flags` | must be 0 in `V000` |
| 4 | 4 | `reserved` | must be 0 |
| 8 | 8 | `len` | exact payload length, without padding |
| 16 | `len` | payload | the section's records |
| 16 + `len` | 0–7 | pad | zero bytes |

Rules the decoder enforces:

- Section ids are **strictly ascending**; each appears at most once (`SectionOrder`).
- Ids `0x0001..=0x007F` are **critical**. An unknown critical id is
  `UnknownCriticalSection`. Every critical section of `V000` is required
  (`MissingSection`).
- Ids `0x0080..` are **ancillary**. An unknown ancillary section is skipped and dropped.
- A payload longer than its records, or bytes after the last section, is `TrailingBytes`.

| Id | Section | Required |
| -- | ------- | -------- |
| `0x01` | Strings | yes |
| `0x02` | Features | yes |
| `0x03` | Types | yes |
| `0x04` | IntTypes | yes |
| `0x05` | Imports | yes |
| `0x06` | Assets | yes |
| `0x07` | Constants | yes |
| `0x08` | Statics | yes |
| `0x09` | Functions | yes |
| `0x0A` | Options | yes |
| `0x80` | Debug | no (ancillary) |

The encoder writes all ten critical sections, even when a table is empty, and Debug only when
`Module::debug` is set.

## Sections

### Strings (`0x01`)

`count: u64`, then per string: `len: u64`, `len` UTF-8 bytes, pad. Invalid UTF-8 is `BadUtf8`.
Every name in the module (paths, variant and field names, import paths, asset paths, source
file paths) is an index into this table.

### Features (`0x02`)

`count: u64`, then per feature (8 bytes): `name: u32` (string), `flags: u32` (`0` or `1`; bit 0
= required; any other value is `BadTag`). A feature names an engine extension the module uses.
A loader that lacks a *required* feature refuses the module. The current VM supports none:
`Program::load` rejects any required feature with `LoadError::RequiredFeature`. The compiler
emits no features yet.

### Types (`0x03`)

`count: u64`, then per ADT:

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `path` | `u32` | string: the full path, e.g. `std::Option`, `crate::Mood` |
| `flags` | `u32` | `0` struct, `1` enum (other values: `BadTag`) |
| `variants` | `u64` | variant count |
| per variant: `name` | `u32` | string; a struct's single variant is named like the struct |
| per variant: `fields` | `u32` | field count |
| per variant: field names | `fields × u32` | string, or `NONE` for a positional field |
| per variant: pad | | to 8 bytes |

Field and variant order is the declaration order in source; the engine depends on it for std
types (see [Builtins and the Host ABI](../std/builtins.md)). Generic ADTs are monomorphized:
every instantiation the program needs is its own entry, interned on first use.

### IntTypes (`0x04`)

`count: u64`, then per int type (40 bytes): `lo: i128`, `hi: i128`, `bits: u32`,
`signed: u32` (`0`/`1`). `lo..=hi` is the range of the type; `bits` and `signed` drive shifts,
INOT and wrapping. Entry 0 must be `i128` (`i128::MIN..=i128::MAX`, 128 bits, signed). Every
Rust integer type and every `Int<lo, hi>` used by an integer instruction gets one entry.

### Imports (`0x05`)

`count: u64`, then per import (8 bytes): `path: u32` (string), `kind: u16`, `params: u16`.

- `path` is the std path of a `#[builtin]` function that has no intrinsic, such as
  `std::narrative::Meta::write` or `std::screen::Entity::position` (type path, then method name,
  generic arguments dropped).
- `kind` uses the function kind codes below and is never `3` (scene): verifier rule
  `SceneImport`. A `pushdown fn` builtin is instant (`2`).
- `params` counts `self`. `std::ui::slider::<LO, HI>(props)` is the one import that also takes
  its const generic arguments: its `params` is 3 (`props`, then `LO` and `HI` as `Int`s).

### Assets (`0x06`)

`count: u64`, then per asset (8 bytes): `kind: u32`, `path: u32` (string).

| `kind` | Asset type |
| -----: | ---------- |
| 0 | image |
| 1 | audio |
| 2 | voice |
| 3 | video |
| 4 | font |
| 5 | shader |

`path` is `/`-separated and relative to the program's asset directory. A system font is
`system-fonts/` followed by its path relative to its system font directory. `kagari build`
packs every listed asset into the `.kiarchive` named by the first path segment (see
[Archives and Save Databases](./archive.md)).

### Constants (`0x07`)

`count: u64`, then per constant: `tag: u32`, `aux: u32`, then the tag's payload, padded to 8.
A constant may refer only to constants with a **lower** index (rule `ForwardConstant`), so a
loader can build them in one forward pass.

| Tag | Constant | `aux` | Payload |
| --: | -------- | ----- | ------- |
| 0 | Bool | `0`/`1` | — |
| 1 | Int | 0 | `i128` |
| 2 | Decimal | scale | `mantissa: i128`; value `mantissa / 10^scale` |
| 3 | Rat | 0 | `num: i128`, `den: i128` |
| 4 | String | string | — |
| 5 | Text, plain | string | — |
| 6 | Text, ruby | 0 | `base: u32`, `reading: u32` (Text constants) |
| 7 | Text, mono-ruby | reading count | `base: u32`, `count × u32` readings, pad |
| 8 | Text, effect | 0 | `base: u32`, `effect: u32` (a `TextEffect` Adt constant) |
| 9 | Text, concat | count | `count × u32`, pad |
| 10 | Tuple (unit = 0 items) | count | `count × u32`, pad |
| 11 | Array | count | `count × u32`, pad |
| 12 | Vec | count | `count × u32`, pad |
| 13 | Adt | field count | `type: u32`, `variant: u32`, `count × u32` fields, pad |
| 14 | Asset | asset | — |
| 15 | Function | function | — |
| 16 | Handle | static | — (a `Setting`/`Meta` handle) |

An `aux` documented as 0 that is nonzero is `NonZeroPadding`; an unknown tag is `BadTag`. The
VM additionally rejects at load a Rat with `den == 0`, and text constants whose parts are not
Text (`LoadError::BadConstant`).

### Statics (`0x08`)

`count: u64`, then per static (16 bytes): `path: u32`, `kind: u32`, `init: u32` (constant),
`reserved: u32` (= 0).

| `kind` | Static |
| -----: | ------ |
| 0 | story variable (`static mut`), part of the story state, saved with the game |
| 1 | setting: the static behind a `Setting<T>` handle |
| 2 | meta: the static behind a `Meta<T>` handle |

### Functions (`0x09`)

`count: u64`, then per function a 16-byte head followed by its code:

| Offset | Size | Field | Meaning |
| -----: | ---: | ----- | ------- |
| 0 | 4 | `path` | string |
| 4 | 1 | `kind` | function kind code |
| 5 | 1 | `flags` | function flags |
| 6 | 2 | `params` | parameters arrive in `r0..params` |
| 8 | 2 | `regs` | registers `r0..regs` |
| 10 | 2 | reserved | = 0 |
| 12 | 4 | `words` | code length in words |
| 16 | `8 × words` | code | code words, see [Instructions](#instructions) |

| Kind code | Kind |
| --------: | ---- |
| 0 | `fn` |
| 1 | `const fn` |
| 2 | instant `pushdown fn` (instant procedure) |
| 3 | scene (`#[scene] pushdown fn`) |

| Flag bit | Name | Meaning |
| -------: | ---- | ------- |
| 0 (`1`) | entry | an `#[entry]` scene |
| 1 (`2`) | main | `fn main()` of the root module, run at boot |
| 2 (`4`) | closure | a closure body; its first parameters are the captures |
| 3 (`8`) | synthetic | made by the compiler: a constructor or builtin wrapper used as a value, or the initializer of a `const`/plain `static` or of a promoted constant whose value holds a `Screen` (and so cannot be a constant); initializers are `const fn` with no parameters |

Unknown flag bits (`0xF0`) are `BadTag`.

### Options (`0x0A`)

8 bytes: `max_call_depth: u32`, `reserved: u32` (= 0). `max_call_depth` is the compiler option
`recursion_depth` (default 2048): more nested calls at run time is a VM panic.

### Debug (`0x80`, ancillary)

`files: u64`, then `files × (path: u32, reserved: u32)`; then exactly one record per function,
in function order: `file: u32` (index into `files`, or `NONE`), `count: u32`, then
`count × (word: u32, offset: u32)`. Each pair maps a code word index to a **byte offset** in the
source file, ascending by word. The verifier requires one record per function
(`DebugFunctions`) and `file` in range. The runtime prints locations as `file@offset`, since it
has no sources.

## Values and references {#values}

The format is untyped at the instruction level: registers hold tagged values and every
instruction checks the tags it needs at run time. Value tags (`kagari_vm::Value`):

| Tag | Meaning |
| --- | ------- |
| `Uninit` | the initial content of every register; reading it panics |
| `Bool`, `Int` (`i128`), `Decimal`, `Rat` | scalars; every integer type is an `Int` at run time |
| `Text`, `String` | rich text (plain, ruby, mono-ruby, effect, concat) and runtime strings |
| `Tuple`, `Array`, `Vec`, `Adt { type, variant, fields }` | aggregates with value semantics |
| `Asset`, `Func`, `Closure`, `Handle` | asset id, function pointer, closure, setting/meta static |
| `Ref` | a reference |
| `Host`, `HostFn` | an opaque engine object (`Screen`, `Element`, `Narrative`, a host part) and an engine-provided callable |

`usize` values are `Int`s in `0..=u64::MAX`; a `u128` above `i128::MAX` is out of range and
panics. Generics are monomorphized only so that the compiler can fold type-dependent operations
(int types, `Option` type ids).

A **reference** is `Ref { root, path }`: `root` is a heap cell (`CELL`), a story static
(`REFSTAT`), or a *host part*, an opaque handle returned by an import such as `&mut Entity`;
`path` is a list of field and index projections. LOAD, STORE, REFFIELD and REFINDEX on a host
part panic (`HostPartAccess`): host parts are only passed back to imports.

- **Value operands auto-deref**: a reference is followed through every reference stored at its
  target and the final value is read.
- Operands marked **raw** are taken as they are, so a reference can be stored in a field, an
  element, a cell or passed as an argument. Call arguments, captures and aggregate elements are
  always raw.
- **References to references collapse**: STORE, REFFIELD, REFINDEX, VECSET and VECPUSH first
  follow every reference along the path or at the target, so they act on the place at the end
  of the chain. A STORE through a reference to a cell that holds a reference writes the
  pointee.

## Instructions {#instructions}

<!-- diagram: formats-kagari-word (mermaid-ish)
flowchart LR
  op["op u16 bits 0-15"] ==> a["a u16 bits 16-31"] ==> b["b u16 bits 32-47"] ==> c["c u16 bits 48-63"]
  c -.-> x["x u64 operand word (2-word instructions only)"]
-->
{{#include ../diagrams/formats-kagari-word.svg}}

A code word is a `u64`: `op | a << 16 | b << 32 | c << 48`. A 2-word instruction is followed by
an operand word `x`. `a`, `b` and `c` are registers (`r0..regs`, at most 65 535) unless noted.
Operand fields an opcode does not use must be zero: the decoder re-encodes every instruction
and rejects any difference (`BadTag`), so each instruction has exactly one encoding.

Jumps: in the file, a jump target is the **word offset** of an instruction start in the same
function. The decoder maps it to an instruction index; a target inside a 2-word instruction is
`BadJumpTarget`. In memory (`Instr`), `Label` is the instruction index; `word_offsets` converts.

### Opcode table {#opcode-table}

Generated from `core/kagari-bytecode/src/instr.rs` (`Instr::opcode`, `Instr::mnemonic`,
`Instr::words`, the variant doc comments); semantics from `runtime_libs/kagari-vm/src/exec/`.
75 opcodes. Gaps in the numbering are unassigned.

| Op | Mnemonic | Words | Operands | Semantics |
| -- | -------- | ----: | -------- | --------- |
| `0x01` | MOVE | 1 | a=dst, b=src (raw) | Copy a register as is (a reference stays a reference). |
| `0x02` | CONST | 2 | a=dst, x=constant | Load a module constant (precomputed at load). |
| `0x03` | BOOL | 1 | a=dst, b=0/1 | `b` other than 0/1 is a decode error. |
| `0x04` | INT | 2 | a=dst, x=i64 | The `x` bits as `i64`, sign-extended to `i128`. |
| `0x05` | FUNC | 2 | a=dst, x=function | A `Func` value (function pointer). |
| `0x06` | CLEAR | 1 | a | The register becomes `Uninit`. |
| `0x10` | IADD | 2 | a=dst, b, c, x=int type | Checked `i128` op, then range check against `[lo, hi]`. |
| `0x11` | ISUB | 2 | a=dst, b, c, x=int type | As IADD. |
| `0x12` | IMUL | 2 | a=dst, b, c, x=int type | As IADD. |
| `0x13` | IDIV | 2 | a=dst, b, c, x=int type | Truncating; divisor 0 is `DivisionByZero`. |
| `0x14` | IREM | 2 | a=dst, b, c, x=int type | Remainder; divisor 0 is `DivisionByZero`. |
| `0x15` | IAND | 2 | a=dst, b, c, x=int type | Bitwise on `i128`, then range check. |
| `0x16` | IOR | 2 | a=dst, b, c, x=int type | As IAND. |
| `0x17` | IXOR | 2 | a=dst, b, c, x=int type | As IAND. |
| `0x18` | ISHL | 2 | a=dst, b, c, x=int type | Shift amount must be `< bits`; result wrapped to `bits`/`signed`, then range check. |
| `0x19` | ISHR | 2 | a=dst, b, c, x=int type | Arithmetic shift of the `i128`; amount `< bits`; range check. |
| `0x1A` | INEG | 2 | a=dst, b, x=int type | Checked negation, range check. |
| `0x1B` | INOT | 2 | a=dst, b, x=int type | `!b` (unsigned: wrapped to `bits`), range check. |
| `0x1C` | ICLAMP | 2 | a=dst, b, x=int type | Clamp to `[lo, hi]`; never panics (saturation). |
| `0x20` | NOT | 1 | a, b | Bool not. |
| `0x21` | BAND | 1 | a, b, c | Non-short-circuit bool and. |
| `0x22` | BOR | 1 | a, b, c | Non-short-circuit bool or. |
| `0x23` | BXOR | 1 | a, b, c | Bool xor. |
| `0x28` | DADD | 1 | a, b, c | Decimal; overflow panics. |
| `0x29` | DSUB | 1 | a, b, c | Decimal; overflow panics. |
| `0x2A` | DMUL | 1 | a, b, c | Decimal; overflow panics. |
| `0x2B` | DDIV | 1 | a, b, c | Exact quotient truncated to 7 fractional digits; zero divisor or overflow panics. |
| `0x2C` | DNEG | 1 | a, b | Decimal negation (checked). |
| `0x30` | RADD | 1 | a, b, c | Rat, lowest terms; overflow panics. |
| `0x31` | RSUB | 1 | a, b, c | As RADD. |
| `0x32` | RMUL | 1 | a, b, c | As RADD. |
| `0x33` | RDIV | 1 | a, b, c | Zero divisor is `DivisionByZero`. |
| `0x34` | RNEG | 1 | a, b | Rat negation (checked). |
| `0x35` | RATNEW | 1 | a, b=num, c=den | Two `Int`s → Rat in lowest terms; `den == 0` panics. |
| `0x36` | RATDEC | 1 | a, b | Rat → Decimal, truncated toward zero to 7 digits. |
| `0x40` | EQ | 1 | a, b, c | Structural equality; tags must match; closures and host functions panic. |
| `0x41` | NE | 1 | a, b, c | Negated EQ. |
| `0x42` | LT | 1 | a, b, c | Ordering on two Int, two Decimal or two Rat. |
| `0x43` | LE | 1 | a, b, c | As LT. |
| `0x44` | GT | 1 | a, b, c | As LT. |
| `0x45` | GE | 1 | a, b, c | As LT. |
| `0x50` | JMP | 2 | x=target | `x` is a word offset of an instruction start. |
| `0x51` | JMPIF | 2 | a=cond, x=target | Jump when `cond` is true. |
| `0x52` | JMPNOT | 2 | a=cond, x=target | Jump when `cond` is false. |
| `0x53` | RET | 1 | a (raw) | Return `a`; `Uninit` panics. |
| `0x54` | UNREACHABLE | 1 | — | Panic (`Unreachable`). |
| `0x60` | CALL | 2 | a=dst, b=first arg, c=argc, x=function | Push a frame; args are the raw registers `b..b+c`. |
| `0x61` | CALLIMP | 2 | a=dst, b=first arg, c=argc, x=import | Host call `Host::import`; raw args. |
| `0x62` | CALLVAL | 2 | a=dst, b, c, x=callee reg (bits 0–15) \| kind (bits 16–23) | Call a `Func`, `Closure` or `HostFn`; kind and arity checked at run time; bits 24–63 zero. |
| `0x63` | CLOSURE | 2 | a=dst, b=first capture, c=count, x=function | Captures are copied by value from raw registers. |
| `0x70` | STEP | 1 | a=dst, b=step record | Suspend the story fiber; `a` receives the step result on resume. Scenes only. |
| `0x71` | RAND | 2 | a=dst, x=index (bits 0–31) \| domain (bits 32–63: 0 enum, 1 int) | Uniform PRNG draw. Scenes only. |
| `0x80` | TUPLE | 1 | a=dst, b=first, c=count | Raw elements; count 0 is unit. |
| `0x81` | ARRAY | 1 | a=dst, b=first, c=count | Raw elements. |
| `0x82` | REPEAT | 2 | a=dst, b=value (raw), x=count (u64) | `[v; n]`; costs `n` extra fuel. |
| `0x83` | ADT | 2 | a=dst, b=first, c=count, x=type (bits 0–31) \| variant (bits 32–63) | Struct or variant value; raw fields. |
| `0x84` | FIELD | 1 | a=dst, b, c=field index (immediate) | Field of a Tuple or Adt. |
| `0x85` | ISVAR | 2 | a=dst, b, x=type \| variant << 32 | Bool: `b` is that variant; an Adt of another type panics. |
| `0x86` | INDEX | 1 | a=dst, b=array or Vec, c=index | Out of range panics. |
| `0x87` | LEN | 1 | a=dst, b | Length of an Array or Vec as `Int`. |
| `0x90` | CELL | 1 | a=dst, b=init (raw) | New heap cell; `a` is a reference to it. |
| `0x91` | REFSTAT | 2 | a=dst, x=static | Reference to a story-variable cell. Scenes and instant procedures only. |
| `0x92` | REFFIELD | 1 | a=dst, b=ref (raw), c=field (immediate) | Extend a reference by a field projection (checked to exist). |
| `0x93` | REFINDEX | 1 | a=dst, b=ref (raw), c=index reg | Extend a reference by an index projection (checked in range). |
| `0x94` | LOAD | 1 | a=dst, b=ref | Read through a reference chain; a non-reference is copied. |
| `0x95` | STORE | 1 | a=ref (raw), b=value (raw) | Write through a reference, copy-on-write along the path. |
| `0xA0` | VECNEW | 1 | a=dst | Empty Vec. |
| `0xA1` | VECGET | 2 | a=dst, b=Vec, c=index, x=type of `std::Option` | `Some(v)` or `None`; never panics on the index. |
| `0xA2` | VECSET | 1 | a=ref (raw), b=index, c=value (raw) | Out of range panics. |
| `0xA3` | VECPUSH | 1 | a=ref (raw), b=value (raw) | Append. |
| `0xB0` | RUBY | 1 | a=dst, b=base, c=reading | Text with ruby. |
| `0xB1` | MRUBY | 1 | a=dst, b=base, c=array of readings | Text with one reading per character. |
| `0xB2` | EFFECT | 1 | a=dst, b=base, c=effect | Text with a `TextEffect` value. |
| `0xB3` | CONCAT | 1 | a=dst, b=array of Text | Concatenated Text (Array or Vec). |
| `0xB4` | TOTEXT | 1 | a=dst, b | Int/Decimal/Rat printed in decimal; Text as is; String as plain text. |

Run-time tag checks are not listed per row: an operand of the wrong tag panics with
`TagMismatch`, a `Uninit` value operand with `ReadUninit`.

### Intrinsics and imports {#intrinsics-and-imports}

A call of a function with a body is a CALL. A call of a std `#[builtin]` is an intrinsic
instruction when one exists, otherwise a CALLIMP of an import (`emit/codegen/call.rs`,
`FnCodegen::builtin`):

| Builtin | Emitted as |
| ------- | ---------- |
| `push_step` | STEP |
| `rng_choose::<T>` | RAND: a fieldless enum gives domain 0 and its type; an `Int<lo, hi>` domain 1 and its int type |
| `asset::<A>(path)`, `system_font(path)` | CONST of an Asset constant when the emitted argument is a constant string; otherwise CALLIMP of the import, like any other builtin |
| `Vec::new`, `Vec::len`, `Vec::get` | VECNEW, LEN, VECGET (with the `std::Option` type id) |
| `Vec::set`, `Vec::push` | VECSET, VECPUSH, then `TUPLE dst, 0, 0` (unit result) |
| `Vec::is_empty` | LEN, `INT 0`, EQ |
| `Integer::into` | MOVE |
| `Integer::saturate` | ICLAMP to the result type |
| `ruby`, `mono_ruby`, `effect`, `concat`, `to_text` | RUBY, MRUBY, EFFECT, CONCAT, TOTEXT |
| `Rat::new`, `Rat::to_decimal` | RATNEW, RATDEC |
| `std::ui::slider::<LO, HI>` | CALLIMP with 3 arguments: `props`, `LO`, `HI` (each `INT`) |
| every other builtin | CALLIMP of the import named by its std path |

A builtin used as a *value* (e.g. passed as a function pointer) gets a synthetic wrapper
function whose body is the same lowering; constructors used as values get an ADT wrapper.
CALLIMP arguments must be consecutive registers; the emitter moves them into an argument window
when they are not.

## Verification {#verification}

`verify` checks what the VM relies on without run-time checks; it reports the first failure
with its function and instruction index. `verify_code` is the same without rule 3's `main`
requirement (used by the compiler for programs without `main`).

1. **Indices.** Every index is in range: strings, types, int types, imports, assets,
   constants, statics, functions, debug files. Int type 0 is `i128`; every int type has
   `lo ≤ hi` and `bits` in `1..=128`. No import has kind scene.
2. **Constants.** Children have lower indices. An Adt constant's variant exists and its field
   count matches. A Handle constant names a setting or meta static.
3. **Entry points.** Exactly one function has the `main` flag; it is a `fn` with no
   parameters. Every `entry` function is a scene.
4. **Function shape.** `params ≤ regs`; the code is not empty; the last instruction is RET, JMP
   or UNREACHABLE (no fall-through); every register operand is `< regs`; every range
   `b..b+c` (call arguments, captures, aggregate elements) fits in `regs`; jump targets are
   inside the code.
5. **Arity.** CALL's `c` equals the callee's `params`, CALLIMP's the import's `params`; ADT's
   `c` equals the variant's field count; CLOSURE's `c` is at most the callee's `params`, and the
   callee has the closure flag.
6. **Kind rules.** For CALL, CALLIMP and the expected kind of CALLVAL, the caller's kind must
   allow the callee's kind (`FnKind::may_call`):

   | Caller | May call |
   | ------ | -------- |
   | `fn` | `fn`, `const fn` |
   | `const fn` | `const fn` |
   | instant | instant, `const fn` |
   | scene | scene, instant, `const fn` |

   STEP and RAND appear only in scenes. REFSTAT appears only in scenes and instant procedures,
   and only on a story-variable static.
7. **Types of type operands.** RAND's enum is an enum with at least one variant, all
   fieldless; RAND's int type exists. ISVAR's and ADT's variant exists. VECGET's type exists.

Value types are not checked statically. This is sound because the VM has no raw memory: every
access is a checked operation on a tagged value, so a type confusion can only panic. What the
verifier guarantees is what the interpreter would otherwise have to re-check on every
instruction: in-range registers and table indices, no fall-through, and the kind discipline
(the story/system separation must hold even for hand-crafted bytecode, because saves and the
story analysis depend on it).

## Panics {#panics}

A VM panic (`kagari_vm::Trap`) is a defined stop of the game; the engine shows it on the crash
screen. `TrapKind`:

| Kind | Raised by |
| ---- | --------- |
| `Overflow` | integer overflow or a result outside `[lo, hi]`; shift amount `≥ bits`; Decimal/Rat overflow; RAND range wider than 2^64 |
| `DivisionByZero` | IDIV, IREM, DDIV, RDIV, RATNEW with a zero divisor |
| `IndexOutOfRange` | INDEX, VECSET, REFINDEX, FIELD/REFFIELD past the end; a negative index |
| `ReadUninit` | reading an `Uninit` register or cell; RET of `Uninit` |
| `TagMismatch` | an operand of the wrong tag; EQ/NE of different tags; ISVAR on another type |
| `KindMismatch` | CALLVAL kind or arity mismatch; STEP/RAND/REFSTAT outside a story run |
| `CallDepth` | more nested frames than `max_call_depth` |
| `Unreachable` | UNREACHABLE; an internal fault the verifier should have excluded |
| `HostPartAccess` | LOAD, STORE, REFFIELD or REFINDEX on a host part |
| `FuelExhausted` | the instruction budget of a run is spent; a reference cycle deeper than 65 536 hops |
| `ClosureEquality` | EQ/NE on closures or host functions |
| `Host(message)` | an import refused its arguments |

Compound assignment on `Int<lo, hi>` saturates through ICLAMP: defined behaviour, not a panic.

## PRNG {#prng}

The story PRNG is xoroshiro128++ (`runtime_libs/kagari-vm/src/rng.rs`). Its state is
`(s0, s1)`, two `u64`, seeded from a `u64` seed by two SplitMix64 outputs: `s0` is the first
output and `s1` the second. SplitMix64's state `x` starts at the seed. All arithmetic wraps.

```text
splitmix64():
    x += 0x9E3779B97F4A7C15
    z = (x ^ (x >> 30)) * 0xBF58476D1CE4E5B9
    z = (z ^ (z >> 27)) * 0x94D049BB133111EB
    return z ^ (z >> 31)
```

For seed 0, `(s0, s1)` is `(0xE220A8397B1DCDAF, 0x6E789E6AA1B965F4)`, and the first `next()` is
`0x6F68E1E7E2646EE1`.

```text
next():
    result = rotl(s0 + s1, 17) + s0
    s1 ^= s0
    s0 = rotl(s0, 49) ^ s1 ^ (s1 << 21)
    s1 = rotl(s1, 28)
    return result
```

`uniform(n)` for `1 ≤ n ≤ 2^64`: when `n = 2^64` it is `next()`. Otherwise let
`t = n.wrapping_neg() % n`, loop `r = next()` until `r ≥ t`, and return `r % n`.

An enum draw is the variant index `uniform(variants)`; an `Int<lo, hi>` draw is
`lo + uniform(hi - lo + 1)`. The seed comes from the platform at `nav::start`
(`Platform::random_seed`); the PRNG state is part of the story state, so replaying a save
reproduces every draw.

## Compatibility {#compatibility}

**Decision.** One version tag in the magic, critical/ancillary section ids, and frozen opcodes.

**Why.** A runtime ships separately from games; it must either run a file exactly or refuse it
with a clear error. Optional data (debug info, future metadata) must not force a version bump.

**Consequences.**

- Opcodes and their meanings are frozen. A new operation gets a new opcode; an existing opcode
  is never repurposed or given a different operand layout.
- New optional data goes in ancillary sections (`0x80..`), which older loaders skip.
- A new required table is a new critical section, which older loaders reject by design.
- Engine extensions a module requires are named in Features with the required flag.
- The version `V000` changes only on a breaking change, once the format is declared stable.
  Until then (pre-1.0) the format may still change in place; every such change must update this
  chapter, the tests in `core/kagari-bytecode/src/tests.rs`, the VM and the emitter together.
- Saves do not depend on this layout: a save is a trace of the entry scene's path, its
  arguments, the PRNG seed and the choices taken (named by function path, choice id and arm),
  not of bytes or indices (see [Saves, Traces and Snapshots](../runtime/saves.md)).

## Invariants

- Little-endian, 8-byte aligned records, zero reserved fields and padding; one encoding per
  module (`decode(encode(m)) == m` and the decoder rejects every non-canonical encoding).
- The decoder never panics and checks only container and record encoding; semantic rules
  belong to the verifier. Do not move checks between them without updating both docs.
- The verifier is sound for the VM: anything the interpreter treats as a `fault()` (internal
  error) must be excluded by verification.
- `Instr::registers` and `Instr::range` list every register an instruction touches; the
  verifier relies on them.

## When you change this

- New opcode: add the `Instr` variant with its doc comment, `opcode`, `mnemonic`, `words`,
  `registers`/`range`, `encode`, `decode`; verifier rules; disassembler operands; VM semantics;
  the `full()` test module so `full_module_uses_every_opcode_and_tag` covers it; the emitter;
  regenerate the [opcode table](#opcode-table).
- New section or constant tag: encoder, decoder, verifier, `full()` test module, VM loader,
  this chapter.
- Load the `kagari-bytecode` skill (`.agents/skills/kagari-bytecode/SKILL.md`).
