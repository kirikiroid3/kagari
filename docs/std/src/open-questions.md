# Open Questions

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
