#!/usr/bin/env python3
"""Seed `web/assets/tiles.png` and `web/assets/tiles.txt` — the **authoring sheet**
for the tile renderer (§11.1 / #460) and the slot table that names every cell of it.

    python3 scripts/seed-tileset.py [--force]

**This script seeds; it does not own.** It laid down the first version of a sheet
that is authored **by hand** from here on, so it refuses to overwrite an existing
sheet unless `--force` is passed — re-running it by reflex must never quietly discard
somebody's art. What it is still good for is re-cutting from the source sheets in
`web/assets/source/` after a change to the mapping, or starting over deliberately.

# The sheet

`SHEET_COLS` x `SHEET_ROWS` cells of `TILE` x `TILE`, most of them empty. The
headroom is the point: a slot's index is `row * SHEET_COLS + col` and **an index is
permanent**, exactly as an `AbilityId` slot is (see `CLAUDE.md`). Art is drawn against
a slot number, so moving one silently repaints every cell that referenced it. Claim
the next free slot; never close a gap.

The allocation, with room left after each band:

| Slots | Band |
|---|---|
| 0-15 | One sprite per **glyph** — what #460 step 1 draws |
| 16-31 | The **wall autotile** run, indexed by a neighbour bitmask — step 2 |
| 32+ | Free |

# The slot table

`web/assets/tiles.txt`, written beside the sheet and **hand-editable from here on**:
one line per allocated slot, `index`, `key`, description. It is not decoration —
`crates/web/src/tiles.rs` embeds it and a test asserts its own glyph -> index mapping
agrees with it, so the sheet, the table and the code cannot drift apart in silence.

# What the art has to satisfy

* PNG, RGBA. **Alpha carries the shape**; the colour channels carry *shading* as grey.
  The shell tints a sprite with the colour the glyph would have been drawn in, by
  multiplying the tint through the greys and restoring the alpha — so a flat white
  sprite comes out as the flat category colour, and a shaded one keeps its shading.
* Cells are **square**, and the shell squashes a sprite into the 14x20 glyph cell at
  draw time, so it comes out about 30% narrower than tall. That keeps the fit and
  every hit test untouched (§11.4); square *cells* are a much larger change.
* **Colour is not yours to choose** (§11.2 **[SETTLED]**): no game system names a
  colour, and a guard's yellow -> orange -> red *is* the AI state machine made
  visible. Art carrying its own palette would leave the threat ladder nowhere to
  live, which is why lifted source art is desaturated on the way in — its luminance
  becomes the shading, and the category supplies the hue.

There is deliberately no image library in play: the repo has no Python dependencies,
so this reads and writes PNG itself.
"""

import argparse
import pathlib
import struct
import zlib

# The sheet's geometry — mirrored by `crates/web/src/tiles.rs`.
SHEET_COLS = 16
SHEET_ROWS = 16
TILE = 48

# Where each band of slots starts. Bands are generous and their starts are fixed: a
# band filling up is answered by claiming free slots after 32, never by sliding the
# next band along (see the module note on permanence).
GLYPH_BASE = 0
WALL_BASE = 16

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "web" / "assets" / "source"
SHEET_OUT = ROOT / "web" / "assets" / "tiles.png"
TABLE_OUT = ROOT / "web" / "assets" / "tiles.txt"

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


# --- The slots ---------------------------------------------------------------

# The **glyph band** (slots 0-15): one sprite per glyph, in the order the render
# reference §2 walks them — building fabric and furniture first, then the goals a plan
# is drawn around, then the things that move. `crates/web/src/tiles.rs` holds the same
# mapping and a test asserts it against the written slot table.
#
# A source entry is `(sheet, tile index)` lifted from `web/assets/source/`; a callable
# is a crude placeholder shape below. **The source art supplies three of the fourteen
# and that is its honest yield**: its own tiles 1-15 are a wall autotile run, which
# step 1 refuses to use (one tile per glyph, no neighbour lookup), so only index 1 — a
# wall with no wall against it — says something a single tile can say. Index 16 is the
# floor. The player sheet is animation frames, so it gives frame 0.
GLYPHS = [
    ("#", "Wall", ("tiles", 1)),
    ("\u25a1", "Building fabric on the plans, never seen (\u00a711.5a)", schematic),
    ("+", "Door panel, closed", door_panel),
    ("\u00d7", "Door frame (hinge)", door_hinge),
    ("}", "Cupboard \u2014 a hiding place", cupboard),
    ("\u03c0", "Table \u2014 partial cover", table),
    ("=", "Duct mouth", duct),
    ("\u00b7", "Floor, inside your field of view", ("tiles", 16)),
    ("E", "The exit \u2014 your own tunnel's mouth", exit_tile),
    ("$", "Intel console", console),
    ("\u03a8", "Comms console", comms),
    ("@", "You, and a decoy you placed", ("player", 0)),
    ("g", "A guard you can see", guard),
    ("z", "A body", body),
]

# The **wall autotile band** (slots 16-31): a wall drawn for each combination of
# neighbouring walls, so step 2 can pick one by looking at four cells.
#
# The slot is the **bitmask itself** — N=1, E=2, S=4, W=8 — so the lookup is
# arithmetic rather than a table anybody has to keep. The source art's run is ordered
# differently (see `web/assets/source/README.md`), so it is mapped in here rather than
# copied across in its own order: doing that once, at the seam, is what buys step 2 an
# index it can compute. The source has no all-four-sides tile, so slot 31 is seeded
# empty and is the first thing worth drawing.
NORTH, EAST, SOUTH, WEST = 1, 2, 4, 8
WALL_SOURCE = {
    0: 1,                            # no wall against it
    EAST: 2,
    NORTH: 3,
    WEST: 4,
    SOUTH: 5,
    EAST | WEST: 6,
    NORTH | SOUTH: 7,
    SOUTH | EAST: 8,
    SOUTH | WEST: 9,
    NORTH | WEST: 10,
    NORTH | EAST: 11,
    NORTH | SOUTH | EAST: 12,
    EAST | WEST | SOUTH: 13,
    NORTH | WEST | SOUTH: 14,
    NORTH | WEST | EAST: 15,
    # NORTH | EAST | SOUTH | WEST has no source tile \u2014 slot 31 stays empty.
}


def sides(mask):
    """A bitmask spelled out, for the slot table: `N-E-S-W`, or `none`."""
    named = [n for bit, n in ((NORTH, "N"), (EAST, "E"), (SOUTH, "S"), (WEST, "W")) if mask & bit]
    return "-".join(named) or "none"


def slots():
    """Every allocated slot, as `(index, key, description, source or None)`.

    One list, walked by both outputs, so the sheet and the table it is described by
    are built from the same statement and cannot disagree.
    """
    out = []
    for i, (glyph, description, source) in enumerate(GLYPHS):
        out.append((GLYPH_BASE + i, f"glyph:{glyph}", description, source))
    for mask in range(16):
        source = WALL_SOURCE.get(mask)
        out.append((
            WALL_BASE + mask,
            f"wall:{mask}",
            f"Wall autotile \u2014 wall neighbours {sides(mask)}",
            ("tiles", source) if source is not None else None,
        ))
    return out


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


def write_table(path, allocated):
    """Write the slot table beside the sheet — the file an author reads to know what
    each cell of it is for, and the one `crates/web/src/tiles.rs` checks itself
    against."""
    lines = [
        "# Intrusion tileset — slot table",
        "#",
        "# One line per allocated slot of `tiles.png`: index, key, description.",
        "# A slot's index is `row * %d + col`, and **an index is permanent** — art is"
        % SHEET_COLS,
        "# drawn against a slot number, so moving one silently repaints every cell that",
        "# referenced it. Claim the next free slot; never close a gap.",
        "#",
        "# Bands, with room left after each. Free slots start at %d." % (WALL_BASE + 16),
        "#",
        "#   %3d-%-3d  one sprite per glyph (§11.3)" % (GLYPH_BASE, GLYPH_BASE + 15),
        "#   %3d-%-3d  the wall autotile run, keyed by a neighbour bitmask"
        % (WALL_BASE, WALL_BASE + 15),
        "#",
        "# A slot listed here with nothing drawn in it is a slot waiting for art; the",
        "# renderer falls back to drawing the character, which is never an error.",
        "#",
        "# Seeded by `scripts/seed-tileset.py`; hand-edited from here on.",
        "",
    ]
    for index, key, description, _ in allocated:
        lines.append(f"{index:<5} {key:<12} {description}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--force",
        action="store_true",
        help="overwrite an existing sheet. Off by default because the sheet is "
             "hand-authored once seeded, and a reflexive re-run must not discard art.",
    )
    args = ap.parse_args()
    if SHEET_OUT.exists() and not args.force:
        raise SystemExit(
            f"{SHEET_OUT} already exists — it is authored by hand from here on.\n"
            "Pass --force to re-seed it from the source art, discarding any drawing."
        )

    allocated = slots()
    width, height = SHEET_COLS * TILE, SHEET_ROWS * TILE
    canvas = [[(0, 0, 0, 0)] * width for _ in range(height)]
    drawn = 0
    for index, key, _description, source in allocated:
        if source is None:
            continue  # an allocated slot with no art yet; the renderer falls back
        tile = squared(source()) if callable(source) else lift(*source)
        ox, oy = (index % SHEET_COLS) * TILE, (index // SHEET_COLS) * TILE
        for y in range(TILE):
            for x in range(TILE):
                grey, alpha = tile.px[y][x]
                canvas[oy + y][ox + x] = (grey, grey, grey, alpha)
        drawn += 1
        origin = source.__name__ if callable(source) else f"{source[0]}[{source[1]}]"
        print(f"  {index:3}  {key:<12} {origin}")

    SHEET_OUT.write_bytes(png(width, height, canvas))
    write_table(TABLE_OUT, allocated)
    print(
        f"{SHEET_OUT}: {width}x{height}, {SHEET_COLS}x{SHEET_ROWS} slots of {TILE}px — "
        f"{drawn} drawn, {len(allocated) - drawn} allocated and empty, "
        f"{SHEET_COLS * SHEET_ROWS - len(allocated)} free"
    )
    print(f"{TABLE_OUT}: {len(allocated)} slots described")


if __name__ == "__main__":
    main()
