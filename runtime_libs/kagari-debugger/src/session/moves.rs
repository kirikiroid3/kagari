//! The moves and lookups between the last choice and the next: `ls`, `goto`, `detail`, `push`,
//! `pop`, `seed`.

use kagari_engine::debug::{DriveEnd, Trace};

use super::pilots::{DetailPilot, PushPilot, Segment, Stay};
use super::views::detail_of;
use super::{Debugger, Result, Target, parse_step};
use crate::protocol::{Detail, LsBody, LsLine, Position, PushBody, SeedBody};

/// The most lines `ls` lists.
const LS_LIMIT: usize = 10_000;

impl Debugger {
    /// The lines from the current step up to the next choice (included).
    pub fn ls(
        &mut self,
        t: &mut Target<'_>,
        head: Option<u32>,
        tail: Option<u32>,
    ) -> Result<LsBody> {
        let trace = Self::trace(t)?;
        let mut pilot = Segment::new(LS_LIMIT);
        let end = self.probe(t, &trace, &mut pilot)?;
        let ends = match (pilot.hit_limit, end) {
            (true, _) => "limit",
            (false, DriveEnd::Ended) => "end",
            (false, DriveEnd::Stopped) => "choice",
        };
        let total = pilot.lines.len();
        let mut lines: Vec<LsLine> = pilot.lines.into_iter().map(|s| s.line).collect();
        if let Some(head) = head {
            lines.truncate(head as usize);
        }
        if let Some(tail) = tail {
            lines.drain(..lines.len().saturating_sub(tail as usize));
        }
        Ok(LsBody {
            lines,
            total,
            ends: ends.to_owned(),
        })
    }

    /// The current trace cut back to the last choice (or the story's start).
    fn from_last_choice(t: &Target<'_>) -> Result<Trace> {
        let mut from = Self::trace(t)?;
        from.steps_after_last_choice = 0;
        Ok(from)
    }

    /// Jumps to `step`, which must lie between the last choice and the next one (the steps after
    /// the last choice up to the next choice, included).
    pub fn goto(&mut self, t: &mut Target<'_>, step: &str) -> Result<Position> {
        let target = parse_step(t.engine.program(), step)?;
        let mut trace = Self::from_last_choice(t)?;
        let mut segment = Segment::new(LS_LIMIT);
        self.probe(t, &trace, &mut segment)?;
        let Some(found) = segment.lines.iter().find(|s| s.step == target) else {
            let last = trace
                .choices
                .last()
                .and_then(|c| c.rsplit_once('/').map(|(choice, _)| format!("`{choice}`")));
            let next = segment
                .lines
                .last()
                .filter(|s| s.line.kind == "choice")
                .map(|s| format!("`{}`", s.line.step));
            return Err(format!(
                "`{step}` is not between the last choice ({}) and the next choice ({}); \
                 use `nav` to find a way there",
                last.as_deref().unwrap_or("the start of the story"),
                next.as_deref().unwrap_or("the end of the story"),
            ));
        };
        trace.steps_after_last_choice = found.steps;
        self.jump(t, &trace, &mut Stay)?;
        Ok(self.position(t))
    }

    /// Everything about `step` (default: the current one); another step must lie between the
    /// last choice and the next one.
    pub fn detail(&mut self, t: &mut Target<'_>, step: Option<&str>) -> Result<Detail> {
        let current = t
            .engine
            .debug_view()
            .and_then(|v| v.step.as_ref().map(|s| s.step));
        let target = match step {
            Some(step) => parse_step(t.engine.program(), step)?,
            None => current.ok_or("no game is running at a step")?,
        };
        if current == Some(target)
            && let Some(view) = t.engine.debug_view()
            && let Some(step) = &view.step
        {
            return Ok(detail_of(&view, step, self.artifact.as_deref()));
        }
        let from = Self::from_last_choice(t)?;
        let mut pilot = DetailPilot::new(target, self.artifact.clone());
        self.probe(t, &from, &mut pilot)?;
        pilot.found.ok_or_else(|| {
            format!(
                "`{}` is not between the last choice and the next choice",
                t.engine.program().step_path(target)
            )
        })
    }

    /// From the next choice on, selects each arm in turn; stops right after the last one.
    /// Unlike the moves that replay the past (`goto`, `pop`, `seed`), `push` runs story code
    /// that has not run yet, so instruction breakpoints stop in it.
    pub fn push(&mut self, t: &mut Target<'_>, arms: &[String]) -> Result<PushBody> {
        if arms.is_empty() {
            return Err("push needs at least one arm".to_owned());
        }
        Self::trace(t)?;
        let mut pilot = PushPilot::new(arms);
        let end = self.step(t, &mut pilot)?;
        if let Some(error) = pilot.error {
            return Err(error);
        }
        if end == DriveEnd::Ended {
            let left = arms.get(pilot.next).map_or("", String::as_str);
            return Err(format!(
                "the story ends before a choice offers `{left}` (stopped at its last step)"
            ));
        }
        Ok(PushBody {
            position: self.position(t),
            selected: pilot.selected,
        })
    }

    /// Back to the last choice, before its arm was selected.
    pub fn pop(&mut self, t: &mut Target<'_>) -> Result<Position> {
        let mut trace = Self::from_last_choice(t)?;
        if trace.choices.pop().is_none() {
            return Err("no choice has been made yet".to_owned());
        }
        self.jump(t, &trace, &mut Segment::new(LS_LIMIT))?;
        Ok(self.position(t))
    }

    /// The PRNG seed; with `seed`, replays the trace with it.
    pub fn seed(&mut self, t: &mut Target<'_>, seed: Option<u64>) -> Result<SeedBody> {
        let mut trace = Self::trace(t)?;
        if let Some(seed) = seed {
            trace.seed = seed;
            self.jump(t, &trace, &mut Stay)?;
        }
        Ok(SeedBody { seed: trace.seed })
    }
}
