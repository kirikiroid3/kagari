# Quick Reference

A one-page summary of the language. Each section links to the chapter with the full rules.

## Function kinds

| Kind | Calls | Holds at runtime | Steps | Story variables |
| ---- | ----- | ---------------- | ----- | --------------- |
| `fn` | `fn`, `const fn`; starts entry scenes with `nav::start` | anything | no | no |
| `#[scene] pushdown fn` (scene) | scenes, instant procedures, `const fn` (constant or finite arguments) | [finite types](../concepts/kinds.md#finite-types) | yes | read and write |
| `pushdown fn` (instant procedure) | instant procedures, `const fn`; no recursion, only bounded `for` loops | finite types | no | read and write |
| `const fn` | `const fn` | anything | no | no |

Only a scene may run a step, call a scene or call `rng_choose`. `pushdown` is a keyword only
directly before `fn` or `Fn`; `scene` only in the pointer type `scene fn(…) -> …`. See
[Function Kinds](../concepts/kinds.md).

| Pointer type | Points to | Closure type |
| ------------ | --------- | ------------ |
| `fn(A) -> B` | a `fn` | `Fn(A) -> B` |
| `const fn(A) -> B` | a `const fn` or a non-capturing closure | `const Fn(A) -> B` |
| `pushdown fn(A) -> B` | an instant procedure | `pushdown Fn(A) -> B` |
| `scene fn(A) -> B` | a scene | none |

## Attributes

| Attribute | On | Meaning |
| --------- | -- | ------- |
| `#[scene]` | `pushdown fn` | A scene: may run steps, call scenes and draw random values |
| `#[entry]` | a `#[scene]` with finite parameter and return types that contain no `Text` | The system can start the story here |
| `#[must_use]` | `struct`, `enum` | Dropping a value, or never completing it, is warning `W3023` |
| `#[builtin]` | functions, structs, traits | Implemented by the compiler and the engine; standard library only |

No attribute takes arguments. See [Attributes](../special/attributes.md).

## Special types

| Type | Finite | `Eq` | Note |
| ---- | ------ | ---- | ---- |
| `bool`, `()`, `Int<lo, hi>` | yes | yes | `Int<lo, hi>` is the story's integer type |
| Rust integers, `Rat`, `Decimal` | no | yes | `0.5` is a `Decimal` |
| `Text` | yes | yes | type of string literals and `t![…]` |
| asset types | yes | yes | values are the asset constants |
| `Option<T>`, structs, enums, tuples, arrays | if every part is | if every part is | no recursive types in the story |
| `scene fn`, `pushdown fn`, `const fn` pointers, `Render` | yes | yes (identity) | |
| `fn` pointers | no | yes (identity) | |
| closures | no | no | second-class in the story |
| `String`, `Vec<T>`, `Duration`, `Color`, `EntityId` | no | yes (`Vec` if `T: Eq`) | constants are still allowed in the story |
| `Screen` | no | yes | the type renders transform |
| `Setting<T>`, `Meta<T>`, `Narrative`, `Element`, screen parts | no | no | opaque handles and values |

See [Builtin Types](../special/builtin-types.md) and [Language Traits](../special/traits.md).

## Forms (`.kix` only) {#forms}

| Delimiter | Accepts |
| --------- | ------- |
| `name!( … )` | attributes, then a fixed number of elements |
| `name![ … ]` | elements only, any number |
| `name!{ … }` | attributes, then any number of elements |

Attributes are `key = expr`, come before the elements, and appear at most once. Elements are
separated by commas, except the juxtaposed items of `t![…]`.

| Form | Desugars to | Value | Attributes | Elements |
| ---- | ----------- | ----- | ---------- | -------- |
| `s!(…)` | `push_step(Say { … })` | `()` | `who`, `voice`, `render`, `history` | at most one `t![…]` |
| `choice!{…}` | `push_step(Choice { … })` | the arms' value type | `id` (required, a string literal), `render` | arms `t![…] => V` and `t![…] if cond => V` |
| `t![…]` | `concat([…])` of `ruby`, `mono_ruby`, `effect` and `to_text` calls | `Text` | none | `"…"`, `["基"]("よみ")`, `["漢字"]("かん"\|"じ")`, `!["…"](effect)`, `{expr}` |
| `column![…]`, `row![…]`, `layer![…]` | `ui::column`, `ui::row`, `ui::layer` | `Element` | none | any number |
| `text!(…)` | `ui::text` | `Element` | `style` | exactly one |
| `image!(…)` | `ui::image` | `Element` | `fit` | exactly one |
| `game_screen!(…)` | `ui::game_screen` | `Element` | `game` (required) | none |
| `button!{…}` | `ui::button` | `Element` | `on_click`, `disabled`, `style` | any number |
| `slider!{…}` | `ui::slider` | `Element` | `value` (required), `on_change`, `step`, `disabled`, `style` | none |
| `toggle!{…}` | `ui::toggle` | `Element` | `value` (required), `on_change`, `disabled`, `style` | any number |
| `select!{…}` | `ui::select` | `Element` | `value` (required), `on_change`, `disabled`, `style` | any number |

`s!` and `choice!` are allowed only in a scene. The UI forms call `fn` functions, so they are used
in `fn` code. See [Forms](../syntax/forms.md) and [Steps](../concepts/steps.md).

## Story items

| Item | Form |
| ---- | ---- |
| Scene | `#[scene] pushdown fn Name(…) -> R { … }` |
| Entry scene | `#[scene] #[entry] pub pushdown fn Main(…) -> Ending { … }` |
| Instant procedure | `pushdown fn name(…) -> R { … }` |
| Story variable | `static mut NAME: FiniteType = constant;` |
| Setting | `static NAME: Setting<T> = setting(init);` |
| Meta field | `static NAME: Meta<T> = meta(init);` |
| Asset | `const NAME: ImageAsset = asset("path");`, likewise `AudioAsset`, `VoiceAsset`, `VideoAsset`, `FontAsset`, `ShaderAsset` |
| System font | `const NAME: FontAsset = system_font("path");` |
| Entity id | `const NAME: EntityId = EntityId(n);`, with `n` ≥ 1 (0 is `BACKGROUND`) |
| Render | `const fn name(mut screen: Screen) -> Screen { …; screen }` |
| Program | `fn main() -> System { … }` in the root module |

## Control flow in the story

| Need | Write |
| ---- | ----- |
| Show a line | `s!(who = "Mio", t!["…"]);` |
| Ask a question | `match choice!{id = "ask", t!["Yes"] => Answer::Yes, t!["No"] => Answer::No} { … }` |
| Hide an arm | `t!["…"] if condition => V` |
| Call a scene | `Name(x);` or `let v = Name(x);` |
| Leave a scene | `return;` / `return value;`, or the block's final expression |
| Branch | `if`, `if let`, `match` (exhaustive, with ranges and tuples) |
| Hub or repeated choice | `loop { match choice!{…} { … } }` with `break` |
| Bounded repetition | `for i in 0..n` with constant bounds |
| Change a counter | `x += 1;` (saturates at the bounds of the `Int`) |
| Compute from story state | call a `const fn`; finite runtime arguments are tabulated |
| Write meta | `META_FIELD.write(v);` |
| Draw at random (scenes only) | `rng_choose::<T>()`, `T` a fieldless enum or `Int<lo, hi>` |
| End the story | return from the entry scene; the value goes to the `on_exit` of `nav::start` |

## Common `std` calls

| Need | Call | Kind |
| ---- | ---- | ---- |
| A time | `ms(500)`, `secs(2)` | `const fn` |
| A colour | `rgb(200, 0, 0)`, `hex(0xC80000)`, `BLACK`, `WHITE`, `TRANSPARENT` | `const fn` |
| A position | `point(640, 360)`, `offset(0, -20)` | `const fn` |
| Change the background | `screen.background().change(BG_ROOF).apply_animation(fade(ms(500)));` | `const fn` |
| Swap a sprite | `screen.entity(MIO).image().change(MIO_SMILE).apply_animation(cut());` | `const fn` |
| Fade an entity in | `screen.entity(MIO).opacity().change(1.0).apply_animation(tween(ms(300)));` | `const fn` |
| Play a motion | `screen.entity(MIO).apply_animation(jump(20, ms(300)));` | `const fn` |
| Show a character | `character(MIO).show(&mut screen, MIO_SMILE, point(320, 0), tween(ms(300)));` | `const fn` |
| Change the music | `screen.music().change(looped(THEME)).apply_animation(crossfade(secs(1)));` | `const fn` |
| Play a sound | `screen.play(sound(CHIME));` | `const fn` |
| Wait in a render | `screen.wait(ms(500));` | `const fn` |
| Style text | `t![!["red"](color_text(rgb(200, 0, 0))) ![""](click_wait())]` | in `t![…]` |
| Convert integers | `x.into()`, `x.saturate()` | `const fn` |
| Read or change a setting | `MUSIC.get()`, `MUSIC.set(v)` | `fn` |
| Read meta | `CLEARED.get()` | `fn` |
| Start the story | `nav::start(story::Main, (CLEARED.get(),), on_ending)` | `fn` |
| Change modes | `nav::call(Config)`, `nav::dialog(Confirm)`, `nav::back()`, `nav::goto(Title)`, `nav::load(slot)`, `nav::quit()` | `fn` |
| A reactive value | `let (count, set_count) = signal(0u32);` | `fn` |

The names above are in the prelude except the studio's own constants (`BG_ROOF`, `MIO`, …). The
full API is in the [STD Reference](../../../../std/en/src/usage.md).
