//! The draw list the engine hands to a renderer. All coordinates are design pixels
//! (`System::resolution`), origin top left, y down. Colors are premultiplied linear-light.

use std::rc::Rc;

use kagari_vm::AssetId;

/// One frame.
#[derive(Clone, Debug)]
pub struct Frame {
    pub size: (u32, u32),
    pub nodes: Vec<Node>,
    /// Whether the renderer keeps this frame as the still that `Node::Still` draws.
    pub capture_still: bool,
}

#[derive(Clone, Debug)]
pub enum Node {
    /// Children drawn into an isolated layer, then composited source-over with `opacity`.
    /// The renderer may skip isolation when there is at most one child.
    Group {
        transform: Affine,
        opacity: f32,
        clip: Option<Rect>,
        children: Vec<Node>,
    },
    /// Children drawn into an isolated layer, then run through a colour shader
    /// (`ColorShader`) in screen space.
    Filter {
        effect: ShaderEffect,
        children: Vec<Node>,
    },
    /// Children drawn into an isolated layer, then composited over the backdrop by a mixing
    /// shader (`MixShader`), which returns the final pixel. `opacity` scales the source.
    Mix {
        effect: ShaderEffect,
        opacity: f32,
        children: Vec<Node>,
    },
    /// Children drawn into an isolated layer whose alpha is multiplied by a mask shader
    /// (`MaskShader`) evaluated over `rect` (in the current transform).
    Masked {
        effect: ShaderEffect,
        rect: Rect,
        children: Vec<Node>,
    },
    /// GPU motion: `stages` (at most one transitional, then at most one local) compute a pose
    /// from `pose`; children are displaced by `P·M(result)·M(pose)⁻¹·P⁻¹` (P = the current
    /// transform, M(x) = T(position)·R(rotation)·S(scale)).
    Motion {
        stages: Vec<MotionStage>,
        pose: Pose,
        children: Vec<Node>,
    },
    /// A texture shader (`TextureShader`) over `rect`, optionally multiplied by a mask shader.
    Paint {
        rect: Rect,
        texture: ShaderEffect,
        mask: Option<ShaderEffect>,
        flip_x: bool,
        flip_y: bool,
    },
    Image {
        rect: Rect,
        image: AssetId,
        uv: Rect,
        flip_x: bool,
        flip_y: bool,
        tint: [f32; 4],
    },
    Fill {
        rect: Rect,
        color: [f32; 4],
    },
    /// `insets` = [top, right, bottom, left] (CSS order, as std `Insets`): source px in the
    /// image, design px in `rect`.
    NineSlice {
        rect: Rect,
        image: AssetId,
        insets: [f32; 4],
        tint: [f32; 4],
    },
    Text {
        origin: [f32; 2],
        layout: Rc<kagari_text::Layout>,
        reveal: Reveal,
        opacity: f32,
    },
    Transition {
        bounds: Rect,
        kind: TransitionDraw,
        progress: f32,
        from: Vec<Node>,
        to: Vec<Node>,
    },
    /// The still image captured by the last frame whose `capture_still` was true.
    Still {
        rect: Rect,
    },
    /// A save thumbnail stored as RGBA8 by the engine.
    Thumbnail {
        rect: Rect,
        slot: ThumbKey,
        generation: u64,
    },
}

/// `[a b c d e f]`: x' = a·x + c·y + e, y' = b·x + d·y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [f32; 6]);

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A user shader invocation: the shader file, its packed `Args` (WGSL storage layout) and its
/// effect time in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderEffect {
    pub shader: AssetId,
    pub args: Rc<[u8]>,
    pub time: f32,
}

/// A motion pose: anchor position in design px, scale, rotation in radians (clockwise, y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub position: [f32; 2],
    pub scale: f32,
    pub rotation: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MotionStage {
    Local {
        effect: ShaderEffect,
    },
    Transitional {
        effect: ShaderEffect,
        from: Pose,
        to: Pose,
        progress: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum TransitionDraw {
    Crossfade,
    Rule {
        mask: AssetId,
        softness: u8,
        reverse: bool,
    },
    Wipe {
        dir: Dir,
    },
    Blinds {
        dir: Dir,
        count: u32,
    },
    /// `hold_fraction` = hold / (duration + hold).
    ThroughColor {
        color: [f32; 4],
        hold_fraction: f32,
    },
    /// A pixel switches once the mask shader's value ×255 is at most the sweeping threshold
    /// (the rule math of `Rule`).
    Masked {
        mask: ShaderEffect,
        softness: u8,
        reverse: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reveal {
    All,
    Units(u32),
    UnitsWithFade { full: u32, partial: f32 },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ThumbKey {
    Manual(u32),
    Quick,
    Auto(u32),
}
