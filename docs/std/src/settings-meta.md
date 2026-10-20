# Settings and Meta (`std/system-setting.kix`)

Both settings and meta are persistent and global: they are stored outside saves and survive across
runs. They differ in who may touch them.

| Store    | Examples                                     | Written by            | Read by |
| -------- | -------------------------------------------- | --------------------- | ------- |
| Settings | volumes, text speed, window mode, language   | the system (`set`)    | the system; the game only as [`Setting<T>`](./system-ctx.md) |
| Meta     | cleared endings, gallery unlocks, read flags | the system, the narrative (write-only), the engine | the system; the narrative only by opt-in at `start` |

## Declarations

Settings, meta fields and save slots are declared in `<system>` with `<setting-*>`, `<meta-*>` and
`<save-slots>`; the syntax book's *System Script* chapter gives the elements. Every setting and
meta field declares an initial value, used when the stored file has no value for it, for example
on first boot or after an update adds a field.

Settings are never part of an analysis, so they may in principle have richer types than story
variables, such as floats or strings. *Tentative:* the standard declarations are bounded types
anyway, because UI controls (sliders, toggles, selectors) are bounded.

The number of save slots is fixed, so the slot list in the UI is finite.

## Writing meta from the narrative

The story writes a meta field with `<meta-set>` (syntax book, *Enums and Variables*). It is an
instant element, not a step. Meta is **write-only** from the narrative: a write that the narrative
can never read back cannot change narrative control flow, so every narrative analysis is
unaffected.

Meta written by the narrative is not reverted by load, rewind or revert. Read flags and unlocks
stay written. That is intended.

## Reading meta from the narrative

Some games make cross-playthrough state part of the story on purpose; the proposal's meta layer
covers them. *Tentative:* a story variable may be declared as a copy of a meta field, with the
`from-meta` attribute (syntax book, *Enums and Variables*).

- The value is copied from meta into σ once, at `start`. During the run it is an ordinary story
  variable.
- The analyzer treats it as a free initial variable, universally quantified over its domain at
  the start of the run, as the proposal's meta layer requires.
- Because the copy is in σ, it is in the save. Loading and rewinding replay with the copy, not
  with the current meta, so they stay deterministic even after meta changed.

Reading meta at any other point is not allowed, since that would make a replay depend on meta
written after the save.

## Read flags

Read-text flags are meta written by the engine on each `advance`, keyed by step id. The narrative
never sees them. The system reads them, for example for skip-read-text and for marking read lines
in the backlog.
