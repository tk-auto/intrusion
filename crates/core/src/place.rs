//! Entity placement — §10.1 steps 7–9, with the §10.6 spacing guarantees.
//!
//! Generation so far carves the board (#7–#11); this module puts the pieces on it:
//! the entry/exit tile, the player, the intel consoles, and the guards. The rules
//! are §10.1's — entry/exit and player in the **largest room**, objectives and
//! guards in any room *except* the start room — plus the three spacing guarantees
//! the old generator entirely lacked (§10.6): the player never spawns next to the
//! exit, the intel never clumps into one room, and no guard's turn-one cone covers
//! the spawn. *"The starting area should be safe" — make it so.*
//!
//! Two lessons from the old generator shape the module:
//!
//! - **Placement must not fail silently** (§10.6). Guards were quietly dropped
//!   (asked 5, got 4); objectives threw after 100 tries. Here a draw either places
//!   the **exact** requested counts or returns `None`, and the caller
//!   ([`generate_level`](crate::generate::generate_level)) rejects the carve and
//!   redraws from the same seed stream — the same loop that rejects a sealed or
//!   over-sighted carve, so "reject the seed" is one mechanism, not three.
//! - **Solvability is asserted after the pieces land, not before.** The §10.6 gate
//!   (#13) proves the *empty* carve is one pathable component — but consoles and
//!   the exit stamp in as solid (§10.3), and a console dropped into a 1-cell
//!   squeeze could pinch the player's only route. So placement re-floods the
//!   player's actual movement graph and requires every console and the exit to be
//!   bump-adjacent (§4.3) to it: start → every objective → exit, on the level as
//!   it will actually be played.

use crate::cell::Cell;
use crate::facility::{Facility, Terrain};
use crate::generate::{shuffle, Layout};
use crate::region::{RegionId, RegionKind};
use crate::rng::Rng;
use crate::state::GUARD_INITIAL_FACING;
use crate::vision::{field_of_view, GUARD_SIGHT_ARC, GUARD_SIGHT_RANGE};

/// The player and the exit spawn at least this far apart (Manhattan) **[START]**.
/// The old generator let them land adjacent — a run that starts won (§10.6). The
/// largest room on the v1 footprint comfortably spans this; a cramped draw that
/// cannot honour it is rejected and redrawn rather than quietly shrunk.
const PLAYER_EXIT_MIN_DISTANCE: u32 = 8;

/// A level recipe: the footprint and the piece counts (§10.2). v1 ships exactly
/// one tuned configuration — [`LevelConfig::V1`] — but the knobs are data so the
/// sim (§13) can sweep them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LevelConfig {
    /// Facility width in cells.
    pub width: u32,
    /// Facility height in cells.
    pub height: u32,
    /// How many guards to place — exactly this many, or the seed is rejected.
    pub guards: usize,
    /// How many intel consoles to place — exactly this many, or the seed is
    /// rejected. The v1 exit rule is *all intel required* (§10.2).
    pub intel: usize,
}

impl LevelConfig {
    /// The v1 configuration (§10.2): 40×40, 5 guards, 3 intel **[START]**.
    pub const V1: Self = Self {
        width: 40,
        height: 40,
        guards: 5,
        intel: 3,
    };
}

/// Where everything starts: the output of §10.1 steps 7–9. Cells only — the
/// *placement* is generation's concern; constructing actors from it (stationary
/// guards today, patrols when the guard AI lands) belongs to the caller.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Placement {
    player: Cell,
    exit: Cell,
    intel: Vec<Cell>,
    guards: Vec<Cell>,
}

impl Placement {
    /// The player's spawn cell, in the largest room (§10.1.7).
    pub fn player(&self) -> Cell {
        self.player
    }

    /// The entry/exit tile (§4.5: the run ends where it began), in the largest
    /// room, at least [`PLAYER_EXIT_MIN_DISTANCE`] from the spawn.
    pub fn exit(&self) -> Cell {
        self.exit
    }

    /// The intel consoles — one per room, never the start room (§10.1.8, §10.6).
    pub fn intel(&self) -> &[Cell] {
        &self.intel
    }

    /// The guard spawns — never the start room, never eyeing the player's spawn
    /// on turn one (§10.1.9, §10.6).
    pub fn guards(&self) -> &[Cell] {
        &self.guards
    }
}

/// Place the pieces on a finished carve, or `None` if this layout cannot honour
/// the counts and spacings — in which case the caller rejects the carve entirely
/// (§10.6: fail loudly or retry the seed; never a silent shortfall).
///
/// Deterministic from `rng` (§12.4): the same layout and stream always place the
/// same board.
pub(crate) fn place(layout: &Layout, config: &LevelConfig, rng: &mut Rng) -> Option<Placement> {
    let facility = layout.facility();

    // The rooms, each with its free floor cells (a region's cell set also holds
    // hideouts and door-adjacent floor; only plain floor takes a piece).
    let rooms: Vec<(RegionId, Vec<Cell>)> = layout
        .regions()
        .regions()
        .filter(|(id, _)| layout.regions().kind(*id) == RegionKind::Room)
        .map(|(id, region)| {
            let floor: Vec<Cell> = region
                .cells()
                .iter()
                .copied()
                .filter(|&c| facility.terrain(c) == Some(Terrain::Floor))
                .collect();
            (id, floor)
        })
        .collect();

    // §10.1.7: the largest room hosts entry/exit and player. Largest by *true*
    // floor area (a pillared room is not its bounding box, §10.5); ties break on
    // scan order, so the choice is deterministic.
    let start_idx = rooms
        .iter()
        .enumerate()
        .max_by_key(|(i, (_, floor))| (floor.len(), usize::MAX - i))
        .map(|(i, _)| i)?;
    let mut start_floor = rooms[start_idx].1.clone();

    // Exit first, then the player: the first shuffled pair far enough apart. A
    // cramped largest room with no such pair fails the draw rather than seating
    // them adjacent (§10.6).
    shuffle(&mut start_floor, rng);
    let (exit, player) = start_floor.iter().enumerate().find_map(|(i, &exit)| {
        start_floor[i + 1..]
            .iter()
            .find(|&&p| p.manhattan_distance(exit) >= PLAYER_EXIT_MIN_DISTANCE)
            .map(|&player| (exit, player))
    })?;

    let mut taken: Vec<Cell> = vec![exit, player];

    // §10.1.8 + §10.6: intel in any room except the start room — and *spread*, one
    // room each, so all three can never land in one room. Rooms are drawn in
    // shuffled order; too few distinct rooms fails the draw.
    let mut others: Vec<usize> = (0..rooms.len()).filter(|&i| i != start_idx).collect();
    shuffle(&mut others, rng);
    if others.len() < config.intel {
        return None;
    }
    let mut intel = Vec::with_capacity(config.intel);
    for &i in others.iter().take(config.intel) {
        intel.push(pick_free(&rooms[i].1, &taken, rng)?);
        taken.push(*intel.last().unwrap());
    }

    // §10.1.9 + §10.6: guards in any room except the start room, and never where
    // the turn-one cone — the real §6 field of view from the spawn cell, facing
    // south as every guard does at spawn (§7.1) — covers the player. This is the
    // same function the sight phase runs, not a conservative box, so "safe on
    // turn one" is exact. Candidates pool across all non-start rooms (guards may
    // share a room; intel cells are already taken), shuffled once; too few safe
    // cells fails the draw — asked-for-5-got-4 is precisely the old bug (§10.6).
    let mut guard_pool: Vec<Cell> = others
        .iter()
        .flat_map(|&i| rooms[i].1.iter().copied())
        .filter(|c| !taken.contains(c))
        .collect();
    shuffle(&mut guard_pool, rng);
    let guards: Vec<Cell> = guard_pool
        .into_iter()
        .filter(|&cell| {
            let cone = field_of_view(
                facility,
                cell,
                GUARD_INITIAL_FACING,
                GUARD_SIGHT_ARC,
                GUARD_SIGHT_RANGE,
            );
            !cone.contains(player)
        })
        .take(config.guards)
        .collect();
    if guards.len() < config.guards {
        return None;
    }

    let placement = Placement {
        player,
        exit,
        intel,
        guards,
    };
    // The post-placement solvability assertion: on the grid as it will actually be
    // played (consoles and exit solid), the player still reaches every objective
    // and the way out. §10.6's "assert it, don't argue it", applied once more
    // after the last pieces land.
    solvable(facility, &placement).then_some(placement)
}

/// A random cell of `floor` not already in `taken`, or `None` if the room is
/// exhausted. Draws by shuffled scan from `rng`, so it is deterministic and does
/// not loop unboundedly on a crowded room.
fn pick_free(floor: &[Cell], taken: &[Cell], rng: &mut Rng) -> Option<Cell> {
    let mut free: Vec<Cell> = floor
        .iter()
        .copied()
        .filter(|c| !taken.contains(c))
        .collect();
    if free.is_empty() {
        return None;
    }
    let i = rng.below(free.len() as u32) as usize;
    Some(free.swap_remove(i))
}

/// Whether the placed level is solvable by the player's actual movement rules:
/// start → every objective → exit (§10.6).
///
/// Floods the cells a *player* can come to occupy — floor, open **and closed**
/// panels (a bump opens them, §10.4), and hideouts (bump-to-enter, §10.3) —
/// with the placed console and exit cells masked solid, as they will be in play.
/// Consoles and the exit are bump-interactions, never stood on (§4.3), so each
/// must be **adjacent** to the flooded set rather than inside it. The pre-placement
/// §10.6 gate proved the empty carve connected; this catches the rarer sin of a
/// console stamped into a squeeze cell pinching the route that proof relied on.
fn solvable(facility: &Facility, placement: &Placement) -> bool {
    let solid: Vec<Cell> = placement
        .intel
        .iter()
        .copied()
        .chain([placement.exit])
        .collect();
    let enterable = |c: Cell| {
        !solid.contains(&c)
            && facility.terrain(c).is_some_and(|t| {
                matches!(
                    t,
                    Terrain::Floor
                        | Terrain::DoorPanelOpen
                        | Terrain::DoorPanelClosed
                        | Terrain::Hideout
                )
            })
    };

    let (w, h) = (facility.width(), facility.height());
    let mut seen = vec![false; (w * h) as usize];
    let idx = |c: Cell| (c.y * w + c.x) as usize;
    seen[idx(placement.player)] = true;
    let mut stack = vec![placement.player];
    while let Some(c) = stack.pop() {
        for n in facility.neighbors(c) {
            if enterable(n) && !seen[idx(n)] {
                seen[idx(n)] = true;
                stack.push(n);
            }
        }
    }

    solid
        .iter()
        .all(|&target| facility.neighbors(target).any(|n| seen[idx(n)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::generate_level;
    use crate::state::{Guard, State};
    use crate::{Direction, GenError, Outcome};

    /// The seeds every property below sweeps. Placement must hold on *accepted*
    /// seeds universally, not on a lucky one (§10.6).
    const SEEDS: u64 = 64;

    fn v1(seed: u64) -> (Layout, Placement) {
        generate_level(&LevelConfig::V1, &mut Rng::new(seed)).expect("the v1 config places")
    }

    /// The room region a cell belongs to.
    fn room_of(layout: &Layout, cell: Cell) -> RegionId {
        let id = layout
            .regions()
            .region_at(cell)
            .expect("a placed cell is in a region");
        assert_eq!(
            layout.regions().kind(id),
            RegionKind::Room,
            "pieces go in rooms"
        );
        id
    }

    /// The floor-cell count of a room region — the "largest room" measure.
    fn floor_area(layout: &Layout, id: RegionId) -> usize {
        layout
            .regions()
            .region(id)
            .cells()
            .iter()
            .filter(|&&c| layout.facility().terrain(c) == Some(Terrain::Floor))
            .count()
    }

    /// §10.6: **exactly** the requested counts, on every accepted seed — never the
    /// old asked-for-5-got-4 silent shortfall.
    #[test]
    fn accepted_seeds_place_exact_counts_on_plain_floor() {
        for seed in 0..SEEDS {
            let (layout, p) = v1(seed);
            assert_eq!(p.intel().len(), LevelConfig::V1.intel, "seed {seed}");
            assert_eq!(p.guards().len(), LevelConfig::V1.guards, "seed {seed}");

            // Every piece on its own plain floor cell — no stacking, no walls.
            let mut all = vec![p.player(), p.exit()];
            all.extend_from_slice(p.intel());
            all.extend_from_slice(p.guards());
            for &c in &all {
                assert_eq!(
                    layout.facility().terrain(c),
                    Some(Terrain::Floor),
                    "seed {seed}: {c:?} is not plain floor"
                );
            }
            let mut dedup = all.clone();
            dedup.sort_unstable_by_key(|c| (c.x, c.y));
            dedup.dedup();
            assert_eq!(
                dedup.len(),
                all.len(),
                "seed {seed}: two pieces share a cell"
            );
        }
    }

    /// §10.1.7 + §10.6 spacing: entry/exit and player share the **largest room**
    /// and never spawn within [`PLAYER_EXIT_MIN_DISTANCE`] of each other.
    #[test]
    fn player_and_exit_share_the_largest_room_well_apart() {
        for seed in 0..SEEDS {
            let (layout, p) = v1(seed);
            let start = room_of(&layout, p.player());
            assert_eq!(
                start,
                room_of(&layout, p.exit()),
                "seed {seed}: split spawn"
            );

            let start_area = floor_area(&layout, start);
            for (id, _) in layout.regions().regions() {
                if layout.regions().kind(id) == RegionKind::Room {
                    assert!(
                        floor_area(&layout, id) <= start_area,
                        "seed {seed}: the start room is not the largest"
                    );
                }
            }

            assert!(
                p.player().manhattan_distance(p.exit()) >= PLAYER_EXIT_MIN_DISTANCE,
                "seed {seed}: player and exit spawned {} apart",
                p.player().manhattan_distance(p.exit())
            );
        }
    }

    /// §10.1.8 + §10.6 spacing: every intel in a room that is neither the start
    /// room nor another intel's room — all three can never clump (the old bug).
    #[test]
    fn intel_spreads_across_distinct_non_start_rooms() {
        for seed in 0..SEEDS {
            let (layout, p) = v1(seed);
            let start = room_of(&layout, p.player());
            let rooms: Vec<RegionId> = p.intel().iter().map(|&c| room_of(&layout, c)).collect();
            assert!(
                !rooms.contains(&start),
                "seed {seed}: intel in the start room"
            );
            for (i, a) in rooms.iter().enumerate() {
                assert!(
                    !rooms[i + 1..].contains(a),
                    "seed {seed}: two intel share a room"
                );
            }
        }
    }

    /// §10.1.9 + §10.6 "the starting area should be safe": guards spawn outside
    /// the start room, and — checked through the **real** turn loop, consoles
    /// stamped and the startup turn run — no guard's turn-one cone covers the
    /// player's spawn.
    #[test]
    fn no_guard_eyes_the_spawn_on_turn_one() {
        for seed in 0..SEEDS {
            let (layout, p) = v1(seed);
            let start = room_of(&layout, p.player());
            for &g in p.guards() {
                assert_ne!(
                    room_of(&layout, g),
                    start,
                    "seed {seed}: guard in the start room"
                );
            }

            let guards = p.guards().iter().map(|&c| Guard::stationary(c)).collect();
            let state = State::new(
                layout,
                p.player(),
                Direction::North,
                guards,
                p.intel().iter().copied(),
                p.exit(),
            );
            assert_eq!(state.outcome(), Outcome::Playing, "seed {seed}");
            for guard in state.guards() {
                assert!(
                    !guard.fov().contains(state.player()),
                    "seed {seed}: the guard at {:?} sees the spawn on turn one",
                    guard.pos()
                );
            }
        }
    }

    /// §12.4: placement is deterministic — the same seed places the same board,
    /// piece for piece.
    #[test]
    fn placement_is_deterministic() {
        for seed in [0, 7, 2026] {
            let (_, a) = v1(seed);
            let (_, b) = v1(seed);
            assert_eq!(a, b, "seed {seed}");
        }
    }

    /// §10.6 "fail loudly or retry the seed": a config that can never be placed —
    /// more intel than rooms can exist — errors out with [`GenError::RetriesExhausted`]
    /// instead of shipping a shortfall or spinning forever.
    #[test]
    fn an_unplaceable_config_fails_loudly() {
        let impossible = LevelConfig {
            intel: 40, // room count is capped at ~12 (§10.2) — never satisfiable
            ..LevelConfig::V1
        };
        assert!(matches!(
            generate_level(&impossible, &mut Rng::new(0)),
            Err(GenError::RetriesExhausted { .. })
        ));
    }

    /// The post-placement solvability flood: a console sealed into a pocket the
    /// player cannot bump from outside fails the check; the same console with its
    /// pocket open passes. (On generated levels the §10.6 gate makes this rare —
    /// this pins the assertion itself.)
    #[test]
    fn solvability_requires_every_target_bump_adjacent() {
        let placement = |intel: Cell| Placement {
            player: Cell::new(5, 5),
            exit: Cell::new(8, 8),
            intel: vec![intel],
            guards: Vec::new(),
        };

        let mut sealed = Facility::walled_box(10, 10);
        // Wall the corner pocket at (1,1) shut: the console inside has no
        // reachable neighbour to bump it from.
        for (x, y) in [(2, 1), (1, 2), (2, 2)] {
            sealed.set_terrain(x, y, Terrain::Wall);
        }
        assert!(!solvable(&sealed, &placement(Cell::new(1, 1))));

        let open = Facility::walled_box(10, 10);
        assert!(solvable(&open, &placement(Cell::new(1, 1))));
    }
}
