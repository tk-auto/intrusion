//! **The archive gate** (§4.6/§14 v3/#573) — what the run's stars make of its ending.
//!
//! §4.6 scores every facility the run walked out of, out of three, and then says the
//! scores grant nothing: *"a verdict a system can spend is a currency the player starts
//! playing toward instead of playing well."* This module is the **one** exception the
//! design has chosen. The stars a campaign has banked decide how hard its **archive** is:
//! below the first threshold the terminus is drawn three harder rules, each threshold
//! cleared takes one off, and a run that cleared them all walks into the building the
//! seed built and nothing more.
//!
//! That is what makes a per-facility score matter to the **run**. Without it, three stars
//! are a report card handed out between raids, read once and dropped; with it, the raid
//! you half-finished in facility two is standing in the last building waiting for you.
//!
//! # It accumulates, and #210's alert deliberately does not
//!
//! The campaign alert *"reaches one hop and does not accumulate… a level that cannot add
//! to itself cannot spiral, which is why there is no decay rate and no floor to tune"*
//! (§14 v3 **[SETTLED]**). This gate breaks that rule on purpose, and the difference that
//! makes it safe is **which direction the loop runs in**.
//!
//! The alert is a *feedback* loop: being loud makes the next facility harder, which makes
//! being loud there more likely. Left to accumulate it compounds, and §2.2's warning about
//! escalation staying recoverable is exactly about that shape. The gate is not a loop at
//! all — it is a **tally with one reader, read once, at the end**. A bad score never makes
//! the next facility harder, so it cannot make the next score worse; nothing about
//! arriving at four stars changes what facility five is. What accumulates is the
//! *statement*, not the pressure, and a statement read once cannot compound.
//!
//! Three properties have to hold for that to stay true, and each is asserted:
//!
//! - **It is never a lock.** The gate draws rules; it never touches the exit
//!   ([`IntelGate`](crate::IntelGate) is not in the §12.6 pool), never stops the archive
//!   generating, and never makes the run unwinnable. Three harder rules is a hard raid.
//! - **The player can still earn.** Stars come from facilities and the map keeps offering
//!   them, so falling short is a reason to raid one more rather than a sentence — which
//!   is precisely the decision the map's gauge exists to put in front of the player
//!   *before* they walk in (§14 v3: *legible before the choice, not after*).
//! - **The thresholds are generous enough that most runs clear one.** If the modal run
//!   arrives at three rules the gate is a tax rather than a curve, and the numbers are
//!   wrong. They are **[START]** and they cannot be guessed — see [`THRESHOLDS`].
//!
//! # A star removes a rule; it does not reroll the set
//!
//! The draw is [`Rng::choose_n`](crate::Rng::choose_n), a partial Fisher–Yates whose
//! first *n* picks do not depend on *n*. So the two rules a six-star run faces are the
//! **first two** of the three a five-star run would have faced: crossing a threshold takes
//! the last rule off the pile rather than dealing a fresh hand. That is what lets the
//! brief name the rules honestly before the player has finished earning — the one they are
//! working to remove is the one they were already shown.

use crate::modifiers::{
    draw_from_pool_beyond, pool_size, ActiveModifier, LevelModifiers, ModifierDirection,
};
use crate::score::Axis;

/// **The most rules the gate can deal** (#573) — **[START]**.
///
/// Three, because that is what fits: the archive spends one modifier slot on its own
/// composite (§12.6/#565) and the campaign alert may deal one more, so three here puts
/// the worst facility the game can produce at exactly `MODIFIER_CAP`'s five active rules
/// and no further — a terminus that can still be written down as a level-seed token
/// (§12.7). If play says the gate is a tax rather than a curve, the ticket's second lever
/// is this number rather than a decay rule: a run-length accumulation with a decay on it
/// would be unreadable.
pub const GATE_RULES_MAX: usize = 3;

/// **The star totals that take a rule off the archive** (#573) — all **[START]**, and the
/// numbers this whole mechanism is least sure of.
///
/// Ascending, one per rule the gate can deal: clear the first and the archive carries two
/// rules instead of three, clear them all and it carries none.
///
/// **They are placeholders and are meant to move.** What the right numbers are depends on
/// how often a real run earns each star, which is a thing nobody knows until played runs
/// and a campaign-scale sim batch say so — the ticket says as much, and this is the
/// `type:tuning` follow-up it predicts. What can be said now is the shape: six facilities
/// at three stars apiece is a ceiling of **18** ([`ceiling`](ArchiveGate::ceiling)), a good
/// run lands well short of it, and the first threshold sits at a third of the ceiling so
/// that a run which took two facilities seriously has already bought something.
pub const THRESHOLDS: [u32; GATE_RULES_MAX] = [6, 10, 14];

// Ascending, or "cleared so far" below would be counting an unordered list. A threshold
// table out of order is the kind of edit that produces a plausible-looking wrong number
// rather than a failure, so it fails the build instead.
const _: () = {
    let mut i = 1;
    while i < THRESHOLDS.len() {
        assert!(
            THRESHOLDS[i - 1] < THRESHOLDS[i],
            "the archive gate's thresholds must ascend",
        );
        i += 1;
    }
};

// The gate can only ever deal what the harder side of the §12.6 pool holds, and dealing
// fewer rules than the gauge names would be the facade this module is careful about
// everywhere else. The pool is far wider than three today; this is what says so if it
// ever is not.
const _: () = assert!(
    GATE_RULES_MAX <= pool_size(ModifierDirection::Harder),
    "the archive gate asks the directed pool for more harder rules than it holds",
);

/// Separates the gate's draw from every other use of the run seed (§12.4) — the salt
/// [`loudness`](super::loudness) keeps two of, for the same reason: a question added here
/// must not shift the answer to one already asked.
const GATE_STREAM_SALT: u64 = 0x_57A5_0000_57A5_0000;

/// **The archive gate**: a run's stars, the ceiling they were earned against, and what
/// the two make of the terminus (#573).
///
/// A value rather than a pair of loose numbers because both surfaces that report it — the
/// map's gauge and the facility brief — need the same three answers, and a screen
/// re-deriving *"how many rules is that?"* from a threshold table is the second copy of a
/// rule §11.3 exists to forbid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ArchiveGate {
    stars: u32,
    ceiling: u32,
}

impl ArchiveGate {
    /// The gate a run standing on `stars` faces, on a country whose facilities before the
    /// terminus are worth `ceiling` between them.
    ///
    /// The ceiling is carried rather than computed from [`DEPTH_TO_ARCHIVE`] so the gauge
    /// is honest on a country that is not the standard one: `Campaign::to_depth` is a real
    /// knob (§14 v3 lists the depth as **[START]**), and a bar drawn to eighteen on a
    /// four-facility run would be promising ground that does not exist.
    ///
    /// [`DEPTH_TO_ARCHIVE`]: super::DEPTH_TO_ARCHIVE
    #[must_use]
    pub fn new(stars: u32, ceiling: u32) -> Self {
        Self { stars, ceiling }
    }

    /// What the run has banked (§4.6) — the sum over every facility it walked out of.
    pub fn stars(self) -> u32 {
        self.stars
    }

    /// What this run's facilities are worth between them, the archive excluded — the
    /// gauge's span, and the only thing the run is measured against that is a fact about
    /// the *country* rather than about the player.
    pub fn ceiling(self) -> u32 {
        self.ceiling
    }

    /// **How many harder rules the archive carries**, 0…[`GATE_RULES_MAX`]: one off for
    /// each of [`THRESHOLDS`] the run has reached.
    ///
    /// The whole of the gate's arithmetic, in one place, so no screen has to hold a copy
    /// of the table to say what it is showing.
    pub fn rules(self) -> usize {
        let cleared = THRESHOLDS
            .iter()
            .filter(|&&threshold| self.stars >= threshold)
            .count();
        GATE_RULES_MAX - cleared
    }

    /// **The next threshold the run has not reached**, or `None` once every rule is off —
    /// what turns the gauge from a readout into a decision (*"one more raid"*).
    pub fn next_threshold(self) -> Option<u32> {
        THRESHOLDS
            .iter()
            .copied()
            .find(|&threshold| self.stars < threshold)
    }

    /// **The contribution the gate makes to the archive** (§12.6) — [`rules`](Self::rules)
    /// picks from the *harder* side of the directed pool, or `None` for a run that has
    /// cleared the lot.
    ///
    /// `given` is what the facility already plays: the terminus's own composite expansion,
    /// and whatever the campaign alert (#210) has drawn onto it. Passing it is what keeps
    /// the count the gauge names equal to the count the building has — see
    /// [`draw_from_pool_beyond`], where the objection is argued.
    ///
    /// Built over [`LevelModifiers::neutral`] like every contributing source, not over the
    /// default: a source built from the game's baseline would silently ask for the §4.5
    /// intel gate. It cannot ask for one anyway — no pool entry touches the gate — and the
    /// pair of facts is what makes *"the ending is never unreachable"* true by
    /// construction rather than by review.
    ///
    /// **Never a private list** (§12.6). Every rule the archive can be dealt is a rule the
    /// game already has, with a documented direction and a row on the Level info tab; a
    /// gate with its own vocabulary would be a second difficulty system wearing the
    /// modifier seam's clothes.
    #[must_use]
    pub fn contribution(self, run_seed: u64, given: LevelModifiers) -> Option<LevelModifiers> {
        let picks = self.rules();
        (picks > 0).then(|| {
            draw_from_pool_beyond(
                LevelModifiers::neutral(),
                given,
                ModifierDirection::Harder,
                picks,
                run_seed ^ GATE_STREAM_SALT,
            )
        })
    }

    /// **The rules the archive is carrying, named** — what the facility brief lists before
    /// the press that commits to entering (§14 v3: *legible before the choice, not after*).
    ///
    /// Empty for a run that has cleared every threshold, which the brief says in words
    /// rather than by drawing nothing.
    ///
    /// Derived from the contribution rather than from a second walk of the pool, so the
    /// rules that are *named* and the rules that are *dealt* cannot come apart. The
    /// subtraction is what a contribution costs: it is built over
    /// [`LevelModifiers::neutral`], whose own active set is not empty — it names the
    /// campaign's §4.5 gate — so what the draw added is what is left once that row is
    /// taken off.
    #[must_use]
    pub fn rules_drawn(self, run_seed: u64, given: LevelModifiers) -> Vec<ActiveModifier> {
        let Some(drawn) = self.contribution(run_seed, given) else {
            return Vec::new();
        };
        let quiet = LevelModifiers::neutral().active();
        drawn
            .active()
            .into_iter()
            .filter(|rule| !quiet.contains(rule))
            .collect()
    }
}

/// How many stars one completed facility is worth (§4.6) — the axes, counted, so the
/// gauge's span moves with the score rather than with a number written twice.
pub const STARS_PER_FACILITY: u32 = Axis::ALL.len() as u32;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifiers::{Composite, IntelGate};

    /// A gate on the standard country's ceiling — six facilities at three apiece.
    fn gate(stars: u32) -> ArchiveGate {
        ArchiveGate::new(
            stars,
            STARS_PER_FACILITY * crate::campaign::DEPTH_TO_ARCHIVE,
        )
    }

    /// What the archive already plays before the gate says anything — the composite's
    /// expansion, which is what `Campaign::level_at` hands the draw.
    fn given() -> LevelModifiers {
        Composite::Archive.expansion()
    }

    /// **Every boundary of the [START] table**, pinned — the acceptance criterion stated
    /// as the test it asks for, so a tuning change to [`THRESHOLDS`] is a visible edit
    /// here rather than a silent one in play.
    #[test]
    fn each_threshold_cleared_takes_one_rule_off_the_archive() {
        // Below the first: the full three.
        for stars in 0..THRESHOLDS[0] {
            assert_eq!(gate(stars).rules(), 3, "{stars} stars");
        }
        for stars in THRESHOLDS[0]..THRESHOLDS[1] {
            assert_eq!(gate(stars).rules(), 2, "{stars} stars");
        }
        for stars in THRESHOLDS[1]..THRESHOLDS[2] {
            assert_eq!(gate(stars).rules(), 1, "{stars} stars");
        }
        // And at the top of the table, nothing — including a perfect run, which is the
        // ceiling and not a special case.
        for stars in THRESHOLDS[2]..=gate(0).ceiling() {
            assert_eq!(gate(stars).rules(), 0, "{stars} stars");
        }
        assert_eq!(gate(0).ceiling(), 18, "six facilities, three stars apiece");

        // The gauge's other question: what is the run reaching for?
        assert_eq!(gate(0).next_threshold(), Some(THRESHOLDS[0]));
        assert_eq!(gate(THRESHOLDS[0]).next_threshold(), Some(THRESHOLDS[1]));
        assert_eq!(gate(THRESHOLDS[2]).next_threshold(), None);
    }

    /// **The draw is exactly what the gate promised**, bends the way it said, and never
    /// deals the same rule twice (§2.3/§12.6/#573).
    #[test]
    fn the_gate_deals_the_rules_it_names_and_every_one_of_them_lands() {
        for seed in [1_u64, 42, 8371, 123_456, u64::MAX] {
            for stars in 0..=18 {
                let gate = gate(stars);
                let named = gate.rules_drawn(seed, given());
                assert_eq!(named.len(), gate.rules(), "seed {seed}, {stars} stars");
                // Every one of them harder, which is the §2.3 guarantee the pool's own
                // filter makes true by construction.
                assert!(
                    named
                        .iter()
                        .all(|rule| rule.direction == ModifierDirection::Harder),
                    "seed {seed}: the gate dealt a rule that bends the run's way",
                );
                // …and no rule twice: `choose_n` draws without replacement, and a gauge
                // saying three over two distinct rules would be the count lying.
                let mut names: Vec<&str> = named.iter().map(|rule| rule.name).collect();
                names.sort_unstable();
                let distinct = names.len();
                names.dedup();
                assert_eq!(names.len(), distinct, "seed {seed}: a rule was dealt twice");

                // **Every rule actually lands.** The archive already locks its prize room
                // and already stands at the guard knob's reach, and both are harder-pool
                // entries — so this is the assertion that the filter is doing its job and
                // the gauge's number is the building's number.
                let Some(drawn) = gate.contribution(seed, given()) else {
                    assert_eq!(gate.rules(), 0);
                    continue;
                };
                let with = given().union(drawn);
                assert_ne!(
                    with,
                    given(),
                    "seed {seed}: the gate's rules changed nothing about the archive",
                );
                // And it never asks about the exit: the ending stays reachable because
                // there is no pool entry that could close it, not because someone checked.
                assert_eq!(drawn.intel_to_exit, IntelGate::None, "seed {seed}");
            }
        }
    }

    /// **Crossing a threshold takes a rule off the pile; it does not deal a new hand** —
    /// the `choose_n` prefix property, stated as what the brief actually relies on: the set
    /// shrinks by one and every rule still standing was already named.
    ///
    /// Stated as a **subset** rather than as a prefix because the display order is not the
    /// draw order — [`LevelModifiers::active`] walks the fields, so which rule came off is
    /// a fact about the draw and not about the list. What the player is promised is that
    /// earning a star never *adds* a rule they had not been shown, and that is this.
    #[test]
    fn a_star_removes_a_rule_rather_than_rerolling_them() {
        for seed in [1_u64, 42, 8371] {
            let named = |stars: u32| -> Vec<&'static str> {
                gate(stars)
                    .rules_drawn(seed, given())
                    .iter()
                    .map(|rule| rule.name)
                    .collect()
            };
            let mut previous = named(0);
            for &threshold in &THRESHOLDS {
                let now = named(threshold);
                assert_eq!(now.len() + 1, previous.len(), "seed {seed}");
                assert!(
                    now.iter().all(|rule| previous.contains(rule)),
                    "seed {seed}: the hand was reshuffled — {previous:?} became {now:?}",
                );
                previous = now;
            }
            assert!(previous.is_empty(), "seed {seed}: every threshold cleared");
        }
    }

    /// **Same run seed, same gate** (§12.4) — and two countries do not agree by accident.
    #[test]
    fn the_gate_is_a_function_of_the_run_seed_and_the_stars() {
        for seed in [7_u64, 99, 4242] {
            for stars in [0, 6, 10] {
                assert_eq!(
                    gate(stars).contribution(seed, given()),
                    gate(stars).contribution(seed, given()),
                );
            }
        }
        let hands: std::collections::BTreeSet<String> = [1_u64, 42, 8371, 123_456, u64::MAX]
            .iter()
            .map(|&seed| format!("{:?}", gate(0).rules_drawn(seed, given())))
            .collect();
        assert!(hands.len() > 1, "the draw ignores its seed: {hands:?}");
    }
}
