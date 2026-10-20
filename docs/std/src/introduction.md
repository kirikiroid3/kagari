# Introduction

This book describes the Kagari standard library and the **game system** built on it: the title
screen, settings, save and load, backlog, and the in-game UI around the story.

The *Kagari Language Syntax Reference* (the syntax book, `docs/syntax`) describes the story
itself: `.kis` scenes, `.kirp` render functions and `.kix` extensions. This book describes
everything around the story, and the standard `.kix` files through which the two talk to each
other.

| File                     | Scope  | Provides                                                       |
| ------------------------ | ------ | -------------------------------------------------------------- |
| `std/ui.kix`             | system | UI elements, layout and style types                            |
| `std/system-setting.kix` | system | Setting and meta declarations, and their types                 |
| `std/narrative.kix`      | system | The abstract `Narrative` type: lines, history, save and load   |
| `std/system-ctx.kix`     | game   | Deferred references to settings, and the `Hud` component      |

## Design constraints

The game system is designed from these facts:

1. A pushdown automaton is powerful enough to implement the game system.
2. The game system has persistent state (such as the volume setting) and ephemeral state (such
   as the current screen).
3. The game system always starts at the same position, the start screen.
4. The game system performs meta operations on the narrative state: save, load, and reading the
   step history.
5. When loading a save, the game system can jump to any position the player can reach.
6. A save contains only game progress, never system settings. Settings are global.

The narrative model is fixed by the engine proposal (*An Analyzable Model for a Visual Novel
Engine*), which is locked. Nothing in the game system may change what the narrative analyses
see: system state never feeds narrative control flow.

## Conventions

The conventions are those of the syntax book.

- *Tentative* marks syntax or rules that are proposed but not settled. Each tentative item is
  either harmless to rename or listed in [Open Questions](./open-questions.md).
- Code blocks tagged `xml` are `.kis` script, and blocks tagged `kix` are `.kix` definitions.
  Blocks tagged `text` are semantic notation, not source code.
- "Step" always means a point where the narrative waits for player input. "Mode" means a screen
  of the game system (see [Modes](./modes.md)).
