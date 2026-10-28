# Diagnostics

Every diagnostic of the Kagari compiler has a numbered code. The pages in this part list each code
with its meaning, a minimal program that triggers it, and how to fix it.

## Codes

A code is a letter and four digits, such as `E2001`:

- `E` is an error and `W` a warning. A warning uses the same digit range as the errors of its
  check, so `W2039` sits among the type errors.
- The first digit tells which check reports it:

| Codes | Check | Page |
| ----- | ----- | ---- |
| `E0xxx` | Syntax | [Syntax](syntax.md) |
| `E1xxx` | Modules, items and name resolution, assets | [Modules, Items and Names](modules.md) |
| `E2xxx`, `W2xxx` | Types, patterns and `match` exhaustiveness | [Types](types.md) |
| `E3xxx`, `W3xxx` | Kinds: the rules of `fn`, `pushdown fn`, `const fn` and scenes | [Kinds](kinds.md) |
| `E4xxx` | Constant evaluation | [Constant Evaluation](eval.md) |
| `E5xxx`, `W5xxx` | Story analysis | [Story Analysis](analysis.md) |
| `E6xxx`, `W6xxx` | Game system checks | [Game System](system.md) |
| `E7xxx` | Bytecode emission | [Emit](emit.md) |

## Severity

- An **error** stops the build: `kagari build` writes no `.kagari` file while any error remains,
  and `kagari check` exits with a non-zero status.
- A **warning** reports something that is probably a mistake (an unreachable `match` arm, a
  dropped `#[must_use]` value, an unreachable scene). The build still succeeds.

## Output format

`kagari check` and `kagari build` print each diagnostic on one line:

```text
path:line:col: error[E2001]: message
```

or `warning[W2039]` for a warning. Lines and columns are 1-based; columns count characters, not
bytes. Paths are relative to the project directory. When a diagnostic refers to a second place, an
indented note line follows it:

```text
src/main.kis:2:4: error[E1014]: message
  note: src/main.kis:1:4: message
```

If there is nothing to report, `kagari check` prints `no diagnostics`. The language server shows the
same diagnostics, with the same codes and messages, in the editor.

## Looking a code up

Every code has its own heading whose anchor is the lowercase code. To read about `E2001`, open
[`types.md#e2001`](types.md#e2001); in general the page is the one for the code's first digit (see
the table above) and the anchor is `#` followed by the code in lowercase, such as `#w5009`.
