# `std::system` {#stdsystem}

> **安定性:** 安定 — プログラムの記述、設定、モード型、既見セット、入力バインディング。

`std::system` はゲームシステムを記述します。ゲームシステムとは、タイトル画面、セーブ・ロード画面、設定画面、バックログ、ゲーム内 UI など、物語の周りにあるすべてのものです。ゲームの `main` 関数は [`System`](#system) 値を返し、エンジンはそれが記述するプログラムを実行します。

システムのコードは `fn` です。コンストラクタ（`setting`、`volume`、`bind`、`default_bindings`）は `const fn` なので、`static` や `const` の初期化式に書けます。

このモジュールには 2 つのサブモジュールがあります。

- [`std::system::nav`](./system-nav.md) はモードスタックを変更します。
- [`std::system::saves`](./system-saves.md) はセーブスロットを読み取り、削除します。

```kis
pub mod nav;
pub mod saves;
```

プレリュードは `nav`、`Setting`、`setting`、`System`、`Settings`、`Volume`、`volume`、`RootPage`、`Overlay`、`GameOverlay`、`Dialog` をインポートします。その他のアイテムは `use std::system::{seen_image, default_bindings};` のようにパスでインポートしてください。

## 概要 {#overview}

| アイテム | 種別 | 概要 |
| -------- | ---- | ---- |
| [`Setting<T>`](#setting) | ビルトイン構造体 | セーブの外に保存される、永続的でグローバルな設定 |
| [`setting(init)`](#setting-fn) | `const fn` | 設定を宣言する |
| [`Volume`](#volume)、[`volume(level)`](#volume-fn) | 構造体、`const fn` | バスの音量 |
| [`Settings`](#settings) | 構造体 | エンジンが読む設定ハンドル |
| [`RootPage`](#rootpage)、[`Overlay`](#overlay)、[`GameOverlay`](#gameoverlay)、[`Dialog`](#dialog) | 構造体 | モード型 |
| [`System`](#system) | 構造体 | `main` が返すプログラム |
| [`seen_image`](#seen_image)、[`heard`](#heard)、[`watched`](#watched) | `fn` | 既見セット |
| [`set_music`](#set_music) | `fn` | システム音楽チャンネル |
| [`Key`](#key)、[`MouseButton`](#mousebutton)、[`PadButton`](#padbutton)、[`Input`](#input) | 列挙型 | バインドできる入力 |
| [`Action`](#action) | 列挙型 | バインディングの動作 |
| [`Binding`](#binding)、[`bind`](#bind)、[`default_bindings`](#default_bindings) | 構造体、`const fn` | 入力バインディング |

## 構造体: `Setting<T>` {#setting}

```kis
#[builtin] pub struct Setting<T>;
```

永続的でグローバルな設定へのハンドルです。値はコンフィグファイルに保存され、セーブには決して保存されません。そのため、セーブのロードや巻き戻しで値が変わることはありません。

初期化式が [`setting(init)`](#setting-fn) である不変の `static` が設定を宣言します。

```kis
static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static MUSIC: Setting<Volume> = setting(volume(60));
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static PETALS_ON: Setting<bool> = setting(true);
```

- エンジンは [`Settings`](#settings) に挙げられたハンドルを読みます。スタジオは上の `PETALS_ON` のように、独自の UI やレンダー用の設定をいくつでも宣言できます。
- `fn` のコードでは [`get`](#setting-get) と [`set`](#setting-set) を呼びます。
- `const fn`（レンダー）の中では、ハンドルは単なる値です。`Entity::gate`（[`std::screen`](./screen.md) を参照）に渡すことができ、`Setting<bool>` が `true` の間だけエンティティを描画させます。`get` と `set` は `fn` なので、そこでは呼べません。[コンパイラ規則](./appendix/rules.md) を参照してください。
- 設定は物語の制御フローに決して届かないため、物語の解析はどの設定のどの値に対しても成り立ちます。物語から書き込める [`Meta<T>`](./narrative.md) と比較してください。

### `setting(init)` {#setting-fn}

```kis
#[builtin] pub const fn setting<T>(init: T) -> Setting<T>;
```

- `init` `T` — コンフィグファイルにこの設定の値がないとき（初回起動時や、アップデートで設定が追加されたときなど）に使う値。
- 戻り値: `Setting<T>` — ハンドル。
- 種別: `const fn` · ビルトイン: はい

### `Setting::get()` {#setting-get}

```kis
#[builtin] pub fn get(&self) -> T;
```

- 戻り値: `T` — 現在の値。
- 種別: `fn` · ビルトイン: はい

`get` はリアクティブなソースです。これを呼ぶリアクティブな属性や [`effect`](./ui.md#effect) は、値が変わると再実行されます。

### `Setting::set(value)` {#setting-set}

```kis
#[builtin] pub fn set(&self, value: T);
```

- `value` `T` — 新しい値。コンフィグファイルに書き込まれます。
- 種別: `fn` · ビルトイン: はい

```kix
button!{on_click = || FULLSCREEN.set(!FULLSCREEN.get()), t!["Fullscreen"]}
```

## 構造体: `Volume` {#volume}

```kis
pub struct Volume {
    pub level: Int<0, 100>,
    pub muted: bool,
}
```

音量設定の値です。

- `level` — パーセント単位の音量。`Int<0, 100>` なので、これにバインドした [`slider!`](./ui.md#slider) は範囲の属性なしで 0 から 100 まで動きます。
- `muted` — `true` にすると、`level` を保ったままバスを消音します。

### `volume(level)` {#volume-fn}

```kis
pub const fn volume(level: Int<0, 100>) -> Volume {
    Volume { level, muted: false }
}
```

- `level` `Int<0, 100>` — パーセント単位の音量。
- 戻り値: `Volume` — `level` の、消音されていない音量。
- 種別: `const fn` · ビルトイン: いいえ

## 構造体: `Settings` {#settings}

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

エンジン自身が読む設定ハンドルで、[`System::settings`](#system) に渡します。各フィールドは [`setting`](#setting-fn) で宣言したハンドルです。この構造体は、どのハンドルがどの役割を持つかをエンジンに伝えるだけです。

| フィールド | 意味 |
| ---------- | ---- |
| `text_speed` | 1 文字あたりの表示時間。`ZERO` なら行を一度に表示します |
| `auto_delay`、`auto_delay_per_char` | オートモードで行の後に待つ時間。`auto_delay` に、行の 1 文字ごとに `auto_delay_per_char` を足したものです。オートモードでのクリック待ちポーズの待ち時間でもあります |
| `skip_unread` | スキップモードがプレイヤーの未読テキストもスキップするかどうか |
| `master`、`music`、`sound`、`voice` | バスの音量。[`std::screen::audio`](./screen-audio.md) を参照してください |
| `voice_sustain` | ボイスが次のステップまで再生され続けるかどうか |
| `wait_for_voice` | オートモードがボイスの終了を待つかどうか |
| `window_opacity` | テキストウィンドウの不透明度（パーセント） |
| `fullscreen` | フルスクリーン（`true`）かウィンドウか |
| `animations` | `false` のとき、すべての時間が 0 とみなされ、アニメーションとトランジションは即座に終わります |

## モード型 {#mode-types}

システムは **モード** のスタックです。モードの種類は、その UI を組み立てる `fn` であるコンポーネントの戻り値の型で決まります。各モード型は `Overlay(column![…])` のように [`Element`](./ui.md#element) を包みます。

| モード | コンポーネントのシグネチャ | 例 | スタック上の位置 |
| ------ | -------------------------- | -- | ---------------- |
| ルートページ | `fn() -> RootPage` | タイトル、ギャラリー、クレジット | ベースのみ。ゲームと同時には存在しません |
| ゲームモード | `fn(Narrative) -> Element`（[`System::game`](#system)） | ゲームビュー | ベースのみ |
| オーバーレイ | `fn() -> Overlay` | コンフィグ、ロード | 任意のモードの上に積まれます |
| ゲームオーバーレイ | `fn(Narrative) -> GameOverlay` | セーブ、バックログ | `Narrative::call` によって、ゲームのベースの上にだけ積まれます |
| ダイアログ | `fn() -> Dialog` | 確認 | 任意のモードの上に積まれ、下のモードの静止画の上に描画されます |

- 入力は最上位のモードにだけ届きます。
- オーバーレイやダイアログがゲームモードの上にある間、物語は一時停止し、画面で実行中のアニメーションは完了させられます。そのため、下のモードは静止しています。
- `Narrative` ハンドルを受け取るのはゲームビューとゲームオーバーレイだけです。そのため、実行中のゲームを必要とする操作（セーブ、バックログを開く）はルートページには書けず、型検査を通りません。
- モードスタックを変更する操作は [`std::system::nav`](./system-nav.md) にあります。コンパイラは、これらの操作が作るモードのグラフ（到達不能なモード、抜け出せないモード、非有界なスタック）を検査します。[ゲームシステムの診断](../../../reference/ja/src/diagnostics/system.md) を参照してください。

### 構造体: `RootPage` {#rootpage}

```kis
pub struct RootPage(pub Element);
```

モードスタックのベースとなるページで、タイトル、ギャラリー、クレジットなどです。[`System::start`](#system) のページと [`nav::goto`](./system-nav.md#goto) の行き先はルートページです。

### 構造体: `Overlay` {#overlay}

```kis
pub struct Overlay(pub Element);
```

[`nav::call`](./system-nav.md#call) または [`Action::Open`](#action) によって任意のモードの上に積まれるモードで、コンフィグ画面やロード画面などです。

### 構造体: `GameOverlay` {#gameoverlay}

```kis
pub struct GameOverlay(pub Element);
```

`Narrative::call` または [`Action::OpenInGame`](#action) によってゲームの上に積まれるモードです。コンポーネントは実行中のゲームの `Narrative` ハンドルを受け取るので、セーブ、バックログの読み取り、巻き戻しができます。セーブ画面、バックログ、ゲーム内メニューはゲームオーバーレイです。

### 構造体: `Dialog` {#dialog}

```kis
pub struct Dialog(pub Element);
```

[`nav::dialog`](./system-nav.md#dialog) によって積まれるモーダルなモードで、確認などに使います。下のモードの静止画の上に描画されます。

## 構造体: `System` {#system}

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

プログラムです。`main` はルートモジュールにある、`System` を返す `fn` です。エンジンは起動時に一度だけこれを実行します。すべてのフィールドが必須です。

| フィールド | 意味 |
| ---------- | ---- |
| `title` | ウィンドウのタイトル |
| `resolution` | デザイン解像度。レンダーと UI のすべての位置と長さは、この解像度のピクセルで表します |
| `start` | システムが最初に表示するルートページ |
| `game` | ゲームモードのビュー。`Narrative` ハンドルを受け取り、通常は `game_screen!` とメッセージウィンドウを描画します |
| `save_slots` | 手動スロットの数: `SaveSlot::Manual(0)` … `Manual(save_slots - 1)` |
| `auto_slots` | オートセーブスロットの数: `SaveSlot::Auto(0)` … `Auto(auto_slots - 1)` |
| `backlog` | `Narrative::history` のエントリーの最大数 |
| `font` | デフォルトフォント。`TextStyle` とテキスト効果のどちらもフォントを指定しないテキストのフォントです。必須です |
| `text` | ボタンとトグルの外での [`ui::label`](./ui.md#label) と [`text!`](./ui.md#text) のデフォルトの `TextStyle` |
| `settings` | エンジンが読む設定ハンドル |
| `bindings` | 入力バインディング |

`main` は通常のコードとして実行されるため、その呼び出しは他の呼び出しと同様に再帰の深さに数えられます。

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

完全なタイトル画面、ゲームビュー、設定画面は [システムのレシピ](./guides/system.md) にあります。

## 既見セット {#seen-sets}

既見セットは、エンジンが自動で管理するメタの値です。プレイヤーがこれまでに見たもの・聴いたもの・観たものを、すべてのプレイを通して記録します。既読フラグと同じく、ロードや巻き戻しで元に戻ることはありません。リアクティブなソースです。CG ギャラリー、音楽鑑賞、ムービー鑑賞を、スタジオ側の記録なしに実現できます。

### 関数: `seen_image(image)` {#seen_image}

```kis
#[builtin] pub fn seen_image(image: ImageAsset) -> bool;
```

- `image` `ImageAsset` — 画像。
- 戻り値: `bool` — `image` がこれまでにいずれかのステップの論理画面に（どの画像レイヤーであれ）載ったことがあるかどうか。
- 種別: `fn` · ビルトイン: はい

### 関数: `heard(audio)` {#heard}

```kis
#[builtin] pub fn heard(audio: AudioAsset) -> bool;
```

- `audio` `AudioAsset` — トラックまたはサウンド。
- 戻り値: `bool` — `audio` がこれまでに再生されたことがあるかどうか。チャンネルで `Loop` が始まった場合と、ワンショットのサウンドが再生された場合が該当します。
- 種別: `fn` · ビルトイン: はい

### 関数: `watched(video)` {#watched}

```kis
#[builtin] pub fn watched(video: VideoAsset) -> bool;
```

- `video` `VideoAsset` — ムービー。
- 戻り値: `bool` — `video` がこれまでに再生されたことがあるかどうか。
- 種別: `fn` · ビルトイン: はい

## 関数: `set_music(track, fade)` {#set_music}

```kis
#[builtin] pub fn set_music(track: Option<crate::screen::audio::Loop>, fade: crate::screen::audio::AudioFade);
```

- `track` `Option<Loop>` — 再生するトラック。`None` はシステム音楽を止めます。
- `fade` `AudioFade` — チャンネルが古いトラックから新しいトラックへ移る方法。例: `crossfade(ms(500))`。
- 種別: `fn` · ビルトイン: はい

**システム音楽チャンネル** を設定します。このチャンネルは、モードスタック上でゲームが実行されていない間（タイトル画面や音楽鑑賞画面など）に聞こえます。ゲームの実行中は、代わりにゲーム画面の音楽チャンネルが再生されます。`fn` なので、システムのコードだけが呼べます。`Loop`、`looped`、`AudioFade` については [`std::screen::audio`](./screen-audio.md) を参照してください。

> **安定性:** 予定 — 現在再生中のもの（現在のトラックや、再生位置・一時停止などの音楽鑑賞画面の再生状態）の読み取りは、まだ設計されていません。

## バインディング {#bindings}

[`System::bindings`](#system) は入力を動作に対応付けます。`Vec<Binding>` なので、スタジオは [`default_bindings()`](#default_bindings) から始めてエントリーを追加・置換します。

**フォーカスキーは組み込みで、バインドできません。** 矢印キー、十字キー、Tab と Shift+Tab はコントロール間でフォーカスを移動し、Enter と `PadButton::South` はフォーカス中のコントロールを実行します（[フォーカス](./ui.md#focus) を参照）。

### 列挙型: `Key` {#key}

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

キーボードのキーです。Enter、Escape、Space、Backspace、Tab、矢印キー、PageUp/PageDown、Home/End、修飾キー Shift・Control・Alt、F1–F12、英字 A–Z、数字 `Digit0`–`Digit9` があります。修飾キーはそれ自体が独立したキーで、キーの組み合わせはありません。

### 列挙型: `MouseButton` {#mousebutton}

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

マウスボタンまたはホイールの方向です。`Back` と `Forward` はサイドボタンです。

### 列挙型: `PadButton` {#padbutton}

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

ゲームパッドのボタンで、刻印ではなく位置で名付けられています。`South`、`East`、`West`、`North` は、それぞれ下・右・左・上のフェイスボタンです。`LeftStick` と `RightStick` はスティックの押し込みです。`PadButton::South` と十字キーはフォーカスの操作にも使われます（[フォーカス](./ui.md#focus) を参照）。

### 列挙型: `Input` {#input}

```kis
pub enum Input {
    Key(Key),
    Mouse(MouseButton),
    Pad(PadButton),
}
```

バインドできる入力です。

### 列挙型: `Action` {#action}

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

バインディングの動作です。

| 動作 | 効果 | 有効な条件 |
| ---- | ---- | ---------- |
| `Advance` | `NextStep` を送ります。ステップのタイムラインが実行中なら先にそれを完了させます | ゲームモードが最上位 |
| `HoldSkip` | 入力を押している間スキップモード | ゲームモードが最上位 |
| `ToggleSkip`、`ToggleAuto` | スキップモードまたはオートモードを切り替えます | ゲームモードが最上位 |
| `HideWindow` | テキストウィンドウを隠します | ゲームモードが最上位 |
| `QuickSave`、`QuickLoad` | [`SaveSlot::Quick`](./system-saves.md#saveslot) に書き込む、またはロードします | ゲームモードが最上位 |
| `OpenInGame(o)` | ゲームオーバーレイ `o` を積みます | ゲームモードが最上位 |
| `Back` | [`nav::back()`](./system-nav.md#back)。ベースでは無視されます | 常に |
| `Open(o)` | オーバーレイ `o` を積みます | 常に |

`Action::Back` のバインディングは、[`E6003`](../../../reference/ja/src/diagnostics/system.md) の検査において、すべてのモードからの出口としても数えられます。

### 構造体: `Binding` {#binding}

```kis
pub struct Binding {
    pub input: Input,
    pub action: Action,
}
```

動作に対応付けられた入力です。

### 関数: `bind(input, action)` {#bind}

```kis
pub const fn bind(input: Input, action: Action) -> Binding {
    Binding { input, action }
}
```

- `input` `Input` — 入力。
- `action` `Action` — 動作。
- 戻り値: `Binding` — `action` に対応付けられた `input`。
- 種別: `const fn` · ビルトイン: いいえ

### 関数: `default_bindings()` {#default_bindings}

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

- 戻り値: `Vec<Binding>` — 下のデフォルトバインディング。
- 種別: `const fn` · ビルトイン: いいえ

| 入力 | 動作 |
| ---- | ---- |
| `Key::Enter`、`Key::Space`、`MouseButton::Left`、`MouseButton::WheelDown`、`PadButton::South` | `Advance` |
| `Key::Control`、`PadButton::RightTrigger` | `HoldSkip` |
| `MouseButton::Right`、`PadButton::North` | `HideWindow` |
| `Key::Escape`、`PadButton::East` | `Back` |

デフォルトを `const fn` で拡張します。

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

> **安定性:** 予定 — 入力を任意の `fn` にバインドすること（フルスクリーン切り替えなど）とタッチ入力は、まだサポートされていません。
