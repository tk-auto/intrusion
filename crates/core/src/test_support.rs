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
use crate::guard::Guard;
use crate::state::{Event, Input, State};

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

/// The interior cells of a `w × h` walled box — the §7.5 beat a *fixture* guard is
/// handed so it has somewhere to patrol.
///
/// A beat is normally cut from the §10.5 region graph ([`crate::beat`]), and a
/// hand-built room has none; a guard with no beat has no territory and holds
/// ([`Guard::territory`](crate::Guard)). So a test that wants a guard to actually
/// sweep an open room says so with this, rather than relying on a box drawn around a
/// remembered spawn cell — the anchor #398 removed.
pub(crate) fn open_beat(w: u32, h: u32) -> Vec<Cell> {
    (1..h.saturating_sub(1))
        .flat_map(|y| (1..w.saturating_sub(1)).map(move |x| Cell::new(x, y)))
        .collect()
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

/// The player's own tunnel (§4.5/§10.7/#466), hand-laid: the straight run of cells from
/// `exit` out to the border along `dir`, `exit` first and the way-out cell last. The
/// fixture twin of the generator's `carve_exit_duct`, for a state that needs a real way
/// in and out without running a carve.
pub(crate) fn exit_tunnel_cells(w: u32, h: u32, exit: Cell, dir: Direction) -> Vec<Cell> {
    let mut cells = vec![exit];
    let mut cell = exit;
    while let Some(next) = cell.step(dir) {
        assert!(next.x < w && next.y < h, "the run left the grid");
        cells.push(next);
        if next.x == 0 || next.y == 0 || next.x == w - 1 || next.y == h - 1 {
            return cells;
        }
        cell = next;
    }
    panic!("a straight run always meets the border");
}

/// [`open_room`] with the player's own tunnel on it (§4.5/#466) — an empty `w × h` box
/// whose `exit` is the inner mouth of a crawlspace running out to the border along
/// `dir`. Pair it with [`exit_tunnel_cells`] to name the way-out cell a run starts on.
pub(crate) fn room_with_tunnel(w: u32, h: u32, exit: Cell, dir: Direction) -> Layout {
    let mut layout = open_room(w, h);
    layout.set_exit_duct(crate::duct::Duct::exit_tunnel(exit_tunnel_cells(
        w, h, exit, dir,
    )));
    layout
}

/// Crawl out of the tunnel the run starts in (§4.5/#466) and step into the facility —
/// the opening every real run plays, for a fixture whose subject is what happens
/// *after* it. Leaves the player standing on the floor beside `E`; a no-op for a
/// hand-built state that starts on foot.
pub(crate) fn climb_out_of_the_tunnel(state: &mut State) {
    use crate::state::Input;
    for _ in 0..64 {
        let Some(duct) = state.occupied_duct() else {
            return;
        };
        let cells = duct.cells().to_vec();
        let i = cells
            .iter()
            .position(|&c| c == state.player())
            .expect("the crawler is on its own path");
        let dir = if i > 0 {
            // Crawl on toward the mouth.
            Direction::between(cells[i], cells[i - 1]).expect("the path is contiguous")
        } else {
            // On the mouth: climb out onto the floor it opens into. **Straight ahead**
            // where that works — the mouth faces out along the tunnel's own axis, so
            // this is the cell a player who kept crawling would come up on — and any
            // other side otherwise. Never back onto the path, whose cells may themselves
            // be floor the crawl merely overlies (§10.7 cross-room routing); a step onto
            // one of those is another crawl.
            let facility = state.layout().facility();
            let out_of = |n: Cell| !cells.contains(&n) && facility.can_enter(n, 1.0);
            let ahead = state.player().step(state.facing()).filter(|&n| out_of(n));
            let out = ahead
                .or_else(|| facility.neighbours(state.player()).find(|&n| out_of(n)))
                .expect("a mouth opens onto somewhere");
            Direction::between(state.player(), out).expect("a neighbour is one step away")
        };
        state.step(Input::Step(dir));
    }
    panic!("the tunnel is not that long")
}

/// Leave by the tunnel (§4.5/#466), the way a player does: climb in at `E`, crawl to the
/// border, and step off the board. Returns the events of that **last** step — the win, or
/// the §4.5 refusal if the intel gate is not met (in which case the player is left
/// standing on the way-out cell, exactly as a real refusal leaves them).
///
/// It drives itself off the **usable line** ([`State::affordances`]), so a fixture that
/// uses it is asserting through the same row the player reads.
pub(crate) fn leave_by_the_tunnel(state: &mut State) -> Vec<Event> {
    use crate::state::{Affordance, Input};
    let aimed = |state: &State, want: Affordance| {
        state
            .affordances()
            .into_iter()
            .find_map(|(dir, a)| (a == want).then_some(dir?))
    };
    for _ in 0..64 {
        if let Some(dir) =
            aimed(state, Affordance::Leave).or_else(|| aimed(state, Affordance::ExitRefused))
        {
            return state.step(Input::Step(dir));
        }
        let dir = if let Some(dir) = aimed(state, Affordance::EnterExit) {
            dir
        } else {
            // Inside the tunnel: crawl on, away from the mouth.
            let duct = state.occupied_duct().expect("at the mouth or inside it");
            let cells = duct.cells();
            let i = cells
                .iter()
                .position(|&c| c == state.player())
                .expect("the crawler is on its own path");
            Direction::between(cells[i], cells[i + 1]).expect("the path is contiguous")
        };
        state.step(Input::Step(dir));
    }
    panic!("the tunnel is not that long")
}

/// A hand-built wide strip: a left room and a right room joined by one **manual**
/// door at column 6 (hinges at `(6,1)`/`(6,3)`, panel at `(6,2)`), with a guard
/// patrolling from the left room through the door — so on its beat it walks the
/// closed panel open (§10.4), a change the player did not cause. The player starts at
/// `player` **facing east**, ahead of the door, and the drive below walks it further
/// east each turn: the guard opens the door *behind* the eastward-facing player, so
/// the changed cell is reliably out of the forward FOV (a Wait's 360° look would
/// otherwise see straight through the open doorway — sight and door-sense share the
/// same range, §9.1/§10.4). The close-behind is disabled so the open is isolated.
/// Returns the state and the door's panel cell.
pub(crate) fn guard_door_strip(width: u32, player: Cell) -> (State, Cell) {
    let mut f = Facility::walled_box(width, 6);
    let mut g = RegionGraph::new(width, 6);
    let column =
        |x0: u32, x1: u32| (1..5).flat_map(move |y| (x0..x1).map(move |x| Cell::new(x, y)));
    let left = g.add_region(RegionKind::Room, column(1, 6));
    let right = g.add_region(RegionKind::Room, column(7, width - 1));
    for y in 1..5 {
        f.set_terrain(6, y, Terrain::Wall);
    }
    f.set_terrain(6, 1, Terrain::DoorHinge);
    f.set_terrain(6, 2, Terrain::DoorPanelClosed);
    f.set_terrain(6, 3, Terrain::DoorHinge);
    g.add_door(
        left,
        right,
        [Cell::new(6, 1), Cell::new(6, 3)],
        [Cell::new(6, 2)],
        DoorKind::Manual,
    );
    let mut s = State::new(
        Layout::from_parts(f, g),
        player,
        Direction::East,
        vec![Guard::patrolling_to(Cell::new(4, 2), Cell::new(8, 2))],
        Vec::new(),
        Cell::new(width - 2, 4),
    );
    s.set_guard_close_chance(0); // isolate the open from the close-behind (#146)
    (s, Cell::new(6, 2))
}

/// Walk the player east until the patrolling guard opens the door behind them (the
/// first `by_player: false` open), returning once it has. Stepping east keeps the
/// player facing *away* from the door so the changed cell stays out of the forward
/// FOV. Panics if the guard never opens it.
pub(crate) fn drive_until_guard_opens(s: &mut State) {
    for _ in 0..8 {
        let e = s.step(Input::Step(Direction::East));
        if e.iter().any(|ev| {
            matches!(
                ev,
                Event::DoorOpened {
                    by_player: false,
                    ..
                }
            )
        }) {
            return;
        }
    }
    panic!("the patrolling guard never opened the door");
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

/// Whether `events` reports a capture on `cell` (§4.5).
///
/// The event carries the whole cause since #138 — the guard's index and the mood it
/// made contact in — and a test that asserted on the *cell* has no business pinning
/// either: which of four guards reached you is an artefact of placement order, and a
/// fixture that spelled it out would fail the day a guard is placed differently while
/// still walking into the same cell. So the shared assertion reads the one field the
/// test means, and the fields it does not mean are pinned where they are the point
/// ([`crate::render::verdict`], the cause line).
pub(crate) fn captured_at(events: &[Event], cell: Cell) -> bool {
    events
        .iter()
        .any(|event| matches!(event, Event::Captured { at, .. } if *at == cell))
}
