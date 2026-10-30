#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `kagari debug`: the query engine's REPL on a headless game.

mod common;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use common::{font_home, game, init_project, stderr, stdout};

/// Nested scene calls, a choice guarded by a story variable and a draw.
const STORY: &str = r#"
static mut TRUST: Int<0, 9> = 0;

enum Pick { Bold, Shy }
enum Ride { Wheel, Coaster }
enum Coin { Heads, Tails }

#[scene]
#[entry]
pub pushdown fn Start() {
    s!(who = "Mio", t!["Morning."]);
    s!(t!["Shall we go?"]);
    match choice!{ id = "mood", t!["Be bold"] => Pick::Bold, t!["Be shy"] => Pick::Shy } {
        Pick::Bold => Meet(2),
        Pick::Shy => Meet(0),
    }
    s!(t!["At the gate."]);
    AmusementPark();
    s!(t!["Good night."]);
}

#[scene]
pushdown fn Meet(gain: Int<0, 9>) {
    TRUST = gain;
    s!(t!["Hello there."]);
    Greet();
}

#[scene]
pushdown fn Greet() {
    s!(t!["Nice to meet you."]);
}

#[scene]
pushdown fn AmusementPark() {
    s!(t!["The park."]);
    let ride = choice!{
        id = "ride",
        t!["Wheel"] => Ride::Wheel,
        t!["Coaster"] if TRUST >= 2 => Ride::Coaster,
    };
    match rng_choose::<Coin>() {
        Coin::Heads => s!(t!["Lucky!"]),
        Coin::Tails => s!(t!["Oh well."]),
    }
    match ride {
        Ride::Wheel => s!(t!["Up high."]),
        Ride::Coaster => s!(t!["So fast!"]),
    }
}
"#;

/// Runs `kagari debug` in `dir` with `script` on its standard input.
fn debug(dir: &Path, args: &[&str], script: &str) -> Output {
    let home = font_home();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kagari"))
        .arg("debug")
        .args(args)
        .current_dir(dir)
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &home)
        .env("XDG_DATA_DIRS", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run kagari debug");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn story_project() -> tempfile::TempDir {
    let tmp = init_project();
    std::fs::write(game(&tmp).join("src/scenario/mod.kix"), STORY).unwrap();
    tmp
}

#[test]
fn the_repl_moves_through_the_story_and_navigates() {
    let tmp = story_project();
    let dir = game(&tmp);
    let output = debug(
        &dir,
        &[],
        "ls -t 1\n\
         nav crate::scenario::AmusementPark#5\n\
         push crate::scenario::Pick::Bold Coaster\n\
         ls\n\
         goto crate::scenario::AmusementPark#5\n\
         stack\n\
         pop\n\
         detail\n\
         nav crate::scenario::Start#0\n\
         seed 7\n\
         quit\n",
    );
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(0), "{out}\n{}", stderr(&output));
    let expect = [
        // The debug build ran first.
        "wrote target/v0/game.kagari-cache",
        "stopped (entry) at crate::scenario::Start#0",
        "the story graph is complete",
        // ls -t 1: the next choice.
        "crate::scenario::Start#2  choice `mood`",
        // nav: the path, its draw and a seed.
        "reachable:",
        "push crate::scenario::Pick::Bold crate::scenario::Ride::Coaster",
        "needs crate::scenario::AmusementPark to draw",
        // push follows the path; ls shows the target ahead.
        "selected crate::scenario::Start/mood/crate::scenario::Pick::Bold",
        "selected crate::scenario::AmusementPark/ride/crate::scenario::Ride::Coaster",
        "crate::scenario::AmusementPark#5  So fast!",
        // goto, then the stack names the scenes with the source position.
        "#0 crate::scenario::AmusementPark() at crate::scenario::AmusementPark#5  scenario/mod.kix:",
        "#1 crate::scenario::Start() called  scenario/mod.kix:",
        // pop: back at the ride choice, where the guarded arm is shown.
        "at crate::scenario::AmusementPark#1",
        "shown  crate::scenario::Ride::Coaster",
        // Already passed.
        "unreachable",
        "seed 7",
    ];
    for line in expect {
        assert!(out.contains(line), "missing `{line}` in\n{out}");
    }
}

#[test]
fn breakpoints_by_line_and_by_step() {
    let tmp = story_project();
    let dir = game(&tmp);
    let line = STORY
        .lines()
        .position(|l| l.contains("Nice to meet you."))
        .unwrap()
        + 1;
    let output = debug(
        &dir,
        &[],
        &format!(
            "break scenario/mod.kix:{line} crate::scenario::Start#3\n\
             break scenario/mod.kix:1\n\
             push Shy\n\
             continue\n\
             continue\n\
             quit\n"
        ),
    );
    let out = stdout(&output);
    assert_eq!(output.status.code(), Some(0), "{out}\n{}", stderr(&output));
    let expect = [
        "breakpoint 0: scenario/mod.kix",
        "(crate::scenario::Greet#0)",
        "breakpoint 1: crate::scenario::Start#3 (a step)",
        "error: scenario/mod.kix:1: no code on this line",
        "stopped (breakpoint) at crate::scenario::Greet#0",
        "stopped (breakpoint) at crate::scenario::Start#3",
    ];
    for line in expect {
        assert!(out.contains(line), "missing `{line}` in\n{out}");
    }
}
