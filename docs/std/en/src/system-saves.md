# `std::system::saves`

> **Stability:** Stable — save slots and their metadata.

`std::system::saves` names the save slots and reads and deletes them. Writing a save is
`Narrative::save(slot)` (see [`std::narrative`](./narrative.md)), because only a running game can
be saved; loading one is [`nav::load(slot)`](./system-nav.md#load). The screenshot of a slot is
the element [`ui::thumbnail(slot)`](./ui.md#thumbnail).

A save records the path the player took through the story; it stays loadable as long as the
script still allows that path. See the Reference chapter *Saves, Replay and Determinism* for the
model.

```kis
use std::system::saves;
use std::system::saves::SaveSlot;
```

## Overview {#overview}

| Item | Kind | Summary |
| ---- | ---- | ------- |
| [`SaveSlot`](#saveslot) | enum | A manual, quick or autosave slot |
| [`Timestamp`](#timestamp) | struct | A local date and time |
| [`SaveInfo`](#saveinfo) | struct | What a slot shows |
| [`Divergence`](#divergence) | builtin struct | Why a save no longer fits the program |
| [`info(slot)`](#info) | `fn` | The slot's metadata; reactive |
| [`delete(slot)`](#delete) | `fn` | Empties a slot |

## Enum: `SaveSlot` {#saveslot}

```kis
pub enum SaveSlot {
    /// Manual slot `k`, below `System::save_slots`.
    Manual(u32),
    /// The quick slot.
    Quick,
    /// Autosave slot `k`, below `System::auto_slots`.
    Auto(u32),
}
```

| Variant | Meaning |
| ------- | ------- |
| `Manual(k)` | A manual slot, `0 <= k < System::save_slots` |
| `Quick` | The single quick slot, written and read by `Action::QuickSave` and `Action::QuickLoad` |
| `Auto(k)` | An autosave slot, `0 <= k < System::auto_slots` |

- A `Manual(k)` with `k >= save_slots`, or an `Auto(k)` with `k >= auto_slots`, is out of range.
  Using it is a runtime no-op: [`info`](#info) returns `None`, and `save` does nothing.
- **Autosave.** At every choice step entry and on quit, the engine writes `Auto(k)`, rotating
  through `k` in `0..auto_slots`.

## Struct: `Timestamp` {#timestamp}

```kis
pub struct Timestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}
```

A local date and time.

## Struct: `SaveInfo` {#saveinfo}

```kis
pub struct SaveInfo {
    pub time: Timestamp,
    pub who: Option<Text>,
    pub text: Option<Text>,
}
```

What a slot shows: when it was saved and the line on screen at that time.

- `time` — when the save was written, in local time.
- `who` — the speaker of the line at save time, if any.
- `text` — the text of the line at save time, if a line was shown.

## Struct: `Divergence` {#divergence}

```kis
#[builtin] pub struct Divergence;
```

Why a save no longer fits the program: the script changed so that the path recorded in the save
is no longer a path of the story. Returned in the `Err` of [`nav::load`](./system-nav.md#load).

### `Divergence::describe()` {#divergence-describe}

```kis
#[builtin] pub fn describe(&self) -> Text;
```

- Returns: `Text` — a description of where the save no longer fits the script, for the player.
- Kind: `fn` · Builtin: yes

## Function: `info(slot)` {#info}

```kis
#[builtin] pub fn info(slot: SaveSlot) -> Option<SaveInfo>;
```

- `slot` `SaveSlot` — the slot.
- Returns: `Option<SaveInfo>` — the slot's metadata, or `None` if the slot is empty or out of
  range.
- Kind: `fn` · Builtin: yes

`info` is a reactive source: a slot list re-renders when a save is written or deleted.

```kix
use std::system::saves;
use std::system::saves::SaveSlot;

fn slot_text(slot: SaveSlot) -> Text {
    match saves::info(slot) {
        Some(info) => match info.text {
            Some(text) => text,
            None => "",
        },
        None => "Empty",
    }
}
```

## Function: `delete(slot)` {#delete}

```kis
#[builtin] pub fn delete(slot: SaveSlot);
```

- `slot` `SaveSlot` — the slot to empty.
- Kind: `fn` · Builtin: yes

Empties the slot.

```kix
button!{on_click = || saves::delete(SaveSlot::Manual(0)), t!["Delete"]}
```

A full save and load page is in [System Recipes](./guides/system.md#save-and-load).
