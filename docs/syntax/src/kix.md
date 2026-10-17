# Extensions (`.kix`)

A `.kix` file declares the vocabulary that representing code is written in. It uses the same
syntax as `.kirp`, but works like a C header: it may only contain declarations.

| Allowed                                       | Not allowed                          |
| --------------------------------------------- | ------------------------------------ |
| Type declarations (`struct`, `type`)          | `render`, or any other code that runs |
| Method signatures (`impl` blocks without bodies) | Statements                         |
| Function signatures                           | References to other files            |
| Constants with their values                   |                                      |
| Modules grouping constants (`mod`)            |                                      |

Functions and methods are declared without bodies, ending in `;`. Their implementation is
supplied by the engine or by a studio's extension (see below).

## Why a header

The split lets one language serve two audiences:

- The engine ships **presets** as `.kix` declarations: the `Screen` and entity model, layers,
  and basic animations such as `jump` or `instant_switch`.
- A **studio** that needs more (its own animation curves, effects, layer kinds) writes its own
  `.kix` file and provides the implementation, without changing the engine or the compiler. The
  compiler type-checks all representing code against the declarations, so studio extensions get
  the same checking as the presets.

How a studio's implementation is linked to its declarations is an
[open question](./open-questions.md#kix-implementations).

## Example

The declarations used by the examples in this book. The names and shapes here are illustrative,
not the actual preset library.

```kix
// std.kix — shipped with the engine

type EntityId = u64;
type ImageId = u64;

struct Screen;
impl Screen {
    fn entity(self, id: EntityId) -> Entity;
    fn background(self) -> Layer;
}

struct Entity {
    cord: Cord,
    appearance: Appearance,
}

struct Appearance {
    emotion: Layer,
}

struct Cord;
impl Cord {
    fn apply_animation(self, animation: CordAnimation);
}

struct Layer;
impl Layer {
    fn change(self, image: ImageId) -> LayerImageChange;
}

struct LayerImageChange;
impl LayerImageChange {
    fn apply_animation(self, animation: TransitionAnimation);
}

struct CordAnimation;
struct TransitionAnimation;

fn jump(height: u64) -> CordAnimation;
fn instant_switch() -> TransitionAnimation;
fn fade(frames: u64) -> TransitionAnimation;
```

```kix
// characters.kix — written by the studio

const HANAKO: EntityId = 216;
const AZUSA: EntityId = 217;

mod AZUSA {
    const CLOSE_EYE_SMILE: ImageId = 4001;
}

mod BG {
    const ROOF: ImageId = 9001;
    const CLASSROOM: ImageId = 9002;
}
```

`AZUSA` names both a constant and a module, as in Rust: constants live in the value namespace,
modules in the type namespace, so `AZUSA` is an `EntityId` and `AZUSA::CLOSE_EYE_SMILE` is one of
her images.

How an `ImageId` is bound to an actual image file is an
[open question](./open-questions.md#asset-binding).
