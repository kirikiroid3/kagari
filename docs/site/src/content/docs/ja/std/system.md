---
title: "std::system"
---

> **安定性:** 安定 — プログラムの記述、設定、モード型、既見セット、入力の割り当て。

`std::system` はゲームシステムを記述するモジュールです。ゲームシステムとは、タイトル画面、セーブ・ロード画面、設定画面、バックログ、ゲーム中の UI など、物語を取り巻くすべての部分を指します。ゲームの `main` 関数が [`System`](#system) 値を返し、エンジンはその記述どおりにプログラムを実行します。

システムのコードは `fn` で書きます。コンストラクター（`setting`、`volume`、`bind`、`default_bindings`）は `const fn` なので、`static` や `const` の初期化式にも書けます。

サブモジュールは次の 2 つです。

- [`std::system::nav`](./system-nav.md)：モードスタックを操作します。
- [`std::system::saves`](./system-saves.md)：セーブスロットの読み取りと削除を行います。

```kis
pub mod nav;
pub mod saves;
```

プレリュードには `nav`、`Setting`、`setting`、`System`、`Settings`、`Volume`、`volume`、`RootPage`、`Overlay`、`GameOverlay`、`Dialog` が含まれます。それ以外のアイテムは、`use std::system::{seen_image, default_bindings};` のようにパスを指定してインポートしてください。

## 概要 {#overview}

| アイテム | 種別 | 概要 |
| -------- | ---- | ---- |
| [`Setting<T>`](#setting) | 組み込みの構造体 | 永続的でグローバルな設定。セーブとは別に保存 |
| [`setting(init)`](#setting-fn) | `const fn` | 設定を宣言する |
| [`Volume`](#volume)、[`volume(level)`](#volume-fn) | 構造体、`const fn` | バスの音量 |
| [`Settings`](#settings) | 構造体 | エンジンが読む設定ハンドルの一覧 |
| [`RootPage`](#rootpage)、[`Overlay`](#overlay)、[`GameOverlay`](#gameoverlay)、[`Dialog`](#dialog) | 構造体 | モード型 |
| [`System`](#system) | 構造体 | `main` が返すプログラム |
| [`seen_image`](#seen_image)、[`heard`](#heard)、[`watched`](#watched) | `fn` | 既見セット |
| [`set_music`](#set_music) | `fn` | システム音楽チャンネル |
| [`Key`](#key)、[`MouseButton`](#mousebutton)、[`PadButton`](#padbutton)、[`Input`](#input) | 列挙型 | 割り当てられる入力 |
| [`Action`](#action) | 列挙型 | 割り当てる動作 |
| [`Binding`](#binding)、[`bind`](#bind)、[`default_bindings`](#default_bindings) | 構造体、`const fn` | 入力の割り当て |

## 構造体：`Setting<T>` {#setting}

```kis
#[builtin] pub struct Setting<T>;
```

永続的でグローバルな設定を指すハンドルです。値は設定ファイルに保存され、セーブには一切含まれません。そのため、セーブをロードしても巻き戻しても値は変わりません。

設定を宣言するには、不変の `static` を [`setting(init)`](#setting-fn) で初期化します。

```kis
static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static MUSIC: Setting<Volume> = setting(volume(60));
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static PETALS_ON: Setting<bool> = setting(true);
```

- エンジンが読むのは [`Settings`](#settings) に並べたハンドルだけです。上の `PETALS_ON` のように、自作の UI やレンダーで使う設定は制作者がいくつでも宣言できます。
- `fn` のコードからは [`get`](#setting-get) と [`set`](#setting-set) を呼びます。
- `const fn`（レンダー）の中では、ハンドルはただの値です。`Entity::gate`（[`std::screen`](./screen.md) を参照）に渡すと、その `Setting<bool>` が `true` の間だけエンティティを描画できます。`get` と `set` は `fn` なので、レンダーからは呼べません。詳しくは [標準ライブラリに関するコンパイラの規則](./appendix/rules.md) を参照してください。
- 設定の値が物語の制御フローに影響することはありません。そのため、物語の解析結果はどの設定がどんな値でも成り立ちます。物語から書き換えられる [`Meta<T>`](./narrative.md) とはこの点が異なります。

### `setting(init)` {#setting-fn}

```kis
#[builtin] pub const fn setting<T>(init: T) -> Setting<T>;
```

- `init` `T` — 設定ファイルにこの設定の値がないときに使う値。初回起動時や、アップデートで設定が追加されたときなど。
- 戻り値：`Setting<T>` — ハンドル。
- 種別：`const fn` · 組み込み：はい

### `Setting::get()` {#setting-get}

```kis
#[builtin] pub fn get(&self) -> T;
```

- 戻り値：`T` — 現在の値。
- 種別：`fn` · 組み込み：はい

`get` はリアクティブな値の出どころです。`get` を呼んでいるリアクティブな属性や [`effect`](./ui.md#effect) は、値が変わると再実行されます。

### `Setting::set(value)` {#setting-set}

```kis
#[builtin] pub fn set(&self, value: T);
```

- `value` `T` — 新しい値。設定ファイルに書き込まれます。
- 種別：`fn` · 組み込み：はい

```kix
button!{on_click = || FULLSCREEN.set(!FULLSCREEN.get()), t!["Fullscreen"]}
```

## 構造体：`Volume` {#volume}

```kis
pub struct Volume {
    pub level: Int<0, 100>,
    pub muted: bool,
}
```

音量設定の値です。

- `level` — 音量（パーセント）。型が `Int<0, 100>` なので、ここに結び付けた [`slider!`](./ui.md#slider) は範囲を指定しなくても 0〜100 で動きます。
- `muted` — `true` にするとバスを消音します。`level` の値はそのまま残ります。

### `volume(level)` {#volume-fn}

```kis
pub const fn volume(level: Int<0, 100>) -> Volume {
    Volume { level, muted: false }
}
```

- `level` `Int<0, 100>` — 音量（パーセント）。
- 戻り値：`Volume` — 音量が `level` で、消音されていない値。
- 種別：`const fn` · 組み込み：いいえ

## 構造体：`Settings` {#settings}

```kis
pub struct Settings {
    /// Per character; `ZERO` is instant.
    pub text_speed: Setting<Duration>,
    /// The wait after a line in auto mode.
    pub auto_delay: Setting<Duration>,
    /// Added to `auto_delay` per character of the line.
    pub auto_delay_per_char: Setting<Duration>,
    /// Whether skip mode also skips unread text.
    pub skip_unread: Setting<bool>,
    pub master: Setting<Volume>,
    pub music: Setting<Volume>,
    pub sound: Setting<Volume>,
    pub voice: Setting<Volume>,
    /// Whether a voice keeps playing into the next step.
    pub voice_sustain: Setting<bool>,
    /// Whether auto mode waits for the voice.
    pub wait_for_voice: Setting<bool>,
    pub window_opacity: Setting<Int<0, 100>>,
    pub fullscreen: Setting<bool>,
    /// `false`: every duration counts as 0.
    pub animations: Setting<bool>,
}
```

エンジン自身が読む設定ハンドルをまとめた構造体で、[`System::settings`](#system) に渡します。各フィールドには [`setting`](#setting-fn) で宣言したハンドルを入れます。この構造体の役目は、どのハンドルをどの用途に使うかをエンジンに伝えることだけです。

| フィールド | 意味 |
| ---------- | ---- |
| `text_speed` | 1 文字を表示するのにかかる時間。`ZERO` なら 1 行を一度に表示 |
| `auto_delay`、`auto_delay_per_char` | オートモードで 1 行を表示したあとの待ち時間。`auto_delay` に、行の文字数 × `auto_delay_per_char` を足した長さ。オートモードでのクリック待ちの時間にも使う |
| `skip_unread` | スキップモードで、プレイヤーがまだ読んでいないテキストもスキップするかどうか |
| `master`、`music`、`sound`、`voice` | 各バスの音量。[`std::screen::audio`](./screen-audio.md) を参照 |
| `voice_sustain` | 次のステップに進んでもボイスを再生し続けるかどうか |
| `wait_for_voice` | オートモードでボイスの終了を待つかどうか |
| `window_opacity` | テキストウィンドウの不透明度（パーセント） |
| `fullscreen` | フルスクリーン（`true`）か、ウィンドウ表示か |
| `animations` | `false` にすると、すべての時間が 0 として扱われ、アニメーションもトランジションもすぐに終わる |

## モード型 {#mode-types}

システムは **モード** を積み重ねたスタックです。モードの種類は、そのモードの UI を組み立てる `fn`（コンポーネント）の戻り値の型で決まります。どのモード型も、`Overlay(column![…])` のように [`Element`](./ui.md#element) を包んだものです。

| モード | コンポーネントのシグネチャ | 例 | スタック上での扱い |
| ------ | -------------------------- | -- | ---------------- |
| ルートページ | `fn() -> RootPage` | タイトル、ギャラリー、クレジット | 一番下にだけ置ける。ゲームと同時には存在しない |
| ゲームモード | `fn(Narrative) -> Element`（[`System::game`](#system)） | ゲーム画面 | 一番下にだけ置ける |
| オーバーレイ | `fn() -> Overlay` | 設定、ロード | どのモードの上にも積める |
| ゲームオーバーレイ | `fn(Narrative) -> GameOverlay` | セーブ、バックログ | `Narrative::call` で、ゲームの上にだけ積める |
| ダイアログ | `fn() -> Dialog` | 確認 | どのモードの上にも積める。下のモードを静止画にして、その上に描画 |

- 入力を受け取るのは一番上のモードだけです。
- ゲームモードの上にオーバーレイやダイアログがある間は、物語が一時停止し、画面上で再生中のアニメーションは最後まで進めた状態になります。そのため、下のモードは止まったままです。
- `Narrative` ハンドルを受け取れるのはゲーム画面とゲームオーバーレイだけです。そのため、ゲームの実行中でなければできない操作（セーブやバックログを開くなど）をルートページに書くと、型検査でエラーになります。
- モードスタックを操作する関数は [`std::system::nav`](./system-nav.md) にあります。コンパイラは、これらの操作で組み上がるモードの遷移図を検査し、到達できないモード、抜け出せないモード、際限なく積み上がるスタックを検出します。[ゲームシステムの診断](../diagnostics/system.md) を参照してください。

### 構造体：`RootPage` {#rootpage}

```kis
pub struct RootPage(pub Element);
```

モードスタックの一番下に置くページです。タイトル、ギャラリー、クレジットなどが該当します。[`System::start`](#system) で指定するページと、[`nav::goto`](./system-nav.md#goto) の移動先はルートページです。

### 構造体：`Overlay` {#overlay}

```kis
pub struct Overlay(pub Element);
```

[`nav::call`](./system-nav.md#call) や [`Action::Open`](#action) で、どのモードの上にも積めるモードです。設定画面やロード画面に使います。

### 構造体：`GameOverlay` {#gameoverlay}

```kis
pub struct GameOverlay(pub Element);
```

`Narrative::call` や [`Action::OpenInGame`](#action) で、ゲームの上に積むモードです。コンポーネントは実行中のゲームの `Narrative` ハンドルを受け取るので、セーブ、バックログの読み取り、巻き戻しができます。セーブ画面、バックログ、ゲーム中のメニューはゲームオーバーレイとして作ります。

### 構造体：`Dialog` {#dialog}

```kis
pub struct Dialog(pub Element);
```

[`nav::dialog`](./system-nav.md#dialog) で積むモーダルなモードで、確認などに使います。下のモードを静止画にして、その上に描画します。

## 構造体：`System` {#system}

```kis
pub struct System {
    pub title: Text,
    /// The design resolution every pixel value refers to.
    pub resolution: Size,
    pub start: fn() -> RootPage,
    pub game: fn(Narrative) -> Element,
    pub save_slots: u32,
    pub auto_slots: u32,
    /// The maximum number of backlog entries.
    pub backlog: u32,
    /// The default font: the font of text whose style and effects name none.
    pub font: FontAsset,
    /// The default style of `ui::label` and `text!`.
    pub text: TextStyle,
    pub settings: Settings,
    pub bindings: Vec<Binding>,
}
```

プログラム全体を表す構造体です。`main` はルートモジュールに置く、`System` を返す `fn` です。エンジンは起動時に一度だけ `main` を実行します。フィールドはすべて必須です。

| フィールド | 意味 |
| ---------- | ---- |
| `title` | ウィンドウのタイトル |
| `resolution` | 設計上の解像度。レンダーと UI の位置や長さはすべて、この解像度でのピクセル数で指定する |
| `start` | 起動時に最初に表示するルートページ |
| `game` | ゲームモードの画面。`Narrative` ハンドルを受け取り、通常は `game_screen!` とメッセージウィンドウを描画する |
| `save_slots` | 手動セーブのスロット数。`SaveSlot::Manual(0)` 〜 `Manual(save_slots - 1)` |
| `auto_slots` | オートセーブのスロット数。`SaveSlot::Auto(0)` 〜 `Auto(auto_slots - 1)` |
| `backlog` | `Narrative::history` に残す項目の最大数 |
| `font` | 既定のフォント。`TextStyle` でもテキスト効果でもフォントを指定していないテキストに使う。必須 |
| `text` | ボタンとトグル以外で使う [`ui::label`](./ui.md#label) と [`text!`](./ui.md#text) の既定の `TextStyle` |
| `settings` | エンジンが読む設定ハンドル |
| `bindings` | 入力の割り当て |

`main` も通常のコードとして実行されます。そのため、`main` の中の呼び出しも、ほかの呼び出しと同じく再帰の深さに数えられます。

```kix
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
        start: Title,
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

fn Title() -> RootPage {
    RootPage(button!{on_click = || nav::quit(), t!["Quit"]})
}
```

タイトル画面、ゲーム画面、設定画面の完全な例は [ゲームシステムの実例](./guides/system.md) にあります。

## 既見セット {#seen-sets}

既見セットは、エンジンが自動で記録するメタデータです。プレイヤーがこれまでに見た画像、聴いた音声、観たムービーを、すべてのプレイを通して記録します。既読フラグと同じく、ロードや巻き戻しで元に戻ることはありません。リアクティブな値の出どころでもあります。CG ギャラリー、音楽鑑賞、ムービー鑑賞を、制作者が自分で記録を管理しなくても作れます。

### 関数：`seen_image(image)` {#seen_image}

```kis
#[builtin] pub fn seen_image(image: ImageAsset) -> bool;
```

- `image` `ImageAsset` — 画像。
- 戻り値：`bool` — `image` が、いずれかのステップの論理画面に一度でも載ったかどうか。画像レイヤーの種類は問わない。
- 種別：`fn` · 組み込み：はい

### 関数：`heard(audio)` {#heard}

```kis
#[builtin] pub fn heard(audio: AudioAsset) -> bool;
```

- `audio` `AudioAsset` — トラックまたはサウンド。
- 戻り値：`bool` — `audio` が一度でも再生されたかどうか。チャンネルで `Loop` が始まった場合と、単発のサウンドが鳴った場合が該当する。
- 種別：`fn` · 組み込み：はい

### 関数：`watched(video)` {#watched}

```kis
#[builtin] pub fn watched(video: VideoAsset) -> bool;
```

- `video` `VideoAsset` — ムービー。
- 戻り値：`bool` — `video` が画面上で一度でも再生を開始したかどうか（音声の有無は問わない）。セーブのロード中にステップを再実行しても、ムービーは記録されない。
- 種別：`fn` · 組み込み：はい

## 関数：`set_music(track, fade)` {#set_music}

```kis
#[builtin] pub fn set_music(track: Option<crate::screen::audio::Loop>, fade: crate::screen::audio::AudioFade);
```

- `track` `Option<Loop>` — 再生するトラック。`None` ならシステム音楽を止める。
- `fade` `AudioFade` — 古いトラックから新しいトラックへの切り替え方。例：`crossfade(ms(500))`。
- 種別：`fn` · 組み込み：はい

**システム音楽チャンネル** を設定します。このチャンネルが鳴るのは、モードスタック上でゲームが動いていない間（タイトル画面や音楽鑑賞画面など）です。ゲームの実行中は、代わりに物語の画面の音楽チャンネルが鳴ります。`fn` なので、呼べるのはシステムのコードだけです。`Loop`、`looped`、`AudioFade` については [`std::screen::audio`](./screen-audio.md) を参照してください。

> **安定性:** 予定 — 現在の再生内容（再生中のトラックや、音楽鑑賞画面での再生位置・一時停止などの状態）を読み取る方法は、まだ設計していません。

## 入力の割り当て {#bindings}

[`System::bindings`](#system) は、入力と動作の対応を表します。型は `Vec<Binding>` なので、[`default_bindings()`](#default_bindings) を元に、項目を追加したり置き換えたりして使います。

**フォーカス操作のキーは組み込みで、割り当てを変えられません。** 矢印キー、十字キー、Tab、Shift+Tab でコントロール間のフォーカスを移し、Enter と `PadButton::South` でフォーカス中のコントロールを実行します（[フォーカス](./ui.md#focus) を参照）。

### 列挙型：`Key` {#key}

```kis
pub enum Key {
    Enter,
    Escape,
    Space,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Shift,
    Control,
    Alt,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
}
```

キーボードのキーです。Enter、Escape、Space、Backspace、Tab、矢印キー、PageUp/PageDown、Home/End、修飾キー（Shift、Control、Alt）、F1〜F12、英字 A〜Z、数字 `Digit0`〜`Digit9` があります。修飾キーも単独のキーとして扱い、キーの組み合わせはありません。

### 列挙型：`MouseButton` {#mousebutton}

```kis
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    WheelUp,
    WheelDown,
}
```

マウスのボタン、またはホイールの回転方向です。`Back` と `Forward` はサイドボタンです。

### 列挙型：`PadButton` {#padbutton}

```kis
pub enum PadButton {
    South,
    East,
    West,
    North,
    LeftShoulder,
    RightShoulder,
    LeftTrigger,
    RightTrigger,
    Start,
    Select,
    LeftStick,
    RightStick,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}
```

ゲームパッドのボタンです。名前はボタンの刻印ではなく位置を表します。`South`、`East`、`West`、`North` は、それぞれ下・右・左・上のフェイスボタンです。`LeftStick` と `RightStick` はスティックの押し込みです。`PadButton::South` と十字キーはフォーカス操作にも使います（[フォーカス](./ui.md#focus) を参照）。

### 列挙型：`Input` {#input}

```kis
pub enum Input {
    Key(Key),
    Mouse(MouseButton),
    Pad(PadButton),
}
```

割り当ての対象にできる入力です。

### 列挙型：`Action` {#action}

```kis
pub enum Action {
    Advance,
    HoldSkip,
    ToggleSkip,
    ToggleAuto,
    HideWindow,
    QuickSave,
    QuickLoad,
    Back,
    Open(fn() -> Overlay),
    OpenInGame(fn(Narrative) -> GameOverlay),
}
```

入力に割り当てる動作です。

| 動作 | 効果 | 有効になる条件 |
| ---- | ---- | ---------- |
| `Advance` | `NextStep` を送る。ステップのタイムラインが再生中なら、先にそれを最後まで進める | ゲームモードが一番上 |
| `HoldSkip` | 入力を押している間だけスキップモード | ゲームモードが一番上 |
| `ToggleSkip`、`ToggleAuto` | スキップモード、オートモードのオン・オフを切り替える | ゲームモードが一番上 |
| `HideWindow` | テキストウィンドウを隠す | ゲームモードが一番上 |
| `QuickSave`、`QuickLoad` | [`SaveSlot::Quick`](./system-saves.md#saveslot) にセーブする、またはそこからロードする | ゲームモードが一番上 |
| `OpenInGame(o)` | ゲームオーバーレイ `o` を積む | ゲームモードが一番上 |
| `Back` | [`nav::back()`](./system-nav.md#back)。一番下のモードでは何もしない | 常に |
| `Open(o)` | オーバーレイ `o` を積む | 常に |

`Action::Back` の割り当てがあれば、[`E6003`](../diagnostics/system.md) の検査では、どのモードにもそこから抜け出す手段があるとみなされます。

### 構造体：`Binding` {#binding}

```kis
pub struct Binding {
    pub input: Input,
    pub action: Action,
}
```

入力と動作の組です。

### 関数：`bind(input, action)` {#bind}

```kis
pub const fn bind(input: Input, action: Action) -> Binding {
    Binding { input, action }
}
```

- `input` `Input` — 入力。
- `action` `Action` — 動作。
- 戻り値：`Binding` — `input` に `action` を割り当てた組。
- 種別：`const fn` · 組み込み：いいえ

### 関数：`default_bindings()` {#default_bindings}

```kis
pub const fn default_bindings() -> Vec<Binding> {
    let mut bindings = Vec::new();
    bindings.push(bind(Input::Key(Key::Enter), Action::Advance));
    bindings.push(bind(Input::Key(Key::Space), Action::Advance));
    bindings.push(bind(Input::Mouse(MouseButton::Left), Action::Advance));
    bindings.push(bind(Input::Mouse(MouseButton::WheelDown), Action::Advance));
    bindings.push(bind(Input::Pad(PadButton::South), Action::Advance));
    bindings.push(bind(Input::Key(Key::Control), Action::HoldSkip));
    bindings.push(bind(Input::Pad(PadButton::RightTrigger), Action::HoldSkip));
    bindings.push(bind(Input::Mouse(MouseButton::Right), Action::HideWindow));
    bindings.push(bind(Input::Pad(PadButton::North), Action::HideWindow));
    bindings.push(bind(Input::Key(Key::Escape), Action::Back));
    bindings.push(bind(Input::Pad(PadButton::East), Action::Back));
    bindings
}
```

- 戻り値：`Vec<Binding>` — 下の表の既定の割り当て。
- 種別：`const fn` · 組み込み：いいえ

| 入力 | 動作 |
| ---- | ---- |
| `Key::Enter`、`Key::Space`、`MouseButton::Left`、`MouseButton::WheelDown`、`PadButton::South` | `Advance` |
| `Key::Control`、`PadButton::RightTrigger` | `HoldSkip` |
| `MouseButton::Right`、`PadButton::North` | `HideWindow` |
| `Key::Escape`、`PadButton::East` | `Back` |

既定の割り当てを `const fn` の中で拡張する例です。

```kis
use std::system::{Action, Binding, Input, Key, MouseButton, bind, default_bindings};

const fn bindings() -> Vec<Binding> {
    let mut list = default_bindings();
    list.push(bind(Input::Key(Key::A), Action::ToggleAuto));
    list.push(bind(Input::Key(Key::L), Action::OpenInGame(Backlog)));
    list.push(bind(Input::Mouse(MouseButton::WheelUp), Action::OpenInGame(Backlog)));
    list
}
```

> **安定性:** 予定 — 入力を任意の `fn` に割り当てること（フルスクリーン切り替えなど）と、タッチ入力にはまだ対応していません。
