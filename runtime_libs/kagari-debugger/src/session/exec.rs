//! Step-level execution: `continue`, `next`, `stepIn`, `stepOut` from a step.

use kagari_engine::debug::{DriveEnd, StepKind};

use super::pilots::ExecPilot;
use super::{Debugger, Result, Target};
use crate::protocol::StoppedBody;

/// How far [`Debugger::run`] goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    /// To a step breakpoint, a choice, or the end of the story.
    Continue,
    /// To the next step in this scene (or an outer one).
    Next,
    /// To the next step anywhere, a called scene's included.
    StepIn,
    /// To the step after the current scene returns.
    StepOut,
}

impl Debugger {
    /// Drives the story on from its current step, with side effects off, and installs it where
    /// it stops. `interrupted` is asked at every step whether a pause was requested.
    pub fn run(
        &mut self,
        t: &mut Target<'_>,
        how: Run,
        interrupted: &mut dyn FnMut() -> bool,
    ) -> Result<StoppedBody> {
        let view = t.engine.debug_view().ok_or("no game is running")?;
        let step = view.step.as_ref().ok_or("the story is not at a step")?;
        if let StepKind::Choice { id, .. } = &step.kind {
            return Err(format!(
                "the story waits at the choice `{}/{id}`: select an arm with `push`",
                view.program.function_path(step.step.func)
            ));
        }
        let depth = view.fiber.map_or(0, |f| f.frames().len());
        let mut pilot = ExecPilot::new(how, depth, &self.step_breakpoints, &self.hook, interrupted);
        let end = self.step(t, &mut pilot)?;
        Ok(match end {
            DriveEnd::Ended => self.stopped(
                t,
                "end",
                Some("the story returns after this step".to_owned()),
            ),
            DriveEnd::Stopped => self.stopped(t, pilot.reason.unwrap_or("step"), None),
        })
    }
}
