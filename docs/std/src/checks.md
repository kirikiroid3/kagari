# Checks

This chapter is the contract between the library and the type and kind checker (stage 3 of the
compiler). Stage 3 is not implemented yet; today the compiler resolves std names and reports
resolution errors only.

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

- A string literal has type `Text`.
- `{expr}` in `t![…]` is `to_text(expr)`, so `expr: T` with `T: ToText`.
- In a `pushdown fn`, `to_text` of a finite runtime value is tabulated like any `const fn` call,
  so the line stays a finite function of σ.
- An `#[entry]` scene's parameter types must not contain `Text`, because `fn` code can build
  runtime text with `to_text`.

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

It covers `Change<T>`, `Swap` and `ChannelChange`. A statement that leaves one pending is a
warning, and the change does not happen.

## `for` loops

- `for` over `Vec<T>` is allowed in `fn` and `const fn`.
- In a `pushdown fn`, `for` loops only over constant ranges and arrays.

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
