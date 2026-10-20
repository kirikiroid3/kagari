# Game Script (`.kis`)

A `.kis` file is an XML document describing the story as scenes made of steps.

## Document structure

*Tentative:* the root element is `<kagari>`. It has exactly two children, one `<system>` and one
`<game>`:

```xml
<kagari>
  <system start="title">
    <!-- settings, meta, save slots, modes and their UI -->
  </system>

  <game>
    <import src="characters.kix" />
    <include src="common/templates.kis" />

    <entry name="main" scene="prologue" />

    <declare-enum name="Route">...</declare-enum>
    <var-enum name="route" type="Route" init="None" />

    <template name="...">...</template>

    <scene name="prologue">...</scene>
    <scene name="festival_hub">...</scene>
  </game>
</kagari>
```

| Element    | Contains                                                | See |
| ---------- | ------------------------------------------------------- | --- |
| `<system>` | The game system: settings, meta, save slots, modes and their UI | [System Script](./system.md) |
| `<game>`   | The story                                               | this chapter and the next ones |

`<game>` is a module boundary. Names declared inside it are invisible to the system, except
through what `<entry>` exports and the standard library's narrative interface. In the other
direction, game code sees nothing of the system except the standard library's system context. The
*Kagari STD Book* describes both interfaces.

In this book, **top level** means a direct child of `<game>`, or of the root of a file included
there (see [`<include>`](./reuse.md#include-splice-another-script)). The top-level children are:

| Top-level child                          | Meaning                       |
| ---------------------------------------- | ----------------------------- |
| `<import>`                               | Use a `.kix` file             |
| `<include>`                              | Splice another `.kis` file    |
| `<entry>`                                | Export a starting scene to the system |
| `<declare-enum>`                         | Enum type                     |
| `<var-bool>`, `<var-enum>`, `<var-char>`, `<var-int>` | Global variable |
| `<template>`                             | Reusable script block (macro) |
| `<scene>`                                | Scene                         |

Story content (steps, choices, control flow) only appears inside a `<scene>`.

## Execution model

The engine runs the script as a pushdown machine:

- The **position** is a scene plus a step within it.
- The **call stack** holds one frame per active `<call>`: the return position and that scene's
  local variables.
- The **variables** have finite types only: `bool`, enums, `char` (an 8-bit unsigned
  integer) and *(tentative)* `int` in a declared range.

Elements fall into two groups:

| Kind                   | Elements                                            | Waits for input       |
| ---------------------- | --------------------------------------------------- | --------------------- |
| Steps                  | `<s>`, `<choice>`                                   | Yes                   |
| Instant (not a step)   | `<label>`, `<goto>`, `<call>`, `<return>`, `<match>`, `<set>`, `<add>`, `<sub>`, `<meta-set>` | No |

When the engine enters a step it runs the step's representation, shows it, and waits. Input
moves it on: `NextStep` for `<s>`, an arm selection for `<choice>`. Everything between two steps
executes instantly. Because data is finite and every loop and every cycle of calls must pass
through a step (checked by the compiler), the code between two inputs always terminates, and questions like "can this step
be reached with `met_b` false?" are decidable.

Presentation is one-way: the script triggers representation, but no script element can read the
screen.

## XML notes

- Text inside elements is ordinary XML text: `&lt;`, `&amp;` and friends apply.
- Code inside `<r>` contains `<`, `>` and `&` often. Wrap it in a CDATA section when it does:
  `<r><![CDATA[ ... ]]></r>`.
- Attribute values are always strings in XML; the compiler converts them to the declared type
  of the variable or enum they refer to.

## Example

The festival hub from the proposal: stalls visited from a map, each stall remembering whether it
was visited. Scenes `stall_b` and `dorm_night` are omitted.

```xml
<kagari>
  <system start="title" />

  <game>
    <import src="characters.kix" />

    <entry name="main" scene="day1" />

    <var-bool name="visited_a" init="false" />
    <var-bool name="visited_b" init="false" />
    <var-bool name="visited_c" init="false" />

    <scene name="day1">
      <s><m who="">The school festival starts today.</m></s>
      <call scene="festival_hub" />
      <call scene="dorm_night" />
    </scene>

    <scene name="festival_hub">
      <label id="map" />
      <choice id="map">
        <choice-arm id="a" text="Go to stall A">
          <call scene="stall_a" />
          <goto label="map" />
        </choice-arm>
        <choice-arm id="b" text="Go to stall B">
          <call scene="stall_b" />
          <goto label="map" />
        </choice-arm>
        <match vars="visited_a visited_b">
          <cases>
            <case val="true true" />
            <do>
              <choice-arm id="leave" text="Leave" />
            </do>
          </cases>
          <cases>
            <case val="_ _" />
            <do />
          </cases>
        </match>
      </choice>
    </scene>

    <scene name="stall_a">
      <match var="visited_a">
        <cases>
          <case val="true" />
          <do>
            <s><m who="">The stall is already packing up.</m></s>
            <return />
          </do>
        </cases>
        <cases>
          <case val="false" />
          <do />
        </cases>
      </match>
      <set var="visited_a" val="true" />
      <s>
        <r>screen.entity(HANAKO).cord.apply_animation(jump(20));</r>
        <m who="Hanako">Welcome to stall A!</m>
      </s>
    </scene>

    <!-- stall_b, dorm_night: omitted -->
  </game>
</kagari>
```

Reaching the end of a scene returns to the caller, so the `Leave` arm needs no body: control
falls past the `<choice>`, off the end of `festival_hub`, and back into `day1`. The `<match>`
inside the `<choice>` offers `Leave` only after both stalls were visited (see
[conditional arms](./steps.md#conditional-arms)).
