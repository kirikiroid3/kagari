//! The pilots that steer the engine's drives for the query commands.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use kagari_engine::debug::{ArmView, GameView, Pilot, Steer, StepId, StepKind};

use super::exec::Run;
use super::views::{detail_of, ls_line};
use crate::frames::Artifact;
use crate::hook::HookState;
use crate::protocol::{Detail, LsLine};
use crate::tracker::Tracker;

/// Stays at the trace's position.
pub(super) struct Stay;

impl Pilot for Stay {
    fn steer(&mut self, _view: &GameView<'_>) -> Steer {
        Steer::Stop
    }
}

/// Goes on through lines; at a choice, stops.
fn through_lines(kind: &StepKind<'_>) -> Steer {
    match kind {
        StepKind::Say { .. } => Steer::Next,
        StepKind::Choice { .. } => Steer::Stop,
    }
}

/// A step of a segment: its ID, its `ls` line, and the trace's line count there.
pub(super) struct SegmentStep {
    pub step: StepId,
    pub line: LsLine,
    pub steps: u32,
}

/// Goes on through lines and stops at the next choice (or `limit` lines), recording each step.
pub(super) struct Segment {
    pub lines: Vec<SegmentStep>,
    limit: usize,
    pub hit_limit: bool,
}

impl Segment {
    pub fn new(limit: usize) -> Segment {
        Segment {
            lines: Vec::new(),
            limit,
            hit_limit: false,
        }
    }
}

impl Pilot for Segment {
    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        let Some(step) = &view.step else {
            return Steer::Stop;
        };
        self.lines.push(SegmentStep {
            step: step.step,
            line: ls_line(view, step),
            steps: view.trace.steps_after_last_choice,
        });
        self.hit_limit =
            matches!(step.kind, StepKind::Say { .. }) && self.lines.len() >= self.limit;
        if self.hit_limit {
            return Steer::Stop;
        }
        through_lines(&step.kind)
    }
}

/// Goes on to `target` within the segment and takes its detail.
pub(super) struct DetailPilot {
    target: StepId,
    artifact: Option<Rc<Artifact>>,
    pub found: Option<Detail>,
}

impl DetailPilot {
    pub fn new(target: StepId, artifact: Option<Rc<Artifact>>) -> DetailPilot {
        DetailPilot {
            target,
            artifact,
            found: None,
        }
    }
}

impl Pilot for DetailPilot {
    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        let Some(step) = &view.step else {
            return Steer::Stop;
        };
        if step.step == self.target {
            self.found = Some(detail_of(view, step, self.artifact.as_deref()));
            return Steer::Stop;
        }
        through_lines(&step.kind)
    }
}

/// Whether the arm `spec` names this arm: its arm ID, `#index`, or the last segments of the
/// ID (`B`, `Pick::B` for `crate::Pick::B`).
fn arm_matches(spec: &str, arm_id: &str, index: usize) -> bool {
    arm_id == spec
        || spec.strip_prefix('#').and_then(|i| i.parse::<usize>().ok()) == Some(index)
        || arm_id
            .strip_suffix(spec)
            .is_some_and(|rest| rest.ends_with("::"))
}

/// The one shown arm `spec` names: its index and arm ID; else why not (`choice` names it).
fn resolve_arm<'a>(
    arms: &[ArmView<'a>],
    spec: &str,
    choice: &str,
) -> Result<(u32, &'a str), String> {
    let shown = || arms.iter().enumerate().filter(|(_, a)| a.shown);
    let matching: Vec<(usize, &str)> = shown()
        .filter(|(i, a)| arm_matches(spec, a.arm_id, *i))
        .map(|(i, a)| (i, a.arm_id))
        .collect();
    match matching.as_slice() {
        [(index, arm)] => u32::try_from(*index)
            .map(|index| (index, *arm))
            .map_err(|_| "too many arms".to_owned()),
        [] => {
            let offered: Vec<String> = shown().map(|(_, a)| format!("`{}`", a.arm_id)).collect();
            Err(format!(
                "the choice `{choice}` does not offer `{spec}` (it offers {})",
                offered.join(", ")
            ))
        }
        _ => Err(format!(
            "`{spec}` names more than one arm of the choice `{choice}`"
        )),
    }
}

/// Selects the arms in turn, going on through lines; stops at the step after the last one.
pub(super) struct PushPilot<'a> {
    arms: &'a [String],
    pub next: usize,
    /// The choice ID of each selection.
    pub selected: Vec<String>,
    pub error: Option<String>,
}

impl<'a> PushPilot<'a> {
    pub fn new(arms: &'a [String]) -> PushPilot<'a> {
        PushPilot {
            arms,
            next: 0,
            selected: Vec::new(),
            error: None,
        }
    }
}

impl Pilot for PushPilot<'_> {
    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        let (Some(spec), Some(step)) = (self.arms.get(self.next), &view.step) else {
            return Steer::Stop;
        };
        let StepKind::Choice { id, arms } = &step.kind else {
            return Steer::Next;
        };
        let choice = format!("{}/{id}", view.program.function_path(step.step.func));
        match resolve_arm(arms, spec, &choice) {
            Ok((index, arm)) => {
                self.selected.push(format!("{choice}/{arm}"));
                self.next = self.next.saturating_add(1);
                Steer::Select(index)
            }
            Err(error) => {
                self.error = Some(error);
                Steer::Stop
            }
        }
    }
}

/// Feeds every step to a tracker.
pub(super) struct ObservePilot<'g> {
    pub tracker: Tracker<'g>,
}

impl Pilot for ObservePilot<'_> {
    fn observe(&mut self, view: &GameView<'_>) {
        self.tracker.observe(view);
    }

    fn steer(&mut self, _view: &GameView<'_>) -> Steer {
        Steer::Stop
    }
}

/// Follows a `nav` path's arms (at most `budget` steps) and says whether `target` is reached.
pub(super) struct RealizePilot<'a> {
    push: PushPilot<'a>,
    target: StepId,
    budget: u32,
    pub reached: bool,
}

impl<'a> RealizePilot<'a> {
    pub fn new(arms: &'a [String], target: StepId, budget: u32) -> RealizePilot<'a> {
        RealizePilot {
            push: PushPilot::new(arms),
            target,
            budget,
            reached: false,
        }
    }
}

impl Pilot for RealizePilot<'_> {
    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        let Some(step) = &view.step else {
            return Steer::Stop;
        };
        self.reached = step.step == self.target;
        match self.budget.checked_sub(1) {
            Some(budget) if !self.reached => self.budget = budget,
            _ => return Steer::Stop,
        }
        // After the last arm, lines still go on to the target.
        if self.push.next == self.push.arms.len() {
            return through_lines(&step.kind);
        }
        self.push.steer(view)
    }
}

/// `continue`, `next`, `stepIn`, `stepOut`.
pub(super) struct ExecPilot<'a> {
    how: Run,
    /// The fiber's depth where the run started.
    depth: usize,
    first: bool,
    breakpoints: &'a HashSet<StepId>,
    hook: &'a Rc<RefCell<HookState>>,
    interrupted: &'a mut dyn FnMut() -> bool,
    /// Why it stopped.
    pub reason: Option<&'static str>,
}

impl<'a> ExecPilot<'a> {
    pub fn new(
        how: Run,
        depth: usize,
        breakpoints: &'a HashSet<StepId>,
        hook: &'a Rc<RefCell<HookState>>,
        interrupted: &'a mut dyn FnMut() -> bool,
    ) -> ExecPilot<'a> {
        ExecPilot {
            how,
            depth,
            first: true,
            breakpoints,
            hook,
            interrupted,
            reason: None,
        }
    }

    /// Why the run stops at `step`, if it does.
    fn stops(&mut self, view: &GameView<'_>, step: StepId, choice: bool) -> Option<&'static str> {
        let depth = view.fiber.map_or(0, |f| f.frames().len());
        let stepped = match self.how {
            Run::Continue => false,
            Run::StepIn => true,
            Run::Next => depth <= self.depth,
            Run::StepOut => depth < self.depth,
        };
        // A code step that reached a step ends here; a breakpoint wins over a plain step.
        if self.hook.borrow().story_step() {
            Some("step")
        } else if self.breakpoints.contains(&step) {
            Some("breakpoint")
        } else if stepped {
            Some("step")
        } else if choice {
            Some("choice")
        } else if (self.interrupted)() {
            Some("pause")
        } else {
            None
        }
    }
}

impl Pilot for ExecPilot<'_> {
    fn steer(&mut self, view: &GameView<'_>) -> Steer {
        if std::mem::replace(&mut self.first, false) {
            return Steer::Next;
        }
        let Some(step) = &view.step else {
            return Steer::Stop;
        };
        let choice = matches!(step.kind, StepKind::Choice { .. });
        self.reason = self.stops(view, step.step, choice);
        match self.reason {
            Some(_) => Steer::Stop,
            None => Steer::Next,
        }
    }
}
