# `std::system::saves` {#stdsystemsaves}

> **安定性:** 安定 — セーブスロットとそのメタデータ。

`std::system::saves` はセーブスロットに名前を付け、スロットを読み取り・削除します。セーブの書き込みは `Narrative::save(slot)`（[`std::narrative`](./narrative.md) を参照）です。セーブできるのは実行中のゲームだけだからです。ロードは [`nav::load(slot)`](./system-nav.md#load) です。スロットのスクリーンショットは要素 [`ui::thumbnail(slot)`](./ui.md#thumbnail) です。

セーブはプレイヤーが物語の中でたどった経路を記録します。スクリプトがその経路を許す限り、セーブはロードできます。モデルについてはリファレンスの「セーブ、リプレイ、決定性」の章を参照してください。

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
| [`Divergence`](#divergence) | ビルトイン構造体 | セーブがプログラムに合わなくなった理由 |
| [`info(slot)`](#info) | `fn` | スロットのメタデータ。リアクティブ |
| [`delete(slot)`](#delete) | `fn` | スロットを空にする |

## 列挙型: `SaveSlot` {#saveslot}

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
| `Quick` | ただ 1 つのクイックスロット。`Action::QuickSave` と `Action::QuickLoad` が書き込み・読み取りします |
| `Auto(k)` | オートセーブスロット。`0 <= k < System::auto_slots` |

- `k >= save_slots` の `Manual(k)` や `k >= auto_slots` の `Auto(k)` は範囲外です。これを使っても実行時には何も起きません。[`info`](#info) は `None` を返し、`save` は何もしません。
- **オートセーブ。** 選択肢ステップに入るたびと終了時に、エンジンは `Auto(k)` を書き込みます。`k` は `0..auto_slots` を順に巡ります。

## 構造体: `Timestamp` {#timestamp}

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

## 構造体: `SaveInfo` {#saveinfo}

```kis
pub struct SaveInfo {
    pub time: Timestamp,
    pub who: Option<Text>,
    pub text: Option<Text>,
}
```

スロットに表示する内容で、セーブした日時と、そのとき表示されていた行です。

- `time` — セーブが書き込まれた日時（ローカル時刻）。
- `who` — セーブ時の行の話者（あれば）。
- `text` — セーブ時の行のテキスト（行が表示されていれば）。

## 構造体: `Divergence` {#divergence}

```kis
#[builtin] pub struct Divergence;
```

セーブがプログラムに合わなくなった理由です。スクリプトが変更され、セーブに記録された経路がもはや物語の経路ではなくなったことを表します。[`nav::load`](./system-nav.md#load) の `Err` で返されます。

### `Divergence::describe()` {#divergence-describe}

```kis
#[builtin] pub fn describe(&self) -> Text;
```

- 戻り値: `Text` — セーブがスクリプトのどこで合わなくなったかの、プレイヤー向けの説明。
- 種別: `fn` · ビルトイン: はい

## 関数: `info(slot)` {#info}

```kis
#[builtin] pub fn info(slot: SaveSlot) -> Option<SaveInfo>;
```

- `slot` `SaveSlot` — スロット。
- 戻り値: `Option<SaveInfo>` — スロットのメタデータ。スロットが空か範囲外なら `None`。
- 種別: `fn` · ビルトイン: はい

`info` はリアクティブなソースです。セーブが書き込まれたり削除されたりすると、スロット一覧が再描画されます。

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

## 関数: `delete(slot)` {#delete}

```kis
#[builtin] pub fn delete(slot: SaveSlot);
```

- `slot` `SaveSlot` — 空にするスロット。
- 種別: `fn` · ビルトイン: はい

スロットを空にします。

```kix
button!{on_click = || saves::delete(SaveSlot::Manual(0)), t!["Delete"]}
```

完全なセーブ・ロード画面は [システムのレシピ](./guides/system.md#save-and-load) にあります。
