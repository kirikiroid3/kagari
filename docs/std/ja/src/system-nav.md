# `std::system::nav` {#stdsystemnav}

> **安定性:** 安定 — システムのモードスタックを変更する操作。

`std::system::nav` はナビゲーション操作をまとめたモジュールです。UI がモードスタックを変更する手段は、これらの関数と `Narrative` ハンドルのメソッドだけです。すべてビルトインの `fn` なので、任意の `fn` のコード（ボタンのハンドラ、エフェクト、終了ハンドラ）から呼べますが、レンダーや物語からは呼べません。

プレリュードがモジュール自体をインポートするので、`use` なしで `nav::call(Config)`、`nav::back()` のように呼びます。

モードの種類と、それぞれがスタックのどこに置けるかは [モード型](./system.md#mode-types) を参照してください。

## 概要 {#overview}

| 関数 | 効果 |
| ---- | ---- |
| [`nav::call(o)`](#call) | オーバーレイ `o` を積みます |
| [`nav::dialog(d)`](#dialog) | ダイアログ `d` を積みます |
| [`nav::back()`](#back) | 最上位のモードを取り除きます。ベースでは無視されます |
| [`nav::goto(p)`](#goto) | スタック全体をルートページ `p` で置き換えます |
| [`nav::start(e, args, on_exit)`](#start) | スタックを、エントリーシーン `e` を実行する新しいゲームで置き換えます |
| [`nav::load(slot)`](#load) | スタックを、`slot` から復元したゲームで置き換えます |
| [`nav::quit()`](#quit) | プログラムを即座に終了します |

実行中のゲームに対する操作は `Narrative` ハンドルのメソッドです（[`std::narrative`](./narrative.md) を参照）。

| 操作 | 効果 |
| ---- | ---- |
| `game.advance()` | 物語に `NextStep` を送ります |
| `game.select(arm)` | 現在の選択肢の、表示中のアームを選びます |
| `game.save(slot)` | 実行中のゲームを `slot` に書き込みます |
| `game.rewind(i)` | 物語をバックログのエントリー `i` まで戻します |
| `game.call(o)` | ゲームオーバーレイ `o` を積みます |

ハンドルはゲームビューとゲームオーバーレイにしか存在しないため、「実行中のゲームが必要」という条件は型によって保証されます。タイトル画面のセーブボタンは型検査を通りません。

## モードグラフの検査 {#mode-graph-checks}

コンパイル時に、コンパイラは各モードのハンドラが実行しうる `nav` 操作を集め、モードのグラフを作ります。次の問題を報告します。

| コード | 問題 |
| ------ | ---- |
| `W6001` | 到達可能などのモードからも開かれないモード、または一度も開始されないゲーム |
| `E6002` | スタックのベースにあるモードでの `nav::back()` |
| `E6003` | 出口のないモード（`back`、`goto`、`start`、`load`、`quit` がなく、`Action::Back` のバインディングもない） |
| `W6004` | `nav::call` で互いを開き合うオーバーレイ。スタックが際限なく伸びうる |

[ゲームシステムの診断](../../../reference/ja/src/diagnostics/system.md) を参照してください。

## 関数: `call(overlay)` {#call}

```kis
#[builtin] pub fn call(overlay: fn() -> Overlay);
```

- `overlay` `fn() -> Overlay` — オーバーレイのコンポーネント（関数ポインタ）。
- 種別: `fn` · ビルトイン: はい

現在の最上位モードの上にオーバーレイを積みます。オーバーレイのコンポーネントは、表示されるときに一度だけ実行されます。

```kix
button!{on_click = || nav::call(Config), t!["Config"]}
```

## 関数: `dialog(dialog)` {#dialog}

```kis
#[builtin] pub fn dialog(dialog: fn() -> Dialog);
```

- `dialog` `fn() -> Dialog` — ダイアログのコンポーネント。
- 種別: `fn` · ビルトイン: はい

モーダルなダイアログを積みます。ダイアログは下のモードの静止画の上に描画されます。ダイアログには出口が必要で、通常は [`nav::back()`](#back) を呼ぶボタンを置きます（ないと `E6003`）。

```kix
fn ConfirmQuit() -> Dialog {
    Dialog(column![
        t!["Quit the game?"],
        row![
            button!{on_click = || nav::quit(), t!["Yes"]},
            button!{on_click = || nav::back(), t!["No"]},
        ],
    ])
}
```

## 関数: `back()` {#back}

```kis
#[builtin] pub fn back();
```

- 種別: `fn` · ビルトイン: はい

最上位のモードを取り除き、その下のモードに戻ります。ベースのモード（ルートページまたはゲームモード）では実行時に何もしません。ベースのモードで実行されるとコンパイラが判断できる `nav::back()` はエラー `E6002` です。ルートページから離れるには、代わりに [`nav::goto`](#goto) を使います。`Action::Back` は `nav::back()` を呼びます。

## 関数: `goto(page)` {#goto}

```kis
#[builtin] pub fn goto(page: fn() -> RootPage);
```

- `page` `fn() -> RootPage` — ルートページのコンポーネント。
- 種別: `fn` · ビルトイン: はい

スタック全体をルートページ `page` で置き換えます。ゲームの実行中に呼ぶとゲームは終了します。例えば「タイトルへ戻る」ボタンです。

```kix
button!{on_click = || nav::goto(Title), t!["Back to title"]}
```

## 関数: `start(entry, args, on_exit)` {#start}

```kis
#[builtin] pub fn start<E, A, R>(entry: E, args: A, on_exit: Fn(R));
```

- `entry` `E` — `#[entry]` シーンへのポインタ。型は `scene fn(P1, …, Pn) -> R`。
- `args` `A` — シーンの引数のタプル `(P1, …, Pn)`。
- `on_exit` `Fn(R)` — 終了ハンドラ。シーンの戻り値を受け取ります。
- 種別: `fn` · ビルトイン: はい

スタックを、エントリーシーン `entry` を `args` で実行する新しいゲームで置き換えます。物語に入る唯一の方法です。

- このシグネチャの型付けは特別で、コンパイラは `E`、`A`、`R` を上のように関連付けます。[コンパイラ規則](./appendix/rules.md) を参照してください。
- 引数はエントリーシーンの有限型のパラメータ型を持ちます。引数はセーブに記録され、[メタ](./narrative.md) の値が物語に届く唯一の経路でもあります。システム側で `Meta` を `get()` で読み、引数として渡します。
- エントリーシーンが戻ると、その値が `on_exit` に渡されます。`Ending` 列挙型のような戻り値の型は、通常の網羅的な `match` で処理します。
- `on_exit` がどこにも遷移しない場合、スタックは [`System::start`](./system.md#system) のページに戻ります。
- 新しいゲームは最初から始まり、物語変数は初期値を持ちます。シーンの回想は、別の `#[entry]` シーンの `nav::start` です（[システムのレシピ](./guides/system.md#replay) を参照）。

```kix
fn start_button() -> Element {
    button!{on_click = || nav::start(story::Main, (CLEARED_AZUSA.get(),), on_ending), t!["Start"]}
}

fn on_ending(ending: story::Ending) {
    match ending {
        story::Ending::AzusaEnd => { CLEARED_AZUSA.set(true); nav::goto(Credits) }
        story::Ending::NormalEnd => nav::goto(Credits),
    }
}
```

引数も結果もないエントリーシーンには、`()` と、引数を無視するクロージャを渡します。

```kix
button!{on_click = || nav::start(story::Main, (), |_end| nav::goto(Title)), t!["Start"]}
```

## 関数: `load(slot)` {#load}

```kis
#[builtin] pub fn load(slot: SaveSlot) -> Result<(), Divergence>;
```

- `slot` [`SaveSlot`](./system-saves.md#saveslot) — ロードするスロット。
- 戻り値: `Result<(), Divergence>` — ゲームが復元されたら `Ok(())`、セーブがプログラムに合わなくなっていたら `Err(divergence)`。
- 種別: `fn` · ビルトイン: はい

スタックを、`slot` から復元したゲームで置き換えます。セーブはプレイヤーがたどった経路を記録しています。スクリプトの後のバージョンでその経路が変わった場合、セーブは **乖離** し、`load` は `Err` を返してスタックを変更しません。プレイヤーには [`Divergence::describe()`](./system-saves.md#divergence-describe) を表示してください。

```kix
use std::system::saves::SaveSlot;

fn Load() -> Overlay {
    let (error, set_error) = signal("");
    Overlay(column![
        button!{
            on_click = || match nav::load(SaveSlot::Manual(0)) {
                Ok(()) => {}
                Err(divergence) => set_error(divergence.describe()),
            },
            t!["Slot 1"],
        },
        text!(error()),
        button!{on_click = || nav::back(), t!["Back"]},
    ])
}
```

## 関数: `quit()` {#quit}

```kis
#[builtin] pub fn quit();
```

- 種別: `fn` · ビルトイン: はい

確認なしで、即座にプログラムを終了します。先に確認したい終了ボタンは、スタジオが書いた [`Dialog`](#dialog) を開きます。
