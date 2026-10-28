# Diagnostics

Every diagnostic of the compiler has a code, a severity, a location and a message. Codes are a
public, documented interface: studios look them up in the Reference, and the language server
shows them. This chapter describes the types, how stages emit diagnostics, the code ranges, and
how to add a diagnostic end to end.

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-lang/src/diagnostic.rs` | `DiagnosticCode` (the `codes!` table), `Severity`, `Diag`, `Anchor`, `NodeId`, `Owner`, `Diagnostic`, `Related`, `render_diagnostics`, the `codes_are_numbered` unit test |
| `core/kagari-lang/src/pipeline.rs` | `check`: resolves every `Diag`, maps syntax errors to `E0001`, sorts and deduplicates |
| `core/kagari-lang/src/emit/mod.rs` | `build`: appends emit diagnostics (`E7xxx`) |
| `core/kagari-syntax/src/parse.rs` | `SyntaxError { message, range }` (no code) |
| `core/kagari-lang/tests/diagnostics_book.rs` | Every code has exactly one entry in the Reference diagnostics pages, in `en` and in `ja` |
| `core/kagari-lang/tests/game_source/` | Fixture programs whose snapshots hold rendered diagnostics |
| `docs/reference/{en,ja}/src/diagnostics/*.md` | The user documentation of every code |
| `tooling/kagari-lsp/src/diagnostics.rs` | Conversion to LSP diagnostics (see [IDE Layer](./ide.md#diagnostics)) |

## Types

```rust
pub enum Severity { Error, Warning }

/// Inside query results: anchored, no text ranges.
pub struct Diag {
    pub anchor: Anchor,
    pub code: DiagnosticCode,
    pub message: String,
    pub related: Vec<(Anchor, String)>,
    /// `{@N}` in `message` becomes the `line:col` of `message_anchors[N]`, `{#N}` its source text.
    pub message_anchors: Vec<Anchor>,
}

/// After `Diag::resolve`: what `check` and `build` return.
pub struct Diagnostic {
    pub file: FilePath,
    pub range: TextRange,
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    pub related: Vec<Related>, // { file, range, message }
}
```

`Anchor` names *what* a diagnostic points at, so it survives edits elsewhere in the file (see
[Pipeline and Queries](./pipeline.md#anchors-instead-of-ranges)):

| Anchor | Resolves to |
| ------ | ----------- |
| `Text(file, range)` | A raw range; only for parse errors and checks on the item tree or whole files |
| `ItemName(item)`, `ItemWhole(item)` | An item's name (or the whole item when unnamed); the whole item |
| `ItemAttr(item, i)`, `Variant(item, i)`, `GenericParam(item, i)` | The `i`-th attribute, enum variant name, generic parameter name |
| `Import(item, i)`, `ImportSegment(item, i, j)` | Import `i` of a `use`, or its `j`-th path segment |
| `Node(owner, node)` | A HIR expression, pattern or type of `Owner::Signature(item)` or `Owner::Body(item)`; `NodeId::PatBound(pat, 0 \| 1)` is a range-pattern bound |
| `Segment(owner, node, j)` | Segment `j` of the path a node owns |

`Diag::resolve(db)` looks anchors up in the item source map and the store source maps; an anchor
whose target vanished resolves to an empty range rather than failing.

The severity is not stored in `Diag`: it is `code.default_severity()`, fixed per code by the
`codes!` table. There are no lints to configure and no way to raise or lower a severity.

## Emitting a diagnostic

Stages 1–5 return diagnostics as part of their query results; nothing is printed or collected
through side channels.

```rust
// in a resolver, inferer or checker that owns a `Vec<Diag>`
self.diagnostics.push(
    Diag::new(
        Anchor::Node(Owner::Body(item), NodeId::Expr(expr)),
        DiagnosticCode::KindCallRule,
        format!("cannot call {desc} from {here}: …"),
    )
    .with_related(Anchor::ItemName(callee), format!("`{name}` is declared here")),
);
```

- Pick the narrowest anchor: a path segment over the whole path, an expression over the item.
- Use `with_related` for secondary locations ("first defined here", "declared here"); the kind
  checks have `Env::declared_here`, which skips std items.
- Put source text or positions into the message with `message_anchors` and `{#N}`/`{@N}`
  placeholders, never with ranges computed in the query.
- Write messages in the house style: lower case, no final period, code in backticks, the fact
  first and the reason or fix after a colon or semicolon
  (`` cannot mix `Int<0, 3>` and `u32`; convert one side with `.into()` or `.saturate()` ``).
- Report each root cause once. Skip constructs whose type `references_error()` or whose
  resolution is `Resolution::Error`; those were reported already.

Exceptions to the `Diag` path: syntax errors are `kagari_syntax::SyntaxError`s, turned into
`E0001` `Diagnostic`s by `check`; emit diagnostics are built directly as `Diagnostic`s at the root
file (`root_diagnostic`), since emission is not a query.

### Output

`render_diagnostics(db, diags, base)` prints one line per diagnostic and one indented `note:` line
per related location, with 1-based lines and char-based columns, paths relative to `base`:

```text
src/main.kix:12:5: error[E3001]: cannot call `helper` from the scene `Day1`: …
  note: src/main.kix:3:4: `helper` is declared here
```

`kagari check`, `kagari build` and the fixture snapshots use it; `"no diagnostics\n"` for none.

## Codes

A code is `E` or `W` and four digits. The first digit is the stage group; the prefix is the
severity. `diagnostic.rs`'s unit test `codes_are_numbered` enforces: unique, five characters,
first digit `0`–`7`, and `W` exactly for warning-severity codes.

| Range | Group | Emitted by |
| ----- | ----- | ---------- |
| `E0001` | Syntax (one code for every syntax error) | `kagari-syntax` (lexer, parser, `validate.rs`), mapped in `pipeline.rs` |
| `E1001`–`E1032` | Modules, items, names, asset paths | `item_tree.rs` (`E1005`–`E1013`), `def_map.rs` (modules, imports, impls, `E1026`), `resolve.rs` (names, labels, patterns, assets) |
| `E2001`–`E2038`, `W2039` | Types, exhaustiveness, refutability | `signatures/` (`E2002`–`E2011` in signatures), `infer/`, `typeck/exhaustive.rs` (`E2037`, `E2038`, `W2039`) |
| `E3001`–`E3024`, `W3023` | Kinds and story rules | `typeck/kinds.rs`, `typeck/kinds/*.rs` |
| `E4001`–`E4013` | Constant evaluation | `eval/` (machine, tabulation, const cycles) |
| `E5001`–`E5003`, `E5005`, `W5004`, `W5006`–`W5011` | Story analysis | `analysis/story/`, `analysis/influence.rs` |
| `E6002`, `E6003`, `W6001`, `W6004` | Game system checks | `analysis/system/` |
| `E7001`–`E7003` | Bytecode emission | `emit/mod.rs` |

The group is the kind of rule more than the query that reports it: an evaluation error that the
story exploration runs into (an overflow inside a render evaluated on a reached screen) is still
`E4001`, reported from `analysis/story/screens.rs`.

Numbers are assigned in order within a group, and warnings share the sequence with errors
(`E2038` is followed by `W2039`). The `DiagnosticCode` variant names carry the group as a prefix:
`Module…`/`Item…`/`Resolve…` (E1), `Type…` (E2), `Kind…` (E3), `Eval…` (E4), `Analysis…` (E5),
`System…` (E6), `Emit…` (E7), `Syntax` (E0).

### Names

The Reference gives every code a kebab-case name (`E7001` `missing-main`). Names exist only in the
documentation: the code never prints them. The convention is the variant name without its group
prefix, in kebab case (`KindInstantRecursion` → `instant-recursion`); a few names were chosen to
read better and do not follow it:

| Code | Variant | Name |
| ---- | ------- | ---- |
| `E1005` | `ModuleInBlock` | `file-module-in-block` |
| `E1014` | `ResolveDuplicate` | `duplicate-definition` |
| `E1017` | `ResolvePrivate` | `private-item` |
| `E1018` | `ResolveAmbiguous` | `ambiguous-name` |
| `E1029` | `ResolveAssetOutsideRoot` | `asset-outside-asset-dir` |
| `E2001` | `TypeMismatch` | `mismatched-types` |
| `E3016` | `KindAliasedMut` | `aliased-mut-argument` |

### Codes that user programs (almost) never reach

Each still has a Reference entry with an illustrative snippet and says so:

| Code | Why |
| ---- | --- |
| `E1013` (`trait-not-builtin`), `E1026` (`missing-std-item`) | Only a modified standard library triggers them; std ships inside the compiler |
| `E2012` (`bad-literal`) | The validator already rejects an unknown integer suffix as `E0001`, and HIR lowering turns an invalid literal into `Missing`; kept for literals that reach inference unchecked |
| `E7002` (`function-too-large`) | Needs more than 65 535 registers or more code than the bytecode addresses |
| `E7003` (`invalid-bytecode`) | Internal compiler error: emitted bytecode fails the verifier |

### Stability

> **Decision.** Codes are stable identifiers. A code is never renumbered, never reused for a
> different rule, and its severity never changes. When a rule disappears its code is retired: the
> variant and its Reference entry are removed and the number stays unused.
>
> **Why.** Studios search for codes, write them in notes and issue reports, and the editor shows
> them. A reused number would make old documentation and reports lie.
>
> **Consequences.** New rules always get the next free number in their group. Splitting a rule
> keeps the old code for the most common case. A message may be reworded freely; the meaning of
> the code may not change.

## Adding a diagnostic

1. **Choose the group and number.** The group is where the rule belongs (types `E2`, kinds `E3`, …),
   the number the next free one in that group. Use `W` only for a rule that never makes a program
   wrong to run (unreachable code, unused values, incomplete analysis).
2. **Add the variant** to `codes!` in `core/kagari-lang/src/diagnostic.rs`, in numeric order, with
   the group's prefix (`KindFooBar = "E3025";` or `KindFooBar = "W3025", warning;`).
3. **Emit it** from the query that owns the rule (see [Emitting a diagnostic](#emitting-a-diagnostic)),
   with an anchor, a message in the house style and related notes where useful. For stage 5,
   make sure the analysis still runs only behind the gate.
4. **Add a fixture case.** Extend the matching program in
   `core/kagari-lang/tests/game_source/<area>/NNN__topic/main.kix` (for example
   `script/010__type_errors`, `script/011__kind_errors`) or add a new `NNN__topic` directory with a
   `main.kix` (or `main.kis`) and the asset files it names. Run `cargo test -p kagari-lang
   --test game_source`, review the new snapshot in
   `tests/snapshots/game_source__game_source@<area>__NNN__topic__main.kix.snap`, and accept it (see
   [Testing](../process/testing.md)). A fixture without errors must also build: the test runs
   `build` on it.
5. **Document it in the Reference, in English and Japanese.** Add an entry to the group's page,
   `docs/reference/en/src/diagnostics/<group>.md` and the mirrored
   `docs/reference/ja/src/diagnostics/<group>.md`, in numeric order:

   ````markdown
   ## E3025 `foo-bar` {#e3025}

   **Error.** One or two sentences: what is wrong and why the rule exists.

   ```kix
   // the smallest program that triggers it, valid apart from this error
   ```

   Fix: how to change the program.
   ````

   The heading is ``## CODE `name` {#code}`` with the lower-case code as explicit id, identical in
   `ja`; the code block is byte-identical in both files; the Japanese prose follows the glossary of
   the Reference. If no source program can trigger it, say so after the snippet ("This snippet is
   illustrative: …").
6. **Run the book test**: `cargo test -p kagari-lang --test diagnostics_book`. For each language
   (`docs/reference/en/src/diagnostics` and `docs/reference/ja/src/diagnostics`) it collects every
   heading line `## <E|W>dddd ` of every `.md` file and asserts that each `DiagnosticCode::ALL`
   entry appears exactly once and no other code appears. Any other `## ` heading in those pages
   must therefore not start with a code.
7. **Link the rule** from the Reference or STD Reference page that states it (a sentence and a
   link to `diagnostics/<group>.md#e3025`), in both languages, and mention the code in the Develop
   Guide chapter that implements it.

Changing a message needs a snapshot update only. Removing a rule retires its code (step 2 and
5 in reverse).

## Invariants

- Every `DiagnosticCode` has exactly one Reference entry per language, under its group's page.
- `W` prefix ⇔ warning severity; severity comes from the code only.
- Query results contain `Diag`s with anchors, never `TextRange`s (except `Anchor::Text` for parse-
  and file-level checks).
- `check` output is sorted by file, range, code and message, and contains no exact duplicates.
- An error closes the analysis gate and prevents emission; a warning never does.
- Codes are never renumbered or reused.

## When you change this

- New or removed code: `diagnostic.rs`, a fixture, both Reference pages, the book test (see above).
- New `Anchor` kind: `resolve_anchor`, `Anchor::file`, and every source map that must record the
  range.
- Output format of `render_diagnostics`: every `game_source` snapshot, the Reference
  [diagnostics index](../../../reference/en/src/diagnostics/index.md) (en and ja), which shows the
  format, and the LSP conversion if fields change.
