---
title: "std::screen::anim"
---

> **安定性:** 安定 — アニメーションの値。イージング、プロパティの補間、レイヤーのトランジション、相対的なモーションを扱います。

レンダーで画面を変更するときは、必ずアニメーションの仕方も指定します。このモジュールはそのための値を宣言します。[`Easing`](#easing) は時間曲線、[`Tween`](#tween) は `Prop` の変化、[`Transition`](#transition) はレイヤーの差し替え、[`Motion`](#motion) はエンティティやカメラに加える相対的な軌跡を表します。開始のタイミングは [`Screen`](./screen.md#timeline-methods) のタイムライン用メソッドで決めます。アイテムはどれも `const fn` か単純なデータです。

プレリュードには `Easing`、`Tween`、`Transition`、`Motion`、`tween`、`instant`、`cut`、`fade`、`rule`、`wipe`、`through`、`motion`、`jump`、`nod`、`shake`、`quake` が含まれます。それ以外（`Direction`、`TransitionKind`、`MotionKey`、`rule_with`、`masked`、`masked_with`、`blinds`）は `use std::screen::anim::…` で取り込みます。

## 概要 {#overview}

| アイテム | 種類 | 概要 |
| ---- | ---- | ------- |
| [`Easing`](#easing) | 列挙型 | 時間曲線 |
| [`Tween`](#tween)、`tween`、`instant` | 構造体とコンストラクター | プロパティの変化の仕方 |
| [`Direction`](#direction) | 列挙型 | 画面上の方向 |
| [`TransitionKind`](#transitionkind)、[`Transition`](#transition)、`cut`、`fade`、`rule`、`rule_with`、`masked`、`masked_with`、`wipe`、`blinds`、`through` | 列挙型、構造体とコンストラクター | レイヤーの差し替え方 |
| [`MotionKey`](#motionkey)、[`Motion`](#motion)、`motion` | 構造体とコンストラクター | 相対的な軌跡 |
| [`jump`](#jump)、[`nod`](#nod)、[`shake`](#shake)、[`quake`](#quake) | 関数 | 既製のモーション |

## タイムライン {#the-timeline}

ステップにはそれぞれタイムラインがあります。レンダーは**カーソル**を使って、アニメーションをタイムライン上に予約します。カーソルの位置は、基準点（ステップの開始時点、またはテキストのマーク）とミリ秒単位のずれの組で表します。ステップの開始時点ではカーソルは（開始, 0 ms）にあり、テキストの表示もここから始まります。

```kis
impl Screen {
    #[builtin] pub const fn wait(&mut self, duration: Duration);
    #[builtin] pub const fn barrier(&mut self);
    #[builtin] pub const fn at_mark(&mut self, mark: Mark);
    #[builtin] pub const fn at_entry(&mut self);
    #[builtin] pub const fn begin_text(&mut self);
}
```

- アニメーション、トランジション、モーション、サウンドは、どれも `cursor + delay` の時点で始まります。カーソルを動かさずに続けて書いた文は、同時に始まります。
- `wait(d)` はカーソルを `d` だけ進めます。
- `barrier()` は、それまでに予約したもの（アニメーション、トランジション、モーション、削除、単発のサウンド）がすべて終わる時点までカーソルを進めます。サウンドの長さはエンジンが求めます。
- `at_mark(m)` はカーソルを（マーク `m`, 0）に移します。マークの時刻は、テキスト表示が `mark(n)` に達した時点です（[`std::narrative::text`](./narrative-text.md#marks) を参照）。テキスト中にないマークは、表示の終わりの時点として扱います。
- `at_entry()` はカーソルを（開始, 0）に戻します。
- `begin_text()` は、カーソルの位置でテキスト表示とボイスを始めます。カーソルがマークを基準にしている間に呼ぶと、定数評価エラーになります。

タイムラインはそのステップの中だけのもので、[論理画面](./screen.md#the-logical-screen)には含まれません。

<!-- diagram: screen-anim-timeline (mermaid-ish)
gantt
  section アニメーション
    背景 fade(ms(500))      : 0, 500
    テキスト表示とボイス   : 800, 1400
    表情 fade(ms(300))      : mark(1) から, 300
  section カーソル
    1 開始, 0 ms ==> 2 barrier() 500 ms ==> 3 wait(ms(300)) 800 ms, begin_text() ==> 4 at_mark(Mark(1))
-->
{{#include diagrams/screen-anim-timeline.svg}}

上の図に対応するレンダーです。

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

CG が表示し終わるまでテキストウィンドウを隠しておく例です。

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

> **安定性:** 予定 — `at_mark` 以上にテキスト表示とアニメーションを連動させる仕組み（表示される 1 文字ごとに追従するアニメーションや、アニメーションの終わりを待つ表示など）は、まだ設計していません。

## クリック、スキップ、`animations` 設定 {#clicks-skip-and-the-animations-setting}

- ステップの途中でクリックすると、タイムラインがすべて完了し、テキストも全文が表示されます。ただし `Pause::Click` は、それ自身のクリックを待ちます。
- まだ始まっていない単発のサウンドは破棄します。
- その次のクリックで `NextStep` になります。スキップモードではすぐに完了します。
- `animations` 設定（`Settings::animations`）が false のときは、長さをすべて 0 として扱います。

どの場合も論理画面は変わりません。アニメーションは必ず `change` で設定した値で終わり、モーションは必ず恒等変換で終わるからです。

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

- 名前の付いたイージングは Penner の式（[easings.net](https://easings.net)）に従います。`In` はゆっくり始まり、`Out` はゆっくり終わります。
- `CubicBezier(x1, y1, x2, y2)` は CSS の `cubic-bezier` と同じです。`x1` か `x2` が `0.0..=1.0` の範囲外だと定数評価エラーになります。
- `Steps(n)` は全体を `n` 等分し、各区間の終わりで値が跳びます。`n = 0` は定数評価エラーです。
- 既定値は常に `Linear` です。

## 構造体：`Tween` {#tween}

```kis
pub struct Tween {
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
}
```

プロパティを古い値から新しい値へ変化させる方法です。`Prop<T>` の値を、`cursor + delay` 時点の値から `change` で設定した値まで、`duration` をかけて `easing` に沿って補間します。[`Prop`](./screen.md#prop) も参照してください。

### `tween(duration)` {#tween-fn}

```kis
pub const fn tween(duration: Duration) -> Tween
```

- 戻り値：`duration` をかけて進む、遅延なしの線形の補間。
- 種別：`const fn` · 組み込み：いいえ。

### `instant()` {#instant}

```kis
pub const fn instant() -> Tween
```

- 戻り値：`tween(ZERO)`。時間をかけない補間で、変更をすぐに反映したいときに使います。
- 種別：`const fn` · 組み込み：いいえ。

### `tween.ease(easing)`、`tween.delay(delay)` {#tween-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> Tween
pub const fn delay(mut self, delay: Duration) -> Tween
```

`ease` はイージングを `easing` にした補間を返し、`delay` はカーソルから `delay` 遅れて始まる補間を返します。種別：`const fn` · 組み込み：いいえ。

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

レイヤーの古い内容を新しい内容に差し替える方法です。以下の `e(t)` はイージングを適用した進行度で、0 から 1 まで変化します。

| バリアント | 効果 |
| ------- | ------ |
| `Cut` | 開始と同時に切り替える |
| `Crossfade` | 古い内容を消しながら新しい内容を表示する。不透明度は古い内容が 1 − e(t)、新しい内容が e(t) |
| `Rule { mask, softness, reverse }` | 動いていくしきい値をマスクの輝度が下回った画素から切り替える |
| `Wipe(direction)` | `direction` の向きに拭うように新しい内容を表示する |
| `Blinds { direction, count }` | `direction` と垂直な `count` 本の帯ごとにワイプする |
| `ThroughColor { color, hold }` | いったん `color` に変わり、`hold` の間そのまま保ってから新しい内容を表示する |
| `Masked { mask, softness, reverse }` | 動いていくしきい値をマスクの値 ×255 が下回った画素から切り替える |

**ルール。** マスクの輝度が threshold(t) 以下になった画素から、新しい内容を表示します。しきい値は 0 → 255 と動き、`reverse` のときは 255 → 0 と動きます。`softness` は境界をぼかす幅（輝度の単位）で、0 ならくっきりした境界になります。

**マスク付き。** 規則はルールと同じですが、マスク画像の代わりに[マスクシェーダー](./screen-shader.md#mask)を使います。マスクはレイヤーの矩形上で計算し、マスクの値 × 255 が threshold(t) 以下になった画素から新しい内容を表示します。マスクに渡す時刻はトランジション開始からの経過時間なので、動くマスクはトランジションの間ずっと動きます。既製のマスクが [`std::shaders::mask`](./shaders-mask.md) にあるため、マスク画像を用意しなくてもトランジションを作れます。

## 構造体：`Transition` {#transition}

```kis
pub struct Transition {
    pub kind: TransitionKind,
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
}
```

時間指定付きのレイヤーの差し替えです。`ImageLayer`、`LabelLayer`、`MovieLayer` のどれにも同じトランジションを使えます（[`Swap`](./screen.md#swap) を参照）。以下のコンストラクターはいずれもイージングを `Linear`、遅延を 0 にします。どれも `const fn` で、組み込みではありません。

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
| `cut()` | `Cut`、長さ 0 | 即座に切り替え |
| `fade(d)` | `Crossfade` | `d` をかけたクロスフェード |
| `rule(mask, d)` | `Rule`、softness 0、反転なし | 境界がくっきりしたルール（マスク画像）トランジション |
| `rule_with(mask, softness, reverse, d)` | `Rule` | 上と同じ。境界のぼかし幅（輝度で `softness`）と向きを指定 |
| `masked(mask, d)` | `Masked`、softness 0、反転なし | マスクシェーダーを使った、境界がくっきりしたトランジション |
| `masked_with(mask, softness, reverse, d)` | `Masked` | 上と同じ。境界のぼかし幅（値 × 255 で `softness`）と向きを指定 |
| `wipe(dir, d)` | `Wipe(dir)` | `dir` の向きのワイプ |
| `blinds(dir, count, d)` | `Blinds` | `count` 本の帯がそれぞれ `dir` の向きにワイプ |
| `through(color, d, hold)` | `ThroughColor` | 古い内容が `d / 2` かけて `color` に変わり、`hold` の間保ってから、新しい内容が `d / 2` かけて現れる。合計 `d + hold` |

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

`ease` はイージングを `easing` にしたトランジションを返し、`delay` はカーソルから `delay` 遅れて始まるトランジションを返します。種別：`const fn` · 組み込み：いいえ。

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

モーションのキーです。現在の値からの相対的な変換を表し、`at` の時点でその変換に達します。`easing` は、このキーで終わる区間に使うイージングです。

## 構造体：`Motion` {#motion}

```kis
pub struct Motion {
    pub keys: Vec<MotionKey>,
    pub repeat: u32,
    pub delay: Duration,
}
```

恒等変換で始まって恒等変換で終わる、相対的な軌跡です。エンティティ（[`Entity::apply_animation`](./screen.md#entity-apply_animation)）やカメラ（[`Camera`](./screen.md#camera)）に適用します。キャラクターを跳ねさせたり、うなずかせたり、揺らしたりしても、論理状態は変わりません。

- t = 0 の時点では恒等変換（ずれ (0, 0)、倍率 1.0、回転 0°）です。各キーは現在の値からの相対値で、現在の値に重ねて適用します。
- 各キーの `easing` は、そのキーで終わる区間のイージングです。
- 1 回分の長さは最後のキーの `at` です。モーションは `cursor + delay` から `repeat` 回再生します。

**恒等規則。** モーションは必ず開始時の状態に戻るので、論理画面を変えることはありません。次の場合は定数評価エラーになります。

- キーが 1 つもない。
- `at` が 0、または直前のキーの `at` 以下。
- 最後のキーがずれ (0, 0)、倍率 1.0、回転 0° になっていない。
- `repeat` が 0。

### `motion()` {#motion-fn}

```kis
pub const fn motion() -> Motion
```

- 戻り値：キーのない、1 回だけ再生するモーション（`repeat: 1`、遅延なし）。再生する前にキーを追加します。
- 種別：`const fn` · 組み込み：いいえ。

### `motion.key(at, offset)`、`motion.key_full(at, offset, scale, rotation)` {#motion-key}

```kis
pub const fn key(self, at: Duration, offset: Offset) -> Motion
pub const fn key_full(mut self, at: Duration, offset: Offset, scale: Decimal, rotation: Angle) -> Motion
```

`key` は倍率 1.0、回転 0°、イージング `Linear` のキーを追加します。`key_full` は倍率と回転を指定でき、イージングは `Linear` です。種別：`const fn` · 組み込み：いいえ。

### `motion.ease(easing)` {#motion-ease}

```kis
pub const fn ease(mut self, easing: Easing) -> Motion
```

最後に追加したキーで終わる区間のイージングを設定します。キーがなければ何もしません。種別：`const fn` · 組み込み：いいえ。

### `motion.repeat(times)`、`motion.delay(delay)` {#motion-repeat}

```kis
pub const fn repeat(mut self, times: u32) -> Motion
pub const fn delay(mut self, delay: Duration) -> Motion
```

`repeat` はモーションを `times` 回再生するようにします。0 を渡すと評価エラーです。`delay` はカーソルから `delay` 遅れて始めるようにします。種別：`const fn` · 組み込み：いいえ。

```kis
const fn hop_twice() -> Motion {
    motion()
        .key(ms(150), offset(0, -30)).ease(Easing::OutQuad)
        .key(ms(300), offset(0, 0)).ease(Easing::InQuad)
        .repeat(2)
}
```

## 既製のモーション {#presets}

どれも最後は恒等変換に戻り、すべてのキーで倍率 1.0、回転 0° です。いずれも `const fn` で、組み込みではありません。

```kis
screen.entity(HANAKO).apply_animation(jump(20, ms(300)));
screen.entity(AZUSA).apply_animation(shake(12, ms(400), 4));
screen.camera().apply_animation(quake(8, 4, ms(600)));
```

### 関数：`jump(height, duration)` {#jump}

```kis
pub const fn jump(height: i32, duration: Duration) -> Motion
```

`duration` の間に `height` だけ跳び上がって着地します（1 回）。

| `at` | ずれ | イージング |
| ---- | ------ | ------ |
| `duration / 2` | (0, −height) | `OutQuad` |
| `duration` | (0, 0) | `InQuad` |

### 関数：`nod(depth, duration)` {#nod}

```kis
pub const fn nod(depth: i32, duration: Duration) -> Motion
```

`duration` の間に `depth` だけ沈んで戻ります（1 回）。

| `at` | ずれ | イージング |
| ---- | ------ | ------ |
| `duration / 2` | (0, depth) | `OutQuad` |
| `duration` | (0, 0) | `InQuad` |

### 関数：`shake(amplitude, duration, count)` {#shake}

```kis
pub const fn shake(amplitude: i32, duration: Duration, count: u32) -> Motion
```

`duration` の間に、幅 `amplitude` で左右に `count` 回揺れます。長さ c = `duration / count` の 1 往復を `count` 回繰り返します。`count` = 0 のときは `repeat` に 0 が渡り、評価エラーになります。

| `at` | ずれ | イージング |
| ---- | ------ | ------ |
| `c / 4` | (−amplitude, 0) | `Linear` |
| `3c / 4` | (amplitude, 0) | `Linear` |
| `c` | (0, 0) | `Linear` |

### 関数：`quake(horizontal, vertical, duration)` {#quake}

```kis
pub const fn quake(horizontal: i32, vertical: i32, duration: Duration) -> Motion
```

`duration` の間、50 ms ごとに横 `horizontal`、縦 `vertical` の幅で斜めに揺れます（1 回、キーはすべて `Linear`）。

| `at` | ずれ |
| ---- | ------ |
| k · 50 ms（k = 1 … (duration / 50 ms − 1)、k が奇数） | (horizontal, −vertical) |
| k · 50 ms（k = 1 … (duration / 50 ms − 1)、k が偶数） | (−horizontal, vertical) |
| `duration` | (0, 0) |
