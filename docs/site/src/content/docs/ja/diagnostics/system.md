---
title: "ゲームシステム (E6xxx)"
---

`main()` で組み立てる `System` は、ゲームのモード（ルートページ、オーバーレイ、ダイアログ、ゲームモードとそのオーバーレイ）を定義します。Kagari は、`nav::*` ハンドラによってモード間をどう行き来するかをグラフにまとめます。そのうえで、どのモードにも到達できるか、どのモードからも抜けられるか、モードスタックが有界に収まるかを検査します。

この検査にはゲーム全体が必要なので、以下の例はどれも完結したプログラムになっています。設定を含む共通の最小限の `main()`、開始シーン、ゲーム画面の後に、診断が出るモードを置いています。

## W6001 `unreachable-mode` {#w6001}

**警告。** 到達できるどのモードからもこのモードが開かれないため、プレイヤーの目に触れることがありません。到達できるどのモードも `nav::start` や `nav::load` を呼ばず、ゲーム自体が始まらない場合にも、この警告が出ます。

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

直し方：到達できるハンドラからそのモードを開くか（例：`button!{on_click = || nav::call(Unused), t!["More"]}`）、モードを削除します。

## E6002 `back-from-base` {#e6002}

**エラー。** ルートページやゲームモードのように、モードスタックの一番下にあるモードで `nav::back()` を実行しています。下には何もないので、戻る操作をしても何も起きません。

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

直し方：一番下のモードでは、`nav::back()` ではなく `nav::goto(...)` でページを切り替えるか、`nav::quit()` を使います。

## E6003 `trapped-mode` {#e6003}

**エラー。** このモードのハンドラがどれも `nav::back`、`nav::goto`、`nav::start`、`nav::load`、`nav::quit` を呼ばず、`Action::Back` を実行するキー割り当てもありません。一度開くと、プレイヤーはこのモードから出られなくなります。

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

直し方：抜け道を用意します（例：`button!{on_click = || nav::back(), t!["Cancel"]}`）。あるいは、`Action::Back` にキーを割り当てます（`default_bindings()` を使えば割り当て済みです）。

## W6004 `unbounded-stack` {#w6004}

**警告。** オーバーレイどうしが `nav::call` で互いを開いているか、オーバーレイが自分自身を開いています。呼び出すたびにモードが一つずつ積まれるので、モードスタックが際限なく伸びるおそれがあります。

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

直し方：別のオーバーレイを開く前に元のオーバーレイを閉じるか、スタックに積まずにオーバーレイを切り替えて、循環がスタックに積み重ならないようにします。
