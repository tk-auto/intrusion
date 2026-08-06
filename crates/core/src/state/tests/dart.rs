//! **Dart** through the turn loop (§7.2/§8.3/§8.4, #239) — the experiment.
//!
//! This ability deliberately reopens the §2.3 failure, so what is pinned here is every
//! safeguard that keeps it from *being* that failure. Read as a list, the tests are the
//! ticket's own acceptance criteria:
//!
//! - **The ray is the aim.** It flies along the facing, stops at the first solid or the
//!   first guard, and never exceeds [`DART_RANGE`]. Nothing anywhere picks a target — the
//!   same board with the player turned round hits nothing at all.
//! - **§7.2's gate is unmoved.** Unaware, or no takedown. The §6 sight half is pinned as
//!   the *structural* property that backs it: the dart can never reach a cell the player
//!   cannot see, so the gate has nothing to fail on and the flight's wash leaks nothing.
//! - **The body is the cost.** It drops where the guard stood, on the guard's own radio
//!   clock, and the takedown counts like any other.
//! - **A miss costs everything a hit does**, and reads the same whatever it found.
//!
//! The fixtures shoot **south** at guards standing south of the player. Guards spawn
//! facing south (§7.1), so a guard south of the shooter is looking *away* — unaware
//! (§7.2) — while sitting squarely in the player's own southward cone (§5). Turning the
//! guard round is how the aware case is built.

use crate::state::*;
use crate::test_support::{open_beat, open_room};

/// Where the shooter stands in every fixture below.
const SHOOTER: Cell = Cell { x: 10, y: 10 };

/// A player holding the Dart in a big open room, facing `dir`, with `guards` posted as
/// given and `blocks` stamped into the terrain first.
///
/// The room is big enough that the whole [`DART_RANGE`] fits down the line, so what a
/// firing does or does not reach is the ability's doing and never the wall's.
fn shooter_facing(dir: Direction, guards: Vec<Guard>, blocks: &[(Cell, Terrain)]) -> State {
    let mut layout = open_room(40, 40);
    for &(cell, terrain) in blocks {
        layout.place(cell, terrain);
    }
    State::new(layout, SHOOTER, dir, guards, Vec::new(), Cell::new(38, 38))
        .with_loadout(Loadout::innate().with(AbilityId::Dart))
}

/// The default fixture: facing south, down the line the guards below stand on.
fn shooter(guards: Vec<Guard>) -> State {
    shooter_facing(Direction::South, guards, &[])
}

/// A guard `south` cells south of the shooter — facing south at spawn, so looking away
/// from the player (unaware, §7.2) and inside their southward cone (seen, §6).
fn target(south: u32) -> Guard {
    Guard::stationary(Cell::new(SHOOTER.x, SHOOTER.y + south)).with_beat(open_beat(40, 40))
}

/// Fire the dart, spending the turn (§4.4), and report the flight straight off the event.
fn fire(state: &mut State) -> (u32, bool) {
    let events = state.step(Input::Activate(AbilityId::Dart));
    events
        .iter()
        .find_map(|e| match e {
            Event::DartFired { travelled, hit, .. } => Some((*travelled, *hit)),
            _ => None,
        })
        .expect("the dart went out")
}

// ---------------------------------------------------------------------------
// The ray is the aim (§8.4/appendix 1)
// ---------------------------------------------------------------------------

/// The dart flies **along the facing and nowhere else**. A guard three cells south is
/// taken down when the player faces south, and completely untouched when the player faces
/// any other way — though it is the nearest visible guard on the board in every case.
///
/// **This is the §2.3 test.** *Auto-target-nearest-visible* would have hit it from all
/// four facings, and not doing that is the whole reason a ranged takedown is allowed to
/// exist at all (appendix 1).
#[test]
fn the_dart_flies_along_the_facing_and_never_at_the_nearest_guard() {
    let mut s = shooter(vec![target(3)]);
    let (travelled, hit) = fire(&mut s);
    assert_eq!(travelled, 3, "it stopped on the guard's cell");
    assert!(hit);
    assert!(s.guards().is_empty(), "the guard is permanently out (§7.2)");

    for dir in [Direction::North, Direction::East, Direction::West] {
        let mut s = shooter_facing(dir, vec![target(3)], &[]);
        let (_, hit) = fire(&mut s);
        assert!(
            !hit,
            "facing {dir:?}: the dart does not turn toward a guard"
        );
        assert_eq!(s.guards().len(), 1, "facing {dir:?}: still standing");
    }
}

/// The flight **never exceeds [`DART_RANGE`]**: a guard exactly at it is in reach, and one
/// cell further is not. The pair is the assertion — checking only the far side would pass
/// an off-by-one that quietly shortened the ability.
#[test]
fn the_range_is_a_hard_bound_at_dart_range() {
    let mut s = shooter(vec![target(DART_RANGE)]);
    let (travelled, hit) = fire(&mut s);
    assert_eq!(travelled, DART_RANGE);
    assert!(hit, "the last cell in range is in range");

    let mut s = shooter(vec![target(DART_RANGE + 1)]);
    let (travelled, hit) = fire(&mut s);
    assert_eq!(travelled, DART_RANGE, "it flew its full reach and stopped");
    assert!(!hit, "one cell past the reach is out of it");
    assert_eq!(s.guards().len(), 1);
}

/// The dart stops at **the first solid**, and the stopper is the terrain table's own notion
/// of solid (`blocks_movement`) rather than a second list kept in step here — so a guard
/// behind a wall, a closed door panel, any **solid usable**, a duct entry or a **table** is
/// safe.
///
/// The table is the interesting one and it is deliberate: sight goes straight over partial
/// cover (§10.3), so this is the one thing a player can see past and not shoot past. It is
/// also load-bearing — §10.1a stamps cover into every over-long straight run, which is
/// what bounds the end-of-corridor shot before any number does.
#[test]
fn a_solid_stops_the_dart_wall_door_usable_and_table_alike() {
    for solid in Terrain::ALL
        .into_iter()
        .filter(|t| t.blocks_movement() && *t != Terrain::Hideout)
    {
        // Two cells short of the guard, so the block is unambiguously what stopped it.
        let between = Cell::new(SHOOTER.x, SHOOTER.y + 2);
        let mut s = shooter_facing(Direction::South, vec![target(4)], &[(between, solid)]);
        let (travelled, hit) = fire(&mut s);
        assert_eq!(
            travelled, 1,
            "{solid:?}: the dart stops *against* the solid"
        );
        assert!(!hit, "{solid:?}: nothing behind a solid is a target");
        assert_eq!(s.guards().len(), 1, "{solid:?}");
    }
}

/// **On open floor the dart cannot outreach the player's eyes**, and this is the
/// containment that makes it true: every terrain that blocks **sight** also blocks
/// **movement**, so along a cardinal nothing breaks the sightline without also stopping
/// the dart.
///
/// It is a fact about the §10.3 table rather than about this ability, which is exactly why
/// it is pinned here — [`State::dart_shot`](crate::State::dart_shot) leans on it to say the
/// §11.5 flight wash leaks nothing on open floor, and therefore that the clamp is a
/// crawlspace rule rather than a standing nerf. The day an opaque-but-walkable terrain joins
/// the table (a curtain, a smoke cell), this fails and says where to look.
#[test]
fn the_dart_cannot_outreach_the_player_on_open_floor() {
    for terrain in Terrain::ALL {
        assert!(
            !terrain.blocks_sight() || terrain.blocks_movement(),
            "{terrain:?} breaks a sightline without stopping a dart — see state::dart",
        );
    }
    // The other two halves of the argument — `DART_RANGE` shorter than the §5 sight range,
    // and inside the guard sense so the clamp is inert here — are `const _` assertions
    // beside the constant itself (`state::tuning`), which is the stronger place for a
    // relation between two constants: it fails the build rather than a test run.

    // And the property itself, walked: every cell a dart reaches on open floor is a cell
    // the player's own FOV holds.
    let s = shooter(vec![target(6)]);
    for cell in s.dart_shot().path() {
        assert!(
            s.player_fov().contains(*cell),
            "{cell:?} is in flight but not in sight",
        );
    }
}

/// **A crawler shoots short** (§10.7): a dart *can* be fired out of a duct — a mouth has a
/// floor neighbour — but the clamp cuts its reach to the crawlspace's own degraded sense, so
/// it can never stop on something the player has no picture of.
///
/// This is the case the clamp exists for, and the one the first version of this module talked
/// itself out of. A mid-duct cell has no live sight at all, so an unclamped eight-cell shot
/// would fly blind into a room and its §11.5 wash would report whatever it found there.
///
/// **The clamp, not the walls, is what bounds it**, and the difference showed up in a real
/// build. This fixture's duct is threaded through a wall band, so its interior happens to be
/// solid — but a duct's interior keeps whatever terrain it already had (§10.7), and a crawl
/// that crosses room floor to join two far regions has a walkable interior. The player's own
/// exit tunnel is exactly that: firing along it in the artifact flew three cells over its
/// floor and stopped at the `E`. So the bound asserted below is the *sense*, which holds
/// either way, and never "a duct is made of wall", which does not.
#[test]
fn a_dart_fired_from_a_duct_is_clamped_to_the_crawlspace_sense() {
    let mut s = super::ducts::duct_world().with_loadout(Loadout::innate().with(AbilityId::Dart));
    // Bump the mouth to climb in (§10.7) — the fixture starts on the floor below it,
    // facing north.
    s.step(Input::Step(Direction::North));
    assert!(s.in_duct(), "the fixture has to have the player inside");
    assert_eq!(s.sense_range(), DUCT_SENSE_RANGE);

    // Along *this* duct's own axis there is no line, because this fixture's interior is
    // the wall band it was threaded through — a property of the fixture, not of ducts.
    for dir in [Direction::East, Direction::West, Direction::North] {
        let mut probe = s.clone();
        probe.facing = dir;
        assert_eq!(
            probe.dart_shot().travelled(),
            0,
            "facing {dir:?}: the duct's own axis is solid",
        );
    }

    // Out of the mouth there is — and it is clamped, not eight cells long.
    let mut probe = s.clone();
    probe.facing = Direction::South;
    let travelled = probe.dart_shot().travelled();
    assert!(
        travelled <= DUCT_SENSE_RANGE,
        "a crawler's dart is clamped to the duct sense, got {travelled}",
    );
    assert!(travelled > 0, "the mouth does open onto the room");
}

/// What the dart **flies over**: a loose body (non-solid, §7.2) and the player's own
/// **decoy** (§8.3). Neither is terrain and neither stops anything, so the guard beyond is
/// still the first thing on the line.
///
/// The decoy half is the one worth a test rather than a comment: it is a fake intruder of
/// the player's own making, and a dart that stopped on one would spend the facility's only
/// shot on the player's own prop.
#[test]
fn the_dart_flies_over_a_body_and_over_your_own_decoy() {
    // A body two cells along, left by a takedown that happened somewhere else.
    let mut s = shooter(vec![target(4)]);
    let lying = Cell::new(SHOOTER.x, SHOOTER.y + 2);
    s.bodies.push(crate::body::Body::new(
        lying,
        crate::radio::RadioClock::default(),
        0,
    ));
    let (travelled, hit) = fire(&mut s);
    assert_eq!(travelled, 4, "a body is not a stopper (§7.2)");
    assert!(hit);

    // A decoy in the cell faced, which is the first cell of the dart's own line.
    let mut s = State::new(
        open_room(40, 40),
        SHOOTER,
        Direction::South,
        vec![target(4)],
        Vec::new(),
        Cell::new(38, 38),
    )
    .with_loadout(
        Loadout::innate()
            .with(AbilityId::Dart)
            .with(AbilityId::Decoy),
    );
    s.step(Input::Activate(AbilityId::Decoy));
    assert_eq!(
        s.decoy(),
        Some(Cell::new(SHOOTER.x, SHOOTER.y + 1)),
        "the fake is on the line",
    );
    let (travelled, hit) = fire(&mut s);
    assert_eq!(
        travelled, 4,
        "the dart does not stop on your own fake (§8.3)"
    );
    assert!(hit);
}

/// The dart stops at the **first guard on the line**, whether or not that guard is a legal
/// target — so an aware guard shields the unaware one behind it.
///
/// Stopping and hitting are separate questions on purpose. A flight that skipped illegal
/// targets would shoot *through* the guard in the way, which is both absurd and an aiming
/// aid: the player would not have to know what is standing in the corridor.
#[test]
fn the_first_guard_on_the_line_stops_the_dart_legal_or_not() {
    // The nearer guard faces north, back up the line, so it has the player and is no
    // target; the far one faces away and would be one.
    let mut near = target(2);
    near.face_for_test(Direction::North);
    let mut s = shooter(vec![near, target(5)]);
    s.step(Input::Wait); // one turn, so the turned guard's cone is cast where it looks
    let standing = s.guards().len();
    assert!(
        s.guard_detects_now(&s.guards()[0]),
        "the blocker has to actually have the player",
    );

    let (travelled, hit) = fire(&mut s);
    assert_eq!(travelled, 2, "it stopped on the nearer guard");
    assert!(!hit, "an aware guard is no target (§7.2)");
    assert_eq!(
        s.guards().len(),
        standing,
        "and the guard behind it is untouched",
    );
}

// ---------------------------------------------------------------------------
// §7.2's gate, unmoved
// ---------------------------------------------------------------------------

/// §7.2 **[SETTLED]**: the target must not have detected the player. A guard looking
/// straight back down the line is not taken down — and the dart is spent anyway, because a
/// press that fires is a press that costs (§8.4/#239).
#[test]
fn an_aware_guard_is_no_target_and_the_dart_is_spent_anyway() {
    let mut aware = target(4);
    aware.face_for_test(Direction::North);
    let mut s = shooter(vec![aware]);
    assert!(s.guard_detects_now(&s.guards()[0]));

    let (travelled, hit) = fire(&mut s);
    assert_eq!(travelled, 4, "the dart reached it");
    assert!(!hit, "§7.2's unaware requirement is unmoved");
    assert_eq!(s.guards().len(), 1, "still standing");
    assert_eq!(
        s.ability_state(AbilityId::Dart),
        AbilityState::Exhausted,
        "the level's dart is gone all the same (§8.2)",
    );
}

// ---------------------------------------------------------------------------
// The body is the cost (§7.2/§7.3)
// ---------------------------------------------------------------------------

/// The body drops **where the guard stood**, several cells away, carrying that guard's own
/// radio cadence — and the run's tally counts it like any other takedown (§7.2/§7.3).
///
/// This is the ability's honest counterweight: the body is out there, on a route the
/// player may not be able to walk back down, already counting.
#[test]
fn the_body_falls_where_the_guard_stood_and_the_radio_clock_runs() {
    let mut s = shooter(vec![target(5)]);
    let at = s.guards()[0].pos();

    let events = s.step(Input::Activate(AbilityId::Dart));
    assert!(events.contains(&Event::TakenDown { at }), "{events:?}");
    assert_eq!(s.bodies().len(), 1);
    assert_eq!(s.bodies()[0].cell(), at, "not at the player's feet");
    assert_eq!(s.bodies()[0].fell_at(), at, "and it fell there too");
    assert!(!s.bodies()[0].found(), "nobody has seen it yet");
    // **The §7.3 clock is running.** Left alone for long enough the body starts missing
    // its pings, exactly as an adjacent takedown's does — the honest cost of a takedown
    // you shot from too far away to walk back to and stow (§7.2).
    for _ in 0..crate::radio::PING_INTERVAL * 2 {
        s.step(Input::Wait);
    }
    assert!(
        s.bodies().first().is_none_or(|b| b.missed_pings() > 0),
        "the body inherited the guard's cadence and is being missed (§7.3)",
    );
    // Five cells away with the player standing still — this is not a body the run stows.
    assert_eq!(s.player().manhattan_distance(at), 5);
}

// ---------------------------------------------------------------------------
// A miss costs everything a hit does, and tells you nothing
// ---------------------------------------------------------------------------

/// A firing into an **empty line** spends the turn and the level's only dart, and says so
/// (§11.7). It is never refused for want of a target and the bar never greys for one —
/// either would answer *"is there a guard in front of me?"* for free (§8.4/#239).
#[test]
fn a_dart_into_nothing_spends_the_turn_and_the_use() {
    let mut s = shooter(Vec::new());
    let turn = s.turn();
    assert_eq!(
        s.ability_state(AbilityId::Dart),
        AbilityState::Limited { uses: 1 },
        "an empty line does not grey the entry",
    );

    let (travelled, hit) = fire(&mut s);
    assert!(!hit);
    assert_eq!(travelled, DART_RANGE, "it flew its whole reach");
    assert_eq!(s.turn(), turn + 1, "the turn is spent (§4.4)");
    assert_eq!(s.ability_state(AbilityId::Dart), AbilityState::Exhausted);

    // And there is no second dart, however long the run goes on (§8.2 — nothing recharges,
    // and there is no cooldown behind the budget for one to come back off).
    for _ in 0..80 {
        s.step(Input::Wait);
    }
    assert_eq!(s.ability_state(AbilityId::Dart), AbilityState::Exhausted);
    let turn = s.turn();
    assert!(s.step(Input::Activate(AbilityId::Dart)).is_empty());
    assert_eq!(s.turn(), turn, "a spent dart is a free no-op (§4.4)");
}

/// **Every kind of miss reads the same on the near line** (§11.7/§8.4/#239): an empty line
/// and an aware guard produce the identical message.
///
/// Two messages would be a detector with two settings — the reasoning §8.3 spells out for
/// False Call's silent firing, which bites harder here because what a distinction would
/// leak is worth a takedown.
#[test]
fn every_kind_of_miss_reads_the_same_on_the_near_line() {
    let line = |mut s: State| {
        s.step(Input::Activate(AbilityId::Dart));
        crate::status::near_line(&s).text
    };

    let empty = line(shooter(Vec::new()));

    let mut aware = target(4);
    aware.face_for_test(Direction::North);
    let aware = line(shooter(vec![aware]));

    // A guard past the reach: the third kind of nothing, from the far side.
    let far = line(shooter(vec![target(DART_RANGE + 2)]));

    assert_eq!(empty, aware, "an aware guard reads as an empty line");
    assert_eq!(empty, far, "a guard out of reach reads as an empty line");
    assert!(
        !empty.is_empty(),
        "a press that changed nothing still has to speak (§11.7)",
    );
    // And a hit says something else, or the line would be telling the player nothing at
    // all about the one outcome that matters.
    assert_ne!(empty, line(shooter(vec![target(3)])));
}

/// The flight is **painted**, once, over the cells it actually travelled (§11.5) — the one
/// thing neither the board nor the near line can say.
#[test]
fn the_flight_is_washed_over_the_cells_it_travelled() {
    let mut s = shooter(vec![target(3)]);
    s.step(Input::Activate(AbilityId::Dart));
    let washed: Vec<Cell> = s.effect_cell_marks().collect();
    assert_eq!(
        washed,
        vec![
            Cell::new(SHOOTER.x, SHOOTER.y + 1),
            Cell::new(SHOOTER.x, SHOOTER.y + 2),
            Cell::new(SHOOTER.x, SHOOTER.y + 3),
        ],
        "the path, and not the player's own cell",
    );
    // Momentary: one frame, then gone (§11.5).
    s.step(Input::Wait);
    assert!(
        s.effect_cell_marks().next().is_none(),
        "a flash, not a state",
    );
}

/// A run that does **not** hold the Dart presses the key in silence and spends nothing
/// (§4.4/#244) — the same free no-op as every ability the run was never given.
#[test]
fn a_run_without_the_dart_presses_nothing() {
    let mut s = State::new(
        open_room(40, 40),
        SHOOTER,
        Direction::South,
        vec![target(3)],
        Vec::new(),
        Cell::new(38, 38),
    );
    let turn = s.turn();
    let events = s.step(Input::Activate(AbilityId::Dart));
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(s.turn(), turn, "free (§4.4)");
    assert_eq!(s.guards().len(), 1);
}

/// **Determinism** (§12.4): the same board, the same facing and the same press resolve the
/// same shot every time — the flight, its length and its target.
#[test]
fn the_same_board_fires_the_same_dart() {
    let shot = || {
        let s = shooter(vec![target(4)]);
        let shot = s.dart_shot();
        (shot.direction(), shot.path().to_vec(), shot.hit())
    };
    assert_eq!(shot(), shot());
}
