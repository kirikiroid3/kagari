# `.kagari-cache` Files

A `.kagari-cache` file holds what the compiler learned about a program beyond its bytecode: the
explored story graph of the story analysis and, for a debug build, the debug information of every
bytecode function. Tools and the runtime read it without linking the compiler.

Where in the code:

| Path | Contents |
| ---- | -------- |
| `core/kagari-debug/src/lib.rs` | `EXTENSION`, `ANALYSIS_FILE`, `DEBUG_FILE`, `SCHEMA`, `Error` |
| `core/kagari-debug/src/file.rs` | `AnalysisCache`, `DebugArtifact`, `DiagnosticRecord`, the root tables |
| `core/kagari-debug/src/graph.rs` | `StoryGraph` and its walk (`reached`, `reached_steps`, `step_id`, `locals`, `vars`, `is_keyframe`) |
| `core/kagari-debug/src/value.rs` | `Value`, `Text` and their cells |
| `core/kagari-debug/src/debug_info.rs` | `DebugInfo` and its tables |
| `core/kagari-debug/src/codec.rs` | Typed field access over `kagari_archive::save` |
| `core/kagari-debug/src/tests.rs` | Round trips, the walk, rejections |
| `core/kagari-lang/src/analysis/story/graph.rs`, `src/cache.rs` | The producer of the graph and of the analysis cache; see [Story and System Analysis](../compiler/analysis.md#the-persisted-story-graph) |
| `core/kagari-lang/src/emit/debug.rs` | The producer of the debug information; see [Emitting Bytecode](../compiler/emit.md#debug-information) |
| `tooling/kagari-cli/src/analysis_cache.rs`, `src/command/build/mod.rs` | Where the files are read and written; see [Projects and the CLI](../tooling/cli.md) |

`kagari-debug` depends only on `kagari-archive`, so runtime crates (which must not depend on
`kagari-lang`) can read the graph.

## Container {#container}

**Decision.** A `.kagari-cache` file is a [`.kisave`](./archive.md#kisave) database, uncompressed,
with its own extension and a `kind` field at the root.

**Why.** The content is a database-like tree of tables and arrays, which `.kisave` already
encodes, validates eagerly and versions (`SAVEKAGARI\0\0V000`). One more container would need its
own encoder, validator and tests. Compression is not needed: the file is a local build artifact,
and `.kisave` pages (16 KiB) keep small files small.

**Consequences.** Every read validates the whole tree first (`Save::from_bytes`), so a corrupt file
is an `Error`, never a panic or a wrong graph. Field names are stored per table, so the files are
larger than a packed encoding would be: `example/test_game1` (30 states) gives a 48 KiB analysis
cache and a 416 KiB debug artifact (261 functions); a story at the default `state-cap` (200 000
states, 3 story variables) gives about 142 MiB, read and validated in about 0.25 s by a release
build.

## The two kinds {#kinds}

| Kind (`kind`) | Type | Path | Written by | Keyed by |
| ------------- | ---- | ---- | ---------- | -------- |
| `analysis` | `AnalysisCache` | `<out-dir>/analysis.kagari-cache` | `kagari check`, `kagari build` (when the analysis ran) | the source fingerprint |
| `debug` | `DebugArtifact` | `<out-dir>/<target>/game.kagari-cache` | `kagari build --profile debug` | the script hash of `game.kagari` |

The **source fingerprint** is the lowercase hex SHA-256 of every input the story analysis reads
(`kagari_lang::cache::source_fingerprint`); the **script hash** is the lowercase hex SHA-256 of the
`game.kagari` bytes, as the engine computes it for saves. A reader that computes another value
must ignore the file: the compiler reuses an analysis cache only for an equal fingerprint, and a
debugger must reject a debug artifact whose script hash is not that of the bytecode it runs.

## Root table {#root}

| Field | Type | Kinds | Meaning |
| ----- | ---- | ----- | ------- |
| `kind` | String | both | `analysis` or `debug`; another value is `Error::Kind` |
| `schema` | I64 | both | `SCHEMA` (3); another value is `Error::Schema` |
| `toolchain` | String | both | The `kagari-lang` version that wrote the file |
| `fingerprint` | String | both | The source fingerprint |
| `diagnostics` | Array of tables | `analysis` | The stage-5 diagnostics, resolved (below) |
| `script_hash` | String | `debug` | The script hash of the matching `game.kagari` |
| `story` | Table or Null | both | The story graph; Null when the analysis explored nothing |
| `debug` | Table | `debug` | The debug information |

Optional fields are absent or Null. Ids and counts are I64 cells (readers also accept U64 and
I128 in range).

### Diagnostic records

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `file` | String | The file: an absolute disk path, or the path under the std directory |
| `std` | Bool | Whether `file` names a std file |
| `start`, `end` | I64 | Byte offsets into the file |
| `severity` | String | `error` or `warning` |
| `code` | String | The diagnostic code, e.g. `W5009` |
| `message` | String | The message, resolved |
| `related` | Array of tables | `file`, `std`, `start`, `end`, `message` per related location |

The compiler refuses a whole analysis cache whose records name an unknown code or have
`start > end`.

## The story graph {#story}

The analysis's explored state space (see
[Exploration](../compiler/analysis.md#exploration)): per *context* (a scene instance entered with
given arguments and story variables), its states; a scene call leads into the callee's context and
resumes once per exit of it. Values, frames and story-variable sets are stored once and referred
to by index.

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `complete` | Bool | Every reachable state was visited |
| `incomplete` | Array of `{code, message}` | Why not: `W5006` (a cap) or `W5007` (a path given up) |
| `vars` | Array of String | The story variables (`static mut`) by path |
| `values` | Array of values | Every value, stored once |
| `frames` | Array of arrays of (I64 or Null) | Kept locals: a value index per local of `Point::locals`, Null when uninitialized |
| `globals` | Array of arrays of I64 | Story-variable values, in the order of `vars` |
| `scenes` | Array of `{path, points}` | Per scene its item path and `points`: `{block, locals}` per suspension block some node is at, ascending by block; `locals` are the names of the kept locals in frame order (`""` for a temporary) |
| `contexts` | Array of tables | Below |
| `roots` | Array of I64 | The contexts of the entry scenes, one per argument value |
| `keyframes` | Array of I64 | The value indices of the render functions nodes refer to (`render`) that are keyframe renders (their output does not depend on the input screen), ascending and unique; any other render a node refers to is a delta render |

Context: `scene` (index into `scenes`), `path` (the instance path as the bytecode names the
function, generic arguments included), `args` (value indices), `globals` (at entry), `entry`
(target), `nodes`, `exits` (`{ret, globals, outs}`: returned value, story variables at return,
final values of the `&mut` parameters), `callers` (`[context, node]` pairs of the call nodes that
enter it).

Node: `kind`, `block`, `frame`, `globals`, `next` (an array of targets), and per kind:

| `kind` | Fields | `next` |
| ------ | ------ | ------ |
| `say` | `step`, `render` | One target |
| `choice` | `step`, `render`, `arms`: the arm ID per shown arm | One target per arm |
| `call` | `callee` (a context), `exits`: the callee's exit per successor | One target per exit |
| `draw` | `draws`: the drawn value per successor | One target per value |

`step` is the step's ordinal in its function: the **step ID** is `{context path}#{step}`, the name
the bytecode, the runtime and diagnostics use. An **arm ID** is the arm value's variant path
(`crate::Pick::A`) or `#{index}` of the arm in the arm list, exactly as saves record choices
([Saves, Traces and Snapshots](../runtime/saves.md)). `render` is the index of the step's render
function value, or Null; `StoryGraph::is_keyframe(render)` tells whether it is a keyframe render.

A **target** is a cell: node `n` of the same context is I64 `n`, exit `e` is I64 `-1 - e`, and a
dead end (a fault, or a path the analysis gave up) is Null.

### Values

The way the engine stores values in saves: scalars as scalars, the rest as tables tagged by the
field `"$"`. Functions, closures and screens, which saves cannot hold, get their own tags.

| Value | Cell |
| ----- | ---- |
| `bool` | Bool |
| integers | I128 |
| `Decimal`, `Rat` | Decimal, Rat |
| `String` | String |
| `Text` | `{"$": "text", t}`; a text node is a String (plain) or `{"$": "ruby", base, reading}`, `{"$": "mono_ruby", base, readings}`, `{"$": "effect", base, effect}`, `{"$": "concat", items}` |
| tuple, array, `Vec` | `{"$": "tuple" \| "array" \| "vec", items}` |
| struct, enum value | `{"$": "adt", type, variant, fields}`: type path, variant name (a struct's is its own name) |
| asset | `{"$": "asset", path}`: the bytecode asset path |
| `Setting<T>`, `Meta<T>` handle | `{"$": "handle", path}`: the static's path |
| function | `{"$": "fn", path}`: the instance path |
| closure | `{"$": "closure", path}`: `{owner}::{closure#N}` |
| `Screen` | `{"$": "screen"}`; its content is not stored |

## Debug information {#debug}

`debug.functions` is parallel to the bytecode's function table. Per function:

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `path` | String | The function's path in the bytecode |
| `file` | String or Null | Its source file as the bytecode's Debug section names it; Null for constructors and builtins |
| `body_hash` | String | Hex SHA-256 of the body as the compiler sees it, with its item's promoted constants (its step records and texts), unchanged by comments, layout and edits elsewhere (for hot swap) |
| `scopes` | Array of `{parent, start, end}` | Lexical scopes as source byte ranges; a parent comes before its children |
| `locals` | Array of `{name, scope, ranges}` | Named locals, parameters included; `ranges`: `{start, end, register, cell}` per register holding the local over the half-open code-word range `start..end` (`cell`: the register holds a reference to the local's heap cell) |

`debug.statics` is parallel to the bytecode's static table (story variables, then `Setting` and
`Meta` handles). Per static:

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `path` | String | The static's path in the bytecode |
| `type` | String | Its type with every item fully qualified (`Int<0, 9>`, `std::Option<crate::Mood>`); hot swap restarts the story when a story variable's type changes |

How the emitter derives each field is in
[Emitting Bytecode](../compiler/emit.md#debug-information). The bytecode itself is the same in a
debug and a release build.

## Reading

```rust
let artifact = kagari_debug::DebugArtifact::from_bytes(std::fs::read(path)?)?;
assert_eq!(artifact.script_hash, script_hash_of(&bytecode));
let story = artifact.story.expect("an explored story");
for step in story.reached_steps() {
    println!("{step}"); // crate::Start#0, …
}
```

`StoryGraph::reached` walks from the roots: within a context along the targets of each node, into
the callee's context at a call (before the caller resumes), visiting each context and node once.
`locals(context, node)` pairs the kept-local names of the node's point with its frame, and
`vars(context, node)` the story-variable paths with its values.

## Invariants

- Both kinds are valid `.kisave` files; writing goes through `Save` and `compact()`, reading
  through `Save::from_bytes`, so a loaded file is a valid tree.
- The same program gives the same bytes: the graph's order is the exploration's discovery order
  and every table's fields are sorted by name.
- Indices in a graph refer into the same graph; a node's `next` has exactly as many targets as its
  labels (`arms`, `exits`, `draws`; one for `say`).
- A debug artifact never changes `game.kagari`.

## When you change this

- **The stored shape or meaning** of any field: bump `SCHEMA` (old files are then rejected and
  rewritten), update the tables in this chapter and `tests.rs` (round trip and the rejection).
- **What the analysis computes** for the same sources: bump `FINGERPRINT_SALT` in
  `core/kagari-lang/src/cache.rs` (see
  [Story and System Analysis](../compiler/analysis.md#reusing-the-analysis)).
- **A new value kind**: a tag in `value.rs`, its conversion in `analysis/story/graph.rs`, the table
  above; keep the tags the engine uses for saves identical
  (`runtime_libs/kagari-engine/src/store/value.rs`).
- Load the `kagari-bytecode` skill (`.agents/skills/kagari-bytecode/SKILL.md`), which covers the
  binary formats.
