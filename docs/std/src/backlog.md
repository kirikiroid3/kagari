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

## `<m>`: speaker and voice

```xml
<s><m who="Azusa" voice="azusa_0072_nux">ぬっ</m></s>
<s>The wind stopped.</s>
```

*Tentative.* `<m>` marks a message inside an `<s>`: who says it, and an optional voice line. Text
outside `<m>` is narration. This settles part of the syntax book's open question on text markup.

| Attribute | Required | Meaning                                         |
| --------- | -------- | ----------------------------------------------- |
| `who`     | yes      | A declared character                            |
| `voice`   | no       | A voice id                                      |

The attribute is `who`, not `char`, because the syntax book already uses `char` for the unsigned
8-bit variable type.

### A closed type for lines

Every `<s>` is known at compile time, so the compiler emits a static table from step to line:

```text
line      : StepId -> Line              -- compile-time table, no runtime data
Line      = [Segment]
Segment   = Narration(TextId)
          | Message { who: Character, voice: Option<VoiceId>, text: TextId }

Character = a closed enum, from the character declarations in <game>
VoiceId   = a closed enum, generated from every voice="…" in the program
```

`current_line` and each backlog entry are lookups in this table. Compile checks: `who` names a
declared character, and every `voice` resolves to an asset (see the syntax book's open question on
asset binding).

Playing a voice from the backlog is the system operation `play_voice(v)`. It only uses the audio
system and applies the voice volume setting. The narrative is not involved.

This holds only while text is free of story variables. If text ever interpolated a variable, such
as a player name, `line` would no longer be a function of `StepId`, and each entry would have to
carry the interpolated values. See [Open Questions](./open-questions.md#text-interpolation).

## Revert to here

```text
revert(i) :  game := rewind(game, i);  pop        -- requires game; the top is the backlog
```

`revert(i)` restores the trace prefix up to entry `i` and closes the backlog.

- The target is a prefix of the current trace, so it is reachable by construction.
- It replays from the nearest choice snapshot before `i` and runs renders from the last keyframe
  on the path, so it costs no more than a load.
- Reverting to a `<choice>` entry lands on the choice, and the player selects again.
- Meta is not reverted: read flags and unlocks written after entry `i` stay written. Revert stays
  deterministic because the narrative reads meta only at `start`, when it is copied into σ (see
  [Reading meta](./settings-meta.md#reading-meta-from-the-narrative)).

## Example

```xml
<page name="backlog" requires="game">
  <for-history>
    <row>
      <backlog-line />
      <button on-click="play_voice(entry.voice)">▶</button>
      <button on-click="revert(entry.index)">Revert</button>
    </row>
  </for-history>
  <button on-click="pop">Back</button>
</page>
```

*Tentative* element names. `<for-history>` repeats its body once per history entry, with `entry`
bound to it. `entry.voice` is the voice of the entry's line, looked up in the `line` table. A voice button for an entry without a voice is not shown.
