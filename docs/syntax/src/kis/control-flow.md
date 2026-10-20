# Scenes and Control Flow

Control flow has four forms: `<call>` and `<return>` between scenes, `<goto>` within a scene, and
`<match>` for branching on variables. There is no `if`.

## `<scene>`: scene declaration

```xml
<scene name="festival_hub">
  ...
</scene>
```

A scene is a named sequence of script content. Scenes are the unit of calling: the call stack
holds one frame per active scene.

| Attribute | Required | Meaning                                                                    |
| --------- | -------- | -------------------------------------------------------------------------- |
| `name`    | yes      | Scene name, unique in the program                                          |
| `returns` | no       | Type of the value the scene returns: `bool`, `char` or an enum name        |
| `entry`   | no       | `true` if the system can start the story here (see [Entry scenes](#entry-scenes)) |

- The story starts at an [entry scene](#entry-scenes).
- Scenes may contain local variable declarations (see [Enums and Variables](./data.md)).
- Scenes do not nest.

When execution reaches the end of a scene, the scene returns: its frame is popped and execution
continues after the `<call>` that entered it. A scene with `returns` cannot end this way. It must
leave through a `<return>` that carries a value (see [Returning a value](#returning-a-value)).

## `<call>`: enter a scene

```xml
<call scene="festival_hub" />
<call scene="confession" assign="answer" />
```

Pushes a frame holding the return position, then continues at the start of the named scene.
Calls may be recursive.

| Attribute | Required | Meaning                                         |
| --------- | -------- | ----------------------------------------------- |
| `scene`   | yes      | The scene to call                               |
| `assign`  | no       | The variable that receives the returned value   |

Because the stack remembers where to return, one scene can serve several callers. The festival
hub can be called from `day1` and from `day2` and returns to the right place each time, without
being duplicated.

{{#include ../diagrams/call-stack.svg}}

When the called scene returns, `assign` receives its value. `assign` names a variable visible at
the call, a local of the calling scene or a global, of the called scene's `returns` type. It is
optional even when the called scene returns a value; without it, the value is dropped. `assign` on
a call to a scene without `returns`, or naming a variable of another type, is a compile error.

## `<return>`: leave the scene early

```xml
<return />
<return literal="Accepted" />
<return var="qte_success" />
```

Returns from the current scene immediately, exactly as if execution had reached its end, and
passes a value to the caller if the scene has `returns`. It is an instant element, not a step,
and may appear anywhere in a scene body.

| Attribute | Meaning                                                                        |
| --------- | ------------------------------------------------------------------------------ |
| `var`     | A local variable of the scene; its value is returned                           |
| `literal` | A value, written as in [`<set>`](./data.md#set-assign-a-variable)              |

`var` and `literal` are mutually exclusive. A scene with `returns` needs one of them on every
`<return>`; a scene without `returns` allows neither.

### Returning a value

A scene that needs to report how it ended, such as a confession that is accepted or rejected,
declares `returns` and returns the outcome. The caller receives it with `assign` and branches on
it with `<match>`:

```xml
<scene name="confession" returns="Answer">
  <choice id="confess">
    <choice-arm id="yes" text="Tell her the truth">
      <return literal="Accepted" />
    </choice-arm>
    <choice-arm id="no" text="Say nothing">
      <return literal="Rejected" />
    </choice-arm>
  </choice>
</scene>

<scene name="day3">
  <var-enum name="answer" type="Answer" init="Rejected" />
  <call scene="confession" assign="answer" />
  <match var="answer">
    ...
  </match>
</scene>
```

- `returns` is `bool`, `char` or the name of an enum. A scene cannot return `int`; use `char` or
  an enum instead.
- Every path through a scene with `returns` must end in a `<return>`. A path that can reach the
  end of the scene is a compile error. Like the [termination check](#termination), this check
  counts every branch as possible. A loop that never ends needs no `<return>`, since it never
  reaches the end.
- `var` names a local of the returning scene, not a global, and its type must be the scene's
  `returns` type.

## Entry scenes

```xml
<scene name="main" entry="true">
  ...
</scene>
<scene name="after_story" entry="true">
  ...
</scene>
```

An entry scene is a starting scene that the system can start, by the scene's name (see
[System Script](./system.md)). `<game>` must contain at least one. A program has one `<game>`, so
extra chapters and after stories are further entry scenes of the same `<game>`. An entry scene can
also be called with `<call>` like any other scene.

### The ending

When the entry scene returns, the story is over and control goes back to the system. The system
then decides what comes next, such as rolling the credits or returning to the title, and that
usually depends on which ending the player reached. The entry scene's return value is how the
story tells it.

An entry scene returns `Ending`, an enum the game declares like any other. `entry="true"` implies
`returns="Ending"`. Writing `returns="Ending"` as well is allowed; any other `returns` is a compile
error, and so is an entry scene in a game that declares no `Ending`. The returned value is passed
to the system's [`<on-exit>`](./system.md#game-mode-and-on-exit) handler. Only the value crosses
over: the system cannot see story variables.

A complete program shows where each part lives:

```xml
<kagari>
  <system start="title">
    <game-mode name="game">
      ...
      <!-- 3. Receives the returned `Ending` and matches on it. -->
      <on-exit>
        <case val="AzusaEnd"><meta-set var="cleared_azusa" val="true" /><goto page="credits" /></case>
        <case val="NormalEnd"><goto page="credits" /></case>
      </on-exit>
    </game-mode>
  </system>

  <game>
    <!-- 1. The endings of the story. -->
    <declare-enum name="Ending">
      <enum name="NormalEnd" repr="0" />
      <enum name="AzusaEnd"  repr="1" />
    </declare-enum>

    <!-- 2. The entry scene returns the ending it reached. -->
    <scene name="main" entry="true">
      ...
      <match var="route">
        <cases>
          <case val="Azusa" />
          <do>
            <return literal="AzusaEnd" />
          </do>
        </cases>
        <cases>
          <case val="_" />
          <do />
        </cases>
      </match>
      <return literal="NormalEnd" />
    </scene>
  </game>
</kagari>
```

Because every path through `main` must end in a `<return>`, a path that forgets its ending is a
compile error rather than a silent default. Because `<on-exit>` must cover every variant of
`Ending`, adding an ending without handling it in the system is a compile error too.

This is the same pattern a scene uses to report to its caller (see
[Returning a value](#returning-a-value)), with the system as the caller and `<on-exit>` in place
of the `<match>`.

## `<goto>`: jump within the scene

```xml
<goto label="map" />
```

Continues at the named `<label>` of the current scene. A `<goto>` cannot leave its scene: jumping
into the middle of another scene would break call/return nesting and make the program
unanalyzable. Use `<call>` to move between scenes.

A `<goto>` may only target a label in its own block or in a block that encloses it, such as the
scene body around a `<choice-arm>` or a `<do>`. It cannot jump into a nested block, for example
into another arm of a choice. This keeps every block entered from its start.

A scene that runs in a cycle, such as a day loop, is written as a `<goto>` loop in one driver
scene that calls each day in turn. Scenes that call each other in a ring and never return grow the
call stack on every lap.

## Termination

Everything between two steps runs instantly, so it must always finish. The compiler rejects:

- a `<goto>` loop that does not pass through a step, and
- a cycle of calls that does not pass through a step, such as a scene that calls itself before
  any step.

The check is interprocedural. For each scene the compiler computes whether it *may return without
passing a step*; a `<call>` to such a scene counts as stepless. A loop or call cycle is rejected
when every path around it may be stepless. The computation is a least fixpoint over the call
graph, linear in the size of the program.

## `<match>`: branch on variables

```xml
<match var="route">
  <cases>
    <case val="Hanako" />
    <do>
      <call scene="hanako_ending" />
    </do>
  </cases>
  <cases>
    <case val="Azusa" />
    <case val="Both" />
    <do>
      <call scene="azusa_ending" />
    </do>
  </cases>
  <cases>
    <case val="None" />
    <do>
      <call scene="normal_ending" />
    </do>
  </cases>
</match>
```

`<match>` reads a variable and runs the body of the first `<cases>` block that matches its value.
After the body, execution continues after `</match>`. `<match>` is not a step.

| Element   | Contains                                  | Attribute      | Meaning                         |
| --------- | ----------------------------------------- | -------------- | ------------------------------- |
| `<match>` | one or more `<cases>`                     | `var` or `vars` | Variable(s) to inspect         |
| `<cases>` | one or more `<case>`, then one `<do>`     |                | One arm of the match            |
| `<case>`  | nothing                                   | `val`          | A pattern that selects this arm |
| `<do>`    | script content                            |                | Body run when the arm is chosen |

Several `<case>` elements in one `<cases>` are alternatives: the arm runs if the variable matches
any of them. `<do>` may be empty.

### Patterns

| Pattern          | Matches                                       | Types            |
| ---------------- | --------------------------------------------- | ---------------- |
| A literal        | That value, written as in [`<set>`](./data.md#set-assign-a-variable) | all |
| `_`              | Any value                                     | all              |
| `lo..=hi`        | *Tentative.* Any value from `lo` to `hi`, inclusive | `char`, `int` |

A variant name is the literal for enums, `true`/`false` for `bool`, a decimal number for `char`
and `int`. A range such as `3..=255` expresses a threshold like "affection is at least 3".

### Matching several variables

*Tentative:* `vars` names several variables separated by spaces, and each `val` then holds one
pattern per variable, in the same order, as in a Rust tuple pattern:

```xml
<match vars="visited_a visited_b visited_c">
  <cases>
    <case val="true true true" />
    <do>
      <call scene="festival_end" />
    </do>
  </cases>
  <cases>
    <case val="_ _ _" />
    <do />
  </cases>
</match>
```

`var` and `vars` are mutually exclusive.

### Exhaustiveness

Every possible value of the variable, or combination of values for `vars`, must be covered by some
`<case>`. The compiler rejects a `<match>` that misses one. For `bool` this means both `true` and
`false`; for an enum, every variant; for `char` and `int`, every number in range, usually with `_`
or a range in the last arm.

### Why no `if`

A boolean is an enum with two variants, so `<match>` covers everything `if` would, and `vars` with
ranges covers compound conditions. Requiring every case to be listed is an authoring rule: it makes
the author decide what happens on each branch, including the one where nothing happens. The
analyzer does not need it, since a missing branch is just an edge to the end of the `<match>`.

Branching on several variables can also be written by nesting `<match>`.
