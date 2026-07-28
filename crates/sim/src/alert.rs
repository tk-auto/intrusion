//! The §7.3 alert ladder as a **measurement** (§13.2/#376).
//!
//! §13.2's metric table has always listed *"alert peak — whether escalation
//! escalates"*, and the row has always been `null`: first because there was no alert
//! worth reading, then because the ladder (#311) shipped with nothing measuring it.
//! This module is the row, in the shape the ladder actually has.
//!
//! A peak alone would answer the wrong question. The ladder is a *path*: a run that
//! reaches rung 3 by leaving bodies where cones find them is a different game from one
//! that gets there by being seen over and over, and both read `3`. So what a run
//! records is every [`Escalation`] — **which turn**, **which rung**, and **which
//! trigger** got it there — and the peak falls out of that.
//!
//! # Reading a zero
//!
//! A trigger's count is the number of escalations it **caused**, which is exactly what
//! [`Event::AlertRaised`](intrusion_core::Event) reports: a trigger that fires at or
//! below the rung the facility has already reached escalates nothing and says nothing.
//! So a zero has **two** readings — the bot never did the thing, or something louder
//! always reached that rung first — and neither of them is *"this trigger does not
//! matter"* (§13.4/#260). The report says `never exercised`, and the difference
//! between the two is a question for whoever reads it, not a verdict the harness may
//! reach.

use intrusion_core::{AlertTrigger, TOP_RUNG};

/// Rungs a batch can distribute over, 0..=[`TOP_RUNG`] — the width of the
/// distribution the summary emits. Derived from the ladder rather than written down,
/// so a fourth rung would widen the report rather than silently fall off its end.
pub const RUNGS: usize = TOP_RUNG as usize + 1;

/// One step up the ladder (§7.3): the turn it happened on, the rung it reached, and
/// the trigger that got it there.
///
/// The ladder is monotone and three rungs tall, so a run has **at most three** of
/// these, and the rung of the last one is the run's peak.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Escalation {
    /// The spent turn the rung was reached on ([`State::turn`](intrusion_core::State)).
    pub turn: u32,
    /// The rung reached, 1..=[`TOP_RUNG`].
    pub rung: u32,
    /// What got the facility there (§7.3).
    pub trigger: AlertTrigger,
}

impl Escalation {
    /// The JSON object for one escalation. Field order is fixed; see
    /// `crates/sim/README.md`.
    fn to_json(self) -> String {
        format!(
            "{{\"turn\":{},\"rung\":{},\"trigger\":\"{}\"}}",
            self.turn,
            self.rung,
            trigger_key(self.trigger),
        )
    }
}

/// One run's climb up the ladder (§7.3) — every escalation, in the order they
/// happened. Empty for a run the facility never noticed, which is a rung-0 run and
/// not a missing measurement.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AlertRecord {
    escalations: Vec<Escalation>,
}

impl AlertRecord {
    /// Record an escalation the core reported (`Event::AlertRaised`).
    pub fn record(&mut self, turn: u32, rung: u32, trigger: AlertTrigger) {
        self.escalations.push(Escalation {
            turn,
            rung,
            trigger,
        });
    }

    /// The highest rung the facility reached — **0** for a run it never noticed. Read
    /// off the escalations rather than tracked alongside them, so the peak and the
    /// path can never disagree.
    pub fn peak(&self) -> u32 {
        self.escalations.iter().map(|e| e.rung).max().unwrap_or(0)
    }

    /// The climb, oldest first.
    pub fn escalations(&self) -> &[Escalation] {
        &self.escalations
    }

    /// The JSON array of escalations — `[]` for a facility that stayed quiet.
    pub fn to_json(&self) -> String {
        let body: Vec<String> = self.escalations.iter().map(|e| e.to_json()).collect();
        format!("[{}]", body.join(","))
    }
}

/// A batch's ladder: how the runs' **peaks** were distributed, and which triggers did
/// the escalating (§13.2/#376).
///
/// The distribution is the finding, not the peak: *"most runs end at rung 1"* and
/// *"most runs end at rung 3"* are opposite balance verdicts and have the same
/// maximum.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AlertTally {
    /// Runs whose peak was rung *i*, indexed by rung.
    rungs: [usize; RUNGS],
    /// Escalations each trigger caused, in [`AlertTrigger::ALL`] order.
    triggers: [u64; AlertTrigger::ALL.len()],
}

impl AlertTally {
    /// Tally a batch's records.
    pub fn of<'a>(records: impl IntoIterator<Item = &'a AlertRecord>) -> Self {
        let mut tally = Self::default();
        for record in records {
            // A peak can only be a rung the ladder has, so the index is in range —
            // but a `get_mut` says so rather than trusting it, and a hypothetical
            // out-of-range peak drops from the distribution instead of panicking a
            // batch that has already been paid for.
            if let Some(count) = tally.rungs.get_mut(record.peak() as usize) {
                *count += 1;
            }
            for escalation in record.escalations() {
                let index = AlertTrigger::ALL
                    .iter()
                    .position(|&t| t == escalation.trigger);
                if let Some(index) = index {
                    tally.triggers[index] += 1;
                }
            }
        }
        tally
    }

    /// Runs whose peak was `rung`.
    pub fn runs_at(&self, rung: u32) -> usize {
        self.rungs.get(rung as usize).copied().unwrap_or(0)
    }

    /// Escalations `trigger` caused across the batch. **Zero means "never caused an
    /// escalation"**, which is not the same as "does nothing" — see the module header.
    pub fn caused_by(&self, trigger: AlertTrigger) -> u64 {
        AlertTrigger::ALL
            .iter()
            .position(|&t| t == trigger)
            .map_or(0, |i| self.triggers[i])
    }

    /// The mean peak rung over the batch — the single number a sweep plots. `0.0` for
    /// an empty batch, never a NaN.
    pub fn peak_mean(&self, runs: usize) -> f64 {
        if runs == 0 {
            return 0.0;
        }
        let total: usize = (0..RUNGS).map(|rung| rung * self.rungs[rung]).sum();
        total as f64 / runs as f64
    }

    /// The rung distribution as `{"0":N,...}`, one key per rung.
    pub fn rungs_json(&self) -> String {
        let body: Vec<String> = (0..RUNGS)
            .map(|rung| format!("\"{rung}\":{}", self.rungs[rung]))
            .collect();
        format!("{{{}}}", body.join(","))
    }

    /// The trigger attribution as `{"sighting":N,...}`, keys in
    /// [`AlertTrigger::ALL`] order — **every** trigger, including the ones at zero.
    /// A trigger silently missing from the object would read as one that never fired,
    /// which is the misreading this whole row exists to prevent.
    pub fn triggers_json(&self) -> String {
        let body: Vec<String> = AlertTrigger::ALL
            .iter()
            .zip(self.triggers)
            .map(|(&trigger, count)| format!("\"{}\":{count}", trigger_key(trigger)))
            .collect();
        format!("{{{}}}", body.join(","))
    }

    /// The triggers that caused **no** escalation across the batch (§13.4/#260): the
    /// ones a report must call *inconclusive* rather than *no impact*. Empty when
    /// every trigger fired at least once.
    pub fn never_exercised(&self) -> Vec<AlertTrigger> {
        AlertTrigger::ALL
            .into_iter()
            .filter(|&t| self.caused_by(t) == 0)
            .collect()
    }
}

/// The stable JSON key for a trigger — the schema contract the playtest skill parses
/// (`crates/sim/README.md`).
///
/// Total over the enum on purpose: a trigger added to §7.3 cannot ship without being
/// given a name here, so the attribution can never quietly lose a column.
pub fn trigger_key(trigger: AlertTrigger) -> &'static str {
    match trigger {
        AlertTrigger::Sighting => "sighting",
        AlertTrigger::MissedPing => "missed-ping",
        AlertTrigger::RepeatSightings => "repeat-sightings",
        AlertTrigger::ConsoleTampered => "console-tampered",
        AlertTrigger::BodyFound => "body-found",
        AlertTrigger::SecondPostSilent => "second-post-silent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The peak is read off the path, so the two cannot disagree — and a facility
    /// that never noticed is rung **0**, a real reading rather than a missing one.
    #[test]
    fn the_peak_is_the_highest_rung_the_path_reached() {
        let quiet = AlertRecord::default();
        assert_eq!(quiet.peak(), 0, "a clean raid is rung 0, not null");
        assert_eq!(quiet.to_json(), "[]");

        let mut climbed = AlertRecord::default();
        climbed.record(12, 1, AlertTrigger::Sighting);
        climbed.record(40, 3, AlertTrigger::BodyFound);
        assert_eq!(climbed.peak(), 3);
        assert_eq!(
            climbed.to_json(),
            "[{\"turn\":12,\"rung\":1,\"trigger\":\"sighting\"},\
             {\"turn\":40,\"rung\":3,\"trigger\":\"body-found\"}]",
        );
    }

    /// The distribution is the finding, not the maximum: two batches with the same
    /// peak can be opposite balance readings (§13.2), and the mean is what a sweep
    /// plots.
    #[test]
    fn the_distribution_separates_batches_a_peak_cannot() {
        let at = |rung: u32, trigger: AlertTrigger| {
            let mut record = AlertRecord::default();
            if rung > 0 {
                record.record(10, rung, trigger);
            }
            record
        };
        let mostly_quiet = [
            at(0, AlertTrigger::Sighting),
            at(1, AlertTrigger::Sighting),
            at(1, AlertTrigger::Sighting),
            at(3, AlertTrigger::BodyFound),
        ];
        let mostly_loud = [
            at(3, AlertTrigger::BodyFound),
            at(3, AlertTrigger::BodyFound),
            at(3, AlertTrigger::BodyFound),
            at(3, AlertTrigger::BodyFound),
        ];
        let (quiet, loud) = (AlertTally::of(&mostly_quiet), AlertTally::of(&mostly_loud));
        assert_eq!(quiet.runs_at(0), 1);
        assert_eq!(quiet.runs_at(1), 2);
        assert_eq!(quiet.runs_at(3), 1);
        assert_eq!(quiet.peak_mean(4), 1.25);
        assert_eq!(loud.peak_mean(4), 3.0);
        assert_eq!(
            quiet.rungs_json(),
            "{\"0\":1,\"1\":2,\"2\":0,\"3\":1}",
            "every rung is a column, including the empty one",
        );
        assert_eq!(
            AlertTally::default().peak_mean(0),
            0.0,
            "an empty batch means 0.0, never a NaN",
        );
    }

    /// §13.4/#260: a trigger that caused no escalation is reported as a **zero
    /// column**, never as a missing one — a silently absent key reads as "did not
    /// happen" when the honest reading is "not exercised here".
    #[test]
    fn every_trigger_is_a_column_even_at_zero() {
        let mut record = AlertRecord::default();
        record.record(5, 1, AlertTrigger::MissedPing);
        record.record(9, 3, AlertTrigger::SecondPostSilent);
        let tally = AlertTally::of([&record]);

        assert_eq!(tally.caused_by(AlertTrigger::MissedPing), 1);
        assert_eq!(tally.caused_by(AlertTrigger::Sighting), 0);
        assert_eq!(
            tally.triggers_json(),
            "{\"sighting\":0,\"missed-ping\":1,\"repeat-sightings\":0,\
             \"console-tampered\":0,\"body-found\":0,\"second-post-silent\":1}",
        );
        assert_eq!(
            tally.never_exercised(),
            vec![
                AlertTrigger::Sighting,
                AlertTrigger::RepeatSightings,
                AlertTrigger::ConsoleTampered,
                AlertTrigger::BodyFound,
            ],
            "the inconclusive columns are named, not inferred from a blank",
        );
        assert!(
            AlertTally::of([&record, &record]).never_exercised().len() < 6,
            "a trigger that fired is never called unexercised",
        );
    }

    /// The JSON keys are the schema contract: distinct, stable, and total over the
    /// ladder — a trigger added to §7.3 cannot ship without a column here.
    #[test]
    fn every_trigger_has_its_own_stable_key() {
        let keys: Vec<&str> = AlertTrigger::ALL.iter().copied().map(trigger_key).collect();
        assert_eq!(
            keys,
            [
                "sighting",
                "missed-ping",
                "repeat-sightings",
                "console-tampered",
                "body-found",
                "second-post-silent",
            ],
        );
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "two triggers share a key");
    }
}
