# The `.kagari` Format

A game compiles to one `.kagari` file: a [header](./compilation.md#kagari-header), then sections
of tables and code. This chapter specifies the format version `V000`. The crate `kagari-bytecode`
implements it: the in-memory model, an encoder, a decoder, a load-time verifier and a
disassembler (`kagari disasm`).

## Conventions

- All multi-byte values are little-endian.
- Everything is aligned to 8 bytes. Every record below is a multiple of 8 bytes; *pad* means zero
  bytes up to the next 8-byte boundary.
- References to strings, types, constants, functions and the other tables are `u32` indices into
  their table. `NONE` is `u32::MAX`.
- Handles (the VM's pointers) are 64-bit integers, and `usize` is 64 bits.

## Layout

Bytes 0..16 are the magic `00 00 00 00 4B 41 47 41 52 49 00 00 56 30 30 30`
(`\0\0\0\0KAGARI\0\0V000`). A file whose magic matches except for the version digits has an
unsupported version.

Sections follow at offset 16. Each section is:

1. a 16-byte header: `id: u16`, `flags: u16` (= 0), `reserved: u32` (= 0), `len: u64`, where `len`
   is the exact payload length;
2. the payload;
3. zero padding to the next multiple of 8.

Sections appear in strictly ascending `id` order, each at most once. Ids `0x0001..=0x007F` are
**critical**: a decoder that does not know one rejects the file. Ids `0x0080..` are **ancillary**:
an unknown one is skipped. Every critical section below is required.

| Id | Section | Payload |
| -- | ------- | ------- |
| `0x01` | Strings | `count: u64`; per string: `len: u64`, UTF-8 bytes, pad |
| `0x02` | Features | `count: u64`; per feature: `name: u32` (string), `flags: u32` (bit 0 = required) |
| `0x03` | Types | `count: u64`; per ADT: `path: u32`, `flags: u32` (bit 0 = enum), `variants: u64`; per variant: `name: u32`, `fields: u32`, then `fields × u32` field names (`NONE` for positional), pad |
| `0x04` | IntTypes | `count: u64`; per entry: `lo: i128`, `hi: i128`, `bits: u32`, `signed: u32` (0/1), pad to 40 bytes. Entry 0 is always `i128` (`i128::MIN..=i128::MAX`, 128, 1) |
| `0x05` | Imports | `count: u64`; per import: `path: u32`, `kind: u16`, `params: u16` |
| `0x06` | Assets | `count: u64`; per asset: `kind: u32` (0 image, 1 audio, 2 voice, 3 video, 4 font, 5 shader), `path: u32` (string: `/`-separated, relative to the program's asset directory; a system font is `system-fonts/` and its path relative to its system font directory) |
| `0x07` | Constants | `count: u64`; per constant: `tag: u32`, `aux: u32`, then the payload by tag (see [Constants](#constants)) |
| `0x08` | Statics | `count: u64`; per static: `path: u32`, `kind: u32` (0 story variable, 1 setting, 2 meta), `init: u32` (constant), `reserved: u32` |
| `0x09` | Functions | `count: u64`; per function: `path: u32`, `kind: u8`, `flags: u8`, `params: u16`, `regs: u16`, `reserved: u16`, `words: u32`, then `words × u64` code |
| `0x0A` | Options | `max_call_depth: u32`, `reserved: u32` |
| `0x80` | Debug (ancillary) | `files: u64`, then `files × (path: u32, pad: u32)`; then one record per function, in function order: `file: u32` (index into `files`, or `NONE`), `count: u32`, then `count × (word: u32, offset: u32)`, mapping a code word index to a byte offset in that file |

- Function `kind`: 0 `fn`, 1 `const fn`, 2 instant `pushdown fn`, 3 scene.
- Function `flags`: bit 0 entry, bit 1 `main`, bit 2 closure, bit 3 synthetic (a constructor or
  builtin wrapper).
- Import `kind` uses the same numbering, but is never 3.
- `max_call_depth` is the compiler option [`recursion_depth`](./compilation.md#compiler-options).

## Constants

A constant may refer only to constants with a **lower** index.

| Tag | Value | `aux` | Payload |
| --: | ----- | ----- | ------- |
| 0 | Bool | 0/1 | — |
| 1 | Int | 0 | `i128` |
| 2 | Decimal | scale | `mantissa: i128` |
| 3 | Rat | 0 | `num: i128`, `den: i128` |
| 4 | String | string | — |
| 5 | Text, plain | string | — |
| 6 | Text, ruby | 0 | `base: u32`, `reading: u32` |
| 7 | Text, mono-ruby | count | `base: u32`, `count × u32` readings, pad |
| 8 | Text, effect | 0 | `base: u32`, `effect: u32` |
| 9 | Text, concat | count | `count × u32`, pad |
| 10 | Tuple (unit = 0 items) | count | `count × u32`, pad |
| 11 | Array | count | `count × u32`, pad |
| 12 | Vec | count | `count × u32`, pad |
| 13 | Adt | count | `type: u32`, `variant: u32`, `count × u32` fields, pad |
| 14 | Asset | asset | — |
| 15 | Function | function | — |
| 16 | Handle | static | — |

## Values

The VM has tagged registers and a heap of cells named by handles. Values carry their tag at run
time:

- `Uninit`, `Bool`, `Int` (i128), `Decimal`, `Rat`, `Text`, `String`;
- `Tuple`, `Array`, `Vec`, `Adt { type, variant, fields }`;
- `Asset`, `Func`, `Closure`, `Handle` (a setting or meta static);
- `Ref`, a reference;
- `Host(u64)`: an opaque engine object such as a `Screen`, `Element`, `Narrative` or
  `Divergence`, or a host part.

Every integer type is an `Int` at run time; `usize` values are `Int`s in `0..=u64::MAX`, and a
`u128` value above `i128::MAX` panics. Generics are monomorphized per instantiation, only so that
the compiler can fold type-dependent operations.

### References

A reference is a value `Ref { root, path }`:

- `root` is a heap cell, a story static, or a host part. A host part is an opaque handle returned
  by an import, for example a `&mut Entity`;
- `path` is a list of field and index projections.

LOAD, STORE, REFFIELD and REFINDEX on a host part panic: host parts are only passed back to
imports.

Value operands **auto-deref**: a reference is followed through every reference stored at its
target, and the final value is read. Operands marked *raw* are taken as they are.

## Instructions

A code word is a `u64` laid out as `op: u16 | a: u16 << 16 | b: u16 << 32 | c: u16 << 48`. A
2-word instruction is followed by an operand word `x: u64`. `a`, `b` and `c` are registers
(`r0..regs`, at most 65 535) unless noted otherwise. Registers start as `Uninit`. A jump target is
the word offset of an instruction start in the same function; a target inside an instruction is a
decode error.

| Op | Mnemonic | Words | Operands | Semantics |
| -- | -------- | ----: | -------- | --------- |
| `0x01` | MOVE | 1 | a=dst, b=src (raw) | copy a register |
| `0x02` | CONST | 2 | a=dst, x=constant | load a constant |
| `0x03` | BOOL | 1 | a=dst, b=0/1 | |
| `0x04` | INT | 2 | a=dst, x=i64 | sign-extended to i128 |
| `0x05` | FUNC | 2 | a=dst, x=function | function pointer |
| `0x06` | CLEAR | 1 | a | the register becomes `Uninit` |
| `0x10`–`0x19` | IADD ISUB IMUL IDIV IREM IAND IOR IXOR ISHL ISHR | 2 | a=dst, b, c, x=int type | i128 operation; panics on i128 overflow, division or remainder by zero, a shift ≥ `bits`, or a result outside `[lo, hi]`; shifts use `bits` and `signed` |
| `0x1A` | INEG | 2 | a, b, x=int type | checked negation |
| `0x1B` | INOT | 2 | a, b, x=int type | bitwise not within `bits`/`signed` |
| `0x1C` | ICLAMP | 2 | a, b, x=int type | clamp to `[lo, hi]` (saturation; never panics) |
| `0x20` | NOT | 1 | a, b | bool not |
| `0x21`–`0x23` | BAND BOR BXOR | 1 | a, b, c | non-short-circuit bool operations |
| `0x28`–`0x2B` | DADD DSUB DMUL DDIV | 1 | a, b, c | Decimal; DDIV is exact, then truncated to 7 digits after the point; panics on a zero divisor or overflow |
| `0x2C` | DNEG | 1 | a, b | Decimal negation |
| `0x30`–`0x33` | RADD RSUB RMUL RDIV | 1 | a, b, c | Rat; panics on a zero divisor or overflow |
| `0x34` | RNEG | 1 | a, b | Rat negation |
| `0x35` | RATNEW | 1 | a, b=num, c=den | panics on `den == 0` |
| `0x36` | RATDEC | 1 | a, b | Rat → Decimal, truncated to 7 digits |
| `0x40`–`0x45` | EQ NE LT LE GT GE | 1 | a, b, c | EQ/NE are structural over values (panic on closures); ordering on Int, Decimal or Rat of the same tag |
| `0x50` | JMP | 2 | x=target | |
| `0x51` | JMPIF | 2 | a=cond, x=target | |
| `0x52` | JMPNOT | 2 | a=cond, x=target | |
| `0x53` | RET | 1 | a (raw) | return |
| `0x54` | UNREACHABLE | 1 | — | panic |
| `0x60` | CALL | 2 | a=dst, b=first arg, c=argc, x=function | arguments are the raw registers `b..b+c` |
| `0x61` | CALLIMP | 2 | a, b, c, x=import | host call |
| `0x62` | CALLVAL | 2 | a, b, c, x = callee register (bits 0–15) \| expected kind (bits 16–23) | the callee is a FUNC value or a closure; panics if its kind differs |
| `0x63` | CLOSURE | 2 | a=dst, b=first capture, c=count, x=function | captures are copied by value |
| `0x70` | STEP | 1 | a=dst, b=step record | suspend until input; scenes only |
| `0x71` | RAND | 2 | a=dst, x = kind (bits 32–39: 0 enum, 1 int) \| index (bits 0–31: type or int type) | PRNG draw; scenes only |
| `0x80` | TUPLE | 1 | a, b=first, c=count | |
| `0x81` | ARRAY | 1 | a, b, c | |
| `0x82` | REPEAT | 2 | a, b=value, x=count (u64) | `[v; n]` |
| `0x83` | ADT | 2 | a, b, c=count, x = type (bits 0–31) \| variant (bits 32–63) | struct or variant value |
| `0x84` | FIELD | 1 | a, b, c=field index (immediate) | Tuple or Adt field |
| `0x85` | ISVAR | 2 | a, b, x = type \| variant << 32 | bool: `b` is that variant |
| `0x86` | INDEX | 1 | a, b=array or Vec, c=index | panics when out of range |
| `0x87` | LEN | 1 | a, b | array or Vec length (`usize`) |
| `0x90` | CELL | 1 | a=dst, b=init (raw) | a new heap cell; `dst` is a reference to it |
| `0x91` | REFSTAT | 2 | a, x=static | reference to a story variable |
| `0x92` | REFFIELD | 1 | a, b=ref, c=field (immediate) | |
| `0x93` | REFINDEX | 1 | a, b=ref, c=index register | panics when out of range |
| `0x94` | LOAD | 1 | a, b=ref | read through a reference, following chains |
| `0x95` | STORE | 1 | a=ref, b=value (raw) | write through a reference |
| `0xA0` | VECNEW | 1 | a | empty Vec |
| `0xA1` | VECGET | 2 | a, b, c=index, x=type of `std::Option` | `Some(v)` or `None` |
| `0xA2` | VECSET | 1 | a=ref, b=index, c=value (raw) | panics when out of range |
| `0xA3` | VECPUSH | 1 | a=ref, b=value (raw) | |
| `0xB0` | RUBY | 1 | a, b=base, c=reading | Text |
| `0xB1` | MRUBY | 1 | a, b=base, c=array of readings | |
| `0xB2` | EFFECT | 1 | a, b=base, c=effect | |
| `0xB3` | CONCAT | 1 | a, b=array of Text | |
| `0xB4` | TOTEXT | 1 | a, b | Int in decimal; Decimal and Rat as the compiler prints them; Text as is; String as plain text |

### Intrinsics and imports

A call of a function with a body is a CALL. A call of a standard-library builtin is an intrinsic
instruction when one exists, and otherwise a host call:

| Builtin | Instruction |
| ------- | ----------- |
| `push_step` | STEP |
| `rng_choose::<T>` | RAND (an enum gives kind 0 and its type; an `Int<lo, hi>` kind 1 and its int type) |
| `asset` | CONST of an Asset constant |
| `Vec::new`, `Vec::len`, `Vec::get`, `Vec::set`, `Vec::push` | VECNEW, LEN, VECGET, VECSET, VECPUSH |
| `Vec::is_empty` | LEN, INT 0, EQ |
| `Integer::into` | MOVE |
| `Integer::saturate` | ICLAMP to the result type |
| `ruby`, `mono_ruby`, `effect`, `concat`, `to_text` | RUBY, MRUBY, EFFECT, CONCAT, TOTEXT |
| `Rat::new`, `Rat::to_decimal` | RATNEW, RATDEC |
| every other builtin | CALLIMP of an import named by its std path, such as `std::narrative::Meta::write` |

An import's `kind` is the builtin's kind (a `pushdown fn` is instant), and its `params` count
includes `self`.

## Verification

A loader verifies a module before running it, and rejects it on the first failure:

1. Every index is in range: strings, types, int types, imports, assets, constants, statics,
   functions, features.
2. Constants: children have lower indices; an Adt variant exists and its field count matches;
   Handle constants point to setting or meta statics.
3. Exactly one function has the `main` flag, with kind `fn` and no parameters. Every entry
   function is a scene.
4. Per function: every register operand is `< regs`; `params ≤ regs`; the code is not empty; the
   last instruction is RET, JMP or UNREACHABLE (no fall-through); argument and capture ranges
   `b..b+c` fit in `regs`.
5. Calls: CALL's `c` equals the callee's `params`, CALLIMP's `c` the import's `params`; CLOSURE's
   `c` is at most the callee's `params`, and the callee has the closure flag; ADT's `c` equals the
   variant's field count.
6. Kind rules. For CALL, CALLIMP and the expected kind of CALLVAL, the caller's kind allows these
   callee kinds:

   | Caller | Callees |
   | ------ | ------- |
   | `fn` | `fn`, `const fn` |
   | `const fn` | `const fn` |
   | instant `pushdown fn` | instant, `const fn` |
   | scene | scene, instant, `const fn` |

   STEP and RAND appear only in scenes. REFSTAT appears only in scenes and instant procedures, and
   only on a story-variable static. RAND's type is an enum whose variants all have no fields, and
   its int type exists.
7. The types of ISVAR, ADT and VECGET are ADTs; the variants of ISVAR and ADT exist.

Types are not checked statically. Value tags are checked at run time, and a mismatch panics. This
is sound because the VM has no raw memory.

## Panics

A VM panic is a defined stop of the game that the runtime handles in a platform-specific way. The
VM panics on:

- integer overflow or a result outside the range of its type;
- division by zero;
- an index out of range;
- reading `Uninit`;
- a value tag mismatch;
- a CALLVAL kind mismatch;
- more nested calls than `max_call_depth`;
- `UNREACHABLE`;
- LOAD, STORE, REFFIELD or REFINDEX on a host part.

Compound assignment on `Int<lo, hi>` saturates (ICLAMP); that is defined behaviour, not a panic.

## Compatibility

- Opcodes and their meanings are frozen.
- New optional data goes in ancillary sections, which older loaders skip.
- Engine extensions that a module requires are named in the Features section, with the required
  flag set.
- Once the format is stable, the version `V000` increases only on a breaking change.

## PRNG

The story PRNG is xoroshiro128++. Its state is `(s0, s1)`, two `u64`, seeded from a `u64` seed by
two SplitMix64 outputs. All arithmetic wraps.

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
`lo + uniform(hi - lo + 1)`.
