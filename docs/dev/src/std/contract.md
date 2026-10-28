# The std–Checker Contract

`std` is ordinary Kagari plus a small set of items the compiler knows by name. This chapter is
the contract between the two: the design rules every std item follows, and every rule the
checker (type inference, the kind checker, const evaluation) applies to specific std items. A
std change that breaks a rule here needs a compiler change, and the reverse.

The user-visible subset of these rules is documented for studios in the STD Reference's
appendix *Compiler Rules for std Items* (`docs/std/{en,ja}/src/appendix/rules.md`). Keep both
in sync.

Where in the code:

- `core/kagari-lang/src/signatures/collect.rs` (`std_items`) and `signatures/mod.rs`
  (`StdItems`): the std items the checker knows, by path.
- `core/kagari-lang/src/signatures/traits.rs`: `Eq`, `RandChoose` and explicit impls.
- `core/kagari-lang/src/infer/`: `call.rs` (`nav::start`, `asset`, `system_font`,
  `Integer::into`), `expr.rs` (literals, ranges, `for` element types), `ops.rs` (operators on
  `Decimal`/`Rat`, `Eq` obligations), `finish.rs` (lossy `into`).
- `core/kagari-lang/src/typeck/kinds.rs`, `typeck/kinds/body.rs`, `typeck/kinds/finite.rs`,
  `typeck/kinds/forms.rs`: kinds, finite types, handles, `#[must_use]`, text forms.
- `core/kagari-lang/src/callee.rs`: `push_step` and `rng_choose` as special callee kinds.
- `core/kagari-lang/src/eval/std_items.rs` (`StdTable`) and `eval/machine/builtins.rs`:
  const-evaluation rules of builtins.
- `core/kagari-lang/src/item_tree.rs`: `#[builtin]` placement rules.

## Design rules {#design-rules}

Every std item follows these rules. They are what makes the checker's special cases few.

1. **Kinds.**
   - Everything that builds or changes a `Screen`, every animation, transition and text-effect
     value, and every geometry helper is a `const fn`.
   - The engine's view of a running game, settings and meta reads, navigation, saves and UI are
     `fn`.
   - The only `pushdown fn` items are `push_step`, `rng_choose` and `Meta::write`.
   - Engine-owned state types with timeline semantics are `#[builtin]` structs with `#[builtin]`
     methods. Plain data (geometry, colours, tweens, styles) are ordinary std structs with `pub`
     fields plus `const fn` builders.
2. **Units.** Positions and lengths are `i32`/`u32` pixels in the design resolution
   `System::resolution`. Times are `time::Duration`, whole milliseconds. Fractions (anchor,
   scale, opacity, volume) are `Decimal`; opacity and volume range over `0.0..=1.0`. Volume
   settings are `Volume { level: Int<0, 100>, muted }`. No unit strings, no second scale for any
   quantity.
3. **No hidden defaults.** Every animation constructor takes its duration explicitly. The only
   implicit values are the neutral ones: easing `Linear`, delay 0, rule softness 0, repeat 1,
   identity transforms.
4. **Changes need animations.** Every visible property is a `Prop<T>` (interpolated, takes a
   `Tween`) or a layer (`ImageLayer`, `LabelLayer`, `MovieLayer`, swapped with a
   `Transition`). Only non-interpolable layout settings (`set_plane`, `set_z`, `set_anchor`,
   `set_flip`, `set_blend`, `set_fill_size`, `gate`) are direct setters. `Change<T>`, `Swap` and
   `ChannelChange` are `#[must_use]`: a change without `apply_animation` does not happen.
5. **Animations never change the start or end state.** The logical screen of a step is the
   persistent state after its render: entities with their components, plus the Hud, camera,
   screen filter and audio channels. Snapshots, equality and the consistency check use it.
   - A `Tween`/`Transition` always runs from the old value to the value set by `change`.
   - A `Motion` is a relative path that must end at identity (offset 0, scale 1, rotation 0);
     otherwise const evaluation fails (`E4008`).
   - The timeline, scheduled animations, one-shot sounds and the `ChoiceMenu` belong to the
     step only and are not part of the logical screen.
   - Therefore a click (which completes the step's timeline), skip, or the `animations` setting
     being off can never change the logical screen.
6. **Fixed component set.** Studios add no components. They extend the library with `const fn`
   helpers and constant data; `std::stage` is the reference example of such a layer.
7. **Type-checkable without overloading or coercion.** Asset-typed functions use a trait bound
   (`A: Asset`); interpolation uses a trait bound. The few special signatures are listed below.
   Only std declares traits, all `#[builtin]`: `Eq`, `Integer`, `RandChoose`, `Asset`, `Step`
   and `ToText`. Users never implement them; `Eq` is structural.
8. **References** are shared pointers. In `const fn` and `fn` code they may be bound to locals,
   so a render names an entity once and changes it in several statements. In a `pushdown fn`
   they are parameters only.

```kis
const HANAKO: EntityId = EntityId(1);
const HANAKO_SMILE: ImageAsset = asset("characters/hanako/smile.png");

const fn hanako_appears(mut screen: Screen) -> Screen {
    let hanako = screen.entity(HANAKO);
    hanako.image().change(HANAKO_SMILE).apply_animation(cut());
    hanako.opacity().change(1.0).apply_animation(tween(ms(300)));
    screen
}
```

## Kinds and call rules {#kinds}

- Every std function's kind is the kind it is declared with. The call rules apply to methods
  and builtins exactly as to user functions (`E3001`): a render (`const fn`) cannot call
  `Setting::get`, a component (`fn`) cannot call `push_step`.
- `push_step` and `rng_choose` are `pushdown fn`, but `callee.rs` classifies them as their own
  callee kinds: only a scene may call them (`E3002` for steps, including `s!`/`choice!`;
  `E3024` for `rng_choose`). Other pushdown builtins (`Meta::write`) are instant.
- A `Setting<T>` handle may appear in `const` code only as a value, e.g. passed to
  `Entity::gate`; `Setting::get` and `set` are `fn`, so calling them from `const` code is
  `E3001` with a hint about handles.
- A meta static (`Meta<T>`) used in `const` code is `E3013`: meta is read by `fn` code and
  written by the story.
- An immutable `static` must hold a `Setting<T>` or `Meta<T>` handle (`E3011`); `setting(…)` and
  `meta(…)` outside a static initializer are `E4007`.

## Finite types {#finite-types}

The story may hold at run time only values of finite types (`typeck/kinds/finite.rs`). Of the
`#[builtin]` types, only `Text` and the six asset types (`ImageAsset`, `AudioAsset`,
`VoiceAsset`, `VideoAsset`, `FontAsset`, `ShaderAsset`) are finite. Every other builtin type is
not: `Screen`, `Narrative`, `Vec`, `String`, `Rat`, `Decimal`, `Setting`, `Meta` and the screen
state types. `Render` is `const fn(Screen) -> Screen`, a `const fn` pointer, so it is finite.

Ordinary std structs are finite exactly when their fields are. `EntityId`, `Part` and `Mark`
wrap Rust integers, and `Duration`, `Color`, `Point` and the other geometry types hold Rust
integers or `Decimal`s, so none of them is finite at run time. Story code can still name them
in constant expressions, which are folded.

A `const fn` called from the story with a non-constant argument is **tabulated**: evaluated at
compile time on every combination of its argument values (at most 65 536, else `E4004`), and
its result must be finite (`E3008`). `to_text` of a finite runtime value in a `pushdown fn` is
tabulated like any `const fn` call, so a line stays a finite function of the story state.

## Type rules {#type-rules}

| Rule | Where | Diagnostic |
| ---- | ----- | ---------- |
| A string literal has type `Text`; a literal with a decimal point has type `Decimal` | `infer/expr.rs`, `infer/pat.rs` | `E2001` on mismatch |
| `{expr}` in `t![…]` is `to_text(expr)`, so `expr: T` needs `T: ToText` (`Text`, `String`, `Int<lo, hi>`) | `hir/forms.rs`, trait solver | `E2026` |
| An `#[entry]` scene's parameter and return types must not contain `Text` (`fn` code can build runtime text) | `typeck/kinds.rs` | `E3009` |
| `a..b` and `a..=b` have type `Range<T>` with `T` a type both bounds convert to (`0..7` is `Range<Int<0, 7>>`); `a..`, `..b` and `..` exist only in patterns | `infer/expr.rs` | `E2030` |
| `x.into()` from `S` to `T`, both `Integer`, needs every value of `S` to fit in `T`; `x.saturate()` has no such condition | `infer/call.rs`, `infer/finish.rs` | `E2018` |
| Arithmetic and ordering accept integers, `Decimal` and `Rat` (no mixing); negation needs a signed integer, `Decimal` or `Rat` | `infer/ops.rs` | `E2014`, `E2015` |
| The readings of `mono_ruby` are an array literal with one reading per character of a string-literal base; the base of a ruby is a string literal | `typeck/kinds/forms.rs` | `E3021` |
| The effect of a text effect is a constant expression | `typeck/kinds/forms.rs` | `E3022` |

## Traits {#traits}

- Only std declares traits, and each one is `#[builtin]` (`E1012`, `E1013`). An
  `impl Trait for T` outside std is `E1023`.
- `Eq` (`signatures/traits.rs::structural_eq`) holds for `bool`, every integer type, function
  pointers and named functions, references to `Eq` types, and tuples, arrays, structs and enums
  whose fields are all `Eq`. A `#[builtin]` type is `Eq` only through an explicit `impl Eq` in
  std (`String`, `Vec<T: Eq>`, `Rat`, `Decimal`, the asset types, `Text`, `Screen`). Closures
  are never `Eq`. `==` on a non-`Eq` type is `E2017`.
- `RandChoose` holds for `Int<lo, hi>` (explicit impl) and for fieldless, non-builtin enums
  with at least one variant. This is what makes RAND's verifier rule hold.
- `Integer`, `Asset` and `ToText` hold only through their explicit impls in std. `Step` is
  implemented by `Say` and `Choice<A, N>`; its `Output` is the step's result type.

## Special signatures {#special-signatures}

### `asset::<A>(path)` and `system_font(path)`

`asset` needs `A: Asset`. The argument of both must be a string literal (`E2028`); the path is
resolved at compile time (`E1027`–`E1032`) and the call becomes an Asset constant.

### `nav::start(entry, args, on_exit)`

Declared generically, checked specially (`infer/call.rs::infer_nav_start`):

- `entry` is a `scene fn(P1, …, Pn) -> R` naming an `#[entry]` scene (`E2027` otherwise);
- `args` is the tuple `(P1, …, Pn)`;
- `on_exit` is `Fn(R)`.

### `rng_choose::<T>()`

`T: RandChoose`; scenes only (`E3024`). Lowered to RAND.

### `std::ui::slider::<LO, HI>(props)`

The only builtin whose const generic arguments reach the engine: the emitter appends `LO` and
`HI` as `Int` arguments to the import.

## `#[must_use]` {#must-use}

`Change<T>`, `Swap` and `ChannelChange` are `#[must_use]`. A pending change that is discarded,
or bound to a local and never completed with `apply_animation`, is warning `W3023`, and the
change does not happen.

## `for` loops {#for-loops}

- In `fn` and `const fn`, `for` iterates over `Vec<T>`, arrays and `Range<T>` of an `Integer`
  `T` (`E2031` otherwise).
- In a `pushdown fn`, `for` iterates only over arrays and over ranges whose bounds are constant
  expressions (`E3006`).

## Const-evaluation errors of std builtins {#const-evaluation-errors}

The evaluator (`eval/machine/builtins.rs`, `eval/machine/anim.rs`) reports these as compile
errors:

| Rule | Code |
| ---- | ---- |
| `Vec::set` out of range | `E4005` |
| Motion rules: no keys, `at` not strictly increasing or equal to 0, a last key that is not identity, `repeat` = 0 | `E4008` |
| `CubicBezier` x outside 0–1, `Steps(0)` | `E4009` |
| duplicate text marks | `E4010` |
| `begin_text` while anchored on a mark | `E4011` |
| a `Decimal` fraction outside `0.0..=1.0` where required: opacity, volume, anchor | `E4012` |
| `Duration` overflow or underflow (integer overflow inside `time.kis`) | `E4013` |

General evaluation errors (`E4001` overflow, `E4002` division by zero, `E4003` budget) apply to
std code like to any other.

## Invariants

- Every std item the checker special-cases is reached through `StdItems` or `StdTable` by path.
  Renaming or moving such an item without updating `signatures/collect.rs` and
  `eval/std_items.rs` silently disables the rule (`E1026` reports a missing std item only where
  forms need it).
- The design rules above hold for every new item; an exception is a design decision, recorded in
  [Design Decisions](../foundations/decisions.md).
- This chapter, the STD Reference appendix (en and ja) and the Reference's diagnostics pages
  describe the same rules.

## When you change this

Load the `kagari-std` skill. A new special case needs: the `StdItems` field, the rule with a
diagnostic code, tests in the checker's test module, a row here, and the STD Reference appendix
in both languages.
