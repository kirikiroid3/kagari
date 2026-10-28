# Compiler Rules for std Items

This appendix lists the rules the compiler enforces for std items beyond their signatures: which
std types the story may hold, how literals and forms are typed, the special signatures, and which
evaluation failures are compile errors. Each rule names the diagnostic you get when it is broken;
the diagnostics are described in
[The Kagari Reference](../../../../reference/en/src/diagnostics/index.md).

## Kinds

- Every std function has the kind it is declared with: the `Kind:` badge of its page.
- The call rules of the language apply to std methods and builtins exactly as to your own
  functions ([`E3001`](../../../../reference/en/src/diagnostics/kinds.md#e3001)). So a render
  (`const fn`) cannot call `Setting::get`, and a UI component (`fn`) cannot call `push_step`.
- `push_step` (and so `s!` and `choice!`) and `rng_choose` are callable only from a scene
  ([`E3002`](../../../../reference/en/src/diagnostics/kinds.md#e3002),
  [`E3024`](../../../../reference/en/src/diagnostics/kinds.md#e3024)).

## Finite types

The story can hold only finite types at runtime
([`E3008`](../../../../reference/en/src/diagnostics/kinds.md#e3008)). Among std types, these are
finite:

- `Text`;
- the six asset types (`ImageAsset`, `AudioAsset`, `VoiceAsset`, `VideoAsset`, `FontAsset`,
  `ShaderAsset`);
- `Render` (a `const fn` pointer);
- `Option`, `Result`, `Range` and other std structs and enums whose fields are all finite, for
  example `Visibility` or `PlayMode`.

These are not finite:

- `String`, `Vec<T>`, `Rat`, `Decimal`;
- `Screen`, `Narrative`, `Meta`, `Setting` and every other `#[builtin]` handle or state type;
- data that holds a Rust integer or a `Decimal`: `Duration`, `Color`, `EntityId`, `Part`, `Mark`,
  `Point`, `TextEffect`, `TextStyle`, …

Story code can still name non-finite values in constant expressions, which are folded: `ms(300)`
or `Mark(1)` in a step is fine. A `const fn` call with runtime arguments is tabulated, and its
result must be finite.

## Type rules

- A string literal has type `Text`; a literal with a decimal point has type `Decimal`.
- `{expr}` in `t![…]` is `to_text(expr)`, so `expr: T` with `T: ToText`
  ([`E2026`](../../../../reference/en/src/diagnostics/types.md#e2026)).
- In the story, `to_text` of a finite runtime value is tabulated like any `const fn` call, so the
  line stays a finite function of the story state.
- An `#[entry]` scene's parameter and return types must not contain `Text`, because `fn` code can
  build runtime text with `to_text`
  ([`E3009`](../../../../reference/en/src/diagnostics/kinds.md#e3009)).
- `a..b` and `a..=b` have type `Range<T>`, where `T` is a type both `a` and `b` convert to: `0..7`
  is a `Range<Int<0, 7>>`. A range expression needs both bounds: `a..`, `..b` and `..` exist only
  in patterns ([`E2030`](../../../../reference/en/src/diagnostics/types.md#e2030)).
- `x.into()` with `x: S` and target `T`, both `Integer`, needs every value of `S` to fit in `T`
  ([`E2018`](../../../../reference/en/src/diagnostics/types.md#e2018)). `x.saturate()` has no
  such condition.

## Forms

- `choice!` ids are string literals, unique within their function
  ([`E3019`](../../../../reference/en/src/diagnostics/kinds.md#e3019)).
- `choice!` arm values are fieldless enum variants, each at most once per choice
  ([`E3020`](../../../../reference/en/src/diagnostics/kinds.md#e3020)).
- A ruby base is a string literal; mono-ruby readings are an array literal with one reading per
  base character ([`E3021`](../../../../reference/en/src/diagnostics/kinds.md#e3021)).
- The effect of `!["…"](e)` is a constant expression
  ([`E3022`](../../../../reference/en/src/diagnostics/kinds.md#e3022)).

## Traits

- Only `std` declares traits, and each one is `#[builtin]`. A `trait` item or an
  `impl Trait for T` in a game is an error
  ([`E1012`](../../../../reference/en/src/diagnostics/modules.md#e1012),
  [`E1023`](../../../../reference/en/src/diagnostics/modules.md#e1023)).
- `Eq` holds for primitives, `Int`, function pointers, and structs, enums, tuples and arrays whose
  fields are all `Eq`; for a `#[builtin]` type, only through an `impl Eq` in `std`. Closures are
  never `Eq`. `==` on a non-`Eq` type is
  [`E2017`](../../../../reference/en/src/diagnostics/types.md#e2017).
- An unsatisfied bound (`A: Asset`, `T: RandChoose`, `T: ToText`, …) is
  [`E2026`](../../../../reference/en/src/diagnostics/types.md#e2026).

## `asset::<A>`

- It needs `A: Asset` ([`E2026`](../../../../reference/en/src/diagnostics/types.md#e2026)).
- Its argument must be a string literal
  ([`E2028`](../../../../reference/en/src/diagnostics/types.md#e2028)), not empty
  ([`E1028`](../../../../reference/en/src/diagnostics/modules.md#e1028)), inside the asset
  directory ([`E1029`](../../../../reference/en/src/diagnostics/modules.md#e1029)), and the file
  must exist ([`E1027`](../../../../reference/en/src/diagnostics/modules.md#e1027)).
- `system_font` follows the same literal rule; its own errors are
  [`E1030`](../../../../reference/en/src/diagnostics/modules.md#e1030)–[`E1032`](../../../../reference/en/src/diagnostics/modules.md#e1032).

## `nav::start(entry, args, on_exit)`

The signature `start<E, A, R>(entry: E, args: A, on_exit: Fn(R))` is special
([`E2027`](../../../../reference/en/src/diagnostics/types.md#e2027)):

- `entry` is a `scene fn(P1, …, Pn) -> R` pointer to an `#[entry]` scene;
- `args` is the tuple `(P1, …, Pn)`;
- `on_exit` is `Fn(R)`.

## Settings and meta in `const fn`

- A `Setting<T>` handle may appear in a `const fn` only as a value, that is, passed to
  `Entity::gate`. `get` and `set` are `fn`.
- Meta fields cannot be used in `const` code at all
  ([`E3013`](../../../../reference/en/src/diagnostics/kinds.md#e3013)).
- `Setting` and `Meta` handles exist only as the initializer of an immutable `static`
  ([`E3011`](../../../../reference/en/src/diagnostics/kinds.md#e3011),
  [`E4007`](../../../../reference/en/src/diagnostics/eval.md#e4007)).

## `#[must_use]`

It covers `Change<T>`, `Swap` and `ChannelChange`. A pending change that is discarded, or bound to
a local and never completed with `apply_animation`, is a warning
([`W3023`](../../../../reference/en/src/diagnostics/kinds.md#w3023)), and the change does not
happen.

## `for` loops

- `for` over `Vec<T>`, arrays and `Range<T>` of an `Integer` `T` is allowed in `fn` and
  `const fn`. Anything else is not iterable
  ([`E2031`](../../../../reference/en/src/diagnostics/types.md#e2031)).
- In a `pushdown fn`, `for` loops only over arrays and over ranges whose bounds are constant
  expressions ([`E3006`](../../../../reference/en/src/diagnostics/kinds.md#e3006)).

## Const-evaluation errors

When std code runs during const evaluation, these failures are compile errors:

|Failure|Diagnostic|
|---|---|
|Integer overflow|[`E4001`](../../../../reference/en/src/diagnostics/eval.md#e4001)|
|Division by zero (`Rat::new(_, 0)`, `Duration::div(0)`, …)|[`E4002`](../../../../reference/en/src/diagnostics/eval.md#e4002)|
|`Vec::set` out of range|[`E4005`](../../../../reference/en/src/diagnostics/eval.md#e4005)|
|A `Setting` or `Meta` handle outside a `static` initializer|[`E4007`](../../../../reference/en/src/diagnostics/eval.md#e4007)|
|The `Motion` rules: no keys, `at` not strictly increasing or equal to 0, a last key that is not identity, `repeat` = 0|[`E4008`](../../../../reference/en/src/diagnostics/eval.md#e4008)|
|`CubicBezier` x outside 0–1, and `Steps(0)`|[`E4009`](../../../../reference/en/src/diagnostics/eval.md#e4009)|
|A duplicate text mark|[`E4010`](../../../../reference/en/src/diagnostics/eval.md#e4010)|
|`begin_text` while anchored on a mark|[`E4011`](../../../../reference/en/src/diagnostics/eval.md#e4011)|
|A `Decimal` fraction outside 0.0–1.0 where required: opacity, volume, anchor|[`E4012`](../../../../reference/en/src/diagnostics/eval.md#e4012)|
|`Duration` overflow or underflow|[`E4013`](../../../../reference/en/src/diagnostics/eval.md#e4013)|

At runtime, the same failures in `fn` code are evaluation errors of the running game.
