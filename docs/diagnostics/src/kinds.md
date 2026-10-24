# Kinds (E3xxx)

Stage 3 checks the *kind* of each function and item: plain `fn` code, `const` code, `pushdown fn` story code and `#[scene]` scenes each have rules about what they may call, read and hold. These diagnostics also cover story variables, second-class references and closures, and the shape of `choice!` and text forms.

## E3001 `call-rule`

**Error.** A function calls another function of a kind it may not call, such as a `fn` calling into the story, a `pushdown fn` calling a `fn`, or a `const fn` calling a `fn`.

```kis
fn start() -> Int<0, 3> { 0 }

pushdown fn visit() {
    start();
}
```

Fix: move the call into a function of a kind that is allowed to make it. The only way from `fn` code into the story is `nav::start` of an entry scene.

## E3002 `step-outside-scene`

**Error.** A function that is not a scene runs a step with `s!` or `choice!`. This includes closures and instant procedures.

```kix
pushdown fn visit() {
    s!(t!["no"]);
}
```

Fix: mark the function `#[scene]`, or move the step into a scene.

## E3003 `scene-call-outside-scene`

**Error.** An instant procedure (a `pushdown fn` without `#[scene]`) calls a scene.

```kis
#[scene]
pushdown fn Other() {}

pushdown fn visit() {
    Other();
}
```

Fix: call the scene from another `#[scene]` function, or mark the caller `#[scene]`.

## E3004 `unbounded-loop`

**Error.** An instant procedure uses `while` or another loop with no bound. Instant procedures may only use `for` over an array, or over a range with constant bounds.

```kis
static mut ROUTE: Int<0, 3> = 0;

pushdown fn visit() {
    while ROUTE < 3 { ROUTE += 1; }
}
```

Fix: use a bounded loop such as `for i in [1, 2] { ROUTE = i; }`, or mark the function `#[scene]`.

## E3005 `instant-recursion`

**Error.** Instant procedures call each other in a cycle, either directly or through a function pointer.

```kis
pushdown fn ping() { pong(); }
pushdown fn pong() { ping(); }
```

Fix: mark one of the functions `#[scene]` and pass through a step.

## E3006 `for-not-constant`

**Error.** A `for` loop in story code iterates over a range whose bounds are not constant expressions.

```kis
static mut ROUTE: Int<0, 3> = 0;

pushdown fn visit() {
    let n = ROUTE;
    for i in 0..n { }
}
```

Fix: iterate over an array, or over a range with constant bounds such as `for i in 0..MAX { ... }`.

## E3007 `story-variable-outside-story`

**Error.** A story variable (`static mut`) is read or written outside `pushdown fn` code.

```kis
static mut ROUTE: Int<0, 3> = 0;

fn system() {
    let route = ROUTE;
}
```

Fix: access the story variable only from a `pushdown fn`.

## E3008 `not-finite`

**Error.** A story variable, an entry scene parameter, or the result of a tabulated `const fn` call has a type with too many values. Such types must be finite, such as bounded integers, `bool` and enums.

```kis
static mut COUNT: u32 = 0;
```

Fix: use a bounded type, such as `static mut COUNT: Int<0, 10> = 0;`.

## E3009 `text-in-entry`

**Error.** A parameter type of an entry scene contains `Text`. `fn` code can build text at runtime, so `Text` cannot cross into the story.

```kis
enum Ending { Normal }

#[scene]
#[entry]
pushdown fn Named(name: Text) -> Ending {
    Ending::Normal
}
```

Fix: pass a finite value, such as an enum, and choose the text inside the story.

## E3010 `not-constant`

**Error.** The initializer of a `const` (or of a `static mut`) is not a constant expression. Assignments and calls to `fn` functions are not constant expressions.

```kis
const COUNTED: u32 = { let mut n = 0; n += 1; n };
```

Fix: write the initializer as a constant expression, such as `const COUNTED: u32 = 1;`.

## E3011 `bad-handle`

**Error.** An immutable `static` holds something other than a `Setting<T>` or `Meta<T>` handle.

```kis
static LIMIT_HANDLE: u32 = 3;
```

Fix: use `const` for a constant (`const LIMIT: u32 = 3;`) or `static mut` for a story variable.

## E3012 `reference-in-item`

**Error.** The type of a `const` or `static` contains a reference.

```kis
const BORROWED: &u32 = &1;
```

Fix: store the value itself, such as `const BORROWED: u32 = 1;`.

## E3013 `meta-in-const`

**Error.** `const` code uses a `Meta` field. Meta is read by `fn` code and written by the story, and is never visible to `const` code.

```kis
static CLEARED: Meta<bool> = meta(false);

const fn render(mut screen: Screen) -> Screen {
    let cleared = CLEARED;
    screen
}
```

Fix: read the meta value in `fn` code, or pass it to the `const fn` as a parameter.

## E3014 `second-class-reference`

**Error.** Story code stores a reference in a local or returns one. In the story, references are second-class and can only be passed as arguments.

```kis
struct Point { x: Int<0, 3>, y: Int<0, 3> }

pushdown fn alias(p: &mut Point) {
    let q = &p.x;
}
```

Fix: pass the reference directly as an argument, or copy the value out with `let q = p.x;`.

## E3015 `second-class-closure`

**Error.** Story code stores a closure in a local. In the story, closures are second-class and can only be passed as arguments and called.

```kis
pushdown fn twice(f: pushdown Fn()) {
    f();
    let g = f;
}
```

Fix: call `f` directly instead of binding it to a local.

## E3016 `aliased-mut-argument`

**Error.** In story code, a place is passed as a `&mut` argument and also as another argument of the same call.

```kis
struct Point { x: Int<0, 3>, y: Int<0, 3> }

pushdown fn shift(a: &mut Point, b: &Point) {
    a.x = b.y;
}

pushdown fn alias(p: &mut Point) {
    shift(p, p);
}
```

Fix: pass distinct places, such as `shift(&mut a, &b);`.

## E3017 `immutable-place`

**Error.** Code assigns to a place whose root binding is not declared mutable.

```kis
struct Point { x: Int<0, 3>, y: Int<0, 3> }

#[scene]
pushdown fn Moves() {
    let b = Point { x: 1, y: 2 };
    b.x = 3;
}
```

Fix: declare the binding mutable, as in `let mut b = Point { x: 1, y: 2 };`.

## E3018 `misplaced-attribute`

**Error.** An attribute is used on an item it does not apply to. For example, `#[scene]` on a `fn`, `#[entry]` without `#[scene]`, or `#[must_use]` on a function.

```kis
#[scene] fn not_a_story() {}
```

Fix: put the attribute only where it applies, such as `#[scene] pushdown fn not_a_story() {}`.

## E3019 `choice-id`

**Error.** Two `choice!` forms in the same function use the same `id`. Saves record the choice id, so it must be unique within its function.

```kix
enum Map { A, B }

#[scene]
pushdown fn Main() {
    match choice!{ id = "map", t!["A"] => Map::A, t!["B"] => Map::B } { _ => {} }
    match choice!{ id = "map", t!["A"] => Map::A, t!["B"] => Map::B } { _ => {} }
}
```

Fix: give each choice its own id, such as `id = "map2"`.

## E3020 `choice-arms`

**Error.** A `choice!` lists the same variant in more than one arm. Each variant may be chosen at most once.

```kix
enum Map { A, B }

#[scene]
pushdown fn Main() {
    match choice!{ id = "map", t!["A"] => Map::A, t!["A again"] => Map::A } {
        Map::A => {}
        _ => {}
    }
}
```

Fix: map each arm to a different variant.

## E3021 `ruby-readings`

**Error.** A mono ruby does not give exactly one reading per character of its base text.

```kix
#[scene]
pushdown fn Main() {
    s!(t![["漢字"]("か"|"ん"|"じ")]);
}
```

Fix: give one reading per character, such as `s!(t![["漢字"]("かん"|"じ")]);`.

## E3022 `effect-not-constant`

**Error.** The effect argument of a text `effect(base, e)` is not a constant expression.

```kis
use std::narrative::text::{effect, mark};

fn tagged(n: u8) -> Text {
    effect("a", mark(n))
}
```

Fix: pass a constant effect, such as `effect("a", mark(1))`.

## W3023 `unused-must-use`

**Warning.** A value of a `#[must_use]` type, such as a `Swap`, is created and then dropped. The change it describes does not happen until it is used.

```kis
const fn render(mut screen: Screen) -> Screen {
    screen.background().clear();
    screen
}
```

Fix: apply the change, as in `screen.background().clear().apply_animation(cut());`.

## E3024 `rng-outside-scene`

**Error.** A function that is not a scene calls `rng_choose`. Only a scene can draw a random value.

```kis
enum Coin { Heads, Tails }

pushdown fn flip() -> Coin {
    rng_choose::<Coin>()
}
```

Fix: mark the function `#[scene]`, or draw the value in a scene and pass it in.
