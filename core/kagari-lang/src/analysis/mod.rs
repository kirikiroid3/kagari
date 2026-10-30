//! Whole-program analyses (compilation stage 5).
//!
//! - [`story`]: the story as a recursive state machine over finite data: termination,
//!   stuck choices, the screens that reach each step (consistency and bounded screens),
//!   reachable steps and scenes.
//! - [`influence`]: story variables that never influence the story.
//! - [`system`]: the mode-graph checks of the game system.
//!
//! See `docs/dev/src/compiler/analysis.md`, and in the Language Reference
//! (`docs/site/src/content/docs/en/reference/`) `concepts/scenes.md#termination`,
//! `concepts/presentation.md` (consistency, unbounded screens) and
//! `concepts/system.md#system-checks`.

mod influence;
mod story;
pub(crate) mod system;

use crate::db::Db;
use crate::diagnostic::Diag;
use crate::input::Program;

/// The results of stage 5 for a whole program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    pub diagnostics: Vec<Diag>,
}

#[salsa::tracked(returns(ref))]
pub fn analysis(db: &dyn Db, program: Program) -> Analysis {
    let mut diagnostics = story::story_analysis(db, program).diagnostics.clone();
    diagnostics.extend(influence::check(db, program));
    diagnostics.extend(system::check(db, program));
    Analysis { diagnostics }
}

/// The explored story graph, as `.kagari-cache` files store it; `None` when the story
/// analysis explored nothing. Runs (or reuses) the story analysis: call it only for a
/// program whose stages 1–4 reported no error, as [`analysis`] requires.
pub fn story_graph(db: &dyn Db, program: Program) -> Option<kagari_debug::StoryGraph> {
    story::graph(db, program)
}
