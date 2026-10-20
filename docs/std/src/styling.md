# Styling

## Style is local, layout is an engine pass

The `style` attribute of a UI element (syntax book, *System Script*) names a function in a `.kix`
file of the system scope that returns the element's style. The function is total and takes no
parameters, so the compiler evaluates it.

Flex layout is non-local, but only in *solving sizes*: a child's size depends on its siblings and
on its parent's constraints. Deriving styles stays local. The two are separate:

- **Style** is a node's own properties: paint (colors, font, sprite), box (padding, size
  constraints), and its own item parameters for its parent's layout. There is no cascade: no
  inherited and no selector-matched rules. Shared values such as a font are passed explicitly, as
  theme constants.
- **Layout** is a pure engine pass, `layout(tree, styles, viewport) -> Rects`, recomputed when the
  window size changes and never saved. Text measurement is its one input from the engine. Taffy,
  a Rust crate implementing flexbox and grid, is a ready implementation.

## The parent relation is typed

```text
Style     = { paint: PaintStyle, box: BoxStyle, item: ItemStyle }
ItemStyle = Flex(FlexItem) | Stack(StackItem)
```

The `item` variant must match the layout kind of the parent. Containers such as `<row>` and
`<col>` take a `FlexContainer` style, and their children must have `Flex` item styles. In CSS,
`flex-grow` on a child of a non-flex parent is silently ignored; here it is a compile error.

## Interaction states as overrides

A style carries a partial override for each interaction state, together with the transitions into
and out of that state:

```kix
// std/ui.kix — tentative names

struct ButtonStyle {
    base: Style,
    hover: Option<Override>,
    focus: Option<Override>,
    pressed: Option<Override>,
    disabled: Option<Override>,
}

struct Override {
    patch: Partial<PaintStyle>,
    enter: StateTransition,
    leave: StateTransition,
}

struct StateTransition {
    frames: u64,
}
```

```kix
// studio ui.kix

fn common_btn() -> ButtonStyle {
    ButtonStyle {
        base: Style { paint: PAINT_IDLE, box: BOX_BTN, item: Flex(FLEX_FIXED) },
        hover: Some(Override {
            patch: Partial { background: Some(COLOR_HOVER), ..Partial::none() },
            enter: StateTransition { frames: 6 },
            leave: StateTransition { frames: 6 },
        }),
        focus: None,
        pressed: None,
        disabled: None,
    }
}
```

- **Merge.** A patch acts on a style by right-biased merge, `s ⊕ p`: every field set in `p`
  replaces the one in `s`. Partial records under this merge form a monoid that acts on full
  styles, so stacking states is well defined.
- **Order.** The monoid is not commutative, so the engine fixes the order:
  `base ⊕ hover ⊕ focus ⊕ pressed`. `disabled` replaces the whole chain: a disabled button has no
  hover.
- **Derived partial type.** `Partial<T>` is generated from `T`, with every field wrapped in
  `Option`, so the override type cannot drift from the style type.
- **Paint only.** Patches are `Partial<PaintStyle>`, not `Partial<Style>`. A hover that changed
  padding or size would re-run layout and shift the siblings every time the pointer moves over the
  button. The type rules that out.
- **Transitions.** Continuous fields (colors, opacity, offsets) are interpolated by `enter` and
  `leave`. Discrete fields (sprite, font) cross-fade over the same frames, or switch instantly
  with `frames: 0`.
- **Own transition type.** `TransitionAnimation`, `fade` and the other animations of the
  [prelude](./prelude.md) belong to the game scope, so they are not visible here. `std/ui.kix`
  declares `StateTransition` for the system scope instead.

Every style is constant, so the compiler evaluates every combination of states ahead of time.

## Units

*Tentative.* Lengths are in a virtual design resolution, plus percentages and `fr`, so a layout
scales with the window.
