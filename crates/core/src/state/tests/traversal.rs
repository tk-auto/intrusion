//! The auto lateral-shift past an obstacle (#57).
//!
//! The traversal experiment in [`super::super::traversal`]: which blocked steps slide
//! and which stay the free §4.4 bump, how the forward-diagonal breaks a two-sided
//! tie, the refusals that keep the slide from walking you into a capture, and the
//! runtime kill-switch that restores the plain bump.

use crate::state::*;
use crate::test_support::open_room;

/// §57/§4.4: a step blocked by a pillar with **exactly one** open lateral slides
/// one cell that way instead of dead-stopping. The slide is a successful move —
/// it spends the turn (§4.4) and sets facing to the shift direction (§5).
#[test]
fn a_blocked_step_with_one_open_side_slides_past() {
    let mut layout = open_room(6, 6);
    layout.place(Cell::new(3, 1), Terrain::Wall); // the pillar dead ahead (east)
                                                  // Player hard against the north wall so its two laterals are: north border wall
                                                  // (blocked) and south floor (open) — exactly one, unambiguous.
    let mut s = State::new(
        layout,
        Cell::new(2, 1),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(4, 4),
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(2, 2)
        }],
        "the bump into the pillar slid south into the one open lateral",
    );
    assert_eq!(s.player(), Cell::new(2, 2), "shifted one cell south");
    assert_eq!(
        s.facing(),
        Direction::South,
        "facing follows the successful step (§5), not the aimed east",
    );
    assert_eq!(
        s.turn(),
        turn_before + 1,
        "the slide is a move — it spends the turn"
    );
}

/// §57/§4.4: a blocked step with **both** laterals open *and* both forward-diagonals
/// open — a lone pillar, clear either way — is genuinely ambiguous, so it stays the
/// free wall-bump it has always been (§4.4). The forward-diagonal tiebreak refuses
/// (both open), nothing moves, no turn is spent.
#[test]
fn a_blocked_step_with_both_sides_open_does_not_slide() {
    let mut layout = open_room(6, 6);
    layout.place(Cell::new(3, 2), Terrain::Wall); // lone pillar dead ahead (east)
    let mut s = State::new(
        layout,
        // Both laterals — north (2,1), south (2,3) — and both forward-diagonals —
        // north-east (3,1), south-east (3,3) — are open floor: nothing to round.
        Cell::new(2, 2),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(4, 4),
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(3, 2)
        }],
        "a lone pillar, open both ways round, is ambiguous — a free bump, no slide",
    );
    assert_eq!(s.player(), Cell::new(2, 2), "nothing moved");
    assert_eq!(s.turn(), turn_before, "an ambiguous bump stays free");
}

/// §57/§4.4: a blocked step with **neither** lateral open (a dead end) stays a
/// free bump — there is nowhere to slide.
#[test]
fn a_blocked_step_with_no_open_side_does_not_slide() {
    let mut layout = open_room(6, 6);
    layout.place(Cell::new(2, 1), Terrain::Wall); // dead ahead (north)
    layout.place(Cell::new(3, 2), Terrain::Wall); // east lateral blocked
    layout.place(Cell::new(1, 2), Terrain::Wall); // west lateral blocked
    let mut s = State::new(
        layout,
        Cell::new(2, 2),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(4, 4),
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(2, 1)
        }],
        "boxed in on both sides — a free bump, no slide",
    );
    assert_eq!(s.player(), Cell::new(2, 2), "nothing moved");
    assert_eq!(s.turn(), turn_before, "a dead-end bump stays free");
}

/// §57: the slide targets **plain floor only** — a lateral that is an interactable
/// (here a hideout) does not qualify, so it is never auto-entered. With a cupboard
/// on one side and floor on the other, the one *plain* lateral wins and the player
/// slides onto the floor, away from the cupboard — proof the cupboard was excluded
/// (had it counted, the two open sides would have been ambiguous and nothing would
/// have moved).
#[test]
fn the_slide_never_auto_enters_an_interactable_lateral() {
    let mut layout = open_room(6, 6);
    layout.place(Cell::new(3, 2), Terrain::Wall); // pillar dead ahead (east)
    layout.place(Cell::new(2, 1), Terrain::Hideout); // north lateral: a cupboard
                                                     // south lateral (2,3) stays plain floor
    let mut s = State::new(
        layout,
        Cell::new(2, 2),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(4, 4),
    );

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(2, 3)
        }],
        "slid onto the plain floor, not into the cupboard",
    );
    assert_eq!(s.player(), Cell::new(2, 3), "on the floor south");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::EnteredHideout { .. })),
        "the cupboard was never climbed into",
    );
}

/// §57/§4.5 — detection is deliberately *not* guarded: a slide may land in a
/// guard's cone, because being seen is not losing (§4.5) and the player chose to
/// press into the obstacle. The guard's straight-south cone covers the one open
/// lateral, yet the slide proceeds — dodging cones is a separate follow-up ticket.
#[test]
fn the_slide_proceeds_into_a_guards_cone_being_seen_is_not_losing() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(4, 3), Terrain::Wall); // pillar dead ahead (north)
    layout.place(Cell::new(3, 4), Terrain::Wall); // west lateral blocked
                                                  // east lateral (5,4) is plain floor — a guard two cells north watches it.
    let dest = Cell::new(5, 4);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 2))], // faces south, cone down the column
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(
        s.guards()[0].fov().contains(dest),
        "precondition: the guard's cone covers the destination",
    );
    assert!(
        s.guards()[0].pos().manhattan_distance(dest) > 1,
        "precondition: the guard is not adjacent — only the cone is at stake",
    );
    let turn_before = s.turn();

    s.step(Input::Step(Direction::North));
    assert_eq!(
        s.player(),
        dest,
        "the slide proceeds into the cone — detection is not the slide's concern",
    );
    assert_eq!(s.turn(), turn_before + 1, "the slide spends the turn");
}

/// §57/§2.2/§4.5 — the fairness guard, capture clause: a slide is refused when a
/// guard stands orthogonally adjacent to the destination, even when its cone does
/// *not* cover it (here the destination is in the guard's rear blind spot, §155) —
/// nothing may auto-step the player into a cell a guard can capture next phase.
#[test]
fn the_slide_is_refused_next_to_a_guard_that_could_capture() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(4, 3), Terrain::Wall); // pillar dead ahead (north)
    layout.place(Cell::new(3, 4), Terrain::Wall); // west lateral blocked
                                                  // east lateral (5,4) is plain floor — a guard sits directly south of it.
    let dest = Cell::new(5, 4);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 5))], // faces south; dest is behind it
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(
        !s.guards()[0].fov().contains(dest),
        "precondition: the destination is in the guard's rear blind spot, not its cone",
    );
    assert_eq!(
        s.guards()[0].pos().manhattan_distance(dest),
        1,
        "precondition: the guard is orthogonally adjacent to the destination",
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(4, 3)
        }],
        "a guard could step into the destination — the slide is refused",
    );
    assert_eq!(
        s.player(),
        Cell::new(4, 4),
        "the player did not slide next to the guard"
    );
    assert_eq!(s.turn(), turn_before, "a refused slide stays free");
}

/// §57: with **both** laterals open, the forward-diagonal breaks the tie — the
/// slide rounds the obstacle toward the side where the path continues. A pillar
/// dead ahead that also blocks the north-east forward-diagonal, with the
/// south-east one open, slides *south* (not north): the obstacle extends left, so
/// you round it right. Still one orthogonal step — the diagonal is only read.
#[test]
fn both_sides_open_slides_round_the_obstacle_by_the_open_diagonal() {
    let mut layout = open_room(8, 8);
    layout.place(Cell::new(4, 3), Terrain::Wall); // pillar dead ahead (east)
    layout.place(Cell::new(4, 2), Terrain::Wall); // north-east forward-diagonal blocked
                                                  // south-east forward-diagonal (4,4) stays open floor; both laterals (3,2)/(3,4) open
    let mut s = State::new(
        layout,
        Cell::new(3, 3),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(6, 6),
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(3, 4)
        }],
        "the obstacle blocks the NE diagonal — round it south toward the open SE",
    );
    assert_eq!(s.player(), Cell::new(3, 4), "slid one cell south");
    assert_eq!(
        s.facing(),
        Direction::South,
        "facing follows the slide (§5)"
    );
    assert_eq!(s.turn(), turn_before + 1, "the slide spends the turn");
}

/// §57: the reported playtest case (#57) — crouched against the long edge of a
/// two-cell table, pressing *into* it (a held-crouch dead bump) with both laterals
/// open. The table's second cell blocks the near forward-diagonal while the far one
/// is open, so the slide rounds the table toward the open corridor rather than
/// dead-stopping on "blocked".
///
/// ```text
///   . . π      the table's far cell blocks the NE forward-diagonal
///   . @ π   →  press east into the held table
///   . . .      the SE forward-diagonal is open — round south
/// ```
#[test]
fn crouched_against_a_two_cell_table_slides_round_it() {
    let mut layout = open_room(8, 8);
    layout.place(Cell::new(4, 2), Terrain::PartialCover); // the two-cell table…
    layout.place(Cell::new(4, 3), Terrain::PartialCover); // …the player crouches behind
    let mut s = State::new(
        layout,
        Cell::new(3, 3),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(6, 6),
    );

    // First east bump crouches behind the table run {(4,2),(4,3)} (§10.3).
    let crouch = s.step(Input::Step(Direction::East));
    assert!(
        crouch.contains(&Event::Crouched {
            behind: Cell::new(4, 3)
        }),
        "the first bump crouches behind the table",
    );
    assert_eq!(
        s.player(),
        Cell::new(3, 3),
        "crouching does not move the player"
    );
    let turn_after_crouch = s.turn();

    // Second east bump is a held-crouch dead bump: both laterals (3,2)/(3,4) are
    // open, the table's second cell (4,2) blocks the NE forward-diagonal, and the
    // SE (4,4) is open — so it rounds the table south.
    let slide = s.step(Input::Step(Direction::East));
    assert_eq!(
        slide,
        vec![Event::Moved {
            to: Cell::new(3, 4)
        }],
        "the held-crouch bump rounds the two-cell table south",
    );
    assert_eq!(
        s.player(),
        Cell::new(3, 4),
        "slid one cell south, round the table"
    );
    assert_eq!(
        s.facing(),
        Direction::South,
        "facing follows the slide (§5)"
    );
    assert_eq!(s.turn(), turn_after_crouch + 1, "the slide spends the turn");
}

/// §57/§4.4: with both laterals open but **both** forward-diagonals blocked (the
/// obstacle walls off the path both ways round), the tiebreak has no answer — it
/// stays a free bump, nothing moves.
#[test]
fn both_sides_open_but_both_diagonals_blocked_does_not_slide() {
    let mut layout = open_room(8, 8);
    layout.place(Cell::new(4, 3), Terrain::Wall); // dead ahead (east)
    layout.place(Cell::new(4, 2), Terrain::Wall); // NE forward-diagonal blocked
    layout.place(Cell::new(4, 4), Terrain::Wall); // SE forward-diagonal blocked
    let mut s = State::new(
        layout,
        Cell::new(3, 3), // laterals (3,2)/(3,4) open, but neither leads onward
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(6, 6),
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(4, 3)
        }],
        "blocked both ways round — no unambiguous side, a free bump",
    );
    assert_eq!(s.player(), Cell::new(3, 3), "nothing moved");
    assert_eq!(s.turn(), turn_before, "a refused slide stays free");
}

/// §57: the kill-switch. With the auto-slide turned off, the one-open-side case
/// that would slide (`a_blocked_step_with_one_open_side_slides_past`) reverts to
/// the plain §4.4 free bump — nothing moves, no turn is spent.
#[test]
fn the_auto_slide_kill_switch_restores_the_free_bump() {
    let mut layout = open_room(6, 6);
    layout.place(Cell::new(3, 1), Terrain::Wall); // the pillar dead ahead (east)
    let mut s = State::new(
        layout,
        Cell::new(2, 1), // north border wall blocked, south floor open — would slide
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(4, 4),
    );
    s.set_auto_slide(false);
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(3, 1)
        }],
        "with the slide off, the dead bump is the free §4.4 no-op",
    );
    assert_eq!(s.player(), Cell::new(2, 1), "nothing moved");
    assert_eq!(s.turn(), turn_before, "a free bump does not spend the turn");
}

// --- Autodoors (§8.3/§7.6/§10.4, #241) ----------------------------------------
