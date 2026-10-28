# Constant Evaluation (E4xxx)

The compiler runs `const` code at compile time. It evaluates constants, tabulates `const fn` calls from the story, and builds screens and text marks. These diagnostics report faults found during that evaluation. Faults in a render function are reported at the step that uses it.

## E4001 `overflow` {#e4001}

**Error.** An arithmetic result does not fit in its integer type.

```kis
const OVERFLOW: u8 = 200u8 + 100u8;
```

Fix: use a wider type or smaller operands, such as `const OVERFLOW: u16 = 200u16 + 100u16;`.

## E4002 `division-by-zero` {#e4002}

**Error.** Constant code divides by zero, either with `/` or `%` on integers or by building a rational with a zero denominator, such as `Rat::new(1, 0)`. `Decimal / Decimal` is exact and then truncated to 7 digits, so it never reports an inexact-division error.

```kis
const fn divide(a: i32, b: i32) -> i32 {
    a / b
}
const DIVISION: i32 = divide(1, 0);
```

Fix: make sure the divisor is not zero, such as `divide(1, 2)`.

## E4003 `budget-exceeded` {#e4003}

**Error.** Evaluating a constant took more than the step budget (1000000 steps), usually because of a long or endless loop or recursion.

```kis
const fn spin(n: u64) -> u64 {
    let mut i = 0u64;
    while i < n {
        i = i + 1;
    }
    i
}
const BUDGET: u64 = spin(100000000);
```

Fix: reduce the work, or compute the value outside `const` code.

## E4004 `domain-too-large` {#e4004}

**Error.** Story code calls a `const fn` with runtime arguments, and the compiler must tabulate the call over every combination of argument values. This error is reported when there are more than 65536 combinations.

```kis
static mut BIG: Int<0, 100000> = 0;

const fn same(x: Int<0, 100000>) -> Int<0, 100000> {
    x
}

#[scene]
pushdown fn Start() {
    let big = same(BIG);
}
```

Fix: narrow the parameter types, such as `Int<0, 3>`, so the domain is small.

## E4005 `index-out-of-range` {#e4005}

**Error.** An index into an array or `Vec` is past its length.

```kis
const INDEX: i32 = {
    let a = [1, 2];
    a[2]
};
```

Fix: use an index below the length, such as `a[1]`.

## E4006 `const-cycle` {#e4006}

**Error.** The value of a `const` depends on itself, directly or through other constants, so
it cannot be computed at compile time. Every constant in the cycle is reported, at its own
name; a constant that only uses one of them is not.

```kis
const A: u32 = B + 1;
const B: u32 = A;
```

Fix: break the cycle by giving one of the constants a value that does not refer back, such as `const B: u32 = 0;`.

## E4007 `handle-outside-static` {#e4007}

**Error.** A `Setting` or `Meta` handle is created somewhere other than the initializer of a `static`.

```kis
const HANDLE: Setting<bool> = setting(true);
```

Fix: declare it as a static, as in `static HANDLE: Setting<bool> = setting(true);`.

## E4008 `invalid-motion` {#e4008}

**Error.** A motion or shake is malformed. For example, it has no keys, a key at time zero, keys that do not strictly increase in time, a last key that is not the identity, or a repeat count of zero.

```kix
const A: EntityId = EntityId(3);

const fn no_keys(mut screen: Screen) -> Screen {
    screen.entity(A).apply_animation(motion());
    screen
}

#[scene]
pushdown fn Start() {
    s!(render = no_keys, t!["1"]);
}
```

Fix: give the motion at least one key that ends at the identity, such as `motion().key(ms(100), offset(0, 0))`.

## E4009 `invalid-easing` {#e4009}

**Error.** An easing has parameters outside their valid range. For example, a `CubicBezier` x coordinate outside `0.0..=1.0`, or `Steps(0)`.

```kix
const A: EntityId = EntityId(3);

const fn bezier(mut screen: Screen) -> Screen {
    screen.entity(A).opacity().change(1.0).apply_animation(tween(ms(100)).ease(Easing::CubicBezier(1.5, 0.0, 0.5, 1.0)));
    screen
}

#[scene]
pushdown fn Start() {
    s!(render = bezier, t!["1"]);
}
```

Fix: keep the parameters in range, such as `Easing::CubicBezier(0.5, 0.0, 0.5, 1.0)`.

## E4010 `duplicate-mark` {#e4010}

**Error.** The same text mark appears more than once in one text.

```kis
use std::narrative::text::{concat, effect, mark};

const DUPLICATE_MARKS: Text = concat([effect("a", mark(1)), effect("b", mark(1))]);
```

Fix: give each mark its own number, such as `mark(2)` for the second.

## E4011 `begin-text-on-mark` {#e4011}

**Error.** A render calls `begin_text` while the timeline is anchored on a text mark.

```kix
use std::narrative::Mark;

const fn text_on_mark(mut screen: Screen) -> Screen {
    screen.at_mark(Mark(2));
    screen.begin_text();
    screen
}

#[scene]
pushdown fn Start() {
    s!(render = text_on_mark, t!["1"]);
}
```

Fix: call `begin_text` before anchoring the timeline on a mark.

## E4012 `fraction-out-of-range` {#e4012}

**Error.** A value that must be a fraction in `0.0..=1.0` is outside that range, such as an opacity, a volume, or an anchor coordinate.

```kix
const A: EntityId = EntityId(3);

const fn opacity(mut screen: Screen) -> Screen {
    screen.entity(A).opacity().change(1.5).apply_animation(instant());
    screen
}

#[scene]
pushdown fn Start() {
    s!(render = opacity, t!["1"]);
}
```

Fix: keep the value in `0.0..=1.0`, such as `.change(1.0)`.

## E4013 `duration-overflow` {#e4013}

**Error.** A `Duration` does not fit in `u32` milliseconds, either when it is built or after arithmetic.

```kis
const SECS_OVERFLOW: Duration = secs(5000000);
```

Fix: use a shorter duration, such as `secs(5000)`.
