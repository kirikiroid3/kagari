//! What the query engine reports about a step: its detail, its `ls` line and its scope.

use kagari_engine::debug::{GameView, StepKind, StepView};
use kagari_vm::Value;

use crate::frames::Artifact;
use crate::protocol::{ArmDetail, Detail, LsLine, Scope, Var};
use crate::values::{Names, display, plain, rich, to_graph};

/// Everything about the step the game is at.
pub(super) fn detail_of(
    view: &GameView<'_>,
    step: &StepView<'_>,
    artifact: Option<&Artifact>,
) -> Detail {
    let names = Names {
        program: view.program,
        types: view.types,
    };
    let mut detail = Detail {
        step: view.program.step_path(step.step),
        render: step.render.map(|r| display(names, r)),
        keyframe: keyframe(artifact, names, step.render),
        ..Detail::default()
    };
    match &step.kind {
        StepKind::Say { who, text, voice } => {
            detail.kind = "say".to_owned();
            detail.who = who.map(|w| rich(names, w));
            detail.text = text.map(|t| rich(names, t));
            detail.voice = voice.map(|v| view.asset_path(v).to_owned());
        }
        StepKind::Choice { id, arms } => {
            detail.kind = "choice".to_owned();
            detail.choice = Some((*id).to_owned());
            detail.arms = arms
                .iter()
                .map(|arm| ArmDetail {
                    arm: arm.arm_id.to_owned(),
                    text: rich(names, arm.text),
                    shown: arm.shown,
                })
                .collect();
        }
    }
    detail
}

/// The step record as a scope: the detail's fields, each arm a child of `arms`.
pub(super) fn step_scope(detail: Detail) -> Scope {
    let fields = [
        ("step", Some(detail.step)),
        ("render", detail.render),
        ("who", detail.who),
        ("text", detail.text),
        ("voice", detail.voice),
        ("id", detail.choice),
    ];
    let mut vars: Vec<Var> = fields
        .into_iter()
        .filter_map(|(name, value)| Some(Var::leaf(name, value?)))
        .collect();
    if !detail.arms.is_empty() {
        vars.push(Var {
            name: "arms".to_owned(),
            value: format!("{} arms", detail.arms.len()),
            children: detail
                .arms
                .into_iter()
                .map(|arm| {
                    let hidden = if arm.shown { "" } else { " (hidden)" };
                    Var::leaf(arm.arm, format!("{}{hidden}", arm.text))
                })
                .collect(),
        });
    }
    Scope {
        name: "Step".to_owned(),
        vars,
    }
}

/// Whether the render is a keyframe, as the story graph knows it; a step without a render is
/// not one.
fn keyframe(artifact: Option<&Artifact>, names: Names<'_>, render: Option<&Value>) -> Option<bool> {
    let graph = artifact?.graph.as_ref()?;
    let Some(render) = render else {
        return Some(false);
    };
    let render = to_graph(names, render)?;
    let index = graph.values.iter().position(|v| *v == render)?;
    Some(graph.is_keyframe(u32::try_from(index).ok()?))
}

/// The step's line in `ls`.
pub(super) fn ls_line(view: &GameView<'_>, step: &StepView<'_>) -> LsLine {
    let (kind, who, text) = match &step.kind {
        StepKind::Say { who, text, .. } => ("say", who.map(plain), text.map(plain)),
        StepKind::Choice { id, .. } => ("choice", None, Some((*id).to_owned())),
    };
    LsLine {
        step: view.program.step_path(step.step),
        kind: kind.to_owned(),
        who,
        text: text.unwrap_or_default(),
    }
}
