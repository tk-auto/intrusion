//! Machine-readable output (§13.2): one JSON line per run, one summary line
//! per batch.
//!
//! The schema is the contract the playtest skill parses — documented in
//! `crates/sim/README.md` and pinned byte-for-byte by the tests below. It is
//! flat, its values are integers, fixed strings and fixed-precision floats the
//! harness controls entirely, so the encoding is hand-rolled here rather than
//! buying a serialisation dependency for eight fields.
//!
//! `alert_peak` used to be `null` on every row — the §13.2 metric with nothing behind
//! it. Since #311 gave the facility a ladder and #376 measured it, the row carries the
//! rung a run reached **and the path it took there** (`alert_escalations`), and the
//! summary carries the batch's rung distribution and trigger attribution rather than a
//! single maximum. There is no `null` left in the alert rows: rung **0** is a real
//! reading — a raid the facility never noticed — where the old `null` meant "nothing
//! measured this".

use crate::alert::AlertTally;
use crate::harness::{RunOutcome, RunRecord};
use crate::usage::{diversity, UsageHistogram, Verb};
use intrusion_core::{Axis, Score};

/// A `{"wait":N,...}` object of the histogram's integer counts, keys in
/// [`Verb::ALL`] order — the shared shape the run row and the summary both emit.
fn usage_counts_json(usage: &UsageHistogram) -> String {
    let body: Vec<String> = Verb::ALL
        .iter()
        .map(|&v| format!("\"{}\":{}", v.key(), usage.count(v)))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// A run's three stars as a `{"speed":true,...}` object, or `null` for a run that did not
/// get out (#563).
///
/// `null` and *all three false* are different findings and must stay different bytes: the
/// first is a raid that never finished, the second one that finished badly.
fn score_json(score: Option<Score>) -> String {
    match score {
        None => "null".to_string(),
        Some(score) => {
            let body: Vec<String> = Axis::ALL
                .iter()
                .map(|&axis| format!("\"{}\":{}", axis.label(), score.earned(axis)))
                .collect();
            format!("{{{}}}", body.join(","))
        }
    }
}

/// A profile name as a JSON string, or `null` when the policy had no temperament
/// (a script). The `null` says "no profile"; naming `"balanced"` there would claim
/// a bot played the run.
fn profile_json(profile: Option<&'static str>) -> String {
    profile.map_or_else(|| "null".to_string(), |name| format!("\"{name}\""))
}

impl RunRecord {
    /// The run's JSONL row. Field order is fixed; see `crates/sim/README.md`.
    pub fn to_json_line(&self) -> String {
        format!(
            "{{\"seed\":{},\"profile\":{},\"outcome\":\"{}\",\"turns\":{},\"detections\":{},\"takedowns\":{},\"bodies_found\":{},\"usage\":{},\"alert_peak\":{},\"alert_escalations\":{},\"reinforcements\":{},\"par\":{},\"stars\":{},\"score\":{}}}",
            self.seed,
            profile_json(self.profile),
            self.outcome.as_str(),
            self.turns,
            self.detections,
            self.takedowns,
            self.bodies_found,
            usage_counts_json(&self.usage),
            self.alert.peak(),
            self.alert.to_json(),
            self.reinforcements,
            self.par,
            self.score.map_or_else(|| "null".to_string(), |s| s.stars().to_string()),
            score_json(self.score),
        )
    }
}

/// A batch's aggregates: win rate, turns-to-win, and the per-metric totals
/// from §13.2's table. Numbers, never verdicts (§13.4).
#[derive(Clone, PartialEq, Debug)]
pub struct Summary {
    /// The playstyle profile the batch was played under (§13.2), when every run
    /// agrees on one — `None` for a scripted batch, and equally `None` for a
    /// batch whose rows disagree, since no single name would describe it. The
    /// per-profile comparison the playtest skill reports is one summary per
    /// profile, each attributable; a mixed batch honestly refuses to claim one.
    pub profile: Option<&'static str>,
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
    /// The §7.3 ladder across the batch (#376): how the runs' peak rungs were
    /// **distributed**, and which trigger caused each escalation.
    ///
    /// The distribution rather than a maximum, because a maximum cannot tell the two
    /// findings apart: *"most runs end at rung 1"* and *"most runs end at rung 3"* are
    /// opposite balance verdicts and both peak at 3.
    pub alert: AlertTally,
    /// Guards the ladder walked into the facility across the batch (§7.3/#374) — the
    /// count a run actually **faced**, which is not the same claim as the rung it
    /// reached: an arrival is refused when the facility offers nowhere out of sight.
    pub reinforcements: u64,
    /// **How the batch's winning runs scored** (§15 Q4/#563): how many earned 0, 1, 2 and
    /// 3 stars, indexed by the total.
    ///
    /// The distribution rather than a mean, for the reason the alert tally is one: *"most
    /// wins are one-star"* and *"wins are split between none and three"* are opposite
    /// balance findings with the same average.
    ///
    /// **Over the wins only.** A capture has no score, so folding losses in as zero would
    /// report the win rate a second time under another name.
    pub stars: [usize; 4],
    /// **How often each axis was earned**, across the batch's winning runs — in
    /// [`Axis::ALL`] order.
    ///
    /// This is the number that settles the design's own stated risk: if the stealth column
    /// reads near zero across a batch, the condition-0 threshold is impossible rather than
    /// aspirational, and it moves to ≤ 1 before the design is blamed.
    pub axes: [usize; Axis::ALL.len()],
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
        // One name only when every row carries it: a batch mixing profiles (or one
        // with none at all) is `None` rather than borrowing the first row's label.
        let first = records.first().and_then(|r| r.profile);
        let profile = first.filter(|name| records.iter().all(|r| r.profile == Some(*name)));
        Self {
            profile,
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
            alert: AlertTally::of(records.iter().map(|r| &r.alert)),
            reinforcements: records.iter().map(|r| u64::from(r.reinforcements)).sum(),
            stars: records
                .iter()
                .filter_map(|r| r.score)
                .fold([0; 4], |mut tally, score| {
                    tally[score.stars() as usize] += 1;
                    tally
                }),
            axes: Axis::ALL.map(|axis| {
                records
                    .iter()
                    .filter_map(|r| r.score)
                    .filter(|score| score.earned(axis))
                    .count()
            }),
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
            "{{\"summary\":{{\"profile\":{},\"runs\":{},\"wins\":{},\"captures\":{},\"entombed\":{},\"timeouts\":{},\"win_rate\":{:.4},\"turns_to_win_mean\":{},\"turns_to_win_median\":{},\"detections\":{},\"takedowns\":{},\"bodies_found\":{},\"usage\":{},\"usage_share\":{{{}}},\"diversity\":{:.4},\"alert_peak_mean\":{:.4},\"alert_rungs\":{},\"alert_triggers\":{},\"reinforcements\":{},\"stars\":{},\"star_axes\":{}}}}}",
            profile_json(self.profile),
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
            self.alert.peak_mean(self.runs),
            self.alert.rungs_json(),
            self.alert.triggers_json(),
            self.reinforcements,
            tally_json(&self.stars),
            axes_json(&self.axes),
        )
    }
}

/// The star distribution as `{"0":n,"1":n,"2":n,"3":n}` — keyed by the total, so a reader
/// sees the shape rather than an average that hides it.
fn tally_json(stars: &[usize; 4]) -> String {
    let body: Vec<String> = stars
        .iter()
        .enumerate()
        .map(|(n, count)| format!("\"{n}\":{count}"))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// Per-axis win counts as `{"speed":n,...}`, keyed by [`Axis::label`] so the sim's JSON and
/// the game's own screens name the axes the same.
fn axes_json(axes: &[usize; Axis::ALL.len()]) -> String {
    let body: Vec<String> = Axis::ALL
        .iter()
        .zip(axes)
        .map(|(&axis, count)| format!("\"{}\":{count}", axis.label()))
        .collect();
    format!("{{{}}}", body.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alert::AlertRecord;
    use intrusion_core::AlertTrigger;

    fn record(seed: u64, outcome: RunOutcome, turns: u32) -> RunRecord {
        // A fixed sample usage so the schema strings pin deterministically: two
        // Waits and one Run per record. All records share it, so their signatures
        // are identical and the batch diversity is 0.
        let mut usage = UsageHistogram::new();
        usage.record(Verb::Wait);
        usage.record(Verb::Wait);
        usage.record(Verb::Run);
        // A fixed sample climb, likewise: seen once, then seen enough — a two-rung
        // path, so the row pins both the peak and the shape of the escalation list.
        let mut alert = AlertRecord::default();
        alert.record(9, 1, AlertTrigger::Sighting);
        alert.record(31, 2, AlertTrigger::RepeatSightings);
        RunRecord {
            seed,
            profile: Some("balanced"),
            outcome,
            turns,
            detections: 2,
            takedowns: 1,
            bodies_found: 0,
            usage,
            alert,
            // Two of the three rung 3 owes: a fixed sample, so the row pins the field
            // rather than the ladder — an arrival can be refused, so "reached rung 2"
            // and "faced two newcomers" are separate facts and the schema shows both.
            reinforcements: 2,
            par: FIXTURE_PAR,
            // The stars follow the row's own facts (#563): only a win is scored at all,
            // the sample climb above put it at rung 2 so stealth is gone, and the rest
            // rides on the turns the caller names — which is what lets one fixture
            // produce more than one star total.
            score: (outcome == RunOutcome::Win).then_some(Score {
                speed: turns <= FIXTURE_PAR,
                stealth: false,
                thoroughness: true,
            }),
        }
    }

    /// The par the row fixtures are measured against — round, and far enough above the
    /// turns most of them name that a caller opts *out* of the speed star by asking for a
    /// long run.
    const FIXTURE_PAR: u32 = 200;

    /// The row schema, pinned byte-for-byte: this string is what the playtest
    /// skill parses, so any change here is a deliberate, visible break.
    #[test]
    fn the_run_row_schema_is_pinned() {
        assert_eq!(
            record(17, RunOutcome::Win, 214).to_json_line(),
            "{\"seed\":17,\"profile\":\"balanced\",\"outcome\":\"win\",\"turns\":214,\"detections\":2,\"takedowns\":1,\"bodies_found\":0,\"usage\":{\"wait\":2,\"run\":1,\"camouflage\":0,\"decoy\":0,\"dephase\":0,\"autodoors\":0,\"confusion\":0,\"takedown\":0,\"drag\":0,\"pierce_wall\":0,\"lockdown\":0,\"crouch\":0,\"stow\":0,\"silence_radio\":0,\"drone\":0,\"false_call\":0,\"dart\":0,\"repel\":0},\"alert_peak\":2,\"alert_escalations\":[{\"turn\":9,\"rung\":1,\"trigger\":\"sighting\"},{\"turn\":31,\"rung\":2,\"trigger\":\"repeat-sightings\"}],\"reinforcements\":2,\"par\":200,\"stars\":1,\"score\":{\"speed\":false,\"stealth\":false,\"haul\":true}}"
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
            "{\"summary\":{\"profile\":\"balanced\",\"runs\":4,\"wins\":2,\"captures\":1,\"entombed\":0,\"timeouts\":1,\"win_rate\":0.5000,\"turns_to_win_mean\":105.5,\"turns_to_win_median\":105.5,\"detections\":8,\"takedowns\":4,\"bodies_found\":0,\"usage\":{\"wait\":8,\"run\":4,\"camouflage\":0,\"decoy\":0,\"dephase\":0,\"autodoors\":0,\"confusion\":0,\"takedown\":0,\"drag\":0,\"pierce_wall\":0,\"lockdown\":0,\"crouch\":0,\"stow\":0,\"silence_radio\":0,\"drone\":0,\"false_call\":0,\"dart\":0,\"repel\":0},\"usage_share\":{\"wait\":0.0107,\"run\":0.0053,\"camouflage\":0.0000,\"decoy\":0.0000,\"dephase\":0.0000,\"autodoors\":0.0000,\"confusion\":0.0000,\"takedown\":0.0000,\"drag\":0.0000,\"pierce_wall\":0.0000,\"lockdown\":0.0000,\"crouch\":0.0000,\"stow\":0.0000,\"silence_radio\":0.0000,\"drone\":0.0000,\"false_call\":0.0000,\"dart\":0.0000,\"repel\":0.0000},\"diversity\":0.0000,\"alert_peak_mean\":2.0000,\"alert_rungs\":{\"0\":0,\"1\":0,\"2\":4,\"3\":0},\"alert_triggers\":{\"sighting\":4,\"missed-ping\":0,\"repeat-sightings\":4,\"console-tampered\":0,\"body-found\":0,\"second-post-silent\":0},\"reinforcements\":8,\"stars\":{\"0\":0,\"1\":0,\"2\":2,\"3\":0},\"star_axes\":{\"speed\":2,\"stealth\":0,\"haul\":2}}}"
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

    /// #198: a row's `profile` is what makes a batch attributable, so the summary
    /// claims a name **only** when every run agrees on one. A scripted batch has
    /// no temperament (`null`, never a fake `"balanced"`), and a batch whose rows
    /// disagree refuses to pick one rather than borrowing the first row's label.
    #[test]
    fn a_summary_only_claims_a_profile_every_run_agrees_on() {
        let one_profile = Summary::of(&[
            record(1, RunOutcome::Win, 100),
            record(2, RunOutcome::Capture, 40),
        ]);
        assert_eq!(one_profile.profile, Some("balanced"));
        assert!(one_profile
            .to_json_line()
            .starts_with("{\"summary\":{\"profile\":\"balanced\","));

        let mut cautious = record(3, RunOutcome::Win, 90);
        cautious.profile = Some("cautious");
        let mixed = Summary::of(&[record(1, RunOutcome::Win, 100), cautious]);
        assert_eq!(mixed.profile, None, "two temperaments name neither");
        assert!(mixed
            .to_json_line()
            .starts_with("{\"summary\":{\"profile\":null,"));

        let mut scripted = record(4, RunOutcome::Timeout, 10);
        scripted.profile = None;
        assert!(scripted.to_json_line().contains("\"profile\":null"));
        assert_eq!(Summary::of(&[scripted]).profile, None);
        assert_eq!(
            Summary::of(&[]).profile,
            None,
            "nothing played, nothing claimed"
        );
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

    /// **The star metric is over the wins, and `null` is not zero** (#563).
    ///
    /// A capture has no score at all, so it contributes to neither the distribution nor
    /// the per-axis counts — folding losses in as zero-star wins would report the win rate
    /// a second time under another name.
    #[test]
    fn the_star_metric_counts_only_the_runs_that_got_out() {
        let short = record(1, RunOutcome::Win, 100); // inside par: speed + haul
        let long = record(2, RunOutcome::Win, 400); // over par: haul alone
        let lost = record(3, RunOutcome::Capture, 40);
        let capped = record(4, RunOutcome::Timeout, 999);

        let summary = Summary::of(&[short.clone(), long, lost.clone(), capped.clone()]);
        assert_eq!(
            summary.stars,
            [0, 1, 1, 0],
            "one two-star win and one one-star win, and the two losses count nowhere",
        );
        assert_eq!(
            summary.axes,
            [1, 0, 2],
            "speed once, stealth never (the fixture climbs to rung 2), haul twice",
        );
        assert_eq!(summary.stars.iter().sum::<usize>(), summary.wins);

        // …and the rows say the same: a loss is `null`, never a zero-star score.
        assert!(lost.to_json_line().contains("\"stars\":null"));
        assert!(lost.to_json_line().contains("\"score\":null"));
        assert!(capped.to_json_line().contains("\"stars\":null"));
        assert!(short.to_json_line().contains("\"stars\":2"));
    }

    /// A batch nothing won reports an empty distribution rather than a missing field — the
    /// same honesty `turns_to_win_mean`'s `null` keeps one column over.
    #[test]
    fn a_winless_batch_scores_nothing_without_lying_about_it() {
        let summary = Summary::of(&[record(1, RunOutcome::Capture, 40)]);
        assert_eq!(summary.stars, [0; 4]);
        assert_eq!(summary.axes, [0; Axis::ALL.len()]);
        assert!(summary
            .to_json_line()
            .contains("\"stars\":{\"0\":0,\"1\":0,\"2\":0,\"3\":0}"));
    }
}
