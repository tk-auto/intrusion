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

/// The two crouch **previews** (§10.3/#379) answer for a stance not yet taken, and
/// they must answer what taking it will actually give: `crouch_would_conceal` is
/// checked against `concealed_from` after ducking for real, and `crouch_holds`
/// against whether a step keeps the pose.
///
/// They exist for the §13.2 sim bot, which has to decide whether a table is worth a
/// turn *before* spending it and must ask core rather than re-derive §10.3's
/// half-plane. A preview that could disagree with the pose would be worse than no
/// preview at all — the bot would plan against a rule the game does not run.
#[test]
fn the_crouch_previews_agree_with_the_pose_they_predict() {
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
    let table = Cell::new(5, 4);
    let stance = Cell::new(4, 4);
    // Asked while still standing — the whole point of a preview.
    assert!(!s.crouched());
    let viewers = [
        Cell::new(6, 4), // across the bench
        Cell::new(6, 7), // oblique, across its south table
        Cell::new(4, 1), // past its end, on the player's own side
        Cell::new(1, 4), // behind the player
    ];
    let predicted: Vec<bool> = viewers
        .iter()
        .map(|&v| s.crouch_would_conceal(table, stance, v))
        .collect();
    assert_eq!(
        predicted,
        vec![true, true, false, false],
        "the preview must read §10.3's own geometry",
    );

    // Now take the pose and check the game agrees, viewer for viewer.
    s.step(Input::Step(Direction::East));
    assert!(s.crouched());
    for (&viewer, &was_predicted) in viewers.iter().zip(&predicted) {
        assert_eq!(
            s.concealed_from(viewer),
            was_predicted,
            "{viewer:?}: the preview promised something the pose does not give",
        );
    }

    // The crouch-walk half: the preview says which steps keep the pose, and the
    // turn loop then keeps it for exactly those. South is along the bench (still
    // hugging), west is a cell of air away from it.
    assert!(s.crouch_holds(table, Cell::new(4, 5)), "along the bench");
    assert!(!s.crouch_holds(table, Cell::new(3, 4)), "off the furniture");
    s.step(Input::Step(Direction::South));
    assert!(s.crouched(), "a crouch-walk holds the pose (§10.3)");
    assert_eq!(s.player(), Cell::new(4, 5));
    s.step(Input::Step(Direction::West));
    assert!(!s.crouched(), "and stepping off the bench stands you up");
}

/// A table the player is nowhere near hides nobody, and a cell that is not partial
/// cover at all names no run — so a stale or mistaken anchor previews `false`
/// rather than panicking or inventing cover (§10.3).
#[test]
fn a_preview_against_a_cell_that_is_not_cover_conceals_nothing() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::PartialCover);
    let s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    let floor = Cell::new(8, 8);
    assert!(!s.crouch_would_conceal(floor, Cell::new(7, 8), Cell::new(9, 8)));
    assert!(!s.crouch_holds(floor, Cell::new(8, 7)));
    // …while the real table still previews as cover, so the check is not vacuous.
    assert!(s.crouch_would_conceal(Cell::new(5, 4), Cell::new(4, 4), Cell::new(6, 4)));
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
        Event::IntelTaken {
            remaining: 1,
            still_needed: 0
        }
        .category(),
        Category::Interest
    );
    assert_eq!(
        Event::ExitRefused { still_needed: 1 }.category(),
        Category::Interest
    );
    assert_eq!(Event::Won.category(), Category::Interest);
    assert_eq!(Event::Captured { by: at }.category(), Category::Danger);
    assert_eq!(Event::TakenDown { at }.category(), Category::Owned);
    assert_eq!(Event::BodyFound { at }.category(), Category::Warning);
    // A refused toggle-off is one of your own tools answering back (§8/#304), so it
    // reads in the Owned band beside the activation it declined to undo.
    assert_eq!(Event::RematerializeRefused.category(), Category::Owned);
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

// --- The Vision passive (§5/§6.1, §8.3/#265) ----------------------------------

/// The passive's two `[START]` numbers, pinned (#265): the arc it lifts sight to is
/// the **full 360°** (§6.2 width 5) and the range box is **20**, up from §5's 15.
/// A later retune has to edit this test, which is the point.
#[test]
fn the_vision_passive_pins_its_start_arc_and_range() {
    assert_eq!(FULL_SIGHT_ARC, 5, "the [START] 360° arc width");
    assert_eq!(ENHANCED_SIGHT_RANGE, 20, "the [START] enhanced range");
    const {
        assert!(
            ENHANCED_SIGHT_RANGE > PLAYER_SIGHT_RANGE,
            "the passive must actually extend reach",
        )
    };
}

/// Holding the passive **widens the standing arc to 360°** (#265): a guard directly
/// *behind* the player — which §5's forward half-disc can never show, and which is
/// the whole reason Wait exists (§9.1) — is seen without spending a turn.
///
/// The two loadouts differ in nothing but the passive, so the widening is the
/// ability's and not the geometry's.
#[test]
fn holding_vision_sees_behind_without_waiting() {
    let behind = Cell::new(20, 24); // 4 south of a north-facing player
    let build = |loadout| {
        let mut s = State::new(
            open_room(40, 40),
            Cell::new(20, 20),
            Direction::North,
            vec![Guard::stationary(behind)],
            Vec::new(),
            Cell::new(38, 38),
        )
        .with_loadout(loadout);
        // One spent turn runs the sight phase (§4.2) from the real loadout.
        s.step(Input::Step(Direction::North));
        s
    };

    let bare = build(Loadout::innate());
    assert!(
        !bare.player_fov().contains(behind),
        "precondition: §5's half-disc cannot see behind",
    );

    let enhanced = build(Loadout::innate().with(AbilityId::Vision));
    assert!(
        enhanced.player_fov().contains(behind),
        "the passive makes 360° the standing arc (§8.3/#265)",
    );
    assert_eq!(
        enhanced.ability_state(AbilityId::Vision),
        AbilityState::Passive,
        "and it says so without ever being activated",
    );
}

/// Holding the passive **extends the range box** (#265): a cell 18 north — inside
/// the enhanced 20-box, outside §5's 15 — is lit only for the holder. Nothing
/// occludes, so the box is the only thing that can decide it.
#[test]
fn holding_vision_lights_cells_past_the_base_range() {
    let far = Cell::new(20, 20 - 18);
    let build = |loadout| {
        let mut s = State::new(
            open_room(40, 40),
            Cell::new(20, 20),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 38),
        )
        .with_loadout(loadout);
        s.step(Input::Wait);
        s
    };

    assert!(
        !build(Loadout::innate()).player_fov().contains(far),
        "precondition: 18 cells is outside the §5 15-box",
    );
    assert!(
        build(Loadout::innate().with(AbilityId::Vision))
            .player_fov()
            .contains(far),
        "the passive's 20-box reaches it",
    );
}

/// **No double-widening** (#265): the passive and Wait (§9.1) both reach for the
/// same 360°, so combining them changes nothing about *sight* — the wait's own gift
/// is the widened guard **sense**, a separate channel the passive deliberately does
/// not touch (§9). Pinned as set equality on the FOV, both ways round, so a future
/// stacking bug cannot hide in a cell or two.
#[test]
fn waiting_while_holding_vision_neither_widens_nor_narrows_sight() {
    let build = |wait: bool| {
        let mut s = State::new(
            open_room(40, 40),
            Cell::new(20, 20),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 38),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Vision));
        if wait {
            s.step(Input::Wait);
        } else {
            s.step(Input::Step(Direction::North));
        }
        s
    };

    let moved = build(false);
    let waited = build(true);
    // The player stands on a different cell after a step, so compare the shape of
    // the sight rather than the raw sets: the *arc* is what must not stack.
    let seen_around = |s: &State| {
        let p = s.player();
        [
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ]
        .into_iter()
        .filter(|&d| {
            p.step(d)
                .and_then(|c| c.step(d))
                .and_then(|c| c.step(d))
                .is_some_and(|c| s.player_fov().contains(c))
        })
        .count()
    };
    assert_eq!(
        seen_around(&moved),
        4,
        "the passive alone already sees every way",
    );
    assert_eq!(
        seen_around(&waited),
        4,
        "waiting on top adds nothing to sight — there is nothing past 360°",
    );

    // The sense, by contrast, is untouched by the passive and still widened by the
    // wait: the two channels stay separate (§9/§9.1, #265's "vision only").
    assert_eq!(moved.sense_range(), PLAYER_SENSE_RANGE, "vision only");
    assert_eq!(waited.sense_range(), PLAYER_SENSE_RANGE_WAITING);
}

/// The passive changes **only the player's own sight**, never a guard's (#265): the
/// guard cone the danger overlay is painted from is the guard's own, so holding
/// Vision can show you a guard that cannot see you — the §9-spirit information
/// asymmetry — without ever making the overlay claim you are spotted.
#[test]
fn vision_widens_the_player_not_the_guards() {
    let guard = Cell::new(20, 24); // behind a north-facing player, facing away
    let mut s = State::new(
        open_room(40, 40),
        Cell::new(20, 20),
        Direction::North,
        vec![Guard::stationary(guard)],
        Vec::new(),
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Vision));
    s.step(Input::Step(Direction::North));

    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Seen),
        "the widened arc sees it",
    );
    assert!(
        !s.guards()[0].fov().contains(s.player()),
        "the guard's own cone is untouched — it does not see back",
    );
}

/// §4.4/#304: an ability's shortcut is a **toggle**, and [`State::ability_input`] is
/// the one place that is decided — an active ability switches off, everything else
/// switches on. This is what makes the free toggle-off reachable at all: before it,
/// every path resolved to `Activate` and a started sprint ran its full duration
/// whether the player wanted it or not.
#[test]
fn an_active_ability_resolves_to_the_toggle_off() {
    let mut s = solo(Cell::new(4, 4));
    assert_eq!(
        s.ability_input(AbilityId::Run),
        Input::Activate(AbilityId::Run),
        "ready: the key switches it on",
    );

    s.step(Input::Activate(AbilityId::Run));
    assert!(matches!(
        s.ability_state(AbilityId::Run),
        AbilityState::Active { .. }
    ));
    assert_eq!(
        s.ability_input(AbilityId::Run),
        Input::Deactivate(AbilityId::Run),
        "active: the same key switches it off",
    );

    s.step(Input::Deactivate(AbilityId::Run));
    assert!(matches!(
        s.ability_state(AbilityId::Run),
        AbilityState::Cooling { .. }
    ));
    assert_eq!(
        s.ability_input(AbilityId::Run),
        Input::Activate(AbilityId::Run),
        "cooling: back to the activation that refuses for free",
    );

    // An ability the run does not hold (#244) reads Unusable and resolves to the
    // activation its deck refuses — there is nothing on to switch off.
    assert_eq!(s.ability_state(AbilityId::Decoy), AbilityState::Unusable);
    assert_eq!(
        s.ability_input(AbilityId::Decoy),
        Input::Activate(AbilityId::Decoy),
    );
}

/// #264/#304: a **passive** is never a toggle. Holding it is the whole of its state,
/// so its slot's digit resolves to the activation that has always been a free no-op — the
/// `(on)` marker cannot be pressed off, and only dropping the ability ends it.
#[test]
fn a_passive_cannot_be_switched_off() {
    let mut s = solo(Cell::new(4, 4)).with_loadout(Loadout::innate().with(AbilityId::Vision));
    assert_eq!(s.ability_state(AbilityId::Vision), AbilityState::Passive);
    assert_eq!(
        s.ability_input(AbilityId::Vision),
        Input::Activate(AbilityId::Vision),
        "the passive's key never becomes a toggle-off",
    );

    let turn = s.turn();
    assert!(
        s.step(s.ability_input(AbilityId::Vision)).is_empty(),
        "pressing it says nothing",
    );
    assert_eq!(turn, s.turn(), "and is free, as it always was");
    assert_eq!(
        s.ability_state(AbilityId::Vision),
        AbilityState::Passive,
        "still on",
    );

    // The explicit toggle-off is refused too, whoever sends it (the sim, a replay).
    assert!(s.step(Input::Deactivate(AbilityId::Vision)).is_empty());
    assert_eq!(s.ability_state(AbilityId::Vision), AbilityState::Passive);
}

// --- The debug reveal (§12.6) ------------------------------------------------

/// The playtest reveal is expressed as *sight*, not as a drawing rule: the player's
/// field of view becomes the whole grid, so everything a view derives from it follows
/// on its own — the fog lifts, and every guard reads as **Seen** (§9.2) and therefore
/// paints its cone into the §11.5 danger overlay, wherever it stands. This is the
/// difference between "the picture has an overlay bolted on" and "you can see the
/// level".
#[test]
fn the_debug_reveal_makes_the_players_sight_the_whole_level() {
    use crate::DebugModifiers;
    // A guard far behind a north-facing player: out of the half-disc, out of the
    // guard-sense box, its own cone facing away.
    let guard = Cell::new(20, 34);
    let build = || {
        State::new(
            open_room(40, 40),
            Cell::new(20, 20),
            Direction::North,
            vec![Guard::stationary(guard)],
            Vec::new(),
            Cell::new(38, 38),
        )
    };
    let fogged = build();
    assert_eq!(fogged.perceive_guard(&fogged.guards()[0]), None);
    assert_eq!(
        fogged.visible_cone_cells().count(),
        0,
        "precondition: an unseen guard paints nothing",
    );

    let revealed = build().with_debug(DebugModifiers {
        reveal_whole_level: true,
    });
    let facility = revealed.layout().facility();
    for y in 0..facility.height() {
        for x in 0..facility.width() {
            assert!(
                revealed.player_fov().contains(Cell::new(x, y)),
                "({x},{y}) is outside the revealed sight",
            );
        }
    }
    assert_eq!(
        revealed.perceive_guard(&revealed.guards()[0]),
        Some(GuardPerception::Seen),
        "a guard anywhere is seen, so it draws its full self and its cone",
    );
    assert_eq!(
        revealed.visible_cone_cells().count(),
        revealed.guards()[0].fov().cells().count(),
        "the overlay paints exactly that guard's own cone — no more, no less",
    );
}

/// The reveal changes only what the **player perceives**. Guards look with their own
/// cones, so the facility plays the identical run: same poses, same guard states, same
/// detection, same outcome from the same inputs. Seeing everything is not being
/// everywhere — and, crucially, not being *seen* any differently.
#[test]
fn the_debug_reveal_leaves_the_guards_and_the_world_alone() {
    use crate::DebugModifiers;
    let build = || {
        State::new(
            open_room(20, 20),
            Cell::new(5, 5),
            Direction::South,
            vec![Guard::patrolling_to(Cell::new(12, 12), Cell::new(12, 16))],
            [Cell::new(9, 9)],
            Cell::new(18, 18),
        )
    };
    let mut fogged = build();
    let mut revealed = build().with_debug(DebugModifiers {
        reveal_whole_level: true,
    });
    for input in [
        Input::Step(Direction::South),
        Input::Step(Direction::East),
        Input::Wait,
        Input::Step(Direction::South),
        Input::Step(Direction::East),
    ] {
        fogged.step(input);
        revealed.step(input);
    }
    assert_eq!(fogged.player(), revealed.player());
    assert_eq!(fogged.turn(), revealed.turn());
    assert_eq!(fogged.outcome(), revealed.outcome());
    assert_eq!(fogged.alert(), revealed.alert());
    for (a, b) in fogged.guards().iter().zip(revealed.guards()) {
        assert_eq!(a.pos(), b.pos(), "a guard walks the same beat");
        assert_eq!(a.state(), b.state(), "…in the same state of mind");
        assert_eq!(
            a.fov().cells().count(),
            b.fov().cells().count(),
            "…looking with its own cone, not the player's",
        );
        assert_eq!(
            a.detected_player(),
            b.detected_player(),
            "…and detecting exactly what it would have",
        );
    }
}

/// #310: what is still **out** is not what is still **needed**. The tally counts
/// consoles; [`State::intel_needed_to_exit`] answers the run's gate (§4.5/#244), and
/// [`State::exit_ready`] is exactly that count at zero — the one fact every objective
/// message derives from, so none of them can promise an exit that would refuse.
#[test]
fn the_gate_says_how_much_intel_is_needed_not_how_much_is_out() {
    use crate::modifiers::{IntelGate, LevelModifiers};

    let facility = |gate: IntelGate| {
        State::new(
            open_room(12, 12),
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            [Cell::new(5, 4), Cell::new(6, 5), Cell::new(4, 5)],
            Cell::new(5, 6),
        )
        .with_modifiers(LevelModifiers {
            intel_to_exit: gate,
            ..LevelModifiers::default()
        })
    };

    // Three consoles out under every gate; how many of them are *required* differs.
    for (gate, needed) in [
        (IntelGate::None, 0),
        (IntelGate::AtLeastOne, 1),
        (IntelGate::All, 3),
    ] {
        let mut s = facility(gate);
        assert_eq!(s.objectives_remaining(), 3, "{gate:?}: three consoles out");
        assert_eq!(s.intel_needed_to_exit(), needed, "{gate:?}: what is needed");
        assert_eq!(
            s.exit_ready(),
            needed == 0,
            "{gate:?}: ready iff none needed"
        );
        assert_eq!(s.intel_in_hand(), 0);

        // Take one (bump north): the requirement drops by what the gate counts.
        s.step(Input::Step(Direction::North));
        assert_eq!(s.intel_in_hand(), 1);
        assert_eq!(s.objectives_remaining(), 2);
        assert_eq!(
            s.intel_needed_to_exit(),
            needed.saturating_sub(1).min(2),
            "{gate:?}: one take satisfies `AtLeastOne` and only chips at `All`",
        );
        assert_eq!(s.exit_ready(), s.intel_needed_to_exit() == 0);
    }

    // A facility with no consoles is vacuously satisfied under every gate.
    for gate in [IntelGate::None, IntelGate::AtLeastOne, IntelGate::All] {
        let s = State::new(
            open_room(12, 12),
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(5, 6),
        )
        .with_modifiers(LevelModifiers {
            intel_to_exit: gate,
            ..LevelModifiers::default()
        });
        assert_eq!(s.intel_needed_to_exit(), 0, "{gate:?}: nothing to gate on");
        assert!(s.exit_ready());
    }
}
