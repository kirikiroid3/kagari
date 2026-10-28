# Lexical Structure

A source file is split into **tokens**: identifiers, keywords, literals, labels and punctuation.
Whitespace and comments separate tokens and are otherwise ignored. `.kis` and `.kix` files share
one lexer; the [forms](./forms.md) of `.kix` files are built from the same tokens.

## Source text

- A source file is UTF-8. A file that is not valid UTF-8 cannot be read; a module whose file
  cannot be read is reported as not found ([`E1002`](../diagnostics/modules.md#e1002)).
- A byte order mark (U+FEFF) is not allowed; it is an unexpected character.
- Lines end with LF or CRLF. Both are whitespace.

## Whitespace

These characters are whitespace: space (U+0020), tab (U+0009), line feed (U+000A), vertical tab
(U+000B), form feed (U+000C), carriage return (U+000D), next line (U+0085), left-to-right mark
(U+200E), right-to-left mark (U+200F), line separator (U+2028) and paragraph separator (U+2029).
This is Unicode's `Pattern_White_Space`, the same set as Rust. Other Unicode spaces, such as the
ideographic space U+3000, are not whitespace and are an error outside string literals.

## Comments

| Form | Meaning |
| ---- | ------- |
| `// …` | Line comment, up to the end of the line |
| `/* … */` | Block comment. Block comments nest: `/* a /* b */ c */` is one comment |
| `/// …` | Doc comment of the item that follows. Editors show it on hover |
| `//! …` | By convention, the description of the module at the top of a file |

A comment may appear between any two tokens, also inside forms. Doc comments are line comments
that start with exactly three slashes: `////` is an ordinary comment. `//!` and `/** … */` have no
special meaning for the tools. A block comment without its closing `*/` is an error.

```kis
//! The characters of the game.

/// Hanako's entity on the screen.
pub const HANAKO: EntityId = EntityId(216); /* ids below 1000 are reserved /* by us */ */
```

## Identifiers

An identifier starts with a character of the Unicode class `XID_Start` or `_`, followed by any
number of characters of `XID_Continue`. Non-ASCII identifiers are allowed:

```kis
const 花子: EntityId = EntityId(216);
```

- `_` alone is not an identifier: it is the wildcard of patterns and the placeholder type.
- An identifier must not be a [strict keyword](#strict-keywords). There are no raw identifiers
  (`r#match`).
- Identifiers are compared exactly as written; the compiler does not normalize Unicode.

## Keywords

### Strict keywords

These words are always keywords and cannot be used as names:

```text
as        break     const     continue  crate     else      enum      false
fn        for       if        impl      in        let       loop      match
mod       mut       pub       return    self      Self      static    struct
super     trait     true      type      use       while
```

Two of them exist only for narrow uses:

- `as` renames an import (`use a::b as c;`). As a cast operator it parses but is always an error
  ([`E2019`](../diagnostics/types.md#e2019)); see [Conversions](./numbers.md#conversions).
- `trait` declares a trait, which only the standard library may do
  ([`E1012`](../diagnostics/modules.md#e1012)).

### Contextual keywords

These words are keywords only in one position and ordinary identifiers everywhere else:

| Word | Keyword when | Example |
| ---- | ------------ | ------- |
| `pushdown` | directly before `fn` (a function or a function pointer type) or before `Fn(` | `pushdown fn visit()`, `pushdown Fn(Map)` |
| `scene` | directly before `fn` in a type | `scene fn() -> Answer` |
| `Fn` | directly before `(` in a type | `Fn(u32) -> bool` |

### Rust keywords that are not keywords

`async`, `await`, `dyn`, `extern`, `move`, `ref`, `unsafe`, `where`, `yield` and Rust's reserved
words (`abstract`, `box`, `macro`, …) are ordinary identifiers in Kagari. The features behind them
do not exist; see [Differences from Rust](./rust-differences.md).

## Literals

| Literal | Examples | Type |
| ------- | -------- | ---- |
| Integer | `0`, `1_000`, `0xFF`, `0o17`, `0b1010`, `5u8`, `0x1Fi64` | an integer type, see [Integer literals](./numbers.md#integer-literals) |
| Decimal | `0.5`, `1.0`, `12.25`, `1_000.000_1` | `Decimal` |
| String | `"Mio"`, `"line\nbreak"`, `"\u{3042}"` | `Text` |
| Boolean | `true`, `false` | `bool` |

There are no character literals, byte literals, byte strings, raw strings or floating-point
literals.

### Integer literals

An integer literal is:

1. an optional radix prefix: `0x` (hexadecimal), `0o` (octal) or `0b` (binary), in lower case;
2. digits of that radix, with `_` allowed anywhere as a separator; at least one digit;
3. an optional suffix that fixes the type: `u8`, `u16`, `u32`, `u64`, `u128`, `usize`, `i8`,
   `i16`, `i32`, `i64`, `i128`, `isize`.

In a hexadecimal literal the suffix starts at the first character that is not a hexadecimal digit
or `_`; otherwise at the first character that is not a decimal digit or `_`. A literal with no
digits, a digit outside its radix (`0b102`), or any other suffix is an error. The value must fit
in 128 bits.

Because the suffix is read greedily, `1e5` is the integer `1` with the invalid suffix `e5`, and
`0X1F` is `0` with the invalid suffix `X1F`. A minus sign is not part of the literal: `-128i8` is
the operator `-` applied to `128i8`, and the compiler accepts it because the negated value fits.

### Decimal literals

A decimal literal is digits, a `.`, and digits again, with `_` allowed as a separator after the
first digit of each part. Both parts need a digit: `1.` and `.5` are not decimal literals. There
is no exponent and no suffix. A decimal literal has type [`Decimal`](./numbers.md#decimal).

Two cases are not decimal literals although they look like one:

- After a `.`, the number is split into tuple indices: `pair.0.1` is the field `1` of the field
  `0` of `pair`.
- After `..`, the number stays whole: `0..1.5` is a range up to `1.5`.

### String literals

A string literal is text between double quotes. It may span several lines; its line breaks are
part of the value. A backslash starts an escape:

| Escape | Character |
| ------ | --------- |
| `\n` | line feed |
| `\r` | carriage return |
| `\t` | tab |
| `\\` | backslash |
| `\"` | double quote |
| `\0` | U+0000 |
| `\u{…}` | the Unicode scalar value with 1 to 6 hexadecimal digits; `_` may separate digits |

Any other escape, such as `\'` or `\x41`, is an error, and so is a `\u{…}` that is not a Unicode
scalar value (a surrogate, or above U+10FFFF). There is no line continuation with a trailing
backslash. A string without its closing quote is an error.

A string literal has type `Text`, the constant text of the program (see
[Text and String](./types.md#text-and-string)).

```kis
const GREETING: Text = "\"Good morning,\" she said.\n\u{2026}";
```

### Boolean literals

`true` and `false` are keywords and the two values of `bool`.

## Labels

A **label** is `'` immediately followed by an identifier: `'outer`. Labels name loops and are the
targets of `break` and `continue` (see [Control Flow and Loops](./control-flow.md)). Kagari has no
lifetimes, so this is the only use of `'`.

```kis
'outer: for row in 0..3 {
    for col in 0..3 {
        if col == row { continue 'outer; }
    }
}
```

## Punctuation

The lexer produces single characters. Where the grammar needs a longer operator, the parser joins
**adjacent** characters, with nothing between them. So `>>` in `Vec<Vec<u8>>` closes two generic
argument lists, while `a >> 2` is a shift, and `- >` with a space is not `->`.

| Token | Uses |
| ----- | ---- |
| `(` `)` | grouping, tuples, calls, parameter lists, tuple structs |
| `[` `]` | arrays, indexing, array types, attributes, `t![…]` |
| `{` `}` | blocks, struct literals, item lists, `use` groups, `{expr}` in `t![…]` |
| `<` `>` | generics; less than, greater than |
| `,` `;` `:` | separators; `:` before a type or a bound |
| `.` | field access, method calls, tuple indices |
| `=` | assignment, initializers, form attributes (`who = …`) |
| `!` | logical and bitwise not; forms (`s!(…)`) and text effects (`!["…"](…)`) |
| `+` `-` `*` `/` `%` | arithmetic; `-` negation; `*` dereference and glob imports; `+` between bounds |
| `&` | references (`&x`, `&mut x`, `&T`); bitwise and |
| `\|` | closures (`\|x\| x + 1`); bitwise or; alternatives in patterns; ruby readings |
| `^` | bitwise exclusive or |
| `#` | attributes (`#[scene]`) |
| `?` | the `?` operator on `Option` and `Result` |
| `::` | path separator, turbofish (`f::<u8>()`) |
| `->` | return type |
| `=>` | match arms, choice arms |
| `==` `!=` `<=` `>=` | comparison |
| `&&` `\|\|` | logical and, logical or |
| `<<` `>>` | shifts |
| `+=` `-=` `*=` `/=` `%=` `^=` `&=` `\|=` `<<=` `>>=` | compound assignment |
| `..` `..=` | ranges, rest patterns |

These characters are not tokens and are an error outside comments and strings: `@`, `$`, `~`,
`` ` ``, `\`, and a `'` that does not start a label. `...` is not a token.

## Forms in `.kix` files {#forms-in-kix-files}

A `.kix` file has no tokens of its own. A **form** is an identifier, `!`, and an opening `(`, `[`
or `{`: `s!(…)`, `t![…]`, `choice!{…}`, `button!{…}`. Inside `t![…]`, string literals, ruby
`["漢字"]("かん" | "じ")`, text effects `!["…"](effect)` and interpolations `{expr}` follow each
other without commas. A form in a `.kis` file is an error. The forms and their syntax are
described in [Forms](./forms.md).

## Lexical errors

Every lexical error is reported as [`E0001`](../diagnostics/syntax.md#e0001): an unexpected
character, an unterminated block comment or string, an unknown escape, a malformed `\u{…}`, and a
malformed integer literal. The rest of the file is still read, so later errors may be reported as
well.
