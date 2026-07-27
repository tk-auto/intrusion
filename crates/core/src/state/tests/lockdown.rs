//! Lockdown through the turn loop (§8.3/§10.4, #242).
//!
//! Three things have to be true at once for this ability to be the one the design
//! asked for rather than the one §2.2 forbids, and each has its own section here:
//!
//! - **It raises a wall.** A sealed door refuses a guard's handle, and the guard's
//!   *route* goes the long way round rather than standing at a door it cannot work.
//! - **The wall is only ever temporary.** The seal is released when the window ends,
//!   however it ends — expiry or the free toggle-off — so it can never permanently
//!   sever pathing (§2.2/§7.2's soft-lock class, #170/#182).
//! - **It never traps its owner.** The player bumps their own lock open exactly as
//!   they bump any door, so whatever the geometry, a lockdown cannot box them in.

use crate::state::*;
use crate::test_support::{open_room, region_strip};
use crate::{DoorId, DoorLock};

/// A player holding Lockdown in the [`region_strip`] fixture: a 16×6 strip of
/// room–corridor–room–corridor with a **closed manual door** in wall columns 4, 7 and
/// 11 (panel at y = 2), so the only way along the strip is through them. `guards` are
/// posted as given.
fn locksmith(player: Cell, guards: Vec<Guard>) -> State {
    State::new(
        region_strip(),
        player,
        Direction::East,
        guards,
        Vec::new(),
        Cell::new(14, 4),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Lockdown))
}

/// The door whose panel is in wall column `x` of the fixture.
fn door_in_column(s: &State, x: u32) -> DoorId {
    s.layout()
        .regions()
        .door_at(Cell::new(x, 2))
        .expect("the strip has a door in this column")
}

/// Whether the door in column `x` is currently locked.
fn locked(s: &State, x: u32) -> bool {
    let id = door_in_column(s, x);
    s.layout().regions().door(id).is_locked()
}

/// Lockdown's window, read off its own catalog row rather than restated here.
fn lockdown_duration() -> u32 {
    AbilityId::Lockdown
        .def()
        .economy()
        .expect("Lockdown is an activated ability")
        .duration()
}

// ---------------------------------------------------------------------------
// The reach
// ---------------------------------------------------------------------------

/// **The ability, working**: activating seals every door in reach and *shuts* it on
/// the way (§8.3/#242) — a lock on a door already standing open raises no wall, so the
/// shut is part of the seal. The turn is spent (§4.4) and the act is reported once,
/// with the count, rather than door by door (§11.7).
#[test]
fn activating_shuts_and_seals_every_door_in_reach() {
    // Corridor C, one cell from the door west and two from the door east.
    let fired_at = Cell::new(5, 2);
    let mut s = locksmith(fired_at, Vec::new());
    // Open the near door first, so the shut half of the seal has something to do.
    s.step(Input::Step(Direction::West));
    assert_eq!(s.player(), fired_at, "a door bump is not a step");
    assert!(
        s.layout().regions().door(door_in_column(&s, 4)).is_open(),
        "the player bumped the near door open"
    );

    let turn = s.turn();
    let events = s.step(Input::Activate(AbilityId::Lockdown));
    assert_eq!(s.turn(), turn + 1, "activation spends the turn (§4.4)");
    assert!(
        events.contains(&Event::DoorsSealed {
            count: 2,
            at: fired_at,
        }),
        "one act, reported once, with what it actually took: {events:?}",
    );

    // Columns 4 and 7 are within LOCKDOWN_RADIUS of (5,2); column 11 is not.
    assert!(locked(&s, 4) && locked(&s, 7), "both near doors are sealed");
    assert!(!locked(&s, 11), "the far door is out of reach");
    assert!(
        !s.layout().regions().door(door_in_column(&s, 4)).is_open(),
        "the seal shut the door it found open — the wall is the point",
    );
}

/// The reach is exactly the [`LOCKDOWN_RADIUS`] box (§6.1's metric), asserted against
/// the rule in both directions rather than against a hand-listed set: every door the
/// ability takes is one within the box, and every door within the box is taken. This
/// is the check that stops the radius constant and the behaviour drifting apart.
#[test]
fn the_reach_is_exactly_the_radius_box() {
    for x in [2u32, 6, 9, 13] {
        let s = locksmith(Cell::new(x, 2), Vec::new());
        let taken = s.lockdown_doors();
        for (id, door) in s.layout().regions().doors() {
            let in_box = door
                .cells()
                .any(|c| s.player().sight_distance(c) <= LOCKDOWN_RADIUS);
            assert_eq!(
                taken.contains(&id),
                in_box,
                "player at ({x},2): the set and the box must agree about {id:?}",
            );
        }
    }
}

/// A lockdown with **no door in reach** is refused: free, nothing changed, no turn
/// spent and no cooldown burned (§4.4) — the same shape as the decoy's missing cell
/// and the refused bore. It speaks, because a press that did nothing must say why.
#[test]
fn a_lockdown_with_no_door_in_reach_is_free_and_says_so() {
    // An open room: no doorway anywhere, so the ability has nothing to act on. (Every
    // cell of the strip fixture is within reach of one of its three doors.)
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Lockdown));
    assert!(s.lockdown_doors().is_empty(), "no door within the box");

    let turn = s.turn();
    let events = s.step(Input::Activate(AbilityId::Lockdown));
    assert!(events.contains(&Event::LockdownRefused), "{events:?}");
    assert_eq!(s.turn(), turn, "a refusal is free (§4.4)");
    assert_eq!(
        s.ability_state(AbilityId::Lockdown),
        AbilityState::Ready,
        "and costs no cooldown: the ability never switched on",
    );
    assert!(
        s.layout().regions().doors().all(|(_, d)| !d.is_locked()),
        "and nothing was sealed",
    );
}

/// A door with someone standing in its throat is **locked but not crushed** (§10.4):
/// the crush rule holds against an ability exactly as it holds against a bump, so the
/// door stays open and merely refuses the guards' handle.
#[test]
fn a_door_held_open_by_an_occupant_is_locked_but_never_crushed() {
    let panel = Cell::new(4, 2);
    let mut s = locksmith(Cell::new(3, 2), Vec::new());
    // Bump the door open, then stand in its throat.
    s.step(Input::Step(Direction::East));
    s.step(Input::Step(Direction::East));
    let door = door_in_column(&s, 4);
    assert_eq!(s.player(), panel, "the player holds the throat");
    assert!(s.layout().regions().door(door).is_open());

    s.step(Input::Activate(AbilityId::Lockdown));
    assert!(
        s.layout().regions().door(door).is_open(),
        "a lockdown never shuts a door on an occupant (§10.4)",
    );
    assert!(locked(&s, 4), "…and locks it all the same");
}

// ---------------------------------------------------------------------------
// The wall: what a guard does about it
// ---------------------------------------------------------------------------

/// A guard **cannot open a sealed door** (§10.4/#242): its walk-in bump, the one thing
/// that opens a door on its route, is simply declined. The door opens again on the
/// ordinary bump once the seal is released, which is what makes this a wait rather
/// than a deadlock.
#[test]
fn a_guard_cannot_work_a_sealed_handle() {
    // The player is in room A; the guard is in corridor C, headed for them. The only
    // way through is the door in column 4, and the player seals it.
    let mut s = locksmith(
        Cell::new(2, 2),
        vec![Guard::patrolling_to(Cell::new(6, 2), Cell::new(2, 2))],
    );
    s.set_guard_close_chance(0);
    s.step(Input::Activate(AbilityId::Lockdown));
    assert!(locked(&s, 4));
    let door = door_in_column(&s, 4);

    // However long the guard is left to walk at it, the sealed door does not open —
    // and it never reaches the player behind it.
    for _ in 0..lockdown_duration() - 1 {
        s.step(Input::Wait);
        assert!(
            !s.layout().regions().door(door).is_open(),
            "the seal refuses the guard's handle",
        );
        assert_eq!(s.outcome(), Outcome::Playing, "the wall is holding");
    }
    assert!(!locked(&s, 4), "the window has closed on its own (§8.2)");

    // A wait, never a deadlock: with the seal gone the guard works the door as always.
    for _ in 0..4 {
        s.step(Input::Wait);
    }
    assert!(
        s.layout().regions().door(door).is_open(),
        "everything the guard was going to do, it does when the window ends",
    );
}

/// A guard's **route** treats a sealed doorway as solid (§7.6/#242) — the detour is the
/// whole thing the ability buys, and without it the guard would stand bumping a handle
/// that refuses it. Asserted through the router's own blocked set, which is where the
/// rule lives.
#[test]
fn a_sealed_doorway_is_solid_to_a_guard_s_route() {
    let mut s = locksmith(Cell::new(2, 2), Vec::new());
    assert!(
        s.sealed_route_blocks().is_empty(),
        "nothing is sealed to begin with",
    );

    s.step(Input::Activate(AbilityId::Lockdown));
    assert_eq!(
        s.sealed_route_blocks(),
        vec![Cell::new(4, 2)],
        "the closed panel of the one sealed door",
    );

    // An open sealed door blocks nothing: a doorway you can walk through is passable
    // whoever locked it.
    s.step(Input::Step(Direction::East)); // a step up to the door…
    s.step(Input::Step(Direction::East)); // …then the player bumps their own lock open
    assert!(
        s.sealed_route_blocks().is_empty(),
        "an open doorway is not a wall, sealed or not",
    );
}

// ---------------------------------------------------------------------------
// It is your lock
// ---------------------------------------------------------------------------

/// **The player passes their own lock** (§8.3/#242, the ticket's open question): a
/// sealed door bumps open exactly as any closed door does, so a lockdown can never box
/// its owner in. It is not free — it costs the turn, and it leaves the door *open* for
/// whoever is behind you, which is the whole decision the ability hands the player.
#[test]
fn the_player_bumps_their_own_lock_open() {
    let mut s = locksmith(Cell::new(2, 2), Vec::new());
    s.step(Input::Activate(AbilityId::Lockdown));
    let door = door_in_column(&s, 4);
    assert!(locked(&s, 4) && !s.layout().regions().door(door).is_open());
    s.step(Input::Step(Direction::East)); // up to the door

    let turn = s.turn();
    s.step(Input::Step(Direction::East));
    assert_eq!(s.turn(), turn + 1, "opening it costs the turn (§4.4)");
    assert!(
        s.layout().regions().door(door).is_open(),
        "your own lock does not refuse you",
    );
    assert!(
        locked(&s, 4),
        "the door is still sealed — the seal is a handle rule, not a hold-shut",
    );

    // And the route it just gave back is a route the guards get: an open doorway is
    // no longer blocked to them.
    assert!(s.sealed_route_blocks().is_empty());
}

// ---------------------------------------------------------------------------
// The wall is temporary — the §2.2/§7.2 guarantee
// ---------------------------------------------------------------------------

/// **The seal always expires** (§2.2/§7.2's soft-lock class, #170/#182): the window is
/// the ability's own duration and nothing else, so after it there is not a locked door
/// anywhere in the level — checked every turn, so a seal cannot outlive the window by
/// even one.
#[test]
fn the_seal_can_never_outlive_its_window() {
    let mut s = locksmith(Cell::new(5, 2), Vec::new());
    let events = s.step(Input::Activate(AbilityId::Lockdown));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::DoorsSealed { .. })));

    // The activation turn is itself inside the window (§8.2's N-yields-N timing), so
    // it counts as the first; walk the rest until the ability says it is over.
    let mut window = 1;
    loop {
        assert!(
            s.layout().regions().doors().any(|(_, d)| d.is_locked()),
            "turn {window} of the window: still sealed",
        );
        let events = s.step(Input::Wait);
        window += 1;
        if events.contains(&Event::AbilityExpired {
            ability: AbilityId::Lockdown,
        }) {
            break;
        }
        assert!(
            window < 100,
            "a seal that never expires is the §2.2 soft-lock"
        );
    }
    assert_eq!(
        window,
        lockdown_duration(),
        "the window is the duration, and nothing else",
    );
    assert!(
        s.layout()
            .regions()
            .doors()
            .all(|(_, d)| d.lock() == DoorLock::Unlocked),
        "every seal is released with the window that placed it",
    );

    // …and it stays released, however long the level runs on.
    for _ in 0..20 {
        s.step(Input::Wait);
        assert!(s.layout().regions().doors().all(|(_, d)| !d.is_locked()));
    }
}

/// The free toggle-off (§4.4) hands every door back at once, and refunds nothing —
/// the full lockout still runs (§8.2: cancelling saves you nothing).
#[test]
fn an_early_toggle_off_releases_every_seal() {
    let mut s = locksmith(Cell::new(6, 2), Vec::new());
    s.step(Input::Activate(AbilityId::Lockdown));
    assert!(locked(&s, 4) && locked(&s, 7));

    let turn = s.turn();
    s.step(Input::Deactivate(AbilityId::Lockdown));
    assert_eq!(s.turn(), turn, "a toggle-off is free (§4.4)");
    assert!(
        s.layout().regions().doors().all(|(_, d)| !d.is_locked()),
        "every seal goes with the window",
    );
    assert!(
        matches!(
            s.ability_state(AbilityId::Lockdown),
            AbilityState::Cooling { .. }
        ),
        "…and the full cooldown still runs (§8.2)",
    );
}

// ---------------------------------------------------------------------------
// What the player is shown
// ---------------------------------------------------------------------------

/// The mark is a **standing** one on the doors, not a wash that follows you
/// (§8.3/§11.5/#242/#338): it stays on exactly the doorways the seal took, wherever the
/// player walks. A wall you raised behind you must not appear to move with you.
///
/// Lockdown spends its one `Cells` mark on the doors rather than on the
/// [`LOCKDOWN_RADIUS`] box deliberately: the box would say *this far* for a frame, and
/// the doors say *these ones* for as long as the guards cannot work them.
#[test]
fn the_mark_stays_on_the_doors_the_seal_took() {
    let mut s = locksmith(Cell::new(5, 2), Vec::new());
    s.step(Input::Activate(AbilityId::Lockdown));
    let marked: Vec<Cell> = s.effect_cell_marks().collect();
    assert!(!marked.is_empty(), "the seal is marked");

    // Every marked cell is a cell of a sealed door — never a cell of the box it was
    // measured from.
    for cell in &marked {
        let door = s
            .layout()
            .regions()
            .door_at(*cell)
            .expect("a mark lands on a doorway");
        assert!(
            s.layout().regions().door(door).is_locked(),
            "{cell:?} is marked, so its door is sealed",
        );
    }

    s.step(Input::Step(Direction::East)); // into open corridor, not a doorway
    assert_eq!(
        s.effect_cell_marks().collect::<Vec<_>>(),
        marked,
        "walking away moves nothing: the doors are where they were sealed",
    );
}

/// The mark carries the state for the **whole window** — a [`MarkLife::Standing`] mark,
/// so it outlives every momentary flash and ends with the ability's own window, not one
/// turn before or after (#338).
#[test]
fn the_sealed_doors_are_marked_for_the_whole_window() {
    let mut s = locksmith(Cell::new(2, 2), Vec::new());
    assert_eq!(s.sealed_door_cells().count(), 0, "nothing marked yet");

    s.step(Input::Activate(AbilityId::Lockdown));
    let door = door_in_column(&s, 4);
    let expected: Vec<Cell> = s.layout().regions().door(door).cells().collect();
    assert_eq!(
        s.effect_cell_marks().collect::<Vec<_>>(),
        expected,
        "the whole footprint of the sealed door, and only it",
    );

    // Still marked long after a *momentary* mark would have burned out — that is what
    // makes it a state rather than a moment.
    for _ in 0..EFFECT_FLASH_TURNS + 1 {
        s.step(Input::Wait);
    }
    assert_eq!(
        s.effect_cell_marks().count(),
        expected.len(),
        "a standing mark does not count down",
    );

    for _ in 0..lockdown_duration() {
        s.step(Input::Wait);
    }
    assert_eq!(
        s.effect_cell_marks().count(),
        0,
        "and it goes with the window that placed it",
    );
    assert_eq!(s.sealed_door_cells().count(), 0, "…as does the seal itself");
}

/// The §8.3 **[START]** numbers, pinned so a retune is a visible decision — the radius
/// especially, which is this ability's main power lever.
#[test]
fn the_lockdown_numbers_are_pinned() {
    assert_eq!(LOCKDOWN_RADIUS, 4);
    assert_eq!(lockdown_duration(), 8);
    let economy = AbilityId::Lockdown.def().economy().expect("activated");
    assert_eq!(economy.cooldown(), 40);
    // The seal takes a room, not a wing — so it stays inside the bubble Confusion
    // reaches, which is itself pinned inside the guard sense.
    const { assert!(LOCKDOWN_RADIUS < CONFUSION_RADIUS) };
}
