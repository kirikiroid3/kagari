---
title: "物語の解析 (E5xxx)"
sidebar:
  label: "物語解析 (E5xxx)"
---

型検査が終わると、Kagari は物語全体を探索します。各開始シーンから出発し、物語変数が取りうるすべての値について、あらゆる経路をたどります。これによって、ステップで止まらないループや再帰、何も選べなくなる選択肢、経路しだいで変わる画面、結果に影響しないコードや変数を見つけます。解析を最後まで終えられなかった場合はその旨を報告し、到達しないステップやシーンは報告しません。

## E5001 `stepless-loop` {#e5001}

**エラー。** シーン内のループを一周する経路の中に、`s!`、`choice!`、あるいはそれらを実行するシーン呼び出しを一度も通らないものがあります。このループは、プレイヤーを待たずに永遠に回り続けるおそれがあります。

```kix
static mut DONE: bool = false;

#[scene]
#[entry]
pushdown fn Main() {
    loop {
        if DONE {
            s!(t!["Done."]);
        } else {
            DONE = true;
        }
    }
}
```

直し方：ループを通るどの経路でもステップを実行するようにするか、ステップを通らない経路では `break` でループを抜けます。

## E5002 `stepless-recursion` {#e5002}

**エラー。** シーンが、ステップを通る前に、直接またはほかのシーンを経由して自分自身を呼び出せます。プレイヤーの入力がないまま、呼び出しスタックが際限なく伸びてしまいます。

```kix
static mut DONE: bool = false;

#[scene]
#[entry]
pushdown fn Echo() {
    if DONE {
        Echo();
    }
    s!(t!["Echo."]);
}
```

直し方：再帰呼び出しの前にステップを実行します。この例なら `s!(t!["Echo."]);` を `Echo();` より上に移します。

## E5003 `stuck-choice` {#e5003}

**エラー。** すべてのアームがガードで隠れた状態の `choice!` に到達しうるため、プレイヤーが何も選べなくなります。メッセージには、そうなる物語変数の値の組み合わせを一つ示します。注記には、その選択肢の[ステップ ID](../reference/concepts/steps.md#step-ids) が出ます。

```kix
static mut KEY: bool = false;
static mut KNOCKS: Int<0, 3> = 0;

enum Door { Open, Knock }

#[scene]
#[entry]
pushdown fn Main() {
    loop {
        match choice!{
            id = "door",
            t!["Open the door"] if KEY => Door::Open,
            t!["Knock"] if KNOCKS < 2 => Door::Knock,
        } {
            Door::Open => break,
            Door::Knock => KNOCKS += 1,
        }
    }
}
```

直し方：少なくとも一つのアームが常に表示されるようにします。たとえば、ガードのないアームを追加します。

## W5004 `inconsistent-screen` {#w5004}

**警告。** ステップで表示する画面が、どの経路を通ってきたかによって変わります。経路ごとに、最後に画面を変更した方法が違うためです。これでは、そのステップにロードやスキップで移ったときに、どの画面を出せばよいか決められません。

```kix
const MIO: EntityId = EntityId(1);

enum Place { Roof, Classroom }

const fn roof(mut screen: Screen) -> Screen {
    screen.entity(MIO).opacity().change(1.0).apply_animation(instant());
    screen
}

const fn classroom(mut screen: Screen) -> Screen {
    screen.entity(MIO).opacity().change(0.5).apply_animation(instant());
    screen
}

#[scene]
#[entry]
pushdown fn Main() {
    match choice!{ id = "where", t!["Roof"] => Place::Roof, t!["Classroom"] => Place::Classroom } {
        Place::Roof => s!(render = roof, t!["The wind is cold."]),
        Place::Classroom => s!(render = classroom, t!["It's quiet."]),
    }
    s!(t!["So, about tomorrow…"]);
}
```

直し方：一貫性が最初に崩れるステップに、キーフレームとなるレンダー（入力の画面を使わないレンダー）を指定します（例：`s!(render = evening, t!["So, about tomorrow…"]);`）。あるいは、物語変数でレンダーを切り替えます（例：`s!(render = if PLACE == Place::Roof { roof } else { classroom }, t!["…"]);`）。

## E5005 `unbounded-screens` {#e5005}

**エラー。** ループが回るたびに、レンダーが前とは違う新しい画面を作ります。あるステップに届く画面の数に上限がありません（許容される上限は 64 です）。

```kix
const MIO: EntityId = EntityId(1);

enum Walk { On, Stop }

const fn mio_steps_right(mut screen: Screen) -> Screen {
    let at = screen.entity(MIO).position().get();
    screen.entity(MIO).position().change(point(at.x + 10, at.y)).apply_animation(instant());
    screen
}

#[scene]
#[entry]
pushdown fn Main() {
    loop {
        match choice!{ id = "walk", t!["Walk on"] => Walk::On, t!["Stop"] => Walk::Stop } {
            Walk::On => s!(render = mio_steps_right, t!["You walk on."]),
            Walk::Stop => break,
        }
    }
}
```

直し方：直前の画面に変更を積み重ねるのではなく、絶対値を設定するレンダーを使います。あるいは、有界な物語変数で変化の幅を制限します。

## W5006 `too-many-states` {#w5006}

**警告。** 物語の状態数が解析の上限を超えたため、解析を打ち切りました。到達しないステップやシーンは報告しません。上限の既定値は 200 000 状態です。開始シーンの引数や乱数の抽選が取りうる値が多すぎて列挙できない場合にも、この警告が出ます。

```kix
#[scene]
#[entry]
pushdown fn Main() {
    let n = rng_choose::<Int<0, 100000>>();
    if n == 7 {
        s!(t!["Lucky."]);
    }
}
```

直し方：物語変数、開始シーンの引数、乱数の抽選の型を狭めます。または、`kagari.toml` の `[build] state-cap` で状態数の上限を引き上げます。

## W5007 `incomplete` {#w5007}

**警告。** 解析がある地点から先の物語を追えなくなったため、到達しないステップやシーンは報告しません。警告は解析が止まった位置を指します。この警告は、解析では保持できない値（ローカル変数をキャプチャしたクロージャなど）を、シーンがステップをまたいで持ち続けようとしていることを示します。

ただし、そうしたプログラムはそれ以前の検査ではじかれます（シーンのローカル変数に保持したクロージャは `E3015`、有限でない型の値は `E3008`）。そのため、それらの検査を通ったプログラムでこの警告が出ることは本来ありません。例は載せていません。

直し方：この警告が出たら、そのプログラムを添えてバグとして報告してください。回避するには、値をステップをまたいで持ち続けず、ステップの前に計算します。

## W5008 `unreachable-scene` {#w5008}

**警告。** 開始シーンからのどの経路でも、このシーンが呼ばれません。

```kix
#[scene]
#[entry]
pushdown fn Main() {
    s!(t!["The end."]);
}

// Never called.
#[scene]
pushdown fn Epilogue() {
    s!(t!["Years later."]);
}
```

直し方：到達できるシーンから呼び出すか、`#[entry]` を付けるか、削除します。

## W5009 `unreachable-step` {#w5009}

**警告。** 物語のどの到達可能な状態からも、このステップにたどり着きません。多くは、条件が決して真にならないことが原因です。注記には、そのステップの[ステップ ID](../reference/concepts/steps.md#step-ids) が出ます。

```kix
static mut KNOCKS: Int<0, 3> = 0;

enum Knock { Again, Stop }

#[scene]
#[entry]
pushdown fn Main() {
    while KNOCKS < 2 {
        match choice!{ id = "knock", t!["Knock"] => Knock::Again, t!["Leave"] => Knock::Stop } {
            Knock::Again => KNOCKS += 1,
            Knock::Stop => break,
        }
    }
    // Reached only when `KNOCKS` is 3, which never happens.
    if KNOCKS == 3 {
        s!(t!["The door gives way."]);
    }
}
```

直し方：条件が成り立ちうるように直すか、到達しないステップを削除します。

## W5010 `unused-story-variable` {#w5010}

**警告。** 物語変数が一度も使われていないか、代入されるだけか、読まれてはいても条件、ステップ、メタデータへの書き込み、エンディングのどれにも影響していません。

```kis
static mut WRITE_ONLY: bool = false;   // only assigned: never read
static mut UNTOUCHED: bool = false;    // never used at all
```

直し方：変数を本来使うべき場所（条件、台詞、エンディング）で読むか、削除します。

## W5011 `missing-keyframe` {#w5011}

**警告。** この検査は、`kagari.toml` で `[build] keyframe-check = true` を指定すると有効になります。選択肢、または開始シーンの最初のステップに、キーフレームとなるレンダーがありません。そこへロードやジャンプをすると、もっと前のキーフレームからレンダーを再生し直すことになります。

```kix
const A: EntityId = EntityId(1);

enum Answer { Yes, No }

const fn key(_screen: Screen) -> Screen {
    Screen::empty()
}

const fn delta(mut screen: Screen) -> Screen {
    screen.entity(A).opacity().change(0.5).apply_animation(instant());
    screen
}

#[entry]
#[scene]
pushdown fn Start() {
    s!(render = key, t!["a"]);
    let _ = choice!{ id = "c", render = delta, t!["Yes"] => Answer::Yes, t!["No"] => Answer::No };
}
```

直し方：入力の画面を使わないレンダーを選択肢に指定します（例：`render = key`）。
