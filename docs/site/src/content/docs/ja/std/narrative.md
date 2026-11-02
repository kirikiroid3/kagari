---
title: "std::narrative"
---

> **安定性:** 安定 — ステップ、乱択、メタフィールド、ゲームシステムから見た実行中の物語。

`std::narrative` では次のものを宣言しています。

- ステップの基本操作 `push_step`
- `s!(…)` 構文と `choice!{…}` 構文が作る標準のステップ `Say` と `Choice`
- 物語の中の乱択 `rng_choose`
- メタフィールド（`Meta`）
- ゲームシステムが実行中のゲームに触れるためのハンドル `Narrative`

アイテムの種別は次の 3 通りです。

- `pushdown fn`：`push_step`、`rng_choose`、`Meta::write`（物語のコード）
- `fn`：`Narrative` の全メソッド、`Meta::get`、`Meta::set`（システムのコード）
- `const fn`：`meta`（メタフィールドの初期化子）

また、[`std::narrative::text`](./narrative-text.md) の 3 つのアイテムを再エクスポートしています。

```kis
pub mod text;

pub use self::text::{Mark, Text, TextEffect};
```

ステップ、シーン、物語の関係については [ステップ](../reference/concepts/steps.md) と [シーンと即時手続き](../reference/concepts/scenes.md) を参照してください。

## 概要 {#overview}

|アイテム|種類|概要|
|---|---|---|
|[`Step`](#trait-step)|組み込みのトレイト|物語を中断させるステップ|
|[`push_step`](#function-push_steps)|組み込みの `pushdown fn`|ステップを実行する|
|[`rng_choose`](#function-rng_choose)|組み込みの `pushdown fn`|乱数で値を選ぶ|
|[`Say`](#struct-say)|構造体|`s!(…)` で書くステップ|
|[`Choice`](#struct-choicea-n)|構造体|`choice!{…}` で書くステップ|
|[`ChoiceArm`](#struct-choicearma)|構造体|`Choice` のアーム|
|[`Meta<T>`](#struct-metat)|組み込みの構造体|セーブとは別に永続化される大域値|
|[`meta`](#function-metainit)|組み込みの `const fn`|メタフィールドを宣言する|
|[時間付き入力](#timed-input)|—|予定|
|[`Narrative`](#struct-narrative)|組み込みの構造体|実行中のゲームへのハンドル|
|[`Line`](#struct-line)、[`Arm`](#struct-arm)、[`MenuArm`](#struct-menuarm)、[`Menu`](#struct-menu)、[`HudState`](#struct-hudstate)、[`Entry`](#enum-entry)、[`PlayMode`](#enum-playmode)|レコード|`Narrative` が返す値|

## トレイト：`Step` {#trait-step}

```kis
#[builtin] pub trait Step {
    type Output;
}
```

物語を中断させるステップです。`Output` は、物語が再開するときに受け取る値の型です。ステップは `Say` と `Choice` の 2 つだけで、ゲーム側で `Step` を実装することはできません。

- 種別：— · 組み込み：はい

## 関数：`push_step(s)` {#function-push_steps}

```kis
#[builtin] pub pushdown fn push_step<S: Step>(s: S) -> S::Output;
```

- `s` `S` — 表示するステップ。
- 戻り値：`S::Output` — プレイヤーの入力の結果。`Say` なら `()`、`Choice` なら選ばれたアームの値。
- 種別：`pushdown fn` · 組み込み：はい

ステップを表示して入力を待ち、入力の結果を返します。呼び出せるのはシーンの中だけで、シーン以外でステップを使うとエラー [`E3002`](../diagnostics/kinds.md#e3002) になります。ふつうは直接呼ばず、`s!(…)` 構文や `choice!{…}` 構文で書きます。

## 関数：`rng_choose()` {#function-rng_choose}

```kis
#[builtin] pub pushdown fn rng_choose<T: RandChoose>() -> T;
```

- `T` — フィールドのない列挙型、または `Int<lo, hi>`。
- 戻り値：`T` — 物語の擬似乱数生成器（xoroshiro128++）が選んだ値。どの値も等確率で選ばれます。
- 種別：`pushdown fn` · 組み込み：はい

`rng_choose::<T>()` は、フィールドのない列挙型のバリアントか、`Int<lo, hi>` の範囲の整数を 1 つ選びます。呼び出せるのはシーンの中だけです（[`E3024`](../diagnostics/kinds.md#e3024)）。

- 擬似乱数生成器の状態は物語の状態に含まれ、シードもセーブに保存されます。そのため、セーブをロードすると乱択の結果はすべて同じになります。
- 物語の解析では、乱択を「`T` のどの値にもなりうる自由な選択」として扱います。したがって、すべての結果に対応しておく必要があります。

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

乱択ではプレイヤーの入力を待たないので、物語は中断しません。`rng_choose` しか含まないループはステップを通らないため、エラーになります（[`E5001`](../diagnostics/analysis.md#e5001)）。

## 構造体：`Say` {#struct-say}

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

- `who` `Option<Text>` — 話者名。`None` なら地の文になります。
- `voice` `Option<VoiceAsset>` — テキストと同時に再生が始まり、その行に結び付くボイス。[`std::screen::audio`](./screen-audio.md) を参照してください。
- `render` `Option<Render>` — ステップに入るときに実行するレンダー。[`std::screen`](./screen.md) を参照してください。
- `text` `Option<Text>` — メッセージ。
- `history` `Option<Text>` — `text` の代わりにバックログへ記録するテキスト。`None` なら `text` を記録し、埋め込んだ値は表示したときの値のまま残ります。
- 種別：— · 組み込み：いいえ

`s!(who = …, voice = …, render = …, history = …, t![…])` は `push_step(Say { … })` と同じで、省略した属性は `None` になります。`Say` は `NextStep`（クリックやキー入力）を待ちます。`NextStep` は何の情報も持たず、セーブにも記録されません。

```kix
use std::narrative::text::speed;

const MIO_0012: VoiceAsset = asset("voice/mio_0012.ogg");

#[scene]
pushdown fn Morning() {
    s!(who = "Mio", voice = MIO_0012, t!["Good morning."]);
    s!(history = t!["(The bell rings.)"], t![!["Ding… dong…"](speed(ms(120)))]);
}
```

## 構造体：`Choice<A, N>` {#struct-choicea-n}

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

- `id` `Text` — 選択肢の固定の id。セーブにはこの id で選択を記録します。
- `render` `Option<Render>` — ステップに入るときに実行するレンダー。
- `arms` `[ChoiceArm<A>; N]` — 表示順に並べたアーム。
- 種別：— · 組み込み：いいえ

`choice!{ id = …, render = …, t![…] if guard => V, … }` は `push_step(Choice { … })` と同じです。各アームは `ChoiceArm` になり、`shown` にはアームのガードが入ります（ガードがなければ `true`）。ステップは選ばれたアームの `value` を返します。

- `id` は文字列リテラルで、関数の中で重複してはいけません（[`E3019`](../diagnostics/kinds.md#e3019)）。
- `arms` は `ChoiceArm { … }` を並べた配列リテラルでなければなりません。値はフィールドのない列挙型のバリアントで、同じバリアントを 2 回以上使うことはできません（[`E3020`](../diagnostics/kinds.md#e3020)）。
- すべてのアームが同時に隠れる可能性のある選択肢は、物語の解析でエラーになります（[`E5003`](../diagnostics/analysis.md#e5003)）。

### 無効なアーム {#disabled-arms}

無効なアームはグレーアウトして表示され、選ぶことはできません。表示のためだけのもので、値を持たず `choice!` にも含まれないため、物語の解析からは見えません。選択肢のレンダーがステップの選択肢メニュー（`ChoiceMenu::disabled_arm`。[`std::screen`](./screen.md) を参照）を通して追加します。どのレンダーを使うかは物語が決めます。

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

`disabled_arm(at, text)` は、表示中のリストの位置 `at` にアームを挿入します。`at` がリストの長さを超える場合は末尾に入ります。

## 構造体：`ChoiceArm<A>` {#struct-choicearma}

```kis
pub struct ChoiceArm<A> {
    pub text: Text,
    pub shown: bool,
    pub value: A,
}
```

- `text` `Text` — アームのラベル。
- `shown` `bool` — アームを表示するかどうか。アームのガード、ガードがなければ `true`。
- `value` `A` — アームが選ばれたときにステップが返す値。
- 種別：— · 組み込み：いいえ

## 構造体：`Meta<T>` {#struct-metat}

```kis
#[builtin] pub struct Meta<T>;
```

セーブとは別に永続化される大域値です。「ルート A をクリアした」「トゥルーエンドを見た」といった情報に使います。メタフィールドは、`meta(init)` を値とする不変の `static` として宣言します。

- 種別：— · 組み込み：はい

```kis
static CLEARED_AZUSA: Meta<bool> = meta(false);
```

- 書き込んだ値は、ロードや巻き戻しをしても元に戻りません。
- 物語からメタフィールドに書き込むことはできますが、読むことはできません。メタデータを物語に渡す方法は、開始シーンの引数だけです。
- `const` のコードからはメタデータを参照できません（[`E3013`](../diagnostics/kinds.md#e3013)）。
- `Meta` ハンドルは不変の `static` の初期化子にしか書けません（[`E3011`](../diagnostics/kinds.md#e3011)、[`E4007`](../diagnostics/eval.md#e4007)）。

|メソッド|種別|用途|
|---|---|---|
|`get()`|`fn`|システムのコードから読む（リアクティブ）|
|`set(v)`|`fn`|システムのコードから書く|
|`write(v)`|`pushdown fn`|物語から書く|

### `Meta::get(&self)` {#metagetself}

```kis
#[builtin] pub fn get(&self) -> T;
```

- 戻り値：`T` — 現在の値。リアクティブなので、この値を読む UI は値が変わると更新されます。
- 種別：`fn` · 組み込み：はい

### `Meta::set(&self, value)` {#metasetself-value}

```kis
#[builtin] pub fn set(&self, value: T);
```

- `value` `T` — 新しい値。
- 種別：`fn` · 組み込み：はい

システムのコードから値を設定します。

### `Meta::write(&self, value)` {#metawriteself-value}

```kis
#[builtin] pub pushdown fn write(&self, value: T);
```

- `value` `T` — 新しい値。
- 種別：`pushdown fn` · 組み込み：はい

物語から値を設定します。

```kis
#[scene]
pushdown fn AzusaEnding() {
    CLEARED_AZUSA.write(true);
}
```

## 関数：`meta(init)` {#function-metainit}

```kis
#[builtin] pub const fn meta<T>(init: T) -> Meta<T>;
```

- `init` `T` — メタファイルに値がないときに使う初期値。
- 戻り値：`Meta<T>` — メタフィールド。
- 種別：`const fn` · 組み込み：はい

## 時間付き入力 {#timed-input}

> **安定性:** 予定。

時間制限付きの入力や、物語側から制御するオートプレイにはまだ対応していません。予定しているのは、`fn` のコードから使う単発のタイマーです。指定した `Duration` が過ぎたらクロージャを 1 回だけ呼ぶもので、JavaScript の `setTimeout` に近いものです。繰り返し呼ぶタイマー（`setInterval` に相当するもの）は用意しません。

## 構造体：`Narrative` {#struct-narrative}

```kis
#[builtin] pub struct Narrative;
```

実行中のゲームへのハンドルで、ゲーム画面とゲームオーバーレイに渡されます。読み取りはリアクティブです。

- 種別：— · 組み込み：はい

ゲームシステムが実行中のゲームに触れる手段は、このハンドルだけです。内部は隠されているので、システムのコードから物語変数を読むことはできません。ハンドルを受け取れるのはゲーム画面（`System::game: fn(Narrative) -> Element`）とゲームオーバーレイ（`fn(Narrative) -> GameOverlay`）だけなので、実行中のゲームを必要とする操作には、型の上でもハンドルが必要になります。メソッドはすべて `fn` です。

|メソッド|効果|
|---|---|
|`line()`|現在の行|
|`menu()`|現在の選択肢|
|`hud()`|現在のステップの Hud|
|`history()`|バックログ（古い順）|
|`mode()`、`set_mode(m)`|プレイモード|
|`advance()`|物語に `NextStep` を送る|
|`select(arm)`|現在の選択肢で表示中のアームを選ぶ|
|`save(slot)`|実行中のゲームを `slot` に保存する|
|`rewind(i)`|物語を `history()` の `i` 番目の項目まで巻き戻す|
|`call(o)`|ゲームオーバーレイ `o` を開く|
|`replay_voice(v)`|ボイスバスで `v` をもう一度再生する|

> **安定性:** 予定 — NVL モード。複数の行を改ページまで画面に残す全画面のテキスト表示は、まだ設計していません。対応すると、テキストウィンドウの表示内容や、バックログと `Narrative::line` でのステップの扱いが変わる予定です。

### `Narrative::line(&self)` {#narrativelineself}

```kis
#[builtin] pub fn line(&self) -> Option<Line>;
```

- 戻り値：`Option<Line>` — 表示中の行。選択肢の表示中や最初のステップの前は `None`。
- 種別：`fn` · 組み込み：はい

### `Narrative::menu(&self)` {#narrativemenuself}

```kis
#[builtin] pub fn menu(&self) -> Option<Menu>;
```

- 戻り値：`Option<Menu>` — 表示中の選択肢（無効なアームも含む）。選択肢の表示中でなければ `None`。
- 種別：`fn` · 組み込み：はい

### `Narrative::hud(&self)` {#narrativehudself}

```kis
#[builtin] pub fn hud(&self) -> HudState;
```

- 戻り値：`HudState` — 現在のステップの Hud。ゲーム画面がこれを描きます。
- 種別：`fn` · 組み込み：はい

### `Narrative::history(&self)` {#narrativehistoryself}

```kis
#[builtin] pub fn history(&self) -> Vec<Entry>;
```

- 戻り値：`Vec<Entry>` — 古い順に並んだバックログ。
- 種別：`fn` · 組み込み：はい

バックログの長さの上限は `System::backlog` で決まり、超えた分は古い項目から捨てます。`Entry::Line` には表示したときのテキストが入ります。ステップに `history` があれば、そちらのテキストが入ります。`Entry::Chosen` には選ばれたアームのテキストが入ります。

### `Narrative::mode(&self)` {#narrativemodeself}

```kis
#[builtin] pub fn mode(&self) -> PlayMode;
```

- 戻り値：`PlayMode` — プレイモード。
- 種別：`fn` · 組み込み：はい

### `Narrative::set_mode(&self, mode)` {#narrativeset_modeself-mode}

```kis
#[builtin] pub fn set_mode(&self, mode: PlayMode);
```

- `mode` `PlayMode` — 新しいプレイモード。
- 種別：`fn` · 組み込み：はい

### `Narrative::advance(&self)` {#narrativeadvanceself}

```kis
#[builtin] pub fn advance(&self);
```

- 種別：`fn` · 組み込み：はい

次のステップへ進みます。テキストをクリックしたときと同じく、物語に `NextStep` を送ります。

### `Narrative::select(&self, arm)` {#narrativeselectself-arm}

```kis
#[builtin] pub fn select(&self, arm: Arm);
```

- `arm` `Arm` — 現在の選択肢で表示中のアーム。`menu()` から取得します。
- 種別：`fn` · 組み込み：はい

### `Narrative::save(&self, slot)` {#narrativesaveself-slot}

```kis
#[builtin] pub fn save(&self, slot: SaveSlot);
```

- `slot` `SaveSlot` — 保存先のスロット。[`std::system::saves`](./system-saves.md) を参照してください。
- 種別：`fn` · 組み込み：はい

実行中のゲームを `slot` にセーブします。

### `Narrative::rewind(&self, entry)` {#narrativerewindself-entry}

```kis
#[builtin] pub fn rewind(&self, entry: u32);
```

- `entry` `u32` — `history()` のインデックス。
- 種別：`fn` · 組み込み：はい

物語をバックログの `entry` 番目の項目まで巻き戻します。

### `Narrative::call(&self, overlay)` {#narrativecallself-overlay}

```kis
#[builtin] pub fn call(&self, overlay: fn(Narrative) -> GameOverlay);
```

- `overlay` `fn(Narrative) -> GameOverlay` — 開くオーバーレイ。このハンドルを引数に受け取ります。
- 種別：`fn` · 組み込み：はい

ゲームオーバーレイを開きます（モードスタックに積みます）。[`std::system`](./system.md) を参照してください。

### `Narrative::replay_voice(&self, voice)` {#narrativereplay_voiceself-voice}

```kis
#[builtin] pub fn replay_voice(&self, voice: VoiceAsset);
```

- `voice` `VoiceAsset` — 再生するボイス。
- 種別：`fn` · 組み込み：はい

ボイスバスで `voice` をもう一度再生します。主にバックログで使います。

## 構造体：`Line` {#struct-line}

```kis
pub struct Line {
    pub who: Option<Text>,
    pub text: Option<Text>,
    pub voice: Option<VoiceAsset>,
    pub read: bool,
}
```

`Say` ステップの行です。

- `who`、`text`、`voice` — `Say` ステップと同じ。ただしバックログでは、ステップに `history` があれば `text` はそのテキストになります。
- `read` `bool` — プレイヤーがこの行を以前に読んだかどうか。エンジンの既読フラグに基づきます。
- 種別：— · 組み込み：いいえ

## 構造体：`Arm` {#struct-arm}

```kis
#[builtin] pub struct Arm;
```

現在の選択肢で選べるアームを指す、`Narrative::select` 用の不透明なハンドルです。

- 種別：— · 組み込み：はい

## 構造体：`MenuArm` {#struct-menuarm}

```kis
pub struct MenuArm {
    pub text: Text,
    pub arm: Option<Arm>,
}
```

表示中の選択肢のアームです。

- `text` `Text` — ラベル。
- `arm` `Option<Arm>` — 選ぶときに使うハンドル。ステップの選択肢メニューから追加した無効なアームでは `None`。
- 種別：— · 組み込み：いいえ

## 構造体：`Menu` {#struct-menu}

```kis
pub struct Menu {
    pub arms: Vec<MenuArm>,
    pub align: VAlign,
}
```

表示中の選択肢です。

- `arms` `Vec<MenuArm>` — 表示するアームを順に並べ、無効なアームをそれぞれの `at` の位置に挿入したもの。
- `align` `VAlign` — メニューの位置。レンダーが設定します（`ChoiceMenu::set_align`）。
- 種別：— · 組み込み：いいえ

## 構造体：`HudState` {#struct-hudstate}

```kis
pub struct HudState {
    pub text_window: Visibility,
    pub quick_menu: Visibility,
    pub side_image: Option<ImageAsset>,
}
```

ゲーム画面が描く Hud です。

- `text_window` `Visibility` — テキストウィンドウを表示するかどうか。
- `quick_menu` `Visibility` — クイックメニューを表示するかどうか。
- `side_image` `Option<ImageAsset>` — サイド画像（ある場合）。
- 種別：— · 組み込み：いいえ

## 列挙型：`Entry` {#enum-entry}

```kis
pub enum Entry {
    Line(Line),
    Chosen(Text),
}
```

バックログの項目です。行か、選ばれたアームのテキストのどちらかです。

- 種別：— · 組み込み：いいえ

## 列挙型：`PlayMode` {#enum-playmode}

```kis
pub enum PlayMode {
    Normal,
    Auto,
    Skip,
}
```

ゲームの進め方です。

- `Normal` — プレイヤーの入力で進みます。
- `Auto` — `auto_delay` 設定の時間（と、1 文字ごとに `auto_delay_per_char`）が過ぎると進みます。`wait_for_voice` がオンなら、ボイスが終わるまで待ちます。
- `Skip` — できるだけ速く進みます。`skip_unread` 設定がオンでなければ未読のテキストで止まり、選択肢では必ず止まります。
- 種別：— · 組み込み：いいえ

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

ゲーム画面やキー割り当てから `game.call(Backlog)` で開きます。
