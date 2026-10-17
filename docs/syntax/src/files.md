# Source Files

## `.kis`: game script

The story. An XML document made of scenes, steps, choices, variables and control flow. `.kis` is
the entry point of compilation: everything else is reached from a `.kis` file.

See [Game Script](./kis/overview.md).

## `.kirp`: representing code

How the screen changes when a step is entered. The syntax is Rust-like, and a `.kirp` file must
define exactly one function:

```kirp
fn render(screen: Screen) -> Screen {
    // ...
    screen
}
```

`render` receives the screen as it was before the step and returns the screen after it. The
`<r>` tag in a script is the same thing written inline.

See [Representing Code](./kirp.md).

## `.kix`: extension

The vocabulary that `.kirp` code is written in: types, methods, functions and constants. The
syntax is the same as `.kirp`, but a `.kix` file works like a C header: it only *declares*. It
never contains a `render` function or any statement that runs.

The engine ships basic presets (entity handles, layers, common animations) as `.kix`
declarations; a studio extends the engine by writing its own `.kix` files backed by its own
implementation.

See [Extensions](./kix.md).

## How they reference each other

| From   | Tag                     | To      | Effect                                              |
| ------ | ----------------------- | ------- | --------------------------------------------------- |
| `.kis` | `<include src="…" />`   | `.kis`  | Textual insertion of the other script               |
| `.kis` | `<import src="…" />`    | `.kix`  | Makes the declarations visible; hoisted             |
| `.kis` | `<represent src="…" />` | `.kirp` | Runs that file's `render` when the step is entered |

`.kirp` and `.kix` files never reference other files themselves. Because `<import>` is hoisted
to the whole program, a `.kirp` file sees every `.kix` declaration imported anywhere in it.
