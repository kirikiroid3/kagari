---
title: "ゲームシステム"
---

ゲームシステムとは、タイトル画面、セーブ・ロード画面、設定、バックログ、ゲーム中の UI など、
物語を取り巻く部分すべてのことです。`fn` と `const fn` のコードで書き、
[呼び出し規則](./kinds.md#call-rules) のほかに制限はありません。ただし、システムから物語を
呼び出すことはできず、開始できるだけです。物語変数も読めません。

この章では、入れ子になった二つの機械、プログラムの記述、UI コンポーネント、モード、
`Narrative` ハンドル、モードグラフの検査という順に、システムの仕組みを説明します。
登場するアイテムの仕様は、標準ライブラリのリファレンスの [`std::system`](../../std/system.md)、
[`std::system::nav`](../../std/system-nav.md)、
[`std::system::saves`](../../std/system-saves.md)、
[`std::narrative`](../../std/narrative.md)、
[`std::ui`](../../std/ui.md) を参照してください。

## 入れ子になった二つの機械 {#two-nested-machines}

ゲーム全体は、スタックを共有する単一の機械ではなく、入れ子になった二つの機械でできています。

- 外側の **システム機械** には **モード** のスタックがあります。積まれるのは、ルートページ
  （タイトル、ギャラリー、クレジット）、オーバーレイ（セーブ、ロード、コンフィグ、バックログ）、
  ダイアログ、ゲームモードです。
- ゲームモードの中では、物語が一つ実行されています。これが **物語機械** で、位置、コールスタック、
  物語変数、画面、PRNG の状態、トレースを含みます。システムのコードからは
  [`Narrative` ハンドル](#the-narrative-handle) を通してしか見えません。

<!-- diagram: system-machines (mermaid-ish)
flowchart LR
  subgraph sys["システム機械：モードのスタック（上が最上位）"]
    D["Dialog（例：確認）"]
    O["GameOverlay（例：セーブ、バックログ）"]
    subgraph G["ゲームモード（土台）：System::game ビュー"]
      S["物語機械：位置、コールスタック、物語変数、論理画面、PRNG の状態、選択のトレース。システムからは Narrative 経由でしか見えない"]
    end
    D ==> O ==> G
  end
  Settings["設定：グローバル、セーブ外"]
  Meta["メタデータ：グローバル、セーブ外"]
  S ==>|セーブ| Slots["セーブスロット：物語のトレースを保存"]
-->
{{#include ../diagrams/system-machines.svg}}

物語のコールスタックはゲームモードの内側にあり、システムのスタックに積まれることはありません。
ここから次の三つの性質が導かれます。

- **セーブにはシステムの状態を含められません。** セーブに記録されるのは実行中の物語だけで、
  メニューのフレームや設定はそこから辿れません。
- **ロードは積み増しではなく置き換えです。** 復元した物語は新しいゲームフレームに入ります。
  先に巻き戻すべきメニューのスタックはありません。
- **「ゲームが実行中」かどうかはスタックで決まります。** ゲームフレームがあるときに限り、
  ゲームは実行中です。

| 保存先 | 寿命 | セーブに含まれるか |
| ------ | ---- | ------------------ |
| 設定 | 永続、グローバル（設定ファイル） | 含まれない |
| メタデータ | 永続、グローバル（メタデータのファイル：解放状況、既読フラグ、既見集合） | 含まれない |
| セーブスロット | 永続 | セーブそのもの |
| モードのスタックと各モード内の状態 | 一時的、起動時にリセット | 含まれない |
| 実行中の物語 | 一時的、ゲームフレームの中 | トレースとして含まれる。[セーブ、再生、決定性](./saves.md) を参照 |

## プログラム {#the-program}

```kix
// main.kix
mod story;
mod ui;

use std::narrative::text::text_style;
use std::screen::size;
use std::system::default_bindings;

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("fonts/default.otf");

fn main() -> System {
    System {
        title: "Example",
        resolution: size(1280, 720),
        start: ui::Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: default_bindings(),
    }
}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}
```

`main` はルートモジュールに置く、`System` を返す `fn` です。システムの構成を記述するもので、
エンジンが起動時に一度だけ実行します。`main` がないプログラムはコンパイルできません。

| フィールド | 意味 |
| ---------- | ---- |
| `title` | ウィンドウのタイトル |
| `resolution` | 設計解像度。レンダーと UI の位置や長さはこの解像度でのピクセル数で表し、ゲーム全体を通して変わらない |
| `start` | システムが最初に表示するルートページ。`fn() -> RootPage` |
| `game` | ゲームモードのビュー。`fn(Narrative) -> Element` |
| `save_slots`、`auto_slots` | 手動セーブとオートセーブのスロット数。固定なので、スロットの一覧は有限 |
| `backlog` | バックログに残す項目の最大数 |
| `font` | 既定のフォント。スタイルでも効果でもフォントを指定していないテキストに使う |
| `text` | `ui::label` と `text!` の既定のテキストスタイル |
| `settings` | エンジンが使う設定のハンドル。[設定とメタデータ](#settings-and-meta) を参照 |
| `bindings` | 入力の割り当て。[バインディング](#bindings) を参照 |

`main` は普通のコードとして実行されます。そのため、実行時のほかの呼び出しと同じく、`main` の
呼び出しも再帰の深さに数えます。入れ子の呼び出しは既定で最大 2048 段までで、
[`kagari.toml`](../tools/projects.md) の `recursion-depth` キーで変更できます。これより深く
再帰すると、実行時エラーでゲームが止まります。

**システム音楽。** `std::system::set_music(track, fade)` でシステム音楽チャンネルの曲を設定します。
このチャンネルは、タイトルページのように、モードのスタック上でゲームが実行されていない間に
流れます。ゲームの実行中は、代わりに物語の画面の音楽チャンネルが流れます。`None` を渡すと
止まります。`set_music` は `fn` なので、呼べるのはシステムのコードだけです。

## コンポーネントとシグナル {#components-and-signals}

UI は `std::ui` の UI 構文を使って `fn` のコードで書きます。動作の仕組みは SolidJS と同じです。

```kix
// ui.kix
use crate::MUSIC;

fn Config() -> Overlay {
    let (music, set_music) = signal(MUSIC.get());
    Overlay(column![
        text!(t!["Music: " {music().level}]),
        slider!{
            value = music().level,
            on_change = |v| {
                let vol = Volume { level: v, muted: music().muted };
                set_music(vol);
                MUSIC.set(vol)
            },
        },
        button!{on_click = || nav::back(), t!["Back"]},
    ])
}
```

- UI 構文の [区切り記号の規則](../syntax/forms.md) は、物語の組み込み構文と同じです。`fn` と
  `const fn` のコードの中では **値**（`Element` 型）を作るだけで、ステップにはなりません。
- **コンポーネント** とは、UI の値を返す普通の `fn` です。モードを表示したときに **一度だけ**
  実行されます。ほかの関数と同じように呼び出すことも、`nav::call(Config)` のように関数ポインタ
  として渡すこともできます。
- `signal(init)` はゲッターとセッターの組を返します。シグナルは `fn` のコードでは第一級の値ですが、
  物語や `const fn` では使えません。
- シグナルを読む属性値や要素は **リアクティブ** になり、シグナルが変わるたびに評価し直されます。
  コンポーネントのそれ以外の部分は一度しか実行されません。`Setting::get`、`Meta::get`、
  `Narrative` ハンドルからの読み取り、セーブ情報、既見集合もリアクティブな値の出どころです。
- `on_click` などのハンドラは第一級の [クロージャ](../syntax/values.md) です。
- スライダーの範囲は、値の `Int` 型の範囲で決まります。`Volume::level` は `Int<0, 100>` なので、
  上のスライダーは `min` や `max` 属性を書かなくても 0 から 100 までになります。

UI 構文は `std::ui` にあり、レイアウト（`column![…]`、`row![…]`、`layer![…]`）、
内容（`text!(…)`、`image!(…)`、`game_screen!(…)`）、コントロール（`button!{…}`、
`slider!{…}`、`toggle!{…}`、`select!{…}`）に分かれます。どの構文も、props 構造体を引数に取る
`std::ui` の関数呼び出しに展開されます。その際、リアクティブな属性はクロージャになり、
ハンドラはそのまま残り、各要素もクロージャになります。展開の対応表、レイアウト、操作時の見た目、
フォーカスの規則は [`std::ui`](../../std/ui.md) にあります。

## モード {#modes}

モードの種類は、コンポーネントの戻り値の型で決まります。どのモード型も、`Overlay(column![…])`
のように `Element` を包んだものです。

| 型 | コンポーネント | モードの例 | スタック上の位置 |
| -- | -------------- | ---------- | ---------------- |
| `RootPage` | `fn() -> RootPage` | タイトル、ギャラリー、クレジット | 土台のみ。ゲームフレームとは同時に存在しない |
| `Overlay` | `fn() -> Overlay` | コンフィグ、ロード | どのフレームの上にも積める |
| `GameOverlay` | `fn(Narrative) -> GameOverlay` | セーブ、バックログ | ゲームモードの上にだけ積める |
| `Dialog` | `fn() -> Dialog` | 確認 | オーバーレイと同じ。下のフレームを静止画にした上に描く |
| ゲームモード | `fn(Narrative) -> Element`（`System::game` ビュー） | 実行中の物語 | 土台のみ |

入力を受け取るのは最上位のフレームだけです。ゲームモードの上にオーバーレイやダイアログがある間、
物語は一時停止し、画面で再生中のアニメーションは最終状態まで飛ばされます。そのため、下にある
フレームは常に静止しています。

## 操作 {#operations}

UI からシステムを変えられるのは、次の操作だけです。`std::system::nav` の画面遷移の関数は、
どの `fn` からでも呼べます。

| 操作 | 効果 | 条件 |
| ---- | ---- | ---- |
| `nav::call(o)` | オーバーレイ `o` を積む | |
| `nav::dialog(d)` | ダイアログ `d` を積む | |
| `nav::back()` | 最上位のフレームを取り除く | 土台の上では何もしない |
| `nav::goto(p)` | スタックをルートページ `p` だけにする | |
| `nav::start(e, args, on_exit)` | スタックを、開始シーン `e` を実行する新しいゲームだけにする | |
| `nav::load(k)` | スタックを、スロット `k` から復元したゲームだけにする | セーブが今のスクリプトと合わなければ `Err` を返し、スタックは変えない |
| `nav::quit()` | 確認せずにすぐ終了する | |

実行中のゲームに対する操作は、[`Narrative` ハンドル](#the-narrative-handle) のメソッドです。

| 操作 | 効果 |
| ---- | ---- |
| `game.advance()` | 物語を次のステップへ進める |
| `game.select(arm)` | 現在の選択肢で表示中のアームを選ぶ |
| `game.save(k)` | 実行中のゲームをスロット `k` にセーブする |
| `game.rewind(i)` | 物語をバックログの項目 `i` の時点まで戻す |
| `game.call(o)` | ゲームオーバーレイ `o` を積む |

`nav::quit` は確認を出しません。確認が必要なボタンでは、制作者が用意した `Dialog` を開きます。

## 物語の開始 {#starting-the-story}

```kix
button!{on_click = || nav::start(story::Main, (CLEARED_AZUSA.get(),), on_ending), t!["Start"]}

fn on_ending(ending: story::Ending) {
    match ending {
        story::Ending::AzusaEnd => { CLEARED_AZUSA.set(true); nav::goto(Credits) }
        story::Ending::NormalEnd => nav::goto(Credits),
    }
}
```

`nav::start` は [開始シーン](./entry.md)、その引数、終了ハンドラを受け取ります。物語に入る方法は
これしかありません。

- `story::Main` は `#[entry]` シーン `scene fn(P1, …, Pn) -> R` へのポインタで、引数はタプル
  `(P1, …, Pn)`、終了ハンドラは `Fn(R)` です。
- 引数の型は、開始シーンの有限なパラメータ型です。引数はセーブに記録されます。
- `nav::start` を呼ぶたびに、物語の PRNG に新しいシードが与えられます。詳しくは
  [決定性](./saves.md#determinism) を参照してください。
- 開始シーンが戻ると、戻り値が終了ハンドラに渡されます。ハンドラはすべての値を扱う必要があり、
  これは普通の網羅的な `match` で書けます。ハンドラがどこにも遷移しなければ、スタックは開始
  ページに戻ります。

おまけページからのシーンの **回想** は、別の `#[entry]` シーンを `nav::start` で始めるだけです。
解放には、物語がそのシーンを初めて通ったときに書き込む `Meta<bool>` を使います。

## 設定とメタデータ {#settings-and-meta}

```kis
static MUSIC: Setting<Volume> = setting(volume(60));
static PETALS_ON: Setting<bool> = setting(true);
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

`Setting<T>` や `Meta<T>` を保持する不変の `static` で、設定やメタデータの項目を宣言します。
初期化子に書くのは、保存済みのファイルに値がないとき（初回起動時や、アップデートで項目が
増えたときなど）に使う値です。どちらも永続的でグローバルな値で、セーブとは別に保存されます。

| メソッド | 関数種別 | 設定 | メタデータ |
| -------- | -------- | ---- | ---------- |
| `get()` | `fn` | あり | あり |
| `set(v)` | `fn` | あり | あり |
| `write(v)` | `pushdown fn` | なし | あり。物語からメタデータへ書き込む（書き込み専用） |
| ハンドルそのもの | `const fn` | [遅延参照](./presentation.md#settings-in-renders)。`Entity::gate` に渡す | なし |

このため、設定が物語の制御フローに影響することはなく、物語に対する解析はどの設定値でも
成り立ちます。メタデータが物語に届くのは、開始時の開始引数を通してだけです。

エンジン自体の設定は、`System::settings` に渡す `Settings` レコードに並べたハンドルです。
テキスト速度、オートモードの待ち時間、スキップで未読のテキストも飛ばすかどうか、マスター・
音楽・効果音・ボイスの音量、ボイスの継続再生、オートモードでボイスの終わりを待つかどうか、
ウィンドウの不透明度、フルスクリーン、アニメーションを再生するかどうか（オフのときは長さを
すべて 0 として扱います）があります。音量は `Volume` 型で、レベル `Int<0, 100>` とミュートの
フラグからなります。上の `PETALS_ON` のように、制作者は独自の設定を自前のハンドルとして宣言
できます。

既読フラグは、物語が進むたびにエンジンが[ステップ ID](./steps.md#step-ids) をキーにして書き込むメタデータです。物語からは
見えません。システムはこれを、既読スキップや、バックログで既読の行に印を付けるのに使います。

**既見集合** も、エンジンが同じように管理するメタデータです。`std::system` の `seen_image(i)`、
`heard(a)`、`watched(v)` は、それぞれ画像がステップの論理画面に載ったことがあるか、曲や効果音が
再生されたことがあるか、動画が画面上で再生を始めたことがあるかを返します（動画は音声の有無を
問いません。セーブをロードする際のステップの再実行では記録されません）。既読フラグと同じく、
ロードや巻き戻しで元に戻ることはありません。制作者が自分で記録しなくても、CG ギャラリー、
音楽鑑賞、動画鑑賞の画面を作れます。

```kix
use std::system::seen_image;

fn GalleryThumb(cg: ImageAsset) -> Element {
    button!{disabled = !seen_image(cg), image!(cg)}
}
```

## `Narrative` ハンドル {#the-narrative-handle}

システムが実行中の物語に触れる手段は、`std::narrative` の `Narrative` ハンドルだけです。
エンジンはこのハンドルを、ゲーム画面（`System::game`）とゲームオーバーレイにしか渡しません。
内部表現は隠されているので、システムのコードから物語変数は読めず、システムが何をしても物語の
解析には影響しません。メソッドはすべて `fn` で、読み取りはリアクティブです。

```kis
impl Narrative {
    #[builtin] pub fn line(&self) -> Option<Line>;      // the current s! step
    #[builtin] pub fn menu(&self) -> Option<Menu>;      // the arms shown at the current choice!
    #[builtin] pub fn hud(&self) -> HudState;           // the Hud of the logical screen
    #[builtin] pub fn history(&self) -> Vec<Entry>;     // the backlog, oldest first
    #[builtin] pub fn mode(&self) -> PlayMode;          // normal, auto or skip
    #[builtin] pub fn set_mode(&self, mode: PlayMode);
    #[builtin] pub fn advance(&self);
    #[builtin] pub fn select(&self, arm: Arm);
    #[builtin] pub fn save(&self, slot: SaveSlot);
    #[builtin] pub fn rewind(&self, entry: u32);
    #[builtin] pub fn call(&self, overlay: fn(Narrative) -> GameOverlay);
    #[builtin] pub fn replay_voice(&self, voice: VoiceAsset);
}
```

`game_screen!(game = game)` は実行中の物語の画面を描き、`ui::message(game, style)` は現在の行を
文字送り、テキスト効果、キューとあわせて描きます。`Line`、`Menu`、`HudState`、`Entry`、`PlayMode`
の各レコードの仕様は [`std::narrative`](../../std/narrative.md) にあります。

ハンドルはゲーム画面とゲームオーバーレイにしかありません。そのため、セーブ、物語を進める操作、
セーブ画面のオーバーレイを開く操作など、実行中のゲームが必要な処理は、タイトルページにはそもそも
書けません。セーブ、ロード、巻き戻しについては [セーブ、再生、決定性](./saves.md) で
説明します。

## バインディング {#bindings}

`System::bindings` は、入力（キー、マウスのボタンとホイール、ゲームパッドのボタン）をアクションに
割り当てます。

| アクション | 効果 | 有効な場面 |
| ---------- | ---- | ---------- |
| `Advance` | 物語を進める。ウィンドウを隠しているときは、代わりに再表示する | ゲームモードが最上位のとき |
| `HoldSkip`、`ToggleSkip` | 押している間スキップ／スキップモードの切り替え | ゲームモードが最上位のとき |
| `ToggleAuto` | オートモードの切り替え | ゲームモードが最上位のとき |
| `HideWindow` | テキストウィンドウの表示・非表示の切り替え | ゲームモードが最上位のとき |
| `QuickSave`、`QuickLoad` | クイックスロットへのセーブ／クイックスロットからのロード | ゲームモードが最上位のとき |
| `Back` | `nav::back()` と同じ | 常に |
| `Open(o)` | オーバーレイ `o` を積む | 常に |
| `OpenInGame(o)` | ゲームオーバーレイ `o` を積む | ゲームモードが最上位のとき |

`default_bindings()` は、Enter、Space、マウスの左ボタン、ホイールの下回転、パッドの South を
`Advance` に、Control とパッドの右トリガーを `HoldSkip` に、マウスの右ボタンとパッドの North を
`HideWindow` に、Escape とパッドの East を `Back` に割り当てます。フォーカスの移動と決定は
組み込みの動作で、割り当ては変えられません。

## システムの検査 {#system-checks}

システムの値について量化する解析はないので、システムに決定可能性の問題はありません。
検査するのはモードグラフです。これは、各モードのハンドラが実行しうる操作から、コンパイル時に
組み立てます。ハンドラのクロージャはソースに書かれた有限個しかないので、すべての分岐をたどって
グラフを集められます。

| 検査 | 規則 | 診断 |
| ---- | ---- | ---- |
| 到達可能性 | 一度も表示されないモードには警告を出す | `W6001` |
| 文脈による型付け | ゲームオーバーレイ、セーブ、物語を進める操作、選択は、ゲーム画面とゲームオーバーレイにしかないハンドルのメソッドなので、タイトルページにセーブボタンを置くと型検査で弾かれる | 型エラー |
| 土台からは戻れない | ルートページとゲームモードでは `nav::back()` を使えない | `E6002` |
| 閉じ込めない | 到達できるオーバーレイとダイアログは、すべて戻るかスタックをリセットできなければならない | `E6003` |
| スタックの有界性 | 互いに積み合うモードの循環には警告を出す。スタックがいくらでも伸びうるため | `W6004` |
| エンディングの網羅 | 終了ハンドラは開始シーンの戻り値型のすべての値を扱う | `match` の網羅性 |

ハンドラはチューリング完全なので、いつまでも終わらない可能性もあります。コンパイラはそこまでは
検査しません。

グラフは次のように集めます。

- **モード** とは、いずれかの [モードの種類](#modes) のシグネチャを持つ `fn` です。ゲームモードは
  `System::game`、ルートは `System::start` で、どちらも `main` が返す `System` レコードから
  読み取ります。
- **モードのコード** とは、そのコンポーネントと、そこから推移的に呼ばれる関数や値として使われる
  関数（ハンドラなど）すべてです。操作に渡した関数はその操作の行き先であり、モードのコードには
  含めません。`nav::start` の終了ハンドラは、物語が終わったときに実行されるので、ゲームモードの
  コードとして扱います。
- 行き先が名前の付いた関数でない操作（関数ポインタを保持するローカル変数など）は、その種類の
  モードすべてに到達しうるものとみなします。
- **バインディング** も考慮します。`Action::Open` を割り当てると、どのモードからでもその
  オーバーレイを開けます。`Action::OpenInGame` を割り当てると、ゲームの上にそのゲームオーバーレイを
  開けます。`Action::Back` を割り当てると、すべてのオーバーレイとダイアログから戻る道ができます。
  そのため `default_bindings()` を使えば、閉じ込めるオーバーレイは生じません。バインディングは
  入力であり、モードが別のモードを積むわけではないので、スタックの有界性の検査で循環にはなりません。
- オーバーレイやダイアログからの出口は `nav::back`、`nav::goto`、`nav::start`、`nav::load`、
  `nav::quit` です。

診断の一覧は [ゲームシステム (E6xxx)](../../diagnostics/system.md) にあります。
