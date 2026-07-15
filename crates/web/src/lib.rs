//! The thin web shell (§12.2): the wasm-bindgen entry point and a canvas2d glyph
//! renderer. It stays deliberately thin — all game logic lives in
//! `intrusion-core`; this crate only draws the core's state and (later) feeds it
//! input.
//!
//! Right now it draws one thing: the facility's glyph grid (§11.1), painted a
//! character at a time with `fillText`. That is the whole renderer contract —
//! tiles later swap `fillText` for `drawImage` behind the same idea (§12.2), and
//! colour categories, fog and input all grow from here in their own tickets.

use intrusion_core::{ascii_grid, generate, Rng};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// Cell size in CSS pixels. A monospace glyph reads best in a slightly tall box.
const CELL_W: u32 = 14;
const CELL_H: u32 = 20;

/// The page background and the neutral glyph colour (§11.2 Neutral = scenery).
/// Concrete colours live here in the shell, not in the core, until the colour
/// *category* system (§11.2) lands and owns the mapping.
const BG: &str = "#0b0b0b";
const NEUTRAL: &str = "#cfcfcf";

/// Boot the renderer: generate a facility, mount a canvas, draw it once.
///
/// This is the wasm entry point the page calls after the module initialises. It
/// draws a static frame — the render loop and input pump land with the turn
/// loop; for now it proves the pipeline core → glyph grid → canvas → Pages, and
/// shows the corridor-first partition (§10.1) doing its job. Reload for a new
/// seed; explicit seed entry / sharing (§13.1) is a later ticket.
#[wasm_bindgen]
pub fn start() -> Result<(), JsValue> {
    // The seed is the one impurity the shell owns (§12.1 keeps the *core* pure):
    // read the clock here so each load is a different facility, and hand the core
    // a plain u64. The v1 footprint is 40×40 (§10.2).
    let seed = js_sys::Date::now() as u64;
    let layout = generate(40, 40, &mut Rng::new(seed))
        .map_err(|e| JsValue::from_str(&format!("generation failed: {e:?}")))?;
    let facility = layout.facility();
    let grid = ascii_grid(facility);

    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("no document"))?;

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

    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .dyn_into::<CanvasRenderingContext2d>()?;

    draw_grid(&ctx, &grid);
    Ok(())
}

/// Paint a glyph grid: fill the background, then draw each non-blank glyph
/// centred in its cell. Blank (floor) cells are left as background.
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
            let px = x as f64 * CELL_W as f64 + CELL_W as f64 / 2.0;
            let py = y as f64 * CELL_H as f64 + CELL_H as f64 / 2.0;
            // fill_text only errors on an invalid surface; ignore the unit Ok.
            let _ = ctx.fill_text(&glyph.to_string(), px, py);
        }
    }
}
