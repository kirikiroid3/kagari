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
| `<system>`                           | `<kagari>`                     | `start`                      | declarations, `<sysfn>`, modes | [System](./system.md#system) |
| `<setting-int>`                      | `<system>`                     | `name`, `min`, `max`, `init` | –        | [System](./system.md#settings) |
| `<setting-enum>`                     | `<system>`                     | `name`, `type`, `init`       | –        | [System](./system.md#settings) |
| `<setting-bool>`                     | `<system>`                     | `name`, `init`               | –        | [System](./system.md#settings) |
| `<meta-bool>`, `<meta-char>`         | `<system>`                     | `name`, `init`               | –        | [System](./system.md#meta-fields) |
| `<meta-int>`                         | `<system>`                     | `name`, `min`, `max`, `init` | –        | [System](./system.md#meta-fields) |
| `<meta-enum>`                        | `<system>`                     | `name`, `type`, `init`       | –        | [System](./system.md#meta-fields) |
| `<save-slots>`                       | `<system>`                     | `count`                      | –        | [System](./system.md#save-slots) |
| `<sysfn>`                            | `<system>`                     | `name`, `args`, `return`     | locals, sysfn body | [System](./system.md#system-functions) |
| `<var-i16>`, `<var-vec2>`            | `<sysfn>`                      | `name`, `init`               | –        | [System](./system.md#locals) |
| `<sysfn-arg>`                        | `<sysfn>` body                 | `position`, `var`            | –        | [System](./system.md#sysfn-arg-and-sysfn-return) |
| `<sysfn-return>`                     | `<sysfn>` body                 | `val`                        | –        | [System](./system.md#sysfn-arg-and-sysfn-return) |
| `<sysfn-call>`                       | sysfn body, `<on-exit>` `<do>` | `name`, `save`               | `<sysfn-param>` | [System](./system.md#sysfn-call-and-sysfn-param) |
| `<sysfn-param>`                      | `<sysfn-call>`                 | `var` or `literal`           | –        | [System](./system.md#sysfn-call-and-sysfn-param) |
| `<metafn-start>`, `<metafn-load>`, `<metafn-save>`, `<metafn-select>` | sysfn body, `<on-exit>` `<do>` | `param` | – | [System](./system.md#operations) |
| `<metafn-advance>`, `<metafn-quit>`  | sysfn body, `<on-exit>` `<do>` | –                            | –        | [System](./system.md#operations) |
| `<interface-call>`, `<interface-goto>` | sysfn body, `<on-exit>` `<do>` | `page`                     | –        | [System](./system.md#operations) |
| `<interface-return>`, `<interface-focus-activate>` | sysfn body, `<on-exit>` `<do>` | –              | –        | [System](./system.md#operations) |
| `<interface-focus-move>`             | sysfn body, `<on-exit>` `<do>` | `direction` or `target-idx`  | –        | [System](./system.md#operations) |
| `<page>`                             | `<system>`                     | `name`, `root`, `requires`, `on-keyboard`, `on-controller` | `<layer>`, UI | [System](./system.md#modes) |
| `<dialog>`                           | `<system>`                     | `name`, `requires`, `on-keyboard`, `on-controller` | `<layer>`, UI | [System](./system.md#modes) |
| `<game-mode>`                        | `<system>`                     | `name`, `on-keyboard`, `on-controller` | `<game-screen>`, `<layer>`, `<on-exit>` | [System](./system.md#modes) |
| `<layer>`                            | a mode                         | `name`, `style`              | UI       | [System](./system.md#modes) |
| `<game-screen>`                      | `<game-mode>`                  | –                            | –        | [System](./system.md#modes) |
| `<on-exit>`                          | `<game-mode>`                  | –                            | `<cases>` | [System](./system.md#on-exit) |
| `<button>`, `<text-frame>`           | UI                             | `style`, `on-click`          | text (`<button>`), – | [System](./system.md#ui-elements) |
| `<slider>`, `<toggle>`, `<select>`   | UI                             | `style`, `bind`              | –        | [System](./system.md#ui-elements) |
| `<row>`, `<col>`, `<backlog-line>`   | UI                             | `style`                      | UI (`<row>`, `<col>`) | [System](./system.md#ui-elements) |
| `<for-slots>`, `<for-history>`, `<for-choices>` | UI                  | –                            | UI       | [System](./system.md#repetition) |

In system function bodies, `<match>`, `<set>`, `<add>`, `<sub>` and `<meta-set>` are also allowed
(see [System](./system.md#statements)).

"UI" means UI elements and repetition elements. The *Step* column does not apply to system tags.

The *Step* column: "yes" waits for input, "no" executes instantly, "–" is structural (it only
appears as part of another element or is removed at compile time).
