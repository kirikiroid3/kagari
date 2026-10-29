//! Reveal timing of a line: units, per-unit durations, time pauses, click pauses and marks
//! (`docs/site/src/content/docs/en/std/narrative-text.md` §Reveal, §Pauses, §Marks, §Clicks and
//! the backlog).

use std::rc::Rc;

use kagari_vm::{Text, Value};

use crate::frame::Reveal;
use crate::screen::runtime::MarkTimes;

/// The player's speed setting at step entry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Speeds {
    pub text_speed_ms: u32,
}

#[derive(Clone, Copy, Debug)]
enum Speed {
    Player,
    PerChar(u32),
    Scaled(i128, u32),
    Instant,
}

/// A run of units revealed without a click; times are relative to the segment start.
#[derive(Clone, Copy, Debug)]
struct Segment {
    first_unit: u32,
    end_unit: u32,
    end_ms: u64,
}

/// The precomputed timing of a text.
#[derive(Debug)]
pub(crate) struct RevealPlan {
    /// `(start, len)` of every unit, relative to its segment's start.
    units: Vec<(u64, u64)>,
    segments: Vec<Segment>,
    /// `(mark, segment, ms)`, first occurrence only.
    marks: Vec<(u8, usize, u64)>,
}

struct Builder {
    speeds: Speeds,
    plan: RevealPlan,
    clock: u64,
    first_unit: u32,
}

/// The field `index` of an ADT value, unwrapped from `Option::Some`; `None` for `Option::None`.
fn option_field(v: &Value, index: usize) -> Option<&Value> {
    let Value::Adt(adt) = v else {
        return None;
    };
    match adt.fields.get(index)? {
        Value::Adt(opt) if opt.variant == 1 => opt.fields.first(),
        _ => None,
    }
}

fn duration_ms(v: &Value) -> u64 {
    match v {
        Value::Adt(adt) => match adt.fields.first() {
            Some(Value::Int(ms)) => u64::try_from(*ms).unwrap_or(0),
            _ => 0,
        },
        _ => 0,
    }
}

impl Builder {
    fn unit_len(&self, speed: Speed) -> u64 {
        let player = u64::from(self.speeds.text_speed_ms);
        match speed {
            Speed::Player => player,
            Speed::PerChar(d) => u64::from(d),
            Speed::Instant => 0,
            Speed::Scaled(mantissa, scale) => {
                if mantissa <= 0 {
                    log::warn!("ignored `Scaled` reveal with a factor ≤ 0");
                    return player;
                }
                let r = 10i128
                    .checked_pow(scale)
                    .and_then(|p| i128::from(player).checked_mul(p))
                    .and_then(|n| n.checked_div(mantissa));
                match r {
                    Some(n) => u64::try_from(n.clamp(0, i128::from(u32::MAX))).unwrap_or(0),
                    None => u64::from(u32::MAX),
                }
            }
        }
    }

    fn close_segment(&mut self) {
        let end_unit = u32::try_from(self.plan.units.len()).unwrap_or(u32::MAX);
        self.plan.segments.push(Segment {
            first_unit: self.first_unit,
            end_unit,
            end_ms: self.clock,
        });
        self.first_unit = end_unit;
        self.clock = 0;
    }

    fn walk(&mut self, text: &Text, speed: Speed) {
        match text {
            Text::Plain(s) => {
                let len = self.unit_len(speed);
                for _ in 0..kagari_text::units(s) {
                    self.plan.units.push((self.clock, len));
                    self.clock = self.clock.saturating_add(len);
                }
            }
            Text::Ruby(p) => self.walk(&p.0, speed),
            Text::MonoRuby(p) => self.walk(&p.0, speed),
            Text::Concat(items) => {
                for item in items.iter() {
                    self.walk(item, speed);
                }
            }
            Text::Effect(p) => {
                let (base, eff) = (&p.0, &p.1);
                if let Some(Value::Adt(pause)) = option_field(eff, 9) {
                    match pause.variant {
                        0 => {
                            let d = pause.fields.first().map_or(0, duration_ms);
                            self.clock = self.clock.saturating_add(d);
                        }
                        _ => self.close_segment(),
                    }
                }
                if let Some(Value::Adt(mark)) = option_field(eff, 10)
                    && let Some(Value::Int(n)) = mark.fields.first()
                    && let Ok(n) = u8::try_from(*n)
                    && !self.plan.marks.iter().any(|(m, _, _)| *m == n)
                {
                    let segment = self.plan.segments.len();
                    self.plan.marks.push((n, segment, self.clock));
                }
                let speed = match option_field(eff, 8) {
                    Some(Value::Adt(r)) => match (r.variant, r.fields.first()) {
                        (0, _) => Speed::Player,
                        (1, Some(d)) => {
                            Speed::PerChar(u32::try_from(duration_ms(d)).unwrap_or(u32::MAX))
                        }
                        (2, Some(Value::Decimal(x))) => Speed::Scaled(x.mantissa(), x.scale()),
                        (3, _) => Speed::Instant,
                        _ => speed,
                    },
                    _ => speed,
                };
                self.walk(base, speed);
            }
        }
    }
}

impl RevealPlan {
    pub fn build(text: Option<&Text>, speeds: Speeds) -> Rc<RevealPlan> {
        let mut b = Builder {
            speeds,
            plan: RevealPlan {
                units: Vec::new(),
                segments: Vec::new(),
                marks: Vec::new(),
            },
            clock: 0,
            first_unit: 0,
        };
        if let Some(text) = text {
            b.walk(text, Speed::Player);
        }
        b.close_segment();
        Rc::new(b.plan)
    }

    fn units(&self) -> u32 {
        u32::try_from(self.units.len()).unwrap_or(u32::MAX)
    }
}

/// Where a reveal is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RevealState {
    Revealing,
    WaitingClick { since: u64 },
    Done { at: u64 },
}

/// A reveal running from `text_start` (story ms).
#[derive(Clone, Debug)]
pub(crate) struct RevealRun {
    plan: Rc<RevealPlan>,
    text_start: u64,
    released: Vec<u64>,
    forced: Option<u64>,
}

impl RevealRun {
    pub fn new(plan: Rc<RevealPlan>, text_start: u64) -> RevealRun {
        RevealRun {
            plan,
            text_start,
            released: Vec::new(),
            forced: None,
        }
    }

    /// No units; every mark resolves to `at`.
    pub fn empty(at: u64) -> RevealRun {
        RevealRun::new(RevealPlan::build(None, Speeds { text_speed_ms: 0 }), at)
    }

    pub fn units(&self) -> u32 {
        self.plan.units()
    }

    fn current(&self) -> usize {
        self.released.len()
    }

    fn seg_start(&self, k: usize) -> u64 {
        match k.checked_sub(1) {
            None => self.text_start,
            Some(i) => self.released.get(i).copied().unwrap_or(self.text_start),
        }
    }

    fn segment(&self, k: usize) -> Segment {
        self.plan.segments.get(k).copied().unwrap_or(Segment {
            first_unit: self.plan.units(),
            end_unit: self.plan.units(),
            end_ms: 0,
        })
    }

    fn has_later(&self, k: usize) -> bool {
        k.saturating_add(1) < self.plan.segments.len()
    }

    pub fn state(&self, t: u64) -> RevealState {
        let k = self.current();
        let start = self.seg_start(k);
        let natural = start.saturating_add(self.segment(k).end_ms);
        // The earlier of the natural end and a forced completion, once reached.
        let forced = self.forced.filter(|f| *f <= t);
        let end = match (forced, t >= natural) {
            (Some(f), true) => Some(f.min(natural)),
            (Some(f), false) => Some(f),
            (None, true) => Some(natural),
            (None, false) => None,
        };
        match end {
            None => RevealState::Revealing,
            Some(at) if self.has_later(k) => RevealState::WaitingClick { since: at },
            Some(at) => RevealState::Done { at },
        }
    }

    pub fn reveal(&self, t: u64) -> Reveal {
        if matches!(self.state(t), RevealState::Done { .. }) {
            return Reveal::All;
        }
        let k = self.current();
        let seg = self.segment(k);
        let start = self.seg_start(k);
        if t < start {
            return Reveal::Units(seg.first_unit);
        }
        let l = if self.forced.is_some_and(|f| t >= f) {
            seg.end_ms
        } else {
            t.saturating_sub(start)
        };
        let mut full = seg.first_unit;
        let mut partial = 0.0f32;
        for i in seg.first_unit..seg.end_unit {
            let Some(&(s, len)) = self.plan.units.get(i as usize) else {
                break;
            };
            if s.saturating_add(len) <= l {
                full = i.saturating_add(1);
            } else {
                if l > s && len > 0 {
                    partial = (l.saturating_sub(s)) as f32 / len as f32;
                }
                break;
            }
        }
        if partial > 0.0 {
            Reveal::UnitsWithFade { full, partial }
        } else {
            Reveal::Units(full)
        }
    }

    /// A click while revealing: the current segment shows at once.
    pub fn complete_segment(&mut self, t: u64) {
        self.forced = Some(t);
    }

    /// A click at a click pause.
    pub fn release(&mut self, t: u64) {
        if self.has_later(self.current()) {
            self.released.push(t);
        }
        self.forced = None;
    }

    /// Skip: everything shows at once.
    pub fn complete_all(&mut self, t: u64) {
        while self.has_later(self.current()) {
            self.released.push(t);
        }
        self.forced = Some(t);
    }
}

impl MarkTimes for RevealRun {
    fn mark_ms(&self, mark: u8) -> Option<u64> {
        let current = self.current();
        let (k, ms) = match self.plan.marks.iter().find(|(m, _, _)| *m == mark) {
            Some(&(_, k, ms)) => (k, ms),
            None => {
                let last = self.plan.segments.len().saturating_sub(1);
                (last, self.segment(last).end_ms)
            }
        };
        if k > current {
            return None;
        }
        let at = self.seg_start(k).saturating_add(ms);
        Some(match self.forced {
            Some(f) if k == current => at.min(f),
            _ => at,
        })
    }
}

/// What the click-wait glyph shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ClickWait {
    None,
    /// At a click pause inside the line.
    Pause,
    /// At the end of the line.
    Line,
}

/// What `message()` draws.
#[derive(Clone, Debug)]
pub(crate) struct MessageView {
    pub text: Text,
    pub reveal: Reveal,
    pub click_wait: ClickWait,
}
