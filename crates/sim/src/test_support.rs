//! Shared test scaffolding for the sim's own unit tests — never compiled into the
//! binary (`#[cfg(test)]`).
//!
//! One helper so far: booting a real level exactly as the harness does, so a test
//! reasons about the game generation actually produces rather than a hand-built
//! fixture.

use intrusion_core::{generate_level, Direction, LevelConfig, Placement, Rng, State};

/// Boot a real V1 level exactly as the harness does (§13.2), returning the state
/// and the placement — so a test can compare against the ground truth the bot must
/// *not* peek at.
pub fn boot(seed: u64) -> (State, Placement) {
    let (layout, placement) = generate_level(
        &LevelConfig::V1,
        &intrusion_core::LevelModifiers::default(),
        &mut Rng::new(seed),
    )
    .expect("V1 generates");
    let guards = placement.guards(&layout);
    let state = State::new(
        layout,
        placement.player(),
        Direction::North,
        guards,
        placement.intel().iter().copied(),
        placement.exit(),
    );
    (state, placement)
}
