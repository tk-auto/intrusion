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
use crate::facility::{Facility, Terrain};
use crate::generate::Layout;
use crate::region::{DoorKind, RegionGraph, RegionKind};
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

/// A hand-built strip of four regions with real doors, terrain and graph in
/// lockstep — the fixture for region beats and guard-opened doors:
///
/// ```text
///   col 0    4  7   11   15
///   ################   row 0
///   #AAA×CC×BBB×DDD#
///   #AAA+CC+BBB+DDD#   doors (hinge/panel/hinge) in the wall columns
///   #AAA×CC×BBB×DDD#
///   #AAA#CC#BBB#DDD#
///   ################   row 5
/// ```
///
/// Room A, corridor C, room B, corridor D, one closed door between each pair —
/// the only way along the strip is through the doors. A beat of three regions
/// grown from A covers A+C+B and leaves D outside, which is where these tests
/// park the player.
pub(crate) fn region_strip() -> Layout {
    let mut f = Facility::walled_box(16, 6);
    let mut g = RegionGraph::new(16, 6);
    let column =
        |x0: u32, x1: u32| (1..5).flat_map(move |y| (x0..x1).map(move |x| Cell::new(x, y)));
    let a = g.add_region(RegionKind::Room, column(1, 4));
    let c = g.add_region(RegionKind::Corridor, column(5, 7));
    let b = g.add_region(RegionKind::Room, column(8, 11));
    let d = g.add_region(RegionKind::Corridor, column(12, 15));
    for (x, near, far) in [(4, a, c), (7, c, b), (11, b, d)] {
        for y in 1..5 {
            f.set_terrain(x, y, Terrain::Wall);
        }
        f.set_terrain(x, 1, Terrain::DoorHinge);
        f.set_terrain(x, 2, Terrain::DoorPanelClosed);
        f.set_terrain(x, 3, Terrain::DoorHinge);
        g.add_door(
            near,
            far,
            [Cell::new(x, 1), Cell::new(x, 3)],
            [Cell::new(x, 2)],
            DoorKind::Manual,
        );
    }
    Layout::from_parts(f, g)
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
