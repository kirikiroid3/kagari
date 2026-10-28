# Syntax (E0xxx)

Syntax errors are reported by the parser before any name resolution or type checking happens. A file with syntax errors is still parsed as far as possible, so later diagnostics may also appear, but fixing the syntax errors first usually clears them.

## E0001 `syntax` {#e0001}

**Error.** The source text does not parse as Kagari, or uses a construct that is not allowed in this kind of file — for example, narrative forms such as `s!`, `choice!` or `t!` inside a `.kis` file, which only accepts plain script code.

```kis
#[scene]
pushdown fn Scene() {
    s!(t!["x"]);
}
```

Fix: correct the reported syntax, or move form-based code into a `.kix` file (here, rename `main.kis` to `main.kix`).
