---
title: "std::system::saves"
---

> **安定性:** 安定 — セーブスロットとそのメタデータ。

`std::system::saves` は、セーブスロットを表す型と、スロットを読み取り・削除する関数を提供します。セーブできるのは実行中のゲームだけなので、セーブの書き込みは `Narrative::save(slot)`（[`std::narrative`](./narrative.md) を参照）で行います。ロードには [`nav::load(slot)`](./system-nav.md#load) を使います。スロットのスクリーンショットは [`ui::thumbnail(slot)`](./ui.md#thumbnail) 要素で表示します。

セーブには、プレイヤーが物語の中でたどった経路が記録されます。スクリプトがその経路を許している限り、セーブはロードできます。仕組みについては、リファレンスの「セーブ、再生、決定性」の章を参照してください。

```kis
use std::system::saves;
use std::system::saves::SaveSlot;
```

## 概要 {#overview}

| アイテム | 種別 | 概要 |
| -------- | ---- | ---- |
| [`SaveSlot`](#saveslot) | 列挙型 | 手動・クイック・オートセーブのスロット |
| [`Timestamp`](#timestamp) | 構造体 | ローカルの日時 |
| [`SaveInfo`](#saveinfo) | 構造体 | スロットに表示する内容 |
| [`Divergence`](#divergence) | 組み込みの構造体 | セーブが現在のプログラムと合わなくなった理由 |
| [`info(slot)`](#info) | `fn` | スロットのメタデータ（リアクティブ） |
| [`delete(slot)`](#delete) | `fn` | スロットを空にする |

## 列挙型：`SaveSlot` {#saveslot}

```kis
pub enum SaveSlot {
    /// Manual slot `k`, below `System::save_slots`.
    Manual(u32),
    /// The quick slot.
    Quick,
    /// Autosave slot `k`, below `System::auto_slots`.
    Auto(u32),
}
```

| バリアント | 意味 |
| ---------- | ---- |
| `Manual(k)` | 手動スロット。`0 <= k < System::save_slots` |
| `Quick` | クイックスロット（1 つだけ）。`Action::QuickSave` で書き込み、`Action::QuickLoad` で読み込む |
| `Auto(k)` | オートセーブのスロット。`0 <= k < System::auto_slots` |

- `k >= save_slots` の `Manual(k)` と、`k >= auto_slots` の `Auto(k)` は範囲外です。範囲外のスロットを使っても、実行時には何も起きません。[`info`](#info) は `None` を返し、`save` は何もしません。
- **オートセーブ。** エンジンは選択肢ステップに入るたびと、ゲーム終了時に `Auto(k)` へ書き込みます。`k` は `0..auto_slots` の範囲を順番に巡回します。

## 構造体：`Timestamp` {#timestamp}

```kis
pub struct Timestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}
```

ローカルの日時です。

## 構造体：`SaveInfo` {#saveinfo}

```kis
pub struct SaveInfo {
    pub time: Timestamp,
    pub who: Option<Text>,
    pub text: Option<Text>,
}
```

スロットに表示する内容です。セーブした日時と、そのとき画面に出ていた行を含みます。

- `time` — セーブを書き込んだ日時（ローカル時刻）。
- `who` — セーブ時に表示されていた行の話者。いなければ `None`。
- `text` — セーブ時に表示されていた行のテキスト。行が表示されていなければ `None`。

## 構造体：`Divergence` {#divergence}

```kis
#[builtin] pub struct Divergence;
```

セーブが現在のプログラムと合わなくなった理由を表します。スクリプトが変更され、セーブに記録された経路が物語の中にもう存在しない場合に使われます。[`nav::load`](./system-nav.md#load) が `Err` で返します。

### `Divergence::describe()` {#divergence-describe}

```kis
#[builtin] pub fn describe(&self) -> Text;
```

- 戻り値：`Text` — セーブがスクリプトのどこで合わなくなったかを示す、プレイヤー向けの説明。
- 種別：`fn` · 組み込み：はい

## 関数：`info(slot)` {#info}

```kis
#[builtin] pub fn info(slot: SaveSlot) -> Option<SaveInfo>;
```

- `slot` `SaveSlot` — スロット。
- 戻り値：`Option<SaveInfo>` — スロットのメタデータ。スロットが空か範囲外なら `None`。
- 種別：`fn` · 組み込み：はい

`info` はリアクティブな値の出どころです。セーブが書き込まれたり削除されたりすると、スロット一覧が描画し直されます。

```kix
use std::system::saves;
use std::system::saves::SaveSlot;

fn slot_text(slot: SaveSlot) -> Text {
    match saves::info(slot) {
        Some(info) => match info.text {
            Some(text) => text,
            None => "",
        },
        None => "Empty",
    }
}
```

## 関数：`delete(slot)` {#delete}

```kis
#[builtin] pub fn delete(slot: SaveSlot);
```

- `slot` `SaveSlot` — 空にするスロット。
- 種別：`fn` · 組み込み：はい

スロットを空にします。

```kix
button!{on_click = || saves::delete(SaveSlot::Manual(0)), t!["Delete"]}
```

セーブ・ロード画面の完全な例は [ゲームシステムの実例](./guides/system.md#save-and-load) にあります。
