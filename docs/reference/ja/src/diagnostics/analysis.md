# 物語の解析 (E5xxx) {#story-analysis-e5xxx}

型検査の後、Kagari は物語全体を探索します。各エントリーシーンからのすべての経路を、物語変数が取りうるすべての値について調べます。これにより、ステップで止まらないループや再帰、何も提示できない選択肢、経路によって変わる画面、意味を持たないコードや変数が見つかります。解析が最後まで終わらない場合はその旨を報告し、到達不能なステップやシーンは報告しません。

## E5001 `stepless-loop` {#e5001}

**エラー。** シーン内のループを一周する経路のどれかが、`s!`、`choice!`、またはそれらを実行するシーン呼び出しを 1 つも実行しません。そのループはプレイヤーを待たずに永遠に回り続ける可能性があります。

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

修正: ループを通るすべての経路でステップを実行するか、ステップしない経路では `break` でループを抜けます。

## E5002 `stepless-recursion` {#e5002}

**エラー。** シーンが、ステップを通過する前に、直接または他のシーンを通して自分自身を呼べます。プレイヤーの入力なしに呼び出しスタックが無限に伸びます。

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

修正: 再帰呼び出しの前にステップを実行します。たとえば `s!(t!["Echo."]);` を `Echo();` の上に移します。

## E5003 `stuck-choice` {#e5003}

**エラー。** すべてのアームがガードで隠された `choice!` に解析が到達できます。プレイヤーには選べるものがありません。メッセージには、これが起こる物語変数の値の組を 1 つ示します。

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

修正: 常に少なくとも 1 つのアームが表示されるようにします。たとえばガードのないアームを追加します。

## W5004 `inconsistent-screen` {#w5004}

**警告。** あるステップが表示する画面が、そこに到達した経路によって変わります。経路ごとに最後に画面を変えた方法が異なるためです。そのステップへのロードやスキップでは、どの画面を表示すべきか決められません。

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

修正: 最初に一貫性が崩れるステップにキーフレームのレンダー（入力の画面を無視するレンダー）を与えます。例: `s!(render = evening, t!["So, about tomorrow…"]);`。または物語変数でレンダーを選びます: `s!(render = if PLACE == Place::Roof { roof } else { classroom }, t!["…"]);`

## E5005 `unbounded-screens` {#e5005}

**エラー。** ループが回るたびに、レンダーが新しい別の画面を生成します。あるステップに到達しうる画面の数に上限がありません（上限は 64 です）。

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

修正: 直前の画面に積み上げるのではなく絶対値を設定するレンダーを使うか、有界な物語変数で変化を制限します。

## W5006 `too-many-states` {#w5006}

**警告。** 物語の状態数が解析の上限を超えたため、解析を打ち切り、到達不能なステップやシーンを報告しません。既定の上限は 200 000 状態です。エントリーシーンの引数や乱数の抽選の値が多すぎて列挙できない場合にも、この警告が出ます。

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

修正: 物語変数、エントリーシーンのパラメーター、乱数の抽選の型を狭めるか、`kagari.toml` の `[build] state-cap` で状態の上限を引き上げます。

## W5007 `incomplete` {#w5007}

**警告。** 解析がある地点から先の物語を追えないため、到達不能なステップやシーンを報告しません。警告は解析が止まった位置を指します。これは、シーンが解析で保持できない値（ローカル変数をキャプチャするクロージャなど）をステップをまたいで生かしておこうとしていることを意味します。

そのようなプログラムはそれより前の検査で拒否されます（たとえばシーンのローカル変数に保持したクロージャは `E3015`、有限でない型の値は `E3008`）。そのため、それらの検査を通ったプログラムがこの警告に至ることは通常ありません。例は示しません。

修正: この警告が出た場合は、それを引き起こすプログラムを添えてバグを報告してください。回避策として、値をステップをまたいで保持せず、ステップの前に計算します。

## W5008 `unreachable-scene` {#w5008}

**警告。** エントリーシーンからのどの経路も、このシーンを呼びません。

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

修正: 到達可能なシーンから呼ぶか、`#[entry]` を付けるか、削除します。

## W5009 `unreachable-step` {#w5009}

**警告。** 物語の到達可能な状態のどれも、このステップに至りません。多くの場合、条件が決して真にならないことが原因です。

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

修正: 条件が成り立ちうるように直すか、使われないステップを削除します。

## W5010 `unused-story-variable` {#w5010}

**警告。** 物語変数がまったく使われていないか、代入されるだけか、条件、ステップ、メタへの書き込み、エンディングに一切影響しない形でしか読まれていません。

```kis
static mut WRITE_ONLY: bool = false;   // only assigned: never read
static mut UNTOUCHED: bool = false;    // never used at all
```

修正: 意味を持つべき場所（条件、台詞、エンディング）で変数を読むか、削除します。

## W5011 `missing-keyframe` {#w5011}

**警告。** この検査は `kagari.toml` の `[build] keyframe-check = true` で有効にします。選択肢、またはエントリーシーンの最初のステップにキーフレームのレンダーがありません。そこへのロードやジャンプでは、より前のキーフレームからレンダーを再生し直す必要があります。

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

修正: 入力の画面を無視するレンダーを選択肢に与えます。例: `render = key`
