# System Script

`<system>` holds the **game system** around the story: settings, meta, save slots, and the modes
(title screen, save and load pages, config, backlog, the in-game UI) with their UI. This chapter
gives the syntax. What the elements mean at runtime (the mode stack, the operations, the checks)
is described in the *Kagari STD Book*.

All element and attribute names in this chapter are *tentative*, except `<system>` itself and the
form of interaction attributes: an attribute such as `on-click` holds one operation call, as in
`on-click="push(save)"`. The `start` attribute of `<system>` is tentative.

## `<system>`

```xml
<system start="title">
  <import src="std/ui.kix" />
  <import src="std/system-setting.kix" />
  <import src="std/narrative.kix" />
  <!-- settings, meta, save slots, modes -->
</system>
```

`start` names the root page the system starts on.

`<system>` may contain:

- `<import>` and `<include>`, scoped to the system (see
  [Templates, Include and Import](./reuse.md)). An included file has the root `<kagari-system>`.
- Settings, meta fields and save slots.
- Modes: `<page>`, `<dialog>` and `<game-mode>`.
- *Tentative:* ambient layers, UI drawn above every mode, such as a notification toast. They are
  the only UI outside the mode stack, so they should be rare.

## Settings, meta and save slots

```xml
<system start="title">
  <setting-int  name="bgm_volume"   min="0" max="100" init="80" />
  <setting-int  name="voice_volume" min="0" max="100" init="80" />
  <setting-enum name="text_speed"   type="Speed" init="Normal" />

  <meta-bool name="cleared_azusa" init="false" />

  <save-slots count="60" />
</system>
```

| Element          | Attributes                     | Declares |
| ---------------- | ------------------------------ | -------- |
| `<setting-int>`  | `name`, `min`, `max`, `init`   | An integer setting from `min` to `max` |
| `<setting-enum>` | `name`, `type`, `init`         | A setting of the enum `type` |
| `<meta-bool>`    | `name`, `init`                 | A `bool` meta field |
| `<meta-char>`    | `name`, `init`                 | A `char` meta field |
| `<meta-int>`     | `name`, `min`, `max`, `init`   | An `int` meta field from `min` to `max` |
| `<meta-enum>`    | `name`, `type`, `init`         | A meta field of the enum `type` |
| `<save-slots>`   | `count`                        | The number of save slots |

All attributes are required. `init` is the value used when the stored file has no value for the
field, for example on first boot or after an update adds a field. `<save-slots count>` fixes the
number of slots, so the slot list in the UI is finite.

The game writes meta fields with [`<meta-set>`](./data.md#meta-set-write-a-meta-field) and reads
them only through [`from-meta`](./data.md#from-meta-copy-a-meta-field-at-start).

## Modes

```xml
<system start="title">
  <page name="title" root="true">
    <button on-click="start(main)">New Game</button>
    <button on-click="push(load)">Load</button>
    <button on-click="push(config)">Config</button>
    <button on-click="ask_quit">Quit</button>
  </page>

  <game-mode name="game">
    <game-screen />
    <layer name="text">
      <text-frame on-click="advance" />
    </layer>
    <layer name="quick">
      <button on-click="push(save)">Save</button>
      <button on-click="push(load)">Load</button>
      <button on-click="push(backlog)">Log</button>
    </layer>
    <on-exit>
      <case val="AzusaEnd"><meta-set var="cleared_azusa" val="true" /><goto page="credits" /></case>
      <case val="_"><goto page="title" /></case>
    </on-exit>
  </game-mode>

  <page name="save" requires="game">
    <for-slots>
      <button on-click="push(confirm_overwrite)">…</button>
    </for-slots>
    <button on-click="pop">Back</button>
  </page>

  <dialog name="confirm_overwrite" requires="game">…</dialog>
</system>
```

| Element         | Attributes                 | Meaning |
| --------------- | -------------------------- | ------- |
| `<page>`        | `name`, `root`, `requires` | A mode. `root="true"` makes it a root page; otherwise it is an overlay |
| `<dialog>`      | `name`, `requires`         | An overlay page drawn over a still image of the frames below |
| `<game-mode>`   | `name`                     | The mode that runs the story. One per program |
| `<layer>`       | `name`                     | A named part of a mode's view |
| `<game-screen>` | –                          | The story's screen, drawn as a layer of `<game-mode>` |

`requires="game"` marks an overlay that may only be pushed above a running game, such as Save or
Backlog.

### `<game-mode>` and `<on-exit>`

`<on-exit>` runs when the story's entry scene returns. It receives the `Ending` value the
[entry scene](./control-flow.md#entry-scenes) returns and matches on it. Each `<case val>` holds a
pattern for that value, written as in [`<match>`](./control-flow.md#match-branch-on-variables), and
the operations to run. The cases must be exhaustive.

## UI elements

System UI is markup inside the modes: buttons, sliders, text, layout containers. Their types and
styles come from `std/ui.kix`.

```xml
<button style="common_btn" on-click="push(save)">Save</button>
<slider bind="bgm_volume" />
```

| Element                                 | Meaning |
| --------------------------------------- | ------- |
| `<button>`                              | A clickable element |
| `<slider>`, `<toggle>`, `<select>`      | Controls bound to a setting |
| `<row>`, `<col>`                        | Flex layout containers |
| `<text-frame>`                          | The current line of the story |
| `<backlog-line>`                        | The line of a history entry, inside `<for-history>` |

| Attribute  | On                                 | Meaning |
| ---------- | ---------------------------------- | ------- |
| `on-click` | any interactive element            | Runs one operation |
| `bind`     | `<slider>`, `<toggle>`, `<select>` | Reads a setting and writes it with `set` |
| `style`    | any UI element                     | Names a function of the system scope that returns the element's style |

An interaction attribute holds exactly one operation call, written as a call:
`on-click="push(save)"`. The operations (`push`, `pop`, `goto`, `start`, `load`, `save`,
`advance`, `select`, `revert`, `set`, `play_voice`, `quit`, `ask_quit`) are defined in the
*Kagari STD Book*. *Tentative:* the attribute names other than `on-click`.

### Repetition

The UI repeats over finite collections only:

| Element         | Repeats its body once per            | Binds |
| --------------- | ------------------------------------ | ----- |
| `<for-slots>`   | save slot, up to `<save-slots count>` | –     |
| `<for-history>` | history entry of the story           | `entry` |
| `<for-choices>` | arm offered at the current `<choice>` | –     |

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

Inside `<for-history>`, `entry` is the current history entry: `entry.index` is its position in
the run, and `entry.voice` is the voice of its line. A voice button for an entry without a voice
is not shown.
