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

- `name` is required and unique in the program.
- The story starts at a scene exported by an [`<entry>`](#entry-export-a-starting-scene).
- Scenes may contain local variable declarations (see [Enums and Variables](./data.md)).
- Scenes do not nest.

When execution reaches the end of a scene, the scene returns: its frame is popped and execution
continues after the `<call>` that entered it.

## `<call>`: enter a scene

```xml
<call scene="festival_hub" />
```

Pushes a frame holding the return position, then continues at the start of the named scene.
Calls may be recursive.

Because the stack remembers where to return, one scene can serve several callers. The festival
hub can be called from `day1` and from `day2` and returns to the right place each time, without
being duplicated.

{{#include ../diagrams/call-stack.svg}}

A scene that needs to report how it ended (a confession that is accepted or rejected) sets a
global enum before returning, and the caller branches on it with `<match>`.

## `<entry>`: export a starting scene

```xml
<entry name="main" scene="main" exit="ending" />
<entry name="after_story" scene="after_story" exit="ending" />
```

An entry is a named starting scene that the system can start (see [System Script](./system.md)).
Entries are declared at top level, and `<game>` must contain at least one. A program has one
`<game>`, so extra chapters and after stories are further `<entry>` elements of the same `<game>`.

| Attribute | Required | Meaning                                                                 |
| --------- | -------- | ----------------------------------------------------------------------- |
| `name`    | yes      | Name used by `start(…)` in the system                                   |
| `scene`   | yes      | The scene the narrative starts in                                       |
| `exit`    | no       | An enum variable whose value is passed to `<on-exit>`                   |

### `exit`: report how the story ended

When the entry scene returns, the story is over and control goes back to the system. The system
then decides what comes next, such as rolling the credits or returning to the title, and that
usually depends on which ending the player reached. `exit` is how the story tells it.

`exit="ending"` means: when the entry scene returns, read the variable `ending` from the game state
and pass its value to the system's [`<on-exit>`](./system.md#game-mode-and-on-exit) handler. Only
the value crosses over. The system declares no variable for it and cannot see `ending` itself.

`ending` is an ordinary story variable, declared in `<game>` like any other. It can be a global,
or a local of the entry scene, since the value is read as that scene returns. It must be an enum,
because `<on-exit>` matches on the value exhaustively.

A complete program shows where each part lives:

```xml
<kagari>
  <system start="title">
    <game-mode name="game">
      ...
      <!-- 4. Receives the value of `ending` and matches on it. -->
      <on-exit>
        <case val="AzusaEnd"><meta-set var="cleared_azusa" val="true" /><goto page="credits" /></case>
        <case val="NormalEnd"><goto page="credits" /></case>
        <case val="Uninitialized"><goto page="title" /></case>
      </on-exit>
    </game-mode>
  </system>

  <game>
    <!-- 1. An ordinary story variable. No ending is reached yet. -->
    <declare-enum name="Ending">
      <enum name="Uninitialized" repr="0" />
      <enum name="NormalEnd"     repr="1" />
      <enum name="AzusaEnd"      repr="2" />
    </declare-enum>
    <var-enum name="ending" type="Ending" init="Uninitialized" />

    <!-- 2. The story sets it when it reaches an ending. -->
    <scene name="main">
      ...
      <set var="ending" val="AzusaEnd" />
    </scene>

    <!-- 3. When `main` returns, the value of `ending` goes to `<on-exit>`. -->
    <entry name="main" scene="main" exit="ending" />
  </game>
</kagari>
```

The variable starts at `Uninitialized`, not at one of the endings. With `init="NormalEnd"`, a path
that forgets its `<set>` would silently report the normal ending. With `Uninitialized`, that path
reports `Uninitialized`, and because `<on-exit>` must be exhaustive, the system has to say what
happens in that case.

This is the same pattern a scene uses to report to its caller (see [`<call>`](#call-enter-a-scene)),
with the system as the caller and `<on-exit>` in place of the `<match>`.

- `exit` must name an enum variable visible in the entry scene. A variable of another type is
  rejected.
- The value is read when the entry scene returns, whether by reaching its end or by `<return>`.
- `<on-exit>` must cover every variant of the enum. Adding a variant without handling it in
  `<on-exit>` is a compile error.
- Several entries may name the same variable, as `main` and `after_story` do in the first example.

## `<return>`: leave the scene early

```xml
<return />
```

*Tentative.* Returns from the current scene immediately, exactly as if execution had reached its
end. It is an instant element, not a step, and may appear anywhere in a scene body.

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
