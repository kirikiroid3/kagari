//! The headless input script: one command per line.

use std::fmt;

use kagari_engine::Key;

use crate::exit::{Failure, SCRIPT};

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Advance the clock by this many ms, in 16 ms ticks.
    Wait(u64),
    /// Press, then release.
    Key(Key),
    Down(Key),
    Up(Key),
    /// Move there, then press and release the left button.
    Click(f32, f32),
    Move(f32, f32),
    Wheel(f32),
    /// Write `OUT/{name}.png`.
    Capture(String),
    ExpectCrash,
    AssertNoCrash,
}

/// A script line that does not parse, or a command that fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    pub line: u32,
    pub message: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "script:{}: {}", self.line, self.message)
    }
}

impl std::error::Error for ScriptError {}

impl From<ScriptError> for Failure {
    fn from(e: ScriptError) -> Failure {
        Failure::new(SCRIPT, e.to_string())
    }
}

/// Parses a whole script; lines are 1-based, and blank and `#` lines are skipped.
pub fn parse(text: &str) -> Result<Vec<(u32, Command)>, ScriptError> {
    let mut out = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let tokens: Vec<&str> = trimmed.split_ascii_whitespace().collect();
        let err = |message: String| ScriptError { line, message };
        let (cmd, args) = match tokens.split_first() {
            Some((cmd, args)) => (*cmd, args),
            None => continue,
        };
        let arity = |n: usize| {
            if args.len() == n {
                Ok(())
            } else {
                Err(err(format!("`{cmd}` takes {n} argument(s)")))
            }
        };
        let arg = |i: usize| args.get(i).copied().unwrap_or("");
        let command = match cmd {
            "wait" => {
                arity(1)?;
                let ms = arg(0)
                    .parse::<u64>()
                    .map_err(|_| err(format!("`{}` is not a valid duration in ms", arg(0))))?;
                Command::Wait(ms)
            }
            "key" | "down" | "up" => {
                arity(1)?;
                let name = arg(0);
                let key = Key::ALL
                    .iter()
                    .copied()
                    .find(|k| format!("{k:?}") == name)
                    .ok_or_else(|| err(format!("unknown key `{name}`")))?;
                match cmd {
                    "key" => Command::Key(key),
                    "down" => Command::Down(key),
                    _ => Command::Up(key),
                }
            }
            "click" | "move" => {
                arity(2)?;
                let x = number(arg(0), "coordinate").map_err(err)?;
                let y = number(arg(1), "coordinate").map_err(err)?;
                if cmd == "click" {
                    Command::Click(x, y)
                } else {
                    Command::Move(x, y)
                }
            }
            "wheel" => {
                arity(1)?;
                Command::Wheel(number(arg(0), "wheel delta").map_err(err)?)
            }
            "capture" => {
                arity(1)?;
                let name = arg(0);
                let valid = (1..=64).contains(&name.len())
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
                if !valid {
                    return Err(err(format!("bad capture name `{name}`")));
                }
                Command::Capture(name.to_owned())
            }
            "expect-crash" => {
                arity(0)?;
                Command::ExpectCrash
            }
            "assert-no-crash" => {
                arity(0)?;
                Command::AssertNoCrash
            }
            _ => return Err(err(format!("unknown command `{cmd}`"))),
        };
        out.push((line, command));
    }
    Ok(out)
}

/// A finite `f32`, else `` `{arg}` is not a valid {what}``.
fn number(arg: &str, what: &str) -> Result<f32, String> {
    arg.parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("`{arg}` is not a valid {what}"))
}
