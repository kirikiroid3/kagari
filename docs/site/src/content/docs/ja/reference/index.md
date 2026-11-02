---
title: "はじめに"
---

『言語リファレンス』では、Kagari でビジュアルノベルを作り、リリースするまでに必要なことを説明します。扱うのは、エンジンの考え方、言語、特殊な型と属性、ツール、ランタイムです。コンパイラの診断については、別セクションの [診断](../diagnostics/index.md) にまとめています。

## Kagari とは {#what-kagari-is}

Kagari はビジュアルノベルエンジンで、次の 3 つで構成されています。

- **言語**：Rust に近い言語です。物語、その演出、物語を取り巻くゲームシステム（タイトル画面、メニュー、セーブ、設定）まで、ゲーム全体をこの言語で書きます。
- **コンパイラ**：`kagari` コマンドです。プロジェクトを検査して物語を解析し、`.kagari` ファイル 1 つとアセットアーカイブにビルドします。
- **ランタイム**：ビルドしたゲームを Windows、macOS、Linux で動かします。

ゲームの各部分は、どの**種別**の関数で書くかによって分かれます。

| 種別          | 表現力                                  | 用途 |
| ------------- | --------------------------------------- | ---- |
| `pushdown fn` | 有限のデータを扱うプッシュダウン制御    | 物語：シーン、ステップ、選択肢、物語変数 |
| `const fn`    | チューリング完全で純粋。必要に応じてコンパイル時に評価 | 演出（レンダー関数）、定数、補助関数 |
| `fn`          | チューリング完全                        | ゲームシステム：UI、メニュー、セーブ、設定 |

どの種別からどの種別を呼べるかは、コンパイラが検査します。物語は `pushdown fn` だけでできており、チューリング完全なコードが物語に入るには開始シーンを起動するしかありません。このため、任意のコードが相手ならどんなツールにも答えられない問いに、物語についてならコンパイラが答えられます。たとえば、どの選択肢でも必ず何かしらのアームが表示されるか、あるシーンに到達できるか、あるステップがどの経路から来ても同じ画面を表示するか、といった問いです。その理由は [設計思想：解析できるように作る](./overview/philosophy.md) で説明します。物語以外の部分では、計算に制限はありません。

## 対象読者 {#who-this-reference-is-for}

このリファレンスは、Kagari でゲームを作る制作者に向けたものです。シーンを書くシナリオライター、それを演出するスクリプター、ゲームシステムを組んでゲームをリリースするエンジニアを想定しています。プログラミングの基礎知識は前提とします。言語は Rust の構文と多くの規則を受け継いでいるので、Rust を知っていると理解が早いはずです。Rust と異なる点は [Rust との違い](./syntax/rust-differences.md) にまとめています。

## 構成 {#how-this-reference-is-organized}

| 部 | 内容 |
| -- | ---- |
| 概要 | ソースファイルから実行中のゲームに至るまでの [全体構成](./overview/architecture.md) と、設計の根底にある [設計思想](./overview/philosophy.md) |
| 基本概念 | 言語が実装するモデル：[関数種別](./concepts/kinds.md)、[物語機械](./concepts/story.md)、[ステップ](./concepts/steps.md)、[シーンと即時手続き](./concepts/scenes.md)、[開始シーンとエンディング](./concepts/entry.md)、[演出](./concepts/presentation.md)、[ゲームシステム](./concepts/system.md)、[セーブ、再生、決定性](./concepts/saves.md) |
| 構文 | [字句構造](./syntax/lexical.md) から [組み込み構文](./syntax/forms.md)、[定数式](./syntax/constants.md) まで、言語そのもの |
| 特殊な型、トレイト、属性 | [組み込み型](./special/builtin-types.md)、固定された [言語トレイト](./special/traits.md)、[属性](./special/attributes.md) |
| ビルドと実行 | [プロジェクト](./tools/projects.md)、[`kagari` CLI](./tools/cli.md)、[エディタ対応](./tools/editors.md)、[ランタイム](./tools/runtime.md)、[ヘッドレスモード](./tools/headless.md) |
| 付録 | [早見表](./appendix/quick-reference.md) と [用語集](./appendix/glossary.md) |

Kagari が初めてなら、概要と基本概念を順に読み、構文の部は必要なときに引いてください。概念の各章からは、その章で使う構文へのリンクを張っています。

## 最初の例 {#a-first-look}

```kix
static mut VISITED_A: bool = false;

const STALL_A: ImageAsset = asset("bg/stall_a.png");

enum Ending { Normal }

#[scene]
#[entry]
pushdown fn Day1() -> Ending {
    s!(t!["The school festival starts today."]);
    StallA();
    StallA();
    Ending::Normal
}

#[scene]
pushdown fn StallA() {
    if VISITED_A {
        s!(t!["The stall is already packing up."]);
        return;
    }
    VISITED_A = true;
    s!(who = "Hanako", render = at_stall_a, t!["Welcome to stall A!"]);
}

const fn at_stall_a(mut screen: Screen) -> Screen {
    screen.background().change(STALL_A).apply_animation(fade(ms(500)));
    screen
}
```

- `Day1` は [開始シーン](./concepts/entry.md) です。ゲームシステムはここから物語を始め、戻り値によって、どのエンディングに到達したかを知ります。
- `s!(…)` の一つひとつが [ステップ](./concepts/steps.md) で、物語が止まってプレイヤーを待つ地点です。`t![…]` が表示するテキスト、`who` が話者です。
- `StallA()` は [シーン](./concepts/scenes.md) `StallA` の呼び出しです。シーンから戻ると、物語は呼び出しの次から続きます。2 回目に訪れたときは別の台詞になります。
- `VISITED_A` は [物語変数](./concepts/story.md) です。物語を 1 回通して実行する間の大域的な状態で、型は有限型です。
- `at_stall_a` は [レンダー関数](./concepts/presentation.md) です。ステップに入ったときに画面がどう変わるかを書きます。ここでは 0.5 秒のクロスフェードで背景を切り替えています。

ゲームとして完成させるには、このほかに [ゲームシステム](./concepts/system.md) を記述する `fn main` と、ソースをまとめる [プロジェクト](./tools/projects.md) が必要です。`kagari init` を使えば、どちらもテンプレートから作れます。

```sh
kagari init my-game
cd my-game
kagari build
kagari-desktop-runtime target/v0
```

## 表記の約束 {#conventions}

- *暫定* と書いた箇所は、設計がまだ固まっていないことを示します。名前、既定値、境界的な場合の規則などは、今後のバージョンで変わることがあります。
- `kis` と付いたコードブロックは素のコードで、`.kis` と `.kix` のどちらのファイルにも書けます。`kix` と付いたコードブロックは `s!(…)`、`choice!{…}`、`t![…]` などの [組み込み構文](./syntax/forms.md) を使っており、`.kix` ファイルにしか書けません。
- **ステップ** は常に、物語がプレイヤーの入力を待つ地点、すなわち `s!(…)` か `choice!{…}` を指します。2 つのステップの間にある処理は、待たずに実行されます。
- **σ**（シグマ）は物語状態を表します。物語内の位置、各フレームのローカル変数を含むコールスタック、物語変数をまとめたものです。
- 診断は `E5003` や `W5004` のようにコードで呼びます。一覧は [診断](../diagnostics/index.md) にあります。
- **制作者** はゲームを作るチームを、**エンジン** はゲームを動かすランタイムを指します。

## ほかのセクション {#other-sections}

- [入門](../tutorial/index.md)：Kagari のインストールから最初のゲームまでを案内します。
- [『標準ライブラリのリファレンス』](../std/index.md)：標準ライブラリ `std` のすべてのアイテムを説明します。画面、アニメーション、音声、テキスト、システム、セーブ、UI を扱います。
- [診断](../diagnostics/index.md)：コンパイラが報告するすべての診断の一覧です。
- [『Kagari 開発者ガイド』](https://github.com/kirikiroid3/kagari/blob/main/docs/dev/src/introduction.md)（リポジトリ内、英語のみ）：Kagari 自体の開発に参加する人向けです。設計判断、コンパイラの内部、ファイル形式、ランタイムの内部を扱います。
