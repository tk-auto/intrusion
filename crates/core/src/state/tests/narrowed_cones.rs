//! The `narrowed_guard_cones` modifier through a generated level (§6.1/§7.6/§12.6/#495).
//!
//! An **easier** level modifier: every guard's cone is shorter
//! ([`GuardSight::NARROWED`]) — the same building, the same guards, §7.1's own ~90°
//! wedge, less reach each.
//!
//! The **cone** itself and the §7.6 zones it carries are pinned next door, in
//! `guard::tests` — the subset, the touching ring, the three detection rungs on a
//! shortened ladder. What is pinned here is everything that only exists once a level
//! is running: the seam the modifier is read at, the board it must leave alone, the
//! §11.5 overlay that has to follow the cone exactly, and the §2.3 assertion that it
//! bends the run in the direction its caption claims.

use crate::level_seed::{start_level_with, LevelSeed};
use crate::render::{ascii_grid, render};
use crate::state::*;
use crate::test_support::{open_room, seed_sweep};
use crate::vision::GuardSight;
use crate::{Category, LevelConfig, LevelModifiers, Loadout};
use std::collections::HashSet;

/// A v1 level from `seed`, with the modifier on or off and nothing else moved.
fn level(seed: u64, narrowed: bool) -> State {
    let level = LevelSeed {
        seed,
        modifiers: LevelModifiers {
            narrowed_guard_cones: narrowed,
            ..LevelModifiers::default()
        },
        abilities: Loadout::innate(),
    };
    start_level_with(&LevelConfig::V1, &level).expect("the v1 recipe carves")
}

/// Every cell any guard currently detects — the §11.5 detection set, read off the
/// guards' own cones.
fn watched(state: &State) -> HashSet<Cell> {
    state
        .guards()
        .iter()
        .flat_map(|guard| guard.fov().cells())
        .collect()
}

/// §12.3/§12.6/#495: the modifier is read at **one** seam — how a guard sees — and
/// resolves into the level's [`GuardSight`] rather than being queried anywhere else.
#[test]
fn the_modifier_resolves_into_the_guard_sight_and_nothing_else() {
    assert_eq!(level(1, false).guard_sight(), GuardSight::BASELINE);
    assert_eq!(level(1, true).guard_sight(), GuardSight::NARROWED);
}

/// §12.6/§2.3/#495 — **the directional assertion**, in the strongest frame there is:
/// the same seed, the same building, the same guards standing in the same cells facing
/// the same way, so on turn one every guard's detection set is a **subset** of what it
/// watched at baseline.
///
/// Turn one is where the subset can be exact rather than distributional, and that is not
/// a weaker statement than it looks: a cone is also what a §7.5 patrol *inspects*, so
/// from turn two the two arms send their guards down different legs and the comparison
/// stops being cell-for-cell. The run-long version of the same claim is the sweep below.
///
/// **The "it really bites" half is stated over the sweep, not per seed**, and the reason
/// is the modifier's own character rather than a weakened claim. This one shortens the
/// **reach** and leaves §7.1's wedge alone (appendix 50), so on a seed where every guard
/// happens to open facing into a room shallower than 6 cells the walls were already
/// doing the work and turn one is legitimately identical — which is the same §10.1a fact
/// that makes a shortened cone a one-guard step rather than a three-guard one. So: never
/// more ground than baseline on any seed, strictly less in total, and less on a clear
/// majority of seeds.
#[test]
fn every_guard_watches_a_subset_of_what_it_watched_at_baseline() {
    /// How many of the swept seeds must open with visibly less ground watched. The
    /// measured share is well above this; the margin is for the seeds whose guards all
    /// face short rooms, where walls already bound the baseline's cone.
    const SHRINKS_ON: f64 = 0.5;

    let seeds = seed_sweep(32);
    let (mut shrank, mut narrowed_total, mut baseline_total) = (0usize, 0usize, 0usize);
    for &seed in &seeds {
        let baseline = level(seed, false);
        let narrowed = level(seed, true);
        for (i, (dim, full)) in narrowed.guards().iter().zip(baseline.guards()).enumerate() {
            assert_eq!(
                (dim.pos(), dim.facing()),
                (full.pos(), full.facing()),
                "seed {seed}: guard {i} does not even stand where the baseline's does",
            );
            let (dim, full): (HashSet<Cell>, HashSet<Cell>) =
                (dim.fov().cells().collect(), full.fov().cells().collect());
            assert!(
                dim.is_subset(&full),
                "seed {seed}: guard {i} watches ground the baseline's does not",
            );
        }
        let (dim, full) = (watched(&narrowed).len(), watched(&baseline).len());
        assert!(
            dim <= full,
            "seed {seed}: more ground watched than baseline"
        );
        shrank += usize::from(dim < full);
        narrowed_total += dim;
        baseline_total += full;
    }
    assert!(
        narrowed_total < baseline_total,
        "the modifier is a facade — the same ground is watched (§2.3): \
         {narrowed_total} against {baseline_total}",
    );
    assert!(
        shrank as f64 >= seeds.len() as f64 * SHRINKS_ON,
        "only {shrank} of {} seeds open with less ground watched — the shortened cone \
         is not reaching past the walls often enough to be a rule",
        seeds.len(),
    );
}

/// §12.6/§2.3/#495: the run-long half of the claim. Over a long idle sweep — the guards
/// patrolling undisturbed, the two arms free to diverge onto different legs — the
/// narrowed run still puts **less cone on the building**, on every seed.
///
/// Direction only, never a leaderboard (§13.4): the claim is *no more than baseline*,
/// per seed. It is the run-long statement the turn-one subset cannot make, and the one
/// that would catch a shorter cone paying for itself by sending patrols further afield.
#[test]
fn a_narrowed_run_puts_less_cone_on_the_building() {
    /// Long enough for several patrol legs on either arm, short enough to sweep the
    /// seeds inside the gate.
    const TURNS: u32 = 200;

    let watched_turns = |mut state: State| {
        (0..TURNS)
            .map(|_| {
                state.step(Input::Wait);
                watched(&state).len()
            })
            .sum::<usize>()
    };
    for seed in seed_sweep(16) {
        let narrowed = watched_turns(level(seed, true));
        let baseline = watched_turns(level(seed, false));
        assert!(
            narrowed < baseline,
            "seed {seed}: {narrowed} watched cell-turns against the baseline's {baseline}",
        );
    }
}

/// §12.6/#495: **the same seed builds the same facility**, with the modifier on and
/// off, including where the player, the exit and every guard stand.
///
/// This is what puts the entry on the strictest tier of the pool's same-building
/// guarantee. Placement pins §7.1's own cone for its turn-one spawn check
/// ([`place`](crate::place)), so a shorter cone cannot pass spawn cells the baseline
/// refuses — without that pinning, an *easier* draw would quietly move the guards, and
/// the ±N arms of a comparison would be two facilities rather than two rulesets.
#[test]
fn the_same_seed_builds_the_same_board_either_way() {
    for seed in seed_sweep(32) {
        let baseline = level(seed, false);
        let narrowed = level(seed, true);
        assert_eq!(
            ascii_grid(narrowed.layout().facility()),
            ascii_grid(baseline.layout().facility()),
            "seed {seed}: the carve moved",
        );
        assert_eq!(
            (narrowed.player(), narrowed.facing(), narrowed.exit),
            (baseline.player(), baseline.facing(), baseline.exit),
            "seed {seed}: the player or the way out moved",
        );
        assert_eq!(
            narrowed.guards().iter().map(Guard::pos).collect::<Vec<_>>(),
            baseline.guards().iter().map(Guard::pos).collect::<Vec<_>>(),
            "seed {seed}: the guards moved",
        );
    }
}

/// §11.5 **[SETTLED]**/#495: **the overlay is the detection set**, so a cone that draws
/// smaller must detect smaller — the two cannot be allowed to drift apart, which is
/// exactly what a modifier that painted its own smaller cone while the guards kept
/// detecting the old one would do.
///
/// Asserted as an identity rather than as a shrink: the red set of the frame is
/// **exactly** the union of the drawn cones, and it is strictly smaller than the same
/// frame at baseline. The modifier is a level's answer to *how does a guard see*, read
/// once ([`State::guard_sight`]) by the cones the overlay is painted from, so there is
/// one truth here by construction — this is what holds it that way.
///
/// A hand-built room and `always_show_vision_cones`, the other easier entry, so every
/// guard's cone paints on a frame the test controls: on a generated level the player
/// opens inside their own tunnel (§4.5/#466), perceiving only the mouth peek, and the
/// identity would hold over two empty sets. It is also the one composition worth
/// pinning — the two easier entries a `−2` draw can land together.
#[test]
fn the_danger_overlay_is_exactly_the_narrowed_detection_set() {
    let painted = |state: &State| -> HashSet<Cell> {
        let grid = render(state);
        (0..grid.height())
            .flat_map(|y| (0..grid.width()).map(move |x| Cell::new(x, y)))
            .filter(|c| grid.get(c.x, c.y).bg == Some(Category::Danger))
            .collect()
    };
    let scene = |narrowed: bool| {
        State::new(
            open_room(25, 25),
            Cell::new(2, 22), // far from the guard, and never in its cone
            Direction::South,
            vec![Guard::stationary(Cell::new(12, 4))], // faces south, into the room
            Vec::new(),
            Cell::new(23, 23),
        )
        .with_modifiers(LevelModifiers {
            narrowed_guard_cones: narrowed,
            always_show_vision_cones: true,
            ..LevelModifiers::default()
        })
    };

    let mut narrowed = scene(true);
    narrowed.step(Input::Wait); // the first sight phase casts the cones
    let red = painted(&narrowed);
    assert!(!red.is_empty(), "the guard's cone must paint something");
    assert_eq!(
        red,
        watched(&narrowed),
        "the overlay and the narrowed cone disagree (§11.5)",
    );

    let mut baseline = scene(false);
    baseline.step(Input::Wait);
    let full = painted(&baseline);
    assert_eq!(
        full,
        watched(&baseline),
        "…and the baseline is the same rule"
    );
    assert!(
        red.is_subset(&full) && red.len() < full.len(),
        "the drawn cone did not shrink with the detected one: {} against {}",
        red.len(),
        full.len(),
    );
}

/// §12.4/#495: determinism, with the modifier and without it — same seed, same
/// modifiers, same inputs, the same run cell for cell. The cone is a pure recompute
/// drawing no RNG, so a narrowed run cannot perturb the stream either.
#[test]
fn a_narrowed_run_replays_exactly() {
    for seed in seed_sweep(8) {
        for narrowed in [false, true] {
            let walked = || {
                let mut state = level(seed, narrowed);
                (0..120)
                    .map(|_| {
                        state.step(Input::Wait);
                        state.guards().iter().map(Guard::pos).collect::<Vec<Cell>>()
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(walked(), walked(), "seed {seed}, narrowed {narrowed}");
        }
    }
}
