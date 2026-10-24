# Open Questions

These are known gaps of the library. None of them is designed yet.

## NVL mode

A full-screen text mode where several lines stay on the screen until a page break. It changes what
the text window shows and how the backlog and `Narrative::line` describe a step.

## Layered sprites

Auto-blink and lip sync on layered character sprites: parts that animate on their own, and lip
sync driven by the voice.

## Syncing text and animation

Driving animation from the text reveal, and the reverse, beyond `at_mark`: for example an
animation that follows each revealed character, or a reveal that waits for an animation.

## Audio in system modes

`system::set_music` plays music outside the game, but what is playing is not designed: the current
track, and a music room's playback state.
