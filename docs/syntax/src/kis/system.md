# System Script

`<system>` holds the **game system** around the story: settings, meta, save slots, system
functions, and the modes (title screen, save and load pages, config, backlog, the in-game UI) with
their UI. This chapter gives the syntax and the compile-time checks. What the elements mean at
runtime (the mode stack, the operations, the mode-graph checks) is described in the *Kagari STD
Book*.

All element and attribute names in this chapter are *tentative*, except `<system>` itself and the
call form of handler attributes: an attribute such as `on-click` holds one call, as in
`on-click="interface__call(save)"` or `on-click="quick_save()"`.

## `<system>`

```xml
<system start="title">
  <import src="std/ui.kix" />
  <!-- declarations, system functions, modes -->
</system>
```

`start` is required and names the root page the system starts on. An unknown name is
`kis/unknown-mode`; a mode that is not a root page is `kis/not-a-root-page`.

`<system>` may contain, in any order:

- `<import>` and `<include>`, scoped to the system (see
  [Templates, Include and Import](./reuse.md)). An included file has the root `<kagari-system>`.
- `<declare-enum>`.
- Settings, meta fields and `<save-slots>`.
- System functions: `<sysfn>`.
- Modes: `<page>`, `<dialog>` and `<game-mode>`.

Anything else is `kis/element-not-allowed` ("… is not allowed at the top of `<system>`").

The system has no global variables. The only system variables are the locals of system
functions, settings, meta fields and the bindings of [repetition](#repetition) elements.

## Declarations

### Settings

```xml
<setting-int  name="bgm_volume"  min="0" max="100" init="80" />
<setting-enum name="text_speed"  type="Speed" init="Normal" />
<setting-bool name="skip_unread" init="false" />
```

| Element          | Attributes                     | Declares |
| ---------------- | ------------------------------ | -------- |
| `<setting-int>`  | `name`, `min`, `max`, `init`   | An integer setting from `min` to `max` |
| `<setting-enum>` | `name`, `type`, `init`         | A setting of the enum `type` |
| `<setting-bool>` | `name`, `init`                 | A `bool` setting |

All attributes are required. System functions read settings and write them with `<set>`, and
numeric settings also with `<add>` and `<sub>`, which saturate at the bounds. UI controls write
them through [`bind`](#ui-elements).

### Meta fields

```xml
<meta-bool name="cleared"    init="false" />
<meta-enum name="last_route" type="Route" init="None" />
```

| Element       | Attributes                   | Declares |
| ------------- | ---------------------------- | -------- |
| `<meta-bool>` | `name`, `init`               | A `bool` meta field |
| `<meta-char>` | `name`, `init`               | A `char` meta field |
| `<meta-int>`  | `name`, `min`, `max`, `init` | An `int` meta field from `min` to `max` |
| `<meta-enum>` | `name`, `type`, `init`       | A meta field of the enum `type` |

All attributes are required. `init` is the value used when the stored file has no value for the
field, for example on first boot or after an update adds a field.

System code reads meta fields and writes them only with
[`<meta-set>`](./data.md#meta-set-write-a-meta-field). `<set>`, `<add>` or `<sub>` on a meta field
is `kis/read-only-variable`. The game writes meta fields with `<meta-set>` and reads them only
through [`from-meta`](./data.md#from-meta-copy-a-meta-field-at-start).

Settings and meta fields share one namespace. A setting whose name is taken is
`kis/duplicate-setting`; a meta field whose name is taken is `kis/duplicate-meta`.

### Save slots

```xml
<save-slots count="6" />
```

`count` is at least 1 and fixes the number of slots, so the slot list in the UI is finite. At
most one `<save-slots>` is allowed; a second is `kis/element-not-allowed`. Without `<save-slots>`,
any slot operation or `<for-slots>` is `kis/no-save-slots`.

### Enums

`<declare-enum>` is allowed in `<system>`, `<kagari-system>` and the game. Enums share one
program-wide namespace: the system can use the game's `Ending` or `Route`, and a meta field can
have a game enum type. Declaring the same name twice, in any scopes, is `kis/duplicate-enum`. An
enum may be used before its declaration.

### `std/input.kis`

The bundled file `std/input.kis` (root `<kagari-system>`) is included automatically in the system
scope. It declares the enums of the input payloads. Their reprs are consecutive from 0 in this
order:

| Enum        | Variants |
| ----------- | -------- |
| `Key`       | `Enter`, `Escape`, `Space`, `Backspace`, `Tab`, `Up`, `Down`, `Left`, `Right`, `PageUp`, `PageDown`, `Home`, `End`, `Shift`, `Control`, `Alt`, `F1`…`F12`, `A`…`Z`, `Digit0`…`Digit9` (64 variants) |
| `PadButton` | `None`, `South`, `East`, `West`, `North`, `LeftShoulder`, `RightShoulder`, `LeftTrigger`, `RightTrigger`, `Start`, `Select`, `LeftStick`, `RightStick` |

## System functions

All system logic lives in **system functions**. A handler attribute calls one, or calls one
[operation](#operations) directly.

```xml
<sysfn name="louder" args="i16" return="i16">
  <var-i16 name="step" init="0" />
  <var-i16 name="v" init="0" />
  <sysfn-arg position="0" var="step" />
  <add var="bgm_volume" val="10" />
  <sysfn-return val="v" />
</sysfn>
```

| Attribute | Meaning |
| --------- | ------- |
| `name`    | Unique among system functions (`kis/duplicate-sysfn`). A name starting with `metafn__` or `interface__` is `kis/reserved-name` |
| `args`    | A whitespace-separated list of argument types. Absent or empty means no arguments |
| `return`  | Optional; one return type |

### Types

The types of `args` and `return` are `bool`, `char`, `i16` (an integer from −32768 to 32767),
`vec2` (two `i16` values) and enum names. `int` is `kis/bad-sysfn-type` ("`int` needs a range;
use `i16` or an enum"): a ranged `int` exists only in settings, meta fields and `<var-int>` locals.
Any other unknown name is `kis/unknown-enum`.

A `vec2` literal is two signed integers separated by whitespace, such as `"0 -1"`. x grows to the
right and y grows downward.

**Assignability.** A value of type `src` may flow into `dst` (call arguments, `save`,
`<sysfn-arg>`, `<sysfn-return>`, operation parameters) when the types are equal, or when both are
numeric (`char`, `int`, `i16`) and every value of `src` is a value of `dst`. Narrowing is an
error. `vec2` and enums require the same type.

### Locals

Locals are direct children of `<sysfn>`. They are resolved before the body, so their position
does not matter:

- `<var-bool>`, `<var-char>`, `<var-enum>`, `<var-int>`, as in the game
  (see [Variables](./data.md)).
- `<var-i16 name init>`.
- `<var-vec2 name init>`, where `init` is a `vec2` literal.

A `<var-*>` anywhere else in a system function is `kis/element-not-allowed`. `<var-i16>` and
`<var-vec2>` in the game are `kis/element-not-allowed`. A local repeated in one function is
`kis/duplicate-variable`; a local with the name of a setting or meta field is
`kis/local-shadows-global`. `from-meta` on a local is ignored, with the warning
`kis/unknown-attribute`.

### `<sysfn-arg>` and `<sysfn-return>`

```xml
<sysfn-arg position="0" var="step" />
<sysfn-return val="v" />
```

`<sysfn-arg position var>` copies argument `position` (counted from 0) into the local `var`. A
position past the arguments is `kis/value-out-of-range`; a `var` that is not a local is
`kis/not-a-local`; a type that is not assignable is `kis/assign-mismatch`.

`<sysfn-return>` ends the function. `val` names a local (`kis/not-a-local` otherwise). A function
with `return` needs `val` with an assignable type; a function without `return` forbids `val`. Both
violations are `kis/return-mismatch`.

A function with `return` must end every path in `<sysfn-return val>`. The check is structural: a
`<match>` returns when all its arms return. A function that can reach its end is
`kis/missing-return` ("system function `f` can reach its end without `<sysfn-return>`").

Both elements are allowed only inside `<sysfn>`.

### `<sysfn-call>` and `<sysfn-param>`

```xml
<sysfn-call name="louder" save="v">
  <sysfn-param var="bgm_volume" />
</sysfn-call>
```

`<sysfn-call name save>` calls another system function. Each `<sysfn-param>` has exactly one of
`var` and `literal` (`kis/var-conflict` otherwise). A parameter count different from the
function's `args` is `kis/sysfn-arity`; a parameter that is not assignable is `kis/arg-mismatch`.
An unknown `name` is `kis/unknown-sysfn`.

`save` names a writable variable, a local or a setting, that receives the return value. A meta
field is `kis/read-only-variable`. `save` on a function without `return`, or with a type that is
not assignable, is `kis/assign-mismatch`.

The call graph of system functions must be acyclic: a cycle is `kis/sysfn-recursion` ("recursive
system functions: `a`, `b`"). System functions have no loops either, so every handler terminates.

The game may not call system functions. `<sysfn-call>` in a scene is
`kis/sysfn-call-in-game` ("system function `f` can only be called from the system scope"), or
`kis/unknown-sysfn` for an unknown name.

### Statements

The body of a system function, and each `<do>` of [`<on-exit>`](#on-exit), may contain:

| Statement | Meaning |
| --------- | ------- |
| `<sysfn-arg>`, `<sysfn-return>` | See above; only inside `<sysfn>` |
| `<sysfn-call>` | See above |
| `<metafn-*>`, `<interface-*>` | The [operations](#operations) |
| `<match var>` / `<match vars>` | As in the [game](./control-flow.md#match-branch-on-variables). A `vec2` variable is `kis/not-matchable` |
| `<set>`, `<add>`, `<sub>` | As in the game, on locals and settings |
| `<meta-set>` | As in the game |

Variables resolve in this order: local, setting, meta field. Any other element is
`kis/element-not-allowed` ("… is not allowed in a system function body").

## Operations

Operations are the only way to change the system's state. Each has a tag form, used in system
function bodies, and an attribute-call form, used in [handler attributes](#handler-attributes).

| Tag | Attribute call | Parameter |
| --- | -------------- | --------- |
| `<metafn-start param>` | `metafn__start(e)` | An entry scene. Unknown is `kis/unknown-scene`, not an entry is `kis/not-an-entry` |
| `<metafn-load param>` | `metafn__load(k)` | A slot operand |
| `<metafn-save param>` | `metafn__save(k)` | A slot operand |
| `<metafn-advance>` | `metafn__advance()` | – |
| `<metafn-select param>` | `metafn__select(i)` | A numeric operand; a literal must be 0..=255 |
| `<metafn-quit>` | `metafn__quit()` | – |
| – | `metafn__revert(entry.index)` | Exactly `entry.index`, otherwise `kis/bad-binding` |
| – | `metafn__play_voice(entry.voice)` | Exactly `entry.voice`, otherwise `kis/bad-binding` |
| `<interface-call page>` | `interface__call(m)` | An overlay: a non-root `<page>` or a `<dialog>`, otherwise `kis/not-an-overlay` |
| `<interface-return>` | `interface__return()` | – |
| `<interface-goto page>` | `interface__goto(m)` | A root page, otherwise `kis/not-a-root-page` |
| `<interface-focus-move direction>` / `<interface-focus-move target-idx>` | `interface__focus_move(x)` | See [Focus](#focus) |
| `<interface-focus-activate>` | `interface__focus_activate()` | – |

An unknown mode name is `kis/unknown-mode`. `metafn__start` and `metafn__load` in a program with no
`<game-mode>` are `kis/no-game-mode`. `metafn__revert` and `metafn__play_voice` have no tag form:
their operands exist only inside `<for-history>`.

There is no quit dialog built in: a button that should confirm calls `interface__call(quit)` on a
`<dialog name="quit">` the studio declares. Settings are changed with `<set>` in a system
function.

**Operands.** A parameter text is a literal if it starts with a digit or `-`, contains whitespace,
or is `true` or `false`; an identifier that is a variant of the expected enum is a literal too.
Otherwise it names a variable. Its type must be assignable to the parameter's type.

**Slot operands.** A literal slot is a non-negative integer below `<save-slots count>`; a larger
one is `kis/value-out-of-range`. A numeric variable is also accepted; at runtime, an index out of
range does nothing.

### Focus

`<interface-focus-move>` has exactly one of `direction` and `target-idx`
(`kis/var-conflict` otherwise):

- `direction` is a `vec2` literal or a `vec2` variable.
- `target-idx` is `next`, `prev`, a literal from 0 to 255, or a numeric variable.

In the attribute form, `x` is `next`, `prev`, an integer (an index) or a `vec2` literal (a
direction). `next` and `prev` are keywords only here.

The focusable elements are the `<button>`, `<slider>`, `<toggle>` and `<select>` elements of the
top frame's mode, in document order with repetitions expanded; hidden voice buttons are skipped.
Focus belongs to the frame, and a new frame starts with nothing focused.

- `target-idx="N"` focuses the Nth focusable. An index out of range does nothing.
- `next` and `prev` wrap around.
- With nothing focused, `next`, a direction and `N` start from index 0, and `prev` focuses the
  last focusable.
- A direction focuses the nearest focusable whose center lies within 45° of the vector from the
  focused element's center. If there is none, the focus is unchanged. The vector `0 0` does
  nothing.

`<interface-focus-activate>` runs the `on-click` of a focused button or text frame and flips the
setting of a focused toggle. On anything else it does nothing.

## Handler attributes

A handler attribute holds exactly one call:

```text
ident ( [arg {, arg}] )
```

Whitespace is allowed anywhere, and each argument is trimmed and non-empty. A malformed value is
`kis/bad-handler` ("expected a call such as `f()` or `metafn__save(3)`"). An `ident` starting with
`metafn__` or `interface__` is an [operation](#operations) (`kis/unknown-operation` if there is no
such operation); any other `ident` is a system function (`kis/unknown-sysfn`). A wrong number of
arguments is `kis/sysfn-arity`. Arguments resolve as loop binding, then setting, then meta field;
there are no locals in attributes. A return value is discarded.

| Attribute       | On                                    | Call |
| --------------- | ------------------------------------- | ---- |
| `on-click`      | `<button>`, `<text-frame>`            | Any call, with its arguments written out |
| `on-keyboard`   | `<page>`, `<dialog>`, `<game-mode>`   | A system function with `args="Key"`, written `f()` |
| `on-controller` | `<page>`, `<dialog>`, `<game-mode>`   | A system function with `args="vec2 PadButton"`, written `f()` |

The engine passes the input as positional arguments of the `on-keyboard` and `on-controller`
function, which reads them with `<sysfn-arg>`. An operation, arguments written in the attribute,
or a function with different `args` is `kis/handler-signature`.

Only the top frame's mode receives input:

- `on-keyboard` fires on every key press, including auto-repeat.
- `on-controller` fires on a button press with `0 0` and the button; on a d-pad press with a unit
  vector and `None`; and when a stick leaves its dead zone, with the components scaled to
  −100..=100 and `None`.

Without a handler, that input is ignored; the system does not force controller support. There are
no event objects.

## Modes

```xml
<page name="title" root="true" on-keyboard="title_keys()">…</page>
<dialog name="menu" requires="game">…</dialog>
<game-mode name="game" on-keyboard="game_keys()">…</game-mode>
```

| Element       | Attributes | Meaning |
| ------------- | ---------- | ------- |
| `<page>`      | `name`, `root`, `requires`, `on-keyboard`, `on-controller` | A mode. `root="true"` makes it a root page; otherwise it is an overlay |
| `<dialog>`    | `name`, `requires`, `on-keyboard`, `on-controller` | An overlay drawn over a still image of the frames below |
| `<game-mode>` | `name`, `on-keyboard`, `on-controller` | The mode that runs the story. At most one per program |
| `<layer>`     | `name`, `style` | A named part of a mode's view |
| `<game-screen>` | – | The story's screen, drawn as part of `<game-mode>`. At most one |

Mode names are unique across all modes, and a second `<game-mode>` is `kis/duplicate-mode`.
`root` is a `bool`. `requires="game"` marks an overlay that may only be shown above a running
game, such as Save or Backlog; any other value is `kis/bad-value`, and so is `requires` on a root
page ("a root page cannot require a running game").

`<page>` and `<dialog>` contain layers and UI elements. `<game-mode>` contains only
`<game-screen>`, `<layer>` and exactly one `<on-exit>`.

### `<on-exit>`

`<on-exit>` runs when the story's entry scene returns. It matches on the `Ending` value the
[entry scene](./control-flow.md#entry-scenes) returns, with `<cases>`, `<case val>` and `<do>`
exactly as in [`<match>`](./control-flow.md#match-branch-on-variables). The cases must be
exhaustive (`kis/non-exhaustive-match`). Each `<do>` holds [statements](#statements), without
`<sysfn-arg>` and `<sysfn-return>`.

```xml
<on-exit>
  <cases>
    <case val="Good" />
    <do>
      <meta-set var="cleared" val="true" />
      <interface-goto page="credits" />
    </do>
  </cases>
  <cases>
    <case val="_" />
    <do />
  </cases>
</on-exit>
```

If the case ran no `<interface-goto>`, `<metafn-start>`, `<metafn-load>` or `<metafn-quit>`, the stack resets to the start page of
`<system start>`. A `<game-mode>` without `<on-exit>` is `kis/missing-on-exit`; a second
`<on-exit>` is `kis/element-not-allowed`.

## UI elements

System UI is markup inside the modes. Element types and styles come from `std/ui.kix`; a `style`
is recorded as a name.

| Element                            | Attributes            | Meaning |
| ---------------------------------- | --------------------- | ------- |
| `<button>`                         | `style`, `on-click`   | A clickable element; its content is text only |
| `<slider>`, `<toggle>`, `<select>` | `style`, `bind`       | Controls bound to a setting |
| `<row>`, `<col>`                   | `style`               | Flex layout containers |
| `<text-frame>`                     | `style`, `on-click`   | The current line of the story; only in a layer of `<game-mode>` |
| `<backlog-line>`                   | `style`               | The line of a history entry; only inside `<for-history>` |

`bind` names a setting: `<slider>` a `<setting-int>`, `<toggle>` a `<setting-bool>`, `<select>` a
`<setting-enum>`. Anything that is not a setting is `kis/unknown-setting`; a setting of the wrong
kind is `kis/bind-mismatch`.

### Repetition

The UI repeats over finite collections only. Repetition elements cannot nest in one another.

| Element         | Repeats its body once per             | Binds |
| --------------- | ------------------------------------- | ----- |
| `<for-slots>`   | save slot, up to `<save-slots count>` | `slot.index`, an `int` from 0 to `count - 1` |
| `<for-history>` | history entry of the story            | `entry.index`, `entry.voice` |
| `<for-choices>` | arm offered at the current `<choice>` | `choice.index`, a `char` |

`entry.index` is the entry's position in the run and `entry.voice` the voice of its line. They are
valid only as the arguments of `metafn__revert` and `metafn__play_voice`; any other use is
`kis/bad-binding`. A voice button for an entry without a voice is not shown.

```xml
<page name="backlog" requires="game">
  <for-history>
    <row>
      <backlog-line />
      <button on-click="metafn__play_voice(entry.voice)">▶</button>
      <button on-click="metafn__revert(entry.index)">Revert</button>
    </row>
  </for-history>
  <button on-click="interface__return()">Back</button>
</page>
```

## Example

```xml
<kagari>
  <system start="title">
    <setting-int  name="bgm_volume"  min="0" max="100" init="80" />
    <setting-bool name="skip_unread" init="false" />
    <setting-enum name="text_speed"  type="Speed" init="Normal" />
    <meta-bool name="cleared" init="false" />
    <save-slots count="6" />

    <declare-enum name="Speed">
      <enum name="Slow" repr="0" /><enum name="Normal" repr="1" /><enum name="Fast" repr="2" />
    </declare-enum>

    <sysfn name="title_keys" args="Key">
      <var-enum name="key" type="Key" init="Enter" />
      <sysfn-arg position="0" var="key" />
      <match var="key">
        <cases><case val="Tab" /><do><interface-focus-move target-idx="next" /></do></cases>
        <cases><case val="Up" /><do><interface-focus-move target-idx="prev" /></do></cases>
        <cases><case val="Enter" /><do><interface-focus-activate /></do></cases>
        <cases><case val="_" /><do /></cases>
      </match>
    </sysfn>

    <sysfn name="page_pad" args="vec2 PadButton">
      <var-vec2 name="dir" init="0 0" />
      <var-enum name="button" type="PadButton" init="None" />
      <sysfn-arg position="0" var="dir" />
      <sysfn-arg position="1" var="button" />
      <match var="button">
        <cases><case val="None" /><do><interface-focus-move direction="dir" /></do></cases>
        <cases><case val="South" /><do><interface-focus-activate /></do></cases>
        <cases><case val="East" /><do><interface-return /></do></cases>
        <cases><case val="_" /><do /></cases>
      </match>
    </sysfn>

    <sysfn name="game_keys" args="Key">
      <var-enum name="key" type="Key" init="Enter" />
      <sysfn-arg position="0" var="key" />
      <match var="key">
        <cases><case val="Enter" /><do><metafn-advance /></do></cases>
        <cases><case val="F5" /><do><metafn-save param="0" /></do></cases>
        <cases><case val="Escape" /><do><interface-call page="menu" /></do></cases>
        <cases><case val="_" /><do /></cases>
      </match>
    </sysfn>

    <page name="title" root="true" on-keyboard="title_keys()">
      <button on-click="metafn__start(main)">New Game</button>
      <button on-click="interface__call(load)">Load</button>
      <button on-click="interface__call(config)">Config</button>
      <button on-click="metafn__quit()">Quit</button>
    </page>

    <page name="credits" root="true">
      <button on-click="interface__goto(title)">Back to title</button>
    </page>

    <page name="config" on-controller="page_pad()">
      <slider bind="bgm_volume" />
      <toggle bind="skip_unread" />
      <select bind="text_speed" />
      <button on-click="interface__return()">Back</button>
    </page>

    <page name="load" on-controller="page_pad()">
      <for-slots><button on-click="metafn__load(slot.index)">Load</button></for-slots>
      <button on-click="interface__return()">Back</button>
    </page>

    <page name="save" requires="game" on-controller="page_pad()">
      <for-slots><button on-click="metafn__save(slot.index)">Save</button></for-slots>
      <button on-click="interface__return()">Back</button>
    </page>

    <dialog name="menu" requires="game">
      <button on-click="interface__call(save)">Save</button>
      <button on-click="interface__goto(title)">Title</button>
      <button on-click="interface__return()">Close</button>
    </dialog>

    <game-mode name="game" on-keyboard="game_keys()">
      <game-screen />
      <layer name="text">
        <text-frame on-click="metafn__advance()" />
        <for-choices><button on-click="metafn__select(choice.index)">…</button></for-choices>
      </layer>
      <on-exit>
        <cases>
          <case val="Good" />
          <do>
            <meta-set var="cleared" val="true" />
            <interface-goto page="credits" />
          </do>
        </cases>
        <cases><case val="_" /><do /></cases>
      </on-exit>
    </game-mode>
  </system>

  <game>
    <declare-enum name="Ending">
      <enum name="Good" repr="0" /><enum name="Bad" repr="1" />
    </declare-enum>
    <scene name="main" entry="true">
      ...
    </scene>
  </game>
</kagari>
```
