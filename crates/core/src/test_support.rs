//! Shared test-only helpers.
//!
//! Home of the seed-sweep sampler (#60). The generation-driven property tests in
//! [`generate`](crate::generate), [`place`](crate::place), and [`door`](crate::door)
//! each sweep many seeds building full 40×40 facilities — corridor-first partition
//! plus the §10.6 reachability flood-fill — and those sweeps dominated `cargo test`
//! wall-clock, drifting from §12.1's "testable natively in milliseconds" goal. By
//! default a sweep runs a small spread of seeds so the routine gate stays fast; CI
//! sets `INTRUSION_SLOW_TESTS=1` to run every seed and preserve the full coverage —
//! the seeds are not dropped, just deferred off the every-`cargo test` path.
//!
//! It is also home to the recurring **bare-world builders** — an empty walled room
//! as a [`Layout`], the same box as a passability predicate, and a lone player in
//! one as a [`State`]. Test modules across the crate each used to re-derive these;
//! here they have one home, so an empty room reads the same everywhere.

use std::collections::HashSet;

use crate::cell::{Cell, Direction};
use crate::facility::Facility;
use crate::generate::Layout;
use crate::state::State;

/// The default sampled sweep width — small enough to keep the routine gate fast,
/// wide enough to spread across each sweep's range.
pub(crate) const SAMPLE_SEEDS: u64 = 12;

/// Whether to sweep every seed instead of the [`SAMPLE_SEEDS`] sample. CI sets
/// `INTRUSION_SLOW_TESTS=1` so the exhaustive sweep still runs on every push.
pub(crate) fn exhaustive_seeds() -> bool {
    std::env::var_os("INTRUSION_SLOW_TESTS").is_some()
}

/// The seeds a property test sweeps whose exhaustive range is `0..full`.
///
/// Full range under `INTRUSION_SLOW_TESTS`; otherwise a spread of at most
/// [`SAMPLE_SEEDS`] seeds sampled across the whole range, so low *and* high seeds
/// are still exercised. A sampled failure still prints its seed, and the exhaustive
/// CI run (or `INTRUSION_SLOW_TESTS=1` locally) reproduces it.
pub(crate) fn seed_sweep(full: u64) -> Vec<u64> {
    if exhaustive_seeds() || full <= SAMPLE_SEEDS {
        (0..full).collect()
    } else {
        (0..SAMPLE_SEEDS).map(|i| i * full / SAMPLE_SEEDS).collect()
    }
}

/// An open room: a `w × h` walled box, all interior floor, wrapped as a bare
/// layout. Enough to drive movement, objectives, and capture without generation.
pub(crate) fn open_room(w: u32, h: u32) -> Layout {
    Layout::from_facility(Facility::walled_box(w, h))
}

/// A passability predicate for a `w × h` open box (cells `[0,w) × [0,h)`) with a
/// set of blocked cells punched out — an infinite-grid predicate for pathing tests,
/// the counterpart to [`open_room`]'s real bounded [`Layout`].
pub(crate) fn open_box(w: u32, h: u32, walls: &[Cell]) -> impl Fn(Cell) -> bool {
    let blocked: HashSet<Cell> = walls.iter().copied().collect();
    move |c: Cell| c.x < w && c.y < h && !blocked.contains(&c)
}

/// A player in an empty room, facing north, no guards or objectives, exit unused
/// in a far corner.
pub(crate) fn solo(player: Cell) -> State {
    State::new(
        open_room(10, 10),
        player,
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(8, 8),
    )
}
