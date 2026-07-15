//! Pure, deterministic game logic for Intrusion.
//!
//! This crate is the load-bearing half of the architecture (§12.1): it knows
//! nothing about rendering, input, the DOM, the clock, or the platform. Its
//! whole contract is `state × input → state, events`, and it must be testable
//! natively in milliseconds with no browser.
//!
//! So far it holds the seeded PRNG wrapper (§12.4) — the one primitive every
//! other system builds on — the smallest slice of the facility (a walled
//! rectangle, §4.1/§10, and the pure state→glyph-grid render, §11.1), and the
//! spatial region graph (§10.5): the model that gives corridors and rooms a name.
//! Game systems (generation, guards, vision, …) land in their own tickets.

#![forbid(unsafe_code)]

mod cell;
mod facility;
mod region;
mod render;
mod rng;

pub use cell::Cell;
pub use facility::{Facility, Terrain};
pub use region::{Door, DoorId, Region, RegionGraph, RegionId, RegionKind};
pub use render::ascii_grid;
pub use rng::Rng;
