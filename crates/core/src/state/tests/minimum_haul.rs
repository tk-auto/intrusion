//! **The minimum haul** (§4.5/§14 v3/#574): a facility cannot be left empty-handed.
//!
//! [`IntelGate::AtLeastOne`] asks for one **objective** — an intel console `$` or an
//! equipment cache `¤` — and asks nothing else. What is pinned here is the whole of that
//! rule as the turn loop enforces it: that either kind satisfies it, that a *second*
//! thing is never asked for, that the refusal happens at the mouth and costs nothing,
//! and that nothing is spent getting out.
//!
//! The campaign's half — that every facility boots under this gate, that the archive
//! does not, and that the wallet is untouched by leaving — is in
//! [`campaign::tests`](crate::campaign). This is the rule itself.

use crate::state::*;
use crate::test_support::{climb_out_of_the_tunnel, exit_tunnel_cells, room_with_tunnel};
use crate::{AbilityId, IntelGate, Loadout};

/// A 14×14 room under the minimum haul, with the exit at `(5, 4)` and its tunnel running
/// north to the border — the run opens on that border cell, inside the crawlspace, the
/// way a real one does (§4.5/#466).
///
/// `intel` names the console cells and `caches` the crate cells. The cell the crawl
/// climbs out onto — `(5, 5)`, straight ahead of the mouth — is left clear by every
/// fixture here, so a test can say where the run is standing before it bumps anything.
fn haul_room(intel: Vec<Cell>, caches: Vec<Cell>) -> State {
    let exit = Cell::new(5, 4);
    let mut layout = room_with_tunnel(14, 14, exit, Direction::North);
    for &cell in &caches {
        layout.place(cell, Terrain::EquipmentCache);
    }
    let holds: Vec<AbilityId> = caches.iter().map(|_| AbilityId::Confusion).collect();
    State::new(
        layout,
        *exit_tunnel_cells(14, 14, exit, Direction::North)
            .last()
            .expect("the run has a border cell"),
        Direction::North,
        Vec::new(),
        intel,
        exit,
    )
    .with_loadout(Loadout::innate())
    .with_caches(holds)
}

/// The way out, pressed from wherever the run currently stands: crawl back in through the
/// mouth and step off the board. Returns the events of the **last** press, which is the
/// one the gate answers.
fn answer_the_exit(state: &mut State) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..32 {
        if state.outcome() != Outcome::Playing {
            break;
        }
        let was = state.player();
        events = state.step(Input::Step(Direction::North));
        if state.player() == was && !state.in_duct() {
            break; // refused at the mouth, or nothing to walk into
        }
        if events.iter().any(|e| {
            matches!(
                e,
                Event::Won | Event::ExitRefused { .. } | Event::Bumped { .. }
            )
        }) {
            break;
        }
    }
    events
}

/// **A crate satisfies the gate exactly as a console does** (#574) — the widening, in one
/// assertion. The rule forbids leaving with *nothing*, and a raid that walked out with
/// tech did not walk out with nothing.
#[test]
fn a_crate_opens_the_exit_as_an_intel_console_does() {
    // Two objectives in the room, one of each kind, both out of the player's way.
    let mut s = haul_room(vec![Cell::new(9, 9)], vec![Cell::new(5, 6)]);
    assert_eq!(s.haul_available(), 2, "one console and one crate");
    assert!(!s.exit_ready(), "empty-handed, the exit is shut");
    assert_eq!(s.intel_needed_to_exit(), 1, "and it wants one thing");

    // Out of the tunnel, then bump the crate standing one cell further in.
    climb_out_of_the_tunnel(&mut s);
    assert_eq!(s.player(), Cell::new(5, 5), "out of the mouth");
    s.step(Input::Step(Direction::South));
    assert_eq!(s.caches_opened(), 1, "the crate is open");
    assert_eq!(s.intel_in_hand(), 0, "and no console was touched");

    // …and that is the whole requirement: the console is still out and stays surplus.
    assert_eq!(s.haul_taken(), 1);
    assert_eq!(s.intel_needed_to_exit(), 0);
    assert!(s.exit_ready(), "a crate is a haul (§4.5/#574)");
    assert_eq!(s.objectives_remaining(), 1, "the console is still out");
}

/// **One is the number, and the second thing is never asked for** (#574's *"one is the
/// right number, and it should stay one"*). A quota would be a toll wearing a different
/// hat; the bite the rule wants is only the removal of the zero case.
#[test]
fn one_thing_is_the_whole_requirement() {
    let mut s = haul_room(
        vec![Cell::new(6, 5), Cell::new(9, 9)],
        vec![Cell::new(5, 6)],
    );
    assert_eq!(s.haul_available(), 3);
    climb_out_of_the_tunnel(&mut s);
    s.step(Input::Step(Direction::East));

    assert_eq!(s.intel_in_hand(), 1, "one console taken");
    assert_eq!(s.intel_needed_to_exit(), 0, "and the exit is open");
    assert_eq!(
        s.objectives_remaining() + (s.cache_total() - s.caches_opened()),
        2,
        "with two things still in the building — surplus, not a debt",
    );
    assert!(answer_the_exit(&mut s).contains(&Event::Won));
}

/// **The refusal is answered at the mouth, and it is free** (§4.4/§4.5): bumping `E` with
/// nothing in hand says why, spends no turn, moves nobody, and leaves the tunnel exactly
/// where it was — not hidden, not moved, not closed.
///
/// This is the case #574 hardened, so the *cost* of it is what wants pinning: a run that
/// finds out at the mouth can still turn round and go and get something.
#[test]
fn the_mouth_refuses_an_empty_haul_and_the_refusal_costs_nothing() {
    let mut s = haul_room(vec![Cell::new(9, 9)], vec![Cell::new(5, 6)]);
    climb_out_of_the_tunnel(&mut s);
    let (turn, at, duct) = (s.turn(), s.player(), s.layout().exit_duct().cloned());

    assert!(
        s.affordances()
            .contains(&(Some(Direction::North), Affordance::ExitRefused)),
        "the row refuses before the press does: {:?}",
        s.affordances(),
    );
    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::ExitRefused {
            still_needed: 1,
            gate: IntelGate::AtLeastOne,
        }],
    );
    assert_eq!(s.outcome(), Outcome::Playing);
    assert_eq!(s.turn(), turn, "a refusal spends nothing (§4.4)");
    assert_eq!(s.player(), at, "and moves nobody");
    assert!(!s.in_duct(), "and never let the run into the tunnel");
    assert_eq!(
        s.layout().exit_duct().cloned(),
        duct,
        "the way home is still exactly where it was",
    );

    // And it is recoverable: go and get the crate, come back, and the same press wins.
    s.step(Input::Step(Direction::South));
    assert_eq!(s.caches_opened(), 1);
    assert!(answer_the_exit(&mut s).contains(&Event::Won));
}

/// **A facility with nothing in it is not a softlock** (§4.5). Generation cannot produce
/// one — §10.2's console count floors at [`LevelConfig::INTEL_MIN`](crate::LevelConfig::INTEL_MIN),
/// which the campaign's own sweep asserts — but a hand-built state can, and there the
/// gate is *vacuously* satisfied rather than shut for ever.
#[test]
fn a_facility_with_nothing_in_it_is_vacuously_open() {
    let mut s = haul_room(Vec::new(), Vec::new());
    assert_eq!(s.haul_available(), 0);
    assert_eq!(s.intel_needed_to_exit(), 0);
    assert!(s.exit_ready());
    assert!(answer_the_exit(&mut s).contains(&Event::Won));
}

/// **The complete-the-set gate does not count crates** (§4.5/#217/#244). Quick play and
/// the archive ask for the intel, all of it, and a crate is no part of that set — the
/// widening is `AtLeastOne`'s alone, which is what keeps the archive's rule its own.
#[test]
fn the_all_intel_gate_is_untouched_by_the_widening() {
    let mut s = haul_room(
        vec![Cell::new(6, 5), Cell::new(9, 9)],
        vec![Cell::new(5, 6)],
    )
    .with_modifiers(LevelModifiers {
        intel_to_exit: IntelGate::All,
        ..LevelModifiers::default()
    });
    climb_out_of_the_tunnel(&mut s);
    assert_eq!(s.intel_needed_to_exit(), 2, "both consoles");

    // Open the crate: the salvage is real and the gate has not moved an inch.
    s.step(Input::Step(Direction::South));
    assert_eq!(s.caches_opened(), 1, "the crate is open");
    assert_eq!(
        s.intel_needed_to_exit(),
        2,
        "and the all-intel gate still wants both consoles",
    );
}
