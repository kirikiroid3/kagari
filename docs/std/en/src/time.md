# `std::time`

> **Stability:** Stable — durations in whole milliseconds.

`std::time` declares `Duration`, the one type of time in `std`. Every time in the library (tween
lengths, delays, text speed, pauses, auto-mode delays) is a `Duration` of whole milliseconds. There
are no unit strings and no frame counts.

Every item is ordinary Kagari code (no `#[builtin]`), and every function is a `const fn`. The
prelude imports `Duration`, `ms` and `secs`.

## Overview

|Item|Kind|Summary|
|---|---|---|
|[`Duration`](#struct-duration)|struct|A length of time in whole milliseconds|
|[`ZERO`](#constant-zero)|constant|No time at all|
|[`ms(n)`](#function-msn)|`const fn`|`n` milliseconds|
|[`secs(n)`](#function-secsn)|`const fn`|`n` seconds|
|[`Duration::plus`](#durationplusself-other)|`const fn`|Sum|
|[`Duration::times`](#durationtimesself-n)|`const fn`|Multiplication|
|[`Duration::div`](#durationdivself-n)|`const fn`|Division, rounded down|

## Struct: `Duration`

```kis
pub struct Duration {
    pub ms: u32,
}
```

- `ms` `u32` — the length in milliseconds.
- Kind: — · Builtin: no

A `Duration` is plain data: it can be built with a struct literal, compared with `==` (it is
`Eq`), and its field read directly. It holds a Rust integer, so it is not finite: the story uses
durations only in constant expressions, which are folded.

Overflow or underflow of a `Duration` is an evaluation error; in const evaluation it is
[`E4013`](../../../reference/en/src/diagnostics/eval.md#e4013).

### `Duration::plus(self, other)`

```kis
pub const fn plus(self, other: Duration) -> Duration
```

- `other` `Duration` — the duration to add.
- Returns: `Duration` — the sum of the two durations.
- Kind: `const fn` · Builtin: no

### `Duration::times(self, n)`

```kis
pub const fn times(self, n: u32) -> Duration
```

- `n` `u32` — the factor.
- Returns: `Duration` — this duration `n` times.
- Kind: `const fn` · Builtin: no

### `Duration::div(self, n)`

```kis
pub const fn div(self, n: u32) -> Duration
```

- `n` `u32` — the divisor.
- Returns: `Duration` — this duration divided by `n`, rounded down.
- Kind: `const fn` · Builtin: no

`div` truncates: `ms(1000).div(3)` is `ms(333)`. `n == 0` is a division by zero
([`E4002`](../../../reference/en/src/diagnostics/eval.md#e4002) in const evaluation).

```kis
const BEAT: Duration = ms(400);
const BAR: Duration = BEAT.times(4);
const HALF_BAR: Duration = BAR.div(2);
const INTRO: Duration = secs(2).plus(HALF_BAR);
```

## Constant: `ZERO`

```kis
pub const ZERO: Duration = Duration { ms: 0 };
```

No time at all. Not in the prelude: `use std::time::ZERO;`. As the `text_speed` setting, `ZERO`
shows text instantly.

## Function: `ms(n)`

```kis
pub const fn ms(n: u32) -> Duration
```

- `n` `u32` — milliseconds.
- Returns: `Duration` — `n` milliseconds.
- Kind: `const fn` · Builtin: no

## Function: `secs(n)`

```kis
pub const fn secs(n: u32) -> Duration
```

- `n` `u32` — seconds.
- Returns: `Duration` — `n` seconds, that is `n * 1000` milliseconds.
- Kind: `const fn` · Builtin: no

`secs(n)` with `n * 1000` beyond `u32` overflows.

