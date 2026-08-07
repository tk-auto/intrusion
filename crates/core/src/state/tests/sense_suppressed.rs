//! The `sense_suppressed` modifier (§9/§9.4/§12.6/#493).
//!
//! A **harder** level modifier that switches the §9 sense off: no guard felt through a
//! wall, no door-change cue. What the player knows about the facility is what they can
//! see.
//!
//! Two things make it worth a module of its own rather than a flag checked in passing.
//! The first is that the modifier is defined as much by what it must **not** touch as by
//! what it takes away: a seen guard, its cone, the §11.5 danger overlay and the standing
//! watcher line of an unseen guard that has you (#465) are all fairness information
//! (§2.2), and "the cones are still displayed" is the whole shape of the change. The
//! second is Confusion: the ability's reach is clamped to `sense_range()`, so the *rule*
//! input has to survive a modifier that suppresses the *channel* — otherwise a level
//! modifier silently deletes a verb from the loadout.

use std::collections::HashSet;

use crate::level_seed::{start_level_with, LevelSeed};
use crate::render::render;
use crate::state::*;
use crate::test_support::{drive_until_guard_opens, guard_door_strip, open_room};
use crate::{AbilityId, Category, LevelConfig, LevelModifiers, Loadout};

/// The modifier as a set, so a fixture says which run it is building and nothing else.
fn suppressed() -> LevelModifiers {
    LevelModifiers {
        sense_suppressed: true,
        ..LevelModifiers::default()
    }
}

/// A v1 level from `seed`, with the modifier on or off and nothing else moved.
fn level(seed: u64, off: bool) -> State {
    let level = LevelSeed {
        seed,
        modifiers: if off {
            suppressed()
        } else {
            LevelModifiers::default()
        },
        abilities: Loadout::innate(),
    };
    start_level_with(&LevelConfig::V1, &level).expect("the v1 recipe carves")
}

/// A `w × h` room cut in two by a solid wall along row `wall_y`: the sense passes
/// through walls and sight does not, so a guard on the far side is felt and never seen
/// — even under a Wait's 360° look.
fn split_room(w: u32, h: u32, wall_y: u32) -> crate::generate::Layout {
    use crate::facility::{Facility, Terrain};
    let mut facility = Facility::walled_box(w, h);
    for x in 0..w {
        facility.set_terrain(x, wall_y, Terrain::Wall);
    }
    crate::generate::Layout::from_facility(facility)
}

/// A player at (10,10) and a guard five cells away through solid brick — inside the
/// §9.1 box and outside every line of sight. `off` switches the sense off.
fn a_guard_behind_the_wall(off: bool) -> State {
    let mut state = State::new(
        split_room(20, 20, 12),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(Cell::new(10, 15))],
        Vec::new(),
        Cell::new(18, 10),
    );
    if off {
        state = state.with_modifiers(suppressed());
    }
    state
}

/// Every cell the player perceives a guard on, and how — the channel this modifier is
/// about, read exactly as the renderer reads it.
fn perceptions(state: &State) -> Vec<(Cell, Option<GuardPerception>)> {
    state
        .guards()
        .iter()
        .map(|guard| (guard.pos(), state.perceive_guard(guard)))
        .collect()
}

/// Every cell the sense channel currently marks (§9.5) — guard trail and door cue
/// together, since the modifier takes both halves.
fn marked(state: &State) -> HashSet<Cell> {
    state.sense_marks().map(|mark| mark.cell).collect()
}

/// §9.1/§9.2/§12.6/#493: **no sensed dot on any guard, at any range, through any wall.**
/// The guard the baseline feels five cells away through solid brick is perceived as
/// nothing at all, and the channel it would have marked is empty.
#[test]
fn a_guard_behind_a_wall_is_not_perceived_at_all() {
    let baseline = a_guard_behind_the_wall(false);
    assert_eq!(
        baseline.perceive_guard(&baseline.guards()[0]),
        Some(GuardPerception::Sensed),
        "precondition: the baseline feels the guard through the wall",
    );

    let off = a_guard_behind_the_wall(true);
    assert_eq!(
        off.perceive_guard(&off.guards()[0]),
        None,
        "with the sense off a guard behind a wall is perceived as nothing",
    );
    assert!(
        marked(&off).is_empty(),
        "a guard that is not sensed stamps no cue: {:?}",
        marked(&off),
    );
}

/// §9.1/§12.6/#493: the suppression is **total**, not a shortened box. A Wait widens the
/// sense to `PLAYER_SENSE_RANGE_WAITING` (§9.1) and the guard is inside even the walking
/// box — with the modifier on, neither buys a dot, on the turn of the Wait or after it.
#[test]
fn no_range_and_no_wait_brings_a_dot_back() {
    let mut off = a_guard_behind_the_wall(true);
    for distance in [1, 5, 9] {
        let mut near = State::new(
            split_room(20, 20, 12),
            Cell::new(10, 12 - distance),
            Direction::North,
            vec![Guard::stationary(Cell::new(10, 13))],
            Vec::new(),
            Cell::new(18, 10),
        )
        .with_modifiers(suppressed());
        assert_eq!(
            near.perceive_guard(&near.guards()[0]),
            None,
            "a guard {distance} cells the other side of the wall is still not perceived",
        );
        near.step(Input::Wait);
        assert_eq!(
            near.perceive_guard(&near.guards()[0]),
            None,
            "and a Wait's widened box (§9.1) does not bring it back",
        );
    }
    off.step(Input::Wait);
    assert!(marked(&off).is_empty(), "nor does it stamp a mark");
}

/// §9.4/§10.4/§12.6/#493: **no door cue, ever.** A guard walking a closed door open
/// behind the player — the change the baseline lights the door's whole footprint for —
/// lights nothing.
#[test]
fn a_door_a_guard_opens_lights_no_cue() {
    let (mut baseline, panel) = guard_door_strip(30, Cell::new(7, 2));
    drive_until_guard_opens(&mut baseline);
    assert!(
        baseline.door_cues().any(|c| c == panel),
        "precondition: the baseline lights the door the guard opened",
    );

    let (mut off, panel) = guard_door_strip(30, Cell::new(7, 2));
    off = off.with_modifiers(suppressed());
    drive_until_guard_opens(&mut off);
    assert_eq!(
        off.door_cues().count(),
        0,
        "with the sense off a door change lights nothing",
    );
    assert!(
        !marked(&off).contains(&panel),
        "and nothing paints the panel through the shared channel",
    );
}

/// §9/§12.6/#493: the modifier suppresses the **channel**, never the ladder. The two
/// sense ranges are rule inputs — clamps read them (§8.3) — so they keep their values
/// with the modifier on, in a duct and out of it, waiting and not.
#[test]
fn the_sense_ranges_themselves_are_untouched() {
    for off in [false, true] {
        let mut state = a_guard_behind_the_wall(off);
        assert_eq!(
            state.sense_range(),
            PLAYER_SENSE_RANGE,
            "the walking range is the rule's, not the channel's",
        );
        assert_eq!(state.door_sense_range(), DOOR_SENSE_RANGE);
        state.step(Input::Wait);
        assert_eq!(
            state.sense_range(),
            PLAYER_SENSE_RANGE_WAITING,
            "a Wait still widens the range it is measured in",
        );
    }
}

/// §8.3/§9/§12.6/#493 — **the Confusion decision, pinned.** The blast's reach is
/// `min(CONFUSION_RADIUS, sense_range())`, a **[SETTLED]** clamp, so a modifier that
/// zeroed the range would zero the blast and delete the ability from the loadout. It
/// suppresses the perceived channel instead: the blast keeps its reach, still fires, and
/// still freezes the guard the player can no longer feel.
#[test]
fn confusion_still_fires_and_still_catches_what_it_would_have_caught() {
    let world = |off: bool| {
        let mut state = State::new(
            split_room(20, 20, 12),
            Cell::new(10, 10),
            Direction::North,
            // Behind the wall and inside `CONFUSION_RADIUS`: the guard the baseline
            // would have sensed, and the one the clamp's wording is about.
            vec![Guard::stationary(Cell::new(10, 14))],
            Vec::new(),
            Cell::new(18, 10),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Confusion));
        if off {
            state = state.with_modifiers(suppressed());
        }
        state
    };

    let baseline = world(false);
    let off = world(true);
    assert_eq!(
        off.confusion_blast().radius,
        baseline.confusion_blast().radius,
        "the reach a press would fire is the baseline's",
    );
    assert_eq!(
        off.confusion_blast().radius,
        CONFUSION_RADIUS,
        "and it is the ability's own row, unclamped by the suppression",
    );

    let mut off = off;
    let events = off.step(Input::Activate(AbilityId::Confusion));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ConfusionFired { .. })),
        "the blast goes off with the sense suppressed",
    );
    assert!(
        off.guard_confused(&off.guards()[0]),
        "and it freezes the guard a sensing player would have sensed",
    );
    assert_eq!(
        off.perceive_guard(&off.guards()[0]),
        None,
        "which the player still cannot see coming — that is the whole cost",
    );
}

/// §5/§8.3/§9.1/#493: **a Wait still grants the 360° look.** The innate verb loses its
/// §9.1 widening and keeps its sight half, which is the half no other verb buys — the
/// innate-verb floor (appendix 25) met by the part of it that is about seeing.
#[test]
fn a_wait_still_buys_the_full_circle() {
    let look = |off: bool| {
        let mut state = State::new(
            open_room(20, 20),
            Cell::new(10, 10),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(18, 18),
        );
        if off {
            state = state.with_modifiers(suppressed());
        }
        let forward: HashSet<Cell> = state.player_fov().cells().collect();
        state.step(Input::Wait);
        let around: HashSet<Cell> = state.player_fov().cells().collect();
        (forward, around)
    };
    let (baseline_forward, baseline_around) = look(false);
    let (forward, around) = look(true);

    assert!(
        around.contains(&Cell::new(10, 14)),
        "a Wait sees behind the player (§5), sense or no sense",
    );
    assert!(
        around.len() > forward.len(),
        "and it is still a wider look than the forward arc it replaced",
    );
    assert_eq!(
        (forward, around),
        (baseline_forward, baseline_around),
        "sight is not the sense: both looks are the baseline's, cell for cell",
    );
}

/// §11.5/§2.2/#465/#493: **the watcher line still draws.** A guard that has the player
/// from out of view paints the straight sightline to them in red — §11.5's promise that
/// a watched cell reads as watched, which §2.2 requires. It is not the sense channel and
/// this modifier deliberately leaves it alone, so being caught stays legible when the
/// dot that used to warn you is gone.
#[test]
fn an_unseen_watcher_still_paints_its_line() {
    // The player facing away from a guard that is looking straight at them: out of the
    // forward arc, so unseen, and detecting — exactly the case the line exists for. A
    // guard spawns facing south (§7.1), so the pair is stacked north-to-south and the
    // player turned the other way.
    let state = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::South,
        vec![Guard::stationary(Cell::new(10, 6))],
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_modifiers(suppressed());
    assert_eq!(
        state.perceive_guard(&state.guards()[0]),
        None,
        "precondition: the watcher is neither seen nor sensed",
    );
    assert!(
        state.guard_detects_now(&state.guards()[0]),
        "precondition: it has the player",
    );

    let line: HashSet<Cell> = state.watcher_lines().collect();
    assert!(
        line.contains(&state.player()),
        "the line reaches the player it is drawn for",
    );
    assert!(
        line.contains(&Cell::new(10, 8)),
        "and lies along the sightline between them",
    );
    let g = render(&state);
    assert_eq!(
        g.get(10, 8).bg,
        Some(Category::Danger),
        "the cell the watcher is looking down still reads as danger",
    );
}

/// §11.5/§12.6/#493 — **what the modifier must not touch**, over a seed sweep: the
/// guards the player can see, the cones they paint and the danger overlay built from
/// them are identical to baseline, cell for cell. *"Cones still displayed"* is the shape
/// of the whole modifier, so it is asserted rather than described.
#[test]
fn seen_guards_and_their_cones_are_identical_to_baseline() {
    for seed in crate::test_support::seed_sweep(32) {
        let baseline = level(seed, false);
        let off = level(seed, true);

        let seen = |state: &State| -> Vec<Cell> {
            state
                .guards()
                .iter()
                .filter(|g| state.perceive_guard(g) == Some(GuardPerception::Seen))
                .map(Guard::pos)
                .collect()
        };
        assert_eq!(
            seen(&off),
            seen(&baseline),
            "seed {seed}: a different set of guards is seen",
        );
        assert_eq!(
            off.visible_cone_cells().collect::<HashSet<_>>(),
            baseline.visible_cone_cells().collect::<HashSet<_>>(),
            "seed {seed}: the danger overlay moved",
        );
        assert_eq!(
            off.watcher_lines().collect::<HashSet<_>>(),
            baseline.watcher_lines().collect::<HashSet<_>>(),
            "seed {seed}: a watcher line moved",
        );
    }
}

/// §2.3/§12.6/#493 — **the directional assertion**: the field bites, and bites in the
/// harder direction. Over a sweep of idle turns on each seed, the run with the sense off
/// perceives **strictly fewer** guards than the baseline and marks **no** cell of the
/// channel at all, while every guard it *can see* is one the baseline saw too.
///
/// That is the shape "harder" takes for an information modifier: not a number that rises
/// but a channel that closes, with the fairness channels — sight, cones, overlay — left
/// where they were. A run that took away nothing would fail the first assertion; one
/// that took away sight as well would fail the last.
#[test]
fn the_run_perceives_strictly_less_and_sees_exactly_as_much() {
    /// Long enough for the patrols to walk in and out of the sense box several times.
    const TURNS: u32 = 120;

    let (mut sensed_baseline, mut sensed_off, mut marks_off) = (0usize, 0usize, 0usize);
    for seed in crate::test_support::seed_sweep(16) {
        let (mut baseline, mut off) = (level(seed, false), level(seed, true));
        for turn in 0..TURNS {
            baseline.step(Input::Wait);
            off.step(Input::Wait);

            let count = |state: &State, how: GuardPerception| {
                perceptions(state)
                    .into_iter()
                    .filter(|&(_, seen)| seen == Some(how))
                    .count()
            };
            assert_eq!(
                count(&off, GuardPerception::Sensed),
                0,
                "seed {seed} turn {turn}: a guard was sensed with the sense off",
            );
            assert_eq!(
                count(&off, GuardPerception::Seen),
                count(&baseline, GuardPerception::Seen),
                "seed {seed} turn {turn}: sight moved with the sense",
            );
            sensed_baseline += count(&baseline, GuardPerception::Sensed);
            sensed_off += count(&off, GuardPerception::Sensed);
            marks_off += marked(&off).len();
        }
    }
    assert!(
        sensed_baseline > 0,
        "the sweep never sensed a guard at baseline — it is measuring nothing",
    );
    assert_eq!(
        sensed_off, 0,
        "the modifier is a facade (§2.3): {sensed_off} guards were still felt",
    );
    assert_eq!(
        marks_off, 0,
        "and the channel still marked {marks_off} cells",
    );
}

/// §12.6/#493: **the same seed builds the same board**, with the modifier on and off —
/// the player, the exit, the guards and the carve. The modifier is read at the
/// perception seam alone, which is what puts it on the strictest tier of the pool's
/// same-building guarantee (`draw_from_pool`).
#[test]
fn the_same_seed_builds_the_same_board_either_way() {
    use crate::render::ascii_grid;
    for seed in crate::test_support::seed_sweep(32) {
        let baseline = level(seed, false);
        let off = level(seed, true);
        assert_eq!(
            ascii_grid(off.layout().facility()),
            ascii_grid(baseline.layout().facility()),
            "seed {seed}: the carve moved",
        );
        assert_eq!(
            (off.player(), off.facing(), off.exit),
            (baseline.player(), baseline.facing(), baseline.exit),
            "seed {seed}: the player or the way out moved",
        );
        assert_eq!(
            off.guards().iter().map(Guard::pos).collect::<Vec<_>>(),
            baseline.guards().iter().map(Guard::pos).collect::<Vec<_>>(),
            "seed {seed}: the guards moved",
        );
    }
}
