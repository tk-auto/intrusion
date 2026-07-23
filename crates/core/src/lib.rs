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
//! guarantees. On top of it: the pure state→glyph-grid render (§11.1, drawn
//! through the §11.5a fog — geometry always, contents once seen then remembered,
//! live state only in the current FOV), the spatial
//! region graph (§10.5) that gives corridors and rooms a name, the corridor-first
//! partition (§10.1) that carves them, and the hinged doors (§10.4) it cuts where
//! rooms meet corridors. On top of all that sits the turn loop (§4.2): the running
//! [`State`], `state × input → state, events`, resolving player, sight, and guards in
//! order with the turn-cost rule and the two win/lose conditions — and the sight phase
//! is real: the symmetric-shadowcast field of view (§6), the player's half-disc and
//! the guards' wedges, recomputed every turn. Guards detect on **vision alone** (§9
//! **[SETTLED]** — there is no sound, no hearing): a guard reacts only to what it
//! sees. On top of the loop sits the **ability economy** (§8.1/§8.2): a data-driven
//! ability catalog and the time economy — turn cost, duration, cooldown — stepped
//! at the end-of-turn hook the loop reserves, with the `duration + cooldown` lockout
//! emergent. The individual ability *effects*, guard AI, and the rest land in their
//! own tickets, in the phase hooks the loop already calls.

#![forbid(unsafe_code)]

mod ability;
mod beat;
mod body;
mod category;
mod cell;
mod cover;
mod door;
mod facility;
mod generate;
mod guard;
mod input;
mod path;
mod place;
mod radio;
mod region;
mod render;
mod rng;
mod state;
mod status;
mod targeting;
#[cfg(test)]
mod test_support;
mod vision;

pub use ability::{
    Ability, AbilityId, AbilityState, AbilityStatus, Behaviour, Effect, TargetingMode,
};
pub use body::Body;
pub use category::Category;
pub use cell::{Cell, Direction};
pub use door::DoorAction;
pub use facility::{Facility, Terrain};
pub use generate::{generate, generate_level, GenError, Layout, SIGHTLINE_MAX_RUN};
pub use guard::{Guard, GuardState};
pub use input::{
    ability_hotkey, ability_input_for_key, input_for_key, ui_command_for_key, UiCommand,
};
pub use place::{LevelConfig, Placement};
pub use region::{Door, DoorCell, DoorId, Region, RegionGraph, RegionId, RegionKind};
pub use render::{
    ability_at, ascii_grid, is_ability_button, render, render_screen, GlyphCell, Grid, ScreenUi,
    Visibility, HEADER_ROWS, STATUS_ROWS,
};
pub use rng::Rng;
pub use state::{
    Affordance, Event, GuardPerception, Input, Outcome, State, PLAYER_SENSE_RANGE,
    PLAYER_SENSE_RANGE_WAITING,
};
pub use status::{message_for, near_line, Message};
pub use targeting::{within_range, Target, Targeting, TileCursor};
pub use vision::{
    field_of_view, field_of_view_with_peek, VisibleSet, GUARD_SIGHT_ARC, GUARD_SIGHT_RANGE,
    PLAYER_SIGHT_ARC, PLAYER_SIGHT_RANGE, WAIT_SIGHT_ARC,
};
