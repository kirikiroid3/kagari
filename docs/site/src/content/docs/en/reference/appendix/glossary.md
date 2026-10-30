---
title: "Glossary"
---

Terms used in this reference, in alphabetical order. Each entry links to the chapter that defines
it. Terms of the standard library link to the *STD Reference*.

- **Analysis cache.** The file `<out-dir>/analysis.kagari-cache` in which `kagari check` and
  `kagari build` keep the result of the story analysis, to reuse it while the sources, the
  compiler options and the toolchain are unchanged. [The `kagari` CLI](../tools/cli.md#the-analysis-cache)
- **Arm.** One option of a `choice!`, written `t![…] => V` or `t![…] if cond => V`; selecting it
  makes the choice evaluate to `V`. Also a branch of a `match`.
  [Steps](../concepts/steps.md)
- **Asset.** A file in the project's asset directory (image, audio, voice, video, font or shader),
  named in source by a constant such as `asset("bg/roof.png")`, and packed into `.kiarchive`
  files by the build. [Builtin Types](../special/builtin-types.md)
- **Attribute.** An annotation `#[name]` on an item, such as `#[scene]`, `#[entry]` or
  `#[must_use]`. [Attributes](../special/attributes.md)
- **Backlog.** The history of the lines and choices shown in the running story, which the player
  can read back and rewind to. [The Game System](../concepts/system.md)
- **Builtin.** An item implemented by the engine and marked `#[builtin]`; only the standard library
  declares builtins. [Builtin Types](../special/builtin-types.md)
- **Call rules.** The rules that say which function kind may call which: `fn` never calls
  `pushdown fn`, and `pushdown fn` never calls `fn`. [Function Kinds](../concepts/kinds.md)
- **Choice (step).** A step written `choice!{…}` that shows its arms, waits for the player to
  select one, and evaluates to the selected variant. [Steps](../concepts/steps.md)
- **Choice ID.** The stable name of a selection recorded in a save: the function path, the
  choice's `id` and the variant, as in `story::Prologue/confess/Confess::Yes`.
  [Saves, Replay and Determinism](../concepts/saves.md)
- **Component (screen).** Data attached to an entity, such as position, opacity or an image layer;
  the set of component types is fixed by the standard library.
  [Presentation](../concepts/presentation.md)
- **Component (UI).** A plain `fn` that returns a UI value; it runs once when its mode is shown.
  [The Game System](../concepts/system.md)
- **Consistency.** The property that the screen a step shows depends only on the story state, not
  on the path that reached the step; a violation is the warning `W5004`.
  [Presentation](../concepts/presentation.md)
- **Const evaluation.** The compiler running `const fn` code at compile time, within a fixed
  budget, to compute constants, renders and tabulated calls.
  [Constant Expressions](../syntax/constants.md)
- **`const fn`.** The function kind for pure, deterministic, Turing-complete code: renders,
  constants and helpers. [Function Kinds](../concepts/kinds.md)
- **Constant expression.** An expression whose value the compiler computes at compile time.
  [Constant Expressions](../syntax/constants.md)
- **Cursor.** The point on a step's timeline at which a render schedules the next animation.
  [`std::screen`](../../std/screen.md)
- **Debug build.** A build with `kagari build --profile debug`: the same `game.kagari` as a
  release build, plus `game.kagari-cache` with the explored story graph and debug information for
  debugging tools. [The `kagari` CLI](../tools/cli.md#build-profiles)
- **Decidable.** Of a question about programs: answerable by an algorithm for every program. Story
  reachability in Kagari is decidable. [Philosophy](../overview/philosophy.md)
- **Deferred reference.** A setting handle used in a render; the render cannot read its value and
  can only gate an entity on it, so the engine applies it when it draws.
  [Presentation](../concepts/presentation.md)
- **Delta.** A render that is not a keyframe: its output depends on the screen before the step.
  [Presentation](../concepts/presentation.md)
- **Diagnostic.** A numbered error (`E…`) or warning (`W…`) reported by the compiler.
  [Diagnostics](../../diagnostics/index.md)
- **Divergence.** The reason a save no longer fits a changed program, such as a choice ID that no
  longer resolves; loading returns it as an error. [Saves, Replay and Determinism](../concepts/saves.md)
- **Easing.** The curve along which a tween interpolates. [`std::screen::anim`](../../std/screen-anim.md)
- **Effect value.** A shader paired with its arguments, such as a `Filter` or a `Mix`, as the
  screen holds it. [`std::screen::shader`](../../std/screen-shader.md)
- **Ending.** The value an entry scene returns when the story is over; the exit handler receives
  it. [Entry Scenes and Endings](../concepts/entry.md)
- **Entity.** An object on the screen, identified by an `EntityId` constant; renders spawn, change
  and despawn entities. [Presentation](../concepts/presentation.md)
- **Entry arguments.** The values of finite types that `nav::start` passes to an entry scene; the
  save records them and the analysis covers every value. [Entry Scenes and Endings](../concepts/entry.md)
- **Entry scene.** A scene marked `#[entry]`: a scene the game system can start, and a root of the
  story analysis. [Entry Scenes and Endings](../concepts/entry.md)
- **Exit handler.** The closure passed to `nav::start` that receives the ending and decides where
  the game goes next. [Entry Scenes and Endings](../concepts/entry.md)
- **Finite type.** A type with finitely many values, such as `bool`, `Int<lo, hi>` or an enum of
  finite data; every value the story holds at run time has one.
  [Function Kinds](../concepts/kinds.md)
- **`fn`.** The function kind for Turing-complete run-time code: the game system.
  [Function Kinds](../concepts/kinds.md)
- **Form.** A fixed piece of built-in syntax written `name!(…)`, `name![…]` or `name!{…}`, such as
  `s!`, `choice!`, `t!` and the UI forms; allowed only in `.kix` files. [Forms](../syntax/forms.md)
- **Function kind.** See *Kind*.
- **Game mode.** The mode at the base of the mode stack that holds the running story.
  [The Game System](../concepts/system.md)
- **Game overlay.** An overlay that needs a running game, such as Save or Backlog; it receives the
  `Narrative` handle. [The Game System](../concepts/system.md)
- **Game system.** Everything around the story: title page, menus, save and load pages, settings,
  backlog and game view, written as `fn` code. [The Game System](../concepts/system.md)
- **Guard.** The condition `if cond` of a choice arm; the arm is shown only while it holds.
  [Steps](../concepts/steps.md)
- **Headless mode.** Running a game without a window, audio device or gamepads, driven by a
  script. [Headless Mode](../tools/headless.md)
- **Instant procedure.** A `pushdown fn` without `#[scene]`: it never steps, never recurses and
  loops only a constant number of times, so it always finishes without input.
  [Scenes and Instant Procedures](../concepts/scenes.md)
- **`.kagari` file.** The compiled program that `kagari build` writes, `game.kagari`.
  [The `kagari` CLI](../tools/cli.md)
- **`kagari.toml`.** The manifest of a project. [Projects](../tools/projects.md)
- **Keyframe.** A render whose output does not depend on its input screen, so it declares the
  complete screen; the compiler detects keyframes itself. [Presentation](../concepts/presentation.md)
- **`.kiarchive` file.** An asset archive written by `kagari build` next to `game.kagari`.
  [The `kagari` CLI](../tools/cli.md)
- **Kind.** One of the three function kinds, `fn`, `pushdown fn` and `const fn`, written before
  `fn`; it decides what a function may do and call. [Function Kinds](../concepts/kinds.md)
- **`.kis` / `.kix` file.** A source file: `.kis` holds the plain language, `.kix` the language
  plus forms. The extension selects the syntax only. [Source Files and Modules](../syntax/modules.md)
- **Layer.** The image, label or movie content of an entity, changed only through a transition.
  [Presentation](../concepts/presentation.md)
- **Logical screen.** The persistent state of the screen after a step's render: the entities with
  their components, the Hud, the camera, the screen filter and the audio channels. Animations never
  change it. [Presentation](../concepts/presentation.md)
- **Mark.** A named point in the reveal of a text, written `mark(n)`, at which a render can
  schedule animations. [`std::narrative::text`](../../std/narrative-text.md)
- **Meta.** Persistent, global state outside saves, such as unlocks, read-text flags and seen
  images; a `Meta<T>` handle. The story may only write it. [The Game System](../concepts/system.md)
- **Mode.** A frame on the system's mode stack: a root page, an overlay, a game overlay, a dialog
  or the game mode. [The Game System](../concepts/system.md)
- **Motion.** An animation of an entity or the camera along a relative path that ends where it
  started. [`std::screen::anim`](../../std/screen-anim.md)
- **`Narrative` handle.** The game system's only view of the running story: it shows lines and
  choices, sends input, saves and rewinds, but cannot read story variables.
  [The Game System](../concepts/system.md)
- **Overlay.** A mode pushed over the current one, such as Config or Load.
  [The Game System](../concepts/system.md)
- **Plane.** One of the fixed drawing planes of the screen (`Background`, `Stage`, `Front`,
  `OverWindow`); an entity is drawn in its plane, ordered by its `z`.
  [`std::screen`](../../std/screen.md)
- **Prelude.** The names imported into every module automatically. [Source Files and Modules](../syntax/modules.md)
- **Presentation.** What the player sees and hears, computed by render functions and never read
  back by the story. [Presentation](../concepts/presentation.md)
- **Pushdown system.** A finite-state machine with an unbounded call stack; its reachability is
  decidable. The story is one. [Philosophy](../overview/philosophy.md)
- **`pushdown fn`.** The function kind of the story: pushdown control over finite data.
  [Function Kinds](../concepts/kinds.md)
- **Random draw.** A value the story draws with `rng_choose`; the generator's seed is part of the
  save, and the analysis covers every value a draw can give.
  [Saves, Replay and Determinism](../concepts/saves.md)
- **Reachability.** Whether some run of the story can arrive at a given scene, step or state.
  [Philosophy](../overview/philosophy.md)
- **Reactive.** Of a UI attribute or element: re-evaluated whenever a signal or other reactive
  source it reads changes. [The Game System](../concepts/system.md)
- **Render (render function).** A `const fn(Screen) -> Screen` passed as a step's `render`
  attribute: it turns the screen before the step into the screen after it.
  [Presentation](../concepts/presentation.md)
- **Replay.** Running the story again along a recorded trace, to load a save or rewind.
  [Saves, Replay and Determinism](../concepts/saves.md)
- **Root page.** A base mode without a running game, such as Title, Gallery or Credits.
  [The Game System](../concepts/system.md)
- **Runtime.** The program that plays a built game; the desktop runtime implements the runtime
  profile `native_v0`. [The Runtime](../tools/runtime.md)
- **Save.** The record of a running story in a save slot: its trace, from which loading replays
  the story. [Saves, Replay and Determinism](../concepts/saves.md)
- **Scene.** A `pushdown fn` marked `#[scene]`: the only kind of function that may run steps and
  call other scenes. [Scenes and Instant Procedures](../concepts/scenes.md)
- **Screen.** The value of type `Screen` that renders transform: a world of entities with their
  components, plus the screen's own resources. [Presentation](../concepts/presentation.md)
- **Second-class.** Of a reference or a closure in a `pushdown fn`: it may be passed as an
  argument and used, but never stored, returned or captured.
  [Values, References and Closures](../syntax/values.md)
- **Setting.** A persistent, global player preference, declared as a `Setting<T>` handle; it never
  reaches the story. [The Game System](../concepts/system.md)
- **Shader.** A WGSL file (`.wgsl`) that the renderer runs on the GPU, with one of six roles: colour
  filter, mix, texture, mask, local motion or transitional motion. Its asset type, such as
  `ColorShader<A>`, states the role and the argument type `A`.
  [`std::screen::shader`](../../std/screen-shader.md)
- **σ (sigma).** The narrative state: the position, the call stack with its locals, and the story
  variables. [The Story Machine](../concepts/story.md)
- **Signal.** A reactive value in `fn` code, created by `signal(init)` as a getter and a setter.
  [The Game System](../concepts/system.md)
- **Snapshot.** A copy of σ and the logical screen kept as a cache next to a trace, so that
  replay can start from it. *Tentative:* the current runtime does not store snapshots; it replays
  the trace. [Saves, Replay and Determinism](../concepts/saves.md)
- **State cap.** The most story states the analysis explores, 200 000 by default; past it the
  warning `W5006` is reported. [Projects](../tools/projects.md)
- **Step.** A point where the story stops and waits for the player: an `s!(…)` or a `choice!{…}`.
  [Steps](../concepts/steps.md)
- **Step ID.** The stable name of a step: its function path, `#` and the step's position among
  the steps of that function in source order, such as `crate::story::FirstDate#28`. Read flags
  are keyed by it. [Steps](../concepts/steps.md#step-ids)
- **Story.** The `pushdown fn` of a game: its scenes, instant procedures and story variables.
  [The Story Machine](../concepts/story.md)
- **Story machine.** The pushdown machine that runs the story inside the game mode.
  [The Story Machine](../concepts/story.md)
- **Story variable.** A `static mut` of finite type: global narrative state for one run of the
  story, readable and writable only from `pushdown fn`. [The Story Machine](../concepts/story.md)
- **Studio.** The team that writes and ships a game. [Introduction](../index.md)
- **Stuck choice.** A `choice!` reachable in a state where every arm's guard is false; the error
  `E5003`. [Steps](../concepts/steps.md)
- **System machine.** The outer machine of a running game: the stack of modes.
  [The Game System](../concepts/system.md)
- **Tabulation.** Evaluating a `const fn` called from the story, at compile time, on every
  combination of its finite arguments, to prove that the call finishes with a finite result on
  every possible input. [Function Kinds](../concepts/kinds.md)
- **Termination.** The guarantee that the code between two steps always finishes.
  [Scenes and Instant Procedures](../concepts/scenes.md)
- **Text.** A rich text value of type `Text`, written `t![…]`, with ruby and text effects.
  [Steps](../concepts/steps.md)
- **Text effect.** A constant `TextEffect` applied to part of a text with `!["…"](effect)`, such as
  a colour or a pause. [Steps](../concepts/steps.md)
- **Timeline.** The schedule of animations and sounds of one step; it is not part of the logical
  screen. [`std::screen`](../../std/screen.md)
- **Trace.** The record a save keeps: the entry scene and its arguments, the choice IDs of every
  selection, the steps after the last choice, the script hash and the random seed.
  [Saves, Replay and Determinism](../concepts/saves.md)
- **Transition.** The animation of a layer swap: cut, crossfade, rule, wipe and others.
  [`std::screen::anim`](../../std/screen-anim.md)
- **Tween.** The animation of a property from its old value to its new one, with a duration,
  an easing and a delay. [`std::screen::anim`](../../std/screen-anim.md)
- **Unbounded screens.** A render that makes the set of screens at a step grow past the cap; the
  error `E5005`. [Presentation](../concepts/presentation.md)
- **Value semantics.** Every type is a value type: assignment, passing and returning copy the
  value, and changing the copy never changes the original.
  [Values, References and Closures](../syntax/values.md)
- **Witness.** A path from the start of the story to a position. A save is a witness that its
  position is reachable. [Saves, Replay and Determinism](../concepts/saves.md)
