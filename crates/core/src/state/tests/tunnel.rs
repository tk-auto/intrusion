//! The player's own tunnel (§4.5/§10.7/#466): the way in, the way out, and the run
//! that begins and ends inside it.
//!
//! The exit `E` is the inner mouth of a duct that runs to the level border and through
//! it to the outside world. So the fiction is played rather than narrated: the run
//! **opens** on the border cell, the first inputs crawl to `E` and climb out, and
//! leaving is the same thing backwards, ending in a step **off the board** that the
//! §4.5 intel gate answers.
//!
//! What the turn loop owes that model is here — the crawl, the two exit affordances,
//! the win check on a target that is not a cell — while [`super::ducts`] pins the §10.7
//! shortcut the tunnel reuses.

use crate::state::*;
use crate::test_support::{climb_out_of_the_tunnel, exit_tunnel_cells, room_with_tunnel};
use crate::IntelGate;

/// A 12×12 room whose exit sits at `(5, 4)` with its tunnel running **north** to the
/// border — so `E` is bumped from the south and the way out is `(5, 0)`. The player
/// starts where a real run starts: on that border cell, inside the crawlspace.
fn tunnelled(intel: Vec<Cell>) -> State {
    let exit = Cell::new(5, 4);
    State::new(
        room_with_tunnel(12, 12, exit, Direction::North),
        the_way_out(),
        Direction::North,
        Vec::new(),
        intel,
        exit,
    )
}

/// The way-out cell of [`tunnelled`]'s tunnel — where every run of it begins.
fn the_way_out() -> Cell {
    *exit_tunnel_cells(12, 12, Cell::new(5, 4), Direction::North)
        .last()
        .expect("the run has a last cell")
}

/// **The run opens inside the tunnel** (§4.5/#466), on the border cell, facing in — and
/// the first inputs crawl it out into the facility. No turn is spent getting there and
/// nothing is animated: it is an ordinary sequence of turns from the very first one.
///
/// While it crawls, the player is concealed and contact-safe (§10.7), which is what
/// makes "the starting area should be safe" (§10.6) a guarantee rather than a hope.
#[test]
fn a_run_begins_inside_the_tunnel_and_crawls_out() {
    let mut s = tunnelled(Vec::new());
    assert_eq!(
        s.player(),
        the_way_out(),
        "on the border, in the crawlspace"
    );
    assert!(s.in_duct(), "inside the tunnel from frame one");
    assert_eq!(s.turn(), 0, "and no turn has been spent to be there");
    assert_eq!(
        s.facing(),
        Direction::South,
        "facing along the tunnel, into the facility",
    );
    assert!(
        s.concealed_from(Cell::new(5, 6)),
        "a crawler is concealed from anywhere (§10.7)",
    );

    // Four crawls to the mouth, one cell and one turn each (§4.4).
    for step in 1..=4 {
        let events = s.step(Input::Step(Direction::South));
        assert_eq!(
            events,
            vec![Event::DuctCrawled {
                to: Cell::new(5, step)
            }]
        );
        assert_eq!(s.turn(), step, "a crawl is a spent turn");
        assert!(s.in_duct());
    }
    assert_eq!(s.player(), Cell::new(5, 4), "standing in the mouth at E");

    // At the mouth the peek looks out of it (§6.1/§10.7) — down into the room, the way
    // the tunnel points — which is the read that makes climbing out a decision.
    assert_eq!(s.facing(), Direction::South);
    assert!(s.player_fov().contains(Cell::new(5, 6)), "the room ahead");

    // And the climb out is an ordinary move onto the floor.
    let events = s.step(Input::Step(Direction::South));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(5, 5)
        }]
    );
    assert!(!s.in_duct(), "out on the floor of the facility");
}

/// **`E` from the facility side is `exit: enter`** (§4.5/#466): bumping it climbs into
/// the tunnel, exactly as a §10.7 shortcut's entry does, and the usable line says which
/// of the two it is. The row must not read the same for a shortcut you *found* and the
/// only way home.
#[test]
fn the_exit_is_entered_from_the_facility_side() {
    let mut s = tunnelled(Vec::new());
    climb_out_of_the_tunnel(&mut s);
    assert!(!s.in_duct());
    let outside = s.player();

    assert!(
        s.affordances()
            .contains(&(Some(Direction::North), Affordance::EnterExit)),
        "the mouth offers `exit: enter`: {:?}",
        s.affordances(),
    );
    assert_eq!(Affordance::EnterExit.label(), "exit: enter");
    assert_eq!(
        Affordance::EnterDuct.label(),
        "duct: enter",
        "a found shortcut keeps its own words",
    );

    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::EnteredDuct {
            at: Cell::new(5, 4),
            own_tunnel: true,
        }]
    );
    assert!(s.in_duct(), "the bump climbed in");
    assert_eq!(s.player(), Cell::new(5, 4));
    assert_ne!(s.player(), outside);
}

/// **The way out is a step off the board** (§4.5/#466), and the intel gate answers it:
/// met, the run is won; unmet, it refuses, changes nothing and costs nothing.
///
/// Both readings come from the same cell and the same press, which is the point — the
/// §4.5 rule is unchanged and only the cell you are standing on when you read it has
/// moved.
#[test]
fn the_way_out_wins_or_refuses_and_a_refusal_is_free() {
    // One objective still out: the gate is unmet.
    let mut s = tunnelled(vec![Cell::new(8, 8)]);
    assert!(!s.exit_ready());
    let (turn, at) = (s.turn(), s.player());
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

    // Nothing to fetch: the gate is met, and the same press ends the run.
    let mut s = tunnelled(Vec::new());
    assert!(s.exit_ready());
    let events = s.step(Input::Step(Direction::North));
    assert_eq!(events, vec![Event::Won]);
    assert_eq!(s.outcome(), Outcome::Won);
}

/// **The row offers the way out once you have been inside** (§4.5/§11.4/#466), and not
/// before: it is held back through the opening crawl, and it is there — `exit: leave`,
/// aimed off the board — the moment a run that has actually been played comes home.
#[test]
fn the_way_out_is_offered_once_the_run_has_been_inside() {
    let mut s = tunnelled(Vec::new());
    assert_eq!(s.affordances(), Vec::new(), "not on the way in");

    // Out into the facility, and straight back down the tunnel: nothing was fetched
    // (there is nothing to fetch), so this is the same cell under the same gate.
    climb_out_of_the_tunnel(&mut s);
    while s.player() != the_way_out() {
        s.step(Input::Step(Direction::North));
    }
    assert!(s.in_duct(), "back on the border cell");
    assert_eq!(
        s.affordances(),
        vec![(Some(Direction::North), Affordance::Leave)],
        "now the row offers exactly one thing, aimed off the board",
    );
}

/// **The gate is answered at the mouth** (§4.5/#466). Short of the intel, bumping `E`
/// from the facility side refuses — free, with the §4.5 message — instead of letting the
/// player climb in and crawl to the border to be told no somewhere they can do nothing
/// about it. The usable line says so before they press: `exit: needs the intel` on the
/// very cell that will read `exit: enter` the moment the gate is met.
#[test]
fn the_mouth_refuses_short_of_the_intel_gate() {
    // The one console sits just beyond the climb-out cell, so the whole run is: out of
    // the mouth, one bump for the intel, and back in.
    let mut s = tunnelled(vec![Cell::new(5, 6)]);
    climb_out_of_the_tunnel(&mut s);
    assert_eq!(s.player(), Cell::new(5, 5), "out of the mouth");
    assert!(!s.exit_ready(), "an objective is still out");

    assert!(
        s.affordances()
            .contains(&(Some(Direction::North), Affordance::ExitRefused)),
        "the row refuses before the press does: {:?}",
        s.affordances(),
    );
    let (turn, at) = (s.turn(), s.player());
    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::ExitRefused {
            still_needed: 1,
            gate: IntelGate::AtLeastOne,
        }],
    );
    assert!(!s.in_duct(), "and it never let us into the tunnel");
    assert_eq!(s.turn(), turn, "a refusal spends nothing (§4.4)");
    assert_eq!(s.player(), at, "and moves nobody");

    // Take the intel — a bump, so it moves nobody — and the same cell, the same press,
    // is now the way home.
    s.step(Input::Step(Direction::South));
    assert!(s.exit_ready(), "the gate is met");
    assert_eq!(s.player(), at, "taking intel is a bump, not a move");
    let events = s.step(Input::Step(Direction::North));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::EnteredDuct { .. })),
        "with the gate met the mouth opens: {events:?}",
    );
    assert!(s.in_duct());
}

/// The way out is **one cell and one direction** (§4.5/#466). Every other step off the
/// board is the free mis-input a wall bump has always been (§4.3), and every other cell
/// of the tunnel is walled in — the §10.7 confinement, which is what keeps a crawl a
/// crawl.
///
/// And on the opening crawl the row is **silent about it**: the run starts on that cell,
/// and a usable line whose only entry is *leave* is the wrong first thing to say to a
/// player who has not been in yet. The press still answers (below, and in
/// [`the_way_out_wins_or_refuses_and_a_refusal_is_free`]) — it is the offer that waits.
#[test]
fn only_the_border_cell_leaves_and_only_outward() {
    let mut s = tunnelled(Vec::new());

    // Sideways off the board from the way-out cell: nothing offered, nothing spent.
    for dir in [Direction::East, Direction::West] {
        let events = s.step(Input::Step(dir));
        assert_eq!(s.turn(), 0, "{dir:?} was a free bump");
        assert!(
            !events.contains(&Event::Won),
            "{dir:?} is not the way out: {events:?}",
        );
    }
    assert_eq!(
        s.affordances(),
        Vec::new(),
        "and the row says nothing about the way home before we have been inside (#466)",
    );

    // One crawl inward, and the way out is behind us: the step that would have won is
    // now a plain crawl.
    s.step(Input::Step(Direction::South));
    assert_eq!(s.affordances(), Vec::new(), "no exit from mid-tunnel");
    let events = s.step(Input::Step(Direction::North));
    assert_eq!(events, vec![Event::DuctCrawled { to: the_way_out() }]);
    assert_eq!(
        s.outcome(),
        Outcome::Playing,
        "crawling back is not leaving"
    );
}

/// A **dragged body cannot follow you home** (§10.7/§8.3): the tunnel refuses a
/// dragging player exactly as any duct entry does — a body does not fit in the walls —
/// so leaving is a decision to let it go. The refusal is a free bump, and the usable
/// line goes quiet rather than promising a climb it will not deliver.
#[test]
fn a_dragged_body_cannot_be_taken_into_the_tunnel() {
    // A guard standing in the room below the mouth, facing away (§7.1's spawn facing),
    // so the player can climb out behind it and put it down.
    let exit = Cell::new(5, 4);
    let mut s = State::new(
        room_with_tunnel(12, 12, exit, Direction::North),
        the_way_out(),
        Direction::North,
        vec![crate::Guard::stationary(Cell::new(5, 6))],
        Vec::new(),
        exit,
    );
    climb_out_of_the_tunnel(&mut s);
    assert_eq!(s.player(), Cell::new(5, 5), "out of the mouth, behind it");

    // Take it down, walk onto the body and wait to take hold (§8.3/#451).
    s.step(Input::Step(Direction::South));
    assert_eq!(s.bodies().len(), 1, "a takedown leaves a body (§7.2)");
    s.step(Input::Step(Direction::South));
    s.step(Input::Wait);
    assert_eq!(s.dragging(), Some(Cell::new(5, 6)), "hands full");

    // Back to the mouth and bump it: refused, free, and unoffered.
    while s.player() != Cell::new(5, 5) {
        s.step(Input::Step(Direction::North));
    }
    let turn = s.turn();
    let events = s.step(Input::Step(Direction::North));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::EnteredDuct { .. })),
        "a body cannot follow into the walls: {events:?}",
    );
    assert!(!s.in_duct());
    assert_eq!(s.turn(), turn, "and the refusal is free (§4.4)");
    assert!(
        !s.affordances()
            .iter()
            .any(|(_, a)| *a == Affordance::EnterExit),
        "the row does not offer what the bump will not do",
    );
}
