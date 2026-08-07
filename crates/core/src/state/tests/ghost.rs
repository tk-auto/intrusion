//! The **ghost** debug switch through the turn loop (§12.6/#507).
//!
//! A playtest **instrument**, not a level modifier: while it is on, no guard ever
//! detects the player, so a run that misbehaved on the deployed page can be stood in
//! rather than only watched. It is the one thing behind the §12.6 debug gate that
//! touches the facility, and this file pins the four claims that make that containable
//! — what it stops, what it deliberately does **not** stop, that it lets go the
//! ordinary way, and that the danger overlay keeps its picture.
//!
//! The switch itself is one clause on the §10.3 concealment path
//! ([`State::concealed_from`]), so there is no new detection rule to pin here: what is
//! pinned is that the clause reaches every consequence of detection at once, and stops
//! exactly where §4.5 says it must.
//!
//! [`State::concealed_from`]: crate::State::concealed_from

use crate::state::*;
use crate::test_support::open_room;
use crate::DebugModifiers;

/// A state built with the ghost already on — the baked-build path
/// ([`State::with_debug`]).
fn ghosted(state: State) -> State {
    state.with_debug(DebugModifiers {
        ghost: true,
        ..DebugModifiers::default()
    })
}

/// A guard standing point-blank in front of the player, facing them: `stationary`
/// spawns facing south (§7.1), so the player is placed directly below it, inside the
/// wedge and on the touching ring.
fn nose_to_nose() -> State {
    State::new(
        open_room(12, 12),
        Cell::new(5, 6),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 5))],
        Vec::new(),
        Cell::new(10, 10),
    )
}

/// A guard three cells off with a pillar between it and the player: the startup look
/// (§4.2) finds nothing, and one step sideways is the step into its open wedge.
///
/// The scene `guards::a_fresh_detection_is_reported_once_and_rearms_on_broken_contact`
/// pins the ordinary detection on, borrowed here so that the only thing differing
/// between the control and the ghost is the switch — and so that the claim is about a
/// detection the run has yet to make, not one the startup turn already made.
fn about_to_be_seen() -> State {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 7), Terrain::Wall);
    State::new(
        layout,
        Cell::new(5, 8),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 5))],
        Vec::new(),
        Cell::new(10, 10),
    )
}

/// **The switch's whole promise** (§12.6/#507): with a guard adjacent and facing the
/// player, nothing detects them — no live detection, no [`Event::Detected`], no §7.6
/// transition out of Calm, and so no §7.3 rung-1 sighting trigger.
///
/// The control run is the same scene without the switch, so every assertion below is
/// paired with the thing it is the absence of: this is a scene that detects loudly.
#[test]
fn no_guard_detects_a_ghost() {
    // The control: step out of the pillar's shadow into the open wedge, and it is
    // exactly as loud as stepping in front of a guard should be.
    let mut seen = about_to_be_seen();
    assert!(
        !seen.guards()[0].detected_player(),
        "precondition: unseen behind the pillar",
    );
    let events = seen.step(Input::Step(Direction::East));
    assert!(
        events.iter().any(|e| matches!(e, Event::Detected { .. })),
        "stepping into the wedge is a sighting: {events:?}",
    );
    assert!(seen.guard_detects_now(&seen.guards()[0]), "…and it has you");
    assert_eq!(seen.guards()[0].state(), GuardState::Chasing, "…and reacts");
    // A rung-1 sighting is a run of contact turns inside the §7.6 window, so it takes a
    // few turns of being stared at rather than one.
    for _ in 0..12 {
        seen.step(Input::Wait);
    }
    assert!(seen.alert() >= 1, "…and pushes the §7.3 ladder to rung 1");

    // The same scene and the same step, ghosted.
    let mut ghost = ghosted(about_to_be_seen());
    for input in
        std::iter::once(Input::Step(Direction::East)).chain(std::iter::repeat_n(Input::Wait, 12))
    {
        let events = ghost.step(input);
        assert!(
            !events.iter().any(|e| matches!(e, Event::Detected { .. })),
            "a ghost is never sighted: {events:?}",
        );
    }
    assert!(
        !ghost.guard_detects_now(&ghost.guards()[0]),
        "the cone passes through a ghost",
    );
    assert!(
        !ghost.guards()[0].detected_player(),
        "…and the guard's own latch is never set",
    );
    assert_eq!(
        ghost.guards()[0].state(),
        GuardState::Calm,
        "so no chase ever starts (§7.6)",
    );
    assert_eq!(
        ghost.alert(),
        0,
        "and the §7.3 ladder never leaves the floor"
    );
    assert_eq!(
        ghost.outcome(),
        Outcome::Playing,
        "a dozen turns in the open wedge, and the run is still live",
    );
}

/// **Contact still captures** (§4.5 **[SETTLED]**) — the one behaviour the switch must
/// not change, and the reason it can be as blunt as it is. §8.3 already says this about
/// exactly this state: *invisible is not safe*.
///
/// A 1-wide corridor, the guard facing across it into the wall, so nothing about this
/// capture comes from being seen: the guard is called down the corridor and its only
/// route runs straight through the cell the ghost is standing in. Walking a ghost
/// through a patrol route is still a way to lose.
#[test]
fn contact_still_captures_a_ghost() {
    let mut s = ghosted(State::new(
        open_room(10, 3),
        Cell::new(6, 1),
        Direction::East,
        vec![Guard::patrolling(Cell::new(7, 1))],
        Vec::new(),
        Cell::new(1, 1),
    ));
    s.set_guard_close_chance(0);
    assert!(
        !s.guard_detects_now(&s.guards()[0]),
        "precondition: nothing sees the ghost",
    );

    assert!(
        s.call_guards_to_for_test(Cell::new(1, 1), 1),
        "the guard was free to answer",
    );
    for _ in 0..16 {
        if s.outcome() != Outcome::Playing {
            break;
        }
        s.step(Input::Wait);
    }
    assert_eq!(
        s.outcome(),
        Outcome::Lost,
        "contact is capture (§4.5), ghost or not",
    );
    assert!(
        !s.guards().is_empty() && !s.guards()[0].detected_player(),
        "…and it never saw the player it walked into",
    );
}

/// Flipped on **mid-chase**, the switch lets a pursuer go through the ordinary §7.6
/// lose-sight path — the guard stops chasing and searches the ground it last had the
/// player on — rather than through any special case of its own.
///
/// That falls out of the seam: a ghost is concealed, a concealed player is not
/// detected, and a chaser that stops detecting loses contact exactly as it does when
/// the player rounds a corner. The point of asserting it is that nothing else happens
/// either — the guard is not reset, teleported or becalmed.
#[test]
fn flipping_it_on_mid_chase_ends_the_chase_the_ordinary_way() {
    // A **patrolling** guard, so the chase it drops is one it could act on: it walks to
    // where it last had the player and searches there, which is the whole §7.6 shape a
    // special case would have skipped.
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 8),
        Direction::North,
        vec![Guard::patrolling(Cell::new(5, 5))],
        Vec::new(),
        Cell::new(11, 11),
    );
    s.set_guard_close_chance(0);
    let last_known = s.player();
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Chasing,
        "precondition: a live chase",
    );

    s.toggle_ghost();
    assert!(
        !s.guard_detects_now(&s.guards()[0]),
        "the chaser has lost the player the moment the switch goes on",
    );

    // Step clear of the lead so the pursuit is resolved against ground the player has
    // left — and so contact, which the ghost does not stop, cannot end the run first.
    for _ in 0..12 {
        s.step(Input::Step(Direction::East));
        if s.guards()[0].searching() {
            break;
        }
    }
    assert_eq!(
        s.outcome(),
        Outcome::Playing,
        "nothing walked into the ghost"
    );
    assert_ne!(
        s.guards()[0].state(),
        GuardState::Chasing,
        "the chase is over (§7.6)",
    );
    assert!(
        s.guards()[0].searching(),
        "…and it is combing the ground it last had them on, like any lost chase",
    );
    assert_eq!(
        s.guards()[0].search_focus(),
        Some(last_known),
        "…which is exactly where the player was when the switch went on",
    );
}

/// **The danger overlay keeps painting** (§11.5/#507), and this is the one place the
/// picture is allowed to diverge from the rule.
///
/// §11.5 is **[SETTLED]** that the overlay is the *literal* detection set; under the
/// ghost that set is empty, so a literal reading would blank the board exactly when
/// someone is debugging vision. So the overlay keeps painting the set that *would*
/// detect a detectable player: red goes back to meaning *this cell is watched*, and the
/// picture is identical to the un-ghosted one — which is what the two runs are compared
/// for here rather than merely asserting it is non-empty.
#[test]
fn the_danger_overlay_still_paints_watched_cells() {
    let plain = nose_to_nose();
    let ghost = ghosted(nose_to_nose());

    let watched: Vec<Cell> = plain.visible_cone_cells().collect();
    assert!(
        watched.contains(&plain.player()),
        "precondition: the player's own cell is watched",
    );
    assert_eq!(
        ghost.visible_cone_cells().collect::<Vec<_>>(),
        watched,
        "the overlay is the same picture with the switch on",
    );
    assert!(
        ghost.in_visible_danger(ghost.player()),
        "…so a ghost standing in a cone still reads as standing in one",
    );
}

/// The **latch** (§12.6/#507): using the switch marks the *run*, and switching it back
/// off does not unmark it. The turns already recorded were played under bent rules and
/// no later toggle un-bends them — the same honesty omni-vision's own row shows about
/// tile memory.
///
/// This is what the shell refuses the replay export on, so it is asserted as a latch
/// rather than as a live read of the switch.
#[test]
fn the_ghost_latches_on_the_run_and_never_lifts() {
    let mut s = nose_to_nose();
    assert!(!s.ghosted(), "an ordinary run is not latched");
    assert!(!s.debug().ghost);

    s.toggle_ghost();
    assert!(s.debug().ghost, "the switch is on");
    assert!(s.ghosted(), "…and the run is marked");

    s.toggle_ghost();
    assert!(!s.debug().ghost, "the switch is off again");
    assert!(
        s.ghosted(),
        "but the run stays marked — those turns happened"
    );

    // A run that *starts* ghosted is marked from turn zero, so there is never a stretch
    // of it that could honestly be handed on.
    let baked = ghosted(nose_to_nose());
    assert!(baked.ghosted());
}

/// The switch is **off by default and reachable only through the debug seam**: nothing
/// in a level's own config can turn it on, because it is not on
/// [`LevelModifiers`](crate::LevelModifiers) at all — a `State` built the ordinary way
/// plays the ordinary game.
///
/// The compiler already guarantees the shape of this; what it does not guarantee is
/// that a future default flips, so the default is pinned where the rule is read.
#[test]
fn a_run_is_not_ghosted_unless_a_debug_session_says_so() {
    let s = nose_to_nose();
    assert_eq!(s.debug(), DebugModifiers::default());
    assert!(!s.debug().ghost);
    assert!(!s.ghosted());
    assert!(
        s.guard_detects_now(&s.guards()[0]) || s.guards()[0].fov().contains(s.player()),
        "…and the guards look for it exactly as they always did",
    );
}

/// The **perception-only** view of a switch set (§12.4/#507) — what a replay is
/// re-simulated under, so a rule-bending session cannot alter a run it merely watches.
///
/// Pinned here rather than beside the type because the property that matters is the
/// pair: the ghost is dropped, and everything that only changes the *picture* is kept.
#[test]
fn the_perception_only_view_drops_the_ghost_and_keeps_the_rest() {
    let both = DebugModifiers {
        reveal_whole_level: true,
        ghost: true,
    };
    assert_eq!(
        both.perception_only(),
        DebugModifiers {
            reveal_whole_level: true,
            ghost: false,
        },
    );
    assert_eq!(
        DebugModifiers::default().perception_only(),
        DebugModifiers::default(),
    );

    // …and a run re-simulated under it is the run as recorded: the guards look, and
    // the same scene detects.
    let watched = nose_to_nose().with_debug(both.perception_only());
    assert!(
        !watched.ghosted(),
        "so the watcher's own switch never latches"
    );
    assert!(watched.guard_detects_now(&watched.guards()[0]));
}
