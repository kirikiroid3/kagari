---
title: "ゲームシステム"
---

ゲームシステムとは、物語の周りにあるものすべてです：タイトル画面、セーブとロードのページ、設定、
バックログ、ゲーム内 UI。これらは `fn` と `const fn` のコードで書き、[呼び出し規則](./kinds.md#call-rules)
以外の制限はありません。システムは物語を呼び出せず、開始することだけができ、物語変数も読めません。

この章ではシステムのモデルを説明します：入れ子になった二つの機械、プログラムの記述、UI
コンポーネント、モード、`Narrative` ハンドル、モードグラフの検査です。ここで出てくるアイテムの
仕様は STD リファレンスの [`std::system`](../../std/system.md)、
[`std::system::nav`](../../std/system-nav.md)、
[`std::system::saves`](../../std/system-saves.md)、
[`std::narrative`](../../std/narrative.md)、
[`std::ui`](../../std/ui.md) にあります。

## 入れ子になった二つの機械 {#two-nested-machines}

ゲーム全体は、一つのスタックを共有する一つの機械ではなく、入れ子になった二つの機械です。

- 外側の **システム機械** は **モード** のスタックを持ちます：ルートページ（タイトル、ギャラリー、
  クレジット）、オーバーレイ（セーブ、ロード、コンフィグ、バックログ）、ダイアログ、そして
  ゲームモードです。
- ゲームモードは実行中の物語、つまり **物語機械** を一つ保持します：位置、コールスタック、
  物語変数、画面、PRNG の状態、トレースです。システムのコードからは
  [`Narrative` ハンドル](#the-narrative-handle) を通してしか見えません。

<!-- diagram: system-machines (mermaid-ish)
flowchart LR
  subgraph sys["システム機械：モードスタック（上が先頭）"]
    D["Dialog（例：確認）"]
    O["GameOverlay（例：セーブ、バックログ）"]
    subgraph G["ゲームモード（土台）：System::game ビュー"]
      S["物語機械：位置、コールスタック、物語変数、論理画面、PRNG の状態、選択のトレース。Narrative 経由でのみ見える"]
    end
    D ==> O ==> G
  end
  Settings["設定：グローバル、セーブ外"]
  Meta["メタ：グローバル、セーブ外"]
  S ==>|セーブ| Slots["セーブスロット：物語のトレースを保持"]
-->
{{#include ../diagrams/system-machines.svg}}

物語のコールスタックはゲームモードの中にあり、システムのスタックには決して載りません。ここから
三つの性質が導かれます。

- **セーブはシステムの状態を含められません。** セーブが記録するのは実行中の物語だけです。
  メニューのフレームや設定はそこから辿れません。
- **ロードはプッシュではなく置き換えです。** ロードは復元した物語を新しいゲームフレームに入れます。
  先に巻き戻すべきメニューのスタックはありません。
- **「ゲームが実行中」はスタックの性質です。** ゲームフレームが存在するとき、かつそのときに限り
  成り立ちます。

| 保存先 | 寿命 | セーブに含まれるか |
| ------ | ---- | ------------------ |
| 設定 | 永続、グローバル（設定ファイル） | いいえ |
| メタ | 永続、グローバル（メタファイル：解放状況、既読フラグ、既見集合） | いいえ |
| セーブスロット | 永続 | それ自体がセーブ |
| モードスタックとモードローカルの状態 | 一時的、起動時にリセット | いいえ |
| 実行中の物語 | 一時的、ゲームフレーム内 | はい、トレースとして。[セーブ、再生、決定性](./saves.md) を参照 |

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

`main` はルートモジュールにある、`System` を返す `fn` です。システムを記述し、エンジンが起動時に
一度だけ実行します。`main` のないプログラムはコンパイルできません。

| フィールド | 意味 |
| ---------- | ---- |
| `title` | ウィンドウタイトル |
| `resolution` | 設計解像度。レンダーと UI の位置や長さはこの解像度のピクセル単位で、ゲーム全体で固定 |
| `start` | システムが最初に表示するルートページ。`fn() -> RootPage` |
| `game` | ゲームモードのビュー。`fn(Narrative) -> Element` |
| `save_slots`、`auto_slots` | 手動セーブとオートセーブのスロット数。固定なのでスロット一覧は有限 |
| `backlog` | バックログが保持するエントリーの最大数 |
| `font` | 既定のフォント。スタイルにも効果にもフォント指定のないテキストに使う |
| `text` | `ui::label` と `text!` の既定のテキストスタイル |
| `settings` | エンジンの設定のハンドル。[設定とメタ](#settings-and-meta) を参照 |
| `bindings` | 入力のバインディング。[バインディング](#bindings) を参照 |

`main` は普通のコードとして実行されるので、実行時のすべての呼び出しと同じく、その呼び出しも
再帰の深さに数えられます。既定では入れ子の呼び出しは最大 2048 段で、
[`kagari.toml`](../tools/projects.md) の `recursion-depth` キーで設定します。これより深い再帰は
実行時エラーとしてゲームを停止させます。

**システム音楽。** `std::system::set_music(track, fade)` はシステム音楽チャンネルを設定します。
これはタイトルページのように、モードスタック上でゲームが実行されていない間に流れます。
ゲームの実行中は、代わりにゲーム画面の音楽チャンネルが流れます。`None` で停止します。これは `fn`
なので、呼べるのはシステムのコードだけです。

## コンポーネントとシグナル {#components-and-signals}

UI は `std::ui` の UI フォームを使って `fn` のコードで書き、SolidJS と同じセマンティクスを
持ちます。

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

- UI フォームは物語のフォームと同じ [区切り記号の規則](../syntax/forms.md) に従います。`fn` と
  `const fn` のコードの中では **値**（`Element` 型）を作り、ステップにはなりません。
- **コンポーネント** は UI の値を返す普通の `fn` です。モードが表示されたときに **一度だけ**
  実行されます。他の関数と同じように呼ぶか、`nav::call(Config)` のようにポインタとして渡します。
- `signal(init)` はゲッターとセッターを返します。シグナルは `fn` のコードでは第一級ですが、
  物語や `const fn` では使えません。
- シグナルを読む属性値や要素は **リアクティブ** で、シグナルが変わるたびに再評価されます。
  コンポーネントのそれ以外のコードは一度だけ実行されます。`Setting::get`、`Meta::get`、
  `Narrative` ハンドルの読み取り、セーブ情報、既見集合もリアクティブな情報源です。
- `on_click` のようなハンドラは第一級の [クロージャ](../syntax/values.md) です。
- スライダーの範囲は、その値の `Int` 型の範囲です。`Volume::level` は `Int<0, 100>` なので、
  上のスライダーは `min` や `max` 属性なしで 0 から 100 までになります。

UI フォームは `std::ui` から来ます：レイアウト（`column![…]`、`row![…]`、`layer![…]`）、
内容（`text!(…)`、`image!(…)`、`game_screen!(…)`）、コントロール（`button!{…}`、
`slider!{…}`、`toggle!{…}`、`select!{…}`）。各フォームは props 構造体を伴う `std::ui` 関数の
呼び出しに脱糖されます。リアクティブな属性はクロージャになり、ハンドラはそのまま残り、各要素は
クロージャになります。脱糖の表、レイアウト、操作時の見た目、フォーカスの規則は
[`std::ui`](../../std/ui.md) にあります。

## モード {#modes}

モードの種類は、そのコンポーネントの戻り値の型で決まります。各モード型は `Overlay(column![…])`
のように `Element` を包みます。

| 型 | コンポーネント | モードの例 | スタック上の位置 |
| -- | -------------- | ---------- | ---------------- |
| `RootPage` | `fn() -> RootPage` | タイトル、ギャラリー、クレジット | 土台としてのみ。ゲームフレームとは共存しない |
| `Overlay` | `fn() -> Overlay` | コンフィグ、ロード | 何の上にもプッシュできる |
| `GameOverlay` | `fn(Narrative) -> GameOverlay` | セーブ、バックログ | ゲームモードの上にのみプッシュできる |
| `Dialog` | `fn() -> Dialog` | 確認 | オーバーレイと同様。下のフレームの静止画の上に描かれる |
| ゲームモード | `fn(Narrative) -> Element`、`System::game` ビュー | 実行中の物語 | 土台としてのみ |

入力は最上位のフレームだけに届きます。ゲームモードの上にオーバーレイやダイアログがある間、物語は
一時停止し、画面で実行中のアニメーションは最終状態までスキップされるので、下のフレームは常に
静止しています。

## 操作 {#operations}

UI がシステムを変えるのは次の操作を通してだけです。`std::system::nav` のナビゲーション関数は
どの `fn` からでも呼べます。

| 操作 | 効果 | 条件 |
| ---- | ---- | ---- |
| `nav::call(o)` | オーバーレイ `o` をプッシュ | |
| `nav::dialog(d)` | ダイアログ `d` をプッシュ | |
| `nav::back()` | 最上位のフレームをポップ | 土台では無視される |
| `nav::goto(p)` | スタックをルートページ `p` にリセット | |
| `nav::start(e, args, on_exit)` | スタックを、エントリーシーン `e` を実行する新しいゲームにリセット | |
| `nav::load(k)` | スタックを、スロット `k` から復元したゲームにリセット | セーブがスクリプトに合わなくなっていれば `Err` を返し、スタックは変えない |
| `nav::quit()` | 確認なしで即座に終了 | |

実行中のゲームに対する操作は [`Narrative` ハンドル](#the-narrative-handle) のメソッドです。

| 操作 | 効果 |
| ---- | ---- |
| `game.advance()` | 物語を次のステップに進める |
| `game.select(arm)` | 現在の選択肢の、表示されているアームを選ぶ |
| `game.save(k)` | 実行中のゲームをスロット `k` にセーブ |
| `game.rewind(i)` | 物語をバックログのエントリー `i` まで戻す |
| `game.call(o)` | ゲームオーバーレイ `o` をプッシュ |

`nav::quit` は確認しません。確認が必要なボタンは、スタジオが書いた `Dialog` を開きます。

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

`nav::start` は [エントリーシーン](./entry.md)、その引数、終了ハンドラを受け取ります。これが
物語に入る唯一の方法です。

- `story::Main` は `#[entry]` シーン `scene fn(P1, …, Pn) -> R` へのポインタ、引数はタプル
  `(P1, …, Pn)`、終了ハンドラは `Fn(R)` です。
- 引数の型はエントリーシーンの有限なパラメータ型です。引数はセーブに記録されます。
- `nav::start` のたびに物語の PRNG が新しくシードされます。[決定性](./saves.md#determinism) を
  参照してください。
- エントリーシーンが戻ると、その値が終了ハンドラに渡されます。ハンドラはすべての値を扱わなければ
  なりません。これは普通の網羅的な `match` です。ハンドラがどこにも遷移しなければ、スタックは
  開始ページにリセットされます。

おまけページからのシーンの **回想** は、別の `#[entry]` シーンを `nav::start` することです。
その解放には、物語がそのシーンを初めて通ったときに書き込む `Meta<bool>` を使います。

## 設定とメタ {#settings-and-meta}

```kis
static MUSIC: Setting<Volume> = setting(volume(60));
static PETALS_ON: Setting<bool> = setting(true);
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

`Setting<T>` または `Meta<T>` を保持する不変の `static` は、設定またはメタのフィールドを宣言します。
初期化子は、保存されたファイルにその値がないとき（初回起動時や、アップデートでフィールドが
追加されたときなど）に使われる値です。どちらも永続的かつグローバルで、セーブの外に保存されます。

| メソッド | 関数種別 | 設定 | メタ |
| -------- | -------- | ---- | ---- |
| `get()` | `fn` | あり | あり |
| `set(v)` | `fn` | あり | あり |
| `write(v)` | `pushdown fn` | なし | あり：物語がメタに書き込む。書き込み専用 |
| ハンドルそのもの | `const fn` | [遅延参照](./presentation.md#settings-in-renders)。`Entity::gate` に渡す | なし |

したがって設定が物語の制御フローに届くことはなく、物語のすべての解析は設定のどの値についても
成り立ちます。メタが物語に届くのは、開始時のエントリー引数を通してだけです。

エンジン自身の設定は、`System::settings` に渡す `Settings` レコードのハンドルです：テキスト速度、
オートモードの待ち時間、スキップで未読テキストも飛ばすか、マスター・音楽・効果音・ボイスの音量、
ボイスの継続再生とオートモードでのボイス待ち、ウィンドウの不透明度、フルスクリーン、そして
アニメーションを再生するか（オフのときはすべての長さが 0 として扱われます）。音量は `Volume`
で、レベル `Int<0, 100>` とミュートフラグからなります。スタジオは `PETALS_ON` のように、独自の
設定を自分のハンドルとして宣言します。

既読フラグは、物語が進むたびにエンジンがステップをキーとして書き込むメタです。物語からは
見えません。システムは既読スキップと、バックログで既読の行に印を付けるために使います。

**既見集合** もエンジンが同じように管理するメタです。`std::system` の `seen_image(i)`、
`heard(a)`、`watched(v)` は、画像がステップの論理画面に載ったことがあるか、曲や効果音が再生された
ことがあるか、動画が再生されたことがあるかを返します。既読フラグと同じく、ロードや巻き戻しで
元に戻ることはありません。スタジオ側で記録しなくても、CG ギャラリー、音楽室、動画室を作れます。

```kix
use std::system::seen_image;

fn GalleryThumb(cg: ImageAsset) -> Element {
    button!{disabled = !seen_image(cg), image!(cg)}
}
```

## `Narrative` ハンドル {#the-narrative-handle}

システムが実行中の物語を見るのは、`std::narrative` の `Narrative` ハンドルを通してだけです。
エンジンはこれをゲームビュー（`System::game`）とゲームオーバーレイにだけ渡します。内部表現は
隠されているので、システムのコードは物語変数を読めず、システムが何をしても物語の解析には影響
しません。すべてのメソッドは `fn` で、読み取りはリアクティブです。

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
文字送り、効果、キューとともに描きます。レコード `Line`、`Menu`、`HudState`、`Entry`、`PlayMode`
の仕様は [`std::narrative`](../../std/narrative.md) にあります。

ハンドルはゲームビューとゲームオーバーレイにしか存在しないので、セーブ、物語を進める操作、
セーブ画面のオーバーレイを開く操作など、実行中のゲームを必要とするものはタイトルページには
書くことすらできません。セーブ、ロード、巻き戻しは [セーブ、再生、決定性](./saves.md) で
説明します。

## バインディング {#bindings}

`System::bindings` は入力（キー、マウスボタンとホイール、パッドのボタン）をアクションに
対応付けます。

| アクション | 効果 | 有効な場面 |
| ---------- | ---- | ---------- |
| `Advance` | 物語を進める。ウィンドウが隠れていれば、代わりに表示する | ゲームモードが最上位のとき |
| `HoldSkip`、`ToggleSkip` | 押している間スキップ／スキップモードの切り替え | ゲームモードが最上位のとき |
| `ToggleAuto` | オートモードの切り替え | ゲームモードが最上位のとき |
| `HideWindow` | テキストウィンドウの非表示の切り替え | ゲームモードが最上位のとき |
| `QuickSave`、`QuickLoad` | クイックスロットへのセーブ／からのロード | ゲームモードが最上位のとき |
| `Back` | `nav::back()` と同じ | どこでも |
| `Open(o)` | オーバーレイ `o` をプッシュ | どこでも |
| `OpenInGame(o)` | ゲームオーバーレイ `o` をプッシュ | ゲームモードが最上位のとき |

`default_bindings()` は Enter、Space、マウス左ボタン、ホイール下、パッドの South を `Advance` に、
Control とパッドの右トリガーを `HoldSkip` に、マウス右ボタンとパッドの North を `HideWindow` に、
Escape とパッドの East を `Back` に割り当てます。フォーカスの移動と決定は組み込みで、
バインドできません。

## システムの検査 {#system-checks}

システムには決定可能性の問題がありません。システムの値について量化する解析はないからです。
検査の対象はモードグラフで、各モードのハンドラが実行しうる操作からコンパイル時に作られます。
ハンドラのクロージャは有限個でソースに書かれているので、すべての分岐にわたってグラフを集められます。

| 検査 | 規則 | 診断 |
| ---- | ---- | ---- |
| 到達可能性 | 決して表示されないモードは警告 | `W6001` |
| 文脈の型付け | ゲームオーバーレイ、セーブ、物語を進める操作、選択は、ゲームビューとゲームオーバーレイにしか存在しないハンドルのメソッドなので、タイトルページのセーブボタンは型検査を通らない | 型エラー |
| 土台からは戻れない | ルートページとゲームモードでは `nav::back()` を使えない | `E6002` |
| 閉じ込めない | 到達可能なオーバーレイとダイアログはすべて、戻るかスタックをリセットできる | `E6003` |
| 有界なスタック | 互いに循環してプッシュし合うモードは警告。スタックが際限なく伸びうる | `W6004` |
| 網羅的なエンディング | 終了ハンドラはエントリーシーンの戻り値型のすべての値を扱う | `match` の網羅性 |

ハンドラはチューリング完全なので、永遠に走り続けることもありえます。コンパイラはそれを検査しません。

グラフは次のように集められます。

- **モード** とは、いずれかの [モードの種類](#modes) のシグネチャを持つ `fn` です。ゲームモードは
  `System::game`、ルートは `System::start` で、どちらも `main` が返す `System` レコードから
  読み取られます。
- **モードのコード** とは、そのコンポーネントと、そこから推移的に呼ばれるか値として使われる
  （ハンドラなど）すべての関数です。操作に渡された関数はその操作の行き先であり、モードのコード
  ではありません。`nav::start` の終了ハンドラはゲームモードのコードです。物語が終わったときに
  実行されるからです。
- 行き先が名前の付いた関数でない操作（ポインタを保持するローカル変数など）は、その種類の
  すべてのモードに到達しうるとみなされます。
- **バインディング** も数えられます。`Action::Open` のバインディングはどのモードからでもその
  オーバーレイを開き、`Action::OpenInGame` のバインディングはゲームの上にそのゲームオーバーレイを
  開き、`Action::Back` のバインディングはすべてのオーバーレイとダイアログからの戻り道になります。
  したがって `default_bindings()` を使えば、閉じ込めるオーバーレイはありません。バインディングは
  入力であってモード同士のプッシュではないので、有界なスタックの検査で循環を作ることはありません。
- オーバーレイやダイアログからの出口は `nav::back`、`nav::goto`、`nav::start`、`nav::load`、
  `nav::quit` です。

診断の一覧は [ゲームシステム (E6xxx)](../../diagnostics/system.md) にあります。
