//! Replay capture (§12.4): a run is `(level, [inputs])`, and this is the sim's
//! shareable form of one.
//!
//! The *notation* — the string that spells an [`Input`] stream — lives in the
//! core ([`intrusion_core::to_script`]/[`parse_script`](intrusion_core::parse_script)),
//! shared by every consumer. What lives here is the sim-specific half: the
//! [`Replay`] value the `--emit-replay` mode hands out (the level paired with the
//! captured inputs), and the end-to-end round-trip test that a captured bot run
//! replays byte-for-byte through [`Scripted`](crate::Scripted).

use intrusion_core::{to_script, Input, LevelSeed};

/// A captured run: the [`LevelSeed`] and the exact [`Input`] stream a policy issued
/// (§12.4). The pair is the whole replay — feed `inputs` back through
/// [`Scripted`](crate::Scripted) on the level's seed and the run reproduces.
///
/// The level, not a bare seed, is what the replay carries (#245): it is the run's
/// whole config `(seed, modifiers, abilities)`, so a baked replay boots the same
/// preset the sim captured it under — the `AtLeastOne` intel gate (§13.3), not quick
/// play's stricter one — and the playback matches the captured run exactly.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Replay {
    /// The level the run booted from — its whole reproducible config.
    pub level: LevelSeed,
    /// The inputs issued, in order.
    pub inputs: Vec<Input>,
}

impl Replay {
    /// The replay's inputs in the core script notation ([`to_script`]).
    pub fn script(&self) -> String {
        to_script(&self.inputs)
    }

    /// The replay as a single JSON line — the §12.4 pair and nothing else, the
    /// shareable form `--emit-replay` prints and slice C bakes into an Artifact. The
    /// `seed` field is the **level-seed token** ([`LevelSeed::encode`], #333), so the
    /// baked run reproduces its whole config and not just its geometry (#245) —
    /// inputs replayed against a config that drifted underneath them are meaningless
    /// (§12.4). `inputs` is the script string; feed it back on the level to reproduce
    /// the run.
    ///
    /// A config with no token (one no run can hold — see [`LevelSeed::encode`]) emits
    /// an empty `seed`, which decodes to nothing and so falls to a fresh run rather
    /// than to a plausible wrong one (#110). No sim preset is such a config: the sim
    /// boots the innate-only baseline (§13.3), well inside the §8.3 cap.
    pub fn to_json_line(&self) -> String {
        format!(
            "{{\"seed\":\"{}\",\"inputs\":\"{}\"}}",
            self.level.encode().unwrap_or_default(),
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
    /// inputs)` pair, the `seed` field the **level-seed token** carrying the captured
    /// preset (#245/#333), the inputs in the script notation, nothing else.
    ///
    /// The token is pinned as a literal rather than recomputed, so a change to the
    /// format shows up here as a failing schema — a baked replay is read back by a
    /// build that may be newer than the one that wrote it.
    #[test]
    fn the_emit_schema_is_pinned() {
        let replay = Replay {
            level: LevelSeed::sim(42),
            inputs: vec![
                Input::Step(Direction::North),
                Input::Activate(AbilityId::Run),
                Input::Wait,
            ],
        };
        assert_eq!(
            replay.to_json_line(),
            "{\"seed\":\"nfdxttsytrdorexcqn\",\"inputs\":\"N+r.\"}"
        );
        // The baked token decodes straight back to the captured preset.
        assert_eq!(
            LevelSeed::decode("nfdxttsytrdorexcqn"),
            Some(LevelSeed::sim(42))
        );
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

            // Replaying it reproduces the run exactly (§12.4). The one field that
            // legitimately differs is `profile`: it records *who decided*, not what
            // happened, and a script has no temperament — so a replayed row says
            // `null` where the bot's said `balanced` (#198). Every game metric
            // must still match, so the comparison normalises that field rather
            // than dropping the byte-for-byte check.
            let mut round_trip =
                run_one(seed, &mut Scripted::new(reparsed), cap).expect("generates");
            assert_eq!(
                (original.profile, round_trip.profile),
                (Some("balanced"), None),
                "seed {seed}: a replayed row must not claim the bot's temperament",
            );
            round_trip.profile = original.profile;
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
