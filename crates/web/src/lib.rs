//! The thin web shell (§12.2): the wasm-bindgen entry point, a canvas2d glyph
//! renderer, and the input pump. It stays deliberately thin — all game logic lives
//! in `intrusion-core`; this crate only draws the core's state and feeds it input.
//!
//! It now runs the turn loop (§4.2). Boot generates a facility, drops the player in,
//! and draws it; arrow keys (or WASD / vi keys) drive [`State::step`], and every
//! keypress redraws. The renderer is still the smallest thing that works — the
//! terrain glyph grid (§11.1) with the player and guards painted on top — so colour
//! categories (§11.2), fog (§11.5a), the player-centred viewport and explicit hotkeys
//! (§11.4/§11.6) all still belong to their own render tickets. Placement here (a scan
//! for floor cells) is a preview harness; real placement is generation's job (§10.1).

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{
    ascii_grid, generate, Cell, Direction, Facility, Guard, Input, Rng, State, Terrain,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement, KeyboardEvent};

/// Cell size in CSS pixels. A monospace glyph reads best in a slightly tall box.
const CELL_W: u32 = 14;
const CELL_H: u32 = 20;

/// Colours the shell owns until the colour-category system (§11.2) lands and maps
/// them from information categories. Concrete colours never live in the core (§11.2).
const BG: &str = "#0b0b0b";
const NEUTRAL: &str = "#cfcfcf"; // scenery
const PLAYER: &str = "#7ab8ff"; // the player glyph, so `@` reads at a glance
const GUARD: &str = "#e0803a"; // guards

/// Boot the game: generate a facility, place the player, draw it, and start listening
/// for input (§4.2, §13.1's build→play half).
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
    let ctx = mount_canvas(&document, state.layout().facility())?;

    let game = Rc::new(RefCell::new(Game { state, ctx }));
    game.borrow().draw();
    install_input(&document, &game)?;
    Ok(())
}

/// The running game plus the surface it draws to — the shell's whole mutable world.
struct Game {
    state: State,
    ctx: CanvasRenderingContext2d,
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

    /// Draw one frame: the terrain glyph grid, then the guards and the player on top.
    fn draw(&self) {
        let grid = ascii_grid(self.state.layout().facility());
        draw_grid(&self.ctx, &grid);
        for guard in self.state.guards() {
            draw_glyph(&self.ctx, guard.pos(), 'g', GUARD);
        }
        draw_glyph(&self.ctx, self.state.player(), '@', PLAYER);
    }
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

/// Create the canvas sized to the facility, mount it, and hand back its 2d context.
fn mount_canvas(
    document: &Document,
    facility: &Facility,
) -> Result<CanvasRenderingContext2d, JsValue> {
    // Mount into #app if the page provides it, else the body.
    let mount = document
        .get_element_by_id("app")
        .or_else(|| document.body().map(Into::into))
        .ok_or_else(|| JsValue::from_str("no mount point"))?;

    let canvas: HtmlCanvasElement = document
        .create_element("canvas")?
        .dyn_into::<HtmlCanvasElement>()?;
    canvas.set_width(facility.width() * CELL_W);
    canvas.set_height(facility.height() * CELL_H);
    mount.append_child(&canvas)?;

    Ok(canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .dyn_into::<CanvasRenderingContext2d>()?)
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

/// Paint a glyph grid: fill the background, then draw each non-blank glyph centred in
/// its cell. Blank (floor) cells are left as background. Sets the font and alignment
/// the on-top glyphs ([`draw_glyph`]) then reuse.
fn draw_grid(ctx: &CanvasRenderingContext2d, grid: &[String]) {
    let rows = grid.len() as u32;
    let cols = grid.first().map_or(0, |r| r.chars().count()) as u32;

    ctx.set_fill_style_str(BG);
    ctx.fill_rect(0.0, 0.0, (cols * CELL_W) as f64, (rows * CELL_H) as f64);

    ctx.set_fill_style_str(NEUTRAL);
    ctx.set_font(&format!("{}px ui-monospace, monospace", CELL_H - 2));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");

    for (y, row) in grid.iter().enumerate() {
        for (x, glyph) in row.chars().enumerate() {
            if glyph == ' ' {
                continue;
            }
            draw_char(ctx, x as u32, y as u32, glyph);
        }
    }
}

/// Draw one glyph at a cell in `color`, over the terrain grid. Relies on the font and
/// alignment [`draw_grid`] set for this frame.
fn draw_glyph(ctx: &CanvasRenderingContext2d, cell: Cell, glyph: char, color: &str) {
    ctx.set_fill_style_str(color);
    draw_char(ctx, cell.x, cell.y, glyph);
}

/// Paint a single character centred in cell `(x, y)` with the current fill style.
fn draw_char(ctx: &CanvasRenderingContext2d, x: u32, y: u32, glyph: char) {
    let px = x as f64 * CELL_W as f64 + CELL_W as f64 / 2.0;
    let py = y as f64 * CELL_H as f64 + CELL_H as f64 / 2.0;
    // fill_text only errors on an invalid surface; ignore the unit Ok.
    let _ = ctx.fill_text(&glyph.to_string(), px, py);
}
