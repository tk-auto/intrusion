//! The §13.2 signature metrics: the **ability-usage histogram** and **strategy
//! diversity** (#137), counted from the core's [`Event`] stream.
//!
//! These are the two metrics §13.2 calls out by name. The histogram is the one
//! that "would have caught the old game's free neutralise on day one — 94% usage
//! is a scream": a per-run count of every verb the player spent a turn on, so a
//! dominant ability (or a dead one) is legible at a glance. Diversity is the one
//! the design calls "the most important and the least obvious": win rate tells you
//! if the game is *hard*, diversity tells you if it is *interesting* — a batch
//! every seed solves with the same ability sequence is a puzzle with one answer.
//!
//! **Count from events, never from issued inputs** (§4.4, §13.2): an activation
//! the economy *refused* costs no turn and emits no [`Event::AbilityActivated`],
//! so it never reaches the histogram. The one verb with no event of its own is
//! [`Verb::Wait`] — waiting spends the turn silently — so the harness records it
//! from the spent-turn signal (a Wait always spends the turn and can never be
//! refused, so there is nothing an event would tell us that the spent turn does
//! not). Everything else is a distinct event.
//!
//! # What is `[START]`
//!
//! Two definitions here are starting values, named so they are easy to swap
//! (§13.2 asks for exactly that):
//!
//! - the **strategy signature** is the run's usage vector **L1-normalised** — a
//!   profile of *how the turns were spent*, independent of run length;
//! - the **diversity score** is the **mean pairwise Euclidean distance** between
//!   run signatures — 0 when every run played identically, larger as they spread.
//!
//! [`Event`]: intrusion_core::Event
//! [`Event::AbilityActivated`]: intrusion_core::Event::AbilityActivated

use intrusion_core::AbilityId;

/// One verb the usage histogram counts (#137, §13.2): the four activated
/// abilities plus the innate verbs that shape a strategy — Wait, Takedown, Drag.
///
/// Move is deliberately absent: it is "not shown in the UI" (§8.3) and is the
/// default nothing-else verb, so counting it would drown the signal the histogram
/// exists to surface. Run appears once — as the ability it is — not twice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verb {
    /// Spent a turn where you stood (§8.3) — the only verb with no event of its
    /// own; recorded from the spent turn.
    Wait,
    /// Activated Run (§8.3) — [`Event::AbilityActivated`](intrusion_core::Event::AbilityActivated).
    Run,
    /// Activated Camouflage (§8.3).
    Camouflage,
    /// Activated Decoy (§8.3).
    Decoy,
    /// Activated Dephase (§8.3).
    Dephase,
    /// Landed a takedown (§7.2) — [`Event::TakenDown`](intrusion_core::Event::TakenDown).
    Takedown,
    /// Grabbed a body to drag (§8.3) — [`Event::BodyGrabbed`](intrusion_core::Event::BodyGrabbed).
    /// The grab is the decision the histogram counts; the half-speed steps that
    /// follow are Moves.
    Drag,
}

impl Verb {
    /// Every verb, in the fixed order the histogram, signature vector and JSON
    /// object all use. Reordering this reorders the schema, so it is a deliberate,
    /// pinned decision (the tests below assert the order).
    pub const ALL: [Verb; 7] = [
        Verb::Wait,
        Verb::Run,
        Verb::Camouflage,
        Verb::Decoy,
        Verb::Dephase,
        Verb::Takedown,
        Verb::Drag,
    ];

    /// The activated ability this verb is, if any — the bridge from an
    /// [`AbilityId`] activation event to its histogram slot.
    pub fn of_ability(id: AbilityId) -> Verb {
        match id {
            AbilityId::Run => Verb::Run,
            AbilityId::Camouflage => Verb::Camouflage,
            AbilityId::Decoy => Verb::Decoy,
            AbilityId::Dephase => Verb::Dephase,
        }
    }

    /// The stable JSON key for this verb (see `crates/sim/README.md`).
    pub fn key(self) -> &'static str {
        match self {
            Verb::Wait => "wait",
            Verb::Run => "run",
            Verb::Camouflage => "camouflage",
            Verb::Decoy => "decoy",
            Verb::Dephase => "dephase",
            Verb::Takedown => "takedown",
            Verb::Drag => "drag",
        }
    }

    /// This verb's index into a [`UsageHistogram`]'s counts / a signature vector.
    fn index(self) -> usize {
        Verb::ALL
            .iter()
            .position(|&v| v == self)
            .expect("every verb is in ALL")
    }
}

/// The per-run ability-usage histogram (#137, §13.2): one count per [`Verb`],
/// accumulated as the run steps.
///
/// Also the batch total — [`merged`](Self::merged) sums run histograms, so the
/// same type reads at both scales. The counts are integers straight off the event
/// stream; the derived [`signature`](Self::signature) is the normalised form used
/// for [`diversity`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct UsageHistogram {
    counts: [u32; Verb::ALL.len()],
}

impl UsageHistogram {
    /// A fresh, all-zero histogram.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one use of `verb` (one spent turn on it).
    pub fn record(&mut self, verb: Verb) {
        self.counts[verb.index()] += 1;
    }

    /// How many times `verb` was used.
    pub fn count(&self, verb: Verb) -> u32 {
        self.counts[verb.index()]
    }

    /// The total of all counted verbs — turns spent on a *counted* verb (Move
    /// turns are not counted), so this is `<=` the run's spent turns.
    pub fn total(&self) -> u32 {
        self.counts.iter().sum()
    }

    /// Sum two histograms slot for slot — batch aggregation across runs.
    pub fn merged(mut self, other: &UsageHistogram) -> Self {
        for (a, b) in self.counts.iter_mut().zip(other.counts) {
            *a += b;
        }
        self
    }

    /// The run's **strategy signature** `[START]`: its usage vector L1-normalised
    /// to sum to 1 — a profile of how the turns were spent, independent of how many
    /// there were. A run that spent no turn on any counted verb (pure movement, or
    /// an instant capture) has the zero vector, which reads as "no strategy to
    /// compare" and sits at distance 0 from another such run.
    pub fn signature(&self) -> [f64; Verb::ALL.len()] {
        let total = self.total();
        let mut sig = [0.0; Verb::ALL.len()];
        if total > 0 {
            for (s, &c) in sig.iter_mut().zip(&self.counts) {
                *s = f64::from(c) / f64::from(total);
            }
        }
        sig
    }
}

/// The batch **diversity score** `[START]` (#137, §13.2): the mean pairwise
/// Euclidean distance between run [`signature`](UsageHistogram::signature)s.
///
/// 0 when every run played identically (the same policy twice scores ~0); larger
/// as strategies spread. Fewer than two runs have no pair to compare, so the score
/// is 0 — "nothing to diversify", never a divide-by-zero. This is the number that
/// answers "is the game interesting, or a puzzle with one answer?" (§13.2) — and,
/// per §13.4, it is *reported*, never ruled on.
pub fn diversity(histograms: &[UsageHistogram]) -> f64 {
    let sigs: Vec<[f64; Verb::ALL.len()]> = histograms.iter().map(|h| h.signature()).collect();
    let mut sum = 0.0;
    let mut pairs = 0u64;
    for (i, a) in sigs.iter().enumerate() {
        for b in &sigs[i + 1..] {
            let dist2: f64 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum();
            sum += dist2.sqrt();
            pairs += 1;
        }
    }
    if pairs == 0 {
        0.0
    } else {
        sum / pairs as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The verb order is fixed and every verb maps to a distinct JSON key — the
    /// schema the playtest skill parses (pinned so a reorder is a visible break).
    #[test]
    fn the_verb_order_and_keys_are_pinned() {
        let keys: Vec<&str> = Verb::ALL.iter().map(|v| v.key()).collect();
        assert_eq!(
            keys,
            [
                "wait",
                "run",
                "camouflage",
                "decoy",
                "dephase",
                "takedown",
                "drag"
            ]
        );
        // Each ability activation lands in its own slot.
        assert_eq!(Verb::of_ability(AbilityId::Run), Verb::Run);
        assert_eq!(Verb::of_ability(AbilityId::Camouflage), Verb::Camouflage);
        assert_eq!(Verb::of_ability(AbilityId::Decoy), Verb::Decoy);
        assert_eq!(Verb::of_ability(AbilityId::Dephase), Verb::Dephase);
    }

    /// Recording accumulates the exact per-verb counts, and `total` sums them —
    /// the histogram half of "a scripted policy yields the exact expected
    /// histogram" (§13.2), at the counting layer.
    #[test]
    fn recording_counts_each_verb_exactly() {
        let mut h = UsageHistogram::new();
        for _ in 0..3 {
            h.record(Verb::Wait);
        }
        h.record(Verb::Run);
        h.record(Verb::Dephase);
        h.record(Verb::Dephase);

        assert_eq!(h.count(Verb::Wait), 3);
        assert_eq!(h.count(Verb::Run), 1);
        assert_eq!(h.count(Verb::Dephase), 2);
        assert_eq!(h.count(Verb::Decoy), 0);
        assert_eq!(h.total(), 6);
    }

    /// Merging sums slot for slot — batch aggregation.
    #[test]
    fn merging_sums_slot_for_slot() {
        let mut a = UsageHistogram::new();
        a.record(Verb::Wait);
        a.record(Verb::Takedown);
        let mut b = UsageHistogram::new();
        b.record(Verb::Wait);
        b.record(Verb::Drag);

        let m = a.merged(&b);
        assert_eq!(m.count(Verb::Wait), 2);
        assert_eq!(m.count(Verb::Takedown), 1);
        assert_eq!(m.count(Verb::Drag), 1);
        assert_eq!(m.total(), 4);
    }

    /// The signature is the L1-normalised usage vector, and a zero histogram is
    /// the zero vector (not a NaN from dividing by zero).
    #[test]
    fn the_signature_is_the_normalised_usage_vector() {
        let mut h = UsageHistogram::new();
        h.record(Verb::Wait);
        h.record(Verb::Wait);
        h.record(Verb::Run);
        h.record(Verb::Run); // 2 wait, 2 run → each 0.5
        let sig = h.signature();
        assert_eq!(sig[Verb::Wait.index()], 0.5);
        assert_eq!(sig[Verb::Run.index()], 0.5);
        assert_eq!(sig.iter().sum::<f64>(), 1.0);

        assert_eq!(UsageHistogram::new().signature(), [0.0; Verb::ALL.len()]);
    }

    /// §13.2's diversity property, at the metric layer: two policies that play
    /// **differently** score higher than the **same** policy twice (~0), which is
    /// the whole point — win rate measures difficulty, diversity measures whether
    /// the game has more than one answer.
    #[test]
    fn different_strategies_score_higher_than_identical_ones() {
        let wait_only = {
            let mut h = UsageHistogram::new();
            for _ in 0..10 {
                h.record(Verb::Wait);
            }
            h
        };
        let run_only = {
            let mut h = UsageHistogram::new();
            for _ in 0..10 {
                h.record(Verb::Run);
            }
            h
        };

        // The same policy twice scores ~0.
        let identical = diversity(&[wait_only, wait_only]);
        assert_eq!(identical, 0.0);

        // Two genuinely different strategies score higher.
        let mixed = diversity(&[wait_only, run_only]);
        assert!(
            mixed > identical,
            "different strategies must be more diverse than identical ones ({mixed} vs {identical})",
        );
        // wait-only vs run-only are orthogonal unit vectors: distance √2.
        assert!((mixed - std::f64::consts::SQRT_2).abs() < 1e-9);

        // Fewer than two runs: nothing to compare, score 0 (no divide-by-zero).
        assert_eq!(diversity(&[wait_only]), 0.0);
        assert_eq!(diversity(&[]), 0.0);
    }
}
