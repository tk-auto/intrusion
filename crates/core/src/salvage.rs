//! **What a facility's equipment caches hold** (§2.2/§8.3/§14 v3/#209) — the one draw
//! that decides which pieces of salvaged tech are in the crates.
//!
//! §14 v3 calls salvaged tech accumulating across facilities *"the run's power curve and
//! the reason the campaign exists"*, and records that last time it was "fully built and
//! reachable by nobody": no facility ever generated a cache, so no ability could ever be
//! unlocked. [`crate::place`] plants the crates; this decides what is in them.
//!
//! # It is a property of the facility, and of nothing else
//!
//! A crate's contents are drawn from the **facility seed alone** — not from what the run
//! is carrying. A building is stocked before anybody breaks into it, and stocking it out
//! of the intruder's pockets would be the facility knowing who was coming.
//!
//! The consequence is deliberate: **a run can meet the same tech twice.** Walking up to a
//! crate holding the Decoy you already carry is bad luck, not a bug — the bump refuses it
//! for free ([`Affordance::SalvageCarried`](crate::Affordance)) and there are seven other
//! things in the world to find. What it is *not* is a reward economy that has to be made
//! whole: no consolation prize, no "second copy upgrades the first" (that is the
//! ability-upgrade sink's, a different ticket), and no draw that peeks at the loadout to
//! spare you the disappointment.
//!
//! # Within one facility they are all different
//!
//! The flavour says how many crates a facility hides (§14 v3: a Depot one, a Workshop
//! two, a Vault three), and this fills them **in one draw** — a seeded shuffle of
//! [`AbilityId::TECH`], with the crates taking a prefix of it. So the three crates of a
//! Vault hold three different things by construction, with no rejection loop and no draw
//! that could fail: searching one building is never two of the same box.
//!
//! Across facilities there is no such rule, because there is nothing to enforce it with —
//! each facility is drawn from its own seed and knows nothing of the ones before it.
//!
//! # Derived from the facility seed, never a fresh source
//!
//! The draw hangs off the level's own seed through [`SALVAGE_STREAM_SALT`] (§12.4), on
//! the same discipline as the campaign's per-facility and per-map streams: a stream of
//! its own, so what a crate holds cannot shift because generation drew first, and
//! generation cannot shift because a crate was drawn.
//!
//! Taking the seed and nothing else is also what keeps a **level-seed token** honest
//! (§13.1): the crates of a shared facility are the crates whoever shared it found,
//! whatever either of you happened to be carrying at the time.

use crate::ability::AbilityId;
use crate::rng::Rng;

/// Separates the cache draw from every other use of a level's seed (§12.4) — the same
/// two-streams-that-never-share-a-position rule the campaign's facility and map salts
/// keep.
const SALVAGE_STREAM_SALT: u64 = 0x_CAC4_E7EC_CAC4_E7EC;

/// **What this facility's caches hold** (§8.3/#209): `wanted` distinct pieces of
/// salvaged tech, drawn deterministically from the level seed — one per crate, in the
/// order the crates are placed.
///
/// Shorter than `wanted` only if a facility asked for more crates than there is tech in
/// the catalogue, which no flavour does ([`CacheCount::MAX`](crate::CacheCount) is three
/// against a pool of eight).
pub fn cache_contents(seed: u64, wanted: usize) -> Vec<AbilityId> {
    let mut pool = AbilityId::TECH;
    let mut rng = Rng::new(seed ^ SALVAGE_STREAM_SALT);
    // Fisher-Yates over the whole pool, then its first `wanted` entries — so *which*
    // tech a facility is stocked with varies with the seed, and the crates of one
    // building are distinct by construction rather than by a check.
    for i in (1..pool.len()).rev() {
        let j = rng.below((i + 1) as u32) as usize;
        pool.swap(i, j);
    }
    pool.into_iter().take(wanted).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::Loadout;
    use crate::level_seed::{start_level, LevelSeed};
    use crate::modifiers::{CacheCount, LevelModifiers};
    use std::collections::HashSet;

    /// **Same seed, same crates** (§12.4) — the property the golden test over a whole
    /// facility rests on, stated directly on the draw.
    #[test]
    fn the_draw_is_a_function_of_the_seed() {
        for seed in [0, 1, 8371, u64::MAX] {
            for wanted in 0..=CacheCount::MAX {
                assert_eq!(
                    cache_contents(seed, wanted),
                    cache_contents(seed, wanted),
                    "seed {seed}, {wanted} crates",
                );
            }
        }
    }

    /// **A facility's crates are all different.** Not by a rejection loop — by taking a
    /// prefix of one shuffle — so it holds on every seed, at every count, with no draw
    /// that could fail.
    #[test]
    fn no_two_crates_in_one_facility_hold_the_same_tech() {
        for seed in 0..300 {
            for wanted in 0..=CacheCount::MAX {
                let drawn = cache_contents(seed, wanted);
                assert_eq!(drawn.len(), wanted, "seed {seed}: short of {wanted} crates");
                let distinct: HashSet<_> = drawn.iter().collect();
                assert_eq!(distinct.len(), drawn.len(), "seed {seed}: {drawn:?}");
            }
        }
    }

    /// **A prefix, so the counts nest.** A Vault's three crates are a Workshop's two with
    /// one more behind them on the same seed — which makes a flavour's count read as *how
    /// much of this building's stock you get at* rather than as a different building.
    #[test]
    fn a_bigger_facility_holds_the_smaller_ones_crates_and_more() {
        for seed in 0..200 {
            let three = cache_contents(seed, 3);
            assert_eq!(cache_contents(seed, 2), three[..2], "seed {seed}");
            assert_eq!(cache_contents(seed, 1), three[..1], "seed {seed}");
            assert!(cache_contents(seed, 0).is_empty());
        }
    }

    /// It is a **draw**, not a constant: different facilities are stocked with different
    /// tech, or "derived from the facility seed" would be satisfied by a hard-coded
    /// answer that happened to be reproducible (§2.3, the anti-facade guard).
    #[test]
    fn different_facilities_hold_different_tech() {
        let drawn: HashSet<_> = (0..200).flat_map(|seed| cache_contents(seed, 1)).collect();
        assert_eq!(
            drawn.len(),
            AbilityId::TECH.len(),
            "every piece of tech should be findable on some seed, got {drawn:?}",
        );
    }

    /// **The draw does not look at the loadout** (#209): a facility is stocked before
    /// anybody breaks into it, so meeting tech you already carry is bad luck rather than
    /// something the world rearranges itself to prevent. Two runs standing in the same
    /// facility find the same crates, whatever either is carrying.
    #[test]
    fn what_a_run_carries_does_not_restock_the_building() {
        let level = |abilities| LevelSeed {
            seed: 8371,
            modifiers: LevelModifiers {
                caches: CacheCount::Three,
                ..LevelModifiers::default()
            },
            abilities,
        };
        let bare = start_level(&level(Loadout::innate())).expect("the v1 footprint carves");
        let found = bare.cache_contents();
        assert_eq!(found.len(), 3);

        // A run walking in already holding the first crate's tech finds the very same
        // crates in the very same cells.
        let laden =
            start_level(&level(Loadout::innate().with(found[0]))).expect("the v1 footprint carves");
        assert_eq!(laden.cache_contents(), found);
        assert_eq!(laden.equipment_caches(), bare.equipment_caches());
    }

    /// **The golden facility** (§12.4/#209): one named seed, booted twice, plants its
    /// crates in the same cells holding the same tech — and both are written down, so a
    /// change to the derivation is a red test rather than a silently different game for
    /// everyone who has shared a run.
    ///
    /// Presence, position and contents, which is the whole of what a cache is.
    #[test]
    fn the_same_facility_seed_plants_the_same_crates() {
        let level = LevelSeed {
            seed: 8371,
            modifiers: LevelModifiers {
                caches: CacheCount::Three,
                ..LevelModifiers::default()
            },
            abilities: Loadout::innate(),
        };
        let boot = || start_level(&level).expect("the v1 footprint carves");
        let (a, b) = (boot(), boot());
        assert_eq!(a.equipment_caches(), b.equipment_caches());
        assert_eq!(a.cache_contents(), b.cache_contents());

        assert_eq!(a.cache_contents(), GOLDEN_STOCK, "the stock changed");
        assert_eq!(a.equipment_caches(), golden_crates(), "the crates moved");
    }

    /// The golden facility's stock and crate cells — written out so the two assertions
    /// above read as one statement about one building.
    /// Refreshed when the Saver joined the pool (#243), again for the Drone (#273) and
    /// again for False Call and the Guide (#504/#505): a shuffle over twelve entries
    /// deals this seed a different three than a shuffle over ten did. Every tech ever added moves this, and that is the point of pinning it —
    /// the stock is derived from the seed and the roster, never carried in the
    /// level-seed token, so a change to *either* has to be a visible decision rather
    /// than a quietly different game.
    const GOLDEN_STOCK: [AbilityId; 3] = [
        AbilityId::Camouflage,
        AbilityId::PierceWall,
        AbilityId::Saver,
    ];
    fn golden_crates() -> [crate::Cell; 3] {
        [
            crate::Cell::new(19, 20),
            crate::Cell::new(29, 1),
            crate::Cell::new(17, 29),
        ]
    }

    /// **The knob is what plants them** (§12.6): the same seed at [`CacheCount::None`] is
    /// a facility with no crate in it at all, which is every quick-play level.
    #[test]
    fn no_knob_no_crates() {
        let bare = start_level(&LevelSeed::quick_play(8371)).expect("the v1 footprint carves");
        assert!(bare.equipment_caches().is_empty());
        assert!(bare.cache_contents().is_empty());
    }
}
