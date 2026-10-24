# Introduction

Every diagnostic of the Kagari compiler has a numbered code. This book lists each code with an
explanation, a minimal program that triggers it, and how to fix it.

## The scheme

A code is a letter and four digits, such as `E2001`:

- `E` is an error and `W` a warning. A warning uses the same digits as the errors of its stage.
- The first digit is the compiler stage that reports it:

| Codes | Stage |
| ----- | ----- |
| `E0xxx` | Syntax |
| `E1xxx` | Modules, items and name resolution |
| `E2xxx` | Types |
| `E3xxx` | Kinds: the rules of `fn`, `pushdown fn`, `const fn` and scenes |
| `E4xxx` | Constant evaluation |
| `E5xxx` | Story analysis |
| `E6xxx` | Game system checks |
| `E7xxx` | Bytecode emission |

An error stops the build; a warning does not.

## Output format

`kagari check` and `kagari build` print each diagnostic as

```text
path:line:col: error[E2001]: message
```

or `warning[W5009]` for a warning, one line per diagnostic.
