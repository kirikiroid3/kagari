---
title: "ゲームシステム (E6xxx)"
---

`main()` が組み立てる `System` は、ゲームのモード（ルートページ、オーバーレイ、ダイアログ、ゲームモードとそのオーバーレイ）を定義します。Kagari は `nav::*` ハンドラーがこれらのモード間をどう移動するかのグラフを作ります。そして、すべてのモードに到達できること、すべてのモードから抜けられること、モードスタックが有界であることを検査します。

これらの検査にはゲーム全体が必要なため、以下の例はそれぞれ完全なプログラムです。設定を含む同じ最小限の `main()`、エントリーシーン、ゲームビューに続いて、診断を引き起こすモードを置いています。

## W6001 `unreachable-mode` {#w6001}

**警告。** 到達可能などのモードもこのモードを開かないため、プレイヤーはこれを見ることができません。到達可能などのモードも `nav::start` や `nav::load` を呼ばず、ゲーム自体が始まらない場合にも、この警告が出ます。

```kix
use std::narrative::text::text_style;
use std::screen::size;

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("fonts/default.otf");

fn main() -> System {
    System {
        title: "Example",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: Vec::new(),
    }
}

#[scene]
#[entry]
pushdown fn Start() {}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}

fn Title() -> RootPage {
    RootPage(button!{on_click = || nav::start(Start, (), |_end| ()), t!["Start"]})
}

fn Unused() -> Overlay {
    Overlay(button!{on_click = || nav::back(), t!["Back"]})
}
```

修正: 到達可能なハンドラーからそのモードを開くか（例: `button!{on_click = || nav::call(Unused), t!["More"]}`）、削除します。

## E6002 `back-from-base` {#e6002}

**エラー。** ルートページやゲームモードなど、モードスタックの底にあるモードで `nav::back()` が実行されます。下に何もないため、戻る操作は何もしません。

```kix
use std::narrative::text::text_style;
use std::screen::size;

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("fonts/default.otf");

fn main() -> System {
    System {
        title: "Example",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: Vec::new(),
    }
}

#[scene]
#[entry]
pushdown fn Start() {}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}

fn Title() -> RootPage {
    RootPage(column![
        button!{on_click = || nav::start(Start, (), |_end| ()), t!["Start"]},
        button!{on_click = || nav::back(), t!["Back"]},
    ])
}
```

修正: 底のモードからは、`nav::back()` の代わりに `nav::goto(...)` でページを切り替えるか、`nav::quit()` を使います。

## E6003 `trapped-mode` {#e6003}

**エラー。** このモードのどのハンドラーも `nav::back`、`nav::goto`、`nav::start`、`nav::load`、`nav::quit` を呼ばず、`Action::Back` を実行するキー割り当てもありません。いったん開くと、プレイヤーはそのモードから抜けられません。

```kix
use std::narrative::text::text_style;
use std::screen::size;

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("fonts/default.otf");

fn main() -> System {
    System {
        title: "Example",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: Vec::new(),
    }
}

#[scene]
#[entry]
pushdown fn Start() {}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}

fn Title() -> RootPage {
    RootPage(column![
        button!{on_click = || nav::start(Start, (), |_end| ()), t!["Start"]},
        button!{on_click = || nav::dialog(ConfirmReset), t!["Reset"]},
    ])
}

// No way out.
fn ConfirmReset() -> Dialog {
    Dialog(button!{on_click = || MUSIC.set(volume(60)), t!["Reset"]})
}
```

修正: 抜け道を追加します（例: `button!{on_click = || nav::back(), t!["Cancel"]}`）。または `Action::Back` にキーを割り当てます（`default_bindings()` は割り当てています）。

## W6004 `unbounded-stack` {#w6004}

**警告。** オーバーレイが `nav::call` で互いを開く、または自分自身を開きます。呼び出しのたびにモードが 1 つ積まれるため、モードスタックが際限なく伸びる可能性があります。

```kix
use std::narrative::text::text_style;
use std::screen::size;

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("fonts/default.otf");

fn main() -> System {
    System {
        title: "Example",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: Vec::new(),
    }
}

#[scene]
#[entry]
pushdown fn Start() {}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}

fn Title() -> RootPage {
    RootPage(column![
        button!{on_click = || nav::start(Start, (), |_end| ()), t!["Start"]},
        button!{on_click = || nav::call(Gallery), t!["Gallery"]},
    ])
}

fn Gallery() -> Overlay {
    Overlay(column![
        button!{on_click = || nav::call(Gallery), t!["Again"]},
        button!{on_click = || nav::back(), t!["Back"]},
    ])
}
```

修正: 別のオーバーレイを開く前に戻るか、積まずにオーバーレイを切り替えて、循環がスタックに積み重ならないようにします。
