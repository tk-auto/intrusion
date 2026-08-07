//! **The three stars** (§15 Q4/§14 v2, #563): what a completed facility was worth,
//! said one axis at a time.
//!
//! A raid that got out is scored on three independent questions — was it *fast*, was it
//! *quiet*, did it take *everything* — and each one is worth one star. The total is the
//! count of the three, so the range is 0–3 and a run that crawled out slow, loud and
//! half-empty earned none of them. **Escaping is not the first star**; a floor that made
//! winning worth one would spend the most legible mark on the fact the player already
//! knows.
//!
//! # Why three marks and not one number
//!
//! A weighted total mapped onto tiers would say *"two stars"* and nothing else. Three
//! independent stars say **which one you missed**, which is the only thing a rating is
//! good for in a game with no meta-progression: §2.2's promise is that the one thing
//! carried between runs is what you learned, and *"you had it all but you were seen"* is
//! a sentence you can act on next time. That is also why the axes are never summed into
//! a weighting — a run cannot trade speed for silence, because the two questions are
//! about different mistakes.
//!
//! # It grants nothing
//!
//! Nothing in the game reads a [`Score`] — not an ability, not a modifier, not the
//! wallet, and nothing at all across runs (§2.2). It is a **verdict**, and a verdict a
//! system can spend is a currency the player starts playing toward instead of playing
//! well. The one exception the design has chosen is the campaign's archive gate (#573,
//! v3), which will read the run's accumulated total and nothing more; until it lands the
//! set of readers is empty, and [`tests`](self) keeps it that way deliberately rather
//! than by accident.
//!
//! # And it settles §15 Q4's other half
//!
//! Q4 asked whether takedowns should **cost score**. They do, and not by a rule of their
//! own: a takedown leaves a body (§7.2), a found body is a rung-3 trigger (§7.3), and any
//! rung above zero costs the stealth star. Aggression is priced through the clock the
//! design already built — one mechanism, no leaderboard, and the ghost↔aggressive
//! spectrum falls out of it.

use serde::{Deserialize, Serialize};

use crate::verdict::{RunStats, Verdict};

/// **Par**: the base allowance every facility gets, before its contents are counted
/// (§14 v2) — **[START]**.
///
/// It is charged **per cell of span** (width + height) rather than as a flat number,
/// because the raid a par is measuring is mostly *crossing the building*: in and out of a
/// 40×40 is eighty cells of walking, and the size is screen-bound (§10.2/§11.4) so the
/// span is a fair stand-in for how far the ground goes. **Two** turns per cell of span,
/// not one: the way in is not a straight line, because a raid that walked one would be
/// walking through guards.
pub const PAR_SPAN: u32 = 2;

/// What each **intel console** adds to par (§14 v2) — **[START]**.
///
/// **This is the term that carries the number**, and it is large on purpose: a console is
/// not a detour off a route, it is a *search* — the room it stands in is fogged until you
/// have been in it (§11.5a), and exploration is most of a raid (§10). Finding one, taking
/// it, and getting back out to look for the next is most of what the clock is spent on.
pub const PAR_PER_INTEL: u32 = 90;

/// What each **equipment cache** adds to par (§8.3/#209) — **[START]**.
///
/// Less than a console's, because a crate is not a thing the level asks for: it is a
/// detour the player chooses, and the thoroughness star is what pays for taking it. Par
/// allows for the detour rather than funding it comfortably.
pub const PAR_PER_CACHE: u32 = 50;

/// **A facility's par turn count** (§14 v2/#563): the allowance the speed star is
/// measured against, derived from the building's own contents.
///
/// Derived, never flat — a *Vault* has one more of everything (§14 v3) and legitimately
/// takes longer than an *Outpost*, so holding both to one constant would read as the game
/// being arbitrary, which is worse than having no speed star at all. The three constants
/// above are the whole formula, and every one of them is **[START]**.
///
/// `span` is `width + height`; `intel` and `caches` are what the facility actually
/// **holds**, not what the recipe asked for — a par is a fact about the building the
/// player is standing in.
///
/// # The numbers come from measurement, and the first set was badly wrong
///
/// Par shipped at `span + 25×consoles + 15×crates`, which put quick play on **155** — and
/// **no** all-intel run has ever come in under it. A human playing with the fog lifted
/// *and* the guards blinded missed it, which is about as close to the optimal walk as the
/// game allows, and a 100-seed bot batch at the quick-play gate agreed: zero speed stars,
/// median 428 turns. The mistake was measuring the first numbers against the **sim's**
/// gate (`AtLeastOne`), where a run takes one console and leaves — a completely different
/// job from taking all three. Appendix 61 §4 records it.
///
/// The tuned set puts quick play on **430**, against that 428 median: a threshold roughly
/// half of competent all-intel runs clear, which is what a demanding-but-reachable star
/// looks like.
pub fn par_for(span: u32, intel: usize, caches: usize) -> u32 {
    PAR_SPAN * span + PAR_PER_INTEL * intel as u32 + PAR_PER_CACHE * caches as u32
}

/// One of the three axes a completed facility is scored on (§15 Q4/#563).
///
/// An enum rather than three booleans in a row, so every surface that reports a score
/// walks [`ALL`](Axis::ALL) and cannot quietly print two of them: the whole value of the
/// readout is that it names the axis you missed, and a missing row is exactly the failure
/// mode a hand-written list has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    /// **You got out inside the facility's par** ([`par_for`]).
    Speed,
    /// **Nobody ever knew**: the run ended at security condition 0 (§7.3).
    ///
    /// The ladder never decays, so this is a claim about the *whole* raid and not about
    /// its last turn — one sighting or one missed ping anywhere in it is the star gone.
    /// Demanding on purpose.
    Stealth,
    /// **You took everything the building had**: every intel console *and* every
    /// equipment cache (§4.5/§8.3).
    ///
    /// Read off the facility's **contents**, never off the win condition: the exit gate
    /// is a level modifier (`intel_to_exit`, §4.5/§12.6), so a campaign facility that
    /// opens on the **minimum haul** — one objective, console or crate (#574) — still has
    /// the rest of its consoles and crates standing in it, and this still means something.
    /// Reading the star off the gate instead would make it free in every mode that asks
    /// for less than everything.
    Thoroughness,
}

impl Axis {
    /// Every axis, in the order they are reported. Speed, stealth, thoroughness — the
    /// order the design states them in, so the screens and this agree without a second
    /// list to keep in step.
    pub const ALL: [Axis; 3] = [Axis::Speed, Axis::Stealth, Axis::Thoroughness];

    /// What the axis is **called on screen** (§11.8).
    ///
    /// No row in §11.8's table is needed for these: *speed*, *stealth* and *haul* are the
    /// world's own words for what they measure, and the design has no different word for
    /// any of them. *Haul* rather than *thoroughness* on one count — the screens are
    /// 40 columns (§11.4) and the run's word for "everything the building had" is what
    /// you walked out with.
    ///
    /// `const`, so the end screen's alignment column can be measured at compile time —
    /// the same discipline the campaign map's own width bounds use.
    pub const fn label(self) -> &'static str {
        match self {
            Axis::Speed => "speed",
            Axis::Stealth => "stealth",
            Axis::Thoroughness => "haul",
        }
    }

    /// The one line that says **what this star was for** — printed beside the axis on the
    /// end screen, so a missed star is a thing the player can go and do differently.
    ///
    /// It states the *condition*, not the outcome: "inside par" tells you what to aim at,
    /// where "you were too slow" only tells you what happened.
    pub fn blurb(self) -> &'static str {
        match self {
            Axis::Speed => "out inside par",
            Axis::Stealth => "never noticed",
            Axis::Thoroughness => "took everything",
        }
    }
}

/// **A completed facility's score** (§15 Q4/§14 v2/#563): three independent stars.
///
/// Only a raid the player walked out of has one — see [`Score::of`]. A capture is not a
/// zero-star run, it is a run with no score at all: the end screen's job there is to say
/// *why you lost* (§14 v2), and marking that with three empty stars would be a rating
/// where a reason belongs.
///
/// `Serialize`/`Deserialize` because the campaign carries a run's scores and the campaign
/// is autosaved (§12.5) — the record has to survive a closed tab, exactly as the wallet
/// and the route do. It survives nothing further than that (§2.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Score {
    /// Out inside the facility's par ([`Axis::Speed`]).
    pub speed: bool,
    /// Left at condition 0 ([`Axis::Stealth`]).
    pub stealth: bool,
    /// Took every console and every crate ([`Axis::Thoroughness`]).
    pub thoroughness: bool,
}

impl Score {
    /// **The score of a finished run**, or `None` for a run that did not get out.
    ///
    /// The gate is [`Ending::won`](crate::Ending::won) and nothing else: scoring a capture would need every
    /// axis to answer a question the run never finished asking. This is the one
    /// constructor the game uses, so "scoring happens only on escape" is a property of the
    /// type rather than a check each surface has to remember.
    pub fn of(verdict: &Verdict) -> Option<Self> {
        verdict
            .ending
            .won()
            .then(|| Self::from_stats(verdict.stats))
    }

    /// The three axes read off a run's [`RunStats`] — the arithmetic, with the
    /// escape gate left to [`of`](Self::of).
    ///
    /// Every input is a fact the state already tracks: the spent turns against the par it
    /// was handed at level start, the alert rung standing at the end (which the §7.3
    /// no-decay rule makes the run's peak), and what was taken against what the building
    /// held.
    fn from_stats(stats: RunStats) -> Self {
        Self {
            // `<=` and not `<`: par is an allowance, and a run that spends exactly its
            // allowance has met it. An off-by-one here would make the star's own stated
            // rule ("inside par") a lie at exactly the boundary a player would test.
            speed: stats.turns <= stats.par,
            stealth: stats.alert_peak == 0,
            // Both halves, and `>=` on neither: taking *everything* is the axis, so a
            // facility with nothing in it (no consoles is impossible, no crates is the
            // common case) is thoroughly emptied by definition — which is the quick-play
            // degeneracy the design records rather than hides.
            thoroughness: stats.intel >= stats.intel_total && stats.caches >= stats.caches_total,
        }
    }

    /// Whether `axis` was earned — the accessor every reporting surface walks
    /// [`Axis::ALL`] through, so no screen can print two axes and forget the third.
    pub fn earned(self, axis: Axis) -> bool {
        match axis {
            Axis::Speed => self.speed,
            Axis::Stealth => self.stealth,
            Axis::Thoroughness => self.thoroughness,
        }
    }

    /// **How many stars**, 0–3 — the count of the axes earned, and never a weighting.
    ///
    /// The only number this type hands out, and the only one #573's archive gate will
    /// accumulate. Deriving it rather than storing it is what keeps the axes the truth and
    /// the total a summary of them.
    pub fn stars(self) -> u32 {
        Axis::ALL.iter().filter(|&&a| self.earned(a)).count() as u32
    }
}

/// The mark an **earned** star wears, and the mark a missed one wears (§11.3).
///
/// `★` is also the campaign map's **archive** glyph, and the two live on that one screen
/// — see `docs/render-reference.md`, which records the second reading. They do not
/// collide in practice because they are different *kinds* of thing in different places: a
/// node standing in the picture, and a mark on a labelled axis row. Splitting them would
/// cost the score the one glyph every player already reads as *this is the good one*.
pub const STAR_EARNED: char = '★';
/// See [`STAR_EARNED`].
pub const STAR_MISSED: char = '☆';

impl Score {
    /// The score as its three marks, `★★☆` — the glance form, always beside the named
    /// axes and never instead of them (#563: knowing *which* one you missed is the whole
    /// point).
    pub fn marks(self) -> String {
        Axis::ALL
            .iter()
            .map(|&axis| {
                if self.earned(axis) {
                    STAR_EARNED
                } else {
                    STAR_MISSED
                }
            })
            .collect()
    }
}

/// What a score is called when there is none to show (§11.8's plain register): the
/// campaign's own word for a raid that has not happened yet.
pub const NO_SCORE: &str = "not raided";

#[cfg(test)]
mod tests;
