---
title: "早見表"
---

言語の要点を 1 ページにまとめた早見表です。詳しい規則は、各節のリンク先の章を参照してください。

## 関数種別 {#function-kinds}

| 種別 | 呼び出せるもの | 実行時に保持できる値 | ステップ | 物語変数 |
| ---- | -------------- | -------------------- | -------- | -------- |
| `fn` | `fn`、`const fn`。`nav::start` で開始シーンを始める | 制限なし | 不可 | 不可 |
| `#[scene] pushdown fn`（シーン） | シーン、即時手続き、`const fn`（引数は定数か有限の値） | [有限型](../concepts/kinds.md#finite-types) | 可 | 読み書き可 |
| `pushdown fn`（即時手続き） | 即時手続き、`const fn`。再帰は不可、`for` ループは有界なもののみ | 有限型 | 不可 | 読み書き可 |
| `const fn` | `const fn` | 制限なし | 不可 | 不可 |

ステップの実行、シーンの呼び出し、`rng_choose` の呼び出しができるのはシーンだけです。`pushdown` がキーワードになるのは `fn` か `Fn` の直前だけで、`scene` はポインタ型 `scene fn(…) -> …` の中だけです。詳しくは[関数種別](../concepts/kinds.md)を参照してください。

| ポインタ型 | 指す先 | クロージャ型 |
| ---------- | ------ | ------------ |
| `fn(A) -> B` | `fn` | `Fn(A) -> B` |
| `const fn(A) -> B` | `const fn`、または何もキャプチャしないクロージャ | `const Fn(A) -> B` |
| `pushdown fn(A) -> B` | 即時手続き | `pushdown Fn(A) -> B` |
| `scene fn(A) -> B` | シーン | なし |

## 属性 {#attributes}

| 属性 | 付ける対象 | 意味 |
| ---- | ---------- | ---- |
| `#[scene]` | `pushdown fn` | シーンにする。ステップの実行、シーンの呼び出し、乱数の取得ができる |
| `#[entry]` | 引数と戻り値の型が有限で、`Text` を含まない `#[scene]` | システムがここから物語を始められる |
| `#[must_use]` | `struct`、`enum` | 値を捨てたり完了させなかったりすると警告 `W3023` |
| `#[shader_arg]` | フィールドに名前があり、どれもシェーダーが読める型の `struct` | その構造体をシェーダーの引数にできる |
| `#[builtin]` | 関数、構造体、トレイト | コンパイラとエンジンが実装する。標準ライブラリ専用 |

引数を取る属性はありません。詳しくは[属性](../special/attributes.md)を参照してください。

## 特別な型 {#special-types}

| 型 | 有限 | `Eq` | 備考 |
| -- | ---- | ---- | ---- |
| `bool`、`()`、`Int<lo, hi>` | はい | はい | 物語で使う整数型は `Int<lo, hi>` |
| Rust の整数型、`Rat`、`Decimal` | いいえ | はい | `0.5` は `Decimal` |
| `Text` | はい | はい | 文字列リテラルと `t![…]` の型 |
| アセット型 | はい | はい | 値はアセット定数に限られる |
| `Option<T>`、構造体、列挙型、タプル、配列 | 構成要素がすべて有限なら | 構成要素がすべて `Eq` なら | 物語では再帰的な型は使えない |
| `scene fn`、`pushdown fn`、`const fn` のポインタ、`Render` | はい | はい（同じ関数かどうか） | |
| `fn` のポインタ | いいえ | はい（同じ関数かどうか） | |
| クロージャ | いいえ | いいえ | 物語では第二級 |
| `String`、`Vec<T>`、`Duration`、`Color`、`EntityId` | いいえ | はい（`Vec` は `T: Eq` のとき） | 定数なら物語でも使える |
| `Screen` | いいえ | はい | レンダーが受け取って返す型 |
| `Setting<T>`、`Meta<T>`、`Narrative`、`Element`、画面の部品 | いいえ | いいえ | 中身の見えないハンドルや値 |

[組み込み型](../special/builtin-types.md)と[言語トレイト](../special/traits.md)も参照してください。

## 組み込み構文（`.kix` のみ） {#forms}

| 括弧 | 書けるもの |
| ---- | ---------- |
| `name!( … )` | 属性と、決まった数の要素 |
| `name![ … ]` | 要素だけを任意の数 |
| `name!{ … }` | 属性と、任意の数の要素 |

属性は `key = expr` の形で書き、要素より前に置きます。同じ属性は一度しか書けません。要素はカンマで区切ります。ただし `t![…]` の項目だけは、区切らずに並べて書きます。

| 構文 | 展開先 | 値 | 属性 | 要素 |
| ---- | ------ | -- | ---- | ---- |
| `s!(…)` | `push_step(Say { … })` | `()` | `who`、`voice`、`render`、`history` | `t![…]` を 1 つまで |
| `choice!{…}` | `push_step(Choice { … })` | アームの値の型 | `id`（必須。文字列リテラル）、`render` | アーム `t![…] => V` と `t![…] if cond => V` |
| `t![…]` | `ruby`、`mono_ruby`、`effect`、`to_text` の呼び出しを並べた `concat([…])` | `Text` | なし | `"…"`、`["基"]("よみ")`、`["漢字"]("かん"\|"じ")`、`!["…"](effect)`、`{expr}` |
| `column![…]`、`row![…]`、`layer![…]` | `ui::column`、`ui::row`、`ui::layer` | `Element` | なし | 任意の数 |
| `text!(…)` | `ui::text` | `Element` | `style` | ちょうど 1 つ |
| `image!(…)` | `ui::image` | `Element` | `fit` | ちょうど 1 つ |
| `game_screen!(…)` | `ui::game_screen` | `Element` | `game`（必須） | なし |
| `button!{…}` | `ui::button` | `Element` | `on_click`、`disabled`、`style` | 任意の数 |
| `slider!{…}` | `ui::slider` | `Element` | `value`（必須）、`on_change`、`step`、`disabled`、`style` | なし |
| `toggle!{…}` | `ui::toggle` | `Element` | `value`（必須）、`on_change`、`disabled`、`style` | 任意の数 |
| `select!{…}` | `ui::select` | `Element` | `value`（必須）、`on_change`、`disabled`、`style` | 任意の数 |

`s!` と `choice!` はシーンの中でしか使えません。UI 構文は `fn` の関数を呼び出すので、`fn` のコードで使います。詳しくは[組み込み構文](../syntax/forms.md)と[ステップ](../concepts/steps.md)を参照してください。

## 物語のアイテム {#story-items}

| アイテム | 書き方 |
| -------- | ------ |
| シーン | `#[scene] pushdown fn Name(…) -> R { … }` |
| 開始シーン | `#[scene] #[entry] pub pushdown fn Main(…) -> Ending { … }` |
| 即時手続き | `pushdown fn name(…) -> R { … }` |
| 物語変数 | `static mut NAME: FiniteType = constant;` |
| 設定 | `static NAME: Setting<T> = setting(init);` |
| メタデータ | `static NAME: Meta<T> = meta(init);` |
| アセット | `const NAME: ImageAsset = asset("path");`。`AudioAsset`、`VoiceAsset`、`VideoAsset`、`FontAsset` も同様 |
| シェーダー | `const NAME: ColorShader<Args> = asset("name.wgsl");`。`MixShader`、`TextureShader`、`MaskShader`、`LocalMotionShader`、`TransitionalMotionShader` も同様。`Args` は `()` か `#[shader_arg]` 構造体 |
| システムフォント | `const NAME: FontAsset = system_font("path");` |
| エンティティ ID | `const NAME: EntityId = EntityId(n);`。`n` は 1 以上（0 は `BACKGROUND`） |
| レンダー | `const fn name(mut screen: Screen) -> Screen { …; screen }` |
| プログラム | ルートモジュールに `fn main() -> System { … }` |

## 物語の制御フロー {#control-flow-in-the-story}

| したいこと | 書き方 |
| ---------- | ------ |
| 台詞を表示する | `s!(who = "Mio", t!["…"]);` |
| 選択肢を出す | `match choice!{id = "ask", t!["Yes"] => Answer::Yes, t!["No"] => Answer::No} { … }` |
| アームを隠す | `t!["…"] if condition => V` |
| シーンを呼ぶ | `Name(x);` または `let v = Name(x);` |
| シーンから抜ける | `return;` / `return value;`、またはブロック末尾の式 |
| 分岐する | `if`、`if let`、`match`（網羅的。範囲やタプルも使える） |
| 拠点や繰り返しの選択肢 | `loop { match choice!{…} { … } }` と `break` |
| 回数の決まった繰り返し | 範囲が定数の `for i in 0..n` |
| カウンターを増減する | `x += 1;`（`Int` の範囲で飽和する） |
| 物語の状態から値を計算する | `const fn` を呼ぶ。有限の実行時引数は表化される |
| メタデータに書き込む | `META_FIELD.write(v);` |
| 乱数を引く（シーンのみ） | `rng_choose::<T>()`。`T` はフィールドのない列挙型か `Int<lo, hi>` |
| 物語を終える | 開始シーンから戻る。戻り値は `nav::start` の `on_exit` に渡る |

## よく使う `std` の呼び出し {#common-std-calls}

| したいこと | 呼び出し | 種別 |
| ---------- | -------- | ---- |
| 時間 | `ms(500)`、`secs(2)` | `const fn` |
| 色 | `rgb(200, 0, 0)`、`hex(0xC80000)`、`BLACK`、`WHITE`、`TRANSPARENT` | `const fn` |
| 位置 | `point(640, 360)`、`offset(0, -20)` | `const fn` |
| 背景を変える | `screen.background().change(BG_ROOF).apply_animation(fade(ms(500)));` | `const fn` |
| 立ち絵を差し替える | `screen.entity(MIO).image().change(MIO_SMILE).apply_animation(cut());` | `const fn` |
| エンティティをフェードインさせる | `screen.entity(MIO).opacity().change(1.0).apply_animation(tween(ms(300)));` | `const fn` |
| モーションを再生する | `screen.entity(MIO).apply_animation(jump(20, ms(300)));` | `const fn` |
| キャラクターを表示する | `character(MIO).show(&mut screen, MIO_SMILE, point(320, 0), tween(ms(300)));` | `const fn` |
| BGM を変える | `screen.music().change(looped(THEME)).apply_animation(crossfade(secs(1)));` | `const fn` |
| 効果音を鳴らす | `screen.play(sound(CHIME));` | `const fn` |
| レンダーの中で待つ | `screen.wait(ms(500));` | `const fn` |
| テキストを装飾する | `t![!["red"](color_text(rgb(200, 0, 0))) ![""](click_wait())]` | `t![…]` の中 |
| 整数を変換する | `x.into()`、`x.saturate()` | `const fn` |
| 設定を読む・変える | `MUSIC.get()`、`MUSIC.set(v)` | `fn` |
| メタデータを読む | `CLEARED.get()` | `fn` |
| 物語を始める | `nav::start(story::Main, (CLEARED.get(),), on_ending)` | `fn` |
| モードを切り替える | `nav::call(Config)`、`nav::dialog(Confirm)`、`nav::back()`、`nav::goto(Title)`、`nav::load(slot)`、`nav::quit()` | `fn` |
| リアクティブな値 | `let (count, set_count) = signal(0u32);` | `fn` |

上の表の名前は、制作者が定義した定数（`BG_ROOF`、`MIO` など）を除き、すべてプレリュードにあります。API の全体は[標準ライブラリのリファレンス](../../std/usage.md)を参照してください。
