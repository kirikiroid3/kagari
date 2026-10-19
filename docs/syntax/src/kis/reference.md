# Tag Reference

"Scene body" means script content allowed inside `<scene>`, `<choice-arm>`, `<do>`, `<snip>` and
`<template>`: steps, instant elements and `<x>`. Local variable declarations are only allowed as
direct children of `<scene>`.

## Structure

| Tag                    | Step | Allowed in                 | Attributes        | Children                         | See |
| ---------------------- | ---- | -------------------------- | ----------------- | -------------------------------- | --- |
| `<kagari>` *(tent.)*   | –    | document root              | –                 | top-level declarations           | [Overview](./overview.md#document-structure) |
| `<scene>`              | –    | top level                  | `name`            | scene body                       | [Control Flow](./control-flow.md#scene-scene-declaration) |
| `<include>`            | –    | top level                  | `src`             | –                                | [Reuse](./reuse.md#include-splice-another-script) |
| `<import>`             | –    | top level                  | `src`             | –                                | [Reuse](./reuse.md#import-use-extension-definitions) |

## Steps

| Tag            | Step | Allowed in    | Attributes                     | Children                          | See |
| -------------- | ---- | ------------- | ------------------------------ | --------------------------------- | --- |
| `<s>`          | yes  | scene body    | –                              | text, `<r>`, `<represent>`        | [Steps](./steps.md#s-ordinary-step) |
| `<choice>`     | yes  | scene body    | `id`                           | `<choice-arm>`, `<match>`, `<r>`, `<represent>` | [Steps](./steps.md#choice-and-choice-arm-choice-step) |
| `<choice-arm>` | –    | `<choice>`, `<do>` in a choice | `id`, `text`  | scene body                        | [Steps](./steps.md#choice-and-choice-arm-choice-step) |

## Control flow

| Tag        | Step | Allowed in             | Attributes | Children                  | See |
| ---------- | ---- | ---------------------- | ---------- | ------------------------- | --- |
| `<label>`  | no   | scene body             | `id`       | –                         | [Steps](./steps.md#label-position-marker) |
| `<goto>`   | no   | scene body             | `label`    | –                         | [Control Flow](./control-flow.md#goto-jump-within-the-scene) |
| `<call>`   | no   | scene body             | `scene`    | –                         | [Control Flow](./control-flow.md#call-enter-a-scene) |
| `<return>` *(tent.)* | no | scene body       | –          | –                         | [Control Flow](./control-flow.md#return-leave-the-scene-early) |
| `<match>`  | no   | scene body, `<choice>` | `var` or `vars` | `<cases>`            | [Control Flow](./control-flow.md#match-branch-on-variables) |
| `<cases>`  | –    | `<match>`              | –          | `<case>`…, then `<do>`    | [Control Flow](./control-flow.md#match-branch-on-variables) |
| `<case>`   | –    | `<cases>`              | `val`      | –                         | [Control Flow](./control-flow.md#match-branch-on-variables) |
| `<do>`     | –    | `<cases>`              | –          | scene body; only `<choice-arm>` inside `<choice>` | [Control Flow](./control-flow.md#match-branch-on-variables) |

## Data

| Tag              | Step | Allowed in                 | Attributes             | Children  | See |
| ---------------- | ---- | -------------------------- | ---------------------- | --------- | --- |
| `<declare-enum>` | –    | top level                  | `name`                 | `<enum>`  | [Data](./data.md#declare-enum-enum-type) |
| `<enum>`         | –    | `<declare-enum>`           | `name`, `repr`         | –         | [Data](./data.md#declare-enum-enum-type) |
| `<var-bool>`     | no   | top level, scene *(tent.)* | `name`, `init`         | –         | [Data](./data.md#variable-declarations) |
| `<var-enum>`     | no   | top level, scene *(tent.)* | `name`, `type`, `init` | –         | [Data](./data.md#variable-declarations) |
| `<var-char>`     | no   | top level, scene *(tent.)* | `name`, `init`         | –         | [Data](./data.md#variable-declarations) |
| `<var-int>` *(tent.)* | no | top level, scene *(tent.)* | `name`, `min`, `max`, `init` | – | [Data](./data.md#variable-declarations) |
| `<set>`          | no   | scene body                 | `var`, `val`           | –         | [Data](./data.md#set-assign-a-variable) |
| `<add>`, `<sub>` *(tent.)* | no | scene body       | `var`, `val`           | –         | [Data](./data.md#add-and-sub-change-a-counter) |

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

The *Step* column: "yes" waits for input, "no" executes instantly, "–" is structural (it only
appears as part of another element or is removed at compile time).
