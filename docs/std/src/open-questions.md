# Open Questions

## Paused animations

While an opaque page is on top of `Game`, the narrative receives no input. Should the engine also
pause the screen's running animations and voice playback, or let them finish behind the page?

## Quit confirmation

Should `quit` from in-game require a confirmation dialog by rule, or is that left to the studio's
UI?

## Interaction syntax

`on-click="push(save)"` or a dedicated attribute per operation, such as `<button push="save">`?
The second is easier to validate by schema; the first keeps one attribute per event.

## Settings in system views

System views may read setting values directly, so `Setting<T>` is unnecessary there. Should system
code see plain `T`, or the same `Setting<T>` with an elimination form, so that one style function
can be shared by both scopes?

## Text interpolation

If `<s>` text ever interpolates a story variable, such as a player name, a line is no longer a
function of its step, and backlog entries must carry the interpolated values. Is interpolation
needed at all, and if so, are those values recorded in the history or recomputed by replay?

## Shared `.kix` across scopes

A `.kix` file of plain data, such as character constants or animation presets, may be imported in
both scopes. Is it the same definition in both, or two copies that merely have the same names?
It matters only if a value ever crosses between the scopes.

## Units

The virtual design resolution: one fixed resolution per game, or several with a rule for choosing?
