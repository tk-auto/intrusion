#!/usr/bin/env python3
"""Generate the **placeholder** tileset at `web/assets/tiles.png` (§11.1 / #460).

The tile renderer needs a sheet before it has any art, and a sheet nobody can
regenerate is a binary blob with no provenance — so the placeholder is *code*, and
this script is the whole of it. Run it from the repo root:

    python3 scripts/make-placeholder-tiles.py

Real art replaces `web/assets/tiles.png` and this script goes with it; until then
the shapes here are the crude stand-ins that let `?tiles=1` show something.

**The sheet contract** (also stated in `docs/render-reference.md` §6, which is the
prose form and the thing art has to satisfy):

* PNG, RGBA, `SHEET_COLS` cells per row, cell `TILE_W` x `TILE_H` — the 14x20
  glyph cell at 2x, so nothing about the fit or the hit tests changes.
* **Alpha carries the shape**; the colour channels carry *shading* as grey. The
  shell tints a sprite with the colour the glyph would have been drawn in, by
  multiplying the tint through the greys and restoring the alpha — so a flat white
  sprite comes out as the flat category colour, and a shaded one keeps its shading.
* The index of a glyph's sprite is `row * SHEET_COLS + col`, and the glyph -> index
  table lives in `crates/web/src/tiles.rs`. This script writes the sprites in that
  same order, from `SPRITES` below, so the two cannot drift without the order here
  being edited.

There is deliberately no image library in play: the repo has no Python
dependencies and a placeholder generator is not worth acquiring one for, so this
writes the PNG itself (a single IDAT of filter-0 scanlines).
"""

import pathlib
import struct
import zlib

# The sheet geometry — mirrored by `crates/web/src/tiles.rs`.
SHEET_COLS = 8
TILE_W = 28
TILE_H = 40


class Tile:
    """One sprite's pixel buffer: `(grey, alpha)` per pixel, origin top-left.

    Grey is the shading the tint multiplies through (255 = the full category
    colour), alpha the shape. Every primitive clips to the tile, so a shape that
    runs off the edge is cropped rather than wrapping into its neighbour.
    """

    def __init__(self):
        self.px = [[(0, 0)] * TILE_W for _ in range(TILE_H)]

    def dot(self, x, y, grey=255, alpha=255):
        if 0 <= x < TILE_W and 0 <= y < TILE_H:
            self.px[y][x] = (grey, alpha)

    def rect(self, x0, y0, x1, y1, grey=255, alpha=255):
        """A filled rectangle, inclusive of both corners."""
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                self.dot(x, y, grey, alpha)

    def frame(self, x0, y0, x1, y1, grey=255, alpha=255):
        """A one-pixel outline — the shape a plan draws, not the substance."""
        for x in range(x0, x1 + 1):
            self.dot(x, y0, grey, alpha)
            self.dot(x, y1, grey, alpha)
        for y in range(y0, y1 + 1):
            self.dot(x0, y, grey, alpha)
            self.dot(x1, y, grey, alpha)

    def disc(self, cx, cy, r, grey=255, alpha=255):
        for y in range(cy - r, cy + r + 1):
            for x in range(cx - r, cx + r + 1):
                if (x - cx) ** 2 + (y - cy) ** 2 <= r * r:
                    self.dot(x, y, grey, alpha)


def wall():
    """`#` — a brick course. Dense: the wall is the loudest inert thing there is."""
    t = Tile()
    t.rect(0, 0, TILE_W - 1, TILE_H - 1, grey=210)
    # Mortar lines, every ten rows, offset course to course so it reads as masonry.
    for i, y in enumerate(range(9, TILE_H, 10)):
        t.rect(0, y, TILE_W - 1, y, grey=90)
        for x in range(14 if i % 2 else 0, TILE_W, TILE_W):
            t.rect(x, max(0, y - 9), x, y, grey=90)
    return t


def schematic():
    """`□` — building fabric on the plans, never yet seen. An outline: the shape of
    a wall without the substance of one (render reference §2.3)."""
    t = Tile()
    t.frame(3, 6, TILE_W - 4, TILE_H - 7)
    return t


def door_panel():
    """`+` — a closed door panel, with its handle."""
    t = Tile()
    t.rect(4, 2, TILE_W - 5, TILE_H - 3, grey=200)
    t.frame(4, 2, TILE_W - 5, TILE_H - 3)
    t.disc(TILE_W - 9, TILE_H // 2, 2)
    return t


def door_hinge():
    """`×` — the frame the panel swings in: a post, load-bearing."""
    t = Tile()
    t.rect(2, 0, 7, TILE_H - 1, grey=170)
    t.rect(TILE_W - 8, 0, TILE_W - 3, TILE_H - 1, grey=170)
    return t


def cupboard():
    """`}` — a locker you can climb into: an alcove with a split door."""
    t = Tile()
    t.frame(3, 3, TILE_W - 4, TILE_H - 4)
    t.rect(4, 4, TILE_W - 5, TILE_H - 5, grey=110)
    t.rect(TILE_W // 2, 4, TILE_W // 2, TILE_H - 5, grey=230)
    t.disc(TILE_W // 2 - 3, TILE_H // 2, 1)
    t.disc(TILE_W // 2 + 3, TILE_H // 2, 1)
    return t


def table():
    """`π` — partial cover: a top and two legs, standing *in* a room."""
    t = Tile()
    t.rect(2, 12, TILE_W - 3, 16, grey=220)
    t.rect(5, 17, 8, TILE_H - 6, grey=160)
    t.rect(TILE_W - 9, 17, TILE_W - 6, TILE_H - 6, grey=160)
    return t


def duct():
    """`=` — a duct mouth: a grille, slats across a recess."""
    t = Tile()
    t.frame(3, 8, TILE_W - 4, TILE_H - 9)
    for y in range(11, TILE_H - 10, 4):
        t.rect(6, y, TILE_W - 7, y + 1, grey=200)
    return t


def floor_dot():
    """`·` — floor inside your sight, and the quietest mark on the board."""
    t = Tile()
    t.disc(TILE_W // 2, TILE_H // 2, 2, grey=200)
    return t


def exit_tile():
    """`E` — your own tunnel's mouth: an arrow out, pointing up and away."""
    t = Tile()
    for i in range(9):
        t.rect(TILE_W // 2 - i, 8 + i, TILE_W // 2 + i, 8 + i)
    t.rect(TILE_W // 2 - 3, 17, TILE_W // 2 + 3, TILE_H - 6)
    return t


def console():
    """`$` — an intel console: a screen on a stand, with its readout."""
    t = Tile()
    t.frame(3, 5, TILE_W - 4, TILE_H - 13)
    t.rect(5, 7, TILE_W - 6, TILE_H - 15, grey=120)
    for y in range(9, TILE_H - 16, 3):
        t.rect(7, y, TILE_W - 10, y, grey=240)
    t.rect(TILE_W // 2 - 2, TILE_H - 12, TILE_W // 2 + 1, TILE_H - 8, grey=180)
    t.rect(6, TILE_H - 7, TILE_W - 7, TILE_H - 5, grey=180)
    return t


def comms():
    """`Ψ` — the comms console: the same stand, wearing an aerial."""
    t = Tile()
    t.rect(TILE_W // 2 - 1, 2, TILE_W // 2, 12)
    t.rect(TILE_W // 2 - 7, 5, TILE_W // 2 - 6, 12)
    t.rect(TILE_W // 2 + 6, 5, TILE_W // 2 + 7, 12)
    t.rect(TILE_W // 2 - 7, 12, TILE_W // 2 + 7, 13)
    t.frame(4, 16, TILE_W - 5, TILE_H - 8)
    t.rect(6, 18, TILE_W - 7, TILE_H - 10, grey=120)
    t.rect(6, TILE_H - 6, TILE_W - 7, TILE_H - 4, grey=180)
    return t


def figure(head_r, shoulder, grey=255):
    """The shared body plan for the two standing entities — so the player and a
    guard read as the same *kind* of thing, told apart by build and by colour."""
    t = Tile()
    t.disc(TILE_W // 2, 8, head_r, grey=grey)
    t.rect(TILE_W // 2 - shoulder, 14, TILE_W // 2 + shoulder, 27, grey=grey)
    t.rect(TILE_W // 2 - 4, 28, TILE_W // 2 - 2, TILE_H - 4, grey=grey)
    t.rect(TILE_W // 2 + 2, 28, TILE_W // 2 + 4, TILE_H - 4, grey=grey)
    return t


def player():
    """`@` — you: the narrower build, and the one the board is drawn around."""
    return figure(head_r=4, shoulder=5)


def guard():
    """`g` — a guard: broader at the shoulder, and wearing a helmet brim."""
    t = figure(head_r=5, shoulder=7)
    t.rect(TILE_W // 2 - 7, 3, TILE_W // 2 + 7, 4)
    return t


def body():
    """`z` — someone taken down: the same figure, lying along the floor."""
    t = Tile()
    t.disc(7, TILE_H - 12, 4)
    t.rect(12, TILE_H - 16, TILE_W - 6, TILE_H - 9, grey=220)
    t.rect(TILE_W - 8, TILE_H - 8, TILE_W - 4, TILE_H - 6, grey=180)
    return t


# The sheet's layout, in index order: fabric and furniture first, then the goals a
# plan is drawn around, then the things that move. `crates/web/src/tiles.rs` holds
# the same glyph -> index mapping, and its tests assert the set is complete.
SPRITES = [
    ("#", wall),
    ("□", schematic),
    ("+", door_panel),
    ("×", door_hinge),
    ("}", cupboard),
    ("π", table),
    ("=", duct),
    ("·", floor_dot),
    ("E", exit_tile),
    ("$", console),
    ("Ψ", comms),
    ("@", player),
    ("g", guard),
    ("z", body),
]


def png(width, height, rows):
    """Encode RGBA `rows` (a list of rows of `(r, g, b, a)`) as a PNG."""

    def chunk(kind, payload):
        return (
            struct.pack(">I", len(payload))
            + kind
            + payload
            + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
        )

    raw = bytearray()
    for row in rows:
        raw.append(0)  # filter type 0 (None) — the sheet is tiny, so don't bother
        for r, g, b, a in row:
            raw += bytes((r, g, b, a))
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def main():
    rows_of_tiles = (len(SPRITES) + SHEET_COLS - 1) // SHEET_COLS
    width = SHEET_COLS * TILE_W
    height = rows_of_tiles * TILE_H
    # A transparent sheet, then each sprite blitted into its slot: the cells the
    # table does not fill stay empty, and an empty slot is never referenced.
    canvas = [[(0, 0, 0, 0)] * width for _ in range(height)]
    for index, (_glyph, draw) in enumerate(SPRITES):
        tile = draw()
        ox = (index % SHEET_COLS) * TILE_W
        oy = (index // SHEET_COLS) * TILE_H
        for y in range(TILE_H):
            for x in range(TILE_W):
                grey, alpha = tile.px[y][x]
                canvas[oy + y][ox + x] = (grey, grey, grey, alpha)

    out = pathlib.Path(__file__).resolve().parent.parent / "web" / "assets" / "tiles.png"
    out.write_bytes(png(width, height, canvas))
    print(f"{out}: {width}x{height}, {len(SPRITES)} sprites in {rows_of_tiles} row(s)")


if __name__ == "__main__":
    main()
