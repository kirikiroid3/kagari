//! The explored story graph: every reachable story state, as the compiler's story analysis
//! found it.
//!
//! Scenes are summarized per *context* (a scene instance entered with given arguments and
//! story variables). A context's nodes are its suspension points; a scene call node leads to
//! the callee's context and resumes once per exit of it. Values, frames (the kept locals of a
//! node) and story-variable sets are stored once and referred to by index.

use std::collections::{BTreeSet, HashSet};

use kagari_archive::save::{ArrayMut, Scalar, TableMut, TableRef, ValueRef};

use crate::Error;
use crate::codec::{
    as_array, as_table, as_u32, bool_field, field, id, ids, int, list, opt_id, opt_u32_field,
    put_ids, put_strings, str_field, strings, u32_field,
};
use crate::value::Value;

/// The explored story graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoryGraph {
    /// Every reachable state was visited: no path was given up and no cap was hit.
    pub complete: bool,
    /// Why the exploration is incomplete (`W5006`, `W5007`); empty when complete.
    pub incomplete: Vec<Incomplete>,
    /// The story variables (mutable statics) by path; [`StoryGraph::globals`] entries hold
    /// their values in this order.
    pub vars: Vec<String>,
    /// Every value, stored once.
    pub values: Vec<Value>,
    /// Kept locals of a node, in the order of [`Point::locals`]; `None` for an uninitialized
    /// local. Entries are indices into `values`.
    pub frames: Vec<Vec<Option<u32>>>,
    /// Values of the story variables, in the order of `vars`.
    pub globals: Vec<Vec<u32>>,
    pub scenes: Vec<Scene>,
    pub contexts: Vec<Context>,
    /// The contexts of the entry scenes, one per argument value.
    pub roots: Vec<u32>,
    /// The render function values (indices into `values`, as [`NodeKind::Say`] and
    /// [`NodeKind::Choice`] refer to them) that are keyframe renders: their output does not
    /// depend on their input screen. Ascending and unique.
    pub keyframes: Vec<u32>,
}

/// Why the exploration stopped early somewhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Incomplete {
    /// The diagnostic code, `W5006` (a cap was hit) or `W5007` (a path was given up).
    pub code: String,
    pub message: String,
}

/// A scene: its path and the kept locals of its suspension points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    /// The scene's item path (without generic arguments).
    pub path: String,
    /// The suspension points that some node is at, ascending by block.
    pub points: Vec<Point>,
}

/// A suspension point of a scene: the compiler's block number and the names of the locals
/// its frames keep, in frame order (`""` for a compiler temporary).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Point {
    pub block: u32,
    pub locals: Vec<String>,
}

/// A scene entered with given arguments and story variables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    /// Index into [`StoryGraph::scenes`].
    pub scene: u32,
    /// The instance path, as the bytecode names the function: the step ID of a node is
    /// `{path}#{step}`.
    pub path: String,
    /// Indices into [`StoryGraph::values`].
    pub args: Vec<u32>,
    /// The story variables at entry: index into [`StoryGraph::globals`].
    pub globals: u32,
    /// Where the scene goes first.
    pub entry: Target,
    pub nodes: Vec<Node>,
    pub exits: Vec<Exit>,
    /// The call nodes that enter this context.
    pub callers: Vec<Caller>,
}

/// A call node that enters a context.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Caller {
    pub context: u32,
    pub node: u32,
}

/// A way a context returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exit {
    /// The returned value.
    pub ret: u32,
    /// The story variables at return.
    pub globals: u32,
    /// The final values of the `&mut` parameters, in order.
    pub outs: Vec<u32>,
}

/// A story state: a suspension point with the kept locals and the story variables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// The suspension point: a [`Point::block`] of the context's scene.
    pub block: u32,
    /// Index into [`StoryGraph::frames`].
    pub frame: u32,
    /// Index into [`StoryGraph::globals`].
    pub globals: u32,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A step without arms (`s!`, or `push_step` of a `Say`).
    Say {
        /// The step's ordinal in its function.
        step: u32,
        /// The step's render function: index into [`StoryGraph::values`].
        render: Option<u32>,
        next: Target,
    },
    /// A `choice!`: per shown arm, its arm ID (the arm value's variant path, or `#index` of
    /// the arm in the list) and where the story goes.
    Choice {
        step: u32,
        render: Option<u32>,
        arms: Vec<(String, Target)>,
    },
    /// A scene call: the callee's context and, per exit of it, where the caller goes.
    Call {
        callee: u32,
        exits: Vec<(u32, Target)>,
    },
    /// A random draw: per drawn value (index into [`StoryGraph::values`]), where the story
    /// goes.
    Draw { draws: Vec<(u32, Target)> },
}

/// Where a resumption leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// A node of the same context.
    Node(u32),
    /// An exit of the same context.
    Exit(u32),
    /// The path stopped: a fault, or a state the analysis could not continue from.
    Dead,
}

impl NodeKind {
    /// The step ordinal of a `Say` or `Choice`.
    pub fn step(&self) -> Option<u32> {
        match self {
            NodeKind::Say { step, .. } | NodeKind::Choice { step, .. } => Some(*step),
            NodeKind::Call { .. } | NodeKind::Draw { .. } => None,
        }
    }

    /// Every target this node resumes to within its context.
    pub fn targets(&self) -> Vec<Target> {
        match self {
            NodeKind::Say { next, .. } => vec![*next],
            NodeKind::Choice { arms, .. } => arms.iter().map(|(_, target)| *target).collect(),
            NodeKind::Call { exits, .. } => exits.iter().map(|(_, target)| *target).collect(),
            NodeKind::Draw { draws } => draws.iter().map(|(_, target)| *target).collect(),
        }
    }
}

impl StoryGraph {
    pub fn context(&self, id: u32) -> Option<&Context> {
        self.contexts.get(id as usize)
    }

    pub fn node(&self, context: u32, node: u32) -> Option<&Node> {
        self.context(context)?.nodes.get(node as usize)
    }

    pub fn value(&self, id: u32) -> Option<&Value> {
        self.values.get(id as usize)
    }

    /// Whether value `value` is a keyframe render.
    pub fn is_keyframe(&self, value: u32) -> bool {
        self.keyframes.binary_search(&value).is_ok()
    }

    /// The step ID (`{path}#{ordinal}`) of a step node.
    pub fn step_id(&self, context: u32, node: u32) -> Option<String> {
        let ctx = self.context(context)?;
        let step = ctx.nodes.get(node as usize)?.kind.step()?;
        Some(format!("{}#{step}", ctx.path))
    }

    /// The kept locals of a node with their values (`None` when uninitialized).
    pub fn locals(&self, context: u32, node: u32) -> Vec<(&str, Option<&Value>)> {
        let Some(ctx) = self.context(context) else {
            return Vec::new();
        };
        let Some(data) = ctx.nodes.get(node as usize) else {
            return Vec::new();
        };
        let names = self
            .scenes
            .get(ctx.scene as usize)
            .and_then(|scene| {
                let at = scene
                    .points
                    .binary_search_by_key(&data.block, |point| point.block)
                    .ok()?;
                scene.points.get(at)
            })
            .map_or(&[][..], |point| &point.locals[..]);
        let frame = self
            .frames
            .get(data.frame as usize)
            .map_or(&[][..], |frame| &frame[..]);
        names
            .iter()
            .zip(frame)
            .map(|(name, value)| (name.as_str(), value.and_then(|id| self.value(id))))
            .collect()
    }

    /// The story variables of a node by path, with their values.
    pub fn vars(&self, context: u32, node: u32) -> Vec<(&str, Option<&Value>)> {
        let globals = self
            .node(context, node)
            .and_then(|data| self.globals.get(data.globals as usize))
            .map_or(&[][..], |globals| &globals[..]);
        self.vars
            .iter()
            .zip(globals)
            .map(|(path, id)| (path.as_str(), self.value(*id)))
            .collect()
    }

    /// Every node reachable from the roots as `(context, node)`, in depth-first order: within
    /// a context along its targets, into a callee's context at each call.
    pub fn reached(&self) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let mut seen_contexts = HashSet::new();
        let mut seen_nodes = HashSet::new();
        let mut stack: Vec<(u32, Target)> = Vec::new();
        let mut enter = |context: u32, stack: &mut Vec<(u32, Target)>| {
            if seen_contexts.insert(context)
                && let Some(ctx) = self.context(context)
            {
                stack.push((context, ctx.entry));
            }
        };
        for root in self.roots.iter().rev() {
            enter(*root, &mut stack);
        }
        while let Some((context, target)) = stack.pop() {
            let Target::Node(node) = target else {
                continue;
            };
            if !seen_nodes.insert((context, node)) {
                continue;
            }
            let Some(data) = self.node(context, node) else {
                continue;
            };
            out.push((context, node));
            for target in data.kind.targets().into_iter().rev() {
                stack.push((context, target));
            }
            // Pushed last, so the callee is walked before the caller resumes.
            if let NodeKind::Call { callee, .. } = data.kind {
                enter(callee, &mut stack);
            }
        }
        out
    }

    /// The step ID of every step node reachable from the roots.
    pub fn reached_steps(&self) -> BTreeSet<String> {
        self.reached()
            .into_iter()
            .filter_map(|(context, node)| self.step_id(context, node))
            .collect()
    }

    pub(crate) fn put(&self, t: &mut TableMut<'_>) -> Result<(), Error> {
        t.set("complete", Scalar::Bool(self.complete))?;
        {
            let mut a = t.array("incomplete")?;
            for reason in &self.incomplete {
                let mut r = a.push_table()?;
                r.set("code", Scalar::String(&reason.code))?;
                r.set("message", Scalar::String(&reason.message))?;
            }
        }
        put_strings(t, "vars", &self.vars)?;
        {
            let mut a = t.array("values")?;
            for value in &self.values {
                value.push(&mut a)?;
            }
        }
        {
            let mut a = t.array("frames")?;
            for frame in &self.frames {
                let mut f = a.push_array()?;
                for slot in frame {
                    f.push(opt_id(*slot))?;
                }
            }
        }
        {
            let mut a = t.array("globals")?;
            for globals in &self.globals {
                let mut g = a.push_array()?;
                for value in globals {
                    g.push(id(*value))?;
                }
            }
        }
        {
            let mut a = t.array("scenes")?;
            for scene in &self.scenes {
                let mut s = a.push_table()?;
                s.set("path", Scalar::String(&scene.path))?;
                let mut points = s.array("points")?;
                for point in &scene.points {
                    let mut p = points.push_table()?;
                    p.set("block", id(point.block))?;
                    put_strings(&mut p, "locals", &point.locals)?;
                }
            }
        }
        {
            let mut a = t.array("contexts")?;
            for context in &self.contexts {
                let mut c = a.push_table()?;
                context.put(&mut c)?;
            }
        }
        put_ids(t, "roots", &self.roots)?;
        put_ids(t, "keyframes", &self.keyframes)
    }

    pub(crate) fn read(t: TableRef<'_>) -> Result<StoryGraph, Error> {
        Ok(StoryGraph {
            complete: bool_field(t, "complete")?,
            incomplete: list(t, "incomplete", |v| {
                let r = as_table(v, "incomplete")?;
                Ok(Incomplete {
                    code: str_field(r, "code")?.to_owned(),
                    message: str_field(r, "message")?.to_owned(),
                })
            })?,
            vars: strings(t, "vars")?,
            values: list(t, "values", Value::read)?,
            frames: list(t, "frames", |v| {
                as_array(v, "frames")?
                    .iter()
                    .map(|slot| match slot {
                        ValueRef::Null => Ok(None),
                        other => as_u32(other, "frames").map(Some),
                    })
                    .collect()
            })?,
            globals: list(t, "globals", |v| {
                as_array(v, "globals")?
                    .iter()
                    .map(|value| as_u32(value, "globals"))
                    .collect()
            })?,
            scenes: list(t, "scenes", |v| {
                let s = as_table(v, "scenes")?;
                Ok(Scene {
                    path: str_field(s, "path")?.to_owned(),
                    points: list(s, "points", |v| {
                        let p = as_table(v, "points")?;
                        Ok(Point {
                            block: u32_field(p, "block")?,
                            locals: strings(p, "locals")?,
                        })
                    })?,
                })
            })?,
            contexts: list(t, "contexts", |v| Context::read(as_table(v, "contexts")?))?,
            roots: ids(t, "roots")?,
            keyframes: {
                let keyframes = ids(t, "keyframes")?;
                if !keyframes.is_sorted_by(|a, b| a < b) {
                    return Err(Error::Malformed("keyframes"));
                }
                keyframes
            },
        })
    }
}

impl Context {
    fn put(&self, c: &mut TableMut<'_>) -> Result<(), Error> {
        c.set("scene", id(self.scene))?;
        c.set("path", Scalar::String(&self.path))?;
        put_ids(c, "args", &self.args)?;
        c.set("globals", id(self.globals))?;
        c.set("entry", target(self.entry))?;
        {
            let mut nodes = c.array("nodes")?;
            for node in &self.nodes {
                let mut n = nodes.push_table()?;
                node.put(&mut n)?;
            }
        }
        {
            let mut exits = c.array("exits")?;
            for exit in &self.exits {
                let mut e = exits.push_table()?;
                e.set("ret", id(exit.ret))?;
                e.set("globals", id(exit.globals))?;
                put_ids(&mut e, "outs", &exit.outs)?;
            }
        }
        let mut callers = c.array("callers")?;
        for caller in &self.callers {
            let mut pair = callers.push_array()?;
            pair.push(id(caller.context))?;
            pair.push(id(caller.node))?;
        }
        Ok(())
    }

    fn read(c: TableRef<'_>) -> Result<Context, Error> {
        Ok(Context {
            scene: u32_field(c, "scene")?,
            path: str_field(c, "path")?.to_owned(),
            args: ids(c, "args")?,
            globals: u32_field(c, "globals")?,
            entry: read_target(field(c, "entry")?)?,
            nodes: list(c, "nodes", |v| Node::read(as_table(v, "nodes")?))?,
            exits: list(c, "exits", |v| {
                let e = as_table(v, "exits")?;
                Ok(Exit {
                    ret: u32_field(e, "ret")?,
                    globals: u32_field(e, "globals")?,
                    outs: ids(e, "outs")?,
                })
            })?,
            callers: list(c, "callers", |v| {
                let pair = as_array(v, "callers")?;
                let at = |i| {
                    pair.get(i)
                        .ok_or(Error::Malformed("callers"))
                        .and_then(|v| as_u32(v, "callers"))
                };
                Ok(Caller {
                    context: at(0)?,
                    node: at(1)?,
                })
            })?,
        })
    }
}

impl Node {
    fn put(&self, n: &mut TableMut<'_>) -> Result<(), Error> {
        n.set("block", id(self.block))?;
        n.set("frame", id(self.frame))?;
        n.set("globals", id(self.globals))?;
        let (kind, next): (&str, Vec<Target>) = match &self.kind {
            NodeKind::Say { step, render, next } => {
                n.set("step", id(*step))?;
                n.set("render", opt_id(*render))?;
                ("say", vec![*next])
            }
            NodeKind::Choice { step, render, arms } => {
                n.set("step", id(*step))?;
                n.set("render", opt_id(*render))?;
                let mut a = n.array("arms")?;
                for (arm, _) in arms {
                    a.push(Scalar::String(arm))?;
                }
                ("choice", arms.iter().map(|(_, target)| *target).collect())
            }
            NodeKind::Call { callee, exits } => {
                n.set("callee", id(*callee))?;
                let ids: Vec<u32> = exits.iter().map(|(exit, _)| *exit).collect();
                put_ids(n, "exits", &ids)?;
                ("call", exits.iter().map(|(_, target)| *target).collect())
            }
            NodeKind::Draw { draws } => {
                let ids: Vec<u32> = draws.iter().map(|(value, _)| *value).collect();
                put_ids(n, "draws", &ids)?;
                ("draw", draws.iter().map(|(_, target)| *target).collect())
            }
        };
        n.set("kind", Scalar::String(kind))?;
        let mut a = n.array("next")?;
        push_targets(&mut a, &next)
    }

    fn read(n: TableRef<'_>) -> Result<Node, Error> {
        let next: Vec<Target> = list(n, "next", read_target)?;
        fn paired<T>(labels: Vec<T>, next: &[Target]) -> Result<Vec<(T, Target)>, Error> {
            if labels.len() != next.len() {
                return Err(Error::Malformed("next"));
            }
            Ok(labels.into_iter().zip(next.iter().copied()).collect())
        }
        let kind = match str_field(n, "kind")? {
            "say" => NodeKind::Say {
                step: u32_field(n, "step")?,
                render: opt_u32_field(n, "render")?,
                next: match next.as_slice() {
                    [next] => *next,
                    _ => return Err(Error::Malformed("next")),
                },
            },
            "choice" => NodeKind::Choice {
                step: u32_field(n, "step")?,
                render: opt_u32_field(n, "render")?,
                arms: paired(strings(n, "arms")?, &next)?,
            },
            "call" => NodeKind::Call {
                callee: u32_field(n, "callee")?,
                exits: paired(ids(n, "exits")?, &next)?,
            },
            "draw" => NodeKind::Draw {
                draws: paired(ids(n, "draws")?, &next)?,
            },
            _ => return Err(Error::Malformed("kind")),
        };
        Ok(Node {
            block: u32_field(n, "block")?,
            frame: u32_field(n, "frame")?,
            globals: u32_field(n, "globals")?,
            kind,
        })
    }
}

/// A target as a cell: node `n` is `n`, exit `e` is `-1 - e`, a dead end is `Null`.
fn target(target: Target) -> Scalar<'static> {
    match target {
        Target::Node(node) => Scalar::I64(i64::from(node)),
        Target::Exit(exit) => Scalar::I64((-1_i64).saturating_sub(i64::from(exit))),
        Target::Dead => Scalar::Null,
    }
}

fn push_targets(a: &mut ArrayMut<'_>, targets: &[Target]) -> Result<(), Error> {
    for t in targets {
        a.push(target(*t))?;
    }
    Ok(())
}

fn read_target(v: ValueRef<'_>) -> Result<Target, Error> {
    if v == ValueRef::Null {
        return Ok(Target::Dead);
    }
    let raw = int(v).ok_or(Error::Malformed("target"))?;
    let index = |n: i128| u32::try_from(n).map_err(|_| Error::Malformed("target"));
    if raw >= 0 {
        Ok(Target::Node(index(raw)?))
    } else {
        Ok(Target::Exit(index((-1_i128).saturating_sub(raw))?))
    }
}
