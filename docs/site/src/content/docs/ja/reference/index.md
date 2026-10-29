---
title: "はじめに"
---

『言語リファレンス』は、Kagari でビジュアルノベルを書いて出荷するための方法を説明します。エンジンの概念、言語、特殊な型と属性、ツール、ランタイムを扱います。コンパイラの診断は独立したセクション [診断](../diagnostics/index.md) にあります。

## Kagari とは {#what-kagari-is}

Kagari はビジュアルノベルエンジンです。次のものから成ります。

- **言語**。Rust に近い言語で、ゲーム全体をこれで書きます。物語、その演出、そして物語を取り巻くゲームシステム（タイトル画面、メニュー、セーブ、設定）です。
- **コンパイラ**。`kagari` コマンドです。プロジェクトを検査し、物語を解析し、一つの `.kagari` ファイルとアセットアーカイブにビルドします。
- **ランタイム**。ビルドしたゲームを Windows、macOS、Linux で実行します。

ゲームの各部分は、それを書く関数の**種別**によって区別されます。

| 種別          | 能力                                    | 用途 |
| ------------- | --------------------------------------- | ---- |
| `pushdown fn` | 有限データ上のプッシュダウン制御        | 物語：シーン、ステップ、選択肢、物語変数 |
| `const fn`    | チューリング完全、純粋、必要に応じてコンパイル時に評価 | 演出（レンダー関数）、定数、補助関数 |
| `fn`          | チューリング完全                        | ゲームシステム：UI、メニュー、セーブ、設定 |

どの種別がどの種別を呼べるかはコンパイラが検査します。物語は `pushdown fn` だけで構成され、チューリング完全なコードはエントリーシーンを開始することでしか物語に入れません。そのため、任意のコードに対してはどんなツールにも答えられない問いに、コンパイラが物語について答えられます。すべての選択肢が何かしらのアームを提示するか、あるシーンに到達できるか、あるステップがどの経路で到達しても同じ画面を表示するか、といった問いです。理由は [設計思想：構成による解析可能性](./overview/philosophy.md) で説明します。ゲームの残りの部分は自由に計算できます。

## 本リファレンスの対象読者 {#who-this-reference-is-for}

本リファレンスは、Kagari でゲームを作るスタジオを対象とします。シーンを書くシナリオライター、それを演出するスクリプター、ゲームシステムを作りゲームを出荷するエンジニアです。プログラミングの基礎知識を前提とします。言語は Rust の構文と多くの規則を受け継いでいるので、Rust を知っていると役立ちます。Kagari が Rust と異なる点は [Rust との違い](./syntax/rust-differences.md) にまとめてあります。

## 本リファレンスの構成 {#how-this-reference-is-organized}

| 部 | 内容 |
| -- | ---- |
| 概要 | ソースファイルから実行中のゲームまでの Kagari のゲームの [アーキテクチャ](./overview/architecture.md) と、設計の背後にある [設計思想](./overview/philosophy.md) |
| 中核概念 | 言語が実装するモデル：[関数種別](./concepts/kinds.md)、[物語機械](./concepts/story.md)、[ステップ](./concepts/steps.md)、[シーンと即時手続き](./concepts/scenes.md)、[エントリーシーンとエンディング](./concepts/entry.md)、[演出](./concepts/presentation.md)、[ゲームシステム](./concepts/system.md)、[セーブ、リプレイと決定性](./concepts/saves.md) |
| 構文 | [字句構造](./syntax/lexical.md) から [フォーム](./syntax/forms.md)、[定数式](./syntax/constants.md) までの言語 |
| 特殊な型、トレイト、属性 | [ビルトイン型](./special/builtin-types.md)、固定された [言語トレイト](./special/traits.md) の集合、[属性](./special/attributes.md) |
| ビルドと実行 | [プロジェクト](./tools/projects.md)、[`kagari` CLI](./tools/cli.md)、[エディタ支援](./tools/editors.md)、[ランタイム](./tools/runtime.md)、[ヘッドレスモード](./tools/headless.md) |
| 付録 | [クイックリファレンス](./appendix/quick-reference.md) と [用語集](./appendix/glossary.md) |

Kagari が初めての読者は、概要と中核概念を順に読み、構文の部はリファレンスとして使ってください。各概念の章から、その章で使う構文へリンクしています。

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

- `Day1` は [エントリーシーン](./concepts/entry.md) です。ゲームシステムはここから物語を開始し、その戻り値によって、どのエンディングに到達したかがシステムに伝わります。
- それぞれの `s!(…)` は [ステップ](./concepts/steps.md) です。物語が止まってプレイヤーを待つ地点です。`t![…]` はそのテキストで、`who` は話者を示します。
- `StallA()` は [シーン](./concepts/scenes.md) `StallA` を呼び出します。シーンが戻ると、物語は呼び出しの後から続きます。二回目の訪問では別の台詞が表示されます。
- `VISITED_A` は [物語変数](./concepts/story.md) です。物語の一回の実行にわたる大域的な状態で、有限型をもちます。
- `at_stall_a` は [レンダー関数](./concepts/presentation.md) です。ステップに入ったときに画面がどう変わるかを表します。ここでは 0.5 秒のクロスフェードで背景を切り替えます。

完全なゲームには、このほかに [ゲームシステム](./concepts/system.md) を記述する `fn main` と、ソースを収める [プロジェクト](./tools/projects.md) が必要です。`kagari init` はその両方をテンプレートから作成します。

```sh
kagari init my-game
cd my-game
kagari build
kagari-desktop-runtime target/v0
```

## 表記規則 {#conventions}

- *暫定* は、まだ確定していない設計を表します。後のバージョンで変わりうる名前、既定値、境界的な場合の規則です。
- `kis` と付いたコードブロックは素のコードで、`.kis` ファイルでも `.kix` ファイルでも有効です。`kix` と付いたコードブロックは `s!(…)`、`choice!{…}`、`t![…]` などの [フォーム](./syntax/forms.md) を使っており、`.kix` ファイルでだけ書けます。
- **ステップ** は常に、物語がプレイヤーの入力を待つ地点、つまり `s!(…)` か `choice!{…}` を指します。二つのステップの間にあるものは、待たずに実行されます。
- **σ**（シグマ）は物語状態を表します。物語内の位置、各フレームのローカル変数を含むコールスタック、物語変数です。
- 診断は `E5003` や `W5004` のようにコードで呼びます。すべての診断は [診断](../diagnostics/index.md) に一覧があります。
- **スタジオ** はゲームを書くチームを、**エンジン** はそれを実行するランタイムを指します。

## 他のセクション {#other-sections}

- [チュートリアル](../tutorial/index.md) は、Kagari の導入から最初のゲームまでを案内します。
- [『STD リファレンス』](../std/index.md) は、標準ライブラリ `std` のすべてのアイテムを説明します。画面、アニメーション、音声、テキスト、システム、セーブ、UI です。
- [診断](../diagnostics/index.md) は、コンパイラが報告するすべての診断の一覧です。
- [『Kagari Develop Guide』](https://github.com/kirikiroid3/kagari/blob/main/docs/dev/src/introduction.md)（リポジトリ内、英語のみ）は、Kagari 自体の開発者向けです。設計判断、コンパイラの内部、ファイル形式、ランタイムの内部を扱います。
