//! Replaces the `kis` and `kix` code blocks of a chapter with highlighted HTML.

use mdbook_markdown::pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use mdbook_markdown::{MarkdownOptions, new_cmark_parser};

use crate::highlight::{Highlighter, Run};

/// Info-string languages that hold Kagari code.
const LANGUAGES: [&str; 2] = ["kis", "kix"];

/// Returns `markdown` with every fenced `kis`/`kix` code block replaced by a `<pre>` HTML block;
/// all other text is kept byte for byte.
///
/// The HTML is written on one line (newlines become `&#10;`) because a raw HTML block inside a
/// list item or block quote would otherwise need the container's indentation or `>` markers on
/// every line. The `<code>` element has the `nohighlight` class and no `language-` class, so
/// mdBook's highlight.js leaves the spans alone.
pub fn highlight_code_blocks(markdown: &str, highlighter: &mut Highlighter) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut copied = 0;
    // Start offset and collected text of the Kagari block being read.
    let mut block: Option<(usize, String)> = None;
    let parser = new_cmark_parser(markdown, &MarkdownOptions::default());
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if is_kagari(&info) => {
                block = Some((range.start, String::new()));
            }
            Event::Text(text) => {
                if let Some((_, code)) = &mut block {
                    code.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                let Some((start, code)) = block.take() else {
                    continue;
                };
                let (Some(before), Some(fence)) =
                    (markdown.get(copied..start), markdown.get(start..range.end))
                else {
                    continue;
                };
                out.push_str(before);
                push_html(&mut out, &highlighter.highlight(&code));
                if fence.ends_with('\n') {
                    out.push('\n');
                }
                copied = range.end;
            }
            _ => {}
        }
    }
    out.push_str(markdown.get(copied..).unwrap_or_default());
    out
}

fn is_kagari(info: &str) -> bool {
    let language = info.split([' ', '\t', ',']).next().unwrap_or_default();
    LANGUAGES.contains(&language)
}

fn push_html(out: &mut String, runs: &[Run<'_>]) {
    out.push_str(r#"<pre><code class="hljs nohighlight">"#);
    for (text, class) in runs {
        match class {
            Some(class) => {
                out.push_str(r#"<span class=""#);
                out.push_str(class);
                out.push_str(r#"">"#);
                push_escaped(out, text);
                out.push_str("</span>");
            }
            None => push_escaped(out, text),
        }
    }
    out.push_str("</code></pre>");
}

fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' => out.push_str("&#10;"),
            c => out.push(c),
        }
    }
}
