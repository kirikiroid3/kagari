# Enums and Variables

Every variable has a finite type: `bool`, an enum declared in the script, `char`, or an `int` in a
declared range. There are no
strings, lists, maps or unbounded integers in script variables. This is what keeps the script a
pushdown system with decidable reachability: two unbounded counters would already make it
Turing-complete.

## `<declare-enum>`: enum type

```xml
<declare-enum name="Route">
  <enum name="None"   repr="0" />
  <enum name="Hanako" repr="1" />
  <enum name="Azusa"  repr="2" />
  <enum name="Both"   repr="3" />
</declare-enum>
```

| Element          | Attribute | Required | Meaning                                         |
| ---------------- | --------- | -------- | ----------------------------------------------- |
| `<declare-enum>` | `name`    | yes      | Type name, unique in the program                |
| `<enum>`         | `name`    | yes      | Variant name, unique within the enum            |
| `<enum>`         | `repr`    | yes      | Integer representation, unique within the enum  |

`repr` is the value stored in bytecode and in save snapshots. Keeping `repr` fixed while renaming
or reordering variants keeps existing saves valid.

Enums are declared at top level.

## Variable declarations

```xml
<var-bool name="met_azusa" init="false" />
<var-enum name="route" type="Route" init="None" />
<var-char name="flags_seen" init="0" />
<var-int name="affection_azusa" min="0" max="5" init="0" />
```

| Element      | Type                       | Attributes                                    |
| ------------ | -------------------------- | --------------------------------------------- |
| `<var-bool>` | `bool`                     | `name`, `init`, `from-meta`                   |
| `<var-enum>` | an enum from `<declare-enum>` | `name`, `type`, `init`, `from-meta`        |
| `<var-char>` | `char`: unsigned 8-bit, 0–255 | `name`, `init`, `from-meta`                |
| `<var-int>`  | *Tentative.* `int` from `min` to `max` | `name`, `min`, `max`, `init`, `from-meta` |

`from-meta` is optional; see [`from-meta`](#from-meta-copy-a-meta-field-at-start).

`char` is a number, not a character: it is exactly `u8`. Prefer `<var-int>` with the smallest
range that fits, such as `0`–`5` for an affection counter: the analyzer represents each variable
with as many bits as its range needs, so a 6-value counter costs 3 bits where a `char` costs 8.

*Tentative:* `init` is required. With an initial value on every variable, a read can never see an
unset variable, and the read-before-write lint becomes unnecessary.

### Scope

- Declared directly under the root: **global**. Visible in every scene; lives for the whole run.
- *Tentative:* declared inside a `<scene>`: **local** to that scene. Created with `init` when the
  scene is called, stored in the scene's stack frame, discarded on return. A recursive call gets
  its own copy.

Variable names are unique within their scope, and a local may not shadow a global.

## `<set>`: assign a variable

```xml
<set var="met_azusa" val="true" />
<set var="route" val="Azusa" />
<set var="affection_azusa" val="3" />
```

Assigns a literal value. `<set>` is not a step.

| Variable type | `val` syntax                         |
| ------------- | ------------------------------------ |
| `bool`        | `true` or `false`                    |
| enum          | A variant name of the variable's enum |
| `char`        | A decimal number, 0–255             |
| `int`         | A decimal number from `min` to `max` |

The value is checked against the variable's type at compile time.

## `<add>` and `<sub>`: change a counter

```xml
<add var="affection_azusa" val="1" />
<sub var="affection_azusa" val="2" />
```

*Tentative.* Adds or subtracts a literal amount on a `char` or `int` variable. Neither is a
step. The result **saturates**: it stops at `max` (255 for `char`) going up and at `min` (0 for
`char`) going down, so every step stays total. When the analyzer can prove that a reachable
`<add>` or `<sub>` hits a bound, it reports a warning, since a counter that silently saturates is
often a sign that its range is too small.

## `<meta-set>`: write a meta field

```xml
<meta-set var="cleared_azusa" val="true" />
```

*Tentative.* Writes a literal value to a meta field declared in `<system>` (see
[meta declarations](./system.md#meta-fields)). `<meta-set>` is not a step. `val`
is written as for `<set>`, against the type of the meta field.

Meta is write-only from the story: no script element reads a meta field back, except
`from-meta` when the story starts. The *Kagari STD Book* explains why this keeps every narrative
analysis unaffected.

## `from-meta`: copy a meta field at start

```xml
<var-bool name="cleared_azusa" init="false" from-meta="cleared_azusa" />
```

*Tentative.* A variable declaration with `from-meta` is copied from the named meta field once,
when the story starts. During the run it is an ordinary variable. The meta field and the variable
must have the same type.
