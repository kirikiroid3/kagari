//! The Crashed state: after a trap the game stops and only shows the trap message.

use std::rc::Rc;

use kagari_vm::Trap;

use crate::HostCtx;
use crate::frame::{Frame, Node, Rect, Reveal};

/// The stopped game's message and its layout.
pub(crate) struct Crash {
    pub message: String,
    layout: Option<Rc<kagari_text::Layout>>,
}

/// Stops the game; later traps are ignored.
pub(crate) fn crash(ctx: &mut HostCtx, trap: Trap) {
    let state = &mut *ctx.state;
    if state.system.crashed.is_some() {
        return;
    }
    log::error!("the game stopped: {trap}");
    state.pending_nav.clear();
    if let Some(game) = state.game.as_mut() {
        game.pending_input = None;
    }
    state.audio.stop_all();
    let message = trap.render(&state.program);
    let width = state.system.resolution().map_or(1280, |r| r.0) as f32;
    let layout = state.text.layout_plain(
        &state.assets,
        &message,
        18.0,
        [1.0; 4],
        Some((width - 64.0).max(1.0)),
    );
    state.system.crashed = Some(Crash { message, layout });
}

/// The frame of the Crashed state: black, with the message at (32, 32).
pub(crate) fn crash_frame(crash: &Crash, size: (u32, u32)) -> Frame {
    let mut nodes = vec![Node::Fill {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: size.0 as f32,
            h: size.1 as f32,
        },
        color: [0.0, 0.0, 0.0, 1.0],
    }];
    if let Some(layout) = &crash.layout {
        nodes.push(Node::Text {
            origin: [32.0, 32.0],
            layout: layout.clone(),
            reveal: Reveal::All,
            opacity: 1.0,
        });
    }
    Frame {
        size,
        nodes,
        capture_still: false,
    }
}
