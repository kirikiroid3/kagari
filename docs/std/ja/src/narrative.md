# `std::narrative` {#stdnarrative}

> **安定性:** 安定 — ステップ、乱択、メタフィールド、実行中の物語に対するシステムの視点。

`std::narrative` は、ステップの基本操作 `push_step`、フォーム `s!(…)` と `choice!{…}` が作る 2 つの標準ステップ `Say` と `Choice`、物語の乱択 `rng_choose`、メタフィールド（`Meta`）、そしてゲームシステムが実行中のゲームを見るためのハンドル `Narrative` を宣言します。

アイテムの種別は 3 つです。

- `pushdown fn`：`push_step`、`rng_choose`、`Meta::write` — 物語のコード
- `fn`：`Narrative` のすべてのメソッド、`Meta::get`、`Meta::set` — システムのコード
- `const fn`：`meta` — メタフィールドの初期化子

このモジュールは [`std::narrative::text`](./narrative-text.md) の 3 つのアイテムを再エクスポートします。

```kis
pub mod text;

pub use self::text::{Mark, Text, TextEffect};
```

ステップ、シーン、物語の関係は [ステップ](../../../reference/ja/src/concepts/steps.md) と [シーンと即時手続き](../../../reference/ja/src/concepts/scenes.md) で説明しています。

## 概要 {#overview}

|アイテム|種類|概要|
|---|---|---|
|[`Step`](#trait-step)|ビルトイントレイト|物語が中断するステップ|
|[`push_step`](#function-push_steps)|ビルトイン `pushdown fn`|ステップを実行する|
|[`rng_choose`](#function-rng_choose)|ビルトイン `pushdown fn`|乱数で値を選ぶ|
|[`Say`](#struct-say)|構造体|`s!(…)` で書くステップ|
|[`Choice`](#struct-choicea-n)|構造体|`choice!{…}` で書くステップ|
|[`ChoiceArm`](#struct-choicearma)|構造体|`Choice` のアーム|
|[`Meta<T>`](#struct-metat)|ビルトイン構造体|セーブの外にある永続的な大域値|
|[`meta`](#function-metainit)|ビルトイン `const fn`|メタフィールドを宣言する|
|[時間付き入力](#timed-input)|—|予定|
|[`Narrative`](#struct-narrative)|ビルトイン構造体|実行中のゲームへのハンドル|
|[`Line`](#struct-line)、[`Arm`](#struct-arm)、[`MenuArm`](#struct-menuarm)、[`Menu`](#struct-menu)、[`HudState`](#struct-hudstate)、[`Entry`](#enum-entry)、[`PlayMode`](#enum-playmode)|レコード|`Narrative` が返すもの|

## Trait: `Step` {#trait-step}

```kis
#[builtin] pub trait Step {
    type Output;
}
```

物語が中断するステップです。`Output` は物語が再開するときに受け取る型です。ステップは `Say` と `Choice` だけで、ゲームが `Step` を実装することはできません。

- Kind: — · Builtin: yes

## Function: `push_step(s)` {#function-push_steps}

```kis
#[builtin] pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

- `s` `S` — 表示するステップ。
- Returns: `S::Output` — プレイヤーの入力が運んだもの：`Say` なら `()`、`Choice` なら選ばれたアームの値。
- Kind: `pushdown fn` · Builtin: yes

`push_step` はステップを表示し、入力を待ち、入力が運んだものを返します。呼べるのはシーンだけで、シーン外のステップは [`E3002`](../../../reference/ja/src/diagnostics/kinds.md#e3002) です。通常はフォーム `s!(…)` と `choice!{…}` を通して書きます。

## Function: `rng_choose()` {#function-rng_choose}

```kis
#[builtin] pub pushdown fn rng_choose<T: RandChoose>() -> T;
```

- `T` — フィールドを持たない列挙型、または `Int<lo, hi>`。
- Returns: `T` — 物語の擬似乱数生成器（xoroshiro128++）が選んだ値。どの値も等確率です。
- Kind: `pushdown fn` · Builtin: yes

`rng_choose::<T>()` は、フィールドを持たない列挙型のバリアント、または `Int<lo, hi>` の整数を 1 つ選びます。呼べるのはシーンだけです（[`E3024`](../../../reference/ja/src/diagnostics/kinds.md#e3024)）。

- 擬似乱数生成器の状態は物語の状態の一部で、そのシードはセーブに含まれます。そのため、セーブをロードするとすべての乱択が再現されます。
- 物語の解析は、各乱択を `T` のすべての値にわたる自由な選択として扱います：すべての結果を扱う必要があります。

```kix
enum Coin { Heads, Tails }

#[scene]
pushdown fn Toss() {
    match rng_choose::<Coin>() {
        Coin::Heads => s!(t!["Heads."]),
        Coin::Tails => s!(t!["Tails."]),
    }
}
```

乱択はプレイヤーを待って中断しません：中断が `rng_choose` だけのループは、ステップを通過しません（[`E5001`](../../../reference/ja/src/diagnostics/analysis.md#e5001)）。

## Struct: `Say` {#struct-say}

```kis
pub struct Say {
    pub who: Option<Text>,
    pub voice: Option<VoiceAsset>,
    pub render: Option<Render>,
    pub text: Option<Text>,
    pub history: Option<Text>,
}

impl Step for Say {
    type Output = ();
}
```

- `who` `Option<Text>` — 話者名。`None` ならメッセージは地の文です。
- `voice` `Option<VoiceAsset>` — テキストと同時に始まり、行に結び付くボイス。[`std::screen::audio`](./screen-audio.md) を参照してください。
- `render` `Option<Render>` — ステップに入るときに実行されるレンダー。[`std::screen`](./screen.md) を参照してください。
- `text` `Option<Text>` — メッセージ。
- `history` `Option<Text>` — `text` の代わりにバックログに記録するテキスト。`None` なら、埋め込んだ値を表示時のまま含めた `text` を記録します。
- Kind: — · Builtin: no

`s!(who = …, voice = …, render = …, history = …, t![…])` は `push_step(Say { … })` で、省略した属性はそれぞれ `None` になります。`Say` は `NextStep`（クリックやキー入力）を待ちます。`NextStep` は情報を運ばず、セーブにも記録されません。

```kix
use std::narrative::text::speed;

const MIO_0012: VoiceAsset = asset("voice/mio_0012.ogg");

#[scene]
pushdown fn Morning() {
    s!(who = "Mio", voice = MIO_0012, t!["Good morning."]);
    s!(history = t!["(The bell rings.)"], t![!["Ding… dong…"](speed(ms(120)))]);
}
```

## Struct: `Choice<A, N>` {#struct-choicea-n}

```kis
pub struct Choice<A, const N: usize> {
    pub id: Text,
    pub render: Option<Render>,
    pub arms: [ChoiceArm<A>; N],
}

impl<A, const N: usize> Step for Choice<A, N> {
    type Output = A;
}
```

- `id` `Text` — 選択肢の安定した id。セーブは選択をこの id で記録します。
- `render` `Option<Render>` — ステップに入るときに実行されるレンダー。
- `arms` `[ChoiceArm<A>; N]` — 表示順のアーム。
- Kind: — · Builtin: no

`choice!{ id = …, render = …, t![…] if guard => V, … }` は `push_step(Choice { … })` です。各アームは `ChoiceArm` になり、その `shown` はアームのガード、ガードがなければ `true` です。ステップは選ばれたアームの `value` を返します。

- `id` は文字列リテラルで、関数内で一意でなければなりません（[`E3019`](../../../reference/ja/src/diagnostics/kinds.md#e3019)）。
- `arms` は `ChoiceArm { … }` レコードの配列リテラルでなければなりません。各値はフィールドを持たない列挙型のバリアントで、各バリアントは 1 回までしか現れません（[`E3020`](../../../reference/ja/src/diagnostics/kinds.md#e3020)）。
- すべてのアームが同時に隠れうる選択肢は、物語解析のエラーです（[`E5003`](../../../reference/ja/src/diagnostics/analysis.md#e5003)）。

### 無効なアーム {#disabled-arms}

無効なアームはグレーアウトして表示され、選べません。表示専用です：値を持たず、`choice!` の一部でもないので、物語解析からは見えません。選択肢のレンダーがステップの選択肢メニュー（`ChoiceMenu::disabled_arm`、[`std::screen`](./screen.md) を参照）を通して追加し、どのレンダーで追加するかを物語が選びます。

```kix
let place = choice!{
    id = "map",
    render = if VISITED_A && VISITED_B { map_open } else { map_leave_locked },
    t!["Stall A"] => Map::A,
    t!["Stall B"] => Map::B,
    t!["Leave"] if VISITED_A && VISITED_B => Map::Leave,
};

const fn map_open(screen: Screen) -> Screen {
    screen
}

const fn map_leave_locked(mut screen: Screen) -> Screen {
    screen.choice_menu().disabled_arm(2, t!["Leave"]);
    screen
}
```

`disabled_arm(at, text)` は、表示されるリストの位置 `at`（リストの長さに丸め込まれます）にアームを挿入します。

## Struct: `ChoiceArm<A>` {#struct-choicearma}

```kis
pub struct ChoiceArm<A> {
    pub text: Text,
    pub shown: bool,
    pub value: A,
}
```

- `text` `Text` — アームのラベル。
- `shown` `bool` — アームを表示するかどうか：アームのガード、または `true`。
- `value` `A` — アームが選ばれたときにステップが返す値。
- Kind: — · Builtin: no

## Struct: `Meta<T>` {#struct-metat}

```kis
#[builtin] pub struct Meta<T>;
```

セーブの外にある永続的な大域値です。「ルート A をクリアした」「トゥルーエンドを見た」などに使います。`meta(init)` を保持する不変の `static` がメタフィールドを宣言します。

- Kind: — · Builtin: yes

```kis
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

- 書き込みはロードや巻き戻しで元に戻りません。
- 物語はメタフィールドに書き込めますが、読み返すことはできません。メタが物語に届くのは、エントリーシーンの引数を通してだけです。
- `const` のコードからメタは見えません（[`E3013`](../../../reference/ja/src/diagnostics/kinds.md#e3013)）。
- `Meta` ハンドルは不変の `static` の初期化子にしかなれません（[`E3011`](../../../reference/ja/src/diagnostics/kinds.md#e3011)、[`E4007`](../../../reference/ja/src/diagnostics/eval.md#e4007)）。

|メソッド|種別|用途|
|---|---|---|
|`get()`|`fn`|システムのコードが読む。リアクティブ|
|`set(v)`|`fn`|システムのコードが書く|
|`write(v)`|`pushdown fn`|物語が書く|

### `Meta::get(&self)` {#metagetself}

```kis
#[builtin] pub fn get(&self) -> T;
```

- Returns: `T` — 現在の値。リアクティブです：これを読む UI は値が変わると更新されます。
- Kind: `fn` · Builtin: yes

### `Meta::set(&self, value)` {#metasetself-value}

```kis
#[builtin] pub fn set(&self, value: T);
```

- `value` `T` — 新しい値。
- Kind: `fn` · Builtin: yes

システムのコードから値を設定します。

### `Meta::write(&self, value)` {#metawriteself-value}

```kis
#[builtin] pub pushdown fn write(&self, value: T);
```

- `value` `T` — 新しい値。
- Kind: `pushdown fn` · Builtin: yes

物語から値を設定します。

```kis
#[scene]
pushdown fn AzusaEnding() {
    CLEARED_AZUSA.write(true);
}
```

## Function: `meta(init)` {#function-metainit}

```kis
#[builtin] pub const fn meta<T>(init: T) -> Meta<T>;
```

- `init` `T` — メタファイルに値がないときに使う値。
- Returns: `Meta<T>` — メタフィールド。
- Kind: `const fn` · Builtin: yes

## 時間付き入力 {#timed-input}

> **安定性:** 予定。

時間付き入力や、物語が駆動するオートプレイにはまだ対応していません。予定している API は、`fn` のコードで一定の `Duration` の後にクロージャを 1 回だけ呼ぶワンショットのタイマーで、JavaScript の `setTimeout` に近いものです。繰り返すタイマー（`setInterval` 相当）は用意しません。

## Struct: `Narrative` {#struct-narrative}

```kis
#[builtin] pub struct Narrative;
```

実行中のゲームへのハンドルで、ゲームビューとゲームオーバーレイに渡されます。読み取りはリアクティブです。

- Kind: — · Builtin: yes

システムが実行中のゲームを見るのは、このハンドルを通してだけです。内部表現は隠されているので、システムのコードが物語変数を読むことはできません。ハンドルはゲームビュー（`System::game: fn(Narrative) -> Element`）とゲームオーバーレイ（`fn(Narrative) -> GameOverlay`）にしか存在しないため、実行中のゲームを必要とする操作はすべて、型の上でハンドルを必要とします。メソッドはすべて `fn` です。

|メソッド|効果|
|---|---|
|`line()`|現在の行|
|`menu()`|現在の選択肢|
|`hud()`|現在のステップの Hud|
|`history()`|バックログ（古い順）|
|`mode()`、`set_mode(m)`|プレイモード|
|`advance()`|物語に `NextStep` を送る|
|`select(arm)`|現在の選択肢の表示中のアームを選ぶ|
|`save(slot)`|実行中のゲームを `slot` に書き込む|
|`rewind(i)`|物語を `history()` の `i` 番目のエントリーまで戻す|
|`call(o)`|ゲームオーバーレイ `o` を開く|
|`replay_voice(v)`|ボイスバスで `v` をもう一度再生する|

> **安定性:** 予定 — NVL モード。複数の行が改ページまで画面に残る全画面テキストモードは、まだ設計されていません。テキストウィンドウの表示内容と、バックログや `Narrative::line` によるステップの記述が変わる予定です。

### `Narrative::line(&self)` {#narrativelineself}

```kis
#[builtin] pub fn line(&self) -> Option<Line>;
```

- Returns: `Option<Line>` — 表示中の行。選択肢の間や最初のステップの前は `None`。
- Kind: `fn` · Builtin: yes

### `Narrative::menu(&self)` {#narrativemenuself}

```kis
#[builtin] pub fn menu(&self) -> Option<Menu>;
```

- Returns: `Option<Menu>` — 無効なアームを含む、表示中の選択肢。選択肢の外では `None`。
- Kind: `fn` · Builtin: yes

### `Narrative::hud(&self)` {#narrativehudself}

```kis
#[builtin] pub fn hud(&self) -> HudState;
```

- Returns: `HudState` — ゲームビューが描く、現在のステップの Hud。
- Kind: `fn` · Builtin: yes

### `Narrative::history(&self)` {#narrativehistoryself}

```kis
#[builtin] pub fn history(&self) -> Vec<Entry>;
```

- Returns: `Vec<Entry>` — 古い順のバックログ。
- Kind: `fn` · Builtin: yes

バックログの長さは `System::backlog` で制限され、古いエントリーから捨てられます。`Entry::Line` は表示時のままのテキスト、またはステップに `history` があればそのテキストを保持します。`Entry::Chosen` は選ばれたアームのテキストを保持します。

### `Narrative::mode(&self)` {#narrativemodeself}

```kis
#[builtin] pub fn mode(&self) -> PlayMode;
```

- Returns: `PlayMode` — プレイモード。
- Kind: `fn` · Builtin: yes

### `Narrative::set_mode(&self, mode)` {#narrativeset_modeself-mode}

```kis
#[builtin] pub fn set_mode(&self, mode: PlayMode);
```

- `mode` `PlayMode` — 新しいプレイモード。
- Kind: `fn` · Builtin: yes

### `Narrative::advance(&self)` {#narrativeadvanceself}

```kis
#[builtin] pub fn advance(&self);
```

- Kind: `fn` · Builtin: yes

次のステップへ進みます：テキストのクリックと同じく、物語に `NextStep` を送ります。

### `Narrative::select(&self, arm)` {#narrativeselectself-arm}

```kis
#[builtin] pub fn select(&self, arm: Arm);
```

- `arm` `Arm` — `menu()` から得た、現在の選択肢の表示中のアーム。
- Kind: `fn` · Builtin: yes

### `Narrative::save(&self, slot)` {#narrativesaveself-slot}

```kis
#[builtin] pub fn save(&self, slot: SaveSlot);
```

- `slot` `SaveSlot` — 書き込むスロット。[`std::system::saves`](./system-saves.md) を参照してください。
- Kind: `fn` · Builtin: yes

実行中のゲームを `slot` にセーブします。

### `Narrative::rewind(&self, entry)` {#narrativerewindself-entry}

```kis
#[builtin] pub fn rewind(&self, entry: u32);
```

- `entry` `u32` — `history()` のインデックス。
- Kind: `fn` · Builtin: yes

物語をバックログの `entry` 番目のエントリーまで戻します。

### `Narrative::call(&self, overlay)` {#narrativecallself-overlay}

```kis
#[builtin] pub fn call(&self, overlay: fn(Narrative) -> GameOverlay);
```

- `overlay` `fn(Narrative) -> GameOverlay` — 開くオーバーレイ。このハンドルを受け取ります。
- Kind: `fn` · Builtin: yes

ゲームオーバーレイを開きます（モードスタックに積みます）。[`std::system`](./system.md) を参照してください。

### `Narrative::replay_voice(&self, voice)` {#narrativereplay_voiceself-voice}

```kis
#[builtin] pub fn replay_voice(&self, voice: VoiceAsset);
```

- `voice` `VoiceAsset` — 再生するボイス。
- Kind: `fn` · Builtin: yes

ボイスバスで `voice` をもう一度再生します。主にバックログから使います。

## Struct: `Line` {#struct-line}

```kis
pub struct Line {
    pub who: Option<Text>,
    pub text: Option<Text>,
    pub voice: Option<VoiceAsset>,
    pub read: bool,
}
```

`Say` ステップの行です。

- `who`、`text`、`voice` — `Say` ステップと同じ（バックログでは、ステップに `history` があれば `text` はそのテキストです）。
- `read` `bool` — プレイヤーがこの行を以前に読んだかどうか。エンジンの既読フラグに基づきます。
- Kind: — · Builtin: no

## Struct: `Arm` {#struct-arm}

```kis
#[builtin] pub struct Arm;
```

現在の選択肢の選択可能なアームです。`Narrative::select` のための不透明なハンドルです。

- Kind: — · Builtin: yes

## Struct: `MenuArm` {#struct-menuarm}

```kis
pub struct MenuArm {
    pub text: Text,
    pub arm: Option<Arm>,
}
```

表示中の選択肢のアームです。

- `text` `Text` — ラベル。
- `arm` `Option<Arm>` — 選ぶためのハンドル。ステップの選択肢メニューから追加された無効なアームでは `None`。
- Kind: — · Builtin: no

## Struct: `Menu` {#struct-menu}

```kis
pub struct Menu {
    pub arms: Vec<MenuArm>,
    pub align: VAlign,
}
```

表示中の選択肢です。

- `arms` `Vec<MenuArm>` — 表示されるアームを順に並べ、無効なアームをそれぞれの `at` の位置に挿入したもの。
- `align` `VAlign` — メニューの位置。レンダーが設定します（`ChoiceMenu::set_align`）。
- Kind: — · Builtin: no

## Struct: `HudState` {#struct-hudstate}

```kis
pub struct HudState {
    pub text_window: Visibility,
    pub quick_menu: Visibility,
    pub side_image: Option<ImageAsset>,
}
```

ゲームビューが描く Hud です。

- `text_window` `Visibility` — テキストウィンドウを表示するかどうか。
- `quick_menu` `Visibility` — クイックメニューを表示するかどうか。
- `side_image` `Option<ImageAsset>` — サイド画像（あれば）。
- Kind: — · Builtin: no

## Enum: `Entry` {#enum-entry}

```kis
pub enum Entry {
    Line(Line),
    Chosen(Text),
}
```

バックログのエントリーです：行、または選ばれたアームのテキスト。

- Kind: — · Builtin: no

## Enum: `PlayMode` {#enum-playmode}

```kis
pub enum PlayMode {
    Normal,
    Auto,
    Skip,
}
```

ゲームの進め方です。

- `Normal` — プレイヤーの入力で進みます。
- `Auto` — `auto_delay` 設定（に 1 文字ごとの `auto_delay_per_char` を加えた時間）の後に進みます。`wait_for_voice` がオンならボイスの終了を待ちます。
- `Skip` — できるだけ速く進みます。`skip_unread` 設定がオンでない限り未読のテキストで止まり、選択肢では常に止まります。
- Kind: — · Builtin: no

## 例：バックログ {#example-a-backlog}

```kix
use std::narrative::Entry;
use std::ui::{each, label, scroll, spacer};

fn Backlog(game: Narrative) -> GameOverlay {
    GameOverlay(column![
        scroll(|| each(|| game.history(), |entry| match entry {
            Entry::Line(line) => match line.text {
                Some(text) => label(text),
                None => spacer(),
            },
            Entry::Chosen(text) => label(text),
        })),
        button!{on_click = || nav::back(), t!["Back"]},
    ])
}
```

ゲームビューやバインディングから `game.call(Backlog)` で開きます。
