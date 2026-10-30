//! The debugger's view of the engine: probing, jumping and stepping drives, the hold, live
//! step breakpoints and pauses.

use super::{TestPlatform, advance_lines, at_choice, boot, key, line, story};
use crate::Key;
use crate::debug::{
    Arrival, DriveEnd, DriveError, GameView, LiveStop, Pilot, Steer, StepId, StepKind, StopReason,
    Trace,
};

/// Goes on from lines, records their text, and stops at the first choice.
#[derive(Default)]
struct UntilChoice {
    lines: Vec<String>,
    observed: Vec<Arrival>,
}

impl Pilot for UntilChoice {
    fn observe(&mut self, view: &GameView<'_>) {
        self.observed.push(view.arrival.clone());
    }

    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        match view.step.as_ref().map(|s| &s.kind) {
            Some(StepKind::Say { text, .. }) => {
                self.lines
                    .push(text.map(crate::story::text_plain).unwrap_or_default());
                Steer::Next
            }
            _ => Steer::Stop,
        }
    }
}

/// Stops at once.
struct Stay;

impl Pilot for Stay {
    fn steer(&mut self, _view: &GameView<'_>) -> Steer {
        Steer::Stop
    }
}

fn trace(e: &crate::Engine) -> Trace {
    e.state.game.as_ref().unwrap().trace.clone()
}

fn step(e: &crate::Engine, id: u32) -> StepId {
    StepId {
        func: e.program().find_function("crate::Start").unwrap(),
        id,
    }
}

#[test]
fn a_probe_looks_ahead_without_moving_the_game() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    assert_eq!(line(&e).as_deref(), Some("One."));
    let mut pilot = UntilChoice::default();
    let end = e.debug_probe(&mut p, 10, &trace(&e), &mut pilot).unwrap();
    assert_eq!(end, DriveEnd::Stopped);
    assert_eq!(pilot.lines, ["One.", "Two.", "Three."]);
    // Steering observes each step it enters, after the followed trace's own first step.
    assert_eq!(
        pilot.observed,
        [Arrival::Start, Arrival::Next, Arrival::Next, Arrival::Next]
    );
    assert_eq!(line(&e).as_deref(), Some("One."));
    assert!(!e.debug_held());
}

#[test]
fn a_jump_installs_the_position_held_until_released() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    let mut to = trace(&e);
    to.choices = vec!["crate::Start/pick/crate::Pick::B".to_owned()];
    let mut pilot = UntilChoice::default();
    // Stops after the selection: the pilot sees "Count 5" first and goes on to "End.", the last
    // line before the story returns, so the drive ends and nothing is installed.
    assert_eq!(
        e.debug_jump(&mut p, 10, &to, &mut pilot).unwrap(),
        DriveEnd::Ended
    );
    assert_eq!(line(&e).as_deref(), Some("One."));
    assert!(
        pilot
            .observed
            .contains(&Arrival::Select("crate::Pick::B".to_owned()))
    );

    assert_eq!(
        e.debug_jump(&mut p, 20, &to, &mut Stay).unwrap(),
        DriveEnd::Stopped
    );
    assert_eq!(line(&e).as_deref(), Some("Count 5"));
    assert!(e.debug_held());
    let mut now = 30;
    advance_lines(&mut e, &mut p, 2, &mut now);
    assert_eq!(line(&e).as_deref(), Some("Count 5"), "held: input dropped");
    e.debug_release();
    advance_lines(&mut e, &mut p, 1, &mut now);
    assert_eq!(line(&e).as_deref(), Some("End."));
}

#[test]
fn a_divergent_jump_changes_nothing() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    let mut to = trace(&e);
    to.choices = vec!["crate::Start/pick/crate::Pick::C".to_owned()];
    let error = e.debug_jump(&mut p, 10, &to, &mut Stay).unwrap_err();
    assert!(matches!(error, DriveError::Divergence(_)), "{error:?}");
    assert_eq!(line(&e).as_deref(), Some("One."));
    assert!(!e.debug_held());
}

#[test]
fn a_step_drives_the_live_game_on() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    let generation = e.debug_view().unwrap().generation;
    let mut pilot = UntilChoice::default();
    assert_eq!(
        e.debug_step(&mut p, 10, &mut pilot).unwrap(),
        DriveEnd::Stopped
    );
    assert!(at_choice(&e));
    assert!(e.debug_held());
    assert_eq!(pilot.lines, ["One.", "Two.", "Three."]);
    assert_eq!(
        pilot.observed,
        [Arrival::Next, Arrival::Next, Arrival::Next]
    );
    let view = e.debug_view().unwrap();
    assert_eq!(view.generation, generation);
    assert_eq!(view.trace.steps_after_last_choice, 3);
    assert_eq!(view.step.as_ref().unwrap().step, step(&e, 3));
}

#[test]
fn a_failed_step_goes_back_to_where_it_started() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    let mut now = 0;
    advance_lines(&mut e, &mut p, 1, &mut now);
    assert_eq!(line(&e).as_deref(), Some("Two."));

    struct SelectAtALine(u32);
    impl Pilot for SelectAtALine {
        fn steer(&mut self, _view: &GameView<'_>) -> Steer {
            self.0 += 1;
            if self.0 == 1 {
                Steer::Next
            } else {
                Steer::Select(0)
            }
        }
    }
    let error = e
        .debug_step(&mut p, now + 10, &mut SelectAtALine(0))
        .unwrap_err();
    assert!(matches!(error, DriveError::Steer(_)), "{error:?}");
    assert_eq!(line(&e).as_deref(), Some("Two."));
    assert_eq!(trace(&e).steps_after_last_choice, 1);
}

#[test]
fn live_play_stops_at_a_step_breakpoint() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    let three = step(&e, 2);
    e.debug_set_breakpoints([three]);
    let mut now = 0;
    advance_lines(&mut e, &mut p, 2, &mut now);
    assert_eq!(line(&e).as_deref(), Some("Three."));
    assert_eq!(
        e.debug_take_stop(),
        Some(LiveStop {
            step: three,
            reason: StopReason::Breakpoint
        })
    );
    assert_eq!(e.debug_take_stop(), None);
    advance_lines(&mut e, &mut p, 2, &mut now);
    assert_eq!(
        line(&e).as_deref(),
        Some("Three."),
        "held at the breakpoint"
    );
    e.debug_release();
    advance_lines(&mut e, &mut p, 1, &mut now);
    assert!(at_choice(&e));
}

#[test]
fn a_pause_holds_the_current_step() {
    let mut p = TestPlatform::default();
    let mut e = boot(&story(), &mut p);
    e.debug_pause();
    assert_eq!(
        e.debug_take_stop(),
        Some(LiveStop {
            step: step(&e, 0),
            reason: StopReason::Pause
        })
    );
    let now = 10;
    key(&mut e, &mut p, Key::Space, now);
    key(&mut e, &mut p, Key::Space, now + 10);
    e.update(&mut p, now + 20);
    assert_eq!(line(&e).as_deref(), Some("One."));
}
