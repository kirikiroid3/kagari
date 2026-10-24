# Types (E2xxx)

Type diagnostics come from lowering signatures (types, constants, aliases, bounds) and from type inference of function and scene bodies: mismatched types, bounded-integer arithmetic, conversions, fields and methods, calls, patterns and match exhaustiveness. Every code here is an error except `W2039`.

## E2001 `mismatched-types`

**Error.** A value's type differs from the type its context expects, such as a `let` annotation, a return type or a function argument.

```kis
fn mismatch() -> bool {
    let n: u32 = true;
    5
}
```

Fix: make the value match the expected type, e.g. `let n: u32 = 1;`, or change the annotation.

## E2002 `unsupported-const`

**Error.** A constant expression in a type (an array length, an `Int` bound or a const generic argument) uses something other than literals, arithmetic, constants and const parameters, or evaluating it forms a cycle.

```kis
const A: i64 = B;
const B: i64 = A;
fn f(n: Int<0, A>) {}
```

Fix: build the constant only from literals, arithmetic and other acyclic constants, e.g. `const A: i64 = 4;`.

## E2003 `const-overflow`

**Error.** Evaluating a constant expression in a type overflowed or divided by zero, or an integer literal in it is too large.

```kis
fn f(a: [u32; 10 / 0]) {}
```

Fix: change the expression so its arithmetic stays in range, e.g. `fn f(a: [u32; 10 / 2]) {}`.

## E2004 `bad-int-bounds`

**Error.** An `Int<lo, hi>` type is empty because `lo > hi`, or a bound does not fit in `i64`.

```kis
fn f(n: Int<5, 0>) {}
```

Fix: write the bounds in ascending order: `fn f(n: Int<0, 5>) {}`.

## E2005 `wrong-generic-count`

**Error.** A type or function receives a different number of generic arguments than it declares; `Int` needs exactly two bounds and primitives take none.

```kis
fn f(n: Int<5>) {}
```

Fix: supply the declared number of arguments: `fn f(n: Int<0, 5>) {}`.

## E2006 `expected-type`

**Error.** A path in type position resolves to something that is not a type, such as a function, enum variant, module or constant.

```kis
enum Route { A, B }
fn f(r: Route::A) {}
```

Fix: name the type instead: `fn f(r: Route) {}`.

## E2007 `expected-const`

**Error.** A const generic parameter was given a type instead of a constant.

```kis
fn f(n: Int<u32, 5>) {}
```

Fix: pass a constant value: `fn f(n: Int<0, 5>) {}`.

## E2008 `expected-trait`

**Error.** A bound or `impl … for` names something that is not a trait.

```kis
struct Point { x: u32, y: u32 }
fn same<T: Point>(a: T) {}
```

Fix: use a trait in the bound, e.g. `fn same<T: Eq>(a: T) {}`.

## E2009 `expected-value`

**Error.** A path in expression position resolves to something that is not a value, such as a module, a type parameter, or a struct or variant with named fields.

```kis
enum Shape { Circle { r: u32 }, Dot }
fn f() {
    let s = Shape::Circle;
}
```

Fix: construct a value: `let s = Shape::Circle { r: 1 };`.

## E2010 `alias-cycle`

**Error.** A type alias expands to itself, directly or through other aliases.

```kis
type A = B;
type B = A;
```

Fix: make one alias name a concrete type, e.g. `type B = u32;`.

## E2011 `infer-in-signature`

**Error.** The placeholder `_` appears in an item signature, where types must be written out.

```kis
fn f(n: _) -> u32 { 1 }
```

Fix: write the type: `fn f(n: u32) -> u32 { 1 }`.

## E2012 `bad-literal`

**Error.** An integer literal has a suffix that is not an integer type.

```kis
fn f() {
    let n = 5u7;
}
```

This snippet is illustrative: the parser already rejects an unknown suffix with `E0001`, so type inference reports `E2012` only for a literal that reached it unchecked.

Fix: use a valid suffix or none: `let n = 5u8;`.

## E2013 `literal-out-of-range`

**Error.** An integer literal, in an expression or a pattern, does not fit the integer type it is checked against.

```kis
fn literals() {
    let small: u8 = 300;
    let counter: Int<0, 5> = 6;
}
```

Fix: use a value within the type's range or widen the type, e.g. `let small: u16 = 300;`.

## E2014 `int-mix`

**Error.** An arithmetic operator combines two different integer types; there is no implicit conversion.

```kis
fn int_ranges(a: Int<0, 5>) {
    let mixed = a + 1u32;
}
```

Fix: convert one side explicitly with `.into()` or `.saturate()`, e.g. `let mixed = a.into() + 1u32;`.

## E2015 `bad-operands`

**Error.** An operator is applied to operand types it does not support, such as `!` on a non-`bool`/integer, negating an unsigned value, or bounded arithmetic whose result range overflows.

```kis
struct Point { x: u32, y: u32 }
fn f(p: Point) -> bool {
    !p
}
```

Fix: apply the operator to a supported type, e.g. `!flag` on a `bool`.

## E2016 `divisor-may-be-zero`

**Error.** The divisor of `/` or `%` has a bounded `Int` type whose range contains 0.

```kis
fn int_ranges(a: Int<0, 5>) {
    let quotient = a / a;
}
```

Fix: narrow the divisor's type so it excludes zero first, e.g. take `b: Int<1, 5>` and write `a / b`.

## E2017 `not-eq`

**Error.** `==` or `!=` is used on a type that is never `Eq`, such as a closure.

```kis
fn equality() -> bool {
    let f = || 1u32;
    f == f
}
```

Fix: compare values that implement `Eq`, e.g. `f() == f()`.

## E2018 `lossy-into`

**Error.** `.into()` converts between integer types where the source does not always fit in the target.

```kis
fn conversions(wide: u32) {
    let byte: u8 = wide.into();
}
```

Fix: clamp explicitly: `let byte: u8 = wide.saturate();`.

## E2019 `as-cast`

**Error.** Kagari has no `as` casts.

```kis
fn conversions(wide: u32) {
    let cast = wide as u8;
}
```

Fix: use `.into()` for lossless or `.saturate()` for clamping conversions: `let cast: u8 = wide.saturate();`.

## E2020 `unknown-field`

**Error.** A struct initializer or field access names a field the type does not have.

```kis
struct Point { x: u32, y: u32 }
fn fields(p: Point) -> u32 {
    let unknown = Point { x: 1, y: 2, z: 3 };
    p.z
}
```

Fix: use only declared fields, e.g. `p.x`.

## E2021 `missing-field`

**Error.** A struct initializer omits a field of the struct.

```kis
struct Point { x: u32, y: u32 }
fn fields() {
    let missing = Point { x: 1 };
}
```

Fix: initialize every field: `let p = Point { x: 1, y: 2 };`.

## E2022 `unknown-method`

**Error.** A method call names a method that neither the type nor any trait in scope provides.

```kis
struct Point { x: u32, y: u32 }
fn methods(p: Point) {
    p.frobnicate();
}
```

Fix: call an existing method, or define `frobnicate` in an `impl Point` block.

## E2023 `not-a-method`

**Error.** Method-call syntax is used on an associated function that has no `self` parameter.

```kis
struct Point { x: u32, y: u32 }
impl Point {
    fn origin() -> Point { Point { x: 0, y: 0 } }
}
fn f(p: Point) {
    p.origin();
}
```

Fix: call it through the type: `Point::origin()`.

## E2024 `wrong-arity`

**Error.** A call supplies a different number of arguments than the function takes.

```kis
fn arity(x: u32) -> u32 {
    arity(x, x)
}
```

Fix: pass exactly the declared arguments: `arity(x)`.

## E2025 `not-callable`

**Error.** A call expression's callee is not a function, function pointer or closure.

```kis
fn f(n: u32) -> u32 {
    n(1)
}
```

Fix: call a function value, or remove the parentheses if `n` was meant as a value.

## E2026 `unsatisfied-bound`

**Error.** A generic argument does not implement a trait required by the callee's bounds.

```kis
fn same<T: Eq>(a: T, b: T) -> bool {
    a == b
}

fn bounds() -> bool {
    same(|| 1u32, || 2u32)
}
```

Fix: pass a type that implements the trait, e.g. `same(1u32, 2u32)`.

## E2027 `bad-nav-start`

**Error.** The first argument of `nav::start` does not name an `#[entry]` scene.

```kis
#[scene]
pushdown fn Side() {}

fn navigation() {
    nav::start(Side, (), |_end| ());
}
```

Fix: start an `#[entry]` scene, e.g. mark `Side` with `#[entry]`.

## E2028 `asset-path-not-literal`

**Error.** The path passed to `asset(…)` is not a string literal, so it cannot be checked at compile time.

```kis
const PATH: Text = "missing.png";
const IMAGE: ImageAsset = asset(PATH);
```

Fix: write the path inline: `const IMAGE: ImageAsset = asset("missing.png");`.

## E2029 `annotations-needed`

**Error.** Inference could not determine a type, e.g. an empty collection never constrained afterwards, or a call/field/iteration on a value of unknown type.

```kis
fn annotations() {
    let list = Vec::new();
}
```

Fix: annotate the type: `let list: Vec<u32> = Vec::new();`.

## E2030 `open-range`

**Error.** A range expression lacks a bound; `a..`, `..b` and `..` exist only in patterns.

```kis
fn f() {
    for i in 0.. {}
}
```

Fix: give both bounds: `for i in 0..10 {}`.

## E2031 `not-iterable`

**Error.** A `for` loop iterates over a value whose type cannot be iterated.

```kis
fn f(flag: bool) {
    for x in flag {}
}
```

Fix: iterate a range or collection, e.g. `for x in 0..5 {}`.

## E2032 `not-indexable`

**Error.** Indexing `[…]` is applied to a value whose type does not support it.

```kis
fn f(n: u32) -> u32 {
    n[0]
}
```

Fix: index an array, `Vec` or other indexable collection instead.

## E2033 `bad-deref`

**Error.** The dereference operator `*` is applied to a value that is not a reference.

```kis
fn f(n: u32) -> u32 {
    *n
}
```

Fix: drop the `*`, or dereference an actual reference such as `r: &u32`.

## E2034 `bad-try`

**Error.** The `?` operator is used on a value that is not `Option`/`Result`, or in a function whose return type does not match the operand's kind.

```kis
fn f(o: Option<u32>) -> u32 {
    o?
}
```

Fix: return the matching type, e.g. `fn f(o: Option<u32>) -> Option<u32> { Some(o?) }`.

## E2035 `bad-pattern`

**Error.** A pattern's shape does not fit what it names: a tuple pattern on a non-tuple variant, the wrong number of fields, an enum where a variant is expected, or a static in a pattern.

```kis
fn f(r: Option<u32>) -> u32 {
    match r {
        Some(a, b) => a,
        None => 0,
    }
}
```

Fix: match the declared shape: `Some(a) => a,`.

## E2036 `capturing-closure`

**Error.** A closure that captures local variables is used where a function pointer is required.

```kis
fn f(n: u32) {
    let g: fn() -> u32 = || n;
}
```

Fix: pass the value as a parameter instead of capturing it, e.g. `let g: fn(u32) -> u32 = |n| n;`.

## E2037 `non-exhaustive-match`

**Error.** A `match` does not cover every possible value of the scrutinee; the message lists uncovered patterns.

```kis
fn bounded(n: Int<0, 10>) -> u32 {
    match n {
        0..=5 => 0,
    }
}
```

Fix: add arms for the missing values or a wildcard: `6..=10 => 1,`.

## E2038 `refutable-pattern`

**Error.** A pattern that can fail to match is used where matching must always succeed: `let`, function and closure parameters, or a `for` binding.

```kis
enum Route { A, B, C }
fn refutable(r: Option<Route>) {
    let Some(x) = r;
}
```

Fix: use `match` or `if let Some(x) = r { … }` to handle the other cases.

## W2039 `unreachable-pattern`

**Warning.** A `match` arm can never be reached because earlier arms already cover every value it matches.

```kis
enum Route { A, B, C }
fn unreachable(r: Route) -> u32 {
    match r {
        Route::A | Route::B => 0,
        Route::B => 1,
        _ => 2,
    }
}
```

Fix: remove the unreachable arm or move it before the arms that cover it.
