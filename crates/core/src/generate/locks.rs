//! The locked prize room (§10.4/§12.6/#236) — generation's last pass.
//!
//! With [`prize_room_locked`](crate::LevelModifiers::prize_room_locked) on, one room of
//! the finished board is shut behind a **key** every guard carries: pick the room, gate
//! its doorways ([`Layout::key_gate_door`]), and assert the §10.6 guarantee the lock
//! puts at risk.
//!
//! **It runs after [`place`](crate::place), not inside it**, and that ordering is the
//! whole reason this is a pass of its own. Which room holds the prize is not a fact
//! about the carve — it is decided by where the crates and consoles land — so the doors
//! that must be locked cannot be chosen while the doorways are being cut. Running last
//! also buys the strongest form of §12.6's determinism promise: this pass draws
//! **nothing** from the RNG, so a seed carves and places byte-identically whether the
//! modifier is on or off, and the two settings differ in exactly the doorways of one
//! room (§12.4).
//!
//! # What the §10.6 guarantee becomes
//!
//! Solvability is asserted twice, and the two halves say different things:
//!
//! - [`place`](crate::place) already proved the board solvable *with* a key — a keyed
//!   door routes exactly like any other closed door once you can open it, so that check
//!   is untouched by this pass.
//! - [`unlocked_reach_holds`] proves the lock gates **its own room and nothing else**:
//!   with every keyed doorway treated as a wall, the player still reaches the exit,
//!   every objective outside the room, the comms console — and **a guard**, which is
//!   what makes the key obtainable at all (§7.2). A seed where the lock seals away the
//!   way out, or every guard in the building, is rejected and redrawn like any other
//!   §10.6 shortfall.

use super::AUTO_CLOSE_DELAY;
use crate::cell::Cell;
use crate::facility::Terrain;
use crate::path;
use crate::place::{footholds, Placement};
use crate::region::RegionId;
use crate::Layout;
use std::collections::HashSet;

/// Lock the prize room's doorways (§10.4/#236), returning the room that was locked —
/// or `None` if this board has no room to lock, which is a placement to reject.
///
/// **Which room, in order of preference:**
///
/// 1. a room hiding an **equipment cache** (§10.2/#209) — the facility's richest prize,
///    and the one whose loss is a choice rather than a lost run;
/// 2. otherwise a room holding an **intel console**, which under quick play's
///    `IntelGate::All` makes the lock a gate on the win itself.
///
/// Within each of those two tiers, a room **no §10.7 duct opens into** is taken first. A
/// crawlspace mouth inside a locked room is a way round the lock that costs nothing, and
/// a modifier a shortcut walks past is a caption rather than a rule (§2.3). It is a
/// preference and not a gate: a board whose every prize room has a duct in it still
/// locks one, because rejecting the seed over it would be redrawing a whole facility to
/// tune one room's difficulty — see [`pick_prize_room`] for what that costs.
///
/// The start room can never be picked, because neither a crate nor a console is ever
/// placed in it (§10.1.8) — which is what keeps the player's own way out on the near
/// side of every lock.
pub(super) fn lock_prize_room(layout: &mut Layout, placement: &Placement) -> Option<RegionId> {
    let room = pick_prize_room(layout, placement)?;
    let doors = layout.regions().region(room).doors().to_vec();
    if doors.is_empty() {
        return None; // a room with no way in is a carve bug, never a lockable prize
    }
    for door in doors {
        layout.key_gate_door(door, AUTO_CLOSE_DELAY);
    }
    Some(room)
}

/// The prize room, by the preference order [`lock_prize_room`] documents: **what is in
/// it decides first**, and the duct only breaks ties inside a tier.
///
/// That ordering is deliberate and it costs something. A crate room a duct opens into is
/// locked ahead of a duct-free console room, so on those seeds the shortcut walks past
/// the lock and the modifier is thinner than it reads. The alternative — letting the
/// duct outrank the prize — decides *what the run is about* on a §10.7 accident, and
/// picking which prize is behind the door is the more important of the two. The tie-break
/// takes what it can within each tier and the cost is flagged rather than papered over.
fn pick_prize_room(layout: &Layout, placement: &Placement) -> Option<RegionId> {
    let regions = layout.regions();
    let rooms = |cells: &[Cell]| -> Vec<RegionId> {
        let mut ids: Vec<RegionId> = Vec::new();
        for &cell in cells {
            if let Some(id) = regions.region_at(cell) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids
    };
    // Crates first, then consoles, each in placement order — a pure function of the
    // board, so which room is locked is as reproducible as the board is (§12.4).
    [rooms(placement.caches()), rooms(placement.intel())]
        .into_iter()
        .find_map(|tier| {
            tier.iter()
                .copied()
                .find(|&id| !duct_opens_into(layout, id))
                .or_else(|| tier.first().copied())
        })
}

/// Whether a §10.7 crawlspace lets the player out **inside** `room` — a mouth of one of
/// the level's ducts standing next to one of its cells.
///
/// Mouths, not the path: a duct's interior may overlie ordinary room floor on its way
/// across the building, but the crawl is confined to the path and you leave it only at
/// an entry (§10.7), so an interior cell passing through a room is no way into it.
fn duct_opens_into(layout: &Layout, room: RegionId) -> bool {
    let regions = layout.regions();
    let facility = layout.facility();
    layout.ducts().iter().any(|duct| {
        duct.entries().into_iter().any(|entry| {
            facility
                .neighbours(entry)
                .any(|n| regions.region_at(n) == Some(room))
        })
    })
}

/// The §10.6 assertion the key lock owes (§10.4/#236): with every **keyed** doorway
/// treated as a wall, the player can still reach the way out, everything the lock was
/// not meant to gate, and **a guard to take the key from**.
///
/// This is the half [`crate::place::solvable`] cannot state. That check floods a board
/// where a closed panel is routable, which is the truth for a player holding the key —
/// so on its own it would happily accept a facility whose lock had sealed the exit, or
/// every guard in the building, behind the same door as the prize.
///
/// What must survive the locks:
///
/// - the **exit** — the run has to be able to end;
/// - every **objective**, **crate** and the **comms console** that is *not* in the
///   locked room — the lock gates its own room, and a second thing gated by accident is
///   a rule nobody chose;
/// - at least one **guard spawn**, because the only way to a key is a takedown (§7.2).
///   Without this the modifier could hand out a lock with no key in the building, which
///   is §2.2's soft lock wearing a different hat.
///
/// Bump targets are checked for **adjacency** to the flood rather than membership, like
/// every §10.6 reach check: a console is solid and is never stood on (§4.3). Guards are
/// checked for membership — a guard is stood beside by standing on the ground it walks.
pub(super) fn unlocked_reach_holds(
    layout: &Layout,
    placement: &Placement,
    locked: RegionId,
) -> bool {
    let facility = layout.facility();
    let regions = layout.regions();
    let keyed: HashSet<Cell> = regions
        .doors()
        .filter(|(_, door)| door.is_keyed())
        .flat_map(|(_, door)| door.cells().collect::<Vec<_>>())
        .collect();
    // The solid usables, exactly as `place::solvable` masks them: they are stamped on
    // the board a run is played on, so a route may not run through one.
    let solid: Vec<Cell> = placement
        .intel()
        .iter()
        .copied()
        .chain([placement.comms(), placement.exit()])
        .chain(placement.caches().iter().copied())
        .collect();
    let enterable = |c: Cell| {
        !keyed.contains(&c)
            && !solid.contains(&c)
            && facility.terrain(c).is_some_and(Terrain::routes_player)
    };

    let (w, h) = (facility.width(), facility.height());
    let reached: HashSet<Cell> =
        footholds(facility, placement.exit(), placement.exit_duct().cells())
            .filter(|&n| enterable(n))
            .flat_map(|foothold| path::flood_from(foothold, w, h, enterable))
            .collect();
    if reached.is_empty() {
        return false;
    }

    let outside = |cell: Cell| regions.region_at(cell) != Some(locked);
    let bumpable = |target: Cell| facility.neighbours(target).any(|n| reached.contains(&n));
    // The exit is never in the locked room (it is in the start room, which holds no
    // prize), so it is checked unconditionally — the way out is not something the lock
    // is ever allowed to gate.
    if !bumpable(placement.exit()) {
        return false;
    }
    if !solid
        .iter()
        .filter(|&&cell| outside(cell))
        .all(|&cell| bumpable(cell))
    {
        return false;
    }
    // A key in the building, and a way to it (§7.2).
    placement
        .guard_cells()
        .iter()
        .any(|cell| reached.contains(cell))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::generate_level;
    use crate::modifiers::{CacheCount, LevelModifiers};
    use crate::place::LevelConfig;
    use crate::region::{DoorKind, RegionKind};
    use crate::test_support::seed_sweep;
    use crate::Rng;

    /// The modifier on, over the recipe a real run plays.
    fn locked() -> LevelModifiers {
        LevelModifiers {
            prize_room_locked: true,
            ..LevelModifiers::default()
        }
    }

    /// Generate a placed level under `modifiers`, or panic with the seed that failed.
    fn level(seed: u64, modifiers: &LevelModifiers) -> (Layout, Placement) {
        generate_level(&LevelConfig::V1, modifiers, &mut Rng::new(seed))
            .unwrap_or_else(|e| panic!("seed {seed}: {e:?}"))
    }

    /// The region every keyed door of `layout` locks — and the assertion that there is
    /// exactly one, which is the modifier's shape: **one** room, all of its doorways.
    fn the_locked_room(layout: &Layout, seed: u64) -> RegionId {
        let regions = layout.regions();
        let rooms: Vec<RegionId> = regions
            .regions()
            .filter(|(id, region)| {
                regions.kind(*id) == RegionKind::Room
                    && region.doors().iter().any(|&d| regions.door(d).is_keyed())
            })
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            rooms.len(),
            1,
            "seed {seed}: one room is locked, not {rooms:?}"
        );
        let room = rooms[0];
        assert!(
            regions
                .region(room)
                .doors()
                .iter()
                .all(|&d| regions.door(d).is_keyed()),
            "seed {seed}: a room with an unlocked way in is not locked",
        );
        room
    }

    /// The cells a player **with no key** can come to occupy: the ordinary §10.6 flood
    /// with every keyed doorway masked solid. The frame both the §10.6 guarantee and
    /// the §2.3 directional assertion below are stated in.
    fn keyless_reach(layout: &Layout, placement: &Placement) -> HashSet<Cell> {
        let facility = layout.facility();
        let keyed: HashSet<Cell> = layout
            .regions()
            .doors()
            .filter(|(_, door)| door.is_keyed())
            .flat_map(|(_, door)| door.cells().collect::<Vec<_>>())
            .collect();
        let solid: Vec<Cell> = placement
            .intel()
            .iter()
            .copied()
            .chain([placement.comms(), placement.exit()])
            .chain(placement.caches().iter().copied())
            .collect();
        let enterable = |c: Cell| {
            !keyed.contains(&c)
                && !solid.contains(&c)
                && facility.terrain(c).is_some_and(Terrain::routes_player)
        };
        let (w, h) = (facility.width(), facility.height());
        footholds(facility, placement.exit(), placement.exit_duct().cells())
            .filter(|&n| enterable(n))
            .flat_map(|foothold| path::flood_from(foothold, w, h, enterable))
            .collect()
    }

    /// §10.4/#236: with the modifier on, **one** room is locked, it is a room holding a
    /// prize, and every one of its doorways is a shut, frameless, self-closing span.
    ///
    /// Each half matters. One room, because a modifier that locked two would be charging
    /// one takedown for two gates the player cannot tell apart. A room holding a prize,
    /// because a locked empty room is a wall with a caption on it (§2.3). And frameless
    /// and shut, because a lock on a door that stays open lasts exactly until the first
    /// patrol walks through it.
    #[test]
    fn one_prize_room_is_locked_behind_shut_automatic_doors() {
        for seed in seed_sweep(120) {
            let (layout, placement) = level(seed, &locked());
            let room = the_locked_room(&layout, seed);
            let prizes: Vec<Cell> = placement
                .intel()
                .iter()
                .chain(placement.caches())
                .copied()
                .filter(|&c| layout.regions().region_at(c) == Some(room))
                .collect();
            assert!(
                !prizes.is_empty(),
                "seed {seed}: the locked room holds nothing worth locking",
            );
            for &id in layout.regions().region(room).doors() {
                let door = layout.regions().door(id);
                assert!(!door.is_open(), "seed {seed}: a locked room starts shut");
                assert!(door.hinges().is_empty(), "seed {seed}: frameless");
                assert!(
                    matches!(door.kind(), DoorKind::Automatic { delay } if delay == AUTO_CLOSE_DELAY),
                    "seed {seed}: it shuts itself on the §10.4 delay",
                );
                assert!(
                    (3..=6).contains(&door.panels().len()),
                    "seed {seed}: {} panels is not a doorway",
                    door.panels().len(),
                );
                for &p in door.panels() {
                    assert_eq!(
                        layout.facility().terrain(p),
                        Some(Terrain::DoorPanelClosed),
                        "seed {seed}: the folded hinge is a panel now",
                    );
                }
            }
        }
    }

    /// #236: a **crate** room is locked in preference to a console room. The lock then
    /// gates loot rather than the win, which is the same rule reading as a choice — and
    /// it is what makes the modifier land differently on a campaign Vault than on a
    /// quick-play facility that hides no crates at all.
    #[test]
    fn a_crate_room_is_locked_in_preference_to_a_console_room() {
        let rich = LevelModifiers {
            caches: CacheCount::One,
            ..locked()
        };
        for seed in seed_sweep(60) {
            let (layout, placement) = level(seed, &rich);
            let room = the_locked_room(&layout, seed);
            let crate_room = layout.regions().region_at(placement.caches()[0]);
            assert_eq!(
                crate_room,
                Some(room),
                "seed {seed}: the crate is what the facility should be hiding",
            );
        }
    }

    /// §10.6 with the locks in place (#236): the lock gates **its own room and nothing
    /// else**. On every accepted seed, a player who has not taken a guard down still
    /// reaches the exit, every objective and crate outside the room, the comms console —
    /// and a guard, which is the only thing that sells keys (§7.2).
    ///
    /// This is a check of the *gate*, not of the generator's mood: the pass rejects a
    /// board that fails it and the carve is redrawn, so a failure here means the
    /// rejection is not wired up rather than that a seed was unlucky.
    #[test]
    fn the_lock_gates_its_own_room_and_nothing_else() {
        for seed in seed_sweep(120) {
            let (layout, placement) = level(seed, &locked());
            let room = the_locked_room(&layout, seed);
            assert!(
                unlocked_reach_holds(&layout, &placement, room),
                "seed {seed}: the lock sealed away more than its own room",
            );
        }
    }

    /// §2.3's **anti-facade** assertion, in the strongest frame any generation-time
    /// modifier here has (#236): the pass draws nothing, so the two settings are the
    /// **same board**, and the difference is exactly that a prize reachable at baseline
    /// is not reachable without a key.
    ///
    /// A modifier that changed nothing observable cannot pass for shipped, and this is
    /// what "observable" means for a lock: the flood a keyless player is confined to
    /// does not come within a bump of the thing behind the door.
    #[test]
    fn the_lock_puts_the_prize_out_of_reach() {
        for seed in seed_sweep(120) {
            let (layout, placement) = level(seed, &locked());
            let room = the_locked_room(&layout, seed);
            let reached = keyless_reach(&layout, &placement);
            let prizes = placement
                .intel()
                .iter()
                .chain(placement.caches())
                .copied()
                .filter(|&c| layout.regions().region_at(c) == Some(room));
            for prize in prizes {
                assert!(
                    !layout
                        .facility()
                        .neighbours(prize)
                        .any(|n| reached.contains(&n)),
                    "seed {seed}: {prize:?} is behind the lock and still reachable",
                );
            }

            // …and the same seed at baseline puts it within reach, which is the other
            // half of the claim: the lock is what moved, not the building.
            let (open_layout, open_placement) = level(seed, &LevelModifiers::default());
            let open_reach = keyless_reach(&open_layout, &open_placement);
            assert!(
                open_placement
                    .intel()
                    .iter()
                    .chain(open_placement.caches())
                    .all(|&c| open_layout
                        .facility()
                        .neighbours(c)
                        .any(|n| open_reach.contains(&n))),
                "seed {seed}: the baseline board is reachable throughout",
            );
        }
    }

    /// §12.4/§12.6 (#236): the pass consumes **no** RNG, so from one seed the modifier
    /// carves and places the identical board — same player, same exit, same intel, same
    /// crates, same guards, same radio clocks — and the only thing that differs anywhere
    /// on the grid is the terrain of the locked room's own doorways, where two hinges
    /// have become panels.
    ///
    /// That is a much stronger claim than the distributional one `automatic_doors` has
    /// to settle for, and it is what lets the assertion above be stated per seed rather
    /// than over a sweep.
    #[test]
    fn the_modifier_leaves_the_rest_of_the_building_alone() {
        for seed in seed_sweep(60) {
            let (open_layout, open_placement) = level(seed, &LevelModifiers::default());
            let (locked_layout, locked_placement) = level(seed, &locked());
            assert_eq!(
                open_placement, locked_placement,
                "seed {seed}: the lock reaches past placement, so it must not move it",
            );

            let room = the_locked_room(&locked_layout, seed);
            let doorway: HashSet<Cell> = locked_layout
                .regions()
                .region(room)
                .doors()
                .iter()
                .flat_map(|&d| locked_layout.regions().door(d).cells().collect::<Vec<_>>())
                .collect();
            let facility = open_layout.facility();
            for y in 0..facility.height() {
                for x in 0..facility.width() {
                    let cell = Cell::new(x, y);
                    if doorway.contains(&cell) {
                        continue;
                    }
                    assert_eq!(
                        facility.terrain(cell),
                        locked_layout.facility().terrain(cell),
                        "seed {seed}: {cell:?} moved, and only the doorways may",
                    );
                }
            }
        }
    }
}
