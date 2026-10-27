//! Post-format safety check: the output must parse without errors to the same tokens (commas
//! aside) and the same comments as the input.

use kagari_syntax::{Parse, SourceKind, SyntaxKind, SyntaxNode, parse};

use crate::FormatError;

pub(crate) fn check(input: &Parse, output: &str, kind: SourceKind) -> Result<(), FormatError> {
    let reparsed = parse(output, kind);
    if !reparsed.errors().is_empty()
        || signature(&input.syntax_node()) != signature(&reparsed.syntax_node())
    {
        return Err(FormatError::Internal(
            "formatting changed the program; this is a formatter bug".to_owned(),
        ));
    }
    Ok(())
}

type Signature = (Vec<(SyntaxKind, String)>, Vec<String>);

fn signature(root: &SyntaxNode) -> Signature {
    let mut tokens = Vec::new();
    let mut comments = Vec::new();
    for token in root
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        match token.kind() {
            SyntaxKind::WHITESPACE | SyntaxKind::COMMA => {}
            SyntaxKind::LINE_COMMENT | SyntaxKind::BLOCK_COMMENT => {
                comments.push(token.text().trim_end().to_owned());
            }
            kind => tokens.push((kind, token.text().to_owned())),
        }
    }
    (tokens, comments)
}
