# Type Inference and Checks

Stage 3 gives every item signature a semantic type, infers every body, and then runs the checks
that need types: pattern exhaustiveness and refutability, and the kind and story rules. All of it
reads HIR, name resolution and the inference result; none of it builds MIR except the
instant-recursion check, which reads MIR edges (see
[Pipeline and Queries](./pipeline.md#checks-before-lowering)).

## Where in the code

| Path | Contents |
| ---- | -------- |
| `core/kagari-lang/src/ty.rs` | `Ty` (interned `TyKind` + `TyFlags`), `IntTy`, `Const`, `GenericArgs`, `FnRef`, `CtorRef`, `FnDefId` |
| `core/kagari-lang/src/signatures/mod.rs` | `program_signatures`, `ProgramSignatures`, `ItemSig`, `FnSig`, `AdtDef`, `Generics`, `StdItems` |
| `core/kagari-lang/src/signatures/collect.rs` | Computes every item signature; finds std items by role |
| `core/kagari-lang/src/signatures/lower.rs` | `TyLowerer`: HIR `TypeRef` → `Ty` (aliases, `Int<lo, hi>`, generic arguments) |
| `core/kagari-lang/src/signatures/consts.rs` | The type-level constant folder (array lengths, `Int` bounds, const arguments) |
| `core/kagari-lang/src/signatures/traits.rs` | `TraitSolver`: explicit std impls, bounds, structural `Eq`, `RandChoose`; projections |
| `core/kagari-lang/src/signatures/subst.rs`, `display.rs` | Substitution and matching; the diagnostic text of types |
| `core/kagari-lang/src/infer/` | `infer` query: `ctx.rs` (context, coercion, joins, instantiation), `table.rs` (unification), `expr.rs`, `ops.rs`, `call.rs`, `path.rs`, `pat.rs`, `finish.rs`, `walk.rs` |
| `core/kagari-lang/src/callee.rs` | `CalleeKind`: what kind of function a call calls |
| `core/kagari-lang/src/typeck/exhaustive.rs` | `pattern_checks`: usefulness, witnesses (`E2037`, `E2038`, `W2039`) |
| `core/kagari-lang/src/typeck/kinds.rs` | `kind_checks`: attributes and item types |
| `core/kagari-lang/src/typeck/kinds/body.rs` | The body walk: call rules, scenes, loops, story variables, finite values, second-class values |
| `core/kagari-lang/src/typeck/kinds/{finite,places,forms,instant}.rs` | Finite types; mutability and `&mut` exclusivity; form rules and `#[must_use]`; instant recursion |
| `core/kagari-lang/src/eval/const_exprs.rs` | `const_exprs`: which expressions are constant (read by the kind checks) |

## Signatures

`program_signatures(program)` computes, for every item of every loaded file (game and std), an
`ItemSig`:

| `ItemSig` | Holds |
| --------- | ----- |
| `Fn(FnSig)` | `generics`, `kind` (`Plain`/`Const`/`Pushdown`), `scene`, `entry`, `builtin`, `self_param`, `self_mutable`, `params` (the `self` type first: `&mut Screen` for `&mut self` in `impl Screen`), `ret` (`()` when omitted) |
| `Adt(AdtDef)` | `generics`, `is_enum`, `builtin`, `must_use`, `variants` (a struct has one, named like it) with field types |
| `Const { generics, ty }`, `Static { ty, mutable }` | Declared types |
| `TypeAlias { generics, ty }` | The aliased type (`None` for a trait's `type Output;`) |
| `Impl { generics, self_ty, trait_item }`, `Trait { generics }` | Impl headers and traits |

**Generics.** An item's generic arguments are laid out as its parent's (impl or trait) followed
by its own; `Generics { parent, parent_count, params }`. A `TyKind::Param { owner, index }` names
its owner and its index in that owner's own `params`. Type parameters carry trait bounds (std
traits only, since users declare no traits; `E2008` if a bound is not a trait).

**Lowering types** (`TyLowerer`): primitives, `Int<lo, hi>` (two constant arguments, each in
`i64` range, `lo ≤ hi`: `E2004`, wrong count `E2005`), ADTs with argument counts, tuples, arrays
(length folded), references, `fn`/`const fn`/`pushdown fn`/`scene fn` pointers, `Fn`/`const
Fn`/`pushdown Fn` closure types, aliases (expanded; cycles `E2010`), `Self`, associated types
(`TyKind::Projection` while the base is a parameter). `_` is an error in signatures (`E2011`) and
an inference variable in bodies. Type-level constants (array lengths, `Int` bounds, const
arguments) go through `fold_const_expr`: integer literals, unary `-`, `+ - * / %`, tail-only
blocks, const parameters and `const` items; anything else is `E2002`, overflow `E2003`.

**Std items by role.** `StdItems` finds the std items the checker refers to (`Text`, `Decimal`,
`Rat`, `Vec`, `Range`, `Option`, `Screen`, `Setting`, `Meta`, `Say`, `Choice`, the asset types,
the traits `Eq`, `Integer`, `Asset`, `Step`, `RandChoose`, `ToText`, and the functions
`push_step`, `rng_choose`, `asset`, `system_font`, `concat`, `ruby`, `mono_ruby`, `effect`,
`nav::start`, `signal`, `Integer::into`, `Integer::saturate`). This is the compiler's half of the
std–checker contract; see [The std–Checker Contract](../std/contract.md).

## The inference engine

`infer(program, item)` runs for each `fn` with a body and each `const`/`static` initializer in the
program. One `InferCtx` walks the body top-down with an expectation:

| `Expect` | Meaning |
| -------- | ------- |
| `None` | No information |
| `Hint(ty)` | Guides inference (literals, closures) but is not checked |
| `Coerce(ty)` | A coercion site: the expression's type must coerce to `ty` |

Coercion sites are arguments, `let` with a type, struct fields, return values, assignments, array
elements and `if`/`match` arms.

### Unification

`table.rs` is a union-find over `InferVar`s with an undo log (`snapshot`/`rollback`), used for
trial unification (`try_unify`, coercions). `unify` is structural; `TyKind::Error` unifies with
everything (errors never cascade); the occurs check rejects cyclic bindings. Const variables
(array lengths, `Int` bounds) unify the same way.

### Integer literals and `Int` ranges

Kagari has two integer families: Rust integers (`u8`…`i128`, `usize`/`isize` as 64-bit) and
bounded integers `Int<lo, hi>`. Mixing them is never implicit.

- An unsuffixed literal `n` is a **literal variable** carrying the hull `[n, n]`. A suffixed
  literal has the suffix's type (out of range: `E2013`).
- Unifying two literal variables merges their hulls. Binding a literal variable to a type is
  allowed only for an integer type and records a *fit* check: the hull must lie in the type's
  range (checked at the end, `E2013`).
- `+ - * / %` on two literals compute a new literal hull (`range_op`) and *link* the operands and
  result: if any member of a link later becomes a Rust integer type, all do (`resolve_links`).
- `Int` arithmetic computes the result range from the operand ranges: `Int<0, 3> + Int<1, 1>` is
  `Int<1, 4>`. Literal operands become `Int<n, n>`. A divisor whose range contains 0 is `E2016`;
  overflow of the range computation, bitwise operators on `Int`, and generic bounds are `E2015`.
- Rust integer arithmetic needs equal types (shifts excepted). `Int` with a Rust integer is
  `E2014` (convert with `.into()` or `.saturate()`).
- Comparisons accept literal/`Int`/Rust-integer combinations under the same rules, plus `Decimal`
  and `Rat` for ordering; `==`/`!=` need `Eq` (an obligation, `E2017`).
- Compound assignment on an `Int` place saturates at run time, so any `Int` right side is
  accepted.
- `x.into()` (`Integer::into`) records an *into check*: the source range must fit in the target
  range, else `E2018` (use `.saturate()`).
- `as` casts parse but are always `E2019`.

At the end (`finish`): projections are normalized, links resolved, a literal bound to
`Int<?lo, ?hi>` fixes the unknown bounds to its hull, and **every remaining literal variable
becomes `Int<lo, hi>` of its own hull** (so `let x = 3;` is `Int<3, 3>`). Then the fit checks,
pattern-literal fits, trait obligations and into checks run, and every recorded type is resolved;
an unresolved variable is `E2029` (reported once, preferring a value's type over a function
type), and becomes `Ty::Error` in the result.

### Coercions

`coerce` first tries plain unification, then (`Coercion` in the result, read by THIR building):

| Coercion | From → to |
| -------- | --------- |
| `IntWiden` | `Int<a, b>` → `Int<c, d>` with `c ≤ a`, `b ≤ d` |
| `FnDefToPtr` | A named function or tuple constructor → a pointer of the same kind (`scene fn` for a `#[scene]` `pushdown fn`) |
| `ToClosure` | A named function or pointer → the closure type of the same kind |
| `ClosureToPtr` | A closure that captures nothing → a pointer of its kind (`E2036` if it captures) |
| `MutToShared` | `&mut T` → `&T` |
| `NeverToAny` | `!` → any type |

`if`/`match` arms without an expected type are joined (`join_arms`): `!` arms are ignored,
bounded arms join to their hull (`if c { 0 } else { 5 }` is `Int<0, 5>`), anything else must
agree with the first arm.

### Paths, calls and methods

- **Paths** (`path.rs`) become `ValueRef`s: a local, a function or tuple constructor with its
  generic arguments (`FnRef`), a unit constructor (`CtorRef`), a `const` (with impl arguments), a
  `static`, or a const parameter. Missing generic arguments become fresh variables;
  `Resolution::Deferred` paths are finished here (`T::f` through `T`'s bounds, `Self::X`,
  associated items of aliases).
- **Calls** instantiate the callee's signature (`fn_shape`), unify the return type with the
  expectation first, then check arguments as coercion sites. Arity: `E2024`; a non-callable:
  `E2025`.
- **Methods** auto-dereference the receiver, look up the name in the type's inherent impl, then in
  the traits that apply (bounds of a parameter, the trait of `Self`, `Integer` for a literal, the
  traits the type implements), and record `ReceiverAdjust { derefs, autoref }`. Unknown: `E2022`;
  an associated function called as a method: `E2023`.
- **Fields** auto-dereference too (`FieldRef { derefs, index }`); unknown field `E2020`, missing
  record fields `E2021`.
- **Trait bounds** of the instantiated generics become *obligations* checked after solving by
  `TraitSolver` (`E2026`). A type implements a trait through a matching std `impl` (whose own
  bounds hold), a bound in scope, or a built-in rule: `Eq` is **structural** (booleans, integers,
  `Int`, pointers and named functions; tuples, arrays, references and non-builtin ADTs whose
  fields are `Eq`; never closures or builtin types without an impl), and `RandChoose` holds for
  every non-builtin, non-empty fieldless enum (`Int<lo, hi>` has an explicit impl).
- **Closures** get the kind of the expected pointer or closure type, else the kind of the
  enclosing function; `ClosureInfo` records the kind and the captured locals in order of first
  use.

### Special cases

| Construct | Rule |
| --------- | ---- |
| `nav::start(entry, args, on_exit)` | Typed by `infer_nav_start`, not by its std signature: `entry` must name an `#[entry]` scene (`E2027`), coerces to its `scene fn(P…) -> R` pointer; `args` must be the tuple `(P…)`; `on_exit` must be `Fn(R)`. The call is recorded as `nav::start::<scene fn(P…) -> R, (P…), R>`. Wrong arity: `E2024` |
| `asset::<A>("…")`, `system_font("…")` | The path must be a string literal (`E2028`); the file itself is checked by name resolution ([Asset paths](./resolve.md#asset-paths)) |
| Range expression | Needs both bounds (`E2030`); `a..`, `..b` and `..` exist only in patterns. `a..b` is `Range<T>` with the bounds joined like `if` arms |
| String literal | Type `Text` (std `narrative::text::Text`) |
| Decimal literal | Type `Decimal` |
| `?` | On `Option` in a function returning `Option`, on `Result` in a function returning `Result` with the same error type (no conversion); otherwise `E2034` |
| `for` | Over arrays, `Vec<T>` and ranges of integers (`E2031` otherwise). Over a range literal of `Int` or integer literals, the loop variable gets the range's own bounds: `for i in 0..3` binds `i: Int<0, 2>` |

`InferenceResult` (per body) holds the type of every expression (before coercion), pattern and
local; `paths`, `method_calls`, `fields`, `records`, `pat_ctors`, `closures`, `coercions`; and the
declared return type. No inference variable survives into it.

## Exhaustiveness and refutability

`pattern_checks(program, item)` implements Maranget-style **usefulness with constructor splitting**
(as in rustc's `pattern/usefulness`). HIR patterns are lowered to deconstructed patterns (a
constructor with sub-patterns); the arms form a matrix; a row is useful when some value it
matches is matched by no earlier row.

- Constructors by type: `bool` → `true`/`false`; Rust integers and `Int<lo, hi>` → integer ranges
  split at the boundaries written in the column; enums → variants (uninhabited ones excluded);
  structs, tuples and fixed-length arrays (up to 1024 elements) → one constructor; `&T` is matched
  through. Every other type (`Text`, `String`, `Decimal`, parameters, …) is an infinite domain:
  only wildcards cover it.
- `const` items used as patterns are read from their initializer: integers through the type-level
  folder, literals, constructors and tuples/records of those syntactically; anything else is an
  opaque constructor that covers only itself.
- An arm that is not useful is `W2039`. A guarded arm never counts toward coverage. The values the
  matrix misses are the witnesses of `E2037` (`non-exhaustive patterns: … not covered`, at most
  three shown, capped at 256 computed).
- `let`, function and closure parameters and `for` patterns must be irrefutable (`E2038`);
  `if let` and `while let` may be refutable.
- Scrutinees whose type has errors or unresolved variables are skipped.

The algorithm sits behind a `PatEnv` trait, so `typeck/exhaustive/tests.rs` tests it on synthetic
types without a database program. MIR building compiles the same patterns to decision trees with
Jacobs' algorithm (`mir/build/matches.rs`); the two are independent and must agree on what a pattern matches.

## Kind and story rules

`kind_checks(program, item)` checks an item's attributes and declared types (`typeck/kinds.rs`),
then walks its body (`kinds/body.rs`) with a stack of contexts. The context of a body is the kind
of its `fn` (`Plain`, `Const`, `Pushdown`); `const`/`static` initializers are `Const`; a closure
body has the kind inference recorded in `ClosureInfo`. Only the body of a `#[scene]` `pushdown fn`
is a **scene**; closures never are. Every other `pushdown` body is an **instant procedure**.
Expressions with erroneous types are skipped, so nothing cascades from inference.

The checks use `const_exprs(program, item)` (`eval/const_exprs.rs`): the set of expressions that
are constant expressions. Constant parts of story code are folded at compile time, so they are
exempt from the finite-value rule.

### Call rules

`callee::classify` turns a callee (a named item or a value of pointer/closure type) into a
`CalleeKind`: `Plain`, `Const`, `Instant`, `Scene`, `Step` (`push_step`) or `Rng` (`rng_choose`).

| Caller context | May call | Otherwise |
| -------------- | -------- | --------- |
| `fn` | `fn`, `const fn` | `E3001` (the only way into the story is `nav::start`) |
| `const fn` / constant initializer | `const fn` | `E3001` (`const` code is pure) |
| instant procedure | `const fn`, instant procedures | `fn`: `E3001`; `push_step`: `E3002`; `rng_choose`: `E3024`; a scene: `E3003` |
| scene | `const fn`, instant procedures, scenes, `push_step`, `rng_choose` | `fn`: `E3001` |

The same table applies to calls through pointers and closures, whose types carry the kind, and to
std methods. `E3002` from a desugared form says to mark the function `#[scene]`.

### Story rules

| Rule | Code | Where |
| ---- | ---- | ----- |
| `loop` and `while` only in scenes (instant procedures and story closures need bounded loops) | `E3004` | `body.rs::check_loop` |
| No cycles among instant procedures, through direct calls, pointers and closures | `E3005` | `kinds/instant.rs` |
| `for` in `pushdown` code over an array or a range with constant bounds | `E3006` | `check_for` |
| Story variables (`static mut`) only in `pushdown` code | `E3007` | `path_expr` |
| Everything the story holds at run time has a finite type | `E3008` | `check_value`, `check_local`, `check_item_types` |
| Entry scene parameters and results contain no `Text` | `E3009` | `check_item_types` |
| `const`/`static` initializers are constant expressions | `E3010` | `check_initializer` |
| Immutable `static`s hold only `Setting<T>`/`Meta<T>` handles | `E3011` | `check_item_types` |
| No references in the types of `const`/`static` items | `E3012` | `check_item_types` |
| No `Meta` handle in `const` code | `E3013` | `path_expr` |
| References in the story are second-class: parameters and arguments only; a reference result only from a method with a reference receiver | `E3014` | `check_local`, `check_value`, `check_item_types` |
| Closures in the story are second-class: passed or called, never stored, returned or captured | `E3015` | `check_value`, `check_local`, `closure` |
| A place passed as `&mut` is not passed as another argument of the same call | `E3016` | `places.rs::check_exclusive` |
| Assignment and `&mut` need a mutable place (all code) | `E3017` | `places.rs::check_mut_place` |
| `#[scene]` on `pushdown fn`, `#[entry]` with `#[scene]`, `#[must_use]` on structs and enums | `E3018` | `kinds.rs::check_attributes` |

**Finite types** (`kinds/finite.rs`): `bool`, `()`, `!`, `Int<lo, hi>`, enums and structs whose
fields are finite (generic arguments substituted), tuples and arrays of finite types, `Text` and
the asset types, and `pushdown fn`, `scene fn` and `const fn` pointers and named functions of those
kinds. Rust integers and other `#[builtin]` types are not (`EntityId`, `Part`, `Mark`, `Duration`,
`Color` hold Rust integers). Generic parameters count as finite (generic code is checked where it is
written), and so do erroneous types. A reference is allowed at the top of a parameter or argument
only. A type met again inside itself, or nested deeper than 64, is infinite.

**Instant recursion** is the one program-wide kind check. `instant_edges(program, item)` scans the
item's MIR for call-graph edges between instant procedures: direct calls, instant procedures used
as values, and calls through `pushdown` pointer or closure values. A value passed directly as an
argument to story code reaches its call down the stack, so naming it is an edge from the caller;
a value that may escape into storage (a story variable, a constant, a return value) may be called
by any call through a value. `instant_recursion(program)` finds strongly connected components
(Kosaraju) and reports each cycle once (`E3005`), preferring a direct call, with the shortest path
back as the message.

### Form rules

Forms desugar before type checking (see [Syntax](./syntax.md#forms)), so their type-dependent rules
are checks on the desugared code (`kinds/forms.rs`), which also catch hand-written equivalents:

| Rule | Code |
| ---- | ---- |
| A `Choice { id, … }` record's `id` is a string literal, unique among the choices of the function | `E3019` |
| A `Choice`'s `arms` is an array literal of `ChoiceArm { … }` whose values are distinct fieldless variants of one enum | `E3020` |
| `mono_ruby(base, [readings…])`: one reading per character of the base literal | `E3021` |
| `effect(base, e)`: the effect is a constant expression | `E3022` |
| A `#[must_use]` value is neither dropped as an expression statement nor bound to a local that never receives a call of one of its type's own methods | `W3023` |

`#[must_use]` messages name what completes the value (for example `apply_animation` for a screen
change); see `unused_message`.

## Invariants

- No inference variable leaves `infer`; unresolved ones are reported once and become `Ty::Error`.
- `Ty::Error` unifies with and implements everything, and every check skips types that
  `references_error()`: one root error, one diagnostic.
- Checks read HIR, `StoreResolution`, `InferenceResult` and `const_exprs`; only `instant_edges`
  reads MIR and must handle its absence.
- `Int` arithmetic and literal hulls use `i128` with checked operations; `u128` values above
  `i128::MAX` are out of range at compile time.
- Item signatures are lowered once, in `program_signatures`, which reports their diagnostics.
  Code that lowers a declared type again uses a quiet `Sink`, so each type error is reported only
  where the type is written.
- Std items are found by role through `StdItems`; never compare item names in checks.
- Every user-visible rule here is documented for studios: kind rules in the Reference
  ([Function Kinds](../../../reference/en/src/concepts/kinds.md),
  [Values, References and Closures](../../../reference/en/src/syntax/values.md),
  [Numbers](../../../reference/en/src/syntax/numbers.md)), rules for std items in the STD
  Reference ([Compiler Rules for std Items](../../../std/en/src/appendix/rules.md)).

## When you change this

- **New type rule or coercion**: `infer/`, then THIR building if the coercion must be lowered
  (`thir/build/expr.rs` reads `coercions`), and the Reference
  [Types](../../../reference/en/src/syntax/types.md) or [Numbers](../../../reference/en/src/syntax/numbers.md)
  page (en and ja).
- **New std item the checker relies on**: a field in `StdItems` (`signatures/collect.rs::std_items`),
  the [std–checker contract](../std/contract.md), and the STD Reference
  [Compiler Rules](../../../std/en/src/appendix/rules.md) page (en and ja).
- **New kind or story rule**: `typeck/kinds/`, a case in `tests/game_source/script/011__kind_errors`
  (or a new fixture), the Reference [Function Kinds](../../../reference/en/src/concepts/kinds.md)
  page, and a diagnostic ([Diagnostics](./diagnostics.md#adding-a-diagnostic)).
- **Pattern changes**: `typeck/exhaustive.rs` with unit tests in `exhaustive/tests.rs`, fixture
  `013__exhaustiveness`, and the decision-tree lowering in `mir/build/matches.rs`.
- Stage 3 rules that stage 5 or the evaluator rely on (finite types, instant procedures without
  unbounded loops, no instant recursion) are preconditions of [analysis](./analysis.md) and
  [const evaluation](./eval.md): weakening one requires changing those too.
- Run `cargo test -p kagari-lang` (fixtures `010__type_errors`, `011__kind_errors`,
  `013__exhaustiveness`, `014__rng_errors`, and the unit tests).
