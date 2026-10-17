# Scenes and Control Flow

Control flow has three forms: `<call>` between scenes, `<goto>` within a scene, and `<match>` for
branching on a variable. There is no `if`.

## `<scene>`: scene declaration

```xml
<scene name="festival_hub">
  ...
</scene>
```

A scene is a named sequence of script content. Scenes are the unit of calling: the call stack
holds one frame per active scene.

- `name` is required and unique in the program.
- *Tentative:* the program starts at the scene named `main`.
- Scenes may contain local variable declarations (see [Enums and Variables](./data.md)).
- Scenes do not nest.

When execution reaches the end of a scene, the scene returns: its frame is popped and execution
continues after the `<call>` that entered it. There is no `<return>` element; to leave early,
`<goto>` a label placed at the end of the scene.

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

## `<goto>`: jump within the scene

```xml
<goto label="map" />
```

Continues at the named `<label>` of the current scene. A `<goto>` cannot leave its scene: jumping
into the middle of another scene would break call/return nesting and make the program
unanalyzable. Use `<call>` to move between scenes.

A loop built from `<goto>` must pass through at least one step. A loop without a step would never
wait for input and might never terminate; the compiler rejects it.

## `<match>`: branch on a variable

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

`<match>` reads one variable and runs the body of the first `<cases>` block that matches its
value. After the body, execution continues after `</match>`. `<match>` is not a step.

| Element   | Contains                                  | Attribute | Meaning                         |
| --------- | ----------------------------------------- | --------- | ------------------------------- |
| `<match>` | one or more `<cases>`                     | `var`     | Variable to inspect             |
| `<cases>` | one or more `<case>`, then one `<do>`     |           | One arm of the match            |
| `<case>`  | nothing                                   | `val`     | A value that selects this arm   |
| `<do>`    | script content                            |           | Body run when the arm is chosen |

Several `<case>` elements in one `<cases>` are alternatives: the arm runs if the variable equals
any of them. `<do>` may be empty.

`val` is written as the value would be written in [`<set>`](./data.md#set-assign-a-variable):
a variant name for enums, `true`/`false` for `bool`, a decimal number for `char`.

### Exhaustiveness

Every possible value of the variable must be covered by some `<case>`. The compiler rejects a
`<match>` that misses one. For `bool` this means both `true` and `false`; for an enum, every
variant.

*Tentative:* `<case val="_" />` matches any value. It is required in practice for `char`, which has
256 values, and must be the last `<cases>`.

### Why no `if`

A boolean is an enum with two variants, so `<match>` covers everything `if` would. Requiring every
case to be listed makes the author decide what happens on each branch, which is also what the
compiler needs in order to check presentation after branches merge.

Branch on several variables by nesting `<match>`.
