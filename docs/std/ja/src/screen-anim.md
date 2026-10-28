# `std::screen::anim` {#stdscreenanim}

> **安定性:** 安定 — アニメーション値です。イージング、プロパティ用のトゥイーン、レイヤー用のトランジション、相対モーションを扱います。

レンダーが画面に加える変更は、すべてアニメーションを明示します。このモジュールはその値を宣言します。[`Easing`](#easing) は時間曲線、[`Tween`](#tween) は `Prop` の変更用、[`Transition`](#transition) はレイヤーの差し替え用、[`Motion`](#motion) はエンティティやカメラにかける相対的な軌跡です。いつ始まるかは [`Screen`](./screen.md#timeline-methods) のタイムラインメソッドが決めます。アイテムはすべて `const fn` か単純なデータです。

プレリュードは `Easing`、`Tween`、`Transition`、`Motion`、`tween`、`instant`、`cut`、`fade`、`rule`、`wipe`、`through`、`motion`、`jump`、`nod`、`shake`、`quake` を再エクスポートします。残り（`Direction`、`TransitionKind`、`MotionKey`、`rule_with`、`masked`、`masked_with`、`blinds`）には `use std::screen::anim::…` が必要です。

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`Easing`](#easing) | 列挙型 | 時間曲線 |
| [`Tween`](#tween)、`tween`、`instant` | 構造体 + コンストラクター | プロパティの動き方 |
| [`Direction`](#direction) | 列挙型 | 画面上の方向 |
| [`TransitionKind`](#transitionkind)、[`Transition`](#transition)、`cut`、`fade`、`rule`、`rule_with`、`masked`、`masked_with`、`wipe`、`blinds`、`through` | 列挙型、構造体 + コンストラクター | レイヤーの差し替え方 |
| [`MotionKey`](#motionkey)、[`Motion`](#motion)、`motion` | 構造体 + コンストラクター | 相対的な軌跡 |
| [`jump`](#jump)、[`nod`](#nod)、[`shake`](#shake)、[`quake`](#quake) | 関数 | モーションのプリセット |

## タイムライン {#the-timeline}

各ステップはタイムラインを持ちます。レンダーは**カーソル**を通してアニメーションをタイムラインにスケジュールします。カーソルは、アンカー（ステップのエントリーまたはテキストのマーク）とミリ秒単位のオフセットからなる位置です。ステップのエントリーでカーソルは (エントリー, 0 ms) にあり、テキストの開始位置も同じです。

```kis
impl Screen {
    #[builtin] pub const fn wait(&mut self, duration: Duration);
    #[builtin] pub const fn barrier(&mut self);
    #[builtin] pub const fn at_mark(&mut self, mark: Mark);
    #[builtin] pub const fn at_entry(&mut self);
    #[builtin] pub const fn begin_text(&mut self);
}
```

- アニメーション、トランジション、モーション、サウンドはすべて `cursor + delay` で始まります。2 回のカーソル移動の間にある文は同時に始まります。
- `wait(d)` はカーソルを `d` だけ進めます。
- `barrier()` はカーソルを、これまでにスケジュールしたすべて（アニメーション、トランジション、モーション、削除、ワンショットサウンド）の終わりへ移します。サウンドの長さはエンジンが解決します。
- `at_mark(m)` はカーソルを (マーク `m`, 0) に移します。マークはテキスト表示が `mark(n)` に達する時刻です（[`std::narrative::text`](./narrative-text.md#marks) を参照）。テキストに含まれないマークは表示の終わりに解決されます。
- `at_entry()` はカーソルを (エントリー, 0) に戻します。
- `begin_text()` はテキスト表示とボイスをカーソル位置で開始させます。カーソルがマークに固定されている間に呼ぶと定数評価エラーです。

タイムラインはステップ限りで、[論理画面](./screen.md#the-logical-screen)には含まれません。

<!-- diagram: screen-anim-timeline (mermaid-ish)
gantt
  section アニメーション
    背景 fade(ms(500))      : 0, 500
    テキスト表示 + ボイス   : 800, 1400
    表情 fade(ms(300))      : mark(1) から, 300
  section カーソル
    1 エントリー, 0 ms ==> 2 barrier() 500 ms ==> 3 wait(ms(300)) 800 ms, begin_text() ==> 4 at_mark(Mark(1))
-->
{{#include diagrams/screen-anim-timeline.svg}}

上の図のレンダーです。

```kis
use std::narrative::Mark;

const fn cg_then_line(mut screen: Screen) -> Screen {
    screen.background().change(CG_001).apply_animation(fade(ms(500)));
    screen.barrier();
    screen.wait(ms(300));
    screen.begin_text();
    screen.at_mark(Mark(1));
    screen.entity(HANAKO).image().change(HANAKO::SURPRISED).apply_animation(fade(ms(300)));
    screen
}
```

CG が出きるまでテキストウィンドウを隠すレンダーです。

```kis
const fn cg_then_text(mut screen: Screen) -> Screen {
    screen.hud().text_window().change(Visibility::Hidden).apply_animation(instant());
    screen.background().change(CG_001).apply_animation(fade(ms(800)));
    screen.barrier();
    screen.hud().text_window().change(Visibility::Shown).apply_animation(tween(ms(200)));
    screen.begin_text();
    screen
}
```

> **安定性:** 予定 — `at_mark` を超えて、テキスト表示からアニメーションを駆動すること、またその逆（たとえば表示される 1 文字ごとに追従するアニメーションや、アニメーションを待つ表示）はまだ設計されていません。

## クリック、スキップ、`animations` 設定 {#clicks-skip-and-the-animations-setting}

- ステップ中のクリックはタイムライン全体を完了させ、テキスト全体を表示します。ただし `Pause::Click` は自分のクリックを待ちます。
- まだ始まっていないワンショットサウンドは破棄されます。
- 次のクリックは `NextStep` です。スキップモードは即座に完了します。
- `animations` 設定（`Settings::animations`）が false のとき、すべての長さは 0 として扱われます。

どれも論理画面を変えません。すべてのアニメーションは `change` が設定した値で終わり、すべてのモーションは恒等変換で終わるからです。

## 列挙型：`Easing` {#easing}

```kis
pub enum Easing {
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InSine,
    OutSine,
    InOutSine,
    InExpo,
    OutExpo,
    InOutExpo,
    InCirc,
    OutCirc,
    InOutCirc,
    InBack,
    OutBack,
    InOutBack,
    InElastic,
    OutElastic,
    InOutElastic,
    InBounce,
    OutBounce,
    InOutBounce,
    CubicBezier(Decimal, Decimal, Decimal, Decimal),
    Steps(u32),
}
```

時間曲線です。

- 名前付きのイージングは Penner の式（[easings.net](https://easings.net)）に従います。`In` はゆっくり始まり、`Out` はゆっくり終わります。
- `CubicBezier(x1, y1, x2, y2)` は CSS の `cubic-bezier` に従います。`x1` と `x2` が `0.0..=1.0` の外にあると定数評価エラーです。
- `Steps(n)` は `n` 等分した各区間の終わりでジャンプします。`n = 0` は定数評価エラーです。
- 既定値はどこでも `Linear` です。

## 構造体：`Tween` {#tween}

```kis
pub struct Tween {
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
}
```

プロパティが古い値から新しい値へどう動くかです。トゥイーンは `Prop<T>` を、`cursor + delay` 時点の値から `change` で設定した値まで、`duration` かけて `easing` で動かします。[`Prop`](./screen.md#prop) を参照してください。

### `tween(duration)` {#tween-fn}

```kis
pub const fn tween(duration: Duration) -> Tween
```

- 戻り値：`duration` かけて進む、遅延なしの線形トゥイーン。
- 種別：`const fn` · ビルトイン：いいえ。

### `instant()` {#instant}

```kis
pub const fn instant() -> Tween
```

- 戻り値：`tween(ZERO)`、時間のかからないトゥイーン。即座に反映させたい変更に使います。
- 種別：`const fn` · ビルトイン：いいえ。

### `tween.ease(easing)`、`tween.delay(delay)` {#tween-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> Tween
pub const fn delay(mut self, delay: Duration) -> Tween
```

`ease` はこのトゥイーンに `easing` を設定したものを、`delay` はカーソルから `delay` 後に始まるものを返します。種別：`const fn` · ビルトイン：いいえ。

```kis
screen.entity(HANAKO).position().change(point(400, 80)).apply_animation(tween(ms(1000)).ease(Easing::InOutQuad));
```

## 列挙型：`Direction` {#direction}

```kis
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}
```

画面上の方向です。ワイプとブラインドで使います。

## 列挙型：`TransitionKind` {#transitionkind}

```kis
pub enum TransitionKind {
    Cut,
    Crossfade,
    Rule { mask: ImageAsset, softness: u8, reverse: bool },
    Wipe(Direction),
    Blinds { direction: Direction, count: u32 },
    ThroughColor { color: Color, hold: Duration },
    Masked { mask: Mask, softness: u8, reverse: bool },
}
```

レイヤーが古い内容を新しい内容にどう差し替えるかです。以下の `e(t)` はイージング適用後の進行度で、0 から 1 まで進みます。

| バリアント | 効果 |
| ------- | ------ |
| `Cut` | 開始時に切り替える |
| `Crossfade` | 古い内容がフェードアウトし、同時に新しい内容がフェードインする。古い不透明度 1 − e(t)、新しい不透明度 e(t) |
| `Rule { mask, softness, reverse }` | マスクの輝度が移動するしきい値以下になった画素から切り替わる |
| `Wipe(direction)` | 新しい内容を `direction` 方向に動きながら見せる |
| `Blinds { direction, count }` | `direction` に垂直な `count` 本の帯がそれぞれワイプする |
| `ThroughColor { color, hold }` | `color` へフェードし、`hold` の間保ってから新しい内容をフェードインする |
| `Masked { mask, softness, reverse }` | マスクの値 ×255 が移動するしきい値以下になった画素から切り替わる |

**ルール。** マスクの輝度が threshold(t) 以下になった画素は新しい内容を表示します。しきい値は 0 → 255、`reverse` なら 255 → 0 と移動します。`softness` は輝度単位でのブレンド幅で、0 は硬い境界です。

**マスク付き。** マスク画像の代わりに[マスクシェーダー](./screen-shader.md#mask)を使う同じ規則です。マスクはレイヤーの矩形の上で計算され、マスクの値 × 255 が threshold(t) 以下になった画素は新しい内容を表示します。マスクの時刻はトランジション開始からの時間なので、アニメーションするマスクはトランジションの間動きます。[`std::shaders::mask`](./shaders-mask.md) に既製のマスクがあるので、トランジションにマスク画像は要りません。

## 構造体：`Transition` {#transition}

```kis
pub struct Transition {
    pub kind: TransitionKind,
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
}
```

タイミング付きのレイヤー差し替えです。同じトランジションが `ImageLayer`、`LabelLayer`、`MovieLayer` の差し替えに使えます（[`Swap`](./screen.md#swap) を参照）。以下のコンストラクターはどれもイージング `Linear`、遅延 0 を設定します。いずれも `const fn` で、ビルトインではありません。

### トランジションのコンストラクター {#transition-constructors}

```kis
pub const fn cut() -> Transition
pub const fn fade(duration: Duration) -> Transition
pub const fn rule(mask: ImageAsset, duration: Duration) -> Transition
pub const fn rule_with(mask: ImageAsset, softness: u8, reverse: bool, duration: Duration) -> Transition
pub const fn masked(mask: Mask, duration: Duration) -> Transition
pub const fn masked_with(mask: Mask, softness: u8, reverse: bool, duration: Duration) -> Transition
pub const fn wipe(direction: Direction, duration: Duration) -> Transition
pub const fn blinds(direction: Direction, count: u32, duration: Duration) -> Transition
pub const fn through(color: Color, duration: Duration, hold: Duration) -> Transition
```

| コンストラクター | 種類 | 効果 |
| ----------- | ---- | ------ |
| `cut()` | `Cut`、長さ 0 | 即座の切り替え |
| `fade(d)` | `Crossfade` | `d` かけたクロスフェード |
| `rule(mask, d)` | `Rule`、softness 0、反転なし | 硬い境界のルール（マスク）トランジション |
| `rule_with(mask, softness, reverse, d)` | `Rule` | 同上。輝度 `softness` 単位のブレンド幅と向きを指定 |
| `masked(mask, d)` | `Masked`、softness 0、反転なし | 硬い境界の、マスクシェーダーによるトランジション |
| `masked_with(mask, softness, reverse, d)` | `Masked` | 同上。値 × 255 で `softness` 単位のブレンド幅と向きを指定 |
| `wipe(dir, d)` | `Wipe(dir)` | `dir` 方向に動くワイプ |
| `blinds(dir, count, d)` | `Blinds` | `dir` 方向にワイプする `count` 本の帯 |
| `through(color, d, hold)` | `ThroughColor` | 古い内容が `d / 2` かけて `color` へフェードし、`hold` 保ってから新しい内容が `d / 2` かけてフェードイン。合計 `d + hold` |

```kis
use std::screen::CENTER;
use std::screen::anim::masked;
use std::shaders::mask::radial;

const BLINDS_RULE: ImageAsset = asset("rule/blinds.png");

screen.background().change(BG::STATION).apply_animation(rule(BLINDS_RULE, ms(1000)));
screen.background().change(BG::NIGHT).apply_animation(through(BLACK, ms(1000), ms(200)));
screen.background().change(BG::ROOF).apply_animation(masked(radial(CENTER), ms(1000)));
```

### `transition.ease(easing)`、`transition.delay(delay)` {#transition-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> Transition
pub const fn delay(mut self, delay: Duration) -> Transition
```

`ease` はこのトランジションに `easing` を設定したものを、`delay` はカーソルから `delay` 後に始まるものを返します。種別：`const fn` · ビルトイン：いいえ。

## 構造体：`MotionKey` {#motionkey}

```kis
pub struct MotionKey {
    pub at: Duration,
    pub offset: Offset,
    pub scale: Decimal,
    pub rotation: Angle,
    pub easing: Easing,
}
```

モーションのキーです。現在値に対する相対変換で、`at` の時点で到達します。`easing` はこのキーで終わる区間のイージングです。

## 構造体：`Motion` {#motion}

```kis
pub struct Motion {
    pub keys: Vec<MotionKey>,
    pub repeat: u32,
    pub delay: Duration,
}
```

恒等変換で始まり恒等変換で終わる相対的な軌跡です。エンティティ（[`Entity::apply_animation`](./screen.md#entity-apply_animation)）またはカメラ（[`Camera`](./screen.md#camera)）で再生し、キャラクターが論理状態を変えずに跳ねる、うなずく、揺れるための手段です。

- 軌跡は t = 0 で恒等変換（オフセット (0, 0)、倍率 1.0、回転 0°）から始まります。キーは現在値に対する相対値で、その上に重ねて適用されます。
- 各キーの `easing` は、そのキーで終わる区間のイージングです。
- 1 回の繰り返しは最後のキーの `at` だけ続き、モーションは `cursor + delay` から `repeat` 回再生されます。

**恒等規則。** モーションは開始位置で終わらなければならないので、論理画面を変えることはありません。次は定数評価エラーです。

- キーがない。
- `at` が 0、または直前のキーより厳密に大きくない。
- 最後のキーがオフセット (0, 0)、倍率 1.0、回転 0° でない。
- `repeat` が 0。

### `motion()` {#motion-fn}

```kis
pub const fn motion() -> Motion
```

- 戻り値：キーのない、1 回再生のモーション（`repeat: 1`、遅延なし）。再生前にキーを追加します。
- 種別：`const fn` · ビルトイン：いいえ。

### `motion.key(at, offset)`、`motion.key_full(at, offset, scale, rotation)` {#motion-key}

```kis
pub const fn key(self, at: Duration, offset: Offset) -> Motion
pub const fn key_full(mut self, at: Duration, offset: Offset, scale: Decimal, rotation: Angle) -> Motion
```

`key` は倍率 1.0、回転 0°、イージング `Linear` のキーを追加します。`key_full` は指定した倍率と回転、イージング `Linear` のキーを追加します。種別：`const fn` · ビルトイン：いいえ。

### `motion.ease(easing)` {#motion-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> Motion
```

最後のキーで終わる区間のイージングを設定します。キーのないモーションでは何もしません。種別：`const fn` · ビルトイン：いいえ。

### `motion.repeat(times)`、`motion.delay(delay)` {#motion-repeat}

```kis
pub const fn repeat(mut self, times: u32) -> Motion
pub const fn delay(mut self, delay: Duration) -> Motion
```

`repeat` はこのモーションを `times` 回再生します。0 は評価エラーです。`delay` はカーソルから `delay` 後に開始させます。種別：`const fn` · ビルトイン：いいえ。

```kis
const fn hop_twice() -> Motion {
    motion()
        .key(ms(150), offset(0, -30)).ease(Easing::OutQuad)
        .key(ms(300), offset(0, 0)).ease(Easing::InQuad)
        .repeat(2)
}
```

## プリセット {#presets}

どのプリセットも恒等変換に戻り、すべてのキーで倍率 1.0、回転 0° です。いずれも `const fn` で、ビルトインではありません。

```kis
screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
screen.entity(AZUSA).apply_animation(shake(12, ms(400), 4));
screen.camera().apply_animation(quake(8, 4, ms(600)));
```

### 関数：`jump(height, duration)` {#jump}

```kis
pub const fn jump(height: i32, duration: Duration) -> Motion
```

`duration` かけて `height` だけ上がって着地します（repeat 1）。

| `at` | オフセット | イージング |
| ---- | ------ | ------ |
| `duration / 2` | (0, −height) | `OutQuad` |
| `duration` | (0, 0) | `InQuad` |

### 関数：`nod(depth, duration)` {#nod}

```kis
pub const fn nod(depth: i32, duration: Duration) -> Motion
```

`duration` かけて `depth` だけ沈んで戻ります（repeat 1）。

| `at` | オフセット | イージング |
| ---- | ------ | ------ |
| `duration / 2` | (0, depth) | `OutQuad` |
| `duration` | (0, 0) | `InQuad` |

### 関数：`shake(amplitude, duration, count)` {#shake}

```kis
pub const fn shake(amplitude: i32, duration: Duration, count: u32) -> Motion
```

`duration` の間に `amplitude` の幅で横に `count` 回揺れます。長さ c = `duration / count` の 1 周期を `count` 回繰り返します。`count` = 0 は `repeat` を通して評価エラーになります。

| `at` | オフセット | イージング |
| ---- | ------ | ------ |
| `c / 4` | (−amplitude, 0) | `Linear` |
| `3c / 4` | (amplitude, 0) | `Linear` |
| `c` | (0, 0) | `Linear` |

### 関数：`quake(horizontal, vertical, duration)` {#quake}

```kis
pub const fn quake(horizontal: i32, vertical: i32, duration: Duration) -> Motion
```

`duration` の間、50 ms ごとに `horizontal` と `vertical` の幅で斜めに揺れます（repeat 1、すべてのキーが `Linear`）。

| `at` | オフセット |
| ---- | ------ |
| k · 50 ms（k = 1 … (duration / 50 ms − 1)、k が奇数） | (horizontal, −vertical) |
| k · 50 ms（k = 1 … (duration / 50 ms − 1)、k が偶数） | (−horizontal, vertical) |
| `duration` | (0, 0) |
