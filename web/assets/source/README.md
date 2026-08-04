# Source art

The sheets `scripts/build-tileset.py` cuts `web/assets/tiles.png` from, kept
verbatim rather than pre-cut so a re-cut needs no new asset and what a sprite was
lifted from stays checkable.

They came out of an **earlier Godot experiment**, not from a brief for this game, so
they are incomplete and carry ideas that are not in scope — ropes for traversal, a
grid floor you can see through, stairs. Treat them as a quarry, not a spec.

## `tiles.png` — 2048×2048, 48×48 cells, content in row 0 only

| Index | What it is |
|---|---|
| 0 | Default — no information |
| 1–15 | **Wall autotile patterns**, by which sides the wall joins (see below) |
| 16 | Floor |
| 17–23 | Unassigned experiments — **not in use** |

The autotile run is indexed by which sides the wall run continues along:

| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|
| none | E | N | W | S | E–W | N–S | SE |

| 9 | 10 | 11 | 12 | 13 | 14 | 15 |
|---|---|---|---|---|---|---|
| SW | NW | NE | NSE | EWS | NWS | NWE |

**Indices 1–15 are a step-2 asset.** #460 is one tile per glyph with no neighbour
lookup, so it can only take index 1 — a wall with no wall against it — as its
representative `#`. The other fourteen are exactly what the autotiler in tiles step 2
will want, which is why the legend is written down here rather than rediscovered
later. Index 16, the floor, needs no neighbours and is used as it is.

## `player.png` — 768×768, 48×48 cells, content in row 0 only

An **animation sheet**: ten frames of one hooded figure. #460 has no animation, so it
takes frame 0 and nothing else.

## What happens to them on the way in

`build-tileset.py` desaturates every lifted tile to greyscale + alpha and normalises
its range. The desaturation is not a choice: §11.2 **[SETTLED]** says no game system
names a colour, and a guard's yellow → orange → red *is* the AI state machine made
visible, so art carrying its own palette would leave the threat ladder nowhere to
live. What survives is the art's structure — luminance becomes the shading the
category tint multiplies through.
