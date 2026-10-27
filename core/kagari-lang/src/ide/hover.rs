//! Hover: the header or type of what is at an offset, with its doc comment.

use kagari_syntax::{SyntaxKind, SyntaxNode, TextRange, TextSize};
use rowan::NodeOrToken;

use super::header::{collapse_whitespace, doc_comment, item_header};
use super::lookup::{Found, reference_at};
use crate::db::Db;
use crate::def_map::{Def, Resolution, def_maps};
use crate::infer::inference;
use crate::input::{Program, SourceFile};
use crate::item_tree::{ItemId, item_source_map};
use crate::parse_query::parse;
use crate::paths::def_path;
use crate::resolve::body_resolution;
use crate::signatures::display_ty;

/// Hover text: Markdown with a `kagari` code block and the doc comment below it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hover {
    /// The reference the text is about.
    pub range: TextRange,
    pub markdown: String,
}

/// What to show for the reference at `offset` in `file`.
pub fn hover(db: &dyn Db, program: Program, file: SourceFile, offset: TextSize) -> Option<Hover> {
    let (owner, range, found) = reference_at(db, program, file, offset)?;
    let (text, doc) = match &found {
        Found::Res(
            Resolution::Def(Def::Item(item)) | Resolution::Assoc(item) | Resolution::SelfType(item),
        )
        | Found::Method(item)
        | Found::DefSite(Def::Item(item)) => {
            let node = item_node(db, *item)?;
            (item_header(&node), doc_comment(&node))
        }
        Found::Res(Resolution::Local(local)) => {
            let resolved = body_resolution(db, program, owner).body.as_ref()?;
            let local_data = &resolved.locals[*local];
            let ty = inference(db, program, owner)?.local_types.get(*local)?;
            let mutability = if local_data.mutable { "mut " } else { "" };
            let text = format!(
                "let {mutability}{}: {}",
                local_data.name,
                display_ty(db, program, ty)
            );
            (text, String::new())
        }
        Found::Res(Resolution::Def(Def::Variant { enum_item, index }))
        | Found::DefSite(Def::Variant { enum_item, index }) => {
            let node = variant_node(db, *enum_item, *index)?;
            (
                collapse_whitespace(&node.text().to_string()),
                doc_comment(&node),
            )
        }
        Found::Res(Resolution::Def(def @ Def::Module(_)))
        | Found::DefSite(def @ Def::Module(_)) => {
            let maps = def_maps(db, program);
            (format!("mod {}", def_path(db, maps, *def)), String::new())
        }
        Found::Res(Resolution::Prim(prim)) => (prim.name().to_owned(), String::new()),
        Found::Res(Resolution::GenericParam { owner, index }) => {
            let map = crate::hir::signature_source_map_of(db, *owner)?;
            let name = *map.generic_params.get(usize::try_from(*index).ok()?)?;
            let source = owner.file.text(db);
            let text = source
                .as_deref()?
                .get(std::ops::Range::<usize>::from(name))?;
            (text.to_owned(), String::new())
        }
        Found::Res(Resolution::Deferred { .. } | Resolution::Error) => return None,
    };
    let mut markdown = format!("```kagari\n{text}\n```");
    if !doc.is_empty() {
        markdown.push_str("\n\n");
        markdown.push_str(&doc);
    }
    Some(Hover { range, markdown })
}

/// The syntax node of `item`.
fn item_node(db: &dyn Db, item: ItemId) -> Option<SyntaxNode> {
    let src = item_source_map(db, item.file).items.get(item.index.idx())?;
    Some(src.ptr.to_node(&parse(db, item.file).syntax_node()))
}

/// The `VARIANT` node of variant `index` of `enum_item`.
fn variant_node(db: &dyn Db, enum_item: ItemId, index: u32) -> Option<SyntaxNode> {
    let src = item_source_map(db, enum_item.file)
        .items
        .get(enum_item.index.idx())?;
    let name = *src.variants.get(usize::try_from(index).ok()?)?;
    let root = parse(db, enum_item.file).syntax_node();
    if !root.text_range().contains_range(name) {
        return None;
    }
    let start = match root.covering_element(name) {
        NodeOrToken::Node(node) => node,
        NodeOrToken::Token(token) => token.parent()?,
    };
    start
        .ancestors()
        .find(|node| node.kind() == SyntaxKind::VARIANT)
}
