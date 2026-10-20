# Tag Reference

"Scene body" means script content allowed inside `<scene>`, `<choice-arm>`, `<do>`, `<snip>` and
`<template>`: steps, instant elements and `<x>`. Local variable declarations are only allowed as
direct children of `<scene>`.

## Structure

"Top level" means a direct child of `<game>` or of a `<kagari-game>` root.

| Tag                    | Step | Allowed in                 | Attributes        | Children                         | See |
| ---------------------- | ---- | -------------------------- | ----------------- | -------------------------------- | --- |
| `<kagari>`             | –    | document root              | –                 | `<system>`, `<game>`             | [Overview](./overview.md#document-structure) |
| `<system>`             | –    | `<kagari>`                 | `start` *(tent.)* | system declarations and modes    | [System Script](./system.md#system) |
| `<game>`               | –    | `<kagari>`                 | –                 | top-level declarations           | [Overview](./overview.md#document-structure) |
| `<kagari-game>`        | –    | root of an included file   | –                 | top-level declarations           | [Reuse](./reuse.md#include-splice-another-script) |
| `<kagari-system>`      | –    | root of an included file   | –                 | as `<system>`                    | [Reuse](./reuse.md#include-splice-another-script) |
| `<include>`            | –    | `<game>`, `<system>`, included roots | `src`   | –                                | [Reuse](./reuse.md#include-splice-another-script) |
| `<import>`             | –    | `<game>`, `<system>`, included roots | `src`   | –                                | [Reuse](./reuse.md#import-use-extension-definitions) |
| `<scene>`              | –    | top level                  | `name`, `returns`, `entry` | scene body              | [Control Flow](./control-flow.md#scene-scene-declaration) |

## Steps

| Tag            | Step | Allowed in    | Attributes                     | Children                          | See |
| -------------- | ---- | ------------- | ------------------------------ | --------------------------------- | --- |
| `<s>`          | yes  | scene body    | –                              | `<m>`, `<r>`, `<represent>`; other text is a comment | [Steps](./steps.md#s-ordinary-step) |
| `<m>`          | –    | `<s>`         | `who`, `voice`                 | text                              | [Steps](./steps.md#m-message) |
| `<choice>`     | yes  | scene body    | `id`                           | `<choice-arm>`, `<match>`, `<r>`, `<represent>` | [Steps](./steps.md#choice-and-choice-arm-choice-step) |
| `<choice-arm>` | –    | `<choice>`, `<do>` in a choice | `id`, `text`  | scene body                        | [Steps](./steps.md#choice-and-choice-arm-choice-step) |

## Control flow

| Tag        | Step | Allowed in             | Attributes | Children                  | See |
| ---------- | ---- | ---------------------- | ---------- | ------------------------- | --- |
| `<label>`  | no   | scene body             | `id`       | –                         | [Steps](./steps.md#label-position-marker) |
| `<goto>`   | no   | scene body             | `label`    | –                         | [Control Flow](./control-flow.md#goto-jump-within-the-scene) |
| `<call>`   | no   | scene body             | `scene`, `assign` | –                  | [Control Flow](./control-flow.md#call-enter-a-scene) |
| `<return>` | no   | scene body             | `var` or `literal` | –                 | [Control Flow](./control-flow.md#return-leave-the-scene-early) |
| `<match>`  | no   | scene body, `<choice>` | `var` or `vars` | `<cases>`            | [Control Flow](./control-flow.md#match-branch-on-variables) |
| `<cases>`  | –    | `<match>`              | –          | `<case>`…, then `<do>`    | [Control Flow](./control-flow.md#match-branch-on-variables) |
| `<case>`   | –    | `<cases>`              | `val`      | –                         | [Control Flow](./control-flow.md#match-branch-on-variables) |
| `<do>`     | –    | `<cases>`              | –          | scene body; only `<choice-arm>` inside `<choice>` | [Control Flow](./control-flow.md#match-branch-on-variables) |

## Data

| Tag              | Step | Allowed in                 | Attributes             | Children  | See |
| ---------------- | ---- | -------------------------- | ---------------------- | --------- | --- |
| `<declare-enum>` | –    | top level                  | `name`                 | `<enum>`  | [Data](./data.md#declare-enum-enum-type) |
| `<enum>`         | –    | `<declare-enum>`           | `name`, `repr`         | –         | [Data](./data.md#declare-enum-enum-type) |
| `<var-bool>`     | no   | top level, scene *(tent.)* | `name`, `init`, `from-meta` | – | [Data](./data.md#variable-declarations) |
| `<var-enum>`     | no   | top level, scene *(tent.)* | `name`, `type`, `init`, `from-meta` | – | [Data](./data.md#variable-declarations) |
| `<var-char>`     | no   | top level, scene *(tent.)* | `name`, `init`, `from-meta` | – | [Data](./data.md#variable-declarations) |
| `<var-int>` *(tent.)* | no | top level, scene *(tent.)* | `name`, `min`, `max`, `init`, `from-meta` | – | [Data](./data.md#variable-declarations) |
| `<set>`          | no   | scene body                 | `var`, `val`           | –         | [Data](./data.md#set-assign-a-variable) |
| `<add>`, `<sub>` *(tent.)* | no | scene body       | `var`, `val`           | –         | [Data](./data.md#add-and-sub-change-a-counter) |
| `<meta-set>` *(tent.)* | no | scene body           | `var`, `val`           | –         | [Data](./data.md#meta-set-write-a-meta-field) |

## Reuse

| Tag          | Step | Allowed in        | Attributes | Children     | See |
| ------------ | ---- | ----------------- | ---------- | ------------ | --- |
| `<template>` | –    | top level         | `name`     | scene body, `<slot>` | [Reuse](./reuse.md#template-and-slot-define-a-reusable-block) |
| `<slot>`     | –    | `<template>` body | `pos`      | –            | [Reuse](./reuse.md#template-and-slot-define-a-reusable-block) |
| `<x>`        | –    | scene body        | `tpl`      | `<snip>`     | [Reuse](./reuse.md#x-and-snip-use-a-template) |
| `<snip>`     | –    | `<x>`             | –          | scene body or text | [Reuse](./reuse.md#x-and-snip-use-a-template) |

## Presentation

| Tag           | Step | Allowed in          | Attributes | Children            | See |
| ------------- | ---- | ------------------- | ---------- | ------------------- | --- |
| `<r>`         | –    | `<s>`, `<choice>`   | –          | `.kirp` statements  | [Presentation](./presentation.md#r-inline-representing-code) |
| `<represent>` | –    | `<s>`, `<choice>`   | `src`      | –                   | [Presentation](./presentation.md#represent-representing-code-from-a-file) |

## System *(all tentative)*

| Tag                                  | Allowed in                     | Attributes                   | Children | See |
| ------------------------------------ | ------------------------------ | ---------------------------- | -------- | --- |
| `<setting-int>`                      | `<system>`                     | `name`, `min`, `max`, `init` | –        | [System](./system.md#settings-meta-and-save-slots) |
| `<setting-enum>`                     | `<system>`                     | `name`, `type`, `init`       | –        | [System](./system.md#settings-meta-and-save-slots) |
| `<meta-bool>`, `<meta-char>`         | `<system>`                     | `name`, `init`               | –        | [System](./system.md#settings-meta-and-save-slots) |
| `<meta-int>`                         | `<system>`                     | `name`, `min`, `max`, `init` | –        | [System](./system.md#settings-meta-and-save-slots) |
| `<meta-enum>`                        | `<system>`                     | `name`, `type`, `init`       | –        | [System](./system.md#settings-meta-and-save-slots) |
| `<save-slots>`                       | `<system>`                     | `count`                      | –        | [System](./system.md#settings-meta-and-save-slots) |
| `<page>`                             | `<system>`                     | `name`, `root`, `requires`   | UI       | [System](./system.md#modes) |
| `<dialog>`                           | `<system>`                     | `name`, `requires`           | UI       | [System](./system.md#modes) |
| `<game-mode>`                        | `<system>`                     | `name`                       | `<game-screen>`, `<layer>`, `<on-exit>` | [System](./system.md#modes) |
| `<layer>`                            | a mode                         | `name`                       | UI       | [System](./system.md#modes) |
| `<game-screen>`                      | `<game-mode>`                  | –                            | –        | [System](./system.md#modes) |
| `<on-exit>`                          | `<game-mode>`                  | –                            | `<case>` | [System](./system.md#game-mode-and-on-exit) |
| `<button>`, `<slider>`, `<toggle>`, `<select>`, `<row>`, `<col>`, `<text-frame>`, `<backlog-line>` | UI | `on-click`, `bind`, `style` | UI | [System](./system.md#ui-elements) |
| `<for-slots>`, `<for-history>`, `<for-choices>` | UI                  | –                            | UI       | [System](./system.md#repetition) |

"UI" means UI elements and repetition elements. The *Step* column does not apply to system tags.

The *Step* column: "yes" waits for input, "no" executes instantly, "–" is structural (it only
appears as part of another element or is removed at compile time).
