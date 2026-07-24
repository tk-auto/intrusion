//! Replay capture (§12.4): a run is `(seed, [inputs])`, and this is the sim's
//! shareable form of one.
//!
//! The *notation* — the string that spells an [`Input`] stream — lives in the
//! core ([`intrusion_core::to_script`]/[`parse_script`](intrusion_core::parse_script)),
//! shared by every consumer. What lives here is the sim-specific half: the
//! [`Replay`] value the `--emit-replay` mode hands out (the seed paired with the
//! captured inputs), and the end-to-end round-trip test that a captured bot run
//! replays byte-for-byte through [`Scripted`](crate::Scripted).

use intrusion_core::{to_script, Input};

/// A captured run: the seed and the exact [`Input`] stream a policy issued
/// (§12.4). The pair is the whole replay — feed `inputs` back through
/// [`Scripted`](crate::Scripted) on `seed` and the run reproduces.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Replay {
    /// The seed the run booted from.
    pub seed: u64,
    /// The inputs issued, in order.
    pub inputs: Vec<Input>,
}

impl Replay {
    /// The replay's inputs in the core script notation ([`to_script`]).
    pub fn script(&self) -> String {
        to_script(&self.inputs)
    }

    /// The replay as a single JSON line — the §12.4 pair and nothing else, the
    /// shareable form `--emit-replay` prints and slice C bakes into an Artifact.
    /// `inputs` is the script string; feed it back on `seed` to reproduce the run.
    pub fn to_json_line(&self) -> String {
        format!(
            "{{\"seed\":{},\"inputs\":\"{}\"}}",
            self.seed,
            self.script()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{run_one, Scripted, StealthBot};
    use intrusion_core::{parse_script, AbilityId, Direction};

    /// The emit schema is pinned byte-for-byte (slice C reads it): the `(seed,
    /// inputs)` pair, the inputs in the script notation, nothing else.
    #[test]
    fn the_emit_schema_is_pinned() {
        let replay = Replay {
            seed: 42,
            inputs: vec![
                Input::Step(Direction::North),
                Input::Activate(AbilityId::Run),
                Input::Wait,
            ],
        };
        assert_eq!(replay.to_json_line(), "{\"seed\":42,\"inputs\":\"N+r.\"}");
    }

    /// The §12.4 property, asserted end to end (slice A acceptance): capture a bot
    /// run's issued inputs, spell them as a script, parse it back, and replay it
    /// through [`Scripted`] — the replay reproduces the run byte-for-byte (same
    /// outcome, turns, and every metric).
    #[test]
    fn a_captured_bot_run_replays_identically_through_the_script() {
        let cap = 400;
        for seed in [0, 1, 7, 42, 99] {
            let (original, replay) =
                crate::capture_one(seed, StealthBot::new(), cap).expect("generates");

            // The captured stream survives a trip through the text notation.
            let script = replay.script();
            let reparsed = parse_script(&script).expect("a captured stream re-parses");
            assert_eq!(reparsed, replay.inputs, "seed {seed}: notation is lossy");

            // Replaying it reproduces the run exactly (§12.4).
            let round_trip = run_one(seed, &mut Scripted::new(reparsed), cap).expect("generates");
            assert_eq!(
                round_trip, original,
                "seed {seed}: the replay did not reproduce the bot run"
            );
            assert_eq!(
                round_trip.to_json_line(),
                original.to_json_line(),
                "seed {seed}: rows differ byte-for-byte"
            );
        }
    }
}
