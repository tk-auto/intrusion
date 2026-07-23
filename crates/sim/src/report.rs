//! Machine-readable output (§13.2): one JSON line per run, one summary line
//! per batch.
//!
//! The schema is the contract the playtest skill parses — documented in
//! `crates/sim/README.md` and pinned byte-for-byte by the tests below. It is
//! flat, its values are integers, fixed strings and fixed-precision floats the
//! harness controls entirely, so the encoding is hand-rolled here rather than
//! buying a serialization dependency for eight fields.
//!
//! `alert_peak` is emitted as `null` on every row: the facility-wide alert it
//! would measure is the radio net's value (#107), which does not exist yet —
//! a `null` says "not measured", where a `0` would lie that it was quiet.

use crate::harness::{RunOutcome, RunRecord};
use crate::usage::{diversity, UsageHistogram, Verb};

/// A `{"wait":N,...}` object of the histogram's integer counts, keys in
/// [`Verb::ALL`] order — the shared shape the run row and the summary both emit.
fn usage_counts_json(usage: &UsageHistogram) -> String {
    let body: Vec<String> = Verb::ALL
        .iter()
        .map(|&v| format!("\"{}\":{}", v.key(), usage.count(v)))
        .collect();
    format!("{{{}}}", body.join(","))
}

impl RunRecord {
    /// The run's JSONL row. Field order is fixed; see `crates/sim/README.md`.
    pub fn to_json_line(&self) -> String {
        format!(
            "{{\"seed\":{},\"outcome\":\"{}\",\"turns\":{},\"detections\":{},\"takedowns\":{},\"bodies_found\":{},\"usage\":{},\"alert_peak\":null}}",
            self.seed,
            self.outcome.as_str(),
            self.turns,
            self.detections,
            self.takedowns,
            self.bodies_found,
            usage_counts_json(&self.usage),
        )
    }
}

/// A batch's aggregates: win rate, turns-to-win, and the per-metric totals
/// from §13.2's table. Numbers, never verdicts (§13.4).
#[derive(Clone, PartialEq, Debug)]
pub struct Summary {
    /// Runs in the batch.
    pub runs: usize,
    /// Runs ending in each [`RunOutcome`], in the same order.
    pub wins: usize,
    /// See [`Summary::wins`].
    pub captures: usize,
    /// See [`Summary::wins`].
    pub entombed: usize,
    /// See [`Summary::wins`].
    pub timeouts: usize,
    /// `wins / runs`; `0.0` for an empty batch.
    pub win_rate: f64,
    /// Mean spent turns over the *winning* runs — `None` when nothing won.
    pub turns_to_win_mean: Option<f64>,
    /// Median spent turns over the *winning* runs — `None` when nothing won.
    pub turns_to_win_median: Option<f64>,
    /// Total fresh detections across the batch.
    pub detections: u64,
    /// Total takedowns across the batch.
    pub takedowns: u64,
    /// Total bodies found by guards across the batch.
    pub bodies_found: u64,
    /// The §13.2 ability-usage histogram (#137) summed across every run — the
    /// batch-wide count per verb, where a dominant or dead ability is legible.
    pub usage: UsageHistogram,
    /// The §13.2 **strategy diversity** score (#137): the mean pairwise distance
    /// between the runs' usage signatures. 0 when every run played the same way,
    /// larger as strategies spread. Reported, never ruled on (§13.4).
    pub diversity: f64,
    /// Total spent turns across the batch — the denominator of the per-verb usage
    /// share (§13.2's "share of turns").
    pub total_turns: u64,
}

impl Summary {
    /// Aggregate a batch of run records.
    pub fn of(records: &[RunRecord]) -> Self {
        let count = |o: RunOutcome| records.iter().filter(|r| r.outcome == o).count();
        let wins = count(RunOutcome::Win);
        let mut win_turns: Vec<u32> = records
            .iter()
            .filter(|r| r.outcome == RunOutcome::Win)
            .map(|r| r.turns)
            .collect();
        win_turns.sort_unstable();
        let mean = (!win_turns.is_empty())
            .then(|| f64::from(win_turns.iter().sum::<u32>()) / win_turns.len() as f64);
        let median = (!win_turns.is_empty()).then(|| {
            let mid = win_turns.len() / 2;
            if win_turns.len() % 2 == 1 {
                f64::from(win_turns[mid])
            } else {
                f64::from(win_turns[mid - 1] + win_turns[mid]) / 2.0
            }
        });
        Self {
            runs: records.len(),
            wins,
            captures: count(RunOutcome::Capture),
            entombed: count(RunOutcome::Entombed),
            timeouts: count(RunOutcome::Timeout),
            win_rate: if records.is_empty() {
                0.0
            } else {
                wins as f64 / records.len() as f64
            },
            turns_to_win_mean: mean,
            turns_to_win_median: median,
            detections: records.iter().map(|r| u64::from(r.detections)).sum(),
            takedowns: records.iter().map(|r| u64::from(r.takedowns)).sum(),
            bodies_found: records.iter().map(|r| u64::from(r.bodies_found)).sum(),
            usage: records
                .iter()
                .fold(UsageHistogram::new(), |acc, r| acc.merged(&r.usage)),
            diversity: diversity(&records.iter().map(|r| r.usage).collect::<Vec<_>>()),
            total_turns: records.iter().map(|r| u64::from(r.turns)).sum(),
        }
    }

    /// Per-verb **usage share of turns** (§13.2): each verb's batch count over the
    /// batch's total spent turns — the "used 94% of turns is a scream" number.
    /// Shares need not sum to 1 (a Move turn is counted for no verb); an empty
    /// batch (0 turns) shares 0.0, never a NaN.
    fn usage_share(&self) -> [f64; Verb::ALL.len()] {
        let mut share = [0.0; Verb::ALL.len()];
        if self.total_turns > 0 {
            for (s, &v) in share.iter_mut().zip(Verb::ALL.iter()) {
                *s = f64::from(self.usage.count(v)) / self.total_turns as f64;
            }
        }
        share
    }

    /// The batch's final JSONL line, keyed `"summary"` so a parser tells it
    /// from run rows structurally. Field order is fixed; floats print at fixed
    /// precision so equal batches print equal bytes.
    pub fn to_json_line(&self) -> String {
        let float = |v: Option<f64>| match v {
            Some(v) => format!("{v:.1}"),
            None => "null".to_string(),
        };
        let share = self.usage_share();
        let share_json: Vec<String> = Verb::ALL
            .iter()
            .zip(share)
            .map(|(&v, s)| format!("\"{}\":{s:.4}", v.key()))
            .collect();
        format!(
            "{{\"summary\":{{\"runs\":{},\"wins\":{},\"captures\":{},\"entombed\":{},\"timeouts\":{},\"win_rate\":{:.4},\"turns_to_win_mean\":{},\"turns_to_win_median\":{},\"detections\":{},\"takedowns\":{},\"bodies_found\":{},\"usage\":{},\"usage_share\":{{{}}},\"diversity\":{:.4},\"alert_peak\":null}}}}",
            self.runs,
            self.wins,
            self.captures,
            self.entombed,
            self.timeouts,
            self.win_rate,
            float(self.turns_to_win_mean),
            float(self.turns_to_win_median),
            self.detections,
            self.takedowns,
            self.bodies_found,
            usage_counts_json(&self.usage),
            share_json.join(","),
            self.diversity,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(seed: u64, outcome: RunOutcome, turns: u32) -> RunRecord {
        // A fixed sample usage so the schema strings pin deterministically: two
        // Waits and one Run per record. All records share it, so their signatures
        // are identical and the batch diversity is 0.
        let mut usage = UsageHistogram::new();
        usage.record(Verb::Wait);
        usage.record(Verb::Wait);
        usage.record(Verb::Run);
        RunRecord {
            seed,
            outcome,
            turns,
            detections: 2,
            takedowns: 1,
            bodies_found: 0,
            usage,
        }
    }

    /// The row schema, pinned byte-for-byte: this string is what the playtest
    /// skill parses, so any change here is a deliberate, visible break.
    #[test]
    fn the_run_row_schema_is_pinned() {
        assert_eq!(
            record(17, RunOutcome::Win, 214).to_json_line(),
            "{\"seed\":17,\"outcome\":\"win\",\"turns\":214,\"detections\":2,\"takedowns\":1,\"bodies_found\":0,\"usage\":{\"wait\":2,\"run\":1,\"camouflage\":0,\"decoy\":0,\"dephase\":0,\"takedown\":0,\"drag\":0},\"alert_peak\":null}"
        );
    }

    /// The summary schema, pinned the same way — including the aggregation
    /// itself: rate over all runs, mean/median over winning runs only, an even
    /// win count splitting the median.
    #[test]
    fn the_summary_schema_and_aggregation_are_pinned() {
        let records = vec![
            record(1, RunOutcome::Win, 100),
            record(2, RunOutcome::Win, 111),
            record(3, RunOutcome::Capture, 40),
            record(4, RunOutcome::Timeout, 500),
        ];
        let summary = Summary::of(&records);
        assert_eq!(
            summary.to_json_line(),
            "{\"summary\":{\"runs\":4,\"wins\":2,\"captures\":1,\"entombed\":0,\"timeouts\":1,\"win_rate\":0.5000,\"turns_to_win_mean\":105.5,\"turns_to_win_median\":105.5,\"detections\":8,\"takedowns\":4,\"bodies_found\":0,\"usage\":{\"wait\":8,\"run\":4,\"camouflage\":0,\"decoy\":0,\"dephase\":0,\"takedown\":0,\"drag\":0},\"usage_share\":{\"wait\":0.0107,\"run\":0.0053,\"camouflage\":0.0000,\"decoy\":0.0000,\"dephase\":0.0000,\"takedown\":0.0000,\"drag\":0.0000},\"diversity\":0.0000,\"alert_peak\":null}}"
        );
    }

    /// No winner means no turns-to-win — `null`, never a fake zero — and an
    /// empty batch divides into a 0.0 rate, not a NaN.
    #[test]
    fn winless_and_empty_batches_stay_honest() {
        let summary = Summary::of(&[record(1, RunOutcome::Capture, 40)]);
        assert_eq!(summary.turns_to_win_mean, None);
        assert_eq!(summary.turns_to_win_median, None);
        assert!(summary
            .to_json_line()
            .contains("\"turns_to_win_mean\":null"));

        let empty = Summary::of(&[]);
        assert_eq!(empty.win_rate, 0.0);
        assert_eq!(empty.runs, 0);
        assert_eq!(empty.diversity, 0.0, "an empty batch has no diversity");
        assert_eq!(empty.total_turns, 0);
    }

    /// The §13.2 diversity signal at the batch level (#137): a batch whose runs
    /// played **differently** scores higher than one whose runs all played the
    /// same. All-same `record`s (identical usage) score 0; swapping one run's
    /// usage to a different verb lifts the score.
    #[test]
    fn a_mixed_batch_is_more_diverse_than_a_uniform_one() {
        let uniform = Summary::of(&[
            record(1, RunOutcome::Win, 100),
            record(2, RunOutcome::Win, 100),
        ]);
        assert_eq!(uniform.diversity, 0.0, "identical usage is not diverse");

        // A run that spent its turns on Dephase instead of Wait/Run.
        let mut odd = record(2, RunOutcome::Win, 100);
        odd.usage = {
            let mut h = UsageHistogram::new();
            h.record(Verb::Dephase);
            h
        };
        let mixed = Summary::of(&[record(1, RunOutcome::Win, 100), odd]);
        assert!(
            mixed.diversity > uniform.diversity,
            "a mixed batch must out-diversify a uniform one ({} vs {})",
            mixed.diversity,
            uniform.diversity,
        );
    }
}
