//! The **tile renderer** (§11.1 / #460) — the shell's second cell primitive, and
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
//! # One tile per glyph, tinted by the colour the glyph would have been
//!
//! Step 1 makes the simplest art rule it can: **a sprite is chosen by the cell's
//! glyph and by nothing else** — no autotiling, no neighbour lookup, no rotation, no
//! animation. The colour is not chosen here either. [`crate::paint`] has already
//! resolved the cell to a concrete colour through the §11.2 table (the category's
//! `fg` live, its `dim` beyond the FOV, the memory slate for a remembered content),
//! and the tile is drawn *in that colour*. So a tile and a glyph carry exactly the
//! same information, in all three knowledge states and both themes, and swapping
//! between them can never change what a cell means.
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
//! **The cell size.** Sprites are authored to the 14x20 glyph cell's aspect (at 2x,
//! [`TILE_W`] x [`TILE_H`]), so `fit_and_draw`'s arithmetic and every hit test — the
//! help button, the ability bar, the tab bar — are untouched. Square tiles would mean
//! the map keeping its own metric while the HUD rows keep the text one, which breaks
//! the single-grid fit; that is step 2's problem and step 1 refuses it.
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
use std::rc::Rc;

use intrusion_core::{GlyphCell, Surface};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement};

use crate::{Game, Metrics};

/// The placeholder tileset, embedded in the wasm (see the module note on delivery).
/// Regenerate it with `scripts/make-placeholder-tiles.py`, which also documents the
/// sheet contract real art has to satisfy; `docs/render-reference.md` §6 is the prose
/// form of the same rule.
const SHEET: &[u8] = include_bytes!("../../../web/assets/tiles.png");

/// The sheet's cells per row. A sprite's index is `row * SHEET_COLS + col`, so the
/// layout is a pure function of the index and adding a sprite never moves an
/// existing one.
const SHEET_COLS: u32 = 8;

/// One sprite's source size: the 14x20 glyph cell ([`crate::CELL_W`]/[`crate::CELL_H`])
/// at **2x**, so a board fitted to a high-DPI screen has real pixels to draw from.
/// The *destination* is always the fitted cell, so this number is invisible to layout.
const TILE_W: f64 = 28.0;
const TILE_H: f64 = 40.0;

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
/// The order is the sheet's own layout, in index order: building fabric and furniture
/// first, then the goals a plan is drawn around, then the things that move.
/// `scripts/make-placeholder-tiles.py` writes its sprites in this same order, and the
/// tests below assert the set covers every glyph `docs/render-reference.md` §2 lists.
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

    /// The sprite this cell would be drawn with, or `None` when it draws as a
    /// character: the mode is off, the cell is chrome, or its glyph has no art.
    ///
    /// **The whole tile/text decision, in one place**, so [`Tiles::draw`] and the
    /// tests below are asking the same question rather than two questions that agree
    /// today. The one condition it does not answer is whether the sheet has finished
    /// decoding — that is a fact about the moment, not about the cell.
    fn sprite_for(&self, cell: GlyphCell) -> Option<u32> {
        if !self.on || cell.surface != Surface::Board {
            return None;
        }
        sprite_index(cell.glyph)
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
        x: f64,
        y: f64,
        cell: GlyphCell,
        colour: &'static str,
        m: &Metrics,
    ) -> bool {
        let Some(index) = self.sprite_for(cell) else {
            return false;
        };
        let Some(atlas) = self.atlases.borrow_mut().tinted(colour) else {
            return false;
        };
        let (sx, sy) = (
            f64::from(index % SHEET_COLS) * TILE_W,
            f64::from(index / SHEET_COLS) * TILE_H,
        );
        // Errors here mean an invalid surface, the same condition `fill_text` ignores;
        // there is nothing a frame can do about it and nothing to fall back *to*.
        ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            &atlas,
            sx,
            sy,
            TILE_W,
            TILE_H,
            x * m.cell_w,
            y * m.cell_h,
            m.cell_w,
            m.cell_h,
        )
        .is_ok()
    }
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
        let canvas = new_canvas(sheet.natural_width(), sheet.natural_height())?;
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
    use intrusion_core::{Category, Fill, Terrain, Visibility};

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
        }
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

    /// Every sprite index lands **inside the sheet**, and no two glyphs share one.
    /// The sheet is `SHEET_COLS`-wide and the generator writes `SPRITES.len()`
    /// sprites, so an index past that would sample transparent pixels — a tile that
    /// is silently invisible, the failure hardest to spot on a dark board.
    #[test]
    fn sprite_indices_are_unique_and_on_the_sheet() {
        let mut seen = Vec::new();
        for (glyph, index) in SPRITES {
            assert!(
                (index as usize) < SPRITES.len(),
                "{glyph:?} indexes {index}, past the {} sprites on the sheet",
                SPRITES.len()
            );
            assert!(!seen.contains(&index), "{glyph:?} reuses index {index}");
            seen.push(index);
        }
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
                tiles(true)
                    .sprite_for(cell(glyph, Surface::Chrome))
                    .is_none(),
                "{glyph:?} in chrome must draw as a character"
            );
            assert!(
                tiles(true)
                    .sprite_for(cell(glyph, Surface::Board))
                    .is_some(),
                "{glyph:?} on the board is a tile"
            );
        }
        // And an unmapped glyph is text on either surface.
        for surface in [Surface::Board, Surface::Chrome] {
            assert!(tiles(true).sprite_for(cell('a', surface)).is_none());
        }
        // With the mode off, nothing is ever a candidate.
        assert!(tiles(false).sprite_for(cell('#', Surface::Board)).is_none());
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
