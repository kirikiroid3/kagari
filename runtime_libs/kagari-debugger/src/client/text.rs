//! The query engine's commands as text: what `kagari debug` reads and what a DAP client's
//! `evaluate` sends, and their results as text.

use serde_json::{Value, json};

use super::locate::Locator;
use crate::protocol::{
    Detail, InitializeBody, LsBody, NavBody, PathItem, Position, PushBody, ScopesBody, SeedBody,
    StackBody, StoppedBody, SwapBody, Var, command,
};

/// The `initialize` answer as a line for the user.
pub fn describe(init: &InitializeBody) -> String {
    let mode = if init.live {
        "a running game"
    } else {
        "the query engine"
    };
    let graph = match &init.artifact {
        None => "no debug artifact: `nav` and local names are unavailable".to_owned(),
        Some(a) if !a.graph => "no story graph: `nav` is unavailable".to_owned(),
        Some(a) if a.complete => "the story graph is complete".to_owned(),
        Some(a) => {
            let codes: Vec<&str> = a.incomplete.iter().map(|i| i.code.as_str()).collect();
            format!("the story graph is incomplete ({})", codes.join(", "))
        }
    };
    format!("debugging {mode}; {graph}. Type `help` for the commands.")
}

/// A command line, parsed.
#[derive(Clone, Debug, PartialEq)]
pub enum Parsed {
    /// A protocol request.
    Request {
        command: &'static str,
        args: Value,
    },
    /// `break`: step IDs or `file:line`s.
    Break(Vec<String>),
    /// `break` without arguments: list the breakpoints.
    Breakpoints,
    /// `delete`: clear every breakpoint.
    Delete,
    /// `swap`: rebuild the debug profile and hot swap the new game in (`kagari/swap`).
    Swap,
    Help,
    Quit,
    /// An empty line.
    Nothing,
}

pub const HELP: &str = "\
Story commands (positions are step IDs such as crate::Start#3):
  stack                      the call stack: scenes, arguments, steps or call sites
  ls [-h N] [-t N]           the lines from here to the next choice (--head, --tail)
  goto <step>                jump to a step between the last choice and the next
  push <arm>...              from the next choice on, select these arms in turn
  pop                        back to the last choice, before its arm was selected
  detail [<step>]            everything about a step (default: the current one)
  nav <step> [--seeds N]     whether a step can be reached from here, and how
  seed [<n>]                 show or set the PRNG seed (replays the trace)
Running:
  continue | c               run to a breakpoint, a choice or the end
  next | n                   to the next step in this scene
  step | s                   to the next step, into scene calls
  out                        to the step after this scene returns
  pause                      stop (a running game)
  vars [<frame>]             the variables of a frame (0: the innermost)
  break <step|file:line>...  add breakpoints (`break` alone lists them)
  delete                     remove every breakpoint
  swap                       rebuild the game and hot swap it in, then set the breakpoints again
  help, quit";

/// Parses a command line.
pub fn parse(line: &str) -> Result<Parsed, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let Some((&name, rest)) = words.split_first() else {
        return Ok(Parsed::Nothing);
    };
    let request = |command: &'static str, args: Value| Ok(Parsed::Request { command, args });
    let one = |what: &str| -> Result<String, String> {
        match rest {
            [only] => Ok((*only).to_owned()),
            _ => Err(format!("`{name}` takes {what}")),
        }
    };
    match name {
        "stack" | "bt" | "backtrace" => request(command::STACK, Value::Null),
        "ls" => {
            let mut head = None;
            let mut tail = None;
            let mut words = rest.iter();
            while let Some(flag) = words.next() {
                let n: u32 = words
                    .next()
                    .and_then(|n| n.parse().ok())
                    .ok_or_else(|| format!("`{flag}` takes a number"))?;
                match *flag {
                    "-h" | "--head" => head = Some(n),
                    "-t" | "--tail" => tail = Some(n),
                    other => return Err(format!("`ls` has no option `{other}`")),
                }
            }
            request(command::LS, json!({ "head": head, "tail": tail }))
        }
        "goto" => request(command::GOTO, json!({ "step": one("a step")? })),
        "push" if rest.is_empty() => Err("`push` takes the arms to select".to_owned()),
        "push" => request(command::PUSH, json!({ "arms": rest })),
        "pop" => request(command::POP, Value::Null),
        "detail" => match rest {
            [] => request(command::DETAIL, Value::Null),
            [step] => request(command::DETAIL, json!({ "step": step })),
            _ => Err("`detail` takes at most a step".to_owned()),
        },
        "nav" => match rest {
            [step] => request(command::NAV, json!({ "step": step })),
            [step, "--seeds", n] | ["--seeds", n, step] => {
                let n: u32 = n.parse().map_err(|_| "`--seeds` takes a number")?;
                request(command::NAV, json!({ "step": step, "seeds": n }))
            }
            _ => Err("`nav` takes a step (and `--seeds N`)".to_owned()),
        },
        "seed" => match rest {
            [] => request(command::SEED, Value::Null),
            [n] => {
                let n: u64 = n.parse().map_err(|_| "`seed` takes a number")?;
                request(command::SEED, json!({ "seed": n }))
            }
            _ => Err("`seed` takes at most a number".to_owned()),
        },
        "continue" | "c" => request(command::CONTINUE, Value::Null),
        "next" | "n" => request(command::NEXT, Value::Null),
        "step" | "s" => request(command::STEP_IN, Value::Null),
        "out" | "finish" => request(command::STEP_OUT, Value::Null),
        "pause" => request(command::PAUSE, Value::Null),
        "vars" | "v" => match rest {
            [] => request(command::SCOPES, json!({ "frame": 0 })),
            [n] => {
                let n: usize = n.parse().map_err(|_| "`vars` takes a frame number")?;
                request(command::SCOPES, json!({ "frame": n }))
            }
            _ => Err("`vars` takes at most a frame number".to_owned()),
        },
        "break" | "b" if rest.is_empty() => Ok(Parsed::Breakpoints),
        "break" | "b" => Ok(Parsed::Break(
            rest.iter().map(|s| (*s).to_owned()).collect(),
        )),
        "delete" => Ok(Parsed::Delete),
        "swap" if rest.is_empty() => Ok(Parsed::Swap),
        "swap" => Err("`swap` takes no arguments".to_owned()),
        "help" | "h" | "?" => Ok(Parsed::Help),
        "quit" | "q" | "exit" => Ok(Parsed::Quit),
        other => Err(format!("unknown command `{other}` (try `help`)")),
    }
}

/// The result of `command` as text.
pub fn format(command: &str, body: &Value, loc: &Locator) -> String {
    fn de<T: serde::de::DeserializeOwned + Default>(body: &Value) -> T {
        serde_json::from_value(body.clone()).unwrap_or_default()
    }
    match command {
        command::STACK | command::STACK_TRACE => stack(&de(body), loc),
        command::SCOPES => scopes(&de(body)),
        command::LS => ls(&de(body)),
        command::GOTO | command::POP => position(&de(body)),
        command::PUSH => {
            let push: PushBody = de(body);
            let mut out = String::new();
            for choice in &push.selected {
                out.push_str(&format!("selected {choice}\n"));
            }
            out.push_str(&position(&push.position));
            out
        }
        command::DETAIL => detail(&de(body)),
        command::NAV => nav(&de(body)),
        command::SEED => {
            let seed: SeedBody = de(body);
            format!("seed {}", seed.seed)
        }
        command::SWAP => de::<SwapBody>(body).summary(),
        _ => String::new(),
    }
}

pub fn position(p: &Position) -> String {
    format!(
        "at {} ({} choices, {} lines after the last, seed {})",
        p.step.as_deref().unwrap_or("no step"),
        p.choices,
        p.steps,
        p.seed
    )
}

/// A `stopped` event as text.
pub fn stopped(s: &StoppedBody) -> String {
    let at = match (&s.step, s.in_code) {
        (Some(step), _) => step.clone(),
        (None, true) => "code".to_owned(),
        (None, false) => "no step".to_owned(),
    };
    let mut out = format!("stopped ({}) at {at}", s.reason);
    if let Some(description) = &s.description {
        out.push_str(&format!(": {description}"));
    }
    out
}

fn stack(body: &StackBody, loc: &Locator) -> String {
    let mut out = Vec::new();
    for (i, frame) in body.frames.iter().enumerate() {
        let args: Vec<String> = frame
            .args
            .iter()
            .map(|a| format!("{} = {}", a.name, a.value))
            .collect();
        let at = match &frame.step {
            Some(step) => format!("at {step}"),
            None if i == 0 => "in code".to_owned(),
            None => "called".to_owned(),
        };
        out.push(format!(
            "#{i} {}({}) {at}  {}",
            frame.function,
            args.join(", "),
            loc.describe(frame.file.as_deref(), frame.offset)
        ));
    }
    out.join("\n")
}

fn scopes(body: &ScopesBody) -> String {
    let mut out = String::new();
    for scope in &body.scopes {
        out.push_str(&format!("{}:\n", scope.name));
        if scope.vars.is_empty() {
            out.push_str("  (none)\n");
        }
        for var in &scope.vars {
            write_var(var, 1, &mut out);
        }
    }
    out.trim_end().to_owned()
}

fn write_var(var: &Var, depth: usize, out: &mut String) {
    out.push_str(&format!(
        "{:indent$}{} = {}\n",
        "",
        var.name,
        var.value,
        indent = depth.saturating_mul(2)
    ));
    if depth < 4 {
        for child in &var.children {
            write_var(child, depth.saturating_add(1), out);
        }
    }
}

/// The first few words of a line.
fn words(text: &str) -> String {
    const LIMIT: usize = 40;
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= LIMIT {
        return flat;
    }
    let cut: String = flat.chars().take(LIMIT).collect();
    format!("{}…", cut.trim_end())
}

fn ls(body: &LsBody) -> String {
    let mut out = Vec::new();
    for line in &body.lines {
        let text = match (line.kind.as_str(), &line.who) {
            ("choice", _) => format!("choice `{}`", line.text),
            (_, Some(who)) => format!("{who}: {}", words(&line.text)),
            (_, None) => words(&line.text),
        };
        out.push(format!("{}  {text}", line.step));
    }
    if body.lines.len() < body.total {
        out.push(format!("({} of {} lines)", body.lines.len(), body.total));
    }
    match body.ends.as_str() {
        "end" => out.push("(the story ends)".to_owned()),
        "limit" => out.push("(stopped listing: too many lines)".to_owned()),
        _ => {}
    }
    out.join("\n")
}

fn detail(d: &Detail) -> String {
    let mut out = vec![format!("{} ({})", d.step, d.kind)];
    if let Some(who) = &d.who {
        out.push(format!("  who:      {who}"));
    }
    if let Some(text) = &d.text {
        out.push(format!("  text:     {text}"));
    }
    if let Some(voice) = &d.voice {
        out.push(format!("  voice:    {voice}"));
    }
    let keyframe = match d.keyframe {
        Some(true) => " (keyframe)",
        Some(false) => " (not a keyframe)",
        None => "",
    };
    out.push(format!(
        "  render:   {}{keyframe}",
        d.render.as_deref().unwrap_or("none")
    ));
    if let Some(choice) = &d.choice {
        out.push(format!("  choice:   {choice}"));
        for arm in &d.arms {
            out.push(format!(
                "    {} {}  {}",
                if arm.shown { "shown " } else { "hidden" },
                arm.arm,
                arm.text
            ));
        }
    }
    out.join("\n")
}

fn nav(n: &NavBody) -> String {
    let mut out = Vec::new();
    match n.answer.as_str() {
        "reachable" => {
            out.push(format!("reachable: {} steps", n.length));
            if n.push.is_empty() {
                out.push(
                    "  no choice on the way (`goto` it once it is in this segment, or `continue`)"
                        .to_owned(),
                );
            } else {
                out.push(format!("  push {}", n.push.join(" ")));
            }
            for item in &n.path {
                if let PathItem::Draw {
                    scene,
                    value,
                    outcomes,
                } = item
                {
                    out.push(format!(
                        "  draws: {scene} draws {value} (one of {outcomes})"
                    ));
                }
            }
            if let Some(search) = &n.seed {
                out.push(match search.found {
                    Some(seed) if seed == search.current => {
                        "  the current seed already gives a path there: `push` it".to_owned()
                    }
                    Some(seed) => format!(
                        "  seed {seed} realizes the draws (tried {} seeds; the draws come out \
                         this way about 1 in {} seeds): `seed {seed}`, then `push`",
                        search.tried, search.odds
                    ),
                    None => format!(
                        "  no seed within {} realizes the {} draws (about 1 in {} seeds would); \
                         try `nav … --seeds N` with a larger N",
                        search.limit, search.draws, search.odds
                    ),
                });
            }
        }
        other => {
            out.push(other.to_owned());
            if let Some(reason) = &n.reason {
                out.push(format!("  {reason}"));
            }
        }
    }
    out.join("\n")
}
