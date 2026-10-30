//! Line stepping inside code. The runtime's code steps stop at every new source *position*; an
//! editor steps by line. After a `next` or `stepIn` from a stop inside code, each stop the step
//! makes is checked against the line it started on (a `stackTrace` of the runtime), and the
//! step is sent again, unseen, while the line and the depth stay the same.

use serde_json::Value;

use super::Adapter;
use super::answers::Waiting;
use crate::protocol::{StackBody, StoppedBody, command};

/// The most code steps that stay on one source line before a step is shown anyway.
const MAX_SILENT_STEPS: u32 = 200;

/// Where a stop inside code is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CodeAt {
    frames: usize,
    file: Option<String>,
    /// The source line, or the byte offset when the file cannot be read.
    line: Option<u32>,
}

/// A `next`/`stepIn` from a stop inside code, repeated while it stays on the line.
pub(super) struct Stepping {
    command: &'static str,
    from: Option<CodeAt>,
    tries: u32,
}

impl Stepping {
    pub fn new(command: &'static str, from: Option<CodeAt>) -> Stepping {
        Stepping {
            command,
            from,
            tries: 0,
        }
    }
}

impl Adapter<'_> {
    /// A `stopped` event while a code step is under way: check where it stopped first. Returns
    /// whether the stop is held back for the check.
    pub(super) fn check_step(&mut self, stopped: &StoppedBody) -> bool {
        self.stepping.is_some()
            && stopped.in_code
            && stopped.reason == "step"
            && self
                .forward(
                    command::STACK_TRACE,
                    Value::Null,
                    Waiting::StepCheck(stopped.clone()),
                )
                .is_ok()
    }

    /// The stack of a held-back stop: step again while on the same line, else show the stop.
    pub(super) fn step_checked(&mut self, stopped: StoppedBody, stack: Option<StackBody>) {
        let at = stack.map(|stack| self.code_position(&stack));
        let again = match (&mut self.stepping, &at) {
            (Some(stepping), Some(at))
                if stepping.from.as_ref() == Some(at) && stepping.tries < MAX_SILENT_STEPS =>
            {
                stepping.tries = stepping.tries.saturating_add(1);
                Some(stepping.command)
            }
            _ => None,
        };
        let sent = again.is_some_and(|step| {
            self.forward(step, Value::Null, Waiting::Restep(stopped.clone()))
                .is_ok()
        });
        if !sent {
            self.stepping = None;
            let fetch = at.is_none();
            self.code_at = at;
            self.show_stopped(stopped, fetch);
        }
    }

    /// Remembers where a stop inside code is, from a stack taken while it holds.
    pub(super) fn note_code_position(&mut self, stack: &StackBody) {
        if !self.told_stopped || !self.last_stop.as_ref().is_some_and(|s| s.in_code) {
            return;
        }
        let at = self.code_position(stack);
        if let Some(stepping) = &mut self.stepping
            && stepping.from.is_none()
        {
            stepping.from = Some(at.clone());
        }
        self.code_at = Some(at);
    }

    fn code_position(&self, stack: &StackBody) -> CodeAt {
        let top = stack.frames.first();
        let line = top.and_then(|frame| {
            let offset = frame.offset?;
            let located = match (&self.session, frame.file.as_deref()) {
                (Some(session), Some(file)) => session.locator.locate(file, offset),
                _ => None,
            };
            Some(located.map_or(offset, |at| at.line))
        });
        CodeAt {
            frames: stack.frames.len(),
            file: top.and_then(|f| f.file.clone()),
            line,
        }
    }
}
