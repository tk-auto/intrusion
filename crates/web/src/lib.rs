//! The thin web shell (§12.2): wasm-bindgen entry point plus, later, a
//! canvas2d glyph renderer and input handling. It stays deliberately thin — all
//! game logic lives in `intrusion-core`; this crate only draws state and feeds
//! it input.
//!
//! Right now it renders nothing. It exists so the workspace builds for
//! `wasm32-unknown-unknown` through wasm-bindgen and there is a real seam to
//! grow the renderer into.

use intrusion_core::Rng;
use wasm_bindgen::prelude::*;

/// Smoke-test entry point proving the core links and runs from wasm.
///
/// Given a seed, returns the run's first random draw. This is a placeholder for
/// the real bootstrap (canvas setup, input wiring, render loop) and exists only
/// to give wasm-bindgen something to export.
#[wasm_bindgen]
pub fn first_draw(seed: u64) -> u64 {
    let mut rng = Rng::new(seed);
    rng.next_u64()
}
