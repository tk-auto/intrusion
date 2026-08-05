//! The **difficulty draw** (§12.6/#297) — the level-modifier seam's difficulty axis.
//!
//! A [`Difficulty`] is a level from −2 to +2 over the quick-play base. It resolves to
//! a concrete [`LevelModifiers`] by drawing `|level|` modifiers from the §12.6
//! **directed pool** ([`draw_from_pool`]) in the sign's direction — *harder* for
//! positive, *easier* for negative — and composing them onto the base.
//! [`Standard`](Difficulty::Standard) draws nothing at all, so it is exactly today's
//! quick play, byte for byte.
//!
//! # It resolves before the run boots, so the token needs no new field
//!
//! The draw is a **pure function of `(level, seed)`** and runs *before* the run
//! starts. What the [`LevelSeed`](crate::LevelSeed) then carries is the **resolved
//! set**, exactly as it already did — so the difficulty number needs no bit in the
//! level-seed token, and a shared token still reproduces the run precisely (§12.4:
//! same seed + same modifiers + same inputs → identical run). A player who is handed
//! a token gets the run, not a recipe for re-rolling one.
//!
//! # Its own sub-stream, and the one thing that escapes it
//!
//! The draw takes from a sub-stream salted away from the generation stream
//! ([`DIFFICULTY_STREAM_SALT`]), the same discipline the quick-play loadout draw
//! keeps, so the *draw itself* never shifts what a seed carves: what the axis returns
//! is a set of rules, and reading it costs the generator nothing.
//!
//! That used to make a flat promise — a seed's facility is byte-identical at every
//! difficulty, and only the rules bending it differ, which is what made the ±N arms of a
//! comparison worth anything. **Since #518 it holds for every pool entry but one.** The
//! salt governs the *stream*; it cannot govern what a drawn modifier then does, and
//! `automatic doors` is read by the **carve** (§12.6), so a draw that picks it produces a
//! different building from the same seed. Every other entry is read at runtime, by the
//! renderer, or by placement, and leaves the carve alone.
//!
//! So the guarantee is graded now, in three tiers: the doors may move the building, the
//! locked room may only re-role cells inside doorways the carve already cut (§10.4), and
//! every other entry leaves the grid byte-identical. That is pinned by
//! `a_difficulty_draw_moves_the_building_only_where_it_is_meant_to` rather than left as
//! a sentence here, because a new generation-reaching entry would falsify it silently —
//! the test is what makes admitting one a deliberate act.
//!
//! # The direction cannot be cancelled
//!
//! §2.3's anti-facade rule wants a **directional assertion** on every shipped
//! modifier, and the draw inherits it: a `+N` draw only ever switches on toggles
//! documented *harder*, and a `−N` draw only ever ones documented *easier*, so no
//! draw can hand back a set that bends the other way from the one asked for. The pool
//! is filtered on [`ModifierDirection`] itself rather than on a list of names, which
//! is what makes that true by construction instead of by review — and the filtering
//! lives in [`draw_from_pool`], so the campaign alert (#210), which draws from the same
//! pool, inherits the same guarantee rather than restating it.

use crate::modifiers::{draw_from_pool, pool_size, LevelModifiers, ModifierDirection};

/// A fixed transform applied to the run seed before the difficulty draw, so it takes
/// from a sub-stream **independent** of generation and of the loadout draw (§12.4).
/// This is what keeps the *draw* from shifting the facility the seed carves — the
/// streams never share a position — while the whole run still derives from one seed.
///
/// It says nothing about what a **drawn** modifier does afterwards: a generation-depth
/// entry still reaches the generator as a parameter, and `automatic doors` is one
/// (§12.6/#518). See the module note above.
const DIFFICULTY_STREAM_SALT: u64 = 0x_D1FF_0000_D1FF_0000;

/// How far the level runs either side of the baseline. The axis is −[`SPAN`]…+[`SPAN`]
/// and [`Difficulty::from_level`] clamps to it, so there is no free integer anywhere
/// that could name a sixth position nothing draws for.
pub const SPAN: i8 = 2;

/// The **difficulty level** of a run (§12.6/#297) — five positions either side of the
/// quick-play baseline, resolved to modifiers by [`draw`](Difficulty::draw).
///
/// A clamped enum rather than an integer: the five positions are the whole axis, the
/// slider that sets it (#298) has exactly these stops, and there is no representable
/// value the draw has no answer for.
///
/// The labels are **meta vocabulary** (§11.8) — they name the run's *setup*, like
/// *seed* and *loadout*, not anything inside the facility — so they say plainly which
/// way the run is bent rather than dressing it as a fiction about the building. They
/// are the same two words [`ModifierDirection`] uses, which is the point: the label a
/// player reads and the direction the draw filters on cannot come apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Difficulty {
    /// −2: two rules bent the player's way.
    MuchEasier,
    /// −1: one rule bent the player's way.
    Easier,
    /// 0: the baseline — exactly today's quick play, nothing drawn.
    #[default]
    Standard,
    /// +1: one rule bent against the player.
    Harder,
    /// +2: two rules bent against the player.
    MuchHarder,
}

impl Difficulty {
    /// Every position, easiest first — the slider's order, and the order #298 draws
    /// its stops in.
    pub const ALL: [Difficulty; 5] = [
        Difficulty::MuchEasier,
        Difficulty::Easier,
        Difficulty::Standard,
        Difficulty::Harder,
        Difficulty::MuchHarder,
    ];

    /// The position's level on the −[`SPAN`]…+[`SPAN`] axis, `0` at the baseline.
    #[must_use]
    pub fn level(self) -> i8 {
        match self {
            Difficulty::MuchEasier => -2,
            Difficulty::Easier => -1,
            Difficulty::Standard => 0,
            Difficulty::Harder => 1,
            Difficulty::MuchHarder => 2,
        }
    }

    /// The position at `level`, **clamped** to the axis — an out-of-range number is
    /// the nearest end, never a panic and never a sixth position.
    #[must_use]
    pub fn from_level(level: i32) -> Self {
        let level = level.clamp(-i32::from(SPAN), i32::from(SPAN)) as i8;
        Self::ALL
            .into_iter()
            .find(|position| position.level() == level)
            .unwrap_or(Difficulty::Standard)
    }

    /// The label a player reads on the slider (#298) — what the position *means*,
    /// not its number.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Difficulty::MuchEasier => "Much easier",
            Difficulty::Easier => "Easier",
            Difficulty::Standard => "Standard",
            Difficulty::Harder => "Harder",
            Difficulty::MuchHarder => "Much harder",
        }
    }

    /// The line under the label: how many rules the position will actually bend, and
    /// which way.
    ///
    /// Counted off [`picks`](Self::picks) rather than off the level, so it stays
    /// **honest when the pool is thinner than the level asks for** — today the easier
    /// side has three candidates and the harder side five, and a −2 that could only
    /// find one rule to bend must not claim two. It says nothing about *which* rules:
    /// the seed is not decided until the run starts, and the resolved set is the Level
    /// info tab's to show once it is.
    #[must_use]
    pub fn blurb(self) -> &'static str {
        match (self.picks(), self.direction()) {
            // The baseline reads in the *same* terms as every other stop — "no rules
            // bent either way" — rather than naming the mode it happens to be. A
            // player reading the slider has no way to know what "quick play,
            // unchanged" is unchanged *from*; the count they can compare against the
            // stop either side of it needs no such context.
            (0, _) | (_, None) => "no rules bent either way",
            (1, Some(ModifierDirection::Easier)) => "one rule bent your way",
            (_, Some(ModifierDirection::Easier)) => "two rules bent your way",
            (1, Some(ModifierDirection::Harder)) => "one rule bent against you",
            (_, Some(ModifierDirection::Harder)) => "two rules bent against you",
        }
    }

    /// Which way this position bends the run, or `None` at the baseline — the
    /// direction the pool is filtered on.
    #[must_use]
    pub fn direction(self) -> Option<ModifierDirection> {
        match self.level() {
            0 => None,
            level if level > 0 => Some(ModifierDirection::Harder),
            _ => Some(ModifierDirection::Easier),
        }
    }

    /// How many modifiers the draw will actually switch on: `|level|`, or the whole
    /// directed pool when it holds fewer than that.
    ///
    /// **A short pool takes what exists** rather than looping to fill the quota or
    /// panicking. The easier side is two candidates deep today, so −2 is exhaustive
    /// and −1 a genuine draw of one; a pool that ever shrank below the level would
    /// simply hand back everything it has, and [`blurb`](Self::blurb) would say so.
    #[must_use]
    pub fn picks(self) -> usize {
        match self.direction() {
            None => 0,
            Some(direction) => (self.level().unsigned_abs() as usize).min(pool_size(direction)),
        }
    }

    /// **The draw** (#297): the modifiers this position contributes for `seed` — a
    /// pure function of the two, and the only randomness in the whole axis.
    ///
    /// Returns the *contribution*, not the run's resolved set: it is composed onto
    /// the quick-play base by [`LevelSeed::quick_play_at`](crate::LevelSeed), which is
    /// the one place a run's modifiers are settled. Every entry it applies bends
    /// [`direction`](Self::direction)-wards, so the contribution can only ever add
    /// pressure to a `+N` or relief to a `−N`.
    ///
    /// The draw itself is [`draw_from_pool`], shared with the campaign alert (#210) so
    /// the two sources that *pick* a rule cannot come to filter the pool differently.
    /// What stays here is what is this axis's own: how many picks a position makes, and
    /// the salt that keeps them off every other stream.
    #[must_use]
    pub fn draw(self, seed: u64) -> LevelModifiers {
        let base = LevelModifiers::default();
        let Some(direction) = self.direction() else {
            // The baseline draws nothing and touches no stream — quick play at
            // Standard is byte-identical to quick play before there was an axis.
            return base;
        };
        draw_from_pool(base, direction, self.picks(), seed ^ DIFFICULTY_STREAM_SALT)
    }

    /// One step towards the easier end, staying put at it — the slider's left, which
    /// **clamps rather than wrapping**: a slider that jumped from its easiest stop to
    /// its hardest would be a trap on a control whose whole job is to be nudged.
    #[must_use]
    pub fn easier(self) -> Self {
        Self::from_level(i32::from(self.level()) - 1)
    }

    /// One step towards the harder end, staying put at it.
    #[must_use]
    pub fn harder(self) -> Self {
        Self::from_level(i32::from(self.level()) + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifiers::{ActiveModifier, POOL};
    use crate::Cell;

    /// A spread of seeds wide enough that a draw which ignored its seed, or one that
    /// only ever landed on the first pool entry, shows up.
    const SEEDS: [u64; 8] = [0, 1, 42, 8371, 4242, 99_991, 123_456, u64::MAX];

    /// How many of `modifiers`' active entries bend each way — the §2.3 directional
    /// assertion is stated in terms of these two counts.
    fn by_direction(modifiers: LevelModifiers) -> (usize, usize) {
        let active = modifiers.active();
        let count = |want| {
            active
                .iter()
                .filter(|m: &&ActiveModifier| m.direction == want)
                .count()
        };
        (
            count(ModifierDirection::Harder),
            count(ModifierDirection::Easier),
        )
    }

    /// The axis is five clamped positions, `0` the baseline, and an integer off the
    /// end lands on the nearest stop rather than naming a position nothing draws for.
    #[test]
    fn the_axis_is_five_clamped_positions_around_the_baseline() {
        assert_eq!(Difficulty::default(), Difficulty::Standard);
        assert_eq!(Difficulty::Standard.level(), 0);
        let levels: Vec<i8> = Difficulty::ALL.into_iter().map(Difficulty::level).collect();
        assert_eq!(levels, vec![-2, -1, 0, 1, 2], "easiest first");
        for position in Difficulty::ALL {
            assert_eq!(
                Difficulty::from_level(i32::from(position.level())),
                position
            );
        }
        // Clamped, not wrapped, at both ends and far past them.
        assert_eq!(Difficulty::from_level(3), Difficulty::MuchHarder);
        assert_eq!(Difficulty::from_level(i32::MAX), Difficulty::MuchHarder);
        assert_eq!(Difficulty::from_level(-3), Difficulty::MuchEasier);
        assert_eq!(Difficulty::from_level(i32::MIN), Difficulty::MuchEasier);
        assert_eq!(Difficulty::MuchHarder.harder(), Difficulty::MuchHarder);
        assert_eq!(Difficulty::MuchEasier.easier(), Difficulty::MuchEasier);
        assert_eq!(Difficulty::Standard.harder(), Difficulty::Harder);
        assert_eq!(Difficulty::Standard.easier(), Difficulty::Easier);
    }

    /// **The baseline draws nothing.** `Standard` is exactly today's quick play, which
    /// is the promise that makes the axis safe to add to a shipped mode at all.
    #[test]
    fn the_baseline_draws_nothing_at_all() {
        for seed in SEEDS {
            assert_eq!(Difficulty::Standard.draw(seed), LevelModifiers::default());
        }
        assert_eq!(Difficulty::Standard.picks(), 0);
        assert_eq!(Difficulty::Standard.direction(), None);
    }

    /// **Same `(level, seed)` → the same set, every time** (§12.4). The draw is a pure
    /// function of the two: it is what lets the resolved set travel in the token while
    /// the difficulty number does not.
    #[test]
    fn the_same_level_and_seed_draw_the_same_set() {
        for position in Difficulty::ALL {
            for seed in SEEDS {
                assert_eq!(
                    position.draw(seed),
                    position.draw(seed),
                    "{position:?} at seed {seed} is not a function of its inputs",
                );
            }
        }
        // …and different seeds genuinely draw differently, or the "seeded" is a lie.
        // One pick from a pool of three: over this spread, at least two distinct sets.
        let drawn: std::collections::BTreeSet<_> = SEEDS
            .iter()
            .map(|&seed| format!("{:?}", Difficulty::Harder.draw(seed)))
            .collect();
        assert!(drawn.len() > 1, "the draw ignores its seed: {drawn:?}");
    }

    /// **The §2.3 directional assertion.** A `+N` draw yields a set at least as hard
    /// as the baseline and a `−N` draw one at least as easy: the count of modifiers
    /// bending the *asked-for* way goes up by exactly [`Difficulty::picks`], and the
    /// count bending the other way does not move at all. No draw can cancel the
    /// direction it was asked for.
    #[test]
    fn no_draw_can_cancel_the_direction_it_was_asked_for() {
        for position in Difficulty::ALL {
            for seed in SEEDS {
                let (harder, easier) = by_direction(position.draw(seed));
                match position.direction() {
                    None => assert_eq!((harder, easier), (0, 0), "the baseline bends nothing"),
                    Some(ModifierDirection::Harder) => {
                        assert_eq!(harder, position.picks(), "{position:?} at seed {seed}");
                        assert_eq!(easier, 0, "a harder draw let an easier rule in");
                    }
                    Some(ModifierDirection::Easier) => {
                        assert_eq!(easier, position.picks(), "{position:?} at seed {seed}");
                        assert_eq!(harder, 0, "an easier draw let a harder rule in");
                    }
                }
            }
        }
    }

    /// A pool with fewer candidates than the level asks for **takes what exists**,
    /// deterministically — it does not loop to fill the quota and it does not panic.
    ///
    /// **Both sides are now deeper than the axis reaches** (#232 gave the easier side
    /// its third candidate, appendix 30), so ±2 draws *distinct* pairs that differ by
    /// seed rather than the one exhaustive set every seed used to get on the easier
    /// end. The claim the test exists for is unchanged and stated generally below —
    /// never more picks than the pool holds — and it is the case that would matter
    /// again the moment a pool shrank.
    #[test]
    fn a_short_pool_takes_what_exists() {
        assert_eq!(Difficulty::MuchEasier.picks(), 2);
        assert_eq!(Difficulty::MuchHarder.picks(), 2);
        for seed in SEEDS {
            let drawn = Difficulty::MuchEasier.draw(seed);
            assert_eq!(drawn.active().len(), 2, "two distinct easier rules");
        }
        // The easier side is a genuine draw now: over this spread of seeds, −2 lands
        // on more than one pair. A pool that had gone back to exactly two deep, or a
        // draw that ignored its seed, shows up here as a single set.
        let easier_pairs: std::collections::BTreeSet<_> = SEEDS
            .iter()
            .map(|&seed| format!("{:?}", Difficulty::MuchEasier.draw(seed).active()))
            .collect();
        assert!(
            easier_pairs.len() > 1,
            "−2 is exhaustive again: {easier_pairs:?}"
        );
        // The general claim, over every position: never more than the pool holds.
        for position in Difficulty::ALL {
            let bound = position.direction().map_or(0, pool_size);
            assert!(position.picks() <= bound);
            for seed in SEEDS {
                assert_eq!(position.draw(seed).active().len(), position.picks());
            }
        }
    }

    /// **The pool is exactly the two questions §12.6 asks of an entry** (#518): is it a
    /// difficulty change, and does it bend in a documented direction? Whether the §13.2
    /// bot can weigh it is deliberately *not* a third — the sim is a smoke detector, not
    /// a judge (§13.4), and a modifier only a human can rule on is still a modifier.
    ///
    /// So this pins the shape rather than a membership list that would churn: every
    /// entry carries a direction, the directed halves partition the pool, and the axis
    /// can actually fill the picks it advertises at every stop.
    #[test]
    fn the_pool_is_directed_and_deep_enough_for_every_stop() {
        assert_eq!(
            pool_size(ModifierDirection::Harder) + pool_size(ModifierDirection::Easier),
            POOL.len(),
            "every entry bends one way or the other — there is no neutral row",
        );
        for position in Difficulty::ALL {
            let Some(direction) = position.direction() else {
                assert_eq!(position.picks(), 0, "the baseline draws nothing");
                continue;
            };
            assert_eq!(
                position.picks(),
                (position.level().unsigned_abs() as usize).min(pool_size(direction)),
                "{position:?} must not advertise a pick the pool cannot fill",
            );
        }
    }

    /// **What a difficulty draw may do to the building** (#518) — three tiers, and the
    /// point of pinning them is that a modifier can only move between tiers deliberately.
    ///
    /// The draw takes a salted sub-stream so that *reading* it costs the generator
    /// nothing. That used to make a flat promise — one seed, one facility, whatever the
    /// difficulty — and admitting two generation-reaching entries turned it into a
    /// graded one:
    ///
    /// 1. **The doors move the building.** `automatic doors` is consumed by the carve
    ///    (§12.6), and the divergence is not confined to doorways: measured over the
    ///    sweep, whole features relocate — on seed 42 a **duct mouth** lands somewhere
    ///    else entirely. A ±N comparison that draws it is comparing two facilities, not
    ///    two rulesets.
    /// 2. **The locked room re-roles cells the carve already cut.** `prize room locked`
    ///    folds a doorway's two hinges into its panel span (§10.4/#236) and touches
    ///    nothing else — the building is the same building, and every differing cell
    ///    belongs to a door the lock is on.
    /// 3. **Everything else leaves the grid byte-identical**, which is the property the
    ///    ±N arms are worth comparing on.
    ///
    /// A failure here is not a test to relax. Tier 3 breaking means a new entry reaches
    /// generation and the guarantee needs restating; tier 2 breaking means the lock has
    /// started moving geometry, which §10.6's own assertions are stated against.
    #[test]
    fn a_difficulty_draw_moves_the_building_only_where_it_is_meant_to() {
        use crate::generate::generate_level;
        use crate::place::LevelConfig;
        use crate::render::ascii_grid;
        use crate::Rng;

        let build = |modifiers: &LevelModifiers, seed: u64| {
            let (layout, _) = generate_level(&LevelConfig::V1, modifiers, &mut Rng::new(seed))
                .unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));
            let keyed: std::collections::HashSet<Cell> = layout
                .regions()
                .doors()
                .filter(|(_, d)| d.is_keyed())
                .flat_map(|(_, d)| d.cells().collect::<Vec<_>>())
                .collect();
            (ascii_grid(layout.facility()), keyed)
        };
        let differing = |a: &[String], b: &[String]| -> Vec<Cell> {
            a.iter()
                .enumerate()
                .flat_map(|(y, row)| {
                    row.chars()
                        .zip(b[y].chars())
                        .enumerate()
                        .filter(|(_, (p, q))| p != q)
                        .map(move |(x, _)| Cell::new(x as u32, y as u32))
                        .collect::<Vec<_>>()
                })
                .collect()
        };

        let (mut saw_doors, mut saw_lock, mut saw_quiet) = (false, false, false);
        for seed in crate::test_support::seed_sweep(24) {
            let (baseline, _) = build(&LevelModifiers::default(), seed);
            for position in Difficulty::ALL {
                let drawn = position.draw(seed);
                let (grid, keyed) = build(&drawn, seed);
                let diff = differing(&baseline, &grid);
                if drawn.automatic_doors {
                    saw_doors = true; // tier 1: the building itself may move
                    continue;
                }
                if drawn.prize_room_locked {
                    saw_lock = true;
                    assert!(
                        !keyed.is_empty() && diff.iter().all(|c| keyed.contains(c)),
                        "seed {seed}, {position:?}: the lock moved {} cells that are not \
                         its own doorways — it must re-role a doorway, never carve one",
                        diff.iter().filter(|c| !keyed.contains(c)).count(),
                    );
                    continue;
                }
                saw_quiet = true; // tier 3
                assert!(
                    diff.is_empty(),
                    "seed {seed}, {position:?}: {} cells moved, so the ±N arms are no \
                     longer one building — a pool entry has started reaching generation",
                    diff.len(),
                );
            }
        }
        assert!(
            saw_doors && saw_lock && saw_quiet,
            "the sweep missed a tier (doors {saw_doors}, lock {saw_lock}, quiet {saw_quiet})",
        );
    }

    /// The blurb counts the rules the position will **actually** bend, so a level
    /// deeper than its pool cannot overstate what it is about to do.
    #[test]
    fn the_blurb_counts_the_rules_the_draw_will_really_bend() {
        for position in Difficulty::ALL {
            let blurb = position.blurb();
            let expected = match position.picks() {
                0 => "no rules",
                1 => "one rule",
                _ => "two rules",
            };
            assert!(blurb.contains(expected), "{position:?}: {blurb}");
            match position.direction() {
                None => {}
                Some(ModifierDirection::Easier) => assert!(blurb.contains("your way"), "{blurb}"),
                Some(ModifierDirection::Harder) => {
                    assert!(blurb.contains("against you"), "{blurb}")
                }
            }
        }
        // Every position reads differently — a slider whose stops all said the same
        // thing would be five ways to press the same button.
        let labels: std::collections::BTreeSet<_> =
            Difficulty::ALL.into_iter().map(Difficulty::label).collect();
        assert_eq!(labels.len(), Difficulty::ALL.len());
    }
}
