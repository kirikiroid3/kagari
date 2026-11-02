---
title: "std::system::nav"
---

> **安定性:** 安定 — システムのモードスタックを操作する関数。

`std::system::nav` には画面遷移の操作がまとまっています。UI からモードスタックを操作する手段は、これらの関数と `Narrative` ハンドルのメソッドだけです。どれも組み込みの `fn` なので、`fn` のコード（ボタンのハンドラ、エフェクト、終了ハンドラ）ならどこからでも呼べます。ただし、レンダーや物語からは呼べません。

モジュール自体がプレリュードに含まれているので、`use` を書かずに `nav::call(Config)` や `nav::back()` のように呼べます。

モードの種類と、それぞれをスタックのどこに置けるかについては [モード型](./system.md#mode-types) を参照してください。

## 概要 {#overview}

| 関数 | 効果 |
| ---- | ---- |
| [`nav::call(o)`](#call) | オーバーレイ `o` を積む |
| [`nav::dialog(d)`](#dialog) | ダイアログ `d` を積む |
| [`nav::back()`](#back) | 一番上のモードを取り除く。一番下のモードでは何もしない |
| [`nav::goto(p)`](#goto) | スタック全体をルートページ `p` に置き換える |
| [`nav::start(e, args, on_exit)`](#start) | スタックを、開始シーン `e` を実行する新しいゲームに置き換える |
| [`nav::load(slot)`](#load) | スタックを、`slot` から復元したゲームに置き換える |
| [`nav::quit()`](#quit) | プログラムをただちに終了する |

実行中のゲームに対する操作は、`Narrative` ハンドルのメソッドとして用意されています（[`std::narrative`](./narrative.md) を参照）。

| 操作 | 効果 |
| ---- | ---- |
| `game.advance()` | 物語に `NextStep` を送る |
| `game.select(arm)` | 現在の選択肢のうち、表示中のアームを選ぶ |
| `game.save(slot)` | 実行中のゲームを `slot` にセーブする |
| `game.rewind(i)` | 物語をバックログの項目 `i` の時点まで戻す |
| `game.call(o)` | ゲームオーバーレイ `o` を積む |

このハンドルはゲーム画面とゲームオーバーレイにしかありません。そのため、「実行中のゲームが必要」という条件は型で保証されます。たとえば、タイトル画面にセーブボタンを置くと型検査でエラーになります。

## モード遷移の検査 {#mode-graph-checks}

コンパイラはコンパイル時に、各モードのハンドラが実行しうる `nav` 操作を集めて、モードの遷移図を組み立てます。そのうえで、次の問題を報告します。

| コード | 問題 |
| ------ | ---- |
| `W6001` | 到達できるどのモードからも開かれないモード、または一度も開始されないゲーム |
| `E6002` | スタックの一番下にあるモードでの `nav::back()` |
| `E6003` | 抜け出す手段のないモード（`back`、`goto`、`start`、`load`、`quit` のどれもなく、`Action::Back` の割り当てもない） |
| `W6004` | `nav::call` で互いを開き合うオーバーレイ。スタックが際限なく積み上がるおそれがある |

[ゲームシステムの診断](../diagnostics/system.md) を参照してください。

## 関数：`call(overlay)` {#call}

```kis
#[builtin] pub fn call(overlay: fn() -> Overlay);
```

- `overlay` `fn() -> Overlay` — オーバーレイのコンポーネント（関数ポインタで渡す）。
- 種別：`fn` · 組み込み：はい

現在一番上にあるモードの上に、オーバーレイを積みます。オーバーレイのコンポーネントは、表示されるときに一度だけ実行されます。

```kix
button!{on_click = || nav::call(Config), t!["Config"]}
```

## 関数：`dialog(dialog)` {#dialog}

```kis
#[builtin] pub fn dialog(dialog: fn() -> Dialog);
```

- `dialog` `fn() -> Dialog` — ダイアログのコンポーネント。
- 種別：`fn` · 組み込み：はい

モーダルなダイアログを積みます。ダイアログは、下のモードを静止画にしてその上に描画されます。ダイアログには抜け出す手段が必要で、通常は [`nav::back()`](#back) を呼ぶボタンを置きます。手段がないとエラー `E6003` になります。

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

## 関数：`back()` {#back}

```kis
#[builtin] pub fn back();
```

- 種別：`fn` · 組み込み：はい

一番上のモードを取り除き、その下のモードに戻ります。一番下のモード（ルートページまたはゲームモード）で呼んだ場合、実行時には何も起きません。ただし、一番下のモードで実行されることがコンパイラにわかる `nav::back()` はエラー `E6002` になります。ルートページから別の画面に移るには、[`nav::goto`](#goto) を使ってください。`Action::Back` は `nav::back()` を呼びます。

## 関数：`goto(page)` {#goto}

```kis
#[builtin] pub fn goto(page: fn() -> RootPage);
```

- `page` `fn() -> RootPage` — ルートページのコンポーネント。
- 種別：`fn` · 組み込み：はい

スタック全体をルートページ `page` に置き換えます。ゲームの実行中に呼ぶと、そのゲームは終了します。「タイトルへ戻る」ボタンなどに使います。

```kix
button!{on_click = || nav::goto(Title), t!["Back to title"]}
```

## 関数：`start(entry, args, on_exit)` {#start}

```kis
#[builtin] pub fn start<E, A, R>(entry: E, args: A, on_exit: Fn(R));
```

- `entry` `E` — `#[entry]` シーンへのポインタ。型は `scene fn(P1, …, Pn) -> R`。
- `args` `A` — シーンに渡す引数のタプル `(P1, …, Pn)`。
- `on_exit` `Fn(R)` — 終了ハンドラ。シーンの戻り値を受け取る。
- 種別：`fn` · 組み込み：はい

スタックを新しいゲームに置き換え、開始シーン `entry` を `args` で実行します。物語に入る方法はこれしかありません。

- このシグネチャの型付けは特殊で、コンパイラは `E`、`A`、`R` を上のように対応付けます。[標準ライブラリに関するコンパイラの規則](./appendix/rules.md) を参照してください。
- 開始引数の型は、開始シーンのパラメータの型（有限型）です。開始引数はセーブに記録されます。また、[メタデータ](./narrative.md) の値を物語に届ける唯一の経路でもあります。システム側で `Meta` を `get()` で読み、引数として渡してください。
- 開始シーンが戻ると、その戻り値が `on_exit` に渡されます。`Ending` 列挙型のような戻り値は、通常の網羅的な `match` で処理します。
- `on_exit` がどこにも遷移しない場合、スタックは [`System::start`](./system.md#system) のページに戻ります。
- 新しいゲームはまっさらな状態で始まり、物語変数はすべて初期値になります。シーンの回想は、回想用の別の `#[entry]` シーンを `nav::start` で開始して実現します（[ゲームシステムの実例](./guides/system.md#replay) を参照）。

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

引数も戻り値もない開始シーンには、`()` と、引数を無視するクロージャを渡します。

```kix
button!{on_click = || nav::start(story::Main, (), |_end| nav::goto(Title)), t!["Start"]}
```

## 関数：`load(slot)` {#load}

```kis
#[builtin] pub fn load(slot: SaveSlot) -> Result<(), Divergence>;
```

- `slot` [`SaveSlot`](./system-saves.md#saveslot) — ロードするスロット。
- 戻り値：`Result<(), Divergence>` — ゲームを復元できたら `Ok(())`、セーブが現在のプログラムと合わなくなっていたら `Err(divergence)`。
- 種別：`fn` · 組み込み：はい

スタックを、`slot` から復元したゲームに置き換えます。セーブにはプレイヤーがたどった経路が記録されています。その後のスクリプトの変更でこの経路が成り立たなくなっていると、セーブと今のプログラムの間に**食い違い**が生じます。この場合 `load` は `Err` を返し、スタックは変わりません。プレイヤーには [`Divergence::describe()`](./system-saves.md#divergence-describe) の説明を表示してください。

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

## 関数：`quit()` {#quit}

```kis
#[builtin] pub fn quit();
```

- 種別：`fn` · 組み込み：はい

確認なしで、ただちにプログラムを終了します。終了前に確認したい場合は、終了ボタンから制作者が用意した [`Dialog`](#dialog) を開くようにします。
