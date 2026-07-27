//! Pierce Wall through the turn loop (§8.3/§8.4, #303).
//!
//! The precondition is the design, so most of this file is refusals: the rule that
//! *exactly one* wall may touch the player is what rules the panic-bore out by
//! construction, and each way of failing it is pinned separately because each is a
//! different thing for the player to do about it.
//!
//! The other half is the §2.3 facade check. A hole only the player could use would
//! be this ticket's whole failure mode, so the guard tests below prove the opposite
//! from the guard's side: a cone reaches through a fresh hole, and a guard walks
//! through it.

use std::collections::HashSet;

use crate::ability::PIERCE_WALL_USES;
use crate::facility::Facility;
use crate::generate::generate_level;
use crate::place::LevelConfig;
use crate::region::{RegionGraph, RegionKind};
use crate::state::*;
use crate::test_support::open_room;
use crate::Rng;

/// A player holding Pierce Wall in a `w × h` walled box, facing north.
fn borer(layout: Layout, player: Cell) -> State {
    State::new(
        layout,
        player,
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(1, 1),
    )
    .with_loadout(Loadout::innate().with(AbilityId::PierceWall))
}

/// An open room with a single interior wall standing east of a player at (5,5) —
/// the one geometry the ability is usable in.
fn one_wall() -> State {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(6, 5), Terrain::Wall);
    borer(layout, Cell::new(5, 5))
}

/// **The ability, working** (§8.3/#303): standing against exactly one wall, the bore
/// turns it into ordinary floor, spends the turn (§4.4), reports itself, and the
/// player walks straight through the route they just cut.
#[test]
fn one_adjacent_wall_bores_and_the_player_walks_through() {
    let mut s = one_wall();
    let wall = Cell::new(6, 5);
    assert_eq!(
        s.bore_target(),
        Ok(wall),
        "exactly one wall, and it is that one"
    );

    let events = s.step(Input::Activate(AbilityId::PierceWall));
    assert!(
        events.contains(&Event::WallBored { at: wall }),
        "the bore reports itself: {events:?}",
    );
    assert_eq!(
        s.turn(),
        1,
        "boring changes the world, so it costs the turn"
    );
    assert_eq!(
        s.layout().facility().terrain(wall),
        Some(Terrain::Floor),
        "the hole is ordinary floor in the one spatial model (§10.5)",
    );

    // And it is a route, not a picture: the player steps into it and out the far side.
    s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), wall);
    s.step(Input::Step(Direction::East));
    assert_eq!(
        s.player(),
        Cell::new(7, 5),
        "through, and out the other side"
    );
}

/// **Nothing to bore** (§8.4): standing in the open, no wall touches the player. A
/// free no-op that says which case it is — silence would read as a dropped key.
#[test]
fn no_adjacent_wall_is_a_free_refusal_that_speaks() {
    let mut s = borer(open_room(12, 12), Cell::new(5, 5));
    assert_eq!(s.bore_target(), Err(BoreRefusal::NothingToBore));

    let events = s.step(Input::Activate(AbilityId::PierceWall));
    assert_eq!(
        events,
        vec![Event::BoreRefused {
            reason: BoreRefusal::NothingToBore
        }],
    );
    assert_eq!(s.turn(), 0, "a refused activation is free (§4.4)");
    assert_eq!(
        s.ability_state(AbilityId::PierceWall),
        AbilityState::Limited {
            uses: PIERCE_WALL_USES
        },
        "and costs no use (§8.2/#302)",
    );
}

/// **Two walls are never disambiguated** (§8.4 [SETTLED]) — and this is the rule
/// that carries the balance, not a limitation to be worked around. A corridor has
/// two side walls and a corner has two walls, so the panic-bore mid-chase is ruled
/// out *by construction*: you can only cut a route from open floor.
#[test]
fn two_or_more_adjacent_walls_are_refused_never_chosen_between() {
    // A corridor: walls on both flanks, open ahead and behind.
    let mut corridor = open_room(12, 12);
    corridor.place(Cell::new(5, 4), Terrain::Wall);
    corridor.place(Cell::new(5, 6), Terrain::Wall);
    let mut s = borer(corridor, Cell::new(5, 5));
    assert_eq!(s.bore_target(), Err(BoreRefusal::TooManyWalls));
    let events = s.step(Input::Activate(AbilityId::PierceWall));
    assert_eq!(
        events,
        vec![Event::BoreRefused {
            reason: BoreRefusal::TooManyWalls
        }],
    );
    assert_eq!(s.turn(), 0, "free");

    // A corner of the facility's own shell: two boundary walls meet there, so the
    // count is two and the refusal is ambiguity, not the shell.
    let corner = borer(open_room(12, 12), Cell::new(1, 1));
    assert_eq!(corner.bore_target(), Err(BoreRefusal::TooManyWalls));

    // Three walls — a dead-end alcove — is the same refusal.
    let mut alcove = open_room(12, 12);
    for cell in [Cell::new(5, 4), Cell::new(5, 6), Cell::new(6, 5)] {
        alcove.place(cell, Terrain::Wall);
    }
    let dead_end = borer(alcove, Cell::new(5, 5));
    assert_eq!(dead_end.bore_target(), Err(BoreRefusal::TooManyWalls));
}

/// **The outer shell is never a route** (§1/§4.5): the intruder enters and leaves by
/// their own tunnel and there is no other exit, so boring out through the facility's
/// boundary is refused outright — with its own message, because the reason is a rule
/// about the world rather than about this wall.
#[test]
fn the_outer_shell_is_refused_and_still_counts_as_a_wall() {
    // Standing against the boundary alone: exactly one wall, and it is the shell.
    let mut s = borer(open_room(12, 12), Cell::new(1, 5));
    assert_eq!(s.bore_target(), Err(BoreRefusal::TheOuterShell));
    let events = s.step(Input::Activate(AbilityId::PierceWall));
    assert_eq!(
        events,
        vec![Event::BoreRefused {
            reason: BoreRefusal::TheOuterShell
        }],
    );
    assert_eq!(
        s.layout().facility().terrain(Cell::new(0, 5)),
        Some(Terrain::Wall),
        "the shell is untouched",
    );

    // Refusing it does not make it stop *counting*: beside one shell wall and one
    // interior wall the bore is refused twice over, and ambiguity is what it reports.
    let mut both = open_room(12, 12);
    both.place(Cell::new(1, 4), Terrain::Wall);
    let s = borer(both, Cell::new(1, 5));
    assert_eq!(s.bore_target(), Err(BoreRefusal::TooManyWalls));
}

/// **A thick wall bores into a pocket, and the pocket is the point** — the ticket's
/// open question, decided the other way.
///
/// The generator deliberately thickens about a third of the interior wall runs to
/// two cells (§10.1.5), so boring one opens a one-cell alcove rather than a route.
/// The ability does not ask what is behind the wall and does not refuse this: a
/// dead-end pocket off the room, out of the through-routes a patrol sweeps, is
/// somewhere to sit a sweep out. It conceals nothing — it is not a cupboard (§10.3)
/// — so whether it is shelter or a trap is the player's judgement, which is the kind
/// of decision worth handing them rather than a refusal.
///
/// The one thing the pocket is *not* is a way onward: standing in it leaves three
/// walls around you, so the ability is unusable from inside — you can dig a hole to
/// hide in, never a tunnel.
#[test]
fn a_thick_wall_bores_into_a_pocket_the_player_can_hide_in() {
    // A two-cell-thick wall course, exactly as `thicken_walls` builds them, so the
    // alcove a bore opens is genuinely enclosed on three sides.
    let mut layout = open_room(12, 12);
    for y in 1..11 {
        layout.place(Cell::new(6, y), Terrain::Wall);
        layout.place(Cell::new(7, y), Terrain::Wall);
    }
    let mut s = borer(layout, Cell::new(5, 5));

    let pocket = Cell::new(6, 5);
    assert_eq!(
        s.bore_target(),
        Ok(pocket),
        "thickness is not the ability's business"
    );
    s.step(Input::Activate(AbilityId::PierceWall));
    assert_eq!(
        s.layout().facility().terrain(pocket),
        Some(Terrain::Floor),
        "the alcove is real floor like any other hole",
    );
    assert_eq!(
        s.ability_state(AbilityId::PierceWall),
        AbilityState::Limited {
            uses: PIERCE_WALL_USES - 1
        },
        "and it cost a use, like any other bore",
    );

    // The player can stand in it — that is what makes it shelter rather than damage.
    s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), pocket);

    // But not dig on from inside it: three walls is no target at all (§8.4).
    assert_eq!(s.bore_target(), Err(BoreRefusal::TooManyWalls));
    assert_eq!(
        s.layout().facility().terrain(Cell::new(7, 5)),
        Some(Terrain::Wall),
        "a hole to hide in, never a tunnel",
    );
}

/// **Walls only** (§10.3): a door, a table, a cupboard and a duct mouth are not
/// walls, so none of them counts toward the one — bumping is what doors are for. The
/// count is of walls and nothing else, which is what keeps the rule sayable.
#[test]
fn only_walls_count_toward_the_one() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(6, 5), Terrain::DoorPanelClosed);
    layout.place(Cell::new(4, 5), Terrain::Hideout);
    layout.place(Cell::new(5, 6), Terrain::PartialCover);
    layout.place(Cell::new(5, 4), Terrain::DuctEntry);
    let s = borer(layout.clone(), Cell::new(5, 5));
    assert_eq!(
        s.bore_target(),
        Err(BoreRefusal::NothingToBore),
        "surrounded by things that are not walls is surrounded by nothing to bore",
    );

    // Swap one of them for a real wall and the ability is usable again — the
    // furniture beside it never made the target ambiguous.
    let mut with_wall = layout;
    with_wall.place(Cell::new(5, 4), Terrain::Wall);
    let s = borer(with_wall, Cell::new(5, 5));
    assert_eq!(s.bore_target(), Ok(Cell::new(5, 4)));
}

/// **The budget is the scarcity** (§8.2/#302/#303): three holes a facility, and the
/// bar says so at every step — a count while there are uses, unusable once there are
/// not. No cooldown ever gates it, and no amount of waiting brings a use back.
#[test]
fn the_uses_deplete_to_unusable_and_never_recharge() {
    // Four wall faces to stand against, so geometry never limits the test.
    let mut layout = open_room(20, 20);
    for y in [3, 6, 9, 12] {
        layout.place(Cell::new(6, y), Terrain::Wall);
    }
    let mut s = borer(layout, Cell::new(5, 3));

    for (i, y) in [3, 6, 9].into_iter().enumerate() {
        assert_eq!(
            s.ability_state(AbilityId::PierceWall),
            AbilityState::Limited {
                uses: PIERCE_WALL_USES - i as u32
            },
        );
        let events = s.step(Input::Activate(AbilityId::PierceWall));
        assert!(events.contains(&Event::WallBored {
            at: Cell::new(6, y)
        }));
        // Walk to the next wall face. No cooldown stands in the way, only the walk.
        for _ in 0..3 {
            s.step(Input::Step(Direction::South));
        }
    }

    assert_eq!(
        s.ability_state(AbilityId::PierceWall),
        AbilityState::Exhausted,
        "spent reads unusable — never Ready, never a cooldown",
    );
    assert_eq!(
        s.player(),
        Cell::new(5, 12),
        "standing against the fourth wall"
    );
    assert_eq!(s.bore_target(), Err(BoreRefusal::NoUsesLeft));

    let turn = s.turn();
    let events = s.step(Input::Activate(AbilityId::PierceWall));
    assert_eq!(
        events,
        vec![Event::BoreRefused {
            reason: BoreRefusal::NoUsesLeft
        }],
    );
    assert_eq!(s.turn(), turn, "the refusal is free");
    assert_eq!(
        s.layout().facility().terrain(Cell::new(6, 12)),
        Some(Terrain::Wall),
        "the fourth wall stands",
    );

    // A whole level of waiting brings nothing back (§8.2's fence).
    for _ in 0..60 {
        s.step(Input::Wait);
    }
    assert_eq!(
        s.ability_state(AbilityId::PierceWall),
        AbilityState::Exhausted
    );
}

/// **A run that does not hold Pierce Wall is not told about it** — the key is the
/// free no-op it is for any ungranted ability (§4.4/#244), and it stays silent
/// rather than explaining a tool the player does not have.
#[test]
fn a_run_without_the_ability_presses_the_key_in_silence() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(6, 5), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(1, 1),
    )
    .with_loadout(Loadout::innate());

    assert_eq!(
        s.ability_state(AbilityId::PierceWall),
        AbilityState::Unusable
    );
    assert_eq!(s.step(Input::Activate(AbilityId::PierceWall)), vec![]);
    assert_eq!(s.turn(), 0);
    assert_eq!(
        s.layout().facility().terrain(Cell::new(6, 5)),
        Some(Terrain::Wall),
        "the wall stands: the ability was never the player's",
    );
}

/// A room split in two by a wall course at `y = 5`, with the player below it at
/// (5,6) and `guard` above — the fixture the two §2.3 facade checks share. A guard
/// spawns facing south (§7.1), so it is looking straight at the partition, and the
/// only thing between it and the player is the one wall the ability can open.
fn split_room(guard: Guard) -> State {
    let mut layout = open_room(12, 12);
    for x in 1..11 {
        layout.place(Cell::new(x, 5), Terrain::Wall);
    }
    State::new(
        layout,
        Cell::new(5, 6),
        Direction::North,
        vec![guard],
        Vec::new(),
        Cell::new(1, 1),
    )
    .with_loadout(Loadout::innate().with(AbilityId::PierceWall))
}

/// **§2.3, from the guard's side — the cone.** A hole only the player could *use*
/// would be this ticket's whole facade, and the first half of proving it is not one
/// is that the guards can *see* through it: the wall stops the cone, the bore opens
/// it, and the player is spotted through a wall that is no longer there. The
/// sightline you cut is a sightline they get.
#[test]
fn a_guards_cone_reaches_through_a_fresh_hole() {
    let mut s = split_room(Guard::stationary(Cell::new(5, 4)));

    // The partition works: two cells apart, the guard looking straight this way,
    // and blind to the player behind the wall.
    let events = s.step(Input::Wait);
    assert!(
        !events.iter().any(|e| matches!(e, Event::Detected { .. })),
        "the wall is between them: {events:?}",
    );

    let events = s.step(Input::Activate(AbilityId::PierceWall));
    assert!(events.contains(&Event::WallBored {
        at: Cell::new(5, 5)
    }));
    assert!(
        events.iter().any(|e| matches!(e, Event::Detected { .. })),
        "the guard sees straight down the opening: {events:?}",
    );
}

/// **§2.3, from the guard's side — the route.** The other half: a guard *walks*
/// through the hole. Guard routing reads the terrain grid, so the opening is a route
/// for them the moment it exists, with nothing anyone has to remember to update.
///
/// The fixture makes the claim unambiguous: a wall course seals the room in two, and
/// a guard's patrol beat has a station on **each** side of it. Before a bore it can
/// never reach the far one — the control run below proves it never does — and after
/// one, the hole is the only way it could have got there.
#[test]
fn a_guard_walks_through_a_fresh_hole_and_could_not_before() {
    // The player stands under the course at (2,6) with exactly one wall above them;
    // the guard patrols the north half with a beat that wants the south half too.
    let fixture = || {
        let mut layout = open_room(12, 12);
        for x in 1..11 {
            layout.place(Cell::new(x, 5), Terrain::Wall);
        }
        State::new(
            layout,
            Cell::new(2, 6),
            Direction::North,
            vec![Guard::patrolling(Cell::new(8, 3))
                .with_beat(vec![Cell::new(8, 3), Cell::new(8, 8)])],
            Vec::new(),
            Cell::new(1, 1),
        )
        .with_loadout(Loadout::innate().with(AbilityId::PierceWall))
    };
    let crossed = |s: &State| s.guards().iter().any(|g| g.pos().y >= 5);

    // The control: no bore, and the beat's far station is simply unreachable.
    let mut sealed = fixture();
    for _ in 0..40 {
        if sealed.outcome() != Outcome::Playing {
            break;
        }
        sealed.step(Input::Wait);
        assert!(!crossed(&sealed), "the sealed course is not crossable");
    }

    // The same level, with one wall bored out of it.
    let mut opened = fixture();
    let events = opened.step(Input::Activate(AbilityId::PierceWall));
    assert!(events.contains(&Event::WallBored {
        at: Cell::new(2, 5)
    }));

    let mut went_through = false;
    for _ in 0..40 {
        // A capture is the guard arriving *at the player*, which is south of the
        // course — it came through the hole to do it.
        if opened.outcome() != Outcome::Playing {
            went_through = true;
            break;
        }
        opened.step(Input::Wait);
        went_through |= crossed(&opened);
        if went_through {
            break;
        }
    }
    assert!(
        went_through,
        "the route you cut is a route they get — no guard crossed in 40 turns",
    );
}

/// **The one spatial model** (§10.5): a bored cell is walkable, so it must belong to
/// exactly one region — the same claim a cupboard recessed into a wall makes
/// (§10.1.6). It joins the region of the space it opens onto, which is the player's.
#[test]
fn the_bored_cell_joins_the_region_it_opens_onto() {
    // Two rooms, a solid wall column between them, and no door at all — the bore is
    // the only opening this level will ever have.
    let mut facility = Facility::walled_box(9, 9);
    let mut regions = RegionGraph::new(9, 9);
    let column =
        |x0: u32, x1: u32| (1..8).flat_map(move |y| (x0..x1).map(move |x| Cell::new(x, y)));
    let west = regions.add_region(RegionKind::Room, column(1, 4));
    regions.add_region(RegionKind::Room, column(5, 8));
    for y in 1..8 {
        facility.set_terrain(4, y, Terrain::Wall);
    }
    let layout = Layout::from_parts(facility, regions);

    let player = Cell::new(3, 4);
    let home = layout
        .regions()
        .region_at(player)
        .expect("the player stands in a region");
    assert_eq!(home, west);
    let mut s = borer(layout, player);

    let wall = Cell::new(4, 4);
    assert_eq!(s.bore_target(), Ok(wall));
    assert_eq!(
        s.layout().regions().region_at(wall),
        None,
        "a wall belongs to no region",
    );

    s.step(Input::Activate(AbilityId::PierceWall));
    assert_eq!(
        s.layout().regions().region_at(wall),
        Some(home),
        "the hole joins the space it was opened from — no region-less floor",
    );
}

/// Every cell a player can walk to from `start` — the §10.6 solvability flood, over
/// the terrain the placement check itself floods (floor, doorways open or shut, and
/// cupboards).
fn reachable(facility: &Facility, start: Cell) -> HashSet<Cell> {
    let enterable = |c: Cell| {
        facility.terrain(c).is_some_and(|t| {
            matches!(
                t,
                Terrain::Floor
                    | Terrain::DoorPanelOpen
                    | Terrain::DoorPanelClosed
                    | Terrain::Hideout
            )
        })
    };
    crate::path::flood_from(start, facility.width(), facility.height(), enterable)
        .into_iter()
        .collect()
}

/// **§10.6 survives a bore, on a real generated level.** Boring only ever *turns a
/// wall into floor*, so it can only ever **add** connectivity: this finds a cell on a
/// generated facility the ability is legal from, bores from it, and asserts the
/// reachable set is a superset of what it was — every objective and the exit still
/// bump-adjacent, plus the hole itself. A guarantee that held before a bore holds
/// after, which is why the ability needs no §10.6 re-check of its own.
#[test]
fn a_bore_only_ever_adds_reachability() {
    let (layout, placement) = generate_level(&LevelConfig::V1, &mut Rng::new(7))
        .expect("the v1 recipe carves and places");

    // Somewhere on this level the ability is legal. Find it the way the player would
    // — by standing places — rather than by constructing the geometry.
    let stand = borable_stand(&layout).expect("a 40×40 facility has a lone wall face somewhere");
    let mut s = borer(layout, stand);
    let before = reachable(s.layout().facility(), stand);

    let wall = s.bore_target().expect("the scan found a legal stand");
    s.step(Input::Activate(AbilityId::PierceWall));
    let after = reachable(s.layout().facility(), stand);

    assert!(
        before.is_subset(&after),
        "a bore never takes a cell away — it only opens one",
    );
    assert!(after.contains(&wall), "and the hole itself is walkable");
    for target in placement
        .intel()
        .iter()
        .copied()
        .chain([placement.comms(), placement.exit()])
    {
        assert!(
            s.layout()
                .facility()
                .neighbours(target)
                .any(|n| after.contains(&n)),
            "{target:?} is still bump-adjacent to the reachable set (§10.6)",
        );
    }
}

/// A cell on `layout` the player could legally bore from — floor, with exactly one
/// adjacent interior wall whose far side is not itself wall. Scanned in row-major
/// order so the answer is the same every run (§12.4).
fn borable_stand(layout: &Layout) -> Option<Cell> {
    let facility = layout.facility();
    for y in 1..facility.height() - 1 {
        for x in 1..facility.width() - 1 {
            let cell = Cell::new(x, y);
            if facility.terrain(cell) != Some(Terrain::Floor) {
                continue;
            }
            let probe = State::new(
                layout.clone(),
                cell,
                Direction::North,
                Vec::new(),
                Vec::new(),
                Cell::new(1, 1),
            )
            .with_loadout(Loadout::innate().with(AbilityId::PierceWall));
            if probe.bore_target().is_ok() {
                return Some(cell);
            }
        }
    }
    None
}

/// A determinism check on the ability itself (§12.4): the same loadout and the same
/// inputs bore the same walls, so a run carrying Pierce Wall replays exactly — the
/// holes are part of the reproducible grid, not a side effect of when it was played.
#[test]
fn the_same_inputs_bore_the_same_walls() {
    let run = || {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(6, 5), Terrain::Wall);
        layout.place(Cell::new(8, 5), Terrain::Wall);
        let mut s = borer(layout, Cell::new(5, 5));
        let mut events = Vec::new();
        for input in [
            Input::Activate(AbilityId::PierceWall),
            Input::Step(Direction::East),
            Input::Step(Direction::East),
            Input::Activate(AbilityId::PierceWall),
        ] {
            events.extend(s.step(input));
        }
        let grid: Vec<Option<Terrain>> = (0..12)
            .flat_map(|y| (0..12).map(move |x| (x, y)))
            .map(|(x, y)| s.layout().facility().terrain_at(x, y))
            .collect();
        (events, grid)
    };
    let first = run();
    let second = run();
    assert_eq!(first.0, second.0, "the same events, in the same order");
    assert_eq!(first.1, second.1, "and the same holes in the same grid");
    assert!(
        first.0.iter().any(|e| matches!(e, Event::WallBored { .. })),
        "the replay actually bored something",
    );
}
