//! **Guide** — the passive compass (§8.2/§8.3/§11.5a, #505).
//!
//! The ability is one line of behaviour and four lines of restraint, and it is the
//! restraint these tests exist to hold. A guide that pathed would be a solver; one that
//! named a cell would undercut the fog **[SETTLED]** in §11.5a and the v3 intel sink
//! (#215) that sells exactly that; one that flickered between two equidistant answers
//! would be a §12.4 desync. So:
//!
//! - **A bearing, and only a bearing.** Exactly one of the eight neighbours is washed
//!   while something is left to take, and none when nothing is.
//! - **Straight-line and wall-blind.** It points *through* a wall at a console rather
//!   than toward the doorway that would actually get you there. That is the
//!   specification, and it will be re-reported as a bug.
//! - **It reveals nothing else.** Tile memory, the fog and every other cell on the board
//!   are exactly as they would be without it.
//! - **Weakest cue on the board.** A threat channel paints over it, always.
//! - **Deterministic.** A tie resolves by the level's own ordering, identically on
//!   replay.

use crate::state::*;
use crate::test_support::open_room;

/// A player holding the Guide in an open room, with intel consoles stamped at
/// `consoles`. The exit is parked in a far corner so it is never the nearest anything.
fn navigator(player: Cell, consoles: Vec<Cell>) -> State {
    State::new(
        open_room(40, 40),
        player,
        Direction::East,
        Vec::new(),
        consoles,
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Guide))
}

/// The one cell the compass is painting **on this frame**, or `None` when it is dark.
fn needle(s: &State) -> Option<Cell> {
    let lit: Vec<Cell> = s.effect_cell_marks().collect();
    assert!(lit.len() <= 1, "a compass paints one cell or none: {lit:?}");
    lit.first().copied()
}

/// Wait until the compass is **due**, then read it (§8.3/#505). The bearing pulses, so
/// every assertion about *where* it points has to stand on a turn it is lit — and these
/// fixtures hold no guards, so spending turns to get there changes nothing else.
fn fix(s: &mut State) -> Option<Cell> {
    for _ in 0..=GUIDE_BLINK_TURNS {
        if s.turn() > 0 && s.turn().is_multiple_of(GUIDE_BLINK_TURNS) {
            return needle(s);
        }
        s.step(Input::Wait);
    }
    unreachable!("the compass comes due at least once every {GUIDE_BLINK_TURNS} turns")
}

// ---------------------------------------------------------------------------
// It pulses
// ---------------------------------------------------------------------------

/// §8.3/#505: the compass is a **pulse**, not a standing line — lit on one turn in
/// [`GUIDE_BLINK_TURNS`] and dark on the rest, **turn zero included**.
///
/// That is the ability's main balance lever, and both halves of it matter. A standing
/// bearing is a line you simply follow, and §11.5a's fog stops being something you plan
/// around; a pulse gives you a *fix* you then walk on your own memory of. And the run
/// opens dark, so the first thing the ability asks is that you spend a few turns before
/// it answers — a compass already pointing on the frame you arrive would make the
/// opening move free.
#[test]
fn the_compass_pulses_and_the_run_opens_dark() {
    assert_eq!(GUIDE_BLINK_TURNS, 3, "the [START] period is pinned");
    let mut s = navigator(Cell::new(20, 20), vec![Cell::new(30, 20)]);

    assert_eq!(s.turn(), 0);
    assert_eq!(needle(&s), None, "the run opens with no fix");
    for turn in 1..=(GUIDE_BLINK_TURNS * 3) {
        s.step(Input::Wait);
        assert_eq!(s.turn(), turn, "one wait, one turn");
        assert_eq!(
            needle(&s).is_some(),
            turn.is_multiple_of(GUIDE_BLINK_TURNS),
            "turn {turn}: lit on one turn in {GUIDE_BLINK_TURNS} and dark on the rest",
        );
    }
}

// ---------------------------------------------------------------------------
// A bearing, and only a bearing
// ---------------------------------------------------------------------------

/// §8.2/#505: **exactly one** of the eight neighbours is washed while an unclaimed
/// objective exists — and it is a *neighbour*, never the objective's own cell.
#[test]
fn one_neighbouring_cell_is_washed_while_something_is_left_to_take() {
    let player = Cell::new(20, 20);
    let mut s = navigator(player, vec![Cell::new(30, 20)]);

    let cell = fix(&mut s).expect("something is left to take");
    assert_eq!(
        cell,
        Cell::new(21, 20),
        "due east of the player, one cell out",
    );
    assert_eq!(
        player.sight_distance(cell),
        1,
        "a compass points at a neighbour, never at the thing itself",
    );
}

/// §8.2/#505: **nothing left unclaimed → nothing painted.** It goes dark rather than
/// pointing home, and the absence is itself information: there is nothing left in this
/// building. Asserted across the claim, which is the moment the set empties.
#[test]
fn it_goes_dark_when_nothing_is_left() {
    // Facing the console, so the first step is the bump that takes it.
    let mut s = State::new(
        open_room(40, 40),
        Cell::new(20, 20),
        Direction::East,
        Vec::new(),
        vec![Cell::new(21, 20)],
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Guide));
    assert!(fix(&mut s).is_some(), "precondition: one console still out");

    s.step(Input::Step(Direction::East));
    assert_eq!(s.objectives_remaining(), 0, "the console is taken");
    assert_eq!(
        fix(&mut s),
        None,
        "and the compass is dark on the next fix it is due — for good",
    );
}

/// §8.2: the bearing moves to the **next** nearest as objectives are claimed — the
/// candidate set is live, not a level-start snapshot.
#[test]
fn claiming_one_moves_the_bearing_to_the_next() {
    let mut s = State::new(
        open_room(40, 40),
        Cell::new(20, 20),
        Direction::East,
        Vec::new(),
        vec![Cell::new(21, 20), Cell::new(20, 4)],
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Guide));
    assert_eq!(
        fix(&mut s),
        Some(Cell::new(21, 20)),
        "east, to the near one"
    );

    s.step(Input::Step(Direction::East)); // bump it
    assert_eq!(
        fix(&mut s),
        Some(Cell::new(20, 19)),
        "north, to the one that is left",
    );
}

/// §8.2/#264: held is on, and it is on for a **held** ability only. A run without the
/// Guide paints nothing, however many consoles are out.
#[test]
fn a_run_that_does_not_hold_it_sees_nothing() {
    let mut s = State::new(
        open_room(40, 40),
        Cell::new(20, 20),
        Direction::East,
        Vec::new(),
        vec![Cell::new(30, 20)],
        Cell::new(38, 38),
    );
    assert_eq!(fix(&mut s), None, "no ability, no compass");
}

// ---------------------------------------------------------------------------
// Straight-line and wall-blind — the specification, not the bug
// ---------------------------------------------------------------------------

/// **The rule that will be re-reported as a defect** (§8.3/#505): the bearing is taken
/// as the crow flies, so with a wall between the player and a console it points *at the
/// wall* — not at the doorway that is the only way there.
///
/// §7.3's "nearest means the shortest walk" is control routing a guard; this is a
/// needle. A guide that pathed would answer §10's exploration outright.
#[test]
fn the_bearing_points_through_a_wall_not_round_it() {
    let mut layout = open_room(40, 40);
    // A wall column at x = 25, with its one gap at the very bottom — so the *walk* to
    // the console runs south, and the *line* to it runs due east.
    for y in 1..38 {
        layout.place(Cell::new(25, y), Terrain::Wall);
    }
    let console = Cell::new(30, 20);
    let mut s = State::new(
        layout,
        Cell::new(20, 20),
        Direction::East,
        Vec::new(),
        vec![console],
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Guide));

    assert_eq!(
        fix(&mut s),
        Some(Cell::new(21, 20)),
        "due east, straight into the wall — the line, never the road",
    );
}

/// §8.3: a diagonal bearing is kept as a **diagonal**, even though movement is cardinal.
/// It is a compass needle, not a suggested move, and rounding it to the nearest cardinal
/// would throw away half the information. Walked over the eight octants, so the sector
/// split is asserted as a whole rather than at one convenient angle.
#[test]
fn the_needle_uses_all_eight_cells() {
    let player = Cell::new(20, 20);
    // (console offset, expected neighbour offset) — one target per octant, each
    // comfortably inside its own 45° sector.
    for (dx, dy, nx, ny) in [
        (10, 0, 1, 0),    // due east
        (10, 10, 1, 1),   // south-east
        (0, 10, 0, 1),    // due south
        (-10, 10, -1, 1), // south-west
        (-10, 0, -1, 0),  // due west
        (-10, -10, -1, -1),
        (0, -10, 0, -1),
        (10, -10, 1, -1),
    ] {
        let console = Cell::new((player.x as i32 + dx) as u32, (player.y as i32 + dy) as u32);
        let mut s = navigator(player, vec![console]);
        let want = Cell::new((player.x as i32 + nx) as u32, (player.y as i32 + ny) as u32);
        assert_eq!(fix(&mut s), Some(want), "bearing to {console:?}");
    }
}

/// The sector boundary itself (§8.3/#505): the axis sectors are 45° wide, so a target
/// that is *nearly* on the axis reads as the axis and one a little further off reads as
/// the diagonal. `tan 22.5° = √2 − 1 ≈ 0.414`, so at twelve cells east the switch falls
/// between four and five cells of south.
#[test]
fn the_octant_boundary_is_where_the_geometry_says() {
    let player = Cell::new(20, 20);
    let bearing = |dy: u32| fix(&mut navigator(player, vec![Cell::new(32, 20 + dy)]));

    // 4/12 = 0.333 < 0.414 — inside the eastward sector.
    assert_eq!(bearing(4), Some(Cell::new(21, 20)), "still due east");
    // 5/12 = 0.417 > 0.414 — over the line, and the needle turns.
    assert_eq!(bearing(5), Some(Cell::new(21, 21)), "now south-east");
}

// ---------------------------------------------------------------------------
// It reveals nothing else (§11.5a [SETTLED])
// ---------------------------------------------------------------------------

/// §11.5a: holding the Guide changes **the one cell and nothing else**. The objective
/// stays fogged, unremembered and undrawn; tile memory is untouched; every other cell of
/// the frame is byte-identical to the run without it.
///
/// This is what leaves #215's v3 intel sink — POI reveal, sold for currency — something
/// to sell. If this ability ever grows into a reveal, that ticket needs revisiting in
/// the same PR.
#[test]
fn it_reveals_nothing_but_the_one_cell() {
    let player = Cell::new(20, 20);
    let console = Cell::new(30, 20);
    let mut with = navigator(player, vec![console]);
    let mut without = State::new(
        open_room(40, 40),
        player,
        Direction::East,
        Vec::new(),
        vec![console],
        Cell::new(38, 38),
    );
    // Both walked to the same turn, so the comparison is frame against frame — and to
    // one the compass is **due** on, which is the only turn it could reveal anything.
    let needle = fix(&mut with).expect("a bearing is up");
    while without.turn() < with.turn() {
        without.step(Input::Wait);
    }

    assert_eq!(
        with.memory().contains(console),
        without.memory().contains(console),
        "the compass remembers nothing for you (§11.5a)",
    );

    let (a, b) = (crate::render(&with), crate::render(&without));
    for y in 0..a.height() {
        for x in 0..a.width() {
            let cell = Cell::new(x, y);
            if cell == needle {
                continue;
            }
            assert_eq!(
                a.get(x, y),
                b.get(x, y),
                "{cell:?} differs, and only the bearing cell may",
            );
        }
    }
    assert_ne!(
        a.get(needle.x, needle.y),
        b.get(needle.x, needle.y),
        "…and it does differ, or this test is asserting nothing",
    );
}

/// §7.3/#505: the **comms console is not a candidate**. §7.3 is explicit that "the cost
/// is the route, not the switch" and that its placement distance is a balance knob the
/// sim sweeps (#448); a passive that pointed at it would hand the counterplay over for
/// the price of a slot. It is also not an objective — you never have to take it.
///
/// Asserted so it cannot become one later by someone widening the predicate.
#[test]
fn the_comms_console_is_not_an_objective() {
    let mut layout = open_room(40, 40);
    // Right next to the player, where it would dominate any bearing if it counted.
    layout.place(Cell::new(21, 20), Terrain::CommsConsole);
    let mut s = State::new(
        layout,
        Cell::new(20, 20),
        Direction::East,
        Vec::new(),
        vec![Cell::new(20, 4)],
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Guide));

    assert_eq!(
        s.comms_console(),
        Some(Cell::new(21, 20)),
        "precondition: the comms console is one cell east",
    );
    assert_eq!(
        fix(&mut s),
        Some(Cell::new(20, 19)),
        "north, to the intel — the radio terminal is not a thing you go and take",
    );
}

// ---------------------------------------------------------------------------
// The weakest cue on the board (§11.5 [SETTLED])
// ---------------------------------------------------------------------------

/// §11.5: the wash sits at the **bottom** of the precedence stack, so a guard's cone
/// paints over it. The compass is a convenience, and it must never sit on top of the
/// thing that can kill you — which is exactly what would happen otherwise, since the
/// bearing cell is one step from the player, right where the eye lives.
#[test]
fn a_threat_cue_paints_over_the_bearing() {
    let player = Cell::new(20, 20);
    let bearing = Cell::new(21, 20);
    // A guard the player can see, north of the bearing cell and facing south (§7.1's
    // spawn facing), so its cone covers that cell.
    let mut s = State::new(
        open_room(40, 40),
        player,
        Direction::East,
        vec![Guard::stationary(Cell::new(21, 14))],
        vec![Cell::new(30, 20)],
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Guide));

    while s.turn() == 0 || !s.turn().is_multiple_of(GUIDE_BLINK_TURNS) {
        s.step(Input::Wait);
    }
    assert!(
        s.guide_bearing() == Some(bearing),
        "precondition: the compass is due, and wants that cell",
    );
    assert!(
        s.visible_cone_cells().any(|c| c == bearing),
        "precondition: and a cone the player can see covers it",
    );
    assert_eq!(
        crate::render(&s).get(bearing.x, bearing.y).bg,
        Some(Category::Danger),
        "red wins: the detection set is never hidden by an advisory layer",
    );
}

// ---------------------------------------------------------------------------
// Determinism (§12.4)
// ---------------------------------------------------------------------------

/// §12.4: two equidistant objectives resolve by a **fixed** rule — the level's own
/// ordering — and never by a draw. A compass that flickered between two answers on a
/// replay would be a desync, so the tie is settled by the candidate list's order and is
/// identical every time it is asked.
#[test]
fn a_tie_resolves_by_the_level_s_own_ordering() {
    let player = Cell::new(20, 20);
    let (first, second) = (Cell::new(20, 10), Cell::new(30, 20));
    assert_eq!(
        player.manhattan_distance(first),
        player.manhattan_distance(second),
        "precondition: the two are the same distance away",
    );

    let mut s = navigator(player, vec![first, second]);
    let answer = fix(&mut s);
    assert_eq!(answer, Some(Cell::new(20, 19)), "the earlier console wins");
    for _ in 0..8 {
        assert_eq!(needle(&s), answer, "and it wins every time it is asked");
    }

    // Swap the placement order and the tie goes the other way — which is the proof that
    // the ordering *is* the rule, rather than a coincidence of the geometry.
    let mut swapped = navigator(player, vec![second, first]);
    assert_eq!(fix(&mut swapped), Some(Cell::new(21, 20)));
}
