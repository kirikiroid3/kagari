# Enums and Variables

Every variable has a finite type: `bool`, an enum declared in the script, or `char`. There are no
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
<var-char name="affection_azusa" init="0" />
```

| Element      | Type                       | Attributes                                    |
| ------------ | -------------------------- | --------------------------------------------- |
| `<var-bool>` | `bool`                     | `name`, `init`                                |
| `<var-enum>` | an enum from `<declare-enum>` | `name`, `type`, `init`                     |
| `<var-char>` | `char`: unsigned 8-bit, 0–255 | `name`, `init`                             |

`char` is a number, not a character: it is the script's only integer type, and it is exactly
`u8`.

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

The value is checked against the variable's type at compile time.

`<set>` only assigns literals. Whether it should also support arithmetic (for example
incrementing an affection counter) is an [open question](../open-questions.md#arithmetic-on-char).
