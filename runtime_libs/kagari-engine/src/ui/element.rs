//! The element arena: nodes with their kind, modifiers, children and layout rect, and the
//! `Element`/`Placed` host objects that name them.

use std::any::Any;
use std::rc::Rc;

use kagari_vm::{AssetId, HostObject, Text, Trap, Value};

use super::style::{ButtonStyleSpec, LookState, Paint, SliderStyleSpec};
use super::text::TextStyleSpec;
use crate::frame::{Rect, ThumbKey};

/// An element of the arena; the generation tells a freed slot's ids from the live one's.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct ElemId {
    index: u32,
    generation: u32,
}

/// `std::ui::Align`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Align {
    Start,
    Center,
    End,
    Stretch,
    SpaceBetween,
}

impl Align {
    pub fn from_index(index: u32) -> Option<Align> {
        [
            Align::Start,
            Align::Center,
            Align::End,
            Align::Stretch,
            Align::SpaceBetween,
        ]
        .get(usize::try_from(index).ok()?)
        .copied()
    }
}

/// `std::ui::Fit`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Fit {
    Stretch,
    Contain,
    Cover,
    Natural,
}

impl Fit {
    pub fn from_index(index: u32) -> Option<Fit> {
        [Fit::Stretch, Fit::Contain, Fit::Cover, Fit::Natural]
            .get(usize::try_from(index).ok()?)
            .copied()
    }
}

/// The builder modifiers of an element (`Element::width`, …).
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Mods {
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// [top, right, bottom, left]; negative insets count as 0.
    pub padding: [f32; 4],
    pub gap: u32,
    /// (main, cross).
    pub align: Option<(Align, Align)>,
    pub grow: Option<u32>,
    pub background: Paint,
    pub opacity: f32,
    pub focus_order: Option<u32>,
}

impl Default for Mods {
    fn default() -> Mods {
        Mods {
            width: None,
            height: None,
            padding: [0.0; 4],
            gap: 0,
            align: None,
            grow: None,
            background: Paint::None,
            opacity: 1.0,
            focus_order: None,
        }
    }
}

/// An `Element` value: an arena node plus the modifiers its builders applied.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct ElementObj {
    pub id: ElemId,
    pub mods: Mods,
}

/// A `Placed` value: an element with its place in a layer.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct PlacedObj {
    pub elem: ElementObj,
    pub place: Place,
}

/// Where a layer child goes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Place {
    /// The child's anchor `(ax, ay)` at `(x, y)` in the layer's inner box.
    At {
        x: f32,
        y: f32,
        ax: f32,
        ay: f32,
    },
    Cover,
}

macro_rules! host_object {
    ($ty:ty, $name:literal) => {
        impl HostObject for $ty {
            fn clone_object(&self) -> Rc<dyn HostObject> {
                Rc::new(self.clone())
            }

            fn type_name(&self) -> &'static str {
                $name
            }

            fn eq_object(&self, other: &dyn HostObject) -> Option<bool> {
                Some(
                    other
                        .as_any()
                        .downcast_ref::<$ty>()
                        .is_some_and(|other| self == other),
                )
            }

            fn as_any(&self) -> &dyn Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
        }
    };
}

host_object!(ElementObj, "std::ui::Element");
host_object!(PlacedObj, "std::ui::Placed");

/// What an element is.
#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Column,
    Row,
    Layer {
        places: Vec<Option<Place>>,
    },
    Each,
    Show,
    Scroll {
        offset: f32,
        content: f32,
    },
    Label {
        text: Text,
    },
    Text {
        style: Option<TextStyleSpec>,
        content: Option<Text>,
    },
    Image {
        fit: Fit,
        source: Option<AssetId>,
    },
    Spacer,
    Thumbnail {
        slot: ThumbKey,
    },
    GameScreen,
    /// The handle is not kept: at most one game runs.
    Message {
        style: Value,
    },
    Button {
        on_click: Option<Value>,
        disabled: bool,
        style: Option<Rc<ButtonStyleSpec>>,
    },
    Toggle {
        value: bool,
        on_change: Option<Value>,
        disabled: bool,
        style: Option<Rc<ButtonStyleSpec>>,
    },
    Slider {
        value: i128,
        lo: i128,
        hi: i128,
        step: u32,
        on_change: Option<Value>,
        disabled: bool,
        style: Option<Rc<SliderStyleSpec>>,
    },
    Select {
        /// Per option: its value and its label element (not laid out as a child).
        options: Vec<Option<(Value, ElemId)>>,
        value: Option<Value>,
        on_change: Option<Value>,
        disabled: bool,
        style: Option<Rc<ButtonStyleSpec>>,
    },
}

impl Kind {
    /// Each and show: their children are spliced into a row or column parent.
    pub fn is_fragment(&self) -> bool {
        matches!(self, Kind::Each | Kind::Show)
    }

    /// A focusable control that is not disabled.
    pub fn is_enabled_control(&self) -> bool {
        match self {
            Kind::Button { disabled, .. }
            | Kind::Toggle { disabled, .. }
            | Kind::Slider { disabled, .. }
            | Kind::Select { disabled, .. } => !disabled,
            _ => false,
        }
    }
}

/// One element.
#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub kind: Kind,
    pub mods: Mods,
    pub children: Vec<Option<ElemId>>,
    /// The parent and the slot this element fills.
    pub parent: Option<(ElemId, u32)>,
    /// The border box, after layout.
    pub rect: Rect,
    pub look: LookState,
}

struct Slot {
    generation: u32,
    node: Option<Node>,
}

/// Every element of every mounted mode.
#[derive(Default)]
pub(crate) struct Arena {
    slots: Vec<Slot>,
    free: Vec<u32>,
}

pub(crate) const ZERO_RECT: Rect = Rect {
    x: 0.0,
    y: 0.0,
    w: 0.0,
    h: 0.0,
};

fn stale() -> Trap {
    Trap::host("element used after its mode was closed")
}

impl Arena {
    /// A new element with `slots` empty child slots.
    pub fn alloc(&mut self, kind: Kind, slots: usize) -> ElemId {
        let node = Node {
            kind,
            mods: Mods::default(),
            children: vec![None; slots],
            parent: None,
            rect: ZERO_RECT,
            look: LookState::default(),
        };
        if let Some(index) = self.free.pop()
            && let Some(slot) = usize::try_from(index)
                .ok()
                .and_then(|i| self.slots.get_mut(i))
        {
            slot.node = Some(node);
            return ElemId {
                index,
                generation: slot.generation,
            };
        }
        let index = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
        self.slots.push(Slot {
            generation: 0,
            node: Some(node),
        });
        ElemId {
            index,
            generation: 0,
        }
    }

    pub fn get(&self, id: ElemId) -> Option<&Node> {
        let slot = self.slots.get(usize::try_from(id.index).ok()?)?;
        if slot.generation == id.generation {
            slot.node.as_ref()
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id: ElemId) -> Option<&mut Node> {
        let slot = self.slots.get_mut(usize::try_from(id.index).ok()?)?;
        if slot.generation == id.generation {
            slot.node.as_mut()
        } else {
            None
        }
    }

    pub fn contains(&self, id: ElemId) -> bool {
        self.get(id).is_some()
    }

    /// Frees an element; its id goes stale.
    pub fn free(&mut self, id: ElemId) {
        let Some(slot) = usize::try_from(id.index)
            .ok()
            .and_then(|i| self.slots.get_mut(i))
        else {
            return;
        };
        if slot.generation != id.generation || slot.node.is_none() {
            return;
        }
        // Children keep their stale `parent`: `occupant` sees the freed parent and lets them move.
        slot.node = None;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.index);
    }

    /// Whether `id` is an `each`/`show` whose children are flex items of a row or column
    /// (directly or through other spliced fragments); its own modifiers are then ignored.
    pub fn is_spliced(&self, id: ElemId) -> bool {
        let mut current = id;
        loop {
            let Some(node) = self.get(current) else {
                return false;
            };
            if !node.kind.is_fragment() {
                return false;
            }
            let Some((parent, _)) = node.parent else {
                return false;
            };
            match self.get(parent).map(|p| &p.kind) {
                Some(Kind::Column | Kind::Row) => return true,
                Some(kind) if kind.is_fragment() => current = parent,
                _ => return false,
            }
        }
    }

    /// The live children of `id`, in slot order.
    pub fn children(&self, id: ElemId) -> Vec<ElemId> {
        self.get(id).map_or_else(Vec::new, |node| {
            node.children
                .iter()
                .flatten()
                .copied()
                .filter(|c| self.contains(*c))
                .collect()
        })
    }

    /// The element filling slot `index` of a live `parent`: a child slot, or a select's option
    /// label (which is not laid out, so it is no child).
    fn occupant(&self, (parent, index): (ElemId, u32)) -> Option<ElemId> {
        let slot = usize::try_from(index).ok()?;
        let node = self.get(parent)?;
        match &node.kind {
            Kind::Select { options, .. } => options.get(slot)?.as_ref().map(|(_, label)| *label),
            _ => node.children.get(slot).copied().flatten(),
        }
    }

    /// Checks that `obj` is live and shown nowhere but at `place`; an element whose old
    /// parent was freed or replaced it is free to move.
    fn check_movable(&self, obj: &ElementObj, place: (ElemId, u32)) -> Result<(), Trap> {
        let node = self.get(obj.id).ok_or_else(stale)?;
        match node.parent {
            Some(old) if old != place && self.occupant(old) == Some(obj.id) => {
                Err(Trap::host("an element can be shown in one place only"))
            }
            _ => Ok(()),
        }
    }

    /// Puts `obj` into slot `index` of `parent`, replacing the old occupant. An element shown
    /// in another live slot is an error; one whose old parent was freed is free to move.
    pub fn attach(&mut self, parent: ElemId, index: u32, obj: &ElementObj) -> Result<(), Trap> {
        self.check_movable(obj, (parent, index))?;
        let slot = usize::try_from(index).map_err(|_| stale())?;
        let old = self
            .get(parent)
            .ok_or_else(stale)?
            .children
            .get(slot)
            .copied()
            .flatten();
        if let Some(old) = old
            && old != obj.id
            && let Some(old) = self.get_mut(old)
        {
            old.parent = None;
        }
        if let Some(parent_node) = self.get_mut(parent) {
            if parent_node.children.len() <= slot {
                parent_node.children.resize(slot.saturating_add(1), None);
            }
            if let Some(child) = parent_node.children.get_mut(slot) {
                *child = Some(obj.id);
            }
        }
        if let Some(node) = self.get_mut(obj.id) {
            node.parent = Some((parent, index));
            node.mods = obj.mods.clone();
        }
        Ok(())
    }

    /// Makes `obj` the label of option `index` of the select `select`, with the option value
    /// `value`, under the same rules as `attach`.
    pub fn attach_option(
        &mut self,
        select: ElemId,
        index: u32,
        value: Value,
        obj: &ElementObj,
    ) -> Result<(), Trap> {
        self.check_movable(obj, (select, index))?;
        let old = self.occupant((select, index));
        if let Some(old) = old
            && old != obj.id
            && let Some(node) = self.get_mut(old)
            && node.parent == Some((select, index))
        {
            node.parent = None;
        }
        if let Some(Kind::Select { options, .. }) = self.get_mut(select).map(|n| &mut n.kind)
            && let Some(slot) = usize::try_from(index).ok().and_then(|i| options.get_mut(i))
        {
            *slot = Some((value, obj.id));
        }
        if let Some(node) = self.get_mut(obj.id) {
            node.parent = Some((select, index));
            node.mods = obj.mods.clone();
        }
        Ok(())
    }

    /// Empties every child slot of `parent`.
    pub fn clear_children(&mut self, parent: ElemId) {
        let slots = self.get(parent).map_or(0, |p| p.children.len());
        for index in 0..slots {
            if let Ok(index) = u32::try_from(index) {
                self.detach(parent, index);
            }
        }
        if let Some(node) = self.get_mut(parent) {
            node.children.clear();
        }
    }

    /// Clears slot `index` of `parent`.
    pub fn detach(&mut self, parent: ElemId, index: u32) {
        let Ok(slot) = usize::try_from(index) else {
            return;
        };
        let old = self
            .get_mut(parent)
            .and_then(|p| p.children.get_mut(slot))
            .and_then(Option::take);
        if let Some(old) = old
            && let Some(node) = self.get_mut(old)
            && node.parent == Some((parent, index))
        {
            node.parent = None;
        }
    }
}
