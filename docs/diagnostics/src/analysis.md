# Story Analysis (E5xxx)

After type checking, Kagari explores the whole story: every path from each entry scene, with every value the story variables can hold. This finds loops and recursions that never stop at a step, choices that can offer nothing, screens that depend on the path taken, and code or variables that never matter. When the analysis cannot finish, it says so and does not report unreachable steps or scenes.

## E5001 `stepless-loop`

**Error.** Some path around a loop in a scene runs no `s!`, `choice!`, or scene call that runs one. That loop could spin forever without waiting for the player.

```kix
static mut DONE: bool = false;

#[scene]
#[entry]
pushdown fn Main() {
    loop {
        if DONE {
            s!(t!["Done."]);
        } else {
            DONE = true;
        }
    }
}
```

Fix: make every path through the loop run a step, or leave the loop with `break` on paths that don't step.

## E5002 `stepless-recursion`

**Error.** A scene can call itself, directly or through other scenes, before it passes a step. The call stack would grow forever without input from the player.

```kix
static mut DONE: bool = false;

#[scene]
#[entry]
pushdown fn Echo() {
    if DONE {
        Echo();
    }
    s!(t!["Echo."]);
}
```

Fix: run a step before the recursive call, for example move `s!(t!["Echo."]);` above `Echo();`.

## E5003 `stuck-choice`

**Error.** The analysis can reach a `choice!` with every arm hidden by its guard, so the player would have nothing to pick. The message gives one set of story variable values where this happens.

```kix
static mut KEY: bool = false;
static mut KNOCKS: Int<0, 3> = 0;

enum Door { Open, Knock }

#[scene]
#[entry]
pushdown fn Main() {
    loop {
        match choice!{
            id = "door",
            t!["Open the door"] if KEY => Door::Open,
            t!["Knock"] if KNOCKS < 2 => Door::Knock,
        } {
            Door::Open => break,
            Door::Knock => KNOCKS += 1,
        }
    }
}
```

Fix: make sure at least one arm is always visible, for example by adding an arm with no guard.

## W5004 `inconsistent-screen`

**Warning.** The screen a step shows depends on which path reached it, because different paths last changed the screen in different ways. Loading or skipping to that step can't tell which screen to show.

```kix
const MIO: EntityId = EntityId(1);

enum Place { Roof, Classroom }

const fn roof(mut screen: Screen) -> Screen {
    screen.entity(MIO).opacity().change(1.0).apply_animation(instant());
    screen
}

const fn classroom(mut screen: Screen) -> Screen {
    screen.entity(MIO).opacity().change(0.5).apply_animation(instant());
    screen
}

#[scene]
#[entry]
pushdown fn Main() {
    match choice!{ id = "where", t!["Roof"] => Place::Roof, t!["Classroom"] => Place::Classroom } {
        Place::Roof => s!(render = roof, t!["The wind is cold."]),
        Place::Classroom => s!(render = classroom, t!["It's quiet."]),
    }
    s!(t!["So, about tomorrow…"]);
}
```

Fix: give the first inconsistent step a keyframe render (one that ignores its input screen), e.g. `s!(render = evening, t!["So, about tomorrow…"]);`, or choose the render by a story variable: `s!(render = if PLACE == Place::Roof { roof } else { classroom }, t!["…"]);`.

## E5005 `unbounded-screens`

**Error.** A render produces a new, different screen every time a loop runs. The number of screens that can reach a step has no bound (the limit is 64).

```kix
const MIO: EntityId = EntityId(1);

enum Walk { On, Stop }

const fn mio_steps_right(mut screen: Screen) -> Screen {
    let at = screen.entity(MIO).position().get();
    screen.entity(MIO).position().change(point(at.x + 10, at.y)).apply_animation(instant());
    screen
}

#[scene]
#[entry]
pushdown fn Main() {
    loop {
        match choice!{ id = "walk", t!["Walk on"] => Walk::On, t!["Stop"] => Walk::Stop } {
            Walk::On => s!(render = mio_steps_right, t!["You walk on."]),
            Walk::Stop => break,
        }
    }
}
```

Fix: use a render that sets absolute values instead of building on the previous screen, or limit the change with a bounded story variable.

## W5006 `too-many-states`

**Warning.** The story has more states than the analysis allows, so it stops and does not report unreachable steps or scenes. The default limit is 200 000 states. The warning also fires when an entry scene's arguments or a random draw have too many values to list.

```kix
#[scene]
#[entry]
pushdown fn Main() {
    let n = rng_choose::<Int<0, 100000>>();
    if n == 7 {
        s!(t!["Lucky."]);
    }
}
```

Fix: narrow the types of story variables, entry scene parameters and random draws, or raise the state limit with `--state-cap`.

## W5007 `incomplete`

**Warning.** The analysis can't follow the story past some point, so unreachable steps and scenes are not reported. A `for` loop in a scene with more than 1 048 576 items is one example; a value the analysis can't store held across a step is another.

```kix
#[scene]
#[entry]
pushdown fn Main() {
    for _lap in 0..2000000 {
        s!(t!["Another lap."]);
    }
}
```

Fix: keep `for` loops in scenes short, and don't keep values the analysis can't store alive across `s!`/`choice!`; compute such results before the step.

## W5008 `unreachable-scene`

**Warning.** No path from an entry scene ever calls this scene.

```kix
#[scene]
#[entry]
pushdown fn Main() {
    s!(t!["The end."]);
}

// Never called.
#[scene]
pushdown fn Epilogue() {
    s!(t!["Years later."]);
}
```

Fix: call the scene from a reachable scene, mark it `#[entry]`, or delete it.

## W5009 `unreachable-step`

**Warning.** No reachable state of the story ever gets to this step, often because a condition can never be true.

```kix
static mut KNOCKS: Int<0, 3> = 0;

enum Knock { Again, Stop }

#[scene]
#[entry]
pushdown fn Main() {
    while KNOCKS < 2 {
        match choice!{ id = "knock", t!["Knock"] => Knock::Again, t!["Leave"] => Knock::Stop } {
            Knock::Again => KNOCKS += 1,
            Knock::Stop => break,
        }
    }
    // Reached only when `KNOCKS` is 3, which never happens.
    if KNOCKS == 3 {
        s!(t!["The door gives way."]);
    }
}
```

Fix: fix the condition so it can hold, or remove the dead step.

## W5010 `unused-story-variable`

**Warning.** A story variable is never used, only ever assigned, or read only in ways that never affect a condition, step, meta write or ending.

```kis
static mut WRITE_ONLY: bool = false;   // only assigned: never read
static mut UNTOUCHED: bool = false;    // never used at all
```

Fix: read the variable where it should matter (a condition, a line, an ending) or delete it.

## W5011 `missing-keyframe`

**Warning.** Turn this check on with `--keyframe-check`. A choice, or the first step of an entry scene, has no keyframe render. Loading or jumping there would have to replay renders from an earlier keyframe.

```kix
const A: EntityId = EntityId(1);

enum Answer { Yes, No }

const fn key(_screen: Screen) -> Screen {
    Screen::empty()
}

const fn delta(mut screen: Screen) -> Screen {
    screen.entity(A).opacity().change(0.5).apply_animation(instant());
    screen
}

#[entry]
#[scene]
pushdown fn Start() {
    s!(render = key, t!["a"]);
    let _ = choice!{ id = "c", render = delta, t!["Yes"] => Answer::Yes, t!["No"] => Answer::No };
}
```

Fix: give the choice a render that ignores its input screen, e.g. `render = key`.
