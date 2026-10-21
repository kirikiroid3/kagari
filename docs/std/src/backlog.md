# Backlog

The backlog is an overlay page with `requires="game"`. It has no state of its own: everything it
shows is a function of the trace.

## History is derived from the trace

```text
History = [Entry]                       -- one entry per step occurrence, in order
Entry   = { index: u64, step: StepId, arm: Option<ArmId> }

history : &Narrative -> History         -- replay of the trace, without renders
```

Entries are keyed by `index`, the position in the run, not by `StepId`, because loops and repeated
calls visit the same step more than once. Since history is derived from the trace, a loaded save
gets its full backlog for free. The engine may cache it, but it is never extra saved state.

## Lines

What a step shows is written with `<m>` elements inside its `<s>` (syntax book, *Steps and
Labels*): each message has a speaker name, its text, and an optional voice line. Text outside
`<m>` is a comment and never reaches the player.

### A closed type for lines

Every `<s>` is known at compile time, so the compiler emits a static table from step to line:

```text
line      : StepId -> Line              -- compile-time table, no runtime data
Line      = [Message]                   -- the <m> elements of the step, in order
Message   = { who: TextId, voice: Option<VoiceId>, text: TextId }   -- empty who: narration

VoiceId   = a closed enum, generated from every voice="…" in the program
```

`current_line` and each backlog entry are lookups in this table. `who` is display text like
`text`. Every `voice` must name a `VoiceAsset` constant, which the compiler checks.

Playing a voice from the backlog is the system operation `metafn__play_voice(entry.voice)`. It
only uses the audio system and applies the voice volume setting. The narrative is not involved.

This holds only while text is free of story variables. If text ever interpolated a variable, such
as a player name, `line` would no longer be a function of `StepId`, and each entry would have to
carry the interpolated values. See [Open Questions](./open-questions.md#text-interpolation).

## Revert to here

```text
metafn__revert(entry.index) :  game := rewind(game, i);  interface__return   -- requires game; the top is the backlog
```

`metafn__revert(entry.index)` restores the trace prefix up to entry `i` and closes the backlog.

- The target is a prefix of the current trace, so it is reachable by construction.
- It replays from the nearest choice snapshot before `i` and runs renders from the last keyframe
  on the path, so it costs no more than a load.
- Reverting to a `<choice>` entry lands on the choice, and the player selects again.
- Meta is not reverted: read flags and unlocks written after entry `i` stay written. Revert stays
  deterministic because the narrative reads meta only at `start`, when it is copied into σ (see
  [Reading meta](./settings-meta.md#reading-meta-from-the-narrative)).

## The backlog page

A backlog page repeats over the history with `<for-history>` and shows each entry's line with
`<backlog-line>`; the syntax book's *System Script* chapter has a complete example. Its buttons
use the two operations above: `metafn__play_voice(entry.voice)` and `metafn__revert(entry.index)`. A voice button
for an entry without a voice is not shown.
