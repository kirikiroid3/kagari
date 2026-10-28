use kagari_archive::save::{Save, ValueRef};

use super::{TestPlatform, advance_lines, at_choice, boot, line, story, story2};
use crate::Engine;
use crate::frame::{Reveal, ThumbKey};
use crate::saves::{load_slot, replay::Divergence, save_slot};
use crate::story::{Entry, StoryInput, text_plain};

/// The backlog as strings: `Line <text>` or `Chosen <text>`.
fn history(e: &Engine) -> Vec<String> {
    e.state
        .game
        .as_ref()
        .unwrap()
        .backlog
        .iter()
        .map(|b| match &b.entry {
            Entry::Line { text, .. } => {
                format!("Line {}", text.as_ref().map(text_plain).unwrap_or_default())
            }
            Entry::Chosen(t) => format!("Chosen {}", text_plain(t)),
        })
        .collect()
}

/// Boots the story and saves at "Count 5" into `Manual(0)`.
fn save_after_choice(p: &mut TestPlatform) -> (Engine, u64) {
    let mut e = boot(&story(), p);
    let mut now = 0;
    advance_lines(&mut e, p, 3, &mut now);
    assert!(at_choice(&e));
    e.state.game.as_mut().unwrap().pending_input = Some(StoryInput::Select(1));
    now = now.saturating_add(10);
    e.update(p, now);
    assert_eq!(line(&e).as_deref(), Some("Count 5"));
    e.with_ctx(p, now, |ctx, vm| {
        save_slot(ctx, vm, ThumbKey::Manual(0));
        Ok(())
    });
    (e, now)
}

#[test]
fn choice_save_load_round_trips() {
    let mut p = TestPlatform::default();
    let (mut e, mut now) = save_after_choice(&mut p);
    let expected = vec![
        "Line One.",
        "Line Two.",
        "Line Three.",
        "Chosen B",
        "Line Count 5",
    ];
    assert_eq!(history(&e), expected);
    let save = Save::from_bytes(p.stores["save-manual-0.kisave"].clone()).unwrap();
    let root = save.root();
    let Some(ValueRef::Array(choices)) = root.get("choices") else {
        panic!("no choices")
    };
    let choices: Vec<_> = choices.iter().collect();
    assert_eq!(
        choices,
        vec![ValueRef::String("crate::Start/pick/crate::Pick::B")]
    );
    assert_eq!(root.get("steps_after_last_choice"), Some(ValueRef::U64(0)));
    assert_eq!(root.get("entry"), Some(ValueRef::String("crate::Start")));

    advance_lines(&mut e, &mut p, 1, &mut now);
    assert_eq!(line(&e).as_deref(), Some("End."));
    let generation = e.state.game.as_ref().unwrap().generation;
    let mut result = None;
    e.with_ctx(&mut p, now, |ctx, vm| {
        result = Some(load_slot(ctx, vm, Some(ThumbKey::Manual(0))));
        crate::system::modes::settle(ctx, vm)
    });
    assert_eq!(result, Some(Ok(())));
    assert_eq!(line(&e).as_deref(), Some("Count 5"));
    assert_eq!(history(&e), expected);
    assert_eq!(e.state.game.as_ref().unwrap().generation, generation + 1);
}

#[test]
fn renamed_choice_diverges() {
    let mut p = TestPlatform::default();
    save_after_choice(&mut p);
    let mut e = boot(&story2(), &mut p);
    let mut result = None;
    e.with_ctx(&mut p, 0, |ctx, vm| {
        result = Some(load_slot(ctx, vm, Some(ThumbKey::Manual(0))));
        Ok(())
    });
    let Some(Err(d)) = result else {
        panic!("the load did not diverge")
    };
    assert!(matches!(d, Divergence::ChoiceChanged { .. }));
    assert_eq!(
        d.describe(),
        "The story no longer reaches the choice `crate::Start/pick/crate::Pick::B`: replaying the save met `crate::Start/pick2` first."
    );
    assert_eq!(line(&e).as_deref(), Some("One."));
}

#[test]
fn quit_autosave_samples_the_latest_time() {
    let source = format!("use std::ui::message;\n{}", story()).replace(
        "game_screen!(game = game)",
        "message(game, text_style(26, WHITE))",
    );
    let mut p = TestPlatform::default();
    let mut e = boot(&source, &mut p);
    e.update(&mut p, 5000);
    e.quit(&mut p);
    assert!(p.stores.contains_key("save-auto-0.kisave"));
    // The message is fully revealed (the click-wait glyph may follow it).
    let reveals = p.thumbnail_reveals.last().unwrap();
    assert_eq!(reveals.first(), Some(&Reveal::All));
}
