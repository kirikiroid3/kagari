#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use super::THREAD;
use super::answers::stopped_event;
use super::breakpoints::breakpoint;
use crate::client::LineBreakpoints;
use crate::protocol::{InstructionPoint, StoppedBody};

#[test]
fn an_unverified_line_says_why() {
    let at = LineBreakpoints {
        reason: Some("no code on this line".to_owned()),
        ..LineBreakpoints::default()
    };
    let bp = breakpoint(3, 7, Path::new("/game/src/main.kis"), &at);
    assert_eq!(bp["verified"], false);
    assert_eq!(bp["line"], 7);
    assert_eq!(bp["message"], "no code on this line");

    let at = LineBreakpoints {
        points: vec![InstructionPoint {
            function: "crate::Start".to_owned(),
            word: 4,
        }],
        ..LineBreakpoints::default()
    };
    let bp = breakpoint(4, 8, Path::new("/game/src/main.kis"), &at);
    assert_eq!(bp["verified"], true);
    assert!(bp.get("message").is_none());
}

#[test]
fn stops_keep_their_reason_and_explain_choices() {
    let body = stopped_event(&StoppedBody {
        reason: "choice".to_owned(),
        step: Some("crate::Start#2".to_owned()),
        ..StoppedBody::default()
    });
    assert_eq!(body["reason"], "choice");
    assert_eq!(body["threadId"], THREAD);
    assert!(body["description"].as_str().unwrap().contains("choice"));
    let plain = stopped_event(&StoppedBody {
        reason: "pause".to_owned(),
        ..StoppedBody::default()
    });
    assert!(plain.get("description").is_none() && plain.get("text").is_none());
}
