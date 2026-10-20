# Introduction

This book describes the source languages of the Kagari visual novel engine.

A Kagari game is written in three kinds of source files:

| Extension | Name           | Syntax    | Purpose                                                   |
| --------- | -------------- | --------- | --------------------------------------------------------- |
| `.kis`    | Game script    | XML       | The story (steps, choices, scenes, variables, control flow) and the game system around it |
| `.kirp`   | Representing   | Rust-like | How the screen changes: `fn render(Screen) -> Screen`     |
| `.kix`    | Extension      | Rust-like | Definitions only: types, methods, functions, constants    |

All three are compiled together into a single `.kagari` bytecode file.

The design follows the model in the engine proposal (*An Analyzable Model for a Visual Novel
Engine*): narrative control is a pushdown system over finite data, and presentation is a function
that the narrative never reads back. The surface syntax differs from the one the proposal
sketches; [Relation to the Proposal](./proposal.md) explains what changed and why.

## Conventions

- *Tentative* marks syntax that this book fills in where the design is not settled yet: attribute
  names, defaults, and edge-case rules. Each tentative item is either harmless to rename or is
  listed in [Open Questions](./open-questions.md).
- Code blocks tagged `xml` are `.kis` script; blocks tagged `kirp` and `kix` are the Rust-like
  languages.
- "Step" always means a point where the engine waits for player input.
