//! The `Doc` layout IR: text plus the places a line may break, grouped so that the printer can
//! decide per group whether it fits flat on the current line.

pub(crate) mod print;
#[cfg(test)]
mod tests;

/// A layout document. See `print` for the exact semantics of each variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Doc {
    Nil,
    Text(String),
    /// A space when flat, a newline when broken.
    Line,
    /// Nothing when flat, a newline when broken.
    SoftLine,
    /// Always a newline.
    HardLine,
    /// Exactly one empty line before the next text; nothing at file start.
    BlankLine,
    Concat(Vec<Doc>),
    Indent(Box<Doc>),
    Group {
        doc: Box<Doc>,
        broken: bool,
    },
    /// Alternative layouts of the same content, tried in order.
    ConditionalGroup(Vec<Doc>),
    /// Chooses by the mode of the enclosing group.
    IfBreak {
        broken: Box<Doc>,
        flat: Box<Doc>,
    },
}

pub(crate) fn text(text: impl Into<String>) -> Doc {
    Doc::Text(text.into())
}

pub(crate) fn concat(docs: Vec<Doc>) -> Doc {
    Doc::Concat(docs)
}

pub(crate) fn group(doc: Doc) -> Doc {
    Doc::Group {
        doc: Box::new(doc),
        broken: false,
    }
}

/// A group that always prints in break mode.
pub(crate) fn broken_group(doc: Doc) -> Doc {
    Doc::Group {
        doc: Box::new(doc),
        broken: true,
    }
}

pub(crate) fn indent(doc: Doc) -> Doc {
    Doc::Indent(Box::new(doc))
}

pub(crate) fn if_break(broken: Doc, flat: Doc) -> Doc {
    Doc::IfBreak {
        broken: Box::new(broken),
        flat: Box::new(flat),
    }
}

pub(crate) fn conditional_group(states: Vec<Doc>) -> Doc {
    Doc::ConditionalGroup(states)
}

/// The forced breaks of a doc: `HardLine`, `BlankLine` and multi-line `Text`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Breaks {
    /// A forced break followed by more content within the doc.
    inner: bool,
    /// A forced break after the doc's last content (a trailing line comment).
    trailing: bool,
    /// The doc prints something (text or a line).
    content: bool,
}

impl Breaks {
    const CONTENT: Breaks = Breaks {
        inner: false,
        trailing: false,
        content: true,
    };

    /// `self` followed by `next`.
    fn then(self, next: Breaks) -> Breaks {
        Breaks {
            inner: self.inner || next.inner || (self.trailing && next.content),
            trailing: next.trailing || (self.trailing && !next.content),
            content: self.content || next.content,
        }
    }

    /// Either of two alternatives.
    fn or(self, other: Breaks) -> Breaks {
        Breaks {
            inner: self.inner || other.inner,
            trailing: self.trailing || other.trailing,
            content: self.content || other.content,
        }
    }
}

fn leaf_breaks(doc: &Doc) -> Breaks {
    match doc {
        Doc::Text(text) if text.is_empty() => Breaks::default(),
        Doc::Text(text) => Breaks {
            inner: text.contains('\n'),
            trailing: false,
            content: true,
        },
        Doc::Line | Doc::SoftLine => Breaks::CONTENT,
        Doc::HardLine | Doc::BlankLine => Breaks {
            inner: false,
            trailing: true,
            content: false,
        },
        _ => Breaks::default(),
    }
}

/// Marks every `Group` broken whose own doc (not counting `ConditionalGroup` alternatives)
/// contains a forced break followed by more content. A forced break at the very end of a group
/// (the newline after a trailing line comment) does not break it, so a trailing comment does not
/// reflow the code before it; the enclosing groups see the break and its continuation.
pub(crate) fn propagate_breaks(doc: &mut Doc) {
    propagate(doc);
}

fn propagate(doc: &mut Doc) -> Breaks {
    match doc {
        Doc::Concat(docs) => docs
            .iter_mut()
            .fold(Breaks::default(), |acc, doc| acc.then(propagate(doc))),
        Doc::Indent(doc) => propagate(doc),
        Doc::Group { doc, broken } => {
            let breaks = propagate(doc);
            if breaks.inner {
                *broken = true;
            }
            breaks
        }
        Doc::ConditionalGroup(states) => {
            for state in states {
                propagate(state);
            }
            Breaks::CONTENT
        }
        Doc::IfBreak { broken, flat } => propagate(broken).or(propagate(flat)),
        leaf => leaf_breaks(leaf),
    }
}

fn breaks(doc: &Doc) -> Breaks {
    match doc {
        Doc::Concat(docs) => docs
            .iter()
            .fold(Breaks::default(), |acc, doc| acc.then(breaks(doc))),
        Doc::Indent(doc) => breaks(doc),
        Doc::Group { doc, broken } => {
            let inside = breaks(doc);
            Breaks {
                inner: inside.inner || *broken,
                ..inside
            }
        }
        Doc::ConditionalGroup(_) => Breaks::CONTENT,
        Doc::IfBreak { broken, flat } => breaks(broken).or(breaks(flat)),
        leaf => leaf_breaks(leaf),
    }
}

/// Whether `doc` cannot print on one line: it has a forced break before some of its content.
/// Explicitly broken groups count; `ConditionalGroup`s do not, as a state may avoid the break.
pub(crate) fn breaks_inside(doc: &Doc) -> bool {
    breaks(doc).inner
}

/// Whether `doc` prints a newline whatever the printer decides, at its end included.
pub(crate) fn has_forced_break(doc: &Doc) -> bool {
    let breaks = breaks(doc);
    breaks.inner || breaks.trailing
}

/// `doc` as it prints flat: lines become spaces or nothing, groups dissolve, `IfBreak` takes its
/// flat branch and a `ConditionalGroup` its first state. Forced breaks stay.
pub(crate) fn flatten(doc: Doc) -> Doc {
    match doc {
        Doc::Line => text(" "),
        Doc::SoftLine => Doc::Nil,
        Doc::Nil | Doc::Text(_) | Doc::HardLine | Doc::BlankLine => doc,
        Doc::Concat(docs) => Doc::Concat(docs.into_iter().map(flatten).collect()),
        Doc::Indent(doc) => indent(flatten(*doc)),
        Doc::Group { doc, .. } => flatten(*doc),
        Doc::ConditionalGroup(states) => states.into_iter().next().map_or(Doc::Nil, flatten),
        Doc::IfBreak { flat, .. } => flatten(*flat),
    }
}
