#!/usr/bin/env python3
"""Build `web/assets/tiles.png`, the sheet the tile renderer embeds (§11.1 / #460).

Run it from the repo root; it takes no arguments and overwrites its one output:

    python3 scripts/build-tileset.py

**The sheet is built, not drawn**, so it has provenance: every sprite on it is
either a tile lifted out of a source sheet in `web/assets/source/` or a crude
placeholder shape defined below, and `SPRITES` is the
single table saying which. `crates/web/src/tiles.rs` holds the same glyph -> index
mapping and its tests assert the set is complete, so the two cannot drift without
one of them failing.

**The sheet contract** (stated in prose in `docs/render-reference.md` §6):

* PNG, RGBA, `SHEET_COLS` cells per row, each cell `TILE` x `TILE` **square**. The
  shell squashes a sprite into the 14x20 glyph cell at draw time, so a sprite is
  authored square and comes out about 30% narrower than tall. Nothing about the fit
  or the hit tests changes, which is the point (§11.4); square *cells* are a much
  larger change and are step 2's problem.
* **Alpha carries the shape**, and the colour channels carry *shading* as grey. The
  shell tints a sprite with the colour the glyph would have been drawn in, by
  multiplying the tint through the greys and restoring the alpha — so a flat white
  sprite comes out as the flat category colour, and a shaded one keeps its shading.
* A sprite's index is `row * SHEET_COLS + col`, and this script writes them in the
  order of `SPRITES`.

**Source art is desaturated on the way in, and that is not negotiable** (§11.2
**[SETTLED]**): no game system names a colour, and a guard's yellow -> orange -> red
*is* the AI state machine made visible. Art carrying its own palette would leave the
threat ladder nowhere to live. What survives is the art's *structure* — the
luminance becomes the shading the tint multiplies through, so a dark fill with light
edge detail stays a mid tone with light edges rather than flattening to a block.

There is deliberately no image library in play: the repo has no Python dependencies,
so this reads and writes PNG itself.
"""

import pathlib
import struct
import zlib

# The built sheet's geometry — mirrored by `crates/web/src/tiles.rs`.
SHEET_COLS = 8
TILE = 48

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "web" / "assets" / "source"
OUT = ROOT / "web" / "assets" / "tiles.png"

# The source sheets, as they came out of the Godot experiment they were drawn for
# (#460). Kept verbatim rather than pre-cut, so what a sprite was lifted from is
# always checkable and a re-cut needs no new asset. Both are 48x48 grids; only the
# first row of each carries anything.
SOURCES = {
    "tiles": (SOURCE / "tiles.png", 48),
    "player": (SOURCE / "player.png", 48),
}


class Tile:
    """One sprite's pixel buffer: `(grey, alpha)` per pixel, origin top-left.

    Grey is the shading the tint multiplies through (255 = the full category
    colour), alpha the shape. Every primitive clips, so a shape that runs off the
    edge is cropped rather than wrapping into its neighbour.

    A tile need not be square while it is being drawn: the placeholder shapes below
    work in a `DRAW_W` x `DRAW_H` box — the *cell's* aspect — and are stretched to the
    sheet's square cell on the way out, so they read as drawn once the shell squashes
    them back. Lifted source art is square to begin with and skips the round trip.
    """

    def __init__(self, w=TILE, h=TILE):
        self.w, self.h = w, h
        self.px = [[(0, 0)] * w for _ in range(h)]

    def dot(self, x, y, grey=255, alpha=255):
        if 0 <= x < self.w and 0 <= y < self.h:
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


# --- Reading source art ------------------------------------------------------


def read_png(path):
    """Decode a PNG to `(width, height, rows of (r, g, b, a))`.

    Handles the two forms the source art actually uses — 8-bit palette with `tRNS`,
    and 8-bit RGBA — and refuses anything else loudly rather than guessing.
    """
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path}: not a PNG")
    pos, idat, palette, alphas = 8, bytearray(), None, None
    width = height = depth = colour = None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        payload = data[pos + 8 : pos + 8 + length]
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(
                ">IIBBBBB", payload
            )
            if depth != 8 or interlace or colour not in (3, 6):
                raise SystemExit(
                    f"{path}: unsupported PNG (depth {depth}, colour type {colour}, "
                    f"interlace {interlace}) — re-export as 8-bit palette or RGBA"
                )
        elif kind == b"PLTE":
            palette = payload
        elif kind == b"tRNS":
            alphas = payload
        elif kind == b"IDAT":
            idat += payload
        pos += 12 + length

    channels = 4 if colour == 6 else 1
    raw = zlib.decompress(bytes(idat))
    stride = width * channels
    lines, previous, at = [], bytearray(stride), 0
    for _ in range(height):
        kind = raw[at]
        at += 1
        line = bytearray(raw[at : at + stride])
        at += stride
        # Undo the per-scanline filter (PNG spec 9.2); `c` is the pixel up-left.
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b = previous[i]
            c = previous[i - channels] if i >= channels else 0
            if kind == 1:
                line[i] = (line[i] + a) & 0xFF
            elif kind == 2:
                line[i] = (line[i] + b) & 0xFF
            elif kind == 3:
                line[i] = (line[i] + (a + b) // 2) & 0xFF
            elif kind == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                nearest = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + nearest) & 0xFF
        lines.append(line)
        previous = line

    rows = []
    for line in lines:
        row = []
        for x in range(width):
            if colour == 6:
                r, g, b, a = line[x * 4 : x * 4 + 4]
            else:
                i = line[x]
                r, g, b = palette[i * 3 : i * 3 + 3]
                a = alphas[i] if alphas and i < len(alphas) else 255
            row.append((r, g, b, a))
        rows.append(row)
    return width, height, rows


def lift(sheet, index):
    """Tile `index` of a source sheet, desaturated to `(grey, alpha)`.

    Sources are laid out row-major at their own tile size and the tile is resampled
    to `TILE` by area-averaging — a no-op at the 48px both sheets happen to use, and
    correct if one ever arrives at another size.

    Grey is **luminance** (Rec. 601), which is what makes the desaturation preserve
    the art's read: an edge that was light against a dark fill stays light against a
    dark fill, and the tint then colours the whole thing by category.
    """
    path, size = SOURCES[sheet]
    width, _, rows = read_png(path)
    per_row = width // size
    ox, oy = (index % per_row) * size, (index // per_row) * size

    tile = Tile()
    for y in range(TILE):
        for x in range(TILE):
            # The source box this destination pixel covers.
            x0, x1 = ox + x * size // TILE, ox + max(x * size // TILE + 1, (x + 1) * size // TILE)
            y0, y1 = oy + y * size // TILE, oy + max(y * size // TILE + 1, (y + 1) * size // TILE)
            grey_sum = alpha_sum = count = 0
            for sy in range(y0, y1):
                for sx in range(x0, x1):
                    r, g, b, a = rows[sy][sx]
                    grey_sum += (299 * r + 587 * g + 114 * b) // 1000
                    alpha_sum += a
                    count += 1
            tile.px[y][x] = (grey_sum // count, alpha_sum // count)
    return stretch(tile)


def stretch(tile, floor=40):
    """Rescale a tile's greys so its darkest ink sits near `floor` and its lightest at
    full strength.

    **The tint needs range to work with.** Source art carries its own colour, so its
    luminance spread is whatever the palette happened to give it — the floor tiles are
    a dark navy with light detail, and taken raw they land in the bottom fifth of the
    scale. Multiplying a category colour through *that* produces mud: every tile comes
    out near-black, and the category the colour was carrying is lost with it.

    Normalising per tile fixes that without touching the art's *structure*, which is
    the half that has to survive (§11.2): an edge that was light against a dark fill is
    still light against a dark fill, it simply uses the whole range to say so. The
    floor keeps ink at all — it is never lifted to zero — because a sprite is shaded,
    not masked, and a tile that bottomed out at pure black would tint to a hole.
    """
    inked = [g for row in tile.px for (g, a) in row if a > 0]
    if not inked:
        return tile
    low, high = min(inked), max(inked)
    if high == low:
        # A flat tile has no range to stretch — the source floor is one solid navy.
        # Left alone it lands wherever its own palette put it, which for that tile is
        # dark enough that the tint takes it to black and the cell disappears. A flat
        # sprite is a *silhouette*, so give it the full category colour and let the
        # category decide how loud that is: Ground stays quiet because Ground's colour
        # is quiet, not because the art happened to be dark.
        for y in range(tile.h):
            for x in range(tile.w):
                grey, alpha = tile.px[y][x]
                if alpha:
                    tile.px[y][x] = (255, alpha)
        return tile
    span = 255 - floor
    for y in range(tile.h):
        for x in range(tile.w):
            grey, alpha = tile.px[y][x]
            if alpha:
                tile.px[y][x] = (floor + (grey - low) * span // (high - low), alpha)
    return tile


# --- Placeholder shapes ------------------------------------------------------
#
# Crude on purpose: a placeholder that looks finished is one nobody replaces. They are
# drawn in a `DRAW_W` x `DRAW_H` box — the 14x20 cell's own aspect at 2x — and
# stretched to the sheet's square cell on the way out, so they read as drawn once the
# shell squashes them back into the cell.
DRAW_W, DRAW_H = 28, 40


def wall():
    """`#` — a brick course. Dense: the wall is the loudest inert thing there is."""
    t = Tile(DRAW_W, DRAW_H)
    t.rect(0, 0, DRAW_W - 1, DRAW_H - 1, grey=210)
    # Mortar lines, every ten rows, offset course to course so it reads as masonry.
    for i, y in enumerate(range(9, DRAW_H, 10)):
        t.rect(0, y, DRAW_W - 1, y, grey=90)
        for x in range(14 if i % 2 else 0, DRAW_W, DRAW_W):
            t.rect(x, max(0, y - 9), x, y, grey=90)
    return t


def schematic():
    """`□` — building fabric on the plans, never yet seen. An outline: the shape of
    a wall without the substance of one (render reference §2.3)."""
    t = Tile(DRAW_W, DRAW_H)
    t.frame(3, 6, DRAW_W - 4, DRAW_H - 7)
    return t


def door_panel():
    """`+` — a closed door panel, with its handle."""
    t = Tile(DRAW_W, DRAW_H)
    t.rect(4, 2, DRAW_W - 5, DRAW_H - 3, grey=200)
    t.frame(4, 2, DRAW_W - 5, DRAW_H - 3)
    t.disc(DRAW_W - 9, DRAW_H // 2, 2)
    return t


def door_hinge():
    """`×` — the frame the panel swings in: a post, load-bearing."""
    t = Tile(DRAW_W, DRAW_H)
    t.rect(2, 0, 7, DRAW_H - 1, grey=170)
    t.rect(DRAW_W - 8, 0, DRAW_W - 3, DRAW_H - 1, grey=170)
    return t


def cupboard():
    """`}` — a locker you can climb into: an alcove with a split door."""
    t = Tile(DRAW_W, DRAW_H)
    t.frame(3, 3, DRAW_W - 4, DRAW_H - 4)
    t.rect(4, 4, DRAW_W - 5, DRAW_H - 5, grey=110)
    t.rect(DRAW_W // 2, 4, DRAW_W // 2, DRAW_H - 5, grey=230)
    t.disc(DRAW_W // 2 - 3, DRAW_H // 2, 1)
    t.disc(DRAW_W // 2 + 3, DRAW_H // 2, 1)
    return t


def table():
    """`π` — partial cover: a top and two legs, standing *in* a room."""
    t = Tile(DRAW_W, DRAW_H)
    t.rect(2, 12, DRAW_W - 3, 16, grey=220)
    t.rect(5, 17, 8, DRAW_H - 6, grey=160)
    t.rect(DRAW_W - 9, 17, DRAW_W - 6, DRAW_H - 6, grey=160)
    return t


def duct():
    """`=` — a duct mouth: a grille, slats across a recess."""
    t = Tile(DRAW_W, DRAW_H)
    t.frame(3, 8, DRAW_W - 4, DRAW_H - 9)
    for y in range(11, DRAW_H - 10, 4):
        t.rect(6, y, DRAW_W - 7, y + 1, grey=200)
    return t


def floor_dot():
    """`·` — floor inside your sight, and the quietest mark on the board."""
    t = Tile(DRAW_W, DRAW_H)
    t.disc(DRAW_W // 2, DRAW_H // 2, 2, grey=200)
    return t


def exit_tile():
    """`E` — your own tunnel's mouth: an arrow out, pointing up and away."""
    t = Tile(DRAW_W, DRAW_H)
    for i in range(9):
        t.rect(DRAW_W // 2 - i, 8 + i, DRAW_W // 2 + i, 8 + i)
    t.rect(DRAW_W // 2 - 3, 17, DRAW_W // 2 + 3, DRAW_H - 6)
    return t


def console():
    """`$` — an intel console: a screen on a stand, with its readout."""
    t = Tile(DRAW_W, DRAW_H)
    t.frame(3, 5, DRAW_W - 4, DRAW_H - 13)
    t.rect(5, 7, DRAW_W - 6, DRAW_H - 15, grey=120)
    for y in range(9, DRAW_H - 16, 3):
        t.rect(7, y, DRAW_W - 10, y, grey=240)
    t.rect(DRAW_W // 2 - 2, DRAW_H - 12, DRAW_W // 2 + 1, DRAW_H - 8, grey=180)
    t.rect(6, DRAW_H - 7, DRAW_W - 7, DRAW_H - 5, grey=180)
    return t


def comms():
    """`Ψ` — the comms console: the same stand, wearing an aerial."""
    t = Tile(DRAW_W, DRAW_H)
    t.rect(DRAW_W // 2 - 1, 2, DRAW_W // 2, 12)
    t.rect(DRAW_W // 2 - 7, 5, DRAW_W // 2 - 6, 12)
    t.rect(DRAW_W // 2 + 6, 5, DRAW_W // 2 + 7, 12)
    t.rect(DRAW_W // 2 - 7, 12, DRAW_W // 2 + 7, 13)
    t.frame(4, 16, DRAW_W - 5, DRAW_H - 8)
    t.rect(6, 18, DRAW_W - 7, DRAW_H - 10, grey=120)
    t.rect(6, DRAW_H - 6, DRAW_W - 7, DRAW_H - 4, grey=180)
    return t


def figure(head_r, shoulder, grey=255):
    """The shared body plan for the two standing entities — so the player and a
    guard read as the same *kind* of thing, told apart by build and by colour."""
    t = Tile(DRAW_W, DRAW_H)
    t.disc(DRAW_W // 2, 8, head_r, grey=grey)
    t.rect(DRAW_W // 2 - shoulder, 14, DRAW_W // 2 + shoulder, 27, grey=grey)
    t.rect(DRAW_W // 2 - 4, 28, DRAW_W // 2 - 2, DRAW_H - 4, grey=grey)
    t.rect(DRAW_W // 2 + 2, 28, DRAW_W // 2 + 4, DRAW_H - 4, grey=grey)
    return t


def player():
    """`@` — you: the narrower build, and the one the board is drawn around."""
    return figure(head_r=4, shoulder=5)


def guard():
    """`g` — a guard: broader at the shoulder, and wearing a helmet brim."""
    t = figure(head_r=5, shoulder=7)
    t.rect(DRAW_W // 2 - 7, 3, DRAW_W // 2 + 7, 4)
    return t


def body():
    """`z` — someone taken down: the same figure, lying along the floor."""
    t = Tile(DRAW_W, DRAW_H)
    t.disc(7, DRAW_H - 12, 4)
    t.rect(12, DRAW_H - 16, DRAW_W - 6, DRAW_H - 9, grey=220)
    t.rect(DRAW_W - 8, DRAW_H - 8, DRAW_W - 4, DRAW_H - 6, grey=180)
    return t


def squared(tile):
    """A drawing-box tile stretched to the sheet's square cell, nearest-neighbour.

    Nearest rather than smooth on purpose: these are hard-edged shapes, and the shell
    scales them again on the way to the screen, so softening them here would only
    compound.
    """
    out = Tile()
    for y in range(TILE):
        for x in range(TILE):
            out.px[y][x] = tile.px[y * tile.h // TILE][x * tile.w // TILE]
    return out


# --- The table ---------------------------------------------------------------

# Glyph -> where its sprite comes from, in **sheet index order**: fabric and furniture
# first, then the goals a plan is drawn around, then the things that move.
# `crates/web/src/tiles.rs` holds the same order.
#
# A source entry is `(sheet, tile index)`; a callable is a placeholder shape below.
#
# **The source art supplies three of the fourteen, and that is the honest yield**
# (see `web/assets/source/README.md`). Its tiles 1-15 are an *autotile* run keyed by
# which sides a wall joins, which is precisely what step 1 refuses to do — one tile per
# glyph, no neighbour lookup — so only index 1, a wall with no wall against it, says
# something a single tile can say. Index 16 is the floor and needs no neighbours at
# all. The player sheet is animation frames, so it gives frame 0 and nothing else.
# Everything else stays a crude placeholder until step 2's autotiler can spend the
# rest.
SPRITES = [
    ("#", ("tiles", 1)),  # a wall with no wall against it
    ("\u25a1", schematic),
    ("+", door_panel),
    ("\u00d7", door_hinge),
    ("}", cupboard),
    ("\u03c0", table),
    ("=", duct),
    ("\u00b7", ("tiles", 16)),  # the floor
    ("E", exit_tile),
    ("$", console),
    ("\u03a8", comms),
    ("@", ("player", 0)),  # the hooded figure, frame 0
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
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def main():
    rows_of_tiles = (len(SPRITES) + SHEET_COLS - 1) // SHEET_COLS
    width, height = SHEET_COLS * TILE, rows_of_tiles * TILE
    # A transparent sheet, then each sprite blitted into its slot: a cell the table
    # does not fill stays empty, and an empty slot is never referenced.
    canvas = [[(0, 0, 0, 0)] * width for _ in range(height)]
    for index, (glyph, source) in enumerate(SPRITES):
        tile = squared(source()) if callable(source) else lift(*source)
        ox, oy = (index % SHEET_COLS) * TILE, (index // SHEET_COLS) * TILE
        for y in range(TILE):
            for x in range(TILE):
                grey, alpha = tile.px[y][x]
                canvas[oy + y][ox + x] = (grey, grey, grey, alpha)
        origin = source.__name__ if callable(source) else f"{source[0]}[{source[1]}]"
        print(f"  {index:2}  {glyph}  {origin}")

    OUT.write_bytes(png(width, height, canvas))
    print(f"{OUT}: {width}x{height}, {len(SPRITES)} sprites in {rows_of_tiles} row(s)")


if __name__ == "__main__":
    main()
