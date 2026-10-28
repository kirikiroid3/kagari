# システムのレシピ {#system-recipes}

ゲームシステムの実例です。プログラムとタイトル画面、ゲームビュー、設定画面、セーブ・ロード画面、CG ギャラリー、音楽鑑賞、回想の解放、バックログを扱います。[`std::system`](../system.md)、[`std::system::nav`](../system-nav.md)、[`std::system::saves`](../system-saves.md)、[`std::ui`](../ui.md)、[`std::ui::style`](../ui-style.md) を使います。UI フォームには `.kix` ファイルが必要です。

レシピは次のような構成のプロジェクトを想定しています。

```text
src/
  main.kix      // main, settings, bindings
  story.kix     // the scenes
  ui.kix        // the modes
```

`kagari.toml` には `entry = "src/main.kix"` を設定します。以下の各ファイルは、そのスニペットをページの順に連結したもので、ページ全体でコンパイルが通ります。アセットのパスは、プロジェクトの `assets/` ディレクトリ以下のファイルを指します。

## プログラムとタイトル画面 {#the-program}

`main` は [`System`](../system.md#system) を返します。設定ハンドルは不変の `static` で、エンジンは `Settings` に挙げられたものを読みます。

```kix
// main.kix
mod story;
mod ui;

use std::narrative::text::text_style;
use std::screen::size;
use std::system::{Action, Binding, Input, Key, bind, default_bindings};

pub static TEXT_SPEED: Setting<Duration> = setting(ms(30));
pub static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
pub static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
pub static SKIP_UNREAD: Setting<bool> = setting(false);
pub static MASTER: Setting<Volume> = setting(volume(100));
pub static MUSIC: Setting<Volume> = setting(volume(60));
pub static SOUND: Setting<Volume> = setting(volume(100));
pub static VOICE: Setting<Volume> = setting(volume(100));
pub static VOICE_SUSTAIN: Setting<bool> = setting(false);
pub static WAIT_FOR_VOICE: Setting<bool> = setting(true);
pub static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
pub static FULLSCREEN: Setting<bool> = setting(false);
pub static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("fonts/default.otf");

fn main() -> System {
    System {
        title: "Example",
        resolution: size(1280, 720),
        start: ui::Title,
        game: ui::GameView,
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
        bindings: bindings(),
    }
}

const fn bindings() -> Vec<Binding> {
    let mut list = default_bindings();
    list.push(bind(Input::Key(Key::A), Action::ToggleAuto));
    list.push(bind(Input::Key(Key::L), Action::OpenInGame(ui::Backlog)));
    list
}
```

タイトル画面は [`RootPage`](../system.md#rootpage) です。[`nav::start`](../system-nav.md#start) で物語を始め、`nav::call` でオーバーレイを開き、`nav::goto` でページを切り替え、[`set_music`](../system.md#set_music) でシステム音楽を再生します。

```kix
// ui.kix
use std::screen::{CENTER, rgba};
use std::system::set_music;
use std::ui::{Align, Fit};
use std::ui::style::{ButtonStyle, Look, Paint, button_style};
use std::narrative::text::text_style;
use crate::story;

const TITLE_BG: ImageAsset = asset("image/title.png");
const TITLE_THEME: AudioAsset = asset("music/title.ogg");

const MENU_BUTTON: ButtonStyle = button_style(Look { background: Paint::None, text: text_style(28, WHITE) })
    .hover(Look { background: Paint::Color(rgb(60, 60, 90)), text: text_style(28, WHITE) })
    .focus(Look { background: Paint::Color(rgb(60, 60, 90)), text: text_style(28, WHITE) })
    .change(tween(ms(150)));

pub fn Title() -> RootPage {
    set_music(Some(looped(TITLE_THEME)), crossfade(ms(500)));
    RootPage(layer![
        image!(fit = Fit::Cover, TITLE_BG).cover(),
        column![
            button!{style = MENU_BUTTON, on_click = || start_game(), t!["Start"]},
            button!{style = MENU_BUTTON, on_click = || nav::call(Load), t!["Load"]},
            button!{style = MENU_BUTTON, on_click = || nav::goto(Gallery), t!["Gallery"]},
            button!{style = MENU_BUTTON, on_click = || nav::goto(MusicRoom), t!["Music"]},
            button!{style = MENU_BUTTON, on_click = || nav::goto(Replays), t!["Replay"]},
            button!{style = MENU_BUTTON, on_click = || nav::call(Config), t!["Config"]},
            button!{style = MENU_BUTTON, on_click = || nav::dialog(ConfirmQuit), t!["Quit"]},
        ].gap(12).at(point(640, 470), CENTER),
    ])
}

fn start_game() {
    nav::start(story::Main, (CLEARED_AZUSA.get(),), on_ending)
}

fn ConfirmQuit() -> Dialog {
    Dialog(column![
        t!["Quit the game?"],
        row![
            button!{on_click = || nav::quit(), t!["Yes"]},
            button!{on_click = || nav::back(), t!["No"]},
        ].gap(24),
    ].align(Align::Center, Align::Center).background(Paint::Color(rgba(0, 0, 0, 200))))
}
```

`Title` コンポーネントはページが表示されるたびに一度実行されるので、プレイヤーがタイトルに戻るたびに `set_music` がタイトル曲を再生し直します。`nav::quit` は確認をしないので、`ConfirmQuit` ダイアログが確認します。

物語がエンディングを返す場合は、終了ハンドラで処理し、[`Meta`](../narrative.md) の値に記録します。

```kix
pub static CLEARED_AZUSA: Meta<bool> = meta(false);

fn on_ending(ending: story::Ending) {
    match ending {
        story::Ending::AzusaEnd => { CLEARED_AZUSA.set(true); nav::goto(Credits) }
        story::Ending::NormalEnd => nav::goto(Credits),
    }
}

fn Credits() -> RootPage {
    RootPage(column![
        t!["Thank you for playing."],
        button!{on_click = || nav::goto(Title), t!["Back to title"]},
    ])
}
```

`story.kix` はエントリーシーンとその結果から始まります。残りは [回想シーン](#replay) で追加します。

```kix
// story.kix
pub enum Ending {
    AzusaEnd,
    NormalEnd,
}

#[scene]
#[entry]
pub pushdown fn Main(cleared: bool) -> Ending {
    s!(t!["Hello."]);
    FestivalNight();
    if cleared { Ending::AzusaEnd } else { Ending::NormalEnd }
}

const fn festival(screen: Screen) -> Screen {
    screen
}
```

## ゲームビュー {#the-game-view}

[`System::game`](../system.md#system) は、ゲーム画面、メッセージウィンドウ、選択肢のボタン、クイックメニューを描きます。`Narrative` ハンドルから読み取るものはすべてリアクティブです。

```kix
use std::narrative::{MenuArm, PlayMode};
use std::narrative::text::TextStyle;
use std::screen::{BOTTOM_CENTER, insets};
use std::system::saves::SaveSlot;
use std::ui::{each, label, message, show};

const WINDOW_STYLE: TextStyle = text_style(26, WHITE);
const NAME_STYLE: TextStyle = text_style(28, hex(0xFFE0A0));

fn who_text(game: Narrative) -> Text {
    match game.line() {
        Some(line) => match line.who {
            Some(who) => who,
            None => "",
        },
        None => "",
    }
}

fn menu_arms(game: Narrative) -> Vec<MenuArm> {
    match game.menu() {
        Some(menu) => menu.arms,
        None => Vec::new(),
    }
}

fn arm_button(game: Narrative, arm: MenuArm) -> Element {
    button!{
        style = MENU_BUTTON,
        disabled = match arm.arm {
            Some(_) => false,
            None => true,
        },
        on_click = || match arm.arm {
            Some(a) => game.select(a),
            None => {}
        },
        label(arm.text),
    }.width(500)
}

fn toggle_mode(game: Narrative, mode: PlayMode) {
    if game.mode() == mode { game.set_mode(PlayMode::Normal) } else { game.set_mode(mode) }
}

pub fn GameView(game: Narrative) -> Element {
    let (hidden, set_hidden) = signal(false);
    layer![
        game_screen!(game = game).cover(),
        show(
            || !hidden(),
            || column![
                text!(style = NAME_STYLE, who_text(game)),
                message(game, WINDOW_STYLE).grow(1),
                row![
                    button!{on_click = || game.save(SaveSlot::Quick), t!["Q.Save"]},
                    button!{on_click = || { let _ = nav::load(SaveSlot::Quick); }, t!["Q.Load"]},
                    button!{on_click = || game.call(Save), t!["Save"]},
                    button!{on_click = || nav::call(Load), t!["Load"]},
                    button!{on_click = || toggle_mode(game, PlayMode::Auto), t!["Auto"]},
                    button!{on_click = || toggle_mode(game, PlayMode::Skip), t!["Skip"]},
                    button!{on_click = || game.call(Backlog), t!["Log"]},
                    button!{on_click = || nav::call(Config), t!["Config"]},
                    button!{on_click = || set_hidden(true), t!["Hide"]},
                ].gap(8),
            ]
                .width(1200)
                .height(200)
                .padding(insets(16, 40, 16, 40))
                .background(Paint::Color(rgba(0, 0, 0, 160))),
        ).at(point(640, 710), BOTTOM_CENTER),
        show(
            || has_menu(game),
            || each(|| menu_arms(game), |arm| arm_button(game, arm)).gap(24),
        ).at(point(640, 300), CENTER),
        show(|| hidden(), || button!{on_click = || set_hidden(false)}.width(1280).height(720)).cover(),
    ]
}

fn has_menu(game: Narrative) -> bool {
    match game.menu() {
        Some(_) => true,
        None => false,
    }
}
```

`each` はアームごとに 1 つのボタンを保ちます。無効なアーム（`arm.arm` が `None`）はグレーアウトして表示されます。

## 設定画面 {#settings-page}

コンフィグ画面は [`Overlay`](../system.md#overlay) なので、タイトルからもゲームからも開けます。設定は `get` で読み、`set` で書きます。`get` はリアクティブなので、どのコントロールもシグナルなしで現在の値を表示します。

```kix
use std::ui::option;
use crate::{AUTO_DELAY, FULLSCREEN, MUSIC, SKIP_UNREAD, SOUND, TEXT_SPEED, VOICE, WINDOW_OPACITY};

fn volume_row(name: Text, handle: Setting<Volume>) -> Element {
    row![
        label(name).width(200),
        slider!{
            value = handle.get().level,
            on_change = |v| handle.set(Volume { level: v, muted: handle.get().muted }),
        }.width(400),
        toggle!{
            value = handle.get().muted,
            on_change = |m| handle.set(Volume { level: handle.get().level, muted: m }),
            t!["Mute"],
        },
    ].gap(16).align(Align::Start, Align::Center)
}

fn speed_button(name: Text, handle: Setting<Duration>, value: Duration) -> Element {
    toggle!{value = handle.get().ms == value.ms, on_change = |_on| handle.set(value), label(name)}
}

fn Config() -> Overlay {
    Overlay(column![
        volume_row("Music", MUSIC),
        volume_row("Sound", SOUND),
        volume_row("Voice", VOICE),
        row![
            label("Text speed").width(200),
            speed_button("Slow", TEXT_SPEED, ms(60)),
            speed_button("Normal", TEXT_SPEED, ms(30)),
            speed_button("Fast", TEXT_SPEED, ms(10)),
            speed_button("Instant", TEXT_SPEED, ms(0)),
        ].gap(8),
        row![
            label("Auto delay").width(200),
            speed_button("Short", AUTO_DELAY, ms(800)),
            speed_button("Normal", AUTO_DELAY, ms(1300)),
            speed_button("Long", AUTO_DELAY, ms(3000)),
        ].gap(8),
        row![
            label("Window").width(200),
            select!{
                value = WINDOW_OPACITY.get(),
                on_change = |v| WINDOW_OPACITY.set(v),
                option(50, label("50%")),
                option(80, label("80%")),
                option(100, label("100%")),
            },
        ].gap(8),
        toggle!{value = SKIP_UNREAD.get(), on_change = |v| SKIP_UNREAD.set(v), t!["Skip unread text"]},
        toggle!{value = FULLSCREEN.get(), on_change = |v| FULLSCREEN.set(v), t!["Fullscreen"]},
        button!{on_click = || nav::back(), t!["Back"]},
    ].gap(16).padding(insets(40, 80, 40, 80)).background(Paint::Color(rgba(0, 0, 0, 220))))
}
```

スライダーの範囲は値の `Int` 型の範囲なので、音量スライダーは `min` や `max` なしで 0 から 100 まで動きます。

> **安定性:** 予定 — ゲームビューの外でのテキスト表示のプレビュー（現在の文字速度を示すサンプル行）は、まだ使えません。

## セーブ・ロード画面 {#save-and-load}

セーブには実行中のゲームが必要なので、セーブ画面は `Narrative` ハンドルを受け取る [`GameOverlay`](../system.md#gameoverlay) です。ロード画面は通常のオーバーレイで、タイトルからも使えます。[`saves::info`](../system-saves.md#info) はリアクティブなので、スロットは書き込みや削除のすぐ後に更新されます。

```kix
use std::system::saves;
use std::system::saves::SaveSlot;
use std::ui::{scroll, thumbnail};

fn slot_text(slot: SaveSlot) -> Text {
    match saves::info(slot) {
        Some(info) => match info.text {
            Some(text) => text,
            None => "",
        },
        None => "Empty",
    }
}

fn slots() -> Vec<u32> {
    let mut list = Vec::new();
    for k in 0..20 {
        list.push(k);
    }
    list
}

fn slot_view(slot: SaveSlot) -> Element {
    row![
        thumbnail(slot).width(160).height(90),
        text!(slot_text(slot)),
    ].gap(16)
}

fn Save(game: Narrative) -> GameOverlay {
    GameOverlay(column![
        scroll(|| each(|| slots(), |k| row![
            button!{on_click = || game.save(SaveSlot::Manual(k)), slot_view(SaveSlot::Manual(k))},
            button!{on_click = || saves::delete(SaveSlot::Manual(k)), t!["Delete"]},
        ])).grow(1),
        button!{on_click = || nav::back(), t!["Back"]},
    ].background(Paint::Color(rgba(0, 0, 0, 200))))
}

fn Load() -> Overlay {
    let (error, set_error) = signal("");
    Overlay(column![
        scroll(|| each(|| slots(), |k| button!{
            on_click = || match nav::load(SaveSlot::Manual(k)) {
                Ok(()) => {}
                Err(divergence) => set_error(divergence.describe()),
            },
            slot_view(SaveSlot::Manual(k)),
        })).grow(1),
        text!(error()),
        button!{on_click = || nav::back(), t!["Back"]},
    ].background(Paint::Color(rgba(0, 0, 0, 200))))
}
```

- 空のスロットは「Empty」と表示され、サムネイルは空です。
- スクリプトが変更されてセーブが合わなくなった場合、[`nav::load`](../system-nav.md#load) は `Err` を返します。スタックは変わらず、画面に `Divergence::describe()` を表示します。
- オートセーブは `SaveSlot::Auto(0)` … `Auto(auto_slots - 1)` で、エンジンが選択肢ごとと終了時に書き込みます。ロード画面で同じように一覧できます。

## CG ギャラリー {#gallery}

[`seen_image`](../system.md#seen_image) は画像がこれまでに画面に出たかどうかを返すので、ギャラリーのために物語側で記録をする必要はありません。`image!` のソースはリアクティブです。

```kix
use std::system::seen_image;
use std::ui::Fit;

mod CG {
    pub const FIREWORKS: ImageAsset = asset("cg/fireworks.png");
    pub const ROOF: ImageAsset = asset("cg/roof.png");
    pub const STATION: ImageAsset = asset("cg/station.png");
}
const LOCKED_TILE: ImageAsset = asset("cg/locked.png");

fn cg_or_lock(cg: ImageAsset) -> ImageAsset {
    if seen_image(cg) { cg } else { LOCKED_TILE }
}

fn cg_tile(cg: ImageAsset, set_viewing: Fn(Option<ImageAsset>)) -> Element {
    button!{
        disabled = !seen_image(cg),
        on_click = || set_viewing(Some(cg)),
        image!(fit = Fit::Cover, cg_or_lock(cg)),
    }.width(320).height(180)
}

fn Gallery() -> RootPage {
    let (viewing, set_viewing) = signal(None);
    RootPage(layer![
        column![
            row![
                cg_tile(CG::FIREWORKS, set_viewing),
                cg_tile(CG::ROOF, set_viewing),
                cg_tile(CG::STATION, set_viewing),
            ].gap(24),
            button!{on_click = || nav::goto(Title), t!["Back"]},
        ].gap(24).at(point(640, 360), CENTER),
        show(
            || match viewing() {
                Some(_) => true,
                None => false,
            },
            || button!{
                on_click = || set_viewing(None),
                image!(fit = Fit::Contain, match viewing() {
                    Some(cg) => cg,
                    None => LOCKED_TILE,
                }),
            }.width(1280).height(720),
        ).cover(),
    ])
}
```

ギャラリーはルートページで、`nav::goto(Gallery)` で入り、`nav::goto(Title)` で出ます。ルートページはベースなので、ここで `nav::back()` を使うとエラー `E6002` になります。

## 音楽鑑賞 {#music-room}

[`heard`](../system.md#heard) はトラックがこれまでに再生されたかどうかを返し、[`set_music`](../system.md#set_music) はシステム音楽チャンネルでトラックを再生します。このチャンネルはゲームが実行されていない間に聞こえます。

```kix
use std::system::{heard, set_music};

const THEME: AudioAsset = asset("music/theme.ogg");
const EVENING: AudioAsset = asset("music/evening.ogg");

fn track_button(title: Text, track: AudioAsset) -> Element {
    button!{
        disabled = !heard(track),
        on_click = || set_music(Some(looped(track)), crossfade(ms(500))),
        text!(if heard(track) { title } else { "???" }),
    }
}

fn MusicRoom() -> RootPage {
    RootPage(column![
        track_button("Theme", THEME),
        track_button("Evening", EVENING),
        button!{on_click = || set_music(None, crossfade(ms(500))), t!["Stop"]},
        button!{on_click = || nav::goto(Title), t!["Back"]},
    ].gap(12))
}
```

> **安定性:** 予定 — 現在再生中のものの読み取り（現在のトラックの強調表示や、再生位置・一時停止状態の表示のため）は、まだ設計されていません。

## 回想シーン {#replay}

回想は別の `#[entry]` シーンで、[`nav::start`](../system-nav.md#start) で開始します。プレイヤーが初めてそのシーンに到達したときに物語が `write` で書き込む `Meta<bool>` によって解放されます。

```kix
// story.kix
pub static SEEN_FESTIVAL: Meta<bool> = meta(false);

#[scene]
pushdown fn FestivalNight() {
    s!(render = festival, t!["The lanterns came on one by one."]);
    // …
    SEEN_FESTIVAL.write(true);
}

#[scene]
#[entry]
pub pushdown fn FestivalReplay() {
    FestivalNight();
}
```

```kix
// ui.kix
fn Replays() -> RootPage {
    RootPage(column![
        button!{
            disabled = !story::SEEN_FESTIVAL.get(),
            on_click = || nav::start(story::FestivalReplay, (), on_replay_end),
            t!["Festival night"],
        },
        button!{on_click = || nav::goto(Title), t!["Back"]},
    ])
}

fn on_replay_end(_end: ()) {
    nav::goto(Replays)
}
```

- 回想は新しい物語として始まり、物語変数は初期値を持ちます。回想するシーンに必要な状態は、エントリーの引数として渡します。
- `Meta::get` はリアクティブなので、メタの値が変わるとすぐにボタンが有効になります。
- 物語はメタに書き込むだけで（`write` は `pushdown fn` です）、読むことはありません。メタが物語に届くのはエントリーの引数を通してだけです。

## バックログ {#backlog}

バックログは `Narrative::history()` を一覧するゲームオーバーレイです。ゲームビューから `game.call(Backlog)` で、またはバインディングの `Action::OpenInGame(Backlog)` で開きます。

```kix
use std::narrative::{Entry, Line};
use std::ui::scroll;

fn line_text(line: Line) -> Text {
    match line.text {
        Some(text) => text,
        None => "",
    }
}

fn entry_view(entry: Entry) -> Element {
    match entry {
        Entry::Line(line) => text!(line_text(line)),
        Entry::Chosen(text) => label(text),
    }
}

pub fn Backlog(game: Narrative) -> GameOverlay {
    GameOverlay(column![
        scroll(|| each(|| game.history(), |entry| entry_view(entry))).grow(1),
        button!{on_click = || nav::back(), t!["Close"]},
    ].background(Paint::Color(rgba(0, 0, 0, 200))))
}
```

エントリー、ボイスの再生（`Narrative::replay_voice`）、巻き戻し（`Narrative::rewind`）については [`std::narrative`](../narrative.md) で説明しています。
