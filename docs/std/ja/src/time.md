# `std::time` {#stdtime}

> **安定性:** 安定 — ミリ秒単位の整数で表す時間の長さ。

`std::time` は、`std` における唯一の時間の型 `Duration` を宣言します。ライブラリのすべての時間（トゥイーンの長さ、遅延、文字送りの速さ、ポーズ、オートモードの待ち時間）は、ミリ秒単位の整数の `Duration` です。単位付き文字列もフレーム数もありません。

すべてのアイテムは通常の Kagari コード（`#[builtin]` なし）で、関数はすべて `const fn` です。プレリュードは `Duration`、`ms`、`secs` をインポートします。

## 概要 {#overview}

|アイテム|種類|概要|
|---|---|---|
|[`Duration`](#struct-duration)|構造体|ミリ秒単位の整数で表す時間の長さ|
|[`ZERO`](#constant-zero)|定数|長さ 0 の時間|
|[`ms(n)`](#function-msn)|`const fn`|`n` ミリ秒|
|[`secs(n)`](#function-secsn)|`const fn`|`n` 秒|
|[`Duration::plus`](#durationplusself-other)|`const fn`|和|
|[`Duration::times`](#durationtimesself-n)|`const fn`|乗算|
|[`Duration::div`](#durationdivself-n)|`const fn`|除算（切り捨て）|

## Struct: `Duration` {#struct-duration}

```kis
pub struct Duration {
    pub ms: u32,
}
```

- `ms` `u32` — ミリ秒単位の長さ。
- Kind: — · Builtin: no

`Duration` は単純なデータです：構造体リテラルで作れ、`==` で比較でき（`Eq` です）、フィールドを直接読めます。Rust の整数を保持するため有限型ではありません：物語は畳み込まれる定数式の中でのみ `Duration` を使います。

`Duration` のオーバーフローやアンダーフローは評価エラーで、定数評価では [`E4013`](../../../reference/ja/src/diagnostics/eval.md#e4013) です。

### `Duration::plus(self, other)` {#durationplusself-other}

```kis
pub const fn plus(self, other: Duration) -> Duration
```

- `other` `Duration` — 加える長さ。
- Returns: `Duration` — 2 つの長さの和。
- Kind: `const fn` · Builtin: no

### `Duration::times(self, n)` {#durationtimesself-n}

```kis
pub const fn times(self, n: u32) -> Duration
```

- `n` `u32` — 倍数。
- Returns: `Duration` — この長さの `n` 倍。
- Kind: `const fn` · Builtin: no

### `Duration::div(self, n)` {#durationdivself-n}

```kis
pub const fn div(self, n: u32) -> Duration
```

- `n` `u32` — 除数。
- Returns: `Duration` — この長さを `n` で割り、切り捨てたもの。
- Kind: `const fn` · Builtin: no

`div` は切り捨てます：`ms(1000).div(3)` は `ms(333)` です。`n == 0` はゼロ除算です（定数評価では [`E4002`](../../../reference/ja/src/diagnostics/eval.md#e4002)）。

```kis
const BEAT: Duration = ms(400);
const BAR: Duration = BEAT.times(4);
const HALF_BAR: Duration = BAR.div(2);
const INTRO: Duration = secs(2).plus(HALF_BAR);
```

## Constant: `ZERO` {#constant-zero}

```kis
pub const ZERO: Duration = Duration { ms: 0 };
```

長さ 0 の時間です。プレリュードには含まれないので、`use std::time::ZERO;` と書きます。`text_speed` 設定を `ZERO` にすると、テキストは即座に表示されます。

## Function: `ms(n)` {#function-msn}

```kis
pub const fn ms(n: u32) -> Duration
```

- `n` `u32` — ミリ秒。
- Returns: `Duration` — `n` ミリ秒。
- Kind: `const fn` · Builtin: no

## Function: `secs(n)` {#function-secsn}

```kis
pub const fn secs(n: u32) -> Duration
```

- `n` `u32` — 秒。
- Returns: `Duration` — `n` 秒、つまり `n * 1000` ミリ秒。
- Kind: `const fn` · Builtin: no

`n * 1000` が `u32` を超える `secs(n)` はオーバーフローします。
