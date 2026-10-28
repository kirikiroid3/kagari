# Emit (E7xxx)

The last step of `kagari build` writes the checked program into the `.kagari` file and verifies
it. It runs only on a program without errors from the earlier checks.

## E7001 `missing-main` {#e7001}

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

## E7002 `function-too-large` {#e7002}

**Error.** A function needs more than 65 535 registers, or more code words than the bytecode can
address. Each local and temporary of a function takes a register, so this happens only for
enormous generated functions.

```kis
// A function that keeps more than 65 535 locals alive at once.
pub fn huge() -> u32 {
    let a0 = 0u32;
    let a1 = 1u32;
    // … up to a65999 …
    let mut s = 0u32;
    s = s + a0;
    s = s + a1;
    // … up to a65999 …
    s
}
```

This snippet is abbreviated: a real trigger spells out all 66 000 `let`s and uses each of them later. Locals that are never used do not take registers.

Fix: split the function into several smaller functions.

## E7003 `invalid-bytecode` {#e7003}

**Error.** An internal compiler error: the emitted bytecode fails the load-time verifier. The
message names the verifier's complaint. No correct program should produce it.

```text
error[E7003]: internal compiler error: the emitted bytecode fails verification: …
```

This snippet is illustrative: it shows the shape of the message, since no correct compiler produces this code from any program.

Fix: report a bug with the program that triggers it.
