//! The Saver: §4.5's one declared exception, through the turn loop (§8.3/#243).
//!
//! A guard's capturing step is turned into a takedown of that guard and the run goes
//! on — **once a facility**. What these pin is the pair the design leans on: that the
//! exception really fires where §4.5 would have ended the run, and that it really
//! stops firing afterwards, so the next contact is lethal exactly as the settled rule
//! says.
//!
//! The setup is deliberately the same one
//! [`a_guard_stepping_into_the_player_captures`](super::guards) uses — a Responding
//! guard walking west into a standing player — so the two files read as the same
//! moment with and without the ability, and a change to the capture path shows up in
//! both.

use crate::ability::{AbilityId, AbilityState, Loadout};
use crate::body::Body;
use crate::state::*;
use crate::test_support::open_room;

/// A guard walking west along row 4, `from` cells out, on its way past the player —
/// the shape of every setup here.
fn closing_from(x: u32) -> Guard {
    let mut guard = Guard::patrolling(Cell::new(x, 4));
    guard.respond_to(Cell::new(1, 4));
    guard
}

/// The moment: a guard two cells away, closing, that will step into the player on the
/// next turn. Its own reactive turn-and-step is spent by [`State::new`]'s startup
/// turn, so the very next [`Input::Wait`] is the capture. `behind` places any further
/// guards walking the same line, for the turns after the first.
fn about_to_be_caught(loadout: Loadout, behind: &[u32]) -> State {
    let mut guards = vec![closing_from(6)];
    guards.extend(behind.iter().map(|x| closing_from(*x)));
    let s = State::new(
        open_room(10, 10),
        Cell::new(4, 4),
        Direction::North,
        guards,
        Vec::new(),
        Cell::new(8, 8),
    )
    .with_loadout(loadout);
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(5, 4),
        "the guard is adjacent"
    );
    assert_eq!(s.outcome(), Outcome::Playing);
    s
}

/// The whole ability, in one turn: the guard that would have captured is **taken
/// down** instead, the run continues, and both facts are reported — the save, and the
/// body it left (§7.2).
///
/// The body falls in the guard's own cell, not the player's: a lunge that is turned
/// over never arrives.
#[test]
fn a_capture_becomes_a_takedown_while_the_saver_is_held() {
    let mut s = about_to_be_caught(Loadout::innate().with(AbilityId::Saver), &[]);

    let events = s.step(Input::Wait);
    assert_eq!(
        events,
        vec![
            Event::Detected {
                by: Cell::new(5, 4)
            },
            Event::CaptureSaved {
                at: Cell::new(4, 4)
            },
            Event::TakenDown {
                at: Cell::new(5, 4)
            },
        ],
        "the grab is reported where it happened, the body where the guard stood",
    );
    assert_eq!(s.outcome(), Outcome::Playing, "the run does not end (§4.5)");
    assert!(
        s.guards().is_empty(),
        "the attacker is permanently out (§7.2)"
    );
    assert_eq!(
        s.bodies().iter().map(Body::cell).collect::<Vec<_>>(),
        vec![Cell::new(5, 4)],
        "a body lies where the guard stood — the §7.3 clock starts there",
    );
}

/// **Without it, the same turn ends the run.** The control for the test above: every
/// difference between the two is the ability, and §4.5 is untouched for a run that
/// does not hold it.
#[test]
fn the_same_capture_without_the_saver_ends_the_run() {
    let mut s = about_to_be_caught(Loadout::innate(), &[]);

    let events = s.step(Input::Wait);
    assert!(
        events.iter().any(|e| matches!(e, Event::Captured { .. })),
        "contact is capture (§4.5): {events:?}",
    );
    assert_eq!(s.outcome(), Outcome::Lost);
}

/// **One per facility, and the second contact is lethal** (§8.2/#302). The exception
/// is bounded by the level rather than by a clock, so waiting changes nothing: the
/// run that was saved is a run with no save left.
#[test]
fn the_next_capture_after_a_save_ends_the_run() {
    // A second guard walking the same line a few cells back, so the run's next
    // contact comes without anything else about the world changing.
    let mut s = about_to_be_caught(Loadout::innate().with(AbilityId::Saver), &[8]);
    s.step(Input::Wait);
    assert_eq!(s.outcome(), Outcome::Playing, "the first one was survived");

    let mut ended = false;
    for _ in 0..8 {
        let events = s.step(Input::Wait);
        if events.iter().any(|e| matches!(e, Event::Captured { .. })) {
            ended = true;
            break;
        }
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::CaptureSaved { .. })),
            "the level's one save is spent — nothing may fire twice",
        );
    }
    assert!(ended, "the second guard reaches the player and captures");
    assert_eq!(s.outcome(), Outcome::Lost);
}

/// What the bar says, before and after (§11.4/§8.2): a budgeted passive reads as the
/// supply it has left, then as spent — never as a standing `(on)` promising a rescue
/// the run has already used, and never as a cooldown counting down to a return that
/// is not coming.
#[test]
fn the_bar_shows_the_save_left_and_then_that_there_is_none() {
    let mut s = about_to_be_caught(Loadout::innate().with(AbilityId::Saver), &[]);
    assert_eq!(
        s.ability_state(AbilityId::Saver),
        AbilityState::Limited { uses: 1 },
        "one save, and the bar says so",
    );

    s.step(Input::Wait);
    assert_eq!(
        s.ability_state(AbilityId::Saver),
        AbilityState::Exhausted,
        "spent for the rest of the facility",
    );
}

/// **A capture that never happens spends nothing.** A guard that walks into a player
/// standing in an unwitnessed cupboard is *refused* (§10.3): it never enters the cell,
/// so there was no contact, so there is nothing for the level's one save to be spent
/// on. The ability fires on the §4.5 moment and not on a near miss — which matters,
/// because the near miss is the common case the whole hiding game is made of.
#[test]
fn a_guard_refused_by_a_cupboard_spends_no_save() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(4, 4), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![closing_from(6)],
        Vec::new(),
        Cell::new(8, 8),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Saver));
    assert!(s.hidden(), "the player is in the cupboard");

    for _ in 0..5 {
        let events = s.step(Input::Wait);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::CaptureSaved { .. })),
            "a refused step is not contact (§10.3): {events:?}",
        );
    }
    assert_eq!(s.outcome(), Outcome::Playing);
    assert_eq!(
        s.ability_state(AbilityId::Saver),
        AbilityState::Limited { uses: 1 },
        "the save is still there to be spent",
    );
}

/// **Determinism** (§12.4): the same seed and the same inputs produce the same run,
/// with the Saver held. It is a passive fired by the world rather than by a press, so
/// nothing about it is in the input stream — which is exactly why it is worth pinning
/// that two replays of one script still agree.
#[test]
fn a_run_holding_the_saver_replays_identically() {
    let script = [
        Input::Step(Direction::South),
        Input::Wait,
        Input::Step(Direction::East),
        Input::Wait,
        Input::Step(Direction::South),
    ];
    let play = || {
        let mut s = about_to_be_caught(Loadout::innate().with(AbilityId::Saver), &[8]);
        let events: Vec<Event> = script.iter().flat_map(|&i| s.step(i)).collect();
        (
            events,
            s.player(),
            s.turn(),
            s.outcome(),
            s.ability_state(AbilityId::Saver),
            s.bodies().iter().map(Body::cell).collect::<Vec<_>>(),
            crate::render::render(&s),
        )
    };
    assert_eq!(
        play(),
        play(),
        "same state + inputs must replay identically"
    );
}
