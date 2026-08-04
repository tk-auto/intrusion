//! The sense channel as one persist-and-fade system (§9/§9.4, #192).
//!
//! What a guard felt through a wall leaves behind — a live dot with a short fading
//! trail, and a ghost of the last cell when it slips out of the box — and the
//! restraint that keeps the trail from becoming a heading (§9.2). The **door** half of
//! the same channel is exercised in [`doors`](super::super::tests::doors), which owns
//! the range, the footprint and the fade a door change lights.

use crate::facility::{Facility, Terrain};
use crate::generate::Layout;
use crate::render::{render, Fill};
use crate::state::*;
use crate::test_support::open_room;
use crate::Category;

/// A `w × h` room cut in two by a solid wall along row `wall_y` — the fixture the whole
/// module is built on, because the sense is *through walls* and sight is not. With the
/// player north of the wall and a guard south of it, the guard is felt and never seen
/// however the player looks: even a Wait's 360° sweep (§5/§8.3) stops at the wall, so a
/// turn can be spent without the scene quietly turning into a *seen* guard.
fn split_room(w: u32, h: u32, wall_y: u32) -> Layout {
    let mut facility = Facility::walled_box(w, h);
    for x in 0..w {
        facility.set_terrain(x, wall_y, Terrain::Wall);
    }
    Layout::from_facility(facility)
}

/// The marks the sense currently carries at `cell`, oldest last — the renderer's own
/// view of the channel, narrowed to one cell.
fn ages_at(state: &State, cell: Cell) -> Vec<u32> {
    let mut ages: Vec<u32> = state
        .sense_marks()
        .filter(|mark| mark.cell == cell)
        .map(|mark| mark.age)
        .collect();
    ages.sort_unstable();
    ages
}

/// A player at (10,10), and a guard walking west→east along row 15 on the far side of
/// the wall: five cells away through solid brick, so it is inside the 10-box (§9.1) and
/// out of every line of sight — sensed the whole way, never seen.
fn a_guard_walking_behind_the_wall() -> State {
    let s = State::new(
        split_room(20, 20, 12),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(6, 15), Cell::new(14, 15))],
        Vec::new(),
        Cell::new(18, 10),
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Sensed),
        "precondition: the guard is felt through a wall, not seen",
    );
    s
}

/// §9.2/§9.4 **[START]**: the two lives of the one channel are pinned, and so is their
/// order. The guard trail is the **shorter** — it is re-stamped every turn its guard is
/// still felt, where a door change gets one chance to be read — and it is the number
/// that decides whether the trail says *was just here* or hands over a heading, so a
/// change to it must be a deliberate, visible edit.
#[test]
fn the_sense_channel_decays_are_pinned() {
    assert_eq!(GUARD_CUE_DECAY_TURNS, 2, "the [START] guard-trail decay");
    assert_eq!(DOOR_CUE_DECAY_TURNS, 3, "the [START] door-cue decay");
    // That the trail never outlives the discrete event beside it is pinned at compile
    // time next to the constants, so the pair cannot silently invert.
}

/// §9.2/#192: a sensed guard on the move leaves a **fading trail** — the cell it stands
/// in carries this turn's mark, the cell it just left carries an older one, and the mark
/// is gone `GUARD_CUE_DECAY_TURNS` turns after the guard was there.
#[test]
fn a_sensed_guard_leaves_a_fading_trail_behind_it() {
    let mut s = a_guard_walking_behind_the_wall();
    let first = s.guards()[0].pos();
    assert_eq!(ages_at(&s, first), vec![0], "the live cell is marked fresh");

    s.step(Input::Wait);
    let second = s.guards()[0].pos();
    assert_ne!(second, first, "precondition: the guard walked on");
    assert_eq!(ages_at(&s, second), vec![0], "the new cell is fresh");
    assert_eq!(ages_at(&s, first), vec![1], "the cell it left has aged");

    s.step(Input::Wait);
    assert_eq!(
        ages_at(&s, first),
        Vec::<u32>::new(),
        "the mark is gone GUARD_CUE_DECAY_TURNS turns after the guard was there",
    );
    assert_eq!(ages_at(&s, second), vec![1], "…and the next one has aged");
}

/// §9.2/#192: the trail is **short** on purpose. A guard walking in a straight line
/// leaves at most `GUARD_CUE_DECAY_TURNS` cells lit at once — the live one and the tail
/// behind it — which is "was just here", not a vector long enough to extrapolate. This
/// is the restraint the whole feature is bounded by: the sense gives position, never
/// intent (§9.2), and a long trail is an arrow.
#[test]
fn the_trail_is_too_short_to_read_as_a_heading() {
    let mut s = a_guard_walking_behind_the_wall();
    for _ in 0..6 {
        s.step(Input::Wait);
        let marks: Vec<Cell> = s.sense_marks().map(|mark| mark.cell).collect();
        assert!(
            marks.len() <= GUARD_CUE_DECAY_TURNS as usize,
            "one guard lights at most GUARD_CUE_DECAY_TURNS cells at once, got {marks:?}",
        );
    }
}

/// §9.2/#192: a guard **standing still** stamps the same cell over and over, so it
/// leaves no trail at all — one mark, always fresh. The watcher whose facing the player
/// would most like to know is exactly the one the channel says nothing extra about.
#[test]
fn a_standing_guard_leaves_no_trail() {
    let held = Cell::new(10, 15);
    let mut s = State::new(
        split_room(20, 20, 12),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(held)],
        Vec::new(),
        Cell::new(18, 10),
    );
    for _ in 0..4 {
        s.step(Input::Wait);
        let marks: Vec<SenseMark> = s.sense_marks().collect();
        assert_eq!(marks.len(), 1, "one cue, refreshed, never a second mark");
        assert_eq!(marks[0].cell, held);
        assert_eq!(marks[0].age, 0, "a standing guard's mark never ages");
    }
}

/// §9.1/§9.2/#192: when a guard leaves the sense box its last felt cell **lingers and
/// fades** instead of blinking out — the ghost half of the shared model. Driven by the
/// Wait that widens the sense to `PLAYER_SENSE_RANGE_WAITING` (§9.1) and then lapses:
/// the wide look finds a guard the walking box cannot reach, and what it found stays on
/// the board for a moment after the look is spent. That moment is the point — "I have
/// lost it, and it was just there" is a fact the player can act on.
#[test]
fn a_guard_that_leaves_the_box_leaves_a_fading_ghost() {
    let far = Cell::new(10, 10 + PLAYER_SENSE_RANGE + 2);
    let mut s = State::new(
        split_room(20, 40, 12),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(far)],
        Vec::new(),
        Cell::new(18, 10),
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        None,
        "precondition: the guard is beyond the walking sense box",
    );
    assert_eq!(ages_at(&s, far), Vec::<u32>::new(), "nothing felt yet");

    // The Wait widens the sense (§9.1) and the wide look stamps what it finds.
    s.step(Input::Wait);
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Sensed),
        "the widened box reaches it",
    );
    assert_eq!(ages_at(&s, far), vec![0], "the wide look leaves a mark");

    // Act, and the box lapses back to the walking one: the guard is out of reach, but
    // the cell it was felt in is still on the board, fading.
    s.step(Input::Step(Direction::North));
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        None,
        "the widened look is spent",
    );
    assert_eq!(ages_at(&s, far), vec![1], "the ghost of the last fix");

    s.step(Input::Step(Direction::North));
    assert_eq!(ages_at(&s, far), Vec::<u32>::new(), "and then it is gone");
}

/// §9.2/#192: a guard the player can **see** stamps nothing. It is drawn in full —
/// glyph, facing, cone — so a trace of the sense under it would be the channel restating
/// what sight already says, in the one colour that is supposed to mean *not seen*.
#[test]
fn a_seen_guard_leaves_no_sense_mark() {
    let guard = Cell::new(10, 6);
    let mut s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(guard)],
        Vec::new(),
        Cell::new(18, 18),
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Seen),
        "precondition: the guard is in the forward half-disc",
    );
    s.step(Input::Wait);
    assert_eq!(
        s.sense_marks().count(),
        0,
        "sight owns a seen guard; the sense marks nothing",
    );
}

/// §11.2/§11.5/#192: the fade is the **shell's** two fills over the core's age — the
/// live cell full strength, the tail behind it quiet — and both are the one `Sensed`
/// category, so a fading mark is never a second colour to learn. The core emits an age
/// and a category and never a pixel.
#[test]
fn the_trail_paints_one_category_at_two_strengths() {
    let mut s = a_guard_walking_behind_the_wall();
    let left = s.guards()[0].pos();
    s.step(Input::Wait);
    let live = s.guards()[0].pos();
    assert_ne!(live, left, "precondition: the guard walked on");

    let g = render(&s);
    let (fresh, faded) = (g.get(live.x, live.y), g.get(left.x, left.y));
    assert_eq!(fresh.bg, Some(Category::Sensed), "the live cell is sensed");
    assert_eq!(fresh.fill, Fill::Full, "…at full strength");
    assert_eq!(faded.bg, Some(Category::Sensed), "so is the trail");
    assert_eq!(faded.fill, Fill::Quiet, "…but quieter");
}

/// §11.5 **[SETTLED]**, over the new channel: **being seen outranks**. Every cell a
/// guard the player can *see* watches paints the danger overlay, including the ones a
/// fading sense mark already lies on — the detection set is the board's one
/// non-negotiable claim, and a trace of where something was a turn ago may not hide it.
///
/// The scene puts the two in the same place on purpose: a sensed guard lays a trail
/// along the room while a second guard, in plain view, sweeps its cone across the very
/// cells the trail runs through.
#[test]
fn a_visible_cone_outranks_a_fading_mark() {
    let mut s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        vec![
            // Behind the player, walking: the trail-layer, sensed through nothing but
            // the §6 half-disc's blind side.
            Guard::patrolling_to(Cell::new(6, 13), Cell::new(14, 13)),
            // Ahead of the player and facing back down the room (guards start facing
            // south): seen, so its cone is drawn, and long enough to reach across the
            // trail.
            Guard::stationary(Cell::new(10, 3)),
        ],
        Vec::new(),
        Cell::new(18, 18),
    );
    // Step rather than Wait: a Wait's 360° look would turn the trail-layer into a *seen*
    // guard and there would be no sensed mark left to outrank.
    s.step(Input::Step(Direction::East));
    s.step(Input::Step(Direction::East));

    let watched: Vec<Cell> = s.visible_cone_cells().collect();
    let stale: Vec<Cell> = s
        .sense_marks()
        .filter(|mark| mark.age > 0)
        .map(|mark| mark.cell)
        .collect();
    assert!(
        stale.iter().any(|cell| watched.contains(cell)),
        "precondition: a fading mark lies under the visible cone ({stale:?})",
    );

    let g = render(&s);
    for cell in watched {
        assert_eq!(
            g.get(cell.x, cell.y).bg,
            Some(Category::Danger),
            "a watched cell reads danger, mark or no mark ({cell:?})",
        );
    }
}
