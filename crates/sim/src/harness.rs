//! The run loop (§13.2): one seeded game under a policy, and batches of them.
//!
//! A run boots exactly as the web shell does — `Rng::new(seed)` →
//! [`generate_level`] with [`LevelConfig::V1`] → [`State::new`] facing north —
//! so a seed here is the same level a player would get from that seed, and a
//! sim finding reproduces in the browser. Metrics are counted from the core's
//! [`Event`] stream as the run steps, never scraped from state or the rendered
//! grid.

use intrusion_core::{
    generate_level, Direction, Event, GenError, LevelConfig, Outcome, Rng, State,
};

use crate::policy::PlayerPolicy;

/// The default cap on **issued inputs** per run before it is ruled a timeout.
///
/// The cap counts what the policy *issues*, not turns the game spends: free
/// actions — a bump into a wall, a refused exit, an idle deactivate — never
/// advance the turn counter (§4.4), so a stuck policy spamming them would hang
/// a turn-capped batch forever. Counting inputs terminates every run
/// unconditionally.
pub const DEFAULT_INPUT_CAP: u32 = 1000;

/// How one run ended. Wins and the two loss shapes come from the core's own
/// end-of-run events; a run nothing ended by the input cap is a timeout —
/// recorded honestly, never coerced into a loss.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RunOutcome {
    /// Every objective in hand and the exit reached ([`Event::Won`]).
    Win,
    /// A guard walked into the player ([`Event::Captured`]).
    Capture,
    /// Dephase expired somewhere solid ([`Event::Entombed`]) — a loss, but a
    /// different fact from a capture, kept distinct like the game-over reason.
    Entombed,
    /// The input cap ran out with the run still live.
    Timeout,
}

impl RunOutcome {
    /// The stable string the JSON schema uses (see `crates/sim/README.md`).
    pub fn as_str(self) -> &'static str {
        match self {
            RunOutcome::Win => "win",
            RunOutcome::Capture => "capture",
            RunOutcome::Entombed => "entombed",
            RunOutcome::Timeout => "timeout",
        }
    }
}

/// One run's metrics — the §13.2 starting table, counted from core events.
///
/// The ability-usage histogram and strategy diversity are the companion
/// metrics ticket; the facility-wide alert peak needs the radio net (#107)
/// before it exists to measure, so it is not a field yet — the JSON row emits
/// it as `null` (see `crates/sim/README.md`) rather than faking a number.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RunRecord {
    /// The seed the run booted from — with the policy's script, the whole replay (§12.4).
    pub seed: u64,
    /// How the run ended.
    pub outcome: RunOutcome,
    /// Spent turns at the end of the run ([`State::turn`]) — free actions excluded.
    pub turns: u32,
    /// Fresh detections ([`Event::Detected`]): how often stealth broke — a held
    /// chase counts once, not once per turn.
    pub detections: u32,
    /// Takedowns landed ([`Event::TakenDown`]) — whether §7.2's cost is real.
    pub takedowns: u32,
    /// Bodies found by guards ([`Event::BodyFound`]) — whether §7.3's clock has teeth.
    pub bodies_found: u32,
}

/// Run one seeded game under `policy`, to a win, a loss, or `input_cap` issued
/// inputs — whichever comes first. Deterministic (§12.4): the same seed and
/// the same policy decisions produce the same record, byte for byte.
pub fn run_one(
    seed: u64,
    policy: &mut dyn PlayerPolicy,
    input_cap: u32,
) -> Result<RunRecord, GenError> {
    let (layout, placement) = generate_level(&LevelConfig::V1, &mut Rng::new(seed))?;
    let mut state = State::new(
        layout,
        placement.player(),
        Direction::North,
        placement.guards(),
        placement.intel().iter().copied(),
        placement.exit(),
    );

    let mut record = RunRecord {
        seed,
        outcome: RunOutcome::Timeout,
        turns: 0,
        detections: 0,
        takedowns: 0,
        bodies_found: 0,
    };
    for _ in 0..input_cap {
        let input = policy.decide(&state);
        for event in state.step(input) {
            match event {
                Event::Detected { .. } => record.detections += 1,
                Event::TakenDown { .. } => record.takedowns += 1,
                Event::BodyFound { .. } => record.bodies_found += 1,
                Event::Won => record.outcome = RunOutcome::Win,
                Event::Captured { .. } => record.outcome = RunOutcome::Capture,
                Event::Entombed { .. } => record.outcome = RunOutcome::Entombed,
                _ => {}
            }
        }
        if state.outcome() != Outcome::Playing {
            break;
        }
    }
    record.turns = state.turn();
    Ok(record)
}

/// Run a batch: one run per seed, each under a fresh policy from `policy_for`
/// — policies are stateful (a script cursor), so sharing one would leak state
/// between runs. A generation failure aborts the batch loudly with the seed
/// that failed; it never ships a silent shortfall.
pub fn run_batch<P: PlayerPolicy>(
    seeds: impl IntoIterator<Item = u64>,
    input_cap: u32,
    mut policy_for: impl FnMut(u64) -> P,
) -> Result<Vec<RunRecord>, (u64, GenError)> {
    seeds
        .into_iter()
        .map(|seed| run_one(seed, &mut policy_for(seed), input_cap).map_err(|e| (seed, e)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Scripted;
    use intrusion_core::{AbilityId, Input};

    /// The acceptance criterion verbatim (§12.4): the same `(seed, policy)`
    /// twice produces byte-identical metric rows.
    #[test]
    fn the_same_seed_and_policy_twice_is_byte_identical() {
        for seed in [0, 7, 42] {
            let script = vec![
                Input::Step(Direction::North),
                Input::Step(Direction::East),
                Input::Wait,
                Input::Step(Direction::South),
            ];
            let a = run_one(seed, &mut Scripted::new(script.clone()), 120).expect("generates");
            let b = run_one(seed, &mut Scripted::new(script), 120).expect("generates");
            assert_eq!(a, b, "seed {seed}: a replay replays");
            assert_eq!(
                a.to_json_line(),
                b.to_json_line(),
                "seed {seed}: rows are byte-identical"
            );
        }
    }

    /// A stuck policy must terminate as a timeout, never hang the batch — and
    /// the cap counts **issued inputs**, not spent turns: a policy spamming
    /// free actions (an idle deactivate never costs a turn, §4.4) ends at the
    /// cap with the turn counter still at zero.
    #[test]
    fn a_free_action_loop_terminates_at_the_input_cap() {
        let cap = 40;
        let mut policy = Scripted::new(vec![Input::Deactivate(AbilityId::Run); cap as usize]);
        let record = run_one(3, &mut policy, cap).expect("generates");
        assert_eq!(record.outcome, RunOutcome::Timeout);
        assert_eq!(record.turns, 0, "free actions never spend a turn");
    }

    /// An idle run (the empty script: wait to the cap) terminates and reports
    /// coherent numbers: spent turns never exceed issued inputs, and the
    /// outcome is a timeout unless a patrol stumbled onto the idle player.
    #[test]
    fn an_idle_run_terminates_with_coherent_numbers() {
        let cap = 80;
        let records = run_batch(0..4, cap, |_| Scripted::new(Vec::new())).expect("generates");
        assert_eq!(records.len(), 4);
        for r in &records {
            assert!(
                r.turns <= cap,
                "seed {}: {} turns > cap {cap}",
                r.seed,
                r.turns
            );
            assert!(
                matches!(r.outcome, RunOutcome::Timeout | RunOutcome::Capture),
                "seed {}: an idle player cannot win or entomb, got {:?}",
                r.seed,
                r.outcome
            );
            assert_eq!(
                r.takedowns, 0,
                "seed {}: idle players strike no one",
                r.seed
            );
        }
    }
}
