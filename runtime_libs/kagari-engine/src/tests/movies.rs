//! Movies through a booted engine: movie audio pauses with the game, and a movie that starts
//! counts as watched, but not while a save replays.

use kagari_vm::{AssetId, Value};

use super::{SYSTEM, TestPlatform, advance_lines, boot, line};
use crate::frame::ThumbKey;
use crate::saves::{load_slot, save_slot};
use crate::system::NavOp;
use crate::system::imports::SystemImport;
use crate::{AudioCommand, Engine};

const MOVIE_SCENE: &str = r#"
use std::ui::spacer;

const M: VideoAsset = asset("m.webm");
const MOVIE: EntityId = EntityId(20);

#[scene]
#[entry]
pushdown fn Start() {
    s!(t!["Before."]);
    s!(render = show_movie, t!["Movie."]);
    s!(t!["After."]);
}

const fn show_movie(mut screen: Screen) -> Screen {
    screen.entity(MOVIE).movie().change(std::screen::movie(M)).apply_animation(cut());
    screen
}

fn Title() -> RootPage {
    let _ = Over;
    nav::start(Start, (), |_end| ());
    RootPage(spacer())
}

fn Over() -> Overlay {
    Overlay(spacer())
}
"#;

fn source() -> String {
    format!("{SYSTEM}{MOVIE_SCENE}")
}

fn movie_id(e: &Engine) -> AssetId {
    let program = &e.state.program;
    let index = program
        .module()
        .assets
        .iter()
        .position(|a| program.string(a.path) == "m.webm")
        .unwrap();
    AssetId(u32::try_from(index).unwrap())
}

/// `std::system::watched(m.webm)`.
fn watched(e: &mut Engine, p: &mut TestPlatform) -> bool {
    let id = movie_id(e);
    let mut out = None;
    e.with_ctx(p, 0, |ctx, vm| {
        out = Some(SystemImport::Watched.call(ctx, vm, vec![Value::Asset(id)])?);
        Ok(())
    });
    matches!(out, Some(Value::Bool(true)))
}

/// Boots the story and advances to the movie's line; the audio before it is dropped.
fn at_movie(p: &mut TestPlatform) -> (Engine, u64) {
    let mut e = boot(&source(), p);
    let mut now = 0;
    e.update(p, now);
    assert_eq!(line(&e).as_deref(), Some("Before."));
    e.take_audio();
    advance_lines(&mut e, p, 1, &mut now);
    assert_eq!(line(&e).as_deref(), Some("Movie."));
    (e, now)
}

#[test]
fn a_started_movie_plays_its_audio_and_counts_as_watched() {
    let mut p = TestPlatform::default();
    let mut e = boot(&source(), &mut p);
    e.update(&mut p, 0);
    assert!(!watched(&mut e, &mut p));
    e.take_audio();
    let mut now = 0;
    advance_lines(&mut e, &mut p, 1, &mut now);
    let audio = e.take_audio();
    assert!(
        audio.iter().any(|c| matches!(
            c,
            AudioCommand::PlayMovie { path, looping: false, .. } if path == "m.webm"
        )),
        "{audio:?}"
    );
    assert!(watched(&mut e, &mut p));
}

#[test]
fn an_overlay_pauses_movie_audio_until_it_closes() {
    let mut p = TestPlatform::default();
    let (mut e, mut now) = at_movie(&mut p);
    let audio = e.take_audio();
    let Some(id) = audio.iter().find_map(|c| match c {
        AudioCommand::PlayMovie { id, .. } => Some(*id),
        _ => None,
    }) else {
        panic!("{audio:?}")
    };
    let over = e.state.program.find_function("crate::Over").unwrap();
    e.state.pending_nav.push(NavOp::Call(Value::Func(over)));
    now += 10;
    e.update(&mut p, now);
    assert_eq!(e.state.system.modes.len(), 2);
    assert_eq!(e.take_audio(), [AudioCommand::Pause { id }]);
    e.state.pending_nav.push(NavOp::Back);
    now += 10;
    e.update(&mut p, now);
    assert_eq!(e.state.system.modes.len(), 1);
    assert_eq!(e.take_audio(), [AudioCommand::Resume { id }]);
}

#[test]
fn a_replayed_save_marks_no_movie_watched_before_it_is_installed() {
    let mut p = TestPlatform::default();
    let (mut e, mut now) = at_movie(&mut p);
    advance_lines(&mut e, &mut p, 1, &mut now);
    assert_eq!(line(&e).as_deref(), Some("After."));
    e.with_ctx(&mut p, now, |ctx, vm| {
        save_slot(ctx, vm, ThumbKey::Manual(0));
        Ok(())
    });
    drop(e);
    p.stores.remove("meta.kisave");

    let mut e = boot(&source(), &mut p);
    e.update(&mut p, 0);
    e.take_audio();
    let mut result = None;
    e.with_ctx(&mut p, 10, |ctx, vm| {
        result = Some(load_slot(ctx, vm, Some(ThumbKey::Manual(0))));
        Ok(())
    });
    assert_eq!(result, Some(Ok(())));
    assert!(!watched(&mut e, &mut p));
    assert!(
        !e.take_audio()
            .iter()
            .any(|c| matches!(c, AudioCommand::PlayMovie { .. }))
    );
    // Installed: the restored screen shows the movie, which now plays.
    e.update(&mut p, 20);
    assert_eq!(line(&e).as_deref(), Some("After."));
    assert!(watched(&mut e, &mut p));
}
