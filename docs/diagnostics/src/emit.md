# Emit (E7xxx)

The emit stage compiles a checked program into `.kagari` bytecode and verifies it. It runs only on
a program without errors from the earlier stages.

## E7001 `missing-main`

**Error.** The root module has no `fn main()`, so the engine has nothing to run at boot. `main` is
the plain `fn` in the root file that describes the game system.

```kix
// main.kix: a root file with a story but no `fn main()`
#[entry]
#[scene]
pushdown fn Start() {
    s!(t!["Hello."]);
}
```

Fix: add `fn main() -> System { … }` to the root file, returning the game's `System`.

## E7002 `function-too-large`

**Error.** A function needs more than 65 535 registers, or more code words than the bytecode can
address. Each local and temporary of a function takes a register, so this happens only for
enormous generated functions.

```kis
// A function whose body binds more than 65 535 locals.
fn huge() {
    let a0 = 0u32;
    let a1 = 0u32;
    // … tens of thousands more …
}
```

This snippet is illustrative: a real trigger must spell out all of those locals, e.g. a generated file whose function binds 66 000 `let`s.

Fix: split the function into several smaller functions.

## E7003 `invalid-bytecode`

**Error.** An internal compiler error: the emitted bytecode fails the load-time verifier. The
message names the verifier's complaint. No correct program should produce it.

```text
error[E7003]: internal compiler error: the emitted bytecode fails verification: …
```

This snippet is illustrative: it shows the shape of the message, since no correct compiler produces this code from any program.

Fix: report a bug with the program that triggers it.
