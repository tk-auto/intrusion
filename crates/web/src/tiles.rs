//! The **tile renderer** (§11.1 / #460, #461) — the shell's second cell primitive, and
//! the whole of what `?tiles=1` turns on.
//!
//! §11.1 **[SETTLED]** says the renderer is *"a separate concern behind one
//! interface. ASCII now; a tile renderer later is a second implementation of the
//! same interface, swapping `fillText` for `drawImage`. The core must not know which
//! is in use."* This module is that second implementation, and the sentence is also
//! its boundary: nothing here reads a [`State`](intrusion_core::State), nothing here
//! decides a glyph, and nothing in `crates/core` changed to make it possible. The
//! grid the core emits is identical with tiles on and off — what differs is only
//! whether [`crate::paint`] answers a cell with `fill_text` or with `draw_image`.
//!
//! # A sprite per glyph, tinted by the colour the glyph would have been
//!
//! Step 1's rule was **a sprite is chosen by the cell's glyph and by nothing else**.
//! The colour is not chosen here either: [`crate::paint`] has already resolved the
//! cell to a concrete colour through the §11.2 table (the category's `fg` live, its
//! `dim` beyond the FOV, the memory slate for a remembered content), and the tile is
//! drawn *in that colour*. So a tile and a glyph carry exactly the same information,
//! in all three knowledge states and both themes, and swapping between them can never
//! change what a cell means.
//!
//! **Step 2 lets a sprite be turned** (#461), which is the first thing to widen that
//! rule, and it widens it in exactly two places:
//!
//! - **Autotiling.** A glyph that reads as a continuous surface ([`AUTOTILED`] — the
//!   wall, so far) picks its sprite from which of its four neighbours draw the same
//!   glyph, so runs join and corners turn. The neighbours come from the **grid this
//!   module was handed** and from nowhere else; see [`neighbour_mask`] for why that
//!   sentence is the whole of the fog safety.
//! - **Facing.** A cell that declares a [`GlyphCell::facing`] is drawn turned to face
//!   it. That is information the character grid does not carry — the ticket's one
//!   deliberate *addition*, licensed by §5 making "you cannot see behind you" a rule —
//!   and the core decides who gets one, so a sensed guard cannot acquire a facing by
//!   the tileset's choice.
//!
//! Both are **rotations of one sprite**, never separate art: sixteen wall
//! neighbourhoods are six shapes at four angles, and an actor is one shape at four.
//! The sheet stores what cannot be derived and the draw path turns the rest
//! ([`canonical`], [`turned`]).
//!
//! What is still refused: animation, and any sprite chosen by anything the grid does
//! not say.
//!
//! Sprites are authored **greyscale with alpha**: the alpha channel is the shape and
//! the greys are shading. Tinting bakes a whole copy of the sheet per colour
//! ([`Atlases::tinted`]) rather than compositing per cell — a 40x40 board every frame
//! is the one thing that would make this expensive — and the colour set is closed and
//! small (~10 categories x 3 knowledge states x 2 themes, and most of them share the
//! one dim gray), so the cache is bounded by construction. It is filled lazily: a
//! session that never flips the theme never bakes the other one.
//!
//! # What tiles deliberately do *not* touch
//!
//! **Backgrounds.** The §11.5 danger overlay, the §9.2 sensed cue and the §8.3 effect
//! wash are `fill_rect`s painted before the glyph layer, and this module never sees
//! them. The board's most important read is therefore entirely outside the blast
//! radius of the tile mode — with tiles on it paints identically, because it is
//! literally the same code.
//!
//! **The cell size.** A sprite is drawn *into* the 14x20 glyph cell, squashed from the
//! square [`TILE`] it was authored at, so `fit_and_draw`'s arithmetic and every hit
//! test — the help button, the ability bar, the tab bar — are untouched. Square
//! *cells* would mean the map keeping its own metric while the HUD rows keep the text
//! one, which breaks the single-grid fit.
//!
//! #461 is where that was supposed to be reopened — corners and joins are what make a
//! non-square tile look wrong — and it is **refused again**, on the evidence of the
//! joins themselves: the wall run's boundary lines sit on the cell's edges, so the
//! squash stretches where the line *is* without moving it, and a room still reads as a
//! rectangle with a lit border. Nothing about the autotiling wants a square cell; what
//! would want one is art with circles in it. The cost of the change has not moved
//! (the map gets its own metric, every hit test in §11.4 gets a second one), so the
//! trade is still bad and this ticket is not the one to make it.
//!
//! **Text.** Tiling letters would be a font, not a tileset, so a tile is drawn only
//! where the glyph *is the world*: on cells the core tags [`Surface::Board`]. The near
//! line, the usable line, the ability bar, the panels and the deployed log are
//! [`Surface::Chrome`] and always draw text.
//!
//! That distinction is per **cell** and not per row, and it has to be. The deployed
//! message log (§11.7) and the verdict card (§14 v2) lay prose *across the map rows*,
//! so "which rows are the map" is the wrong question — asked that way, every `g` in
//! "a guard has seen you" sprouts a guard sprite. The core already knew the answer
//! (it is the difference between its two cell constructors); #460 only gave it a
//! name.
//!
//! # Falling back is the normal case, not the error case
//!
//! Three things make a cell draw as a character instead: the glyph has no sprite, the
//! sheet has not finished decoding, or the mode is off. All three take the same path
//! and none of them is a failure — **a glyph the table has never heard of must render
//! as itself, never as a hole**, so the mapping is safe to be incomplete and a glyph
//! added by a later ticket costs nothing until somebody draws it.
//!
//! # Where the sheet comes from
//!
//! Embedded ([`SHEET`]) and handed to the browser as a `data:` URI. The artifact build
//! packs a single self-contained page under a CSP that blocks every external request,
//! so it could not fetch a `.png` even if one shipped beside it; embedding serves the
//! Pages deploy through the identical code path rather than growing a second one.
//! Decoding is asynchronous, so the first frames of a `?tiles=1` load paint as text
//! and the shell redraws once the sheet lands ([`install`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;
use std::rc::Rc;

use intrusion_core::{Direction, GlyphCell, Grid, Surface};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement};

use crate::{Game, Metrics};

/// The tileset, embedded in the wasm (see the module note on delivery). An
/// **authoring sheet**, hand-drawn from here on and seeded once by
/// `scripts/seed-tileset.py`; `docs/render-reference.md` §6 is the prose form of the
/// contract it satisfies.
const SHEET: &[u8] = include_bytes!("../../../web/assets/tiles.png");

/// The **slot table** beside the sheet, naming every allocated cell of it. Embedded
/// so the tests below can hold [`SPRITES`] against it: the sheet, the table an
/// artist draws against and this mapping are three statements of one fact, and
/// nothing but a test stops them drifting apart in silence.
///
/// Test-only, and that is the right shape: the running game reads the mapping, and
/// the table's job is to make sure the mapping is the one somebody drew against.
#[cfg(test)]
const SLOT_TABLE: &str = include_str!("../../../web/assets/tiles.txt");

/// The sheet's cells per row. A sprite's index is `row * SHEET_COLS + col`, so the
/// layout is a pure function of the index and adding a sprite never moves an
/// existing one.
///
/// **An index is permanent**, for the reason an `AbilityId` slot is (`CLAUDE.md`):
/// art is drawn against a slot number, so moving one silently repaints every cell
/// that referenced it. The sheet is mostly empty on purpose — claim the next free
/// slot, never close a gap.
const SHEET_COLS: u32 = 16;

/// The sheet's rows, and with [`SHEET_COLS`] its whole capacity. Mostly empty and
/// meant to stay that way for a while: the headroom is what lets a slot number be
/// permanent, since claiming the next free one is always available.
///
/// Test-only, like [`SLOT_TABLE`]: the draw path indexes a slot and never asks how
/// many there are, so this exists to bound what the mapping may claim.
#[cfg(test)]
const SHEET_ROWS: u32 = 16;

/// One sprite's source size on the sheet. **Square**, while the destination cell is
/// the 14x20 glyph box ([`crate::CELL_W`]/[`crate::CELL_H`]) scaled to the fit — so a
/// sprite is squashed about 30% narrower than it was drawn.
///
/// That is the deliberate trade #460 makes. Authoring square is what tile art *is*,
/// and squashing at draw time costs nothing but the aspect; the alternative — giving
/// the map its own square metric while the §11.4 HUD rows keep the text one — breaks
/// the single-grid fit that every hit test is built on, and is step 2's problem.
///
/// Kept at the source art's own 48px rather than pre-scaled to the cell: the board is
/// routinely fitted *larger* than that, so downsampling here would throw away
/// resolution the browser then wishes it had.
const TILE: u32 = 48;

/// How many rows of the sheet a tinted atlas has to cover — enough for the highest
/// slot the mapping can index, and not one row more.
///
/// The sheet is an **authoring surface with headroom**, so most of it is empty and
/// will stay that way for a long time. Baking that emptiness would be the one place
/// the headroom cost something real: an atlas per colour, at the full sheet size, is
/// megabytes of canvas apiece for pixels no cell ever samples. Derived rather than
/// written down, so growing the mapping grows the atlas by itself.
const fn atlas_rows() -> u32 {
    let (mut highest, mut i) = (0, 0);
    while i < SPRITES.len() {
        if SPRITES[i].1 > highest {
            highest = SPRITES[i].1;
        }
        i += 1;
    }
    // An autotile band is indexed `base + mask`, so its last reachable slot is the
    // all-four-neighbours one — 15 past its base, whether or not that mask's sprite is
    // one the sheet draws or one it turns.
    let mut i = 0;
    while i < AUTOTILED.len() {
        if AUTOTILED[i].1 + 15 > highest {
            highest = AUTOTILED[i].1 + 15;
        }
        i += 1;
    }
    highest / SHEET_COLS + 1
}

/// The URL field that turns the tile mode on, and the two values it answers to.
///
/// **Unlike the debug activation next door ([`crate::debug`]), this is never stripped**
/// (#460): a debug session is a thing you *do* once, but tiles are a presentation
/// preference, and a preference that vanished on reload would not be one. It is read
/// from the **query** and not the hash, because the hash belongs to the run's own
/// level-seed token — `seed::reflect_level` rewrites it the moment a run starts, and a
/// preference parked there would be overwritten by the first frame.
///
/// It graduates to the options screen (§14 v2) later; the flag is the temporary form.
const TILES_FIELD: &str = "tiles";
const TILES_ON: &str = "1";
const TILES_OFF: &str = "0";

/// The glyph -> sprite table. **Incomplete is a legal state** — a glyph absent here
/// draws as a character (see the module note) — so this lists what has art rather
/// than what exists.
///
/// These are the sheet's **glyph band**, slots 0-15, in the order the render
/// reference §2 walks them: building fabric and furniture first, then the goals a
/// plan is drawn around, then the things that move. Slots 16-31 are the wall
/// autotile run, indexed through [`AUTOTILED`] instead — a glyph that autotiles keeps
/// its entry here as the sprite it draws when nothing else applies.
///
/// The tests below assert this covers every glyph `docs/render-reference.md` §2
/// lists, *and* that each entry matches what [`SLOT_TABLE`] says that slot is for.
const SPRITES: [(char, u32); 14] = [
    ('#', 0),  // wall
    ('□', 1),  // the schematic's building fabric (§11.5a)
    ('+', 2),  // a closed door panel
    ('×', 3),  // a door frame
    ('}', 4),  // a cupboard
    ('π', 5),  // a table (partial cover)
    ('=', 6),  // a duct mouth
    ('·', 7),  // floor, inside your sight
    ('E', 8),  // the exit
    ('$', 9),  // an intel console
    ('Ψ', 10), // the comms console
    ('@', 11), // you, and a decoy you placed
    ('g', 12), // a guard you can see
    ('z', 13), // a body
];

/// The sprite index for a glyph, or `None` when the sheet has no art for it — the
/// text fallback, and the reason the table above is safe to be incomplete.
fn sprite_index(glyph: char) -> Option<u32> {
    SPRITES
        .iter()
        .find(|(g, _)| *g == glyph)
        .map(|(_, index)| *index)
}

/// The glyphs that **autotile**, and where each one's run starts (#461).
///
/// A glyph listed here reads as a *continuous surface*: its sprite is chosen from
/// which of its four neighbours draw the same glyph, so runs join up and corners
/// turn. Everything not listed keeps step 1's rule — one sprite, chosen by the glyph
/// and nothing else.
///
/// Only the wall so far, because only the wall has a run drawn for it. The
/// schematic's `□` is the obvious next one — it is exactly as continuous — and
/// costs a band and nothing else when somebody draws it.
const AUTOTILED: [(char, u32); 1] = [('#', 16)];

/// Where `glyph`'s autotile run starts, or `None` for a glyph that draws one sprite.
fn autotile_base(glyph: char) -> Option<u32> {
    AUTOTILED
        .iter()
        .find(|(g, _)| *g == glyph)
        .map(|(_, base)| *base)
}

/// The four neighbour bits of an autotile mask, **clockwise from north** — the same
/// order and the same sense as [`Direction::ALL`], so "turn the mask a quarter" and
/// "turn the tile a quarter" are the same operation ([`turn_mask`]).
const NORTH: u8 = 1;
const EAST: u8 = 2;
const SOUTH: u8 = 4;
const WEST: u8 = 8;

/// The **six masks the sheet actually draws** — one per rotation orbit of the sixteen
/// (#461). Every other mask is one of these turned, so the sheet holds the shape once
/// and the draw path turns it:
///
/// | Slot | Mask | Shape |
/// |---|---|---|
/// | 16 | none | an isolated block, exposed on all four sides |
/// | 17 | N | an end cap |
/// | 19 | N-E | a corner |
/// | 21 | N-S | a straight run |
/// | 23 | N-E-S | a T |
/// | 31 | N-E-S-W | a crossing, and the plain interior of a mass of wall |
///
/// **The other ten slots of the band are tombstones**, not free space: the band's
/// index is still `base + mask`, and slot 18 is still where mask `E` would live if a
/// later ticket ever drew it a sprite of its own rather than turning slot 17. They are
/// listed in `web/assets/tiles.txt` as the rotations they are, for the reason a
/// retired `AbilityId` keeps its slot (`CLAUDE.md`) — a gap that closes is a gap that
/// silently repaints its neighbours.
///
/// Derived by [`canonical`] rather than written down, and so **test-only** — like
/// [`SLOT_TABLE`], it exists to be asserted against, and to be the one place a reader
/// can see the layout stated rather than computed.
#[cfg(test)]
const CANONICAL: [u8; 6] = [
    0,
    NORTH,
    NORTH | EAST,
    NORTH | SOUTH,
    NORTH | EAST | SOUTH,
    15,
];

/// The **rest facing** every directional sprite is drawn in: a sprite that faces
/// anywhere is drawn facing **south**, down the screen, and turned from there
/// (§11.1/#461).
///
/// South rather than north because that is how the source art was drawn — a top-down
/// figure faces the viewer — and picking the art's own rest costs no rotation in the
/// commonest case a player looks at.
const REST_FACING: Direction = Direction::South;

/// One sprite, ready to draw: where it is on the sheet and how far round it goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Sprite {
    /// The slot index on the sheet.
    index: u32,
    /// Quarter turns **clockwise** to draw it through, 0-3. Zero is the overwhelming
    /// majority of cells and costs no canvas transform at all ([`Tiles::draw`]).
    turns: u8,
}

/// A neighbour mask turned one quarter **clockwise**: N→E→S→W→N, the same rotation
/// [`Direction::clockwise`] describes and the same one the canvas applies.
fn turn_mask(mask: u8) -> u8 {
    (mask << 1 | mask >> 3) & 0xf
}

/// The mask's **orbit representative and the turns back to it**: the smallest mask
/// this one can be turned into, and how many quarter turns clockwise take that
/// representative back to `mask`.
///
/// This is the whole of the deduplication (#461). Sixteen neighbourhoods, six shapes:
/// the four end caps are one sprite at four angles, the four corners another, the two
/// straights a third. Turning at draw time is what lets the sheet hold six images and
/// the band still be indexed by a plain bitmask.
fn canonical(mask: u8) -> (u8, u8) {
    let (mut rep, mut turns) = (mask, 0);
    let mut candidate = mask;
    for back in 0..4 {
        // `candidate` is `mask` turned `back` quarters *anticlockwise*, so turning it
        // `back` quarters clockwise is `mask` again — the invariant a test pins.
        if candidate < rep {
            rep = candidate;
            turns = back;
        }
        candidate = turn_mask(turn_mask(turn_mask(candidate)));
    }
    (rep, turns)
}

/// The quarter turns that take a sprite from its [`REST_FACING`] to `facing`.
fn turns_to_face(facing: Direction) -> u8 {
    let mut turns = 0;
    let mut at = REST_FACING;
    while at != facing {
        at = at.clockwise();
        turns += 1;
    }
    turns
}

/// Which of `(x, y)`'s four neighbours draw the same glyph it does — the autotile
/// mask, computed from **the grid the renderer was handed and from nothing else**.
///
/// **This is the ticket's whole fog risk** (#461/§11.5a). Geometry the player has
/// never seen is masked by the core as the schematic's `□`, not as `#`, so a wall
/// cannot join to it: the join follows the *drawn* glyph, which means the shape
/// channel can say no more than the glyph channel already said. Ask the `State`
/// instead — "is there really a wall there?" — and the masking is defeated through
/// shape while glyph and colour are still telling the truth, which is precisely the
/// leak §2.3 warns about and is invisible in a screenshot because it looks like
/// better art.
///
/// Two neighbours never join, for the same reason: one off the grid, and one on the
/// **chrome** surface. A message log laid across the map rows (§11.7) must not weld a
/// sentence's `#` onto the facility's wall.
fn neighbour_mask(grid: &Grid, x: u32, y: u32, glyph: char) -> u8 {
    const STEPS: [(u8, i64, i64); 4] = [(NORTH, 0, -1), (EAST, 1, 0), (SOUTH, 0, 1), (WEST, -1, 0)];
    STEPS
        .iter()
        .filter(|(_, dx, dy)| joins(grid, x, y, *dx, *dy, glyph))
        .fold(0, |mask, (bit, _, _)| mask | bit)
}

/// Whether the cell `(dx, dy)` away from `(x, y)` continues `glyph`'s surface.
fn joins(grid: &Grid, x: u32, y: u32, dx: i64, dy: i64, glyph: char) -> bool {
    let (Ok(nx), Ok(ny)) = (
        u32::try_from(i64::from(x) + dx),
        u32::try_from(i64::from(y) + dy),
    ) else {
        return false; // off the top or left edge
    };
    if nx >= grid.width() || ny >= grid.height() {
        return false; // off the bottom or right edge
    }
    let neighbour = grid.get(nx, ny);
    neighbour.surface == Surface::Board && neighbour.glyph == glyph
}

/// The tile layer: whether the mode is on, and — once the browser has decoded the
/// sheet — the art and its tinted copies.
pub(crate) struct Tiles {
    /// Whether `?tiles=1` (or a baked build) asked for tiles at all. Fixed at boot:
    /// the mode is a property of the load, not something a run can change.
    on: bool,
    /// The decoded sheet and the tinted atlases baked from it. Interior-mutable
    /// because painting is `&self` all the way down and the cache fills *during* a
    /// paint — the first frame in a new colour is the one that bakes it.
    atlases: RefCell<Atlases>,
}

/// The sheet, once it has decoded, and one tinted copy of it per colour.
#[derive(Default)]
struct Atlases {
    /// `None` until the `data:` URI finishes decoding. Every draw is a fallback to
    /// text until then, which is what makes an async load invisible.
    sheet: Option<HtmlImageElement>,
    /// Tinted copies, keyed by the colour string [`crate::paint`] resolved. The key
    /// is the colour and not the (category, knowledge state, theme) triple on
    /// purpose: rows share shades — most of them share the one standard dim — so
    /// keying by what is actually drawn collapses the duplicates for free.
    tinted: HashMap<&'static str, HtmlCanvasElement>,
}

impl Tiles {
    /// Read the tile mode for this load: the URL's `?tiles=` field if it states one,
    /// otherwise whatever the build baked in, otherwise off.
    ///
    /// The URL wins **in both directions**, so `?tiles=0` turns a baked-on build back
    /// off. That is not symmetry for its own sake: the artifact host strips the hash
    /// and frames the page, so a preview build stamps the mode in rather than being
    /// asked for it, and the override is the only way to see the text renderer in the
    /// very build the tile renderer is being judged against.
    pub(crate) fn boot() -> Self {
        Self {
            on: url_choice().unwrap_or_else(baked),
            atlases: RefCell::new(Atlases::default()),
        }
    }

    /// The tile layer to paint with, or `None` when this load draws as pure text —
    /// the flag's whole job, and what makes the text renderer byte-identical with the
    /// flag absent.
    pub(crate) fn layer(&self) -> Option<&Self> {
        self.on.then_some(self)
    }

    /// The sprite cell `(x, y)` would be drawn with, and how far round, or `None` when
    /// it draws as a character: the mode is off, the cell is chrome, or its glyph has
    /// no art.
    ///
    /// **The whole tile/text decision, in one place**, so [`Tiles::draw`] and the
    /// tests below are asking the same question rather than two questions that agree
    /// today. The one condition it does not answer is whether the sheet has finished
    /// decoding — that is a fact about the moment, not about the cell.
    ///
    /// It takes the **grid** and a position rather than a cell (#461), because a
    /// continuous surface is drawn from its neighbourhood: that argument is the seam
    /// the fog constraint lives on, and taking nothing else is what makes
    /// [`neighbour_mask`]'s promise checkable.
    fn sprite_for(&self, grid: &Grid, x: u32, y: u32) -> Option<Sprite> {
        let cell = grid.get(x, y);
        self.sprite_for_cell(cell, || neighbour_mask(grid, x, y, cell.glyph))
    }

    /// The decision itself, given the cell and — lazily, because only a continuous
    /// surface ever asks — the neighbourhood it sits in.
    ///
    /// Split from [`Tiles::sprite_for`] so the two halves can be tested for what each
    /// is: this one against cells a test can state outright, and [`neighbour_mask`]
    /// against grids the core actually rendered.
    fn sprite_for_cell(&self, cell: GlyphCell, mask: impl FnOnce() -> u8) -> Option<Sprite> {
        if !self.on || cell.surface != Surface::Board {
            return None;
        }
        if let Some(base) = autotile_base(cell.glyph) {
            let (shape, turns) = canonical(mask());
            return Some(Sprite {
                index: base + u32::from(shape),
                turns,
            });
        }
        Some(Sprite {
            index: sprite_index(cell.glyph)?,
            // A cell that faces somewhere is drawn turned to face it (§11.1/#461).
            // Asked of every glyph rather than of a list of actors: a facing is a fact
            // about the cell, and the core only ever writes one on something with a
            // front (the player, a guard it can see).
            turns: cell.facing.map_or(0, turns_to_face),
        })
    }

    /// Take the decoded sheet. Any tint baked from an earlier sheet is dropped with
    /// it, so the cache can never hold a copy of art that is no longer the art.
    fn accept(&self, sheet: HtmlImageElement) {
        let mut atlases = self.atlases.borrow_mut();
        atlases.sheet = Some(sheet);
        atlases.tinted.clear();
    }

    /// Draw `cell` at `(x, y)` as a tile, in `colour`. Answers **`false` when it drew
    /// nothing** — the cell is chrome, its glyph has no sprite, or the sheet has not
    /// landed yet — and the caller then draws the character instead. That return is
    /// the text fallback, and it is the normal path rather than an error path.
    pub(crate) fn draw(
        &self,
        ctx: &CanvasRenderingContext2d,
        grid: &Grid,
        x: u32,
        y: u32,
        colour: &'static str,
        m: &Metrics,
    ) -> bool {
        let Some(sprite) = self.sprite_for(grid, x, y) else {
            return false;
        };
        let Some(atlas) = self.atlases.borrow_mut().tinted(colour) else {
            return false;
        };
        let (sx, sy) = (
            f64::from((sprite.index % SHEET_COLS) * TILE),
            f64::from((sprite.index / SHEET_COLS) * TILE),
        );
        let (dx, dy) = (f64::from(x) * m.cell_w, f64::from(y) * m.cell_h);
        // Errors here mean an invalid surface, the same condition `fill_text` ignores;
        // there is nothing a frame can do about it and nothing to fall back *to*.
        if sprite.turns == 0 {
            return ctx
                .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    &atlas,
                    sx,
                    sy,
                    f64::from(TILE),
                    f64::from(TILE),
                    dx,
                    dy,
                    m.cell_w,
                    m.cell_h,
                )
                .is_ok();
        }
        ctx.save();
        let drawn = turned(ctx, &atlas, (sx, sy), (dx, dy), sprite.turns, m).is_ok();
        ctx.restore();
        drawn
    }
}

/// Draw a sprite through `turns` quarter turns, into the cell whose top-left corner
/// is `at` (#461). The caller has already saved the context and restores it after.
///
/// **The order of the transform is the whole of it.** The cell is 14x20 and the sprite
/// is square, so translate-scale-rotate and translate-rotate-scale are *not* the same
/// picture: scaling first means the sprite turns in its own square space and the
/// squash into the cell happens after, which is what puts a join line that was along
/// the sprite's north edge along the north edge of the cell. Rotate first and the
/// squash lands on the turned axis instead, and a quarter-turned wall comes out
/// sheared to the wrong proportion.
///
/// Quarter turns are otherwise exact: no resampling beyond the squash every tile
/// already takes.
fn turned(
    ctx: &CanvasRenderingContext2d,
    atlas: &HtmlCanvasElement,
    from: (f64, f64),
    at: (f64, f64),
    turns: u8,
    m: &Metrics,
) -> Result<(), JsValue> {
    let tile = f64::from(TILE);
    ctx.translate(at.0 + m.cell_w / 2.0, at.1 + m.cell_h / 2.0)?;
    ctx.scale(m.cell_w / tile, m.cell_h / tile)?;
    ctx.rotate(f64::from(turns) * FRAC_PI_2)?;
    ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
        atlas,
        from.0,
        from.1,
        tile,
        tile,
        -tile / 2.0,
        -tile / 2.0,
        tile,
        tile,
    )
}

impl Atlases {
    /// The sheet tinted with `colour`, baking it on first use. `None` while the sheet
    /// is still decoding, or if the browser refuses a canvas.
    ///
    /// **The bake preserves the greys** (#460). The obvious spelling is one
    /// `source-in` fill, which replaces every colour channel and keeps only the alpha
    /// — correct for a flat sprite, but it throws away the shading that makes
    /// "greyscale with alpha" mean anything. So the tint is *multiplied* through the
    /// greys instead and the sheet is then drawn again as a `destination-in` mask to
    /// restore the alpha the multiply flattened. A sprite drawn in flat white comes
    /// out exactly as `source-in` would have left it, so this is strictly the more
    /// general of the two and costs two extra composites, once per colour, ever.
    fn tinted(&mut self, colour: &'static str) -> Option<HtmlCanvasElement> {
        if let Some(cached) = self.tinted.get(colour) {
            return Some(cached.clone());
        }
        let sheet = self.sheet.as_ref()?;
        // Only the rows the mapping reaches, never the whole authoring sheet — see
        // [`atlas_rows`]. Drawing the sheet at its natural size into a shorter canvas
        // simply clips the empty remainder away.
        let canvas = new_canvas(sheet.natural_width(), atlas_rows() * TILE)?;
        let ctx = canvas_context(&canvas)?;

        ctx.draw_image_with_html_image_element(sheet, 0.0, 0.0)
            .ok()?;
        let (w, h) = (f64::from(canvas.width()), f64::from(canvas.height()));
        ctx.set_global_composite_operation("multiply").ok()?;
        ctx.set_fill_style_str(colour);
        ctx.fill_rect(0.0, 0.0, w, h);
        ctx.set_global_composite_operation("destination-in").ok()?;
        ctx.draw_image_with_html_image_element(sheet, 0.0, 0.0)
            .ok()?;

        self.tinted.insert(colour, canvas.clone());
        Some(canvas)
    }
}

/// Start decoding the sheet, and redraw when it lands.
///
/// A no-op when the mode is off, so a load that never asked for tiles never pays for
/// the sheet. The handle is **weak** — the same reason the clipboard write holds one
/// ([`Game`]): the closure outlives the call that made it, and a strong reference from
/// an element the game owns back to the game would close a cycle for a page that is
/// already gone.
pub(crate) fn install(game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    if !game.borrow().tiles.on {
        return Ok(());
    }
    let image = HtmlImageElement::new()?;
    let handle = Rc::downgrade(game);
    let decoded = image.clone();
    let onload = Closure::<dyn FnMut()>::new(move || {
        let Some(game) = handle.upgrade() else {
            return; // the page went away mid-decode; nothing left to draw on
        };
        let game = game.borrow();
        game.tiles.accept(decoded.clone());
        game.draw();
    });
    image.set_onload(Some(onload.as_ref().unchecked_ref()));
    onload.forget();
    // Set last: a cached decode can complete synchronously here, and the handler has
    // to be attached before it can miss the event.
    image.set_src(&data_uri(SHEET));
    Ok(())
}

/// An offscreen canvas of the given size — offscreen in the plain sense that it is
/// never mounted, so it costs no layout and the page never sees it.
fn new_canvas(width: u32, height: u32) -> Option<HtmlCanvasElement> {
    let canvas: HtmlCanvasElement = web_sys::window()?
        .document()?
        .create_element("canvas")
        .ok()?
        .dyn_into()
        .ok()?;
    canvas.set_width(width);
    canvas.set_height(height);
    Some(canvas)
}

/// A 2d context for an offscreen canvas.
fn canvas_context(canvas: &HtmlCanvasElement) -> Option<CanvasRenderingContext2d> {
    canvas.get_context("2d").ok()??.dyn_into().ok()
}

/// The tile mode the page URL states, or `None` when it states nothing — the field is
/// absent, or carries a value that is neither [`TILES_ON`] nor [`TILES_OFF`].
///
/// Read through the same field parser the seed and debug channels use
/// ([`intrusion_core::field_in`]), so all three agree about what a field even is.
fn url_choice() -> Option<bool> {
    let query = web_sys::window()?.location().search().ok()?;
    choice_in(&query)
}

/// The tile mode a `?a=b&…` query states, or `None` for "it does not say".
///
/// Pure, so the flag's whole grammar is pinned by a native test rather than by a
/// browser. An unrecognised value is deliberately *not* an error and *not* "off": it
/// is somebody else's parameter or a typo, and either way the honest answer is that
/// the URL made no choice, leaving the baked default in charge.
fn choice_in(query: &str) -> Option<bool> {
    match intrusion_core::field_in(query, TILES_FIELD)? {
        TILES_ON => Some(true),
        TILES_OFF => Some(false),
        _ => None,
    }
}

/// Whether the *build* stamped the tile mode in, as a `window.__intrusionTiles`
/// global — how a preview artifact turns tiles on, since the artifact host strips the
/// hash and frames the page, leaving no URL for a reader to edit. Its **presence** is
/// the mode, exactly as `window.__intrusionDebug`'s is ([`crate::debug`]).
fn baked() -> bool {
    web_sys::window()
        .and_then(|window| {
            js_sys::Reflect::get(&window, &JsValue::from_str("__intrusionTiles")).ok()
        })
        .and_then(|value| value.as_string())
        .is_some()
}

/// The sheet as a `data:` URI — the one form that reaches both delivery channels
/// (see the module note).
fn data_uri(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64(png))
}

/// Standard base64, no line breaks. Written out rather than reached for through
/// `btoa` so it is a pure function with a native test: the encoding is the one step
/// between a correct sheet and a page that silently shows no tiles, and a browser is
/// an expensive place to find a bug in it.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut triple = [0u8; 3];
        triple[..chunk.len()].copy_from_slice(chunk);
        let n = u32::from(triple[0]) << 16 | u32::from(triple[1]) << 8 | u32::from(triple[2]);
        for shift in [18, 12, 6, 0] {
            out.push(ALPHABET[(n >> shift & 0x3f) as usize] as char);
        }
        // A short final chunk pads: two bytes spend three characters, one spends two.
        let padding = 3 - chunk.len();
        out.truncate(out.len() - padding);
        out.push_str(&"=".repeat(padding));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use intrusion_core::{
        render, start_level, Category, Fill, Input, LevelSeed, Terrain, Visibility,
    };

    /// A tile layer in a known mode, with nothing decoded — every test here is about
    /// the *decision*, which is the half a native test can reach.
    fn tiles(on: bool) -> Tiles {
        Tiles {
            on,
            atlases: RefCell::new(Atlases::default()),
        }
    }

    /// A cell carrying a glyph on a chosen surface; the rest is the board's default.
    fn cell(glyph: char, surface: Surface) -> GlyphCell {
        GlyphCell {
            glyph,
            fg: Category::Neutral,
            bg: None,
            vis: Visibility::Live,
            fill: Fill::Full,
            surface,
            facing: None,
        }
    }

    /// The sprite for a cell standing alone — no neighbours of its own kind, which is
    /// the neighbourhood most of these tests do not care about.
    fn sprite_of(tiles: &Tiles, cell: GlyphCell) -> Option<Sprite> {
        tiles.sprite_for_cell(cell, || 0)
    }

    /// A real board grid: a run of `seed` walked `steps` paces in from its tunnel,
    /// rendered. The deterministic boot the live shell, the sim and the replay viewer
    /// all share (§12.4/§13.2), so "the grid" here is the one the game really draws.
    ///
    /// The walk is **west**, and it has to be: a run opens inside the player's own
    /// entry tunnel (§10.7/#466) with the whole facility still unexplored, so a frame
    /// taken before they crawl out has no wall drawn anywhere and would make an
    /// autotile test pass by having nothing to test. Ten paces is enough to be
    /// standing in the building with walls in sight on every seed used here.
    fn board(seed: u64, steps: usize) -> Grid {
        let mut state =
            start_level(&LevelSeed::quick_play(seed)).expect("the v1 footprint always carves");
        for _ in 0..steps {
            state.step(Input::Step(Direction::West));
        }
        render(&state)
    }

    /// **The mapping covers every glyph the board can draw** — the terrain table
    /// (`Terrain::glyph`) plus the entity and schematic marks
    /// `docs/render-reference.md` §2 lists. Read off the core's own terrain enum
    /// rather than a copied list, so a terrain added there fails here rather than
    /// quietly rendering as a letter forever.
    ///
    /// The fallback means an uncovered glyph is *legal*, which is exactly why this
    /// test exists: without it "incomplete is fine" quietly becomes "incomplete", and
    /// the sheet stops being a tileset.
    #[test]
    fn every_board_glyph_has_a_sprite() {
        const TERRAIN: [Terrain; 11] = [
            Terrain::Floor,
            Terrain::Wall,
            Terrain::DoorHinge,
            Terrain::DoorPanelClosed,
            Terrain::DoorPanelOpen,
            Terrain::Hideout,
            Terrain::PartialCover,
            Terrain::DuctEntry,
            Terrain::Console,
            Terrain::CommsConsole,
            Terrain::Exit,
        ];
        for terrain in TERRAIN {
            let glyph = terrain.glyph();
            // Floor and an open panel draw *blank* (§2.2) — the gap in the wall is
            // their rendering, and a blank cell never reaches the glyph layer at all.
            if glyph == ' ' {
                continue;
            }
            assert!(
                sprite_index(glyph).is_some(),
                "{terrain:?} draws {glyph:?}, which has no sprite"
            );
        }
        // The marks that are not terrain: the entities (§2.1), the floor dot the FOV
        // draws, and the schematic's building fabric (§2.3).
        for glyph in ['@', 'g', 'z', '·', '□'] {
            assert!(sprite_index(glyph).is_some(), "{glyph:?} has no sprite");
        }
    }

    /// A glyph with no art draws as a **character**, never as a hole — the property
    /// that lets the table be incomplete on purpose. Letters are the case that
    /// matters: prose reaches the glyph layer wherever a chrome row does.
    #[test]
    fn an_unmapped_glyph_has_no_sprite() {
        for glyph in [' ', 'a', 'Q', '7', '?', '─', '\u{2591}'] {
            assert_eq!(sprite_index(glyph), None, "{glyph:?} claimed a sprite");
        }
    }

    /// Parse the slot table into `(index, key)` pairs — the same two fields an author
    /// reads off it, and the whole of what a test needs from it.
    fn slot_table() -> Vec<(u32, &'static str)> {
        SLOT_TABLE
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let mut fields = line.split_whitespace();
                let index = fields
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| panic!("slot table line has no index: {line:?}"));
                let key = fields
                    .next()
                    .unwrap_or_else(|| panic!("slot table line has no key: {line:?}"));
                (index, key)
            })
            .collect()
    }

    /// **The mapping and the slot table say the same thing.** The sheet is authored by
    /// hand against `web/assets/tiles.txt`, so that file is what an artist believes
    /// each cell is for — and if this table disagrees, the game draws a cupboard where
    /// somebody drew a console, with nothing to notice it by.
    ///
    /// Checked in both directions: every glyph here is at the slot the table gives it,
    /// and every `glyph:` slot the table declares is claimed here.
    ///
    /// The band is checked the same way (#461). A `wall:` slot is one the sheet draws
    /// and the autotiler indexes; a `rotation:` slot is one it reaches by *turning*
    /// another, and must therefore be indexed by nothing — the assertion that keeps the
    /// ten empty slots deliberate rather than forgotten, and that would fail the day
    /// somebody drew art into one without telling the mapping.
    #[test]
    fn the_mapping_agrees_with_the_slot_table() {
        let table = slot_table();
        for (glyph, index) in SPRITES {
            let key = table
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, key)| *key)
                .unwrap_or_else(|| panic!("slot {index} ({glyph:?}) is not in the slot table"));
            assert_eq!(
                key,
                format!("glyph:{glyph}"),
                "slot {index} is {key:?} in the slot table but {glyph:?} here",
            );
        }
        for (index, key) in &table {
            if let Some(glyph) = key.strip_prefix("glyph:") {
                let glyph = glyph.chars().next().expect("a glyph after the prefix");
                assert_eq!(
                    sprite_index(glyph),
                    Some(*index),
                    "the table declares {glyph:?} at slot {index}, but the mapping does not",
                );
            } else {
                let (kind, mask) = key.split_once(':').expect("a `kind:value` key");
                let mask: u8 = mask.parse().expect("a band key names its mask");
                let base = autotile_base('#').expect("the wall autotiles");
                assert_eq!(
                    *index,
                    base + u32::from(mask),
                    "slot {index} claims mask {mask}, which the band puts elsewhere",
                );
                assert!(
                    !SPRITES.iter().any(|(_, i)| i == index),
                    "slot {index} ({key}) is in the band — no glyph may claim it",
                );
                let drawn_upright = canonical(mask) == (mask, 0);
                match kind {
                    // A drawn shape: the autotiler indexes it, and does so upright.
                    "wall" => assert!(
                        drawn_upright,
                        "slot {index} is drawn art, but mask {mask} is {:?} turned",
                        canonical(mask),
                    ),
                    // A tombstone: allocated, empty, and reached only by turning.
                    "rotation" => {
                        assert!(
                            !drawn_upright,
                            "slot {index} is declared a rotation, but mask {mask} is a \
                             shape of its own",
                        );
                        assert!(
                            (0..16u8).all(|m| base + u32::from(canonical(m).0) != *index),
                            "slot {index} is declared a rotation, but the autotiler \
                             indexes it — it would sample an empty slot",
                        );
                    }
                    _ => panic!("slot {index} has an unknown kind of key {key:?}"),
                }
            }
        }
        // And every drawn shape is declared: the band's six `wall:` lines, no fewer.
        for shape in CANONICAL {
            let index = autotile_base('#').expect("the wall autotiles") + u32::from(shape);
            let declared = format!("wall:{shape}");
            assert!(
                table.iter().any(|(i, key)| *i == index && *key == declared),
                "the autotiler draws slot {index}, but the table does not declare it",
            );
        }
    }

    /// Every sprite index lands **inside the sheet**, in the glyph band, and no two
    /// glyphs share one. An index off the sheet samples transparent pixels — a tile
    /// that is silently invisible, which is the failure hardest to spot on a dark
    /// board — and one past the glyph band would be squatting on the wall run step 2
    /// has reserved.
    #[test]
    fn sprite_indices_are_unique_and_in_the_glyph_band() {
        let mut seen = Vec::new();
        for (glyph, index) in SPRITES {
            assert!(
                index < SHEET_COLS * SHEET_ROWS,
                "{glyph:?} indexes {index}, off a {SHEET_COLS}x{SHEET_ROWS} sheet",
            );
            assert!(
                index < SHEET_COLS,
                "{glyph:?} indexes {index}, past the glyph band's first row",
            );
            assert!(!seen.contains(&index), "{glyph:?} reuses index {index}");
            seen.push(index);
        }
    }

    /// **Every neighbourhood is one of the six shapes, turned** (#461) — the property
    /// the whole deduplication rests on, and the one that makes ten empty slots safe.
    ///
    /// Three things at once, because they are one statement: the representative is a
    /// shape the sheet draws, turning it by the quarters reported gets the mask back,
    /// and a representative is its own representative at no turns (so the six drawn
    /// slots are drawn upright).
    #[test]
    fn every_neighbourhood_is_a_drawn_shape_turned() {
        for mask in 0..16u8 {
            let (rep, turns) = canonical(mask);
            assert!(
                CANONICAL.contains(&rep),
                "mask {mask} reduces to {rep}, which the sheet does not draw",
            );
            assert!(turns < 4, "mask {mask} asks for {turns} quarter turns");
            let mut turned = rep;
            for _ in 0..turns {
                turned = turn_mask(turned);
            }
            assert_eq!(
                turned, mask,
                "slot for {rep} turned {turns} quarters is {turned}, not {mask}",
            );
        }
        for rep in CANONICAL {
            assert_eq!(
                canonical(rep),
                (rep, 0),
                "a drawn shape must be drawn upright",
            );
        }
    }

    /// **The sixteen masks reach exactly six slots** (#461). Stated separately from the
    /// arithmetic above because it is the thing a sheet author needs to know: these six
    /// have art, and every other slot of the band is reached by turning one of them.
    #[test]
    fn the_band_indexes_six_slots_and_no_others() {
        let base = autotile_base('#').expect("the wall autotiles");
        let mut reached: Vec<u32> = (0..16u8)
            .map(|mask| base + u32::from(canonical(mask).0))
            .collect();
        reached.sort_unstable();
        reached.dedup();
        let expected: Vec<u32> = CANONICAL.iter().map(|&m| base + u32::from(m)).collect();
        assert_eq!(reached, expected);
        assert_eq!(reached.len(), 6, "six images for sixteen neighbourhoods");
    }

    /// **A wall joins only to what the grid draws** (#461/§11.5a) — the fog test the
    /// ticket says to write first.
    ///
    /// Geometry the player has never seen is masked as the schematic's `□`, so a wall
    /// cannot join to it; the join follows the *drawn* glyph and can therefore say no
    /// more than the glyph already said. Asserted over a real opening frame, where
    /// almost the whole facility is still unexplored, in both directions: every bit of
    /// every mask is exactly a same-glyph board neighbour, and — the witness that keeps
    /// this from passing vacuously (#482) — some wall really does sit against
    /// unexplored fabric, with that fabric not counted.
    #[test]
    fn a_wall_never_joins_to_geometry_the_player_has_not_seen() {
        let grid = board(4242, 10);
        let mut walls = 0;
        let mut against_the_fog = 0;
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                if grid.get(x, y).glyph != '#' || grid.get(x, y).surface != Surface::Board {
                    continue;
                }
                walls += 1;
                let mask = neighbour_mask(&grid, x, y, '#');
                for (bit, dx, dy) in [(NORTH, 0, -1), (EAST, 1, 0), (SOUTH, 0, 1), (WEST, -1, 0)] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    let neighbour = (0..grid.width() as i64).contains(&nx)
                        && (0..grid.height() as i64).contains(&ny)
                        && grid.get(nx as u32, ny as u32).surface == Surface::Board;
                    let drawn = neighbour.then(|| grid.get(nx as u32, ny as u32).glyph);
                    assert_eq!(
                        mask & bit != 0,
                        drawn == Some('#'),
                        "({x},{y}) joins by bit {bit} but its neighbour draws {drawn:?}",
                    );
                    if drawn == Some('□') {
                        against_the_fog += 1;
                        assert_eq!(
                            mask & bit,
                            0,
                            "({x},{y}) joined to fabric the player has never seen",
                        );
                    }
                }
            }
        }
        assert!(walls > 0, "the opening frame draws some wall");
        assert!(
            against_the_fog > 0,
            "the opening frame puts some wall against unexplored fabric — \
             without one this test proves nothing",
        );
    }

    /// **Lifting the fog changes the joins** (#461), and every change is a neighbour
    /// that changed glyph. Walking turns `□` into `#` as the player sees it, and the
    /// wall's shape follows — which is the truthful behaviour, not a glitch: the joins
    /// state what is known, and what is known grows.
    #[test]
    fn lifting_the_fog_changes_the_joins() {
        let opening = board(4242, 10);
        let walked = board(4242, 16);
        let mut moved = 0;
        for y in 0..opening.height() {
            for x in 0..opening.width() {
                if opening.get(x, y).glyph != '#' || walked.get(x, y).glyph != '#' {
                    continue;
                }
                let before = neighbour_mask(&opening, x, y, '#');
                let after = neighbour_mask(&walked, x, y, '#');
                if before != after {
                    moved += 1;
                }
            }
        }
        assert!(
            moved > 0,
            "seeing more of the facility must join more of its walls",
        );
    }

    /// **An actor is turned to the way it faces** (#461), and the sprite's own rest
    /// costs no turn at all — the commonest case a player looks at.
    #[test]
    fn a_facing_turns_the_sprite_and_the_rest_facing_costs_nothing() {
        assert_eq!(turns_to_face(REST_FACING), 0);
        // Clockwise, one quarter at a time, all the way round and back.
        let mut facing = REST_FACING;
        for expected in 0..4 {
            assert_eq!(turns_to_face(facing), expected);
            facing = facing.clockwise();
        }
        assert_eq!(facing, REST_FACING, "four quarters is the whole turn");

        let tiles = tiles(true);
        for facing in Direction::ALL {
            let turned = GlyphCell {
                facing: Some(facing),
                ..cell('@', Surface::Board)
            };
            assert_eq!(
                sprite_of(&tiles, turned),
                Some(Sprite {
                    index: sprite_index('@').expect("the player has art"),
                    turns: turns_to_face(facing),
                }),
                "the `@` facing {facing:?} is its one sprite, turned",
            );
        }
        // A cell that faces nowhere is drawn upright — a body, a console, a wall.
        assert_eq!(
            sprite_of(&tiles, cell('z', Surface::Board)).map(|s| s.turns),
            Some(0),
        );
    }

    /// **The flag's whole grammar** (#460). `1` and `0` are choices in either
    /// direction; anything else — absent, empty, a typo, somebody else's parameter —
    /// is *no choice*, leaving whatever the build baked in to decide.
    #[test]
    fn the_url_states_the_tile_mode_or_says_nothing() {
        assert_eq!(choice_in("?tiles=1"), Some(true));
        assert_eq!(choice_in("?tiles=0"), Some(false));
        // Read among a host's own parameters, in either position — the artifact host
        // adds its own, and the Pages deploy can carry anything.
        assert_eq!(choice_in("?__frame_t=17&tiles=1"), Some(true));
        assert_eq!(choice_in("?tiles=0&other=x"), Some(false));

        assert_eq!(choice_in("?tiles=on"), None, "one spelling, not several");
        assert_eq!(choice_in("?tiles=true"), None);
        assert_eq!(choice_in("?tiles="), None, "an empty value is not a value");
        assert_eq!(choice_in("?tiles"), None, "nor a bare field");
        assert_eq!(choice_in("?tilesets=1"), None, "no prefix match");
        assert_eq!(choice_in("?seed=prbjdokbxcqgjnrnco"), None);
        assert_eq!(choice_in(""), None);
    }

    /// The tile layer is absent unless the mode is on — the flag's whole job, and the
    /// guarantee behind "the text renderer is byte-identical with the flag absent".
    #[test]
    fn the_layer_is_absent_unless_the_mode_is_on() {
        assert!(tiles(false).layer().is_none());
        assert!(tiles(true).layer().is_some());
    }

    /// **Chrome never tiles, whatever its glyph is** (#460). A `#` in a panel's rule
    /// and a `g` in a logged sentence both carry a sprite in the table, and both must
    /// still draw as characters — the case that made the surface a property of the
    /// cell rather than of the row.
    ///
    /// Asserted through the same early return the draw path takes, since a canvas is
    /// not reachable from a native test: what is checked is the decision, which is the
    /// part that can be wrong.
    #[test]
    fn a_chrome_cell_is_never_a_candidate_for_a_tile() {
        for glyph in SPRITES.map(|(glyph, _)| glyph) {
            assert!(
                sprite_of(&tiles(true), cell(glyph, Surface::Chrome)).is_none(),
                "{glyph:?} in chrome must draw as a character"
            );
            assert!(
                sprite_of(&tiles(true), cell(glyph, Surface::Board)).is_some(),
                "{glyph:?} on the board is a tile"
            );
        }
        // And an unmapped glyph is text on either surface.
        for surface in [Surface::Board, Surface::Chrome] {
            assert!(sprite_of(&tiles(true), cell('a', surface)).is_none());
        }
        // With the mode off, nothing is ever a candidate.
        assert!(sprite_of(&tiles(false), cell('#', Surface::Board)).is_none());
    }

    /// The sheet is embedded and is a **PNG** — a build that lost the asset, or
    /// pointed at something that is not an image, would produce a page whose tile
    /// mode silently does nothing.
    #[test]
    fn the_embedded_sheet_is_a_png() {
        assert_eq!(&SHEET[..8], b"\x89PNG\r\n\x1a\n");
        assert!(SHEET.len() > 100, "the sheet is a stub");
    }

    /// Base64, against the RFC 4648 vectors — including both padding cases, which are
    /// where a hand-rolled encoder goes wrong and where the failure is a `data:` URI
    /// the browser rejects with no error the shell can see.
    #[test]
    fn base64_matches_the_standard_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // The high bytes a PNG is mostly made of, where a sign error would show.
        assert_eq!(base64(&[0xff, 0xff, 0xff]), "////");
        assert_eq!(base64(&[0x89, 0x50, 0x4e, 0x47]), "iVBORw==");
    }

    /// The URI the browser is handed names the type it is: a `data:` URI with the
    /// wrong media type decodes to nothing, and the sheet simply never appears.
    #[test]
    fn the_sheet_is_delivered_as_a_png_data_uri() {
        let uri = data_uri(SHEET);
        assert!(uri.starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert!(!uri.contains('\n'), "a line break would break the URI");
    }
}
