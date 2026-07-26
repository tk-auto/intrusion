//! The read surface (§9.2/§11.4/§11.5).
//!
//! What a viewer may know, asserted against [`State`]'s query methods: how a guard
//! classifies as seen, sensed or neither (§9.2), what the danger overlay paints and
//! what it spares, how far the senses reach, what concealment hides, and the usable
//! line's affordances mirroring exactly what the bump would do (§11.4) — the
//! guarantee that the offer can never drift from the action.

use crate::state::*;
use crate::test_support::{open_room, solo};

/// The #141 report, pinned: a crouched player must not be seen by a guard
/// whose sight line crosses *any* table of the bench they are behind. The
/// old single-table quarter-plane let a viewer oblique to the anchor look
/// straight past it — through the bench's other tables — and see the
/// player. The run is the cover now; the flank past its end stays open.
#[test]
fn a_bench_conceals_across_its_whole_run() {
    let mut layout = open_room(12, 12);
    for y in 3..=5 {
        layout.place(Cell::new(5, y), Terrain::PartialCover);
    }
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Step(Direction::East)); // crouch, anchored mid-bench
    assert!(s.crouched());

    // Oblique viewers the anchor's own quarter-plane never covered, but
    // whose line to the player crosses the bench's outer tables: concealed.
    assert!(s.concealed_from(Cell::new(6, 7)), "across the south table");
    assert!(s.concealed_from(Cell::new(6, 1)), "across the north table");
    // No table on the line: still seen — the bench is cover, not a cloak.
    assert!(!s.concealed_from(Cell::new(4, 1)), "past the bench's end");
    assert!(!s.concealed_from(Cell::new(1, 4)), "behind the player");
}

/// §10.3: crouch concealment is **directional** — cover blinds only the
/// viewers whose sight line crosses it. Behind a lone table that is the
/// quarter-plane it faces: a viewer across the cover (straight or leaning
/// up to the 45° graze) is blinded; a viewer on the flank or behind the
/// player is not; and without the crouch the same table conceals nothing.
#[test]
fn crouch_conceals_only_across_the_cover() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::PartialCover); // east of the player
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );

    // Standing, the table conceals from no one.
    assert!(!s.concealed_from(Cell::new(7, 4)));

    s.step(Input::Step(Direction::East)); // bump the table: crouch
    assert!(s.crouched());
    // Straight across the table, near and far: concealed.
    assert!(s.concealed_from(Cell::new(6, 4)));
    assert!(s.concealed_from(Cell::new(9, 4)));
    // Leaning, within the quarter-plane (along ≥ across): concealed —
    // including the exact 45° diagonal, which grazes the table's corner.
    assert!(s.concealed_from(Cell::new(6, 3)));
    assert!(s.concealed_from(Cell::new(6, 2)));
    // Past the diagonal — the flank, the perpendicular, and behind: seen.
    assert!(!s.concealed_from(Cell::new(5, 2)));
    assert!(!s.concealed_from(Cell::new(4, 2)));
    assert!(!s.concealed_from(Cell::new(2, 4)));
}

/// §10.3: the cupboard is the stronger hide — omnidirectional. A hidden
/// player is concealed from every direction, cover or none.
#[test]
fn a_hidden_player_is_concealed_from_every_direction() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(4, 4), Terrain::Hideout);
    let s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(s.hidden());
    for viewer in [
        Cell::new(4, 1),
        Cell::new(7, 4),
        Cell::new(4, 7),
        Cell::new(1, 4),
        Cell::new(6, 6),
    ] {
        assert!(
            s.concealed_from(viewer),
            "hidden must conceal from {viewer:?}"
        );
    }
}

/// Events speak the same §11.2 table as the glyphs, so the message ticket can
/// colour its bar without inventing meanings: taking intel is Interest, the
/// capture is Danger, a step is routine Neutral.
#[test]
fn events_declare_their_message_category() {
    use crate::category::Category;
    let at = Cell::new(2, 3);
    assert_eq!(Event::Moved { to: at }.category(), Category::Neutral);
    assert_eq!(Event::Bumped { into: at }.category(), Category::Neutral);
    assert_eq!(Event::EnteredHideout { at }.category(), Category::Owned);
    assert_eq!(Event::Crouched { behind: at }.category(), Category::Owned);
    assert_eq!(
        Event::DoorOpened {
            at,
            by_player: true,
        }
        .category(),
        Category::System
    );
    assert_eq!(
        Event::DoorClosed {
            at,
            by_player: false,
        }
        .category(),
        Category::System
    );
    assert_eq!(
        Event::IntelTaken { remaining: 1 }.category(),
        Category::Interest
    );
    assert_eq!(Event::ExitRefused.category(), Category::Interest);
    assert_eq!(Event::Won.category(), Category::Interest);
    assert_eq!(Event::Captured { by: at }.category(), Category::Danger);
    assert_eq!(Event::TakenDown { at }.category(), Category::Owned);
    assert_eq!(Event::BodyFound { at }.category(), Category::Warning);
}

/// The usable line's contract (§11.4): [`State::affordances`] offers exactly
/// what a bump would do. A live console reads `TakeIntel`; once taken it is
/// just solid and offers nothing; the exit answers by whether the intel is
/// in hand; an empty cupboard offers `Hide`.
#[test]
fn affordances_mirror_what_a_bump_would_do() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(4, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        [Cell::new(6, 5)], // a console east
        Cell::new(5, 4),   // the exit north
    );

    // Console east, exit north (intel still out), cupboard west — each with
    // the direction to bump it.
    assert_eq!(
        s.affordances(),
        vec![
            (Direction::North, Affordance::ExitRefused),
            (Direction::East, Affordance::TakeIntel),
            (Direction::West, Affordance::Hide)
        ],
        "Direction::ALL order: north, east, … west"
    );

    // Take the intel: the console goes solid and the exit opens up.
    s.step(Input::Step(Direction::East));
    assert_eq!(
        s.affordances(),
        vec![
            (Direction::North, Affordance::Leave),
            (Direction::West, Affordance::Hide)
        ],
        "a spent console offers nothing; the exit now offers the win"
    );

    // In the middle of open floor, the line is empty.
    let s = solo(Cell::new(4, 4));
    assert_eq!(s.affordances(), Vec::new());
}

/// An adjacent **aware** guard offers nothing: its bump is a free no-op
/// (§7.2's gate — the unaware case is the takedown test above), and the
/// usable line must never promise what a bump will not do (§2.3). An
/// occupied cupboard is likewise just solid.
#[test]
fn affordances_skip_guards_and_occupied_hideouts() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(6, 5))], // east of the player
        Vec::new(),
        Cell::new(10, 10),
    );
    // Enter the cupboard north; the guard east never shows.
    assert_eq!(s.affordances(), vec![(Direction::North, Affordance::Hide)]);
    s.step(Input::Step(Direction::North));
    assert!(s.hidden());

    // From inside, the cupboard's own cell is the player's — and stepping
    // back out is a plain move, not an affordance.
    assert_eq!(s.affordances(), Vec::new());
}

/// §9.2 classification: from the player's position and facing, each guard is
/// **Seen** (in the FOV), **Sensed** (within the guard-sense box but out of FOV),
/// or neither (out of range → `None`). A guard in view is Seen even though it also
/// sits inside the box — Seen wins, so the dot never doubles the full guard.
#[test]
fn guards_classify_as_seen_sensed_or_neither() {
    let seen = Cell::new(20, 16); // 4 north: in the forward half-disc
    let sensed = Cell::new(20, 25); // 5 south: behind the player, inside the box
    let gone = Cell::new(20, 33); // 13 south: behind and past the 10-box
    let s = State::new(
        open_room(40, 40),
        Cell::new(20, 20),
        Direction::North,
        vec![
            Guard::stationary(seen),
            Guard::stationary(sensed),
            Guard::stationary(gone),
        ],
        Vec::new(),
        Cell::new(38, 38),
    );

    assert!(
        s.player_fov().contains(seen),
        "precondition: seen guard in FOV"
    );
    assert!(
        !s.player_fov().contains(sensed),
        "precondition: sensed guard out of FOV"
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Seen)
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[1]),
        Some(GuardPerception::Sensed),
        "in the box but out of view → position only",
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[2]),
        None,
        "past the box → nothing"
    );
}

/// §11.5/#223: [`visible_cone_cells`] is *exactly* a seen guard's cone (no
/// concealment here), and [`in_visible_danger`] mirrors membership — a cell in the
/// cone is danger, a cell outside it is not. This is the set the shell's
/// held-movement guard reads, the same one the renderer paints.
#[test]
fn the_visible_cone_set_matches_a_seen_guards_cone() {
    // Player at (10,10) facing north; guard adjacent at (9,9), in the FOV, looking
    // south so its wedge covers the player — the same scene the overlay golden uses.
    let s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(Cell::new(9, 9))],
        Vec::new(),
        Cell::new(18, 18),
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Seen),
        "precondition: the guard is in view",
    );
    let cone: std::collections::HashSet<Cell> = s.guards()[0].fov().cells().collect();
    let visible: std::collections::HashSet<Cell> = s.visible_cone_cells().collect();
    assert_eq!(
        visible, cone,
        "the visible-danger set is exactly the seen cone"
    );

    let watched = Cell::new(9, 11); // straight down the wedge
    assert!(cone.contains(&watched));
    assert!(s.in_visible_danger(watched), "a watched cell is danger");
    let clear = Cell::new(0, 0); // a corner the cone never reaches
    assert!(!cone.contains(&clear));
    assert!(
        !s.in_visible_danger(clear),
        "a cell outside the cone is not danger"
    );
}

/// §11.5 [SETTLED] held through the new query: an **unseen** guard's cone is unknown
/// information, so it is not visible danger — [`visible_cone_cells`] is empty and
/// [`in_visible_danger`] is false over the guard's own (out-of-view) cone.
#[test]
fn an_unseen_guards_cone_is_not_visible_danger() {
    // Guard behind the north-facing player at (10,14), looking south (spawn) — away
    // from the player, so it never detects, and unseen, so its cone paints nothing.
    let guard = Cell::new(10, 14);
    let s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(guard)],
        Vec::new(),
        Cell::new(18, 18),
    );
    assert!(
        !s.player_fov().contains(guard),
        "precondition: the guard is unseen"
    );
    assert_eq!(
        s.visible_cone_cells().count(),
        0,
        "an unseen guard contributes no visible cone",
    );
    assert_eq!(s.spot_flash().count(), 0, "it faces away — no fresh spot");
    // A cell the guard's cone genuinely covers (south of it, out of the FOV) is not
    // danger: knowledge the player does not have.
    let in_cone = Cell::new(10, 16);
    assert!(
        s.guards()[0].fov().contains(in_cone),
        "the cell is in the cone"
    );
    assert!(
        !s.player_fov().contains(in_cone),
        "but out of the player's FOV"
    );
    assert!(
        !s.in_visible_danger(in_cone),
        "an unseen cone is not visible danger"
    );
}

/// #223 with #250: a guard the player cannot see that **freshly** detects them
/// contributes no cone (it is unseen), but its momentary spot-flash sightline *is*
/// visible danger — the "you just got spotted, don't blindly march on" case. The
/// shell's held-movement guard reads it through the same query.
#[test]
fn a_fresh_spot_from_an_unseen_guard_is_visible_danger() {
    // Guard at (10,5) facing south; player five south at (10,10) facing south too —
    // the guard is directly behind, unseen, but its cone runs down over the player,
    // so at level start it freshly detects a player it is unseen by (§9.2).
    let s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::South,
        vec![Guard::stationary(Cell::new(10, 5))],
        Vec::new(),
        Cell::new(18, 18),
    );
    assert!(
        !s.player_fov().contains(Cell::new(10, 5)),
        "precondition: the spotter is behind the player, unseen",
    );
    assert_eq!(
        s.visible_cone_cells().count(),
        0,
        "the unseen cone paints no danger on its own",
    );
    // The fresh spot-flash sightline (10,6)..=(10,10) is danger the player can act on.
    for y in 6..=10 {
        assert!(
            s.in_visible_danger(Cell::new(10, y)),
            "the spot sightline at (10,{y}) is visible danger",
        );
    }
    assert!(
        !s.in_visible_danger(Cell::new(12, 8)),
        "off the sightline stays clear — a line, not the whole cone",
    );
}

/// §9.1's headline: the sense **passes through walls** — it is not line of sight.
/// A guard sealed behind a wall, with no line to the player but inside the box, is
/// **Sensed** (position only), not hidden. A walled-off fixture pins this.
#[test]
fn the_sense_passes_through_walls() {
    let mut layout = open_room(20, 20);
    // Wall the whole row y=8 across the interior, sealing the north strip from the
    // player's line of sight.
    for x in 1..=18 {
        layout.place(Cell::new(x, 8), Terrain::Wall);
    }
    let guard = Cell::new(10, 6); // 4 north of the player, behind the wall
    let s = State::new(
        layout,
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(guard)],
        Vec::new(),
        Cell::new(18, 18),
    );

    assert!(
        !s.player_fov().contains(guard),
        "precondition: the wall blocks line of sight to the guard",
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Sensed),
        "no line of sight but inside the box → sensed through the wall",
    );
}

/// §9.1 **[START]**: the sense box is **10**, widening to **20** on a turn the
/// player spent waiting. Both are pinned so a later change is visible. A walled-off
/// guard 11 cells away — just outside the box, no line of sight — is *not* sensed;
/// the same guard becomes Sensed the turn the player waits (10 → 20).
#[test]
fn the_sense_range_is_ten_and_twenty_on_wait() {
    assert_eq!(PLAYER_SENSE_RANGE, 10, "the [START] sense range");
    assert_eq!(
        PLAYER_SENSE_RANGE_WAITING, 20,
        "the [START] wait sense range"
    );

    let mut layout = open_room(40, 40);
    // A full wall row seals the guard from sight, so it can only ever be *sensed*,
    // never seen — even under the 360° look a wait grants (§8.3).
    for x in 1..=38 {
        layout.place(Cell::new(x, 12), Terrain::Wall);
    }
    let guard = Cell::new(20, 9); // 11 north of the player: just past the 10-box
    let mut s = State::new(
        layout,
        Cell::new(20, 20),
        Direction::North,
        vec![Guard::stationary(guard)],
        Vec::new(),
        Cell::new(38, 38),
    );

    assert_eq!(s.sense_range(), 10, "no wait yet: the base box");
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        None,
        "11 cells away is just outside the 10-box",
    );

    s.step(Input::Wait);
    assert_eq!(s.sense_range(), 20, "waiting widens the box");
    assert!(
        !s.player_fov().contains(guard),
        "still walled off from sight"
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Sensed),
        "the wait pulls the guard into the widened box → sensed",
    );
}

// --- Sensing doors (§9.4/§10.4) -----------------------------------------------
