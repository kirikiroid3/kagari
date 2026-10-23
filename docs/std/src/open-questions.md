# Open Questions

These are known gaps of the library. None of them is designed yet.

## Story randomness

The story may use a deterministic pseudo-random generator whose state is part of σ, so replay
reproduces every draw. Its API (draw ranges, weighted picks, how the seed enters the save) is not
designed.

## Timed input

Inputs that come from time instead of the player: auto-advance after a movie ends, and timed
choices that select a default arm when the time runs out. Both need a step that can end without a
click, and a recorded outcome for saves.

## NVL mode

A full-screen text mode where several lines stay on the screen until a page break. It changes what
the text window shows and how the backlog and `Narrative::line` describe a step.

## Weather and particles

Rain, snow, petals and similar effects. They need an animated component that is not a `Motion`
(it never ends at identity) and still keeps the logical screen comparable.

## Layered sprites

Auto-blink and lip sync on layered character sprites: parts that animate on their own, and lip
sync driven by the voice.

## Ruby dictionaries

Ruby for recurring words (names, terms) declared once instead of at every use in `t![…]`.

## Interpolated values in the history

Whether the backlog records the text of an interpolated value as it was shown, or recomputes it.

## Audio in system modes

A music room needs to play a track from a system mode, outside the game's screen. `std` has the
`heard` set to unlock tracks but no playback function for `fn` code.
