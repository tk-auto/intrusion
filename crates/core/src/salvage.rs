//! **What an equipment cache holds** (§2.2/§8.3/§14 v3/#209) — the one draw that
//! decides which piece of salvaged tech a facility is hiding.
//!
//! §14 v3 calls salvaged tech accumulating across facilities *"the run's power curve
//! and the reason the campaign exists"*, and records that last time it was "fully built
//! and reachable by nobody": no facility ever generated a cache, so no ability could
//! ever be unlocked. [`crate::place`] plants the crate; this decides what is in it.
//!
//! # It is a draw over what the run does **not** hold
//!
//! A cache is only a reward if it hands over something new. Drawing blind from
//! [`AbilityId::TECH`] would offer a run its fifth Dephase soon enough — the ticket's
//! "already-owned collision" — and the two ways out of that are to avoid it in the draw
//! or to pay a consolation prize instead. **Avoided by draw**, here: the pool is
//! shuffled deterministically and the first entry the run does not already hold is
//! taken. A consolation prize would be a second reward economy invented to paper over a
//! draw that could simply not make the mistake, and "a second copy upgrades the first"
//! is explicitly the ability-upgrade sink's, not this.
//!
//! **A shuffle rather than a rejection loop**, so the answer is a pure function of the
//! seed and the held set with no unbounded draw in it — and so that a run holding two
//! of the pool still meets the remaining six in a seeded order rather than in
//! `AbilityId::ALL`'s.
//!
//! # Exhausting the pool plants nothing
//!
//! [`None`] when the run already holds every tech there is: the facility generates
//! **without a cache** rather than with an empty crate or a duplicate one. That is the
//! honest reading — there is nothing left in the world for this run to find — and it is
//! the one branch the caller has to respect *before* generation, which
//! [`start_level_with`](crate::start_level_with) does.
//!
//! No standard run reaches it: a campaign raids one facility more than
//! [`DEPTH_TO_ARCHIVE`](crate::DEPTH_TO_ARCHIVE) against a pool of eight, and a
//! level-seed token cannot carry more than [`AbilityId::MAX_TECH_HELD`] tech at all. It
//! is reachable from a hand-built loadout, which is exactly why it is defined and pinned
//! rather than left to whatever the code happened to do.
//!
//! # Derived from the facility seed, never a fresh source
//!
//! The draw hangs off the level's own seed through [`SALVAGE_STREAM_SALT`] (§12.4), on
//! the same discipline as the campaign's per-facility and per-map streams: a stream of
//! its own, so what a cache holds cannot shift because generation drew first, and
//! generation cannot shift because a cache was drawn.
//!
//! It takes the **loadout** as its other input, and that is not a second source of
//! truth: a loadout is part of the [`LevelSeed`](crate::LevelSeed) a facility boots
//! from, so a token still reproduces the cache exactly — including a campaign
//! facility's, whose token carries the tech the run walked in with.

use crate::ability::{AbilityId, Loadout};
use crate::rng::Rng;

/// Separates the cache draw from every other use of a level's seed (§12.4) — the same
/// two-streams-that-never-share-a-position rule the campaign's facility and map salts
/// keep.
const SALVAGE_STREAM_SALT: u64 = 0x_CAC4_E7EC_CAC4_E7EC;

/// **What the cache in this facility holds** (§8.3/#209): a piece of salvaged tech the
/// run does not already carry, drawn deterministically from `(level seed, held set)`.
///
/// `None` when `held` already holds every entry of [`AbilityId::TECH`] — see the module
/// note; the caller plants no cache at all rather than an empty one.
pub fn cache_contents(seed: u64, held: Loadout) -> Option<AbilityId> {
    let mut pool = AbilityId::TECH;
    let mut rng = Rng::new(seed ^ SALVAGE_STREAM_SALT);
    // Fisher-Yates over the whole pool, then the first unheld entry of the shuffled
    // order — so *which* new tech a run is offered varies with the seed, and a run that
    // holds some of the pool is not walked down the catalog order for the rest.
    for i in (1..pool.len()).rev() {
        let j = rng.below((i + 1) as u32) as usize;
        pool.swap(i, j);
    }
    pool.into_iter().find(|&id| !held.contains(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level_seed::{start_level, LevelSeed};
    use crate::modifiers::LevelModifiers;

    /// **Same seed, same crate** (§12.4) — the property the golden test over a whole
    /// facility rests on, stated directly on the draw.
    #[test]
    fn the_draw_is_a_function_of_the_seed_and_the_held_set() {
        for seed in [0, 1, 8371, u64::MAX] {
            assert_eq!(
                cache_contents(seed, Loadout::innate()),
                cache_contents(seed, Loadout::innate()),
            );
        }
    }

    /// It is a **draw**, not a constant: different seeds hand out different tech, or the
    /// "derived from the facility seed" acceptance would be satisfied by a hard-coded
    /// answer that happened to be reproducible (§2.3, the anti-facade guard).
    #[test]
    fn different_facilities_hold_different_tech() {
        let drawn: std::collections::HashSet<_> = (0..200)
            .filter_map(|seed| cache_contents(seed, Loadout::innate()))
            .collect();
        assert_eq!(
            drawn.len(),
            AbilityId::TECH.len(),
            "every piece of tech should be findable on some seed, got {drawn:?}",
        );
    }

    /// **Never what the run already holds** — the no-repeat rule, avoided by draw.
    #[test]
    fn a_cache_never_holds_tech_the_run_already_carries() {
        for seed in 0..200 {
            // Walk a run forward exactly as a campaign does: take what the cache holds,
            // and ask the next facility.
            let mut held = Loadout::innate();
            for _ in 0..AbilityId::TECH.len() {
                let found = cache_contents(seed, held).expect("the pool is not empty yet");
                assert!(
                    !held.contains(found),
                    "seed {seed} offered {found:?} twice over",
                );
                held = held.with(found);
            }
        }
    }

    /// **The golden facility** (§12.4/#209): one named seed, booted twice, plants its
    /// crate in the same cell holding the same tech — and those are written down, so a
    /// change to the derivation is a red test rather than a silently different game for
    /// everyone who has shared a run.
    ///
    /// Presence, position and contents, which is the whole of what a cache is.
    #[test]
    fn the_same_facility_seed_plants_the_same_crate() {
        let level = LevelSeed {
            seed: 8371,
            modifiers: LevelModifiers {
                equipment_cache: true,
                ..LevelModifiers::default()
            },
            abilities: Loadout::innate(),
        };
        let boot = || start_level(&level).expect("the v1 footprint carves");
        let (a, b) = (boot(), boot());
        assert_eq!(a.equipment_cache(), b.equipment_cache());
        assert_eq!(a.cache_holds(), b.cache_holds());

        assert_eq!(
            (a.equipment_cache(), a.cache_holds()),
            (Some(crate::Cell::new(16, 20)), Some(AbilityId::Confusion)),
            "the golden facility's crate moved",
        );

        // **The loadout is an input to the draw**, so a run that already holds that
        // tech is offered something else in the same crate — the no-repeat rule, on a
        // real facility rather than on the draw alone.
        let held = LevelSeed {
            abilities: Loadout::innate().with(AbilityId::Confusion),
            ..level
        };
        let other = start_level(&held).expect("the v1 footprint carves");
        assert_eq!(
            other.equipment_cache(),
            a.equipment_cache(),
            "the crate is the generator's business and must not move with the loadout",
        );
        assert!(matches!(other.cache_holds(), Some(id) if id != AbilityId::Confusion));
    }

    /// **The modifier is what plants it** (§12.6): the same seed without it is a
    /// facility with no crate in it at all, which is every quick-play level.
    #[test]
    fn no_modifier_no_crate() {
        let bare = start_level(&LevelSeed::quick_play(8371)).expect("the v1 footprint carves");
        assert_eq!(bare.equipment_cache(), None);
        assert_eq!(bare.cache_holds(), None);
    }

    /// A run that holds **everything** finds nothing: the pool is exhausted, so the
    /// facility plants no cache rather than an empty crate (see the module note).
    #[test]
    fn an_exhausted_pool_holds_nothing() {
        let mut everything = Loadout::innate();
        for id in AbilityId::TECH {
            everything = everything.with(id);
        }
        for seed in [0, 1, 8371] {
            assert_eq!(cache_contents(seed, everything), None);
        }
        // One short of exhausted still finds the one that is left, on every seed —
        // scarcity narrows the draw, it does not make it fail early.
        for missing in AbilityId::TECH {
            let mut held = Loadout::innate();
            for id in AbilityId::TECH.into_iter().filter(|&id| id != missing) {
                held = held.with(id);
            }
            for seed in [0, 1, 8371] {
                assert_eq!(cache_contents(seed, held), Some(missing));
            }
        }
    }
}
