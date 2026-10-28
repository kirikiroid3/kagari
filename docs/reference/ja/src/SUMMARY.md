# Summary

[はじめに](./introduction.md)

# 概要

- [アーキテクチャ](./overview/architecture.md)
- [設計思想：構成による解析可能性](./overview/philosophy.md)

# 中核概念

- [関数種別](./concepts/kinds.md)
- [物語機械](./concepts/story.md)
- [ステップ](./concepts/steps.md)
- [シーンと即時手続き](./concepts/scenes.md)
- [エントリーシーンとエンディング](./concepts/entry.md)
- [演出](./concepts/presentation.md)
- [ゲームシステム](./concepts/system.md)
- [セーブ、リプレイと決定性](./concepts/saves.md)

# 構文

- [字句構造](./syntax/lexical.md)
- [ソースファイルとモジュール](./syntax/modules.md)
- [アイテム](./syntax/items.md)
- [型](./syntax/types.md)
- [数値](./syntax/numbers.md)
- [値、参照、クロージャ](./syntax/values.md)
- [関数](./syntax/functions.md)
- [式と文](./syntax/expressions.md)
- [パターン](./syntax/patterns.md)
- [制御フローとループ](./syntax/control-flow.md)
- [フォーム](./syntax/forms.md)
- [定数式](./syntax/constants.md)
- [Rust との違い](./syntax/rust-differences.md)

# 特殊な型、トレイト、属性

- [ビルトイン型](./special/builtin-types.md)
- [言語トレイト](./special/traits.md)
- [属性](./special/attributes.md)

# ビルドと実行

- [プロジェクト](./tools/projects.md)
- [`kagari` CLI](./tools/cli.md)
- [エディタ支援](./tools/editors.md)
- [ランタイム](./tools/runtime.md)
- [ヘッドレスモード](./tools/headless.md)

# 付録

- [クイックリファレンス](./appendix/quick-reference.md)
- [用語集](./appendix/glossary.md)
- [診断](./diagnostics/index.md)
  - [構文 (E0xxx)](./diagnostics/syntax.md)
  - [モジュール、アイテム、名前 (E1xxx)](./diagnostics/modules.md)
  - [型 (E2xxx)](./diagnostics/types.md)
  - [種別 (E3xxx)](./diagnostics/kinds.md)
  - [定数評価 (E4xxx)](./diagnostics/eval.md)
  - [物語解析 (E5xxx)](./diagnostics/analysis.md)
  - [ゲームシステム (E6xxx)](./diagnostics/system.md)
  - [バイトコード生成 (E7xxx)](./diagnostics/emit.md)
