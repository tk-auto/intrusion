# Source art

The sheets `scripts/seed-tileset.py` seeded `web/assets/tiles.png` from, kept verbatim
rather than pre-cut so a re-cut needs no new asset and what a sprite was lifted from
stays checkable. The sheet it seeded is **authored by hand** from there on; these are
its starting point, not its master.

They came out of an **earlier Godot experiment**, not from a brief for this game, so
they are incomplete and carry ideas that are not in scope — ropes for traversal, a
grid floor you can see through, stairs. Treat them as a quarry, not a spec.

## `tiles.png` — 2048×2048, 48×48 cells, content in row 0 only

| Index | What it is |
|---|---|
| 0 | Default — no information |
| 1–15 | **Wall autotile patterns**, by which sides the wall is exposed (see below) |
| 16 | Floor |
| 17–23 | Unassigned experiments — **not in use** |

The autotile run is indexed by which sides the wall is **exposed** on — the sides where
the run *stops*, not where it continues:

| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|
| none | E | N | W | S | E–W | N–S | SE |

| 9 | 10 | 11 | 12 | 13 | 14 | 15 |
|---|---|---|---|---|---|---|
| SW | NW | NE | NSE | EWS | NWS | NWE |

**This legend used to be written the other way round, and it was wrong** (#461). The
art settles it: index 1 is a plain block with no line anywhere, and every line the run
adds is a bright boundary along the named side. A plain block is what a wall in the
*middle* of a mass of wall looks like; an isolated pillar is the one that needs a
boundary on all four sides. Read the other way, the run draws a seam down the middle of
every corridor wall, and the tile the source is missing becomes the commonest cell in
the facility instead of the rarest.

Index 16, the floor, needs no neighbours and is used as it is.

The run is seeded into the built sheet's **wall band** (slots 16–31), remapped from the
order above into a **neighbour bitmask** — `N=1, E=2, S=4, W=8`, the complement of the
exposed set — so the autotiler can compute a slot instead of keeping a table. Only the
six rotation representatives are written; the other ten neighbourhoods are those six
turned at draw time (see `docs/render-reference.md` §6.2). The source has no
all-four-sides-exposed tile, so the isolated pillar is **composed** from the run: each
single-exposed tile is the same fill with one boundary line added, so the union of the
four is the tile the artist would have drawn.

## `player.png` — 768×768, 48×48 cells, content in row 0 only

An **animation sheet**: ten frames of one hooded figure, seen from above.

**Nothing is lifted from it, and that is a decision** (#461). #460 took frame 0 as the
`@`; step 2 took it back out. The frames are a walk cycle of a **single facing** with
no cue for which way the figure is looking, so turned through the four quarters one
reads as a blob that has moved rather than as somebody facing west — and a renderer
whose job is to draw facing cannot use art that cannot show it. The `@` is a crude
placeholder body plan, with a brim that says where the front is, until somebody draws
the cue into these. They stay here for that day; the slot is one line from taking them
back.

## What happens to them on the way in

`seed-tileset.py` desaturates every lifted tile to greyscale + alpha and normalises
its range. The desaturation is not a choice: §11.2 **[SETTLED]** says no game system
names a colour, and a guard's yellow → orange → red *is* the AI state machine made
visible, so art carrying its own palette would leave the threat ladder nowhere to
live. What survives is the art's structure — luminance becomes the shading the
category tint multiplies through.
