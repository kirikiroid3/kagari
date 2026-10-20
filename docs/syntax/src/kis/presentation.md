# Presentation Tags

The script says *when* the screen changes; [representing code](../kirp.md) says *how*. Two tags
connect them.

## `<r>`: inline representing code

```xml
<s>
  <r>
    screen.entity(HANAKO).cord.apply_animation(jump(20));
    screen.entity(AZUSA).appearance.emotion.change(AZUSA::CLOSE_EYE_SMILE).apply_animation(instant_switch());
  </r>
  <m who="">"Hanako, calm down."</m>
</s>
```

`<r>` is sugar for an anonymous `.kirp` render function. Its content is placed inside a hidden
wrapper:

```kirp
fn render(screen: Screen) -> Screen {
    // content of <r> goes here
    screen
}
```

So the content is a sequence of statements on `screen`; the wrapper returns it.

If the code contains `<` or `&` (for example generic arguments or `&&`), wrap it in CDATA so the
XML parser leaves it alone. Wrapping code that does not need it is harmless:

```xml
<r><![CDATA[
    screen.entity(HANAKO).cord.apply_animation(jump(20));
]]></r>
```

## `<represent>`: representing code from a file

```xml
<s>
  <represent src="opening.kirp" />
  <m who="">The opening begins.</m>
</s>
```

Runs the `render` function of `opening.kirp`. Use it for representation that is long or reused
in several places. `src` is relative to the script file.

## Where they go

`<r>` and `<represent>` are children of a step: `<s>` or `<choice>`. When the step is entered,
its representation runs before the step waits for input:

{{#include ../diagrams/render-chain.svg}}

- Several `<r>`/`<represent>` children run in document order, each receiving the previous result.
- Position relative to the `<m>` children of `<s>` does not matter. *Tentative:* by convention
  they come first.
- A step without either leaves the screen unchanged.

Representation is attached to steps only. To present differently depending on a variable, put
differently represented steps in the arms of a `<match>`; render functions themselves never see
script variables.

A representation that ignores the screen it receives and builds the whole screen itself is a
[keyframe](../kirp.md#keyframes). Put one at choice points and chapter starts so that loading and
jumping replay little presentation.
