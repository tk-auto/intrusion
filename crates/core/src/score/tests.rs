//! What the three stars promise, pinned (§15 Q4/§14 v2/#563).

use super::*;
use crate::ability::Loadout;
use crate::cell::Cell;
use crate::difficulty::Difficulty;
use crate::guard::GuardState;
use crate::level_seed::{start_level, LevelSeed};
use crate::modifiers::{Composite, LevelModifiers};
use crate::place::LevelConfig;
use crate::verdict::{Ending, RunStats, Verdict};

/// A won run's stats, tuned to earn all three stars — the fixture each test spoils one
/// axis of, so a failure names the axis it broke rather than a whole struct.
fn perfect() -> RunStats {
    RunStats {
        turns: 100,
        intel: 3,
        intel_total: 3,
        caches: 2,
        caches_total: 2,
        par: 170,
        takedowns: 0,
        detections: 0,
        alert_peak: 0,
        salvaged: Loadout::empty(),
        held: Loadout::empty(),
    }
}

fn escaped(stats: RunStats) -> Verdict {
    Verdict {
        ending: Ending::Escaped,
        stats,
    }
}

/// **Each axis answers its own question and no other's.** Spoiling one leaves the other
/// two standing — which is the whole design: a total would blur three findings into one
/// number, and the readout exists to say which one you missed.
#[test]
fn the_three_axes_are_independent() {
    let all = Score::of(&escaped(perfect())).expect("a won run is scored");
    assert_eq!(all.stars(), 3, "the fixture earns everything");

    let slow = Score::of(&escaped(RunStats {
        turns: 171,
        ..perfect()
    }))
    .unwrap();
    assert_eq!(
        slow,
        Score {
            speed: false,
            stealth: true,
            thoroughness: true
        },
        "one turn over par costs speed and nothing else",
    );

    let seen = Score::of(&escaped(RunStats {
        alert_peak: 1,
        ..perfect()
    }))
    .unwrap();
    assert_eq!(
        seen,
        Score {
            speed: true,
            stealth: false,
            thoroughness: true
        },
        "the first rung costs stealth and nothing else",
    );

    for short in [
        RunStats {
            intel: 2,
            ..perfect()
        },
        RunStats {
            caches: 1,
            ..perfect()
        },
    ] {
        let score = Score::of(&escaped(short)).unwrap();
        assert_eq!(
            score,
            Score {
                speed: true,
                stealth: true,
                thoroughness: false
            },
            "either half left behind costs the haul star, and nothing else",
        );
    }
}

/// **All eight combinations are reachable, and each is reported distinctly** — the
/// criterion that stops the three stars collapsing into tiers. Zero is one of the eight:
/// escaping is not a floor, so a run that crawled out slow, loud and half-empty is scored
/// at none.
#[test]
fn every_one_of_the_eight_combinations_is_reachable_and_distinct() {
    let mut seen: Vec<(Score, String, u32)> = Vec::new();
    for speed in [false, true] {
        for stealth in [false, true] {
            for thoroughness in [false, true] {
                let stats = RunStats {
                    // Over par by one when the star is not wanted; exactly on it when it
                    // is — the boundary a player would test.
                    turns: if speed { 170 } else { 171 },
                    alert_peak: u32::from(!stealth),
                    intel: if thoroughness { 3 } else { 2 },
                    ..perfect()
                };
                let score = Score::of(&escaped(stats)).expect("a won run is scored");
                assert_eq!(
                    score,
                    Score {
                        speed,
                        stealth,
                        thoroughness
                    },
                    "the axes are read off the run, one at a time",
                );
                assert_eq!(
                    score.stars(),
                    u32::from(speed) + u32::from(stealth) + u32::from(thoroughness),
                    "the total is the count of the axes and never a weighting",
                );
                seen.push((score, score.marks(), score.stars()));
            }
        }
    }
    assert_eq!(seen.len(), 8, "two states per axis, three axes");
    for (i, left) in seen.iter().enumerate() {
        for right in &seen[i + 1..] {
            assert_ne!(left.0, right.0, "no two combinations are the same score");
            assert_ne!(
                left.1, right.1,
                "…and no two are drawn the same either: {} vs {}",
                left.1, right.1,
            );
        }
    }
    // The extremes, spelled out — a run earns none and a run earns all three.
    assert_eq!(seen.first().map(|s| s.2), Some(0), "zero stars is possible");
    assert_eq!(seen.last().map(|s| s.2), Some(3));
}

/// **Scoring happens only on escape** (§14 v2). A capture is not a zero-star run: it has
/// no score at all, and the screen owes it a reason rather than a rating.
#[test]
fn only_a_run_that_got_out_is_scored() {
    let stats = perfect();
    for lost in [
        Ending::Captured {
            guard: 0,
            state: GuardState::Chasing,
            at: Cell::new(4, 4),
        },
        Ending::Entombed {
            at: Cell::new(4, 4),
        },
    ] {
        let verdict = Verdict {
            ending: lost,
            stats,
        };
        assert_eq!(
            verdict.score(),
            None,
            "{lost:?} ended the run before any axis could be answered",
        );
    }
    assert!(
        escaped(stats).score().is_some(),
        "and the one ending that is a completion is scored",
    );
}

/// **Par is a fact about the facility, not a constant** (§14 v3): an *Outpost* and a
/// *Vault* are not held to the same number, and the ordering follows the flavours' own
/// richness. Pinned by value, so a change to the `[START]` constants is visible here.
#[test]
fn par_is_derived_per_flavour() {
    let par_of = |composite: Composite| {
        let modifiers = LevelModifiers {
            composite,
            ..LevelModifiers::neutral()
        }
        .expand_composite();
        let config = LevelConfig::V1
            .with_intel_count(modifiers.intel_count)
            .with_caches(modifiers.caches);
        par_for(config.width + config.height, config.intel, config.caches)
    };

    // The span is the same 40×40 for every flavour (§10.2 is screen-bound), so what
    // separates these numbers is entirely what each building *holds*.
    assert_eq!(
        par_of(Composite::None),
        155,
        "quick play: 3 intel, no crate"
    );
    assert_eq!(par_of(Composite::Outpost), 130, "one console fewer");
    assert_eq!(par_of(Composite::Depot), 170, "the plain recipe, one crate");
    assert_eq!(
        par_of(Composite::Workshop),
        160,
        "two crates, one console fewer"
    );
    assert_eq!(
        par_of(Composite::Vault),
        225,
        "one more of each, three crates"
    );
    assert_eq!(
        par_of(Composite::Archive),
        205,
        "two more consoles, no crate"
    );

    assert!(
        par_of(Composite::Outpost) < par_of(Composite::Depot),
        "a thinner facility is a shorter job",
    );
    assert!(
        par_of(Composite::Vault) > par_of(Composite::Depot),
        "…and the richest one on the map is the longest",
    );
}

/// **A real level's par comes off the building it actually carved**, not off the recipe
/// that asked for it — and it does not move while the raid is played, since taking a
/// console does not remove it from the facility.
#[test]
fn a_booted_level_reports_its_own_par() {
    let level = LevelSeed::quick_play_at(8371, Difficulty::Standard);
    let state = start_level(&level).expect("the v1 footprint carves");
    let facility = state.layout().facility();
    assert_eq!(
        state.par(),
        par_for(facility.width() + facility.height(), 3, 0),
        "the span it carved, the consoles it seated, the crates it hid",
    );
    assert_eq!(
        state.run_stats().par,
        state.par(),
        "and the reading the end screen gets is that same number",
    );
}

/// **Determinism** (§12.4): the same seed and the same (empty) play score the same. The
/// stars are a pure function of state, so this is really a statement that nothing in the
/// scoring path reads a clock or an unseeded source.
#[test]
fn the_same_seed_scores_the_same() {
    let level = LevelSeed::quick_play_at(4242, Difficulty::Harder);
    let one = start_level(&level).expect("carves");
    let two = start_level(&level).expect("carves");
    assert_eq!(one.par(), two.par());
    assert_eq!(one.run_stats(), two.run_stats());
    assert_eq!(
        Score::of(&escaped(one.run_stats())),
        Score::of(&escaped(two.run_stats())),
    );
}

/// **The marks and the axis names stay in step.** The end screen prints both, and a
/// glance form that disagreed with the named rows would be worse than either alone.
#[test]
fn the_glance_form_matches_the_named_axes() {
    let score = Score {
        speed: true,
        stealth: false,
        thoroughness: true,
    };
    assert_eq!(score.marks(), "★☆★");
    assert_eq!(score.stars(), 2);
    let named: String = Axis::ALL
        .iter()
        .map(|&axis| {
            if score.earned(axis) {
                STAR_EARNED
            } else {
                STAR_MISSED
            }
        })
        .collect();
    assert_eq!(named, score.marks(), "one derivation, two readings");

    // Every axis says what it is and what it was for — a screen cannot print a star with
    // nothing beside it.
    for axis in Axis::ALL {
        assert!(!axis.label().is_empty());
        assert!(!axis.blurb().is_empty());
    }
}

/// **A facility with nothing left to take is thoroughly emptied.** This is the quick-play
/// degeneracy the design records rather than hides: the gate is *all the intel* and no
/// crates are planted, so the haul star follows the escape. Pinned so that if quick play
/// ever grows crates, the change shows up here rather than as a star quietly becoming
/// hard.
#[test]
fn quick_play_has_no_crates_to_miss() {
    assert_eq!(LevelConfig::V1.caches, 0, "quick play plants none");
    let score = Score::of(&escaped(RunStats {
        intel: 3,
        intel_total: 3,
        caches: 0,
        caches_total: 0,
        ..perfect()
    }))
    .unwrap();
    assert!(
        score.thoroughness,
        "with no crate in the building, taking every console is taking everything",
    );
}
