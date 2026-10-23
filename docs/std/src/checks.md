# Checks

This chapter is the contract between the library and the type and kind checker (stage 3 of the
compiler). Stage 3 infers types and checks the type rules, `Eq` and the other trait bounds,
`asset::<A>` and `nav::start`, and every rule below: the kind rules, finite types, settings in
`const fn`, `#[must_use]` and `for`-loop constness.

## Kinds

- Every std function's kind is the kind it is declared with.
- The call rules of the language apply to methods and to builtins exactly as to user functions.
  So a render (`const fn`) cannot call `Setting::get`, and a component (`fn`) cannot call
  `push_step`.

## Finite types

The finite types of the language are extended with:

- the five asset types (`ImageAsset`, `AudioAsset`, `VoiceAsset`, `VideoAsset`, `FontAsset`);
- `Render`;
- `EntityId`, `Part` and `Mark`, only as constants;
- `Text`.

These are not finite:

- `Screen`, `Narrative` and every builtin screen state type;
- `Duration`, `Color`, and all `u32`/`i32`/`Decimal`-based data.

Story code can still name them in constant expressions, which are folded.

## Type rules

- A string literal has type `Text`; a literal with a decimal point has type `Decimal`.
- `{expr}` in `t![…]` is `to_text(expr)`, so `expr: T` with `T: ToText`.
- In a `pushdown fn`, `to_text` of a finite runtime value is tabulated like any `const fn` call,
  so the line stays a finite function of σ.
- An `#[entry]` scene's parameter types must not contain `Text`, because `fn` code can build
  runtime text with `to_text`.
- `a..b` and `a..=b` have type `Range<T>`, where `T` is a type both `a` and `b` convert to:
  `0..7` is a `Range<Int<0, 7>>`. A range expression needs both bounds: `a..`, `..b` and `..`
  exist only in patterns.
- `x.into()` with `x: S` and target `T`, both `Integer`, needs every value of `S` to fit in `T`.
  `x.saturate()` has no such condition.

## Traits

- Only `std` declares traits, and each one is `#[builtin]`. A `trait` item or an
  `impl Trait for T` in the game is an error.
- `Eq` holds for primitives, `Int`, function pointers, and structs, enums, tuples and arrays whose
  fields are all `Eq`; for a `#[builtin]` type, only through an `impl Eq` in `std`. Closures are
  never `Eq`.

## `asset::<A>`

- It needs `A: Asset`.
- Its argument must be a string literal.

## `nav::start(entry, args, on_exit)`

The signature `start<E, A, R>(entry: E, args: A, on_exit: Fn(R))` is special:

- `entry` is a `scene fn(P1, …, Pn) -> R` pointer to an `#[entry]` scene.
- `args` is the tuple `(P1, …, Pn)`.
- `on_exit` is `Fn(R)`.

## Settings in `const fn`

A `Setting<T>` handle may appear in a `const fn` only as a value, i.e. passed to `Entity::gate`.
`get` and `set` are `fn`.

## `#[must_use]`

It covers `Change<T>`, `Swap` and `ChannelChange`. A pending change that is discarded, or bound
to a local and never completed with `apply_animation`, is a warning, and the change does not
happen.

## `for` loops

- `for` over `Vec<T>`, arrays and `Range<T>` of an `Integer` `T` is allowed in `fn` and
  `const fn`.
- In a `pushdown fn`, `for` loops only over arrays and over ranges whose bounds are constant
  expressions.

## Const-evaluation errors

These are compile errors:

- the Motion rules: no keys, `at` not strictly increasing or equal to 0, a last key that is not
  identity, `repeat` = 0;
- `CubicBezier` x outside 0–1, and `Steps(0)`;
- duplicate text marks;
- `begin_text` while anchored on a mark;
- `Vec::set` out of range;
- `Duration` overflow or underflow;
- a `Decimal` fraction outside 0.0–1.0 where the rules require it: opacity, volume, anchor,
  filter amounts except brightness/contrast/saturation, which must be ≥ 0.
