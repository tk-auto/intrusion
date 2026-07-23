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

impl RunRecord {
    /// The run's JSONL row. Field order is fixed; see `crates/sim/README.md`.
    pub fn to_json_line(&self) -> String {
        format!(
            "{{\"seed\":{},\"outcome\":\"{}\",\"turns\":{},\"detections\":{},\"takedowns\":{},\"bodies_found\":{},\"alert_peak\":null}}",
            self.seed,
            self.outcome.as_str(),
            self.turns,
            self.detections,
            self.takedowns,
            self.bodies_found,
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
        }
    }

    /// The batch's final JSONL line, keyed `"summary"` so a parser tells it
    /// from run rows structurally. Field order is fixed; floats print at fixed
    /// precision so equal batches print equal bytes.
    pub fn to_json_line(&self) -> String {
        let float = |v: Option<f64>| match v {
            Some(v) => format!("{v:.1}"),
            None => "null".to_string(),
        };
        format!(
            "{{\"summary\":{{\"runs\":{},\"wins\":{},\"captures\":{},\"entombed\":{},\"timeouts\":{},\"win_rate\":{:.4},\"turns_to_win_mean\":{},\"turns_to_win_median\":{},\"detections\":{},\"takedowns\":{},\"bodies_found\":{},\"alert_peak\":null}}}}",
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
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(seed: u64, outcome: RunOutcome, turns: u32) -> RunRecord {
        RunRecord {
            seed,
            outcome,
            turns,
            detections: 2,
            takedowns: 1,
            bodies_found: 0,
        }
    }

    /// The row schema, pinned byte-for-byte: this string is what the playtest
    /// skill parses, so any change here is a deliberate, visible break.
    #[test]
    fn the_run_row_schema_is_pinned() {
        assert_eq!(
            record(17, RunOutcome::Win, 214).to_json_line(),
            "{\"seed\":17,\"outcome\":\"win\",\"turns\":214,\"detections\":2,\"takedowns\":1,\"bodies_found\":0,\"alert_peak\":null}"
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
            "{\"summary\":{\"runs\":4,\"wins\":2,\"captures\":1,\"entombed\":0,\"timeouts\":1,\"win_rate\":0.5000,\"turns_to_win_mean\":105.5,\"turns_to_win_median\":105.5,\"detections\":8,\"takedowns\":4,\"bodies_found\":0,\"alert_peak\":null}}"
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
    }
}
