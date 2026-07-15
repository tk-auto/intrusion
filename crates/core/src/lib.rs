//! Pure, deterministic game logic for Intrusion.
//!
//! This crate is the load-bearing half of the architecture (§12.1): it knows
//! nothing about rendering, input, the DOM, the clock, or the platform. Its
//! whole contract is `state × input → state, events`, and it must be testable
//! natively in milliseconds with no browser.
//!
//! So far it holds the seeded PRNG wrapper (§12.4) — the one primitive every
//! other system builds on — the grid substrate (§4.1/§4.3/§10.3): the terrain
//! table, the cell-capacity occupancy query, and 4-directional movement with
//! Manhattan distance, all wrapped in the indestructible border the facility
//! guarantees. On top of it: the pure state→glyph-grid render (§11.1), the spatial
//! region graph (§10.5) that gives corridors and rooms a name, the corridor-first
//! partition (§10.1) that carves them, and the hinged doors (§10.4) it cuts where
//! rooms meet corridors. On top of all that sits the turn loop (§4.2): the running
//! [`State`], `state × input → state, events`, resolving player, sight, and guards in
//! order with the turn-cost rule and the two win/lose conditions. The remaining game
//! systems (real vision, guard AI, sound, abilities) land in their own tickets, in the
//! phase hooks the loop already calls.

#![forbid(unsafe_code)]

mod cell;
mod door;
mod facility;
mod generate;
mod region;
mod render;
mod rng;
mod state;

pub use cell::{Cell, Direction};
pub use door::DoorAction;
pub use facility::{Facility, SoundBlocking, Terrain};
pub use generate::{generate, GenError, Layout};
pub use region::{Door, DoorCell, DoorId, Region, RegionGraph, RegionId, RegionKind};
pub use render::ascii_grid;
pub use rng::Rng;
pub use state::{Event, Guard, Input, Outcome, State};
