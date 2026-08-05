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

use intrusion_core::{AbilityId, Event};

/// One verb the usage histogram counts (#137, §13.2): the activated abilities
/// plus the innate verbs that shape a strategy — Wait, Takedown, Drag, Crouch, Stow.
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
    /// Activated Autodoors (§8.3).
    Autodoors,
    /// Activated Confusion (§8.3).
    Confusion,
    /// Landed a takedown (§7.2) — [`Event::TakenDown`](intrusion_core::Event::TakenDown).
    Takedown,
    /// Grabbed a body to drag (§8.3) — [`Event::BodyGrabbed`](intrusion_core::Event::BodyGrabbed).
    /// The grab is the decision the histogram counts; the half-speed steps that
    /// follow are Moves.
    ///
    /// It is a **real** decision since #451: the pickup is a wait spent standing on
    /// the body, not something that rode the step off its cell. The count used to
    /// include grabs no temperament had asked for — a bot crossing a body picked it
    /// up and dropped it again — and does not any more, so a `drag` here now means a
    /// turn the policy chose to spend.
    Drag,
    /// Bored through a wall with Pierce Wall (§8.3/#303). Counted like any other
    /// activation, and worth watching closely: three per level is a small enough
    /// budget that the histogram says directly whether the ability is being used at
    /// all, or is unreachable from where the bot ever stands.
    PierceWall,
    /// Sealed the doors around the bot with Lockdown (§8.3/#242). Worth watching for
    /// the opposite reason to Pierce Wall's: it is refused outright where no door is in
    /// reach, so a flat zero here is as likely to mean "never stands near a doorway" as
    /// "never chooses it".
    Lockdown,
    /// Ducked behind a bench (§10.3/#379) —
    /// [`Event::Crouched`](intrusion_core::Event::Crouched). An innate bump verb like
    /// Takedown and Drag, counted the same way: the *duck* is the decision, so a held
    /// pose costs one count and the waits and crouch-walks that hold it are Waits and
    /// Moves. Its zero was the §13.2 false zero in its purest form — the geometry
    /// §10.1a goes to real trouble over, with no policy that had ever entered it.
    Crouch,
    /// Put a dragged body away inside a cupboard (§10.3/#381) —
    /// [`Event::BodyStored`](intrusion_core::Event::BodyStored). The last link in
    /// §7.2's body chain, and the *loudest* choice in it: the takedown's cost is the
    /// body, taking hold of it costs a turn (§8.3/#451), and letting it go is free —
    /// but stowing it spends the turn, puts
    /// the body beyond every cone and **locks** the cupboard behind it, which stops it
    /// being a hideout at all. Read it against `bodies_found`: the tidier the run, the
    /// flatter that row.
    Stow,
    /// Threw the comms console's switch (§7.7/#405) —
    /// [`Event::CommsSilenced`](intrusion_core::Event::CommsSilenced). A bump verb like
    /// Takedown and Crouch, and the deed is *read* rather than inferred: one bump ends
    /// guard-to-guard call-ins for the rest of the level, so the count is at most one
    /// per run and a `1` here says the whole radio half of §7.3/§7.7 was actually
    /// exercised on that seed.
    ///
    /// Read it against `bodies_found` and the alert rows. Until this verb existed
    /// `Terrain::CommsConsole` had no source in the sim at all — every alert number was
    /// measured in a world where the counterplay was never taken.
    SilenceRadio,
    /// Launched a drone and took its controls (§8.3/#273) —
    /// [`Event::AbilityActivated`](intrusion_core::Event::AbilityActivated), like every
    /// other activation. Taking the keys *back* off a hovering drone is not counted
    /// twice: the histogram counts the decision to spend the ability, and the resume is
    /// the same ability still running.
    ///
    /// **Expect a zero here until a piloting policy exists**, and read it as an
    /// unexercised verb rather than a dead one: piloting is a control mode the bot does
    /// not have (`docs/stats/abilities/drone.md`), so its cue declines by design. This
    /// is the one row in the histogram whose zero is a fact about the *bot* — which is
    /// exactly why it has a row at all, since a verb with no slot could not report the
    /// day the policy lands.
    Drone,
    /// Forged a control call with False Call (§7.7/§8.3/#504) —
    /// [`Event::AbilityActivated`](intrusion_core::Event::AbilityActivated), like every
    /// other activation.
    ///
    /// The row to read against the alert and detection numbers rather than on its own:
    /// what the verb buys is not measured where it is pressed but in the turns after,
    /// and what it *risks* is that the bot called a search onto its own feet. A count
    /// that climbs while `detections` climbs with it is the ticket's own suicide-button
    /// worry showing up as data.
    FalseCall,
}

impl Verb {
    /// Every verb, in the fixed order the histogram, signature vector and JSON
    /// object all use. Reordering this reorders the schema, so it is a deliberate,
    /// pinned decision (the tests below assert the order).
    pub const ALL: [Verb; 16] = [
        Verb::Wait,
        Verb::Run,
        Verb::Camouflage,
        Verb::Decoy,
        Verb::Dephase,
        Verb::Autodoors,
        Verb::Confusion,
        Verb::Takedown,
        Verb::Drag,
        Verb::PierceWall,
        Verb::Lockdown,
        Verb::Crouch,
        Verb::Stow,
        Verb::SilenceRadio,
        Verb::Drone,
        Verb::FalseCall,
    ];

    /// The verb an [`AbilityId`] activation counts as — the bridge from an
    /// activation event to its histogram slot.
    ///
    /// `None` for a **passive** (#264): the histogram counts *decisions*, and a
    /// passive is never activated, so there is no moment to count. Its influence
    /// shows up in the outcome metrics instead, which is the honest place for it —
    /// a slot in this histogram would sit at zero for a run the ability shaped
    /// throughout.
    ///
    /// That holds for the Saver (#243) even though it has a per-level budget: a
    /// budget is not a press either. What spends it is a guard reaching the player,
    /// and the metric that shows it is the pair of counters that move together when it
    /// fires — a capture that did not happen, and a takedown the player never made.
    pub fn of_ability(id: AbilityId) -> Option<Verb> {
        Some(match id {
            AbilityId::Run => Verb::Run,
            AbilityId::Camouflage => Verb::Camouflage,
            AbilityId::Decoy => Verb::Decoy,
            AbilityId::Dephase => Verb::Dephase,
            AbilityId::Autodoors => Verb::Autodoors,
            AbilityId::Confusion => Verb::Confusion,
            AbilityId::PierceWall => Verb::PierceWall,
            AbilityId::Lockdown => Verb::Lockdown,
            AbilityId::Drone => Verb::Drone,
            AbilityId::FalseCall => Verb::FalseCall,
            AbilityId::Vision | AbilityId::Saver => return None,
        })
    }

    /// The verb an [`Event`] counts as — the bridge from the core's event stream to a
    /// histogram slot, and the one place that mapping lives (§13.2: count from
    /// events, never from issued inputs, so an activation the economy refused takes
    /// no slot).
    ///
    /// `None` is the load-bearing half. A **free action** (§4.4) spends no turn and
    /// so has no slot, and the body chain holds both sides of that line side by side:
    /// [`Event::BodyStored`] is a decision that costs the turn and locks a cupboard
    /// (§10.3), so it counts as [`Verb::Stow`]; [`Event::BodyReleased`] beside it
    /// costs nothing and counts as nothing, exactly as a Move does.
    ///
    /// [`Verb::Wait`] is absent because it has no event of its own — the harness
    /// records it from the spent turn.
    ///
    /// [`Event`]: intrusion_core::Event
    /// [`Event::BodyStored`]: intrusion_core::Event::BodyStored
    /// [`Event::BodyReleased`]: intrusion_core::Event::BodyReleased
    pub fn of_event(event: Event) -> Option<Verb> {
        match event {
            Event::AbilityActivated { ability, .. } => Verb::of_ability(ability),
            Event::TakenDown { .. } => Some(Verb::Takedown),
            // The grab is the decision the histogram counts; the half-speed steps
            // that follow are Moves.
            Event::BodyGrabbed { .. } => Some(Verb::Drag),
            Event::BodyStored { .. } => Some(Verb::Stow),
            // The duck itself, and only it (§10.3/#379): re-bumping a table the run
            // already crouched behind is a free no-op that emits no event, so a pose
            // held for a dozen turns still counts once.
            Event::Crouched { .. } => Some(Verb::Crouch),
            // The switch itself (§7.7/#405). Permanent for the level, so a second bump
            // is a no-op that emits nothing and the count never exceeds one.
            Event::CommsSilenced { .. } => Some(Verb::SilenceRadio),
            _ => None,
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
            Verb::Autodoors => "autodoors",
            Verb::Confusion => "confusion",
            Verb::Takedown => "takedown",
            Verb::Drag => "drag",
            Verb::PierceWall => "pierce_wall",
            Verb::Lockdown => "lockdown",
            Verb::Crouch => "crouch",
            Verb::Stow => "stow",
            Verb::SilenceRadio => "silence_radio",
            Verb::Drone => "drone",
            Verb::FalseCall => "false_call",
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
    use intrusion_core::Cell;

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
                "autodoors",
                "confusion",
                "takedown",
                "drag",
                "pierce_wall",
                "lockdown",
                "crouch",
                "stow",
                "silence_radio",
                "drone",
                "false_call"
            ]
        );
        // Each ability activation lands in its own slot.
        assert_eq!(Verb::of_ability(AbilityId::Run), Some(Verb::Run));
        assert_eq!(
            Verb::of_ability(AbilityId::Camouflage),
            Some(Verb::Camouflage)
        );
        assert_eq!(Verb::of_ability(AbilityId::Decoy), Some(Verb::Decoy));
        assert_eq!(Verb::of_ability(AbilityId::Dephase), Some(Verb::Dephase));
        assert_eq!(
            Verb::of_ability(AbilityId::Autodoors),
            Some(Verb::Autodoors)
        );
        assert_eq!(
            Verb::of_ability(AbilityId::Confusion),
            Some(Verb::Confusion)
        );
        assert_eq!(
            Verb::of_ability(AbilityId::PierceWall),
            Some(Verb::PierceWall)
        );
        // A passive has no activation to count (#264) — and so no slot. Every
        // activated ability does have one, so the histogram stays exhaustive over
        // the decisions a run can actually make.
        for id in AbilityId::ALL {
            assert_eq!(
                Verb::of_ability(id).is_none(),
                id.is_passive(),
                "{}",
                id.name(),
            );
        }
    }

    /// §7.2's body chain, event by event (#381): the takedown, the grab and the
    /// **stow** each land in their own slot, and the **release** lands in none.
    ///
    /// The release is the assertion worth having. It is §4.4's free action — letting
    /// a body go costs no turn and refunds nothing — and the histogram counts turns
    /// spent on a decision, so a slot for it would be a turn that was never spent.
    /// Pinned here rather than left implied, so a later change cannot quietly give a
    /// free action a slot (`Move` sits on the same side of the line, and for the same
    /// reason).
    #[test]
    fn the_body_chain_counts_the_stow_and_never_the_release() {
        let at = Cell::new(3, 4);
        assert_eq!(
            Verb::of_event(Event::TakenDown { at }),
            Some(Verb::Takedown)
        );
        assert_eq!(Verb::of_event(Event::BodyGrabbed { at }), Some(Verb::Drag));
        assert_eq!(Verb::of_event(Event::BodyStored { at }), Some(Verb::Stow));
        assert_eq!(Verb::of_event(Event::BodyReleased { at }), None);

        // The same line, drawn through the other verbs: a bump-verb decision counts,
        // a free toggle-off does not, and a *refused* activation emits no
        // `AbilityActivated` at all so it never reaches here.
        assert_eq!(
            Verb::of_event(Event::Crouched { behind: at }),
            Some(Verb::Crouch)
        );
        assert_eq!(
            Verb::of_event(Event::AbilityActivated {
                ability: AbilityId::Run,
                uses_left: None,
            }),
            Some(Verb::Run)
        );
        assert_eq!(
            Verb::of_event(Event::AbilityDeactivated {
                ability: AbilityId::Run,
            }),
            None
        );
        assert_eq!(Verb::of_event(Event::Moved { to: at }), None);
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
