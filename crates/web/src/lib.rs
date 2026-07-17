//! The thin web shell (§12.2): the wasm-bindgen entry point, a canvas2d blitter, and
//! the input pump. It stays deliberately thin — all game logic *and all rendering*
//! live in `intrusion-core`; this crate feeds the core input and paints the grid the
//! core hands back.
//!
//! **The rendering seam (§11.1, and see `core::render`).** The core produces a
//! [`Grid`] of `(glyph, fg-category, bg)` — it decides every glyph, resolves every
//! overlap (glyph priority, §11.3), and tags each cell with an information *category*
//! (§11.2). This shell does exactly **one** rendering job: map each cell's
//! [`Category`] to a concrete colour and draw the glyph. It never decides a glyph,
//! never overlays an entity itself, never picks a colour from game state — if it did,
//! the core would stop being the single source of truth for what the game looks like.
//!
//! It runs the turn loop (§4.2): boot generates a facility, drops the player in, and
//! draws it; arrow keys (or WASD / vi keys) drive [`State::step`], and every keypress
//! redraws. The **whole level is always visible with no scrolling**, on desktop and
//! mobile alike: the canvas is scaled to fit the viewport (aspect preserved) and its
//! backing store is sized in device pixels so glyphs stay crisp; a resize/orientation
//! change recomputes and redraws. A player-*centred* viewport (§11.4), fog (§11.5a),
//! the danger overlay (§11.5), and explicit hotkeys (§11.6) are later render tickets;
//! the full colour-blind-safe palette (§11.2) refines the placeholder table below.
//! Placement here (a floor-cell scan) is a preview harness; real placement is
//! generation's job (§10.1).

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{
    generate, render, Category, Cell, Direction, Facility, Grid, Guard, Input, Rng, State, Terrain,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement, KeyboardEvent, Window};

/// The glyph cell's base aspect (width:height); a monospace glyph reads best in a
/// slightly tall box. Actual on-screen cell size is this scaled to fit the viewport.
const CELL_W: f64 = 14.0;
const CELL_H: f64 = 20.0;

/// The page background.
const BG: &str = "#0b0b0b";

/// Map an information category (§11.2) to a concrete colour — **the shell's one and
/// only rendering decision**. The core tags each cell with a [`Category`]; this table
/// is where category becomes pixels, so recolouring or an accessibility pass is a
/// one-function edit. These are placeholders; the full 16-colour colour-blind-safe
/// palette with darkened background variants is the colour-category ticket (§11.2).
///
/// Every entry must be **visibly distinct** on the dark background (asserted below):
/// the threat ladder Caution→Warning→Danger reads as yellow→orange→red, and System
/// furniture is a muted **brown** rather than a tan that blurs into Caution's yellow.
fn category_color(category: Category) -> &'static str {
    match category {
        Category::Neutral => "#d0d0d0",  // light grey — inert scenery, walls
        Category::Owned => "#4ea6ff",    // blue — you and what you made
        Category::Caution => "#f2d64a",  // yellow — a threat, unaware
        Category::Warning => "#e07d1e",  // orange — a threat, hunting
        Category::Danger => "#d83030",   // red — a threat that has you
        Category::Interest => "#bd6bd6", // purple — goals and rewards
        Category::System => "#9a7040",   // brown — doors, hideouts (furniture)
    }
}

/// Boot the game: generate a facility, place the player, draw it, and start listening
/// for input and resize (§4.2, §13.1's build→play half).
///
/// This is the wasm entry point the page calls after the module initialises. The seed
/// is the one impurity the shell owns (§12.1 keeps the *core* pure): read the clock so
/// each load is a different facility, and hand the core a plain `u64`. The v1 footprint
/// is 40×40 (§10.2). Reload for a new seed; explicit seed entry / sharing (§13.1) is a
/// later ticket.
#[wasm_bindgen]
pub fn start() -> Result<(), JsValue> {
    let seed = js_sys::Date::now() as u64;
    let layout = generate(40, 40, &mut Rng::new(seed))
        .map_err(|e| JsValue::from_str(&format!("generation failed: {e:?}")))?;

    // Preview placement: player at the first floor cell, exit at the last, and a
    // couple of stationary guards for the picture. Real placement — safe start,
    // spread objectives, real patrols — is generation's job (§10.1.7–9, #12).
    let floors = floor_cells(layout.facility());
    let &spawn = floors
        .first()
        .ok_or_else(|| JsValue::from_str("no floor"))?;
    let &exit = floors.last().unwrap();
    let guards = floors
        .iter()
        .skip(floors.len() / 3)
        .step_by(floors.len().max(1) / 2 + 1)
        .filter(|&&c| c != spawn && c != exit)
        .take(2)
        .map(|&c| Guard::stationary(c))
        .collect();

    let state = State::new(layout, spawn, Direction::North, guards, Vec::new(), exit);

    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let canvas = mount_canvas(&document)?;
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .dyn_into::<CanvasRenderingContext2d>()?;

    let game = Rc::new(RefCell::new(Game {
        state,
        canvas,
        ctx,
        metrics: Metrics::base(),
    }));
    game.borrow_mut().fit_and_draw(); // size to the viewport and paint the first frame
    install_input(&document, &game)?;
    install_resize(&game)?;
    Ok(())
}

/// On-screen cell geometry in **backing-store (device) pixels** — the scale that fits
/// the whole level to the viewport at the current device pixel ratio.
#[derive(Clone, Copy)]
struct Metrics {
    cell_w: f64,
    cell_h: f64,
    font: f64,
}

impl Metrics {
    /// A pre-fit placeholder; [`Game::fit_and_draw`] replaces it before the first paint.
    fn base() -> Self {
        Self {
            cell_w: CELL_W,
            cell_h: CELL_H,
            font: CELL_H - 2.0,
        }
    }
}

/// The running game, its canvas, and the current fit — the shell's whole mutable world.
struct Game {
    state: State,
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    metrics: Metrics,
}

impl Game {
    /// Map a key to an [`Input`] and, if it is one the loop takes, step and redraw.
    /// Returns whether the key was consumed (so the caller can stop the page from
    /// scrolling on the arrows). Explicit, stable hotkeys are §11.6's ticket; this is
    /// the minimal movement set.
    fn handle_key(&mut self, key: &str) -> bool {
        let input = match key {
            "ArrowUp" | "w" | "k" => Input::Step(Direction::North),
            "ArrowDown" | "s" | "j" => Input::Step(Direction::South),
            "ArrowLeft" | "a" | "h" => Input::Step(Direction::West),
            "ArrowRight" | "d" | "l" => Input::Step(Direction::East),
            "." | "5" => Input::Wait,
            _ => return false,
        };
        self.state.step(input);
        self.draw();
        true
    }

    /// Fit the canvas to the viewport and redraw. Compute a uniform scale so the whole
    /// `cols × rows` grid fits within the window on both axes (aspect preserved), size
    /// the backing store in device pixels for crisp glyphs, set the CSS size so the
    /// element itself fits (no scrolling), and paint. Called at boot and on every
    /// resize / orientation change.
    fn fit_and_draw(&mut self) {
        let facility = self.state.layout().facility();
        let (cols, rows) = (facility.width() as f64, facility.height() as f64);
        let win = web_sys::window().expect("a window");

        let avail_w = viewport(&win, Window::inner_width).unwrap_or(cols * CELL_W);
        let avail_h = viewport(&win, Window::inner_height).unwrap_or(rows * CELL_H);
        let dpr = win.device_pixel_ratio().max(1.0);

        // CSS pixels per base cell so the level fits both dimensions.
        let scale = (avail_w / (cols * CELL_W)).min(avail_h / (rows * CELL_H));
        let css_w = cols * CELL_W * scale;
        let css_h = rows * CELL_H * scale;

        // Backing store in device pixels; CSS box in layout pixels. Drawing in device
        // pixels then keeps text sharp on high-DPI and mobile screens.
        self.canvas.set_width((css_w * dpr).round() as u32);
        self.canvas.set_height((css_h * dpr).round() as u32);
        let _ = self
            .canvas
            .set_attribute("style", &format!("width:{css_w}px;height:{css_h}px"));

        self.metrics = Metrics {
            cell_w: CELL_W * scale * dpr,
            cell_h: CELL_H * scale * dpr,
            font: (CELL_H - 2.0) * scale * dpr,
        };
        self.draw();
    }

    /// Draw one frame: ask the core to render the whole grid (terrain + entities,
    /// glyph priority resolved), then blit it — colour by category, glyph as given.
    fn draw(&self) {
        paint(&self.ctx, &render(&self.state), &self.metrics);
    }
}

/// Read a viewport dimension (`inner_width` / `inner_height`) as an `f64`, if the
/// browser gives one.
fn viewport(win: &Window, get: fn(&Window) -> Result<JsValue, JsValue>) -> Option<f64> {
    get(win).ok().and_then(|v| v.as_f64())
}

/// Every interior floor cell, row-major — the shell's stand-in for placement until
/// generation does it (§10.1). Border and structure are skipped; only walkable floor.
fn floor_cells(f: &Facility) -> Vec<Cell> {
    let mut cells = Vec::new();
    for y in 0..f.height() {
        for x in 0..f.width() {
            if f.terrain_at(x, y) == Some(Terrain::Floor) {
                cells.push(Cell::new(x, y));
            }
        }
    }
    cells
}

/// Create the canvas, mount it, and hand it back. Its size is set later by
/// [`Game::fit_and_draw`], which fits it to the viewport.
fn mount_canvas(document: &Document) -> Result<HtmlCanvasElement, JsValue> {
    // Mount into #app if the page provides it, else the body.
    let mount = document
        .get_element_by_id("app")
        .or_else(|| document.body().map(Into::into))
        .ok_or_else(|| JsValue::from_str("no mount point"))?;

    let canvas: HtmlCanvasElement = document
        .create_element("canvas")?
        .dyn_into::<HtmlCanvasElement>()?;
    mount.append_child(&canvas)?;
    Ok(canvas)
}

/// Install the keydown pump: each keypress drives one [`Game::handle_key`]. The
/// closure owns a clone of the `Rc` so the game outlives `start`; `forget` hands it to
/// the browser for the page's lifetime (the shell never tears down).
fn install_input(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let game = game.clone();
    let cb = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
        if game.borrow_mut().handle_key(&e.key()) {
            e.prevent_default();
        }
    });
    document.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())?;
    cb.forget();
    Ok(())
}

/// Install the resize pump: refit the canvas to the window on resize / orientation
/// change, so the whole level stays visible without scrolling.
fn install_resize(game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let game = game.clone();
    let cb = Closure::<dyn FnMut()>::new(move || game.borrow_mut().fit_and_draw());
    let win = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    win.add_event_listener_with_callback("resize", cb.as_ref().unchecked_ref())?;
    cb.forget();
    Ok(())
}

/// Blit a rendered [`Grid`] to the canvas: fill the background, then draw each
/// non-blank glyph centred in its cell, coloured by its category ([`category_color`]).
/// Blank cells (floor) are left as background. This is the shell's whole rendering
/// job — the glyphs, overlaps and categories were all decided by `core::render`.
fn paint(ctx: &CanvasRenderingContext2d, grid: &Grid, m: &Metrics) {
    ctx.set_fill_style_str(BG);
    ctx.fill_rect(
        0.0,
        0.0,
        grid.width() as f64 * m.cell_w,
        grid.height() as f64 * m.cell_h,
    );

    ctx.set_font(&format!("{:.1}px ui-monospace, monospace", m.font));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");

    for y in 0..grid.height() {
        for x in 0..grid.width() {
            let cell = grid.get(x, y);
            if cell.glyph == ' ' {
                continue;
            }
            ctx.set_fill_style_str(category_color(cell.fg));
            draw_char(ctx, x as f64, y as f64, cell.glyph, m);
        }
    }
}

/// Paint a single character centred in cell `(x, y)` with the current fill style.
fn draw_char(ctx: &CanvasRenderingContext2d, x: f64, y: f64, glyph: char, m: &Metrics) {
    let px = x * m.cell_w + m.cell_w / 2.0;
    let py = y * m.cell_h + m.cell_h / 2.0;
    // fill_text only errors on an invalid surface; ignore the unit Ok.
    let _ = ctx.fill_text(&glyph.to_string(), px, py);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a `#rrggbb` string into RGB — mirror of what the browser does.
    fn rgb(hex: &str) -> (i32, i32, i32) {
        let h = hex.strip_prefix('#').expect("a #rrggbb colour");
        let n = i32::from_str_radix(h, 16).expect("six hex digits");
        (n >> 16 & 0xff, n >> 8 & 0xff, n & 0xff)
    }

    /// Squared RGB distance — cheap and monotonic, enough to catch two colours that
    /// would read as the same on screen.
    fn dist2(a: (i32, i32, i32), b: (i32, i32, i32)) -> i32 {
        let (dr, dg, db) = (a.0 - b.0, a.1 - b.1, a.2 - b.2);
        dr * dr + dg * dg + db * db
    }

    /// Every category must map to a **visibly distinct** colour. The regression this
    /// guards: `System` (doors, hideouts) once sat a tan hair away from `Caution`
    /// (unaware guards), so doors, hideouts and guards all read as one yellow. The
    /// threat ladder Caution→Warning→Danger and the furniture brown must stay apart.
    #[test]
    fn category_colours_are_all_visibly_distinct() {
        let categories = [
            Category::Neutral,
            Category::Owned,
            Category::Caution,
            Category::Warning,
            Category::Danger,
            Category::Interest,
            Category::System,
        ];
        // ~70 in RGB distance: the old tan/yellow clash measured ~61 and must fail.
        const MIN_DIST2: i32 = 70 * 70;
        for (i, &a) in categories.iter().enumerate() {
            for &b in &categories[i + 1..] {
                let d = dist2(rgb(category_color(a)), rgb(category_color(b)));
                assert!(
                    d >= MIN_DIST2,
                    "{a:?} and {b:?} are too close to tell apart (dist^2 {d} < {MIN_DIST2})"
                );
            }
        }
    }
}
