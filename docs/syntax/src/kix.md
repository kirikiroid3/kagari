# Extensions (`.kix`)

A `.kix` file defines the vocabulary that representing code is written in: the data the engine
works with, and functions over that data. It uses the same syntax as `.kirp`, but it never
contains a `render` function and never runs on its own; it is only ever used by representing
code.

| Allowed                                              | Not allowed                                  |
| ---------------------------------------------------- | -------------------------------------------- |
| Type definitions (`struct`, `type`)                  | `render`                                     |
| Functions and methods, with bodies (`fn`, `impl`)    | Top-level statements                         |
| Constants with their values                          | References to other files                    |
| Modules grouping constants (`mod`)                   | Reading script variables, I/O, clocks, randomness |

## Types describe data

A `struct` in `.kix` works like a message in a protobuf schema: it tells the engine what the data
is. Components of the [screen](./kirp.md#the-screen-is-an-ecs-world) are such structs. Every
component type must implement `Eq`, so that the compiler can compare screens.

The engine's systems, which process entities at runtime (for example advancing an animation frame
by frame), are part of the engine. They are not written in `.kix` or `.kirp`.

## Functions have bodies

Every function and method in a `.kix` file has a body. `.kix` is not a header: functions are
there because they are useful, for example to build an animation value from a few parameters.
The one exception is a function marked `#[builtin]`, which the engine implements; it is only
allowed in the bundled standard library.

Since representing code is evaluated at compile time, so is every `.kix` function it calls.
`.kix` functions are therefore held to the same rules as `render`: they must be **total** and
**deterministic**, and they cannot read script variables.

## The standard library and studio extensions

- The engine ships a **standard library** of `.kix` files, imported as `std/<name>.kix`. Its
  prelude is visible in the game scope without an import and provides the screen access API, the
  common components and basic animations such as `jump` or `instant_switch`. The *Kagari STD
  Book* describes every standard file.
- A **studio** that needs more (its own animation curves, effects, component kinds) writes its own
  `.kix` file, without changing the engine or the compiler. The compiler type-checks all
  representing code against these definitions, so studio extensions get the same checking as the
  standard library.

## Example

The examples in this book use the prelude of the standard library (`Screen`, `jump`,
`instant_switch`, `fade`, …) and these studio definitions:

```kix
// characters.kix — written by the studio

const HANAKO: EntityId = 216;
const AZUSA: EntityId = 217;

mod AZUSA {
    const CLOSE_EYE_SMILE: ImageId = 4001;
}

mod BG {
    const ROOF: ImageId = 9001;
    const CLASSROOM: ImageId = 9002;
}
```

`AZUSA` names both a constant and a module, as in Rust: constants live in the value namespace,
modules in the type namespace, so `AZUSA` is an `EntityId` and `AZUSA::CLOSE_EYE_SMILE` is one of
her images.

How an `ImageId` is bound to an actual image file is an
[open question](./open-questions.md#asset-binding).
