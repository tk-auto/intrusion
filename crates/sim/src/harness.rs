//! The run loop (§13.2): one seeded game under a policy, and batches of them.
//!
//! A run boots exactly as the web shell does — `Rng::new(seed)` →
//! [`generate_level`] → [`State::new`] facing north — so a seed here is the same
//! level a player would get from that seed, and a sim finding reproduces in the
//! browser. What the run boots *with* is a [`RunConfig`] (#256): the recipe, the
//! modifiers and the loadout are a batch input, and the sim preset is only the
//! default. Metrics are counted from the core's [`Event`] stream as the run steps,
//! never scraped from state or the rendered grid.

use intrusion_core::{start_level_with, Event, GenError, Input, Outcome};

use crate::alert::AlertRecord;
use crate::config::RunConfig;
use crate::policy::{PlayerPolicy, Recording};
use crate::replay::Replay;
use crate::usage::{UsageHistogram, Verb};

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
    /// Dephase expired somewhere solid with **nowhere in the facility** to throw the
    /// player clear to ([`Event::Entombed`]) — a loss, but a different fact from a
    /// capture, kept distinct like the game-over reason. Since #329 the ordinary
    /// in-a-wall expiry ejects and stuns instead, so this should read `0` on every
    /// generated batch (§10.6 guarantees somewhere to stand); a non-zero count means
    /// a facility that was never playable, not a bold player.
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

/// One run's metrics — the §13.2 table, counted from core events: the starting
/// set (#135), the ability-usage histogram (#137), and the alert ladder's climb
/// (#376).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RunRecord {
    /// The seed the run booted from — with the policy's script, the whole replay (§12.4).
    pub seed: u64,
    /// The playstyle profile that played the run (§13.2), or `None` when the policy
    /// has no temperament (a script). What makes a batch's rows attributable: two
    /// profiles over the same seeds are two rows that say which is which.
    pub profile: Option<&'static str>,
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
    /// The §13.2 ability-usage histogram (#137): a count per verb — the activated
    /// abilities plus Wait/Takedown/Drag/Crouch/Stow — spent this run. Counted from
    /// core events (a refused activation emits none, so it never counts, §4.4);
    /// Wait, alone among verbs, has no event and is recorded from its spent turn.
    pub usage: UsageHistogram,
    /// The facility alert's climb (§7.3/#376) — every escalation with the turn it
    /// happened on and the trigger that caused it, from which the run's **peak rung**
    /// falls out. §13.2's *"whether escalation escalates"* row, in the shape the
    /// ladder has: the *path* up it, not only how high it got.
    pub alert: AlertRecord,
    /// Guards the ladder walked into the facility this run (§7.3/#374) — rung 2 sends
    /// one, rung 3 two more. Counted rather than derived from the peak rung, because an
    /// arrival is **refused** when the facility offers no cell out of the player's
    /// sight: a run can reach rung 3 and face fewer than three newcomers, and the
    /// difference is a fact about the level rather than about the ladder.
    pub reinforcements: u32,
}

/// Run one seeded game under `policy`, to a win, a loss, or `input_cap` issued
/// inputs — whichever comes first. Deterministic (§12.4): the same seed and
/// the same policy decisions produce the same record, byte for byte.
pub fn run_one(
    seed: u64,
    policy: &mut dyn PlayerPolicy,
    input_cap: u32,
) -> Result<RunRecord, GenError> {
    run_one_with(&RunConfig::sim(), seed, policy, input_cap)
}

/// Run one seeded game under `policy` on the batch's [`RunConfig`] — the [`run_one`]
/// loop with what a run boots from opened up as an input (§13.2/#256), so the sim can
/// measure a modifier, a loadout or a guard count rather than the one preset it was
/// compiled with. [`run_one`] is this called with [`RunConfig::sim`].
pub fn run_one_with(
    config: &RunConfig,
    seed: u64,
    policy: &mut dyn PlayerPolicy,
    input_cap: u32,
) -> Result<RunRecord, GenError> {
    // One seed per run (§12.4): the carve stream continues into the turn loop, where
    // the guard close-behind roll draws from it (§10.4/#146), so a sim run is as
    // deterministic and as faithful to the web build as the rest of the pipeline.
    // The sim boots through the *same* [`start_level_with`] path the web shell and
    // the replay viewer use (§13.2) — only the config differs, and it differs by
    // being *said*: `RunConfig::sim` is the baseline gate (`AtLeastOne`, which keeps
    // the bot's outcome profile mixed, §13.3) and the bare innate-only loadout
    // (§8.3), where web quick play requires the full intel set and a tech draw (#244).
    // The §7.3 ladder's thresholds are the batch's too (#376): the sim preset is the
    // shipped **[START]** set, so a batch that names no knob plays the ladder every
    // run plays, and a sweep bends one threshold without a rebuild.
    let mut state =
        start_level_with(&config.facility, &config.level(seed))?.with_alert_tuning(config.alert);

    let mut record = RunRecord {
        seed,
        profile: policy.profile_name(),
        outcome: RunOutcome::Timeout,
        turns: 0,
        detections: 0,
        takedowns: 0,
        bodies_found: 0,
        usage: UsageHistogram::new(),
        alert: AlertRecord::default(),
        reinforcements: 0,
    };
    for _ in 0..input_cap {
        let input = policy.decide(&state);
        let turns_before = state.turn();
        for event in state.step(input) {
            // Every verb but Wait is counted from the event it emits (#137, §13.2) and
            // never from the input that was issued: an activation the economy refused
            // costs no turn and emits none, so it never counts (§4.4), and a free
            // action has no slot to reach at all. [`Verb::of_event`] owns that mapping,
            // including which events are on the free side of the line.
            if let Some(verb) = Verb::of_event(event) {
                record.usage.record(verb);
            }
            match event {
                Event::Detected { .. } => record.detections += 1,
                Event::TakenDown { .. } => record.takedowns += 1,
                Event::BodyFound { .. } => record.bodies_found += 1,
                // The ladder stepped (§7.3/#376). The core reports *escalations*, not
                // occurrences — a trigger at or below the current rung says nothing —
                // so this is the run's path up the ladder and nothing else. The turn
                // is read from the state rather than carried on the event: phase 1
                // advances the counter before the world phases run, so the state's
                // turn *is* the turn the escalation happened on, whichever phase
                // produced it.
                Event::AlertRaised { rung, trigger } => {
                    record.alert.record(state.turn(), rung, trigger);
                }
                // Guards the ladder walked in (§7.3/#374), counted so the guard count a
                // run actually **faced** is visible rather than inferred from the rung:
                // an arrival can be refused when the facility offers nowhere out of
                // sight, so "reached rung 3" and "faced three more guards" are not the
                // same claim.
                Event::ReinforcementArrived { .. } => record.reinforcements += 1,
                Event::Won => record.outcome = RunOutcome::Win,
                Event::Captured { .. } => record.outcome = RunOutcome::Capture,
                Event::Entombed { .. } => record.outcome = RunOutcome::Entombed,
                _ => {}
            }
        }
        // Wait is the one verb with no event of its own — it spends the turn in
        // silence (§8.3). A Wait can never be refused, so a spent turn is the whole
        // truth an event would carry: record it when the turn actually advanced.
        if matches!(input, Input::Wait) && state.turn() > turns_before {
            record.usage.record(Verb::Wait);
        }
        if state.outcome() != Outcome::Playing {
            break;
        }
    }
    record.turns = state.turn();
    Ok(record)
}

/// Run one seeded game under `policy` and capture the replay alongside the
/// metrics (§12.4): the same [`run_one`] loop, but with the policy wrapped so
/// every issued input is recorded. Returns the run's [`RunRecord`] and the
/// [`Replay`] — `(level, [inputs])` — that reproduces it through
/// [`Scripted`](crate::Scripted).
///
/// The replay carries the [`LevelSeed`](intrusion_core::LevelSeed) it was **actually
/// captured under**, not a bare seed and not a fixed preset (#245/#256): the config's
/// modifiers and loadout go into the token, so a baked replay boots the run the sim
/// played rather than one that drifted underneath it. Recording is a transparent
/// decorator, so the captured run is byte-identical to an unwrapped one; feeding the
/// returned inputs back on the same config lands on the same record. This is the sim
/// half of the replay loop the web viewer (#197) plays back.
pub fn capture_one<P: PlayerPolicy>(
    seed: u64,
    policy: P,
    input_cap: u32,
) -> Result<(RunRecord, Replay), GenError> {
    capture_one_with(&RunConfig::sim(), seed, policy, input_cap)
}

/// Capture a run under an explicit [`RunConfig`] — [`capture_one`] with what the run
/// boots from as an input (§13.2/#256).
///
/// The baked replay carries that config, with one honest gap: the **facility recipe**
/// is not part of the shareable token, so a replay captured off a swept `--guards`
/// only reproduces under the same `--guards` (the web viewer plays the v1 recipe).
pub fn capture_one_with<P: PlayerPolicy>(
    config: &RunConfig,
    seed: u64,
    policy: P,
    input_cap: u32,
) -> Result<(RunRecord, Replay), GenError> {
    let mut recording = Recording::new(policy);
    let record = run_one_with(config, seed, &mut recording, input_cap)?;
    let replay = Replay {
        level: config.level(seed),
        inputs: recording.into_inputs(),
    };
    Ok((record, replay))
}

/// Run a batch: one run per seed, each under a fresh policy from `policy_for`
/// — policies are stateful (a script cursor), so sharing one would leak state
/// between runs. A generation failure aborts the batch loudly with the seed
/// that failed; it never ships a silent shortfall.
pub fn run_batch<P: PlayerPolicy>(
    seeds: impl IntoIterator<Item = u64>,
    input_cap: u32,
    policy_for: impl FnMut(u64) -> P,
) -> Result<Vec<RunRecord>, (u64, GenError)> {
    run_batch_with(&RunConfig::sim(), seeds, input_cap, policy_for)
}

/// Run a batch under `config` — [`run_batch`] with what every run boots from as an
/// input (§13.2/#256), the entry point a modifier, loadout or guard-count sweep
/// drives. [`run_batch`] is this called with [`RunConfig::sim`].
///
/// The config is fixed across the batch and the **seed** is what varies, which is
/// what makes a batch's rows comparable: generation is seed-derived and independent
/// of modifiers and loadout, so two configs over the same seeds raid the same
/// facilities (the property a paired A/B rests on, #257).
pub fn run_batch_with<P: PlayerPolicy>(
    config: &RunConfig,
    seeds: impl IntoIterator<Item = u64>,
    input_cap: u32,
    mut policy_for: impl FnMut(u64) -> P,
) -> Result<Vec<RunRecord>, (u64, GenError)> {
    seeds
        .into_iter()
        .map(|seed| {
            run_one_with(config, seed, &mut policy_for(seed), input_cap).map_err(|e| (seed, e))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::Scripted;
    use crate::{Profile, StealthBot};
    use intrusion_core::{AbilityId, Direction, Input, IntelGate};

    /// **The default config changes nothing** (#256): a batch given no config is the
    /// batch the sim ran before the config was an input at all.
    ///
    /// Pinned as a literal row captured from the previous build rather than compared
    /// against a freshly computed one, because the failure this guards against is
    /// precisely a default that quietly moved — a self-comparison would agree with
    /// itself all the way to a changed baseline.
    ///
    /// The row is the *game's* output, so a change to the game moves it and the refresh
    /// belongs in that PR with the delta read.
    ///
    /// **#466 moved it, and again for a reason that is only half about the game**: the
    /// exit is now the mouth of a tunnel the player starts inside (§4.5), so placement
    /// picks a different `E`, the whole draw shifts behind it (the comms console moved
    /// ahead of the guards so the #232 knob stops perturbing it), and seed 42 carves a
    /// **different facility** — and moving the tunnel's [START] length to 8–16 after the
    /// first playtest re-carved it again, to a 168-turn win. Any run's turns now include
    /// the crawl in and the crawl out, which every run pays.
    /// Nothing about that comparison means anything on its own; the PR's 100-seed
    /// batches are where the reading is.
    ///
    /// **#452 moved it the same way before that**: making automatic doors a level
    /// modifier dropped the per-doorway RNG draw, so seed 42 re-carved and a 130-turn
    /// win became the capture at 98 this row held until #466.
    ///
    /// **#481 moved it again, and this one is a re-carve too**: refusing a console or
    /// the exit any cell whose stamping would seal walkable ground off changes which
    /// cell each usable lands on, so seed 42 carves and places a different board. The
    /// 168-turn win became a capture at 116.
    ///
    /// Before that it moved when the guards began **partitioning the whole level**
    /// (§7.5) — seed 42 was a 111-turn win with zero detections through ground nobody
    /// patrolled — which *was* a real cost, and is the sharpest illustration of one in
    /// the suite. The two kinds of move are worth telling apart when reading this
    /// history: a re-carve makes the row incomparable, a rule change makes it evidence.
    #[test]
    fn the_default_config_reproduces_the_hardcoded_preset_byte_for_byte() {
        const PINNED: &str = "{\"seed\":42,\"profile\":\"balanced\",\"outcome\":\"capture\",\"turns\":116,\"detections\":3,\"takedowns\":0,\"bodies_found\":0,\"usage\":{\"wait\":12,\"run\":1,\"camouflage\":0,\"decoy\":0,\"dephase\":0,\"autodoors\":0,\"confusion\":0,\"takedown\":0,\"drag\":0,\"pierce_wall\":0,\"lockdown\":0,\"crouch\":0,\"stow\":0,\"silence_radio\":0},\"alert_peak\":1,\"alert_escalations\":[{\"turn\":116,\"rung\":1,\"trigger\":\"sighting\"}],\"reinforcements\":0}";
        let record = run_one(42, &mut StealthBot::new(), 400).expect("generates");
        assert_eq!(record.to_json_line(), PINNED);
        // …and the explicit default is the same run, not merely a similar one.
        let explicit = run_one_with(&RunConfig::default(), 42, &mut StealthBot::new(), 400)
            .expect("generates");
        assert_eq!(explicit, record);
    }

    /// The config **reaches the run**: a batch that grants tech plays a game holding
    /// it, and one that bends a modifier plays a game bent by it. Asserted through
    /// the booted state rather than through a metric, so it holds whatever the bot
    /// decides to do with them (#256 is the plumbing; #347 is the deciding).
    #[test]
    fn the_config_reaches_the_booted_run() {
        let config = RunConfig::sim()
            .with_tech("camouflage,decoy")
            .expect("known abilities")
            .with_intel_gate(IntelGate::All)
            .with_modifier("full-layout-known")
            .expect("a known modifier");
        let state = intrusion_core::start_level_with(&config.facility, &config.level(7))
            .expect("generates");
        assert!(state.loadout().contains(AbilityId::Camouflage));
        assert!(state.loadout().contains(AbilityId::Decoy));
        assert_eq!(state.modifiers().intel_to_exit, IntelGate::All);
        assert!(state.modifiers().full_layout_known);
        // The facility a seed carves is independent of the config it is played under
        // — the property a paired A/B (#257) rests on.
        let sim = RunConfig::sim();
        let bare =
            intrusion_core::start_level_with(&sim.facility, &sim.level(7)).expect("generates");
        assert_eq!(
            terrain_of(&state),
            terrain_of(&bare),
            "the config moved the carve"
        );
        assert_eq!(state.player(), bare.player());
    }

    /// Every cell's terrain, as the fingerprint of a carved facility — `Facility` is
    /// storage rather than a value type, so a comparison is spelled out here.
    fn terrain_of(state: &intrusion_core::State) -> Vec<Option<intrusion_core::Terrain>> {
        let facility = state.layout().facility();
        (0..facility.height())
            .flat_map(|y| (0..facility.width()).map(move |x| (x, y)))
            .map(|(x, y)| facility.terrain_at(x, y))
            .collect()
    }

    /// A captured replay carries the config it was **played under** (#245/#256), not
    /// the sim preset: the token decodes back to the very loadout and modifiers the
    /// run held, so a non-default batch's replay reproduces rather than approximates.
    #[test]
    fn a_captured_replay_carries_the_config_it_played() {
        let config = RunConfig::sim()
            .with_tech("camouflage")
            .expect("a known ability")
            .with_intel_gate(IntelGate::All);
        let (_, replay) = capture_one_with(&config, 42, StealthBot::new(), 200).expect("generates");
        assert_eq!(replay.level, config.level(42));
        let token = replay.level.encode().expect("a holdable config");
        assert_eq!(
            intrusion_core::LevelSeed::decode(&token),
            Some(config.level(42)),
            "the baked token names the captured run",
        );
    }

    /// #376: the ladder is **measured through the run**, from the core's own
    /// escalation events — the turn each rung was reached and the trigger that got it
    /// there. Driven with the `careless` profile, which strikes and leaves the body,
    /// so both halves of the ladder (being seen, and the radio) have a chance to fire.
    ///
    /// The assertions are about the record's *shape*, not about a particular seed's
    /// climb: rungs only rise (§7.3 no decay), turns only advance, every escalation
    /// names the rung its trigger reaches, and the peak agrees with the path. A seed
    /// whose facility never notices records an empty climb and a peak of **0**, which
    /// is a reading rather than the old `null`.
    #[test]
    fn a_runs_climb_up_the_ladder_is_recorded_from_its_events() {
        let mut climbed_somewhere = false;
        for seed in 0..12 {
            let record = run_one_with(
                &RunConfig::sim(),
                seed,
                &mut StealthBot::with_profile(Profile::CARELESS),
                400,
            )
            .expect("generates");

            let (mut rung, mut turn) = (0, 0);
            for escalation in record.alert.escalations() {
                assert!(
                    escalation.rung > rung,
                    "seed {seed}: the rung went {rung} → {} — the ladder has no decay",
                    escalation.rung,
                );
                assert!(
                    escalation.turn >= turn,
                    "seed {seed}: an escalation ran back"
                );
                assert_eq!(
                    escalation.rung,
                    escalation.trigger.rung(),
                    "seed {seed}: {:?} reported a rung it does not reach",
                    escalation.trigger,
                );
                assert!(
                    escalation.turn <= record.turns,
                    "seed {seed}: an escalation after the run ended",
                );
                (rung, turn) = (escalation.rung, escalation.turn);
            }
            assert_eq!(record.alert.peak(), rung, "seed {seed}: peak vs path");
            climbed_somewhere |= rung > 0;
        }
        assert!(
            climbed_somewhere,
            "no seed in the sweep escalated at all — the row would be measuring nothing",
        );
    }

    /// #376: the ladder's thresholds are a **batch input**, and they reach the run.
    /// A tuning that makes one turn of contact a confirmed sighting escalates strictly
    /// more often than the shipped ladder over the same seeds — the property a sweep
    /// rests on, and the one that fails if the knob is exposed but never read.
    ///
    /// **Forty seeds, not twelve.** Once the guards partitioned the whole level (§7.5)
    /// rather than covering part of it, the careless bot's contacts got long enough
    /// that the two ladders confirmed the same sightings on all twelve — 16 against 16,
    /// equal rather than reversed. The knob still bites; twelve seeds had simply stopped
    /// being a wide enough sample to see it.
    #[test]
    fn a_swept_alert_threshold_moves_the_measured_ladder() {
        let batch = |config: &RunConfig| -> u32 {
            run_batch_with(config, 0..40, 400, |_| {
                StealthBot::with_profile(Profile::CARELESS)
            })
            .expect("generates")
            .iter()
            .map(|r| r.alert.peak())
            .sum()
        };

        let shipped = RunConfig::sim();
        let touchy = shipped
            .with_alert("sighting-contact-turns=1")
            .expect("a known knob")
            .validated()
            .expect("a legal ladder");
        assert!(
            batch(&touchy) > batch(&shipped),
            "a facility that reports every glance must escalate more, not the same",
        );
    }

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

    /// #137, §13.2: a scripted policy with known inputs yields the **exact**
    /// expected histogram, counted from core events. An all-Wait run spends every
    /// one of its turns waiting, so `wait == turns` and every other verb is 0 — a
    /// seed-independent invariant, since a Wait always spends the turn and can
    /// never be refused.
    #[test]
    fn an_all_wait_run_counts_a_wait_per_turn() {
        for seed in [0, 7, 42] {
            let cap = 60;
            let record = run_one(
                seed,
                &mut Scripted::new(vec![Input::Wait; cap as usize]),
                cap,
            )
            .expect("generates");
            assert_eq!(
                record.usage.count(Verb::Wait),
                record.turns,
                "seed {seed}: every spent turn was a Wait",
            );
            for verb in Verb::ALL {
                if verb != Verb::Wait {
                    assert_eq!(record.usage.count(verb), 0, "seed {seed}: {verb:?} unused");
                }
            }
        }
    }

    /// #137: an ability activation is counted from its event, and exactly once. The
    /// first input activates Run — which happens in the player phase, before any guard
    /// can act — so `run == 1` on every seed, and a *refused* re-activation (Run is now
    /// active, then cooling) counts nothing (§4.4). Wait fills the rest. Run, not a
    /// tech ability, because the sim boots the bare innate-only loadout: activating
    /// tech it does not hold would be a free no-op that counts nothing.
    #[test]
    fn an_activation_is_counted_once_from_its_event() {
        for seed in [0, 7, 42] {
            let cap = 30;
            let mut script = vec![
                Input::Activate(AbilityId::Run),
                Input::Activate(AbilityId::Run), // refused: already active — must not count
            ];
            script.resize(cap as usize, Input::Wait);
            let record = run_one(seed, &mut Scripted::new(script), cap).expect("generates");
            assert_eq!(
                record.usage.count(Verb::Run),
                1,
                "seed {seed}: one activation, one count — the refused retry is free",
            );
            assert_eq!(
                record.usage.count(Verb::Dephase),
                0,
                "seed {seed}: Dephase never fired"
            );
        }
    }

    /// #137 determinism (§12.4): the same batch config produces byte-identical
    /// metrics, the usage histogram and diversity included.
    #[test]
    fn the_usage_metrics_are_deterministic() {
        let script = vec![
            Input::Activate(AbilityId::Run),
            Input::Step(Direction::North),
            Input::Wait,
            Input::Step(Direction::East),
        ];
        let batch = |()| {
            crate::Summary::of(
                &run_batch(0..5, 80, |_| Scripted::new(script.clone())).expect("generates"),
            )
            .to_json_line()
        };
        assert_eq!(batch(()), batch(()), "same config → byte-identical metrics");
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
