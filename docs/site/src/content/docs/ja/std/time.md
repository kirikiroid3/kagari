---
title: "std::time"
---

> **安定性:** 安定 — ミリ秒単位の整数で表す時間の長さ。

`std::time` では、`std` で時間を表す唯一の型 `Duration` を宣言しています。補間の長さ、遅延、文字送りの速さ、一時停止、オートモードの待ち時間など、ライブラリで扱う時間はすべて、ミリ秒単位の整数を持つ `Duration` です。単位付きの文字列やフレーム数で時間を指定することはありません。

どのアイテムも普通の Kagari のコードで書かれていて（`#[builtin]` は付きません）、関数はすべて `const fn` です。プレリュードには `Duration`、`ms`、`secs` が含まれます。

## 概要 {#overview}

|アイテム|種別|概要|
|---|---|---|
|[`Duration`](#struct-duration)|構造体|ミリ秒単位の整数で表す時間の長さ|
|[`ZERO`](#constant-zero)|定数|長さ 0 の時間|
|[`ms(n)`](#function-msn)|`const fn`|`n` ミリ秒|
|[`secs(n)`](#function-secsn)|`const fn`|`n` 秒|
|[`Duration::plus`](#durationplusself-other)|`const fn`|和|
|[`Duration::times`](#durationtimesself-n)|`const fn`|乗算|
|[`Duration::div`](#durationdivself-n)|`const fn`|除算（切り捨て）|

## 構造体：`Duration` {#struct-duration}

```kis
pub struct Duration {
    pub ms: u32,
}
```

- `ms` `u32` — ミリ秒単位の長さ。
- 種別：— · 組み込み：いいえ

`Duration` はただのデータです。構造体リテラルで作れ、`==` で比較でき（`Eq` です）、フィールドも直接読めます。中身は Rust の整数なので有限型ではありません。そのため物語の中では、畳み込まれる定数式の中でしか `Duration` を使いません。

`Duration` がオーバーフローやアンダーフローすると評価エラーになります。定数評価ではエラー [`E4013`](../diagnostics/eval.md#e4013) です。

### `Duration::plus(self, other)` {#durationplusself-other}

```kis
pub const fn plus(self, other: Duration) -> Duration
```

- `other` `Duration` — 加える長さ。
- 戻り値：`Duration` — 二つの長さの和。
- 種別：`const fn` · 組み込み：いいえ

### `Duration::times(self, n)` {#durationtimesself-n}

```kis
pub const fn times(self, n: u32) -> Duration
```

- `n` `u32` — 倍数。
- 戻り値：`Duration` — 元の長さの `n` 倍。
- 種別：`const fn` · 組み込み：いいえ

### `Duration::div(self, n)` {#durationdivself-n}

```kis
pub const fn div(self, n: u32) -> Duration
```

- `n` `u32` — 除数。
- 戻り値：`Duration` — 元の長さを `n` で割り、端数を切り捨てたもの。
- 種別：`const fn` · 組み込み：いいえ

`div` の端数は切り捨てられ、`ms(1000).div(3)` は `ms(333)` になります。`n == 0` のときはゼロ除算です（定数評価ではエラー [`E4002`](../diagnostics/eval.md#e4002)）。

```kis
const BEAT: Duration = ms(400);
const BAR: Duration = BEAT.times(4);
const HALF_BAR: Duration = BAR.div(2);
const INTRO: Duration = secs(2).plus(HALF_BAR);
```

## 定数：`ZERO` {#constant-zero}

```kis
pub const ZERO: Duration = Duration { ms: 0 };
```

長さ 0 の時間です。プレリュードには含まれないので、`use std::time::ZERO;` でインポートします。`text_speed` 設定を `ZERO` にすると、テキストが一度に表示されます。

## 関数：`ms(n)` {#function-msn}

```kis
pub const fn ms(n: u32) -> Duration
```

- `n` `u32` — ミリ秒。
- 戻り値：`Duration` — `n` ミリ秒。
- 種別：`const fn` · 組み込み：いいえ

## 関数：`secs(n)` {#function-secsn}

```kis
pub const fn secs(n: u32) -> Duration
```

- `n` `u32` — 秒。
- 戻り値：`Duration` — `n` 秒（`n * 1000` ミリ秒）。
- 種別：`const fn` · 組み込み：いいえ

`n * 1000` が `u32` の範囲を超えると、`secs(n)` はオーバーフローします。
