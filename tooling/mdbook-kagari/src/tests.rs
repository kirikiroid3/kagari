#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use mdbook_markdown::pulldown_cmark::html;
use mdbook_markdown::{MarkdownOptions, new_cmark_parser};

use crate::chapter::highlight_code_blocks;
use crate::highlight::Highlighter;

fn process(markdown: &str) -> String {
    highlight_code_blocks(markdown, &mut Highlighter::new().unwrap())
}

/// The HTML mdBook renders from `markdown`.
fn render(markdown: &str) -> String {
    let mut out = String::new();
    html::push_html(
        &mut out,
        new_cmark_parser(markdown, &MarkdownOptions::default()),
    );
    out
}

/// The highlighted HTML of one `kis` block holding `code`.
fn highlight(code: &str) -> String {
    let out = process(&format!("```kis\n{code}```\n"));
    out.strip_suffix('\n').unwrap().to_owned()
}

/// The text a browser shows for `html`: tags dropped, entities decoded.
fn text_of(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.replace("&#10;", "\n")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn between<'a>(html: &'a str, open: &str, close: &str) -> &'a str {
    let start = html.find(open).unwrap() + open.len();
    let end = start + html[start..].find(close).unwrap();
    &html[start..end]
}

const PRE: &str = r#"<pre><code class="hljs nohighlight">"#;

#[test]
fn blocks_stay_inside_list_items_and_block_quotes() {
    let markdown = "\
- item

  ```kis
  let x = 1;

  let y = 2;
  ```

> quote
>
> ```kix
> s!(t![\"a\"]);
> ```

after
";
    let out = render(&process(markdown));
    let item = between(&out, "<li>", "</li>");
    assert!(item.contains(PRE), "{out}");
    assert_eq!(
        text_of(between(item, PRE, "</code>")),
        "let x = 1;\n\nlet y = 2;\n"
    );
    let quote = between(&out, "<blockquote>", "</blockquote>");
    assert!(quote.contains(PRE), "{out}");
    assert_eq!(text_of(between(quote, PRE, "</code>")), "s!(t![\"a\"]);\n");
    assert!(out.trim_end().ends_with("<p>after</p>"), "{out}");
}

#[test]
fn other_code_is_kept_byte_for_byte() {
    let markdown = "\
Text with `kis` inline.

```rust
fn main() {}
```

````markdown
```kis
let x = 1;
```
````

```text
plain
```
";
    assert_eq!(process(markdown), markdown);
}

#[test]
fn surrounding_markdown_is_kept() {
    let out = process("# Title\n\nBefore.\n```kix\nlet a = 1;\n```\nAfter.\n");
    assert!(out.starts_with("# Title\n\nBefore.\n<pre>"), "{out}");
    assert!(out.ends_with("</code></pre>\nAfter.\n"), "{out}");
}

#[test]
fn html_shows_exactly_the_code_on_one_line() {
    let code = "// <tag> & \"quote\"\nconst S: &str = \"a < b && 数多\";\n\nfn f() -> bool { 1 << 2 > 3 }\n";
    let html = highlight(code);
    assert!(!html.contains('\n'), "{html}");
    assert_eq!(text_of(between(&html, PRE, "</code>")), code);
}

#[test]
fn inner_nodes_and_later_patterns_win() {
    let html = highlight("#[doc(\"x\")]\nfn ABC() { s!(t![\"y\"]); }\n");
    // The string inside the attribute keeps its own color.
    assert!(
        html.contains(
            r#"<span class="hljs-meta">#[doc(</span><span class="hljs-string">"x"</span>"#
        ),
        "{html}"
    );
    // `@function` comes after the all-caps `@constant` pattern.
    assert!(
        html.contains(r#"<span class="hljs-title">ABC</span>"#),
        "{html}"
    );
    // `@function.special` is the later capture of the form-name pattern.
    assert!(
        html.contains(r#"<span class="hljs-built_in">s</span>!("#),
        "{html}"
    );
}

#[test]
fn snippets_mixing_statements_and_items_are_colored_throughout() {
    let code = "CLEARED.write(true);\n\nconst fn f(mut screen: Screen) -> Screen { screen }\n";
    let html = highlight(code);
    // The statement only parses inside a body, the `const fn` only outside one.
    assert!(
        html.contains(r#"<span class="hljs-literal">true</span>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<span class="hljs-title">f</span>"#),
        "{html}"
    );
    assert!(
        html.contains(r#"<span class="hljs-type">Screen</span>"#),
        "{html}"
    );
    assert_eq!(text_of(between(&html, PRE, "</code>")), code);
}
