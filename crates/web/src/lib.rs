//! The thin web shell (§12.2): the wasm-bindgen entry point, a canvas2d glyph
//! renderer, and the input pump. It stays deliberately thin — all game logic lives
//! in `intrusion-core`; this crate only draws the core's state and feeds it input.
//!
//! It runs the turn loop (§4.2): boot generates a facility, drops the player in, and
//! draws it; arrow keys (or WASD / vi keys) drive [`State::step`], and every keypress
//! redraws. The **whole level is always visible with no scrolling**, on desktop and
//! mobile alike: the canvas is scaled to fit the viewport (aspect preserved) and its
//! backing store is sized in device pixels so glyphs stay crisp; a resize/orientation
//! change recomputes and redraws. A player-*centred* viewport (§11.4) — as opposed to
//! this fit-the-whole-level view — is a later render ticket, as are colour categories
//! (§11.2), fog (§11.5a), and explicit hotkeys (§11.6). Placement here (a floor-cell
//! scan) is a preview harness; real placement is generation's job (§10.1).

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{
    ascii_grid, generate, Cell, Direction, Facility, Guard, Input, Rng, State, Terrain,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement, KeyboardEvent, Window};

/// The glyph cell's base aspect (width:height); a monospace glyph reads best in a
/// slightly tall box. Actual on-screen cell size is this scaled to fit the viewport.
const CELL_W: f64 = 14.0;
const CELL_H: f64 = 20.0;

/// Colours the shell owns until the colour-category system (§11.2) lands and maps
/// them from information categories. Concrete colours never live in the core (§11.2).
const BG: &str = "#0b0b0b";
const NEUTRAL: &str = "#cfcfcf"; // scenery
const PLAYER: &str = "#7ab8ff"; // the player glyph, so `@` reads at a glance
const GUARD: &str = "#e0803a"; // guards

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

    /// Draw one frame: the terrain glyph grid, then the guards and the player on top.
    fn draw(&self) {
        let grid = ascii_grid(self.state.layout().facility());
        draw_grid(&self.ctx, &grid, &self.metrics);
        for guard in self.state.guards() {
            draw_glyph(&self.ctx, guard.pos(), 'g', GUARD, &self.metrics);
        }
        draw_glyph(&self.ctx, self.state.player(), '@', PLAYER, &self.metrics);
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

/// Paint a glyph grid: fill the background, then draw each non-blank glyph centred in
/// its cell. Blank (floor) cells are left as background. Sets the font and alignment
/// the on-top glyphs ([`draw_glyph`]) then reuse.
fn draw_grid(ctx: &CanvasRenderingContext2d, grid: &[String], m: &Metrics) {
    let rows = grid.len() as f64;
    let cols = grid.first().map_or(0, |r| r.chars().count()) as f64;

    ctx.set_fill_style_str(BG);
    ctx.fill_rect(0.0, 0.0, cols * m.cell_w, rows * m.cell_h);

    ctx.set_fill_style_str(NEUTRAL);
    ctx.set_font(&format!("{:.1}px ui-monospace, monospace", m.font));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");

    for (y, row) in grid.iter().enumerate() {
        for (x, glyph) in row.chars().enumerate() {
            if glyph == ' ' {
                continue;
            }
            draw_char(ctx, x as f64, y as f64, glyph, m);
        }
    }
}

/// Draw one glyph at a cell in `color`, over the terrain grid. Relies on the font and
/// alignment [`draw_grid`] set for this frame.
fn draw_glyph(ctx: &CanvasRenderingContext2d, cell: Cell, glyph: char, color: &str, m: &Metrics) {
    ctx.set_fill_style_str(color);
    draw_char(ctx, cell.x as f64, cell.y as f64, glyph, m);
}

/// Paint a single character centred in cell `(x, y)` with the current fill style.
fn draw_char(ctx: &CanvasRenderingContext2d, x: f64, y: f64, glyph: char, m: &Metrics) {
    let px = x * m.cell_w + m.cell_w / 2.0;
    let py = y * m.cell_h + m.cell_h / 2.0;
    // fill_text only errors on an invalid surface; ignore the unit Ok.
    let _ = ctx.fill_text(&glyph.to_string(), px, py);
}
