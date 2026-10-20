# The Prelude (`std/prelude.kix`)

`std/prelude.kix` is visible in the game scope without an import. It is the vocabulary that
representing code (`.kirp` files and `<r>` blocks) starts from: the screen access API, the common
components, the asset types and the basic animations. A studio adds to it with its own `.kix`
files.

*Tentative:* the shapes below are illustrative, the ones the syntax book's examples use. The real
preset library replaces them.

## Basic types

```kix
enum Option<T> {
    None,
    Some(T),
}

type EntityId = u64;
type ImageId = u64;

struct ImageAsset;
struct AudioAsset;
struct VoiceAsset;
```

The asset types are the types of asset constants. The `voice` attribute of `<m>` names a constant
of type `VoiceAsset`.

## The screen

```kix
struct Screen;

impl Screen {
    #[builtin]
    fn empty() -> Screen;
    #[builtin]
    fn entity(self, id: EntityId) -> Entity;
    #[builtin]
    fn background(self) -> Layer;
}

struct Entity {
    cord: Cord,
    appearance: Appearance,
}

struct Cord {
    x: i64,
    y: i64,
}

impl Cord {
    #[builtin]
    fn apply_animation(self, animation: CordAnimation);
}

struct Appearance {
    emotion: Layer,
}

struct Layer {
    image: ImageId,
}

impl Layer {
    #[builtin]
    fn change(self, image: ImageId) -> LayerImageChange;
}

#[must_use]
struct LayerImageChange {
    image: ImageId,
}

impl LayerImageChange {
    #[builtin]
    fn apply_animation(self, transition: TransitionAnimation);
}
```

Functions marked `#[builtin]` have no body: the engine implements them, as the interface to the
ECS world of the screen. `#[builtin]` is only allowed in the bundled standard library.

`LayerImageChange` is `#[must_use]`: a layer change only happens through `apply_animation`, so the
compiler warns about a change that is never applied.

## Animations

```kix
struct CordAnimation {
    height: u64,
    frames: u64,
}

struct TransitionAnimation {
    frames: u64,
}

fn jump(height: u64) -> CordAnimation {
    CordAnimation { height, frames: 12 }
}

fn instant_switch() -> TransitionAnimation {
    TransitionAnimation { frames: 0 }
}

fn fade(frames: u64) -> TransitionAnimation {
    TransitionAnimation { frames }
}
```

These are ordinary `.kix` functions with bodies, so the compiler evaluates them like any studio
function.
