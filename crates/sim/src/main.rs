//! The `sim` binary (§13.2): batch runner over the harness library.
//!
//! Parses a handful of flags, runs N seeded games under a scripted policy, and
//! prints one JSON line per run plus a final summary line to stdout — the
//! machine-readable stream the playtest skill consumes (schema:
//! `crates/sim/README.md`). Argument parsing is hand-rolled: a handful of flags
//! do not buy a dependency.

use std::process::ExitCode;

use intrusion_core::{parse_script, Input, LevelSeed};
use intrusion_sim::{
    ability_names, alert_knob_names, capture_one_with, intel_gate_named, intel_gate_names,
    modifier_names, run_batch_with, Profile, RunConfig, Scripted, StealthBot, Summary,
    DEFAULT_INPUT_CAP,
};

const USAGE: &str = "\
Usage: sim [--runs N] [--seed S] [--cap N] [--config TOKEN] [--guards N]
           [--intel-gate none|one|all] [--modifier NAME]... [--alert NAME=N]...
           [--abilities LIST] [--without LIST]
           [--bot [--profile NAME] | --script MOVES] [--emit-replay]

Run N seeded games headlessly and print JSON lines: one row per run, then a
summary row (schema: crates/sim/README.md).

  --runs N       how many runs; seeds are S, S+1, ... S+N-1   (default 100)
  --seed S       the first seed                               (default 0)
  --cap N        inputs issued per run before it is ruled a
                 timeout                                      (default 1000)
  --config TOKEN a level-seed token (§12.4/#245): the batch
                 runs that token's modifiers and loadout, and
                 its seed is the first seed unless --seed says
                 otherwise                                    (default: the sim preset)
  --guards N     guards to place per facility — the §10.2
                 recipe knob the balance sweep drives         (default 4)
  --intel-gate G how much intel the exit asks for (§4.5):
                 none, one or all                             (default: one)
  --modifier N   switch a level modifier on (#225); repeatable
                 and comma-separated. --help lists them        (default: none)
  --alert K=N    set one §7.3 alert-ladder threshold — how hard
                 a rung is to reach; repeatable and comma-
                 separated. --help lists the knobs             (default: the §7.3 [START]s)
  --abilities L  the tech every run holds (§8.3), comma-
                 separated; the innate set is always held      (default: none — bare)
  --without L    tech to drop from the loadout                 (default: none)
  --bot          play each run with the baseline stealth bot
                 instead of a script (design §13.2)           (default: off)
  --profile NAME the bot's playstyle temperament (§13.2): baseline, cautious,
                 aggressive or careless; needs --bot        (default: baseline)
  --script MOVES inputs replayed from the start of every run:
                 N/E/S/W step, `.` waits, +/- an ability key
                 (e.g. +r) activates/deactivates; after the
                 script the player waits out the run          (default: empty)
  --emit-replay  play one run (seed S) and print its captured
                 replay `{seed,inputs}` (§12.4) instead of the
                 metrics batch — the shareable form            (default: off)

--bot is the balance-signal mode: a greedy stealth player that explores, takes
the intel and leaves, fleeing to hideouts when hunted (§13.2–§13.4). Without it
the empty default script is the idle baseline: how often patrols stumble onto a
player who never moves. A per-seed script is a replay (design §12.4) — with
--runs 1 it reproduces a single run exactly. --bot and --script are exclusive.

--profile picks which temperament plays: one policy at different settings, not
three bots. Running the same seeds under two profiles is how §13.2's strategy
diversity becomes visible — a seed solvable both cautiously and aggressively is
healthy; one where both collapse onto the same line is a puzzle with one answer.
Every emitted row names the profile that produced it.

The config flags are what a batch is *measuring* (#256): the default is the sim
preset — the baseline rules, a bare innate-only loadout and the shipped alert
ladder, so a win rate says the core stealth loop is winnable with no tech — and
every flag above states a departure from it. They compose in a fixed order
whatever order they are written in: --config, --guards, --intel-gate, --modifier,
--alert, --abilities, --without. A batch running one flag against none of it is
the toggle experiment (#257), and generation is seed-derived, so both arms raid
the same facilities.

--alert is the ladder's own sweep (§7.3/#376): every threshold the design marks
[START] — how many contact turns make a sighting, how long the window is, how
many sightings or quiet posts reach a rung, how short the alerted dwell is —
turned from a flag instead of a rebuild. A ladder the design forbids (a dwell
floor of 0, a window too short to ever hold a sighting) is refused at the flag
rather than measured. It is not carried by --emit-replay's token: no shared
config can encode it, so a swept run reproduces only under the same --alert.

--emit-replay captures the chosen policy's issued inputs for the single seed S
and prints the `(seed, inputs)` replay: with --bot, the exact run the bot
played, ready to hand to the web viewer or bake into an Artifact (#197). Its
token carries the config the run was played under, so a non-default batch's
replay reproduces rather than approximates.";

/// Which player drives the batch.
#[derive(Debug)]
enum Policy {
    /// Replay a fixed input list per run (design §12.4).
    Scripted(Vec<Input>),
    /// The baseline stealth bot (§13.2), playing one profile's temperament.
    Bot(Profile),
}

/// The whole usage text: the fixed prose above plus the vocabularies, which a
/// `const` cannot interpolate. Read off the catalog rather than hand-listed, so a
/// new modifier or a newly shipped ability is spellable *and* documented the day it
/// lands — the failure mode `the_usage_text_names_every_vocabulary` exists to catch,
/// fixed at the source rather than pinned by a test.
fn usage() -> String {
    format!(
        "{USAGE}\n\n\
         --intel-gate values: {}\n\
         --modifier names:    {}\n\
         --alert knobs:       {}\n\
         --abilities names:   {}",
        intel_gate_names(),
        modifier_names(),
        alert_knob_names(),
        ability_names(),
    )
}

/// The parsed flags, defaults filled in.
#[derive(Debug)]
struct Args {
    runs: u64,
    seed: u64,
    cap: u32,
    /// What every run of the batch boots from (§13.2/#256): the §10.2 recipe, the
    /// modifiers bending the run, and the loadout it holds.
    config: RunConfig,
    policy: Policy,
    /// Emit the single-seed captured replay (§12.4) instead of the metrics batch.
    emit_replay: bool,
}

/// The config flags, held as written and applied afterwards in a fixed order.
///
/// Order matters — `--abilities` states the whole tech set, so it must not be able to
/// undo a `--without` written before it — and argv order is the wrong thing to take
/// it from: two command lines that name the same flags should describe the same
/// batch. So the flags are collected here and [`resolve`](ConfigFlags::resolve)
/// applies them in the order the usage text promises.
#[derive(Debug, Default)]
struct ConfigFlags {
    /// A level-seed token to take the modifiers and loadout from (#245).
    preset: Option<LevelSeed>,
    guards: Option<usize>,
    intel_gate: Option<String>,
    /// Every `--modifier` written, in order — repeatable and comma-separated.
    modifiers: Vec<String>,
    /// Every `--alert` knob written, in order — repeatable and comma-separated, each
    /// a `name=value` (§7.3/#376). Later settings of the same knob win, which is what
    /// lets a sweep script append one without rewriting the command line.
    alert: Vec<String>,
    abilities: Option<String>,
    without: Option<String>,
}

impl ConfigFlags {
    /// The [`RunConfig`] these flags describe, applied to the sim preset in the fixed
    /// order the usage text promises.
    fn resolve(&self) -> Result<RunConfig, String> {
        let mut config = RunConfig::sim();
        if let Some(preset) = self.preset {
            config = config.with_preset(preset);
        }
        if let Some(guards) = self.guards {
            config = config.with_guards(guards);
        }
        if let Some(name) = &self.intel_gate {
            let gate = intel_gate_named(name).ok_or_else(|| {
                format!(
                    "--intel-gate: unknown gate {name}; known gates: {}",
                    intel_gate_names(),
                )
            })?;
            config = config.with_intel_gate(gate);
        }
        for list in &self.modifiers {
            for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                config = config
                    .with_modifier(name)
                    .map_err(|error| format!("--modifier: {error}"))?;
            }
        }
        for list in &self.alert {
            for setting in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                config = config
                    .with_alert(setting)
                    .map_err(|error| format!("--alert: {error}"))?;
            }
        }
        if let Some(list) = &self.abilities {
            config = config
                .with_tech(list)
                .map_err(|error| format!("--abilities: {error}"))?;
        }
        if let Some(list) = &self.without {
            config = config
                .without_tech(list)
                .map_err(|error| format!("--without: {error}"))?;
        }
        // Every flag is in: check the finished config describes a game the design
        // allows, once, at the point the alerted dwell range is whole (§7.3/§7.5).
        // Unprefixed, unlike the per-flag errors above — this checks the *config*,
        // and a range spelled across two flags has no one flag to blame.
        config.validated()
    }
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut runs = 100;
    let mut seed: Option<u64> = None;
    let mut cap = DEFAULT_INPUT_CAP;
    let mut flags = ConfigFlags::default();
    let mut script: Option<Vec<Input>> = None;
    let mut bot = false;
    let mut profile: Option<String> = None;
    let mut emit_replay = false;
    let mut it = argv.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .ok_or_else(|| format!("{flag} needs a value"))
                .cloned()
        };
        match flag.as_str() {
            "--runs" => runs = parse_number(&value()?, flag)?,
            "--seed" => seed = Some(parse_number(&value()?, flag)?),
            "--cap" => cap = parse_number::<u32>(&value()?, flag)?,
            "--config" => flags.preset = Some(parse_token(&value()?)?),
            "--guards" => flags.guards = Some(parse_number::<usize>(&value()?, flag)?),
            "--intel-gate" => flags.intel_gate = Some(value()?),
            "--modifier" => flags.modifiers.push(value()?),
            "--alert" => flags.alert.push(value()?),
            "--abilities" => flags.abilities = Some(value()?),
            "--without" => flags.without = Some(value()?),
            "--script" => script = Some(parse_script(&value()?)?),
            "--bot" => bot = true,
            "--profile" => profile = Some(value()?),
            "--emit-replay" => emit_replay = true,
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown flag {other}\n\n{}", usage())),
        }
    }
    let policy = match (bot, script) {
        (true, Some(_)) => return Err(format!("--bot and --script are exclusive\n\n{}", usage())),
        (true, None) => Policy::Bot(resolve_profile(profile.as_deref())?),
        (false, _) if profile.is_some() => {
            // A profile is the *bot's* temperament; silently ignoring it under a
            // script would emit rows attributed to a profile that never played.
            return Err(format!("--profile needs --bot\n\n{}", usage()));
        }
        (false, script) => Policy::Scripted(script.unwrap_or_default()),
    };
    Ok(Args {
        runs,
        // A token names a *run*, seed included, so it stands in as the first seed —
        // which is what makes `--config <token> --runs 1` replay the run it names.
        // An explicit `--seed` still wins: the token is then the preset alone.
        seed: seed.or(flags.preset.map(|preset| preset.seed)).unwrap_or(0),
        cap,
        config: flags.resolve()?,
        policy,
        emit_replay,
    })
}

/// Decode a `--config` level-seed token (#245/#333), or refuse it.
///
/// A malformed token is a **hard error**, never a silent fall to the default preset:
/// a batch whose rows claim a config it never ran is the §13.2 attribution failure,
/// and it is exactly what the web surface's graceful fall to a fresh run (#110) must
/// *not* be copied into here.
fn parse_token(token: &str) -> Result<LevelSeed, String> {
    LevelSeed::decode(token)
        .ok_or_else(|| format!("--config: not a level-seed token: {token}\n\n{}", usage()))
}

fn parse_number<T: std::str::FromStr>(text: &str, flag: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{flag}: not a number: {text}"))
}

/// Resolve `--profile NAME` to the temperament it names, defaulting to the
/// baseline when the flag is absent. An unknown name is refused with the whole
/// vocabulary rather than falling back: a batch whose rows claim a profile that
/// never ran is worse than a batch that did not start (§13.2 attribution).
fn resolve_profile(name: Option<&str>) -> Result<Profile, String> {
    match name {
        None => Ok(Profile::BASELINE),
        Some(name) => Profile::by_name(name).ok_or_else(|| {
            format!(
                "--profile: unknown profile {name}; known profiles: {}",
                Profile::names()
            )
        }),
    }
}

/// Capture and print the single-seed replay (§12.4): play seed S under the chosen
/// policy, print its `(seed, inputs)` pair on stdout, and a one-line human summary
/// on stderr so the pipe stays clean for a consumer (slice C's assemble.py).
fn emit_replay(args: &Args) -> ExitCode {
    let captured = match &args.policy {
        Policy::Scripted(script) => capture_one_with(
            &args.config,
            args.seed,
            Scripted::new(script.clone()),
            args.cap,
        ),
        Policy::Bot(profile) => capture_one_with(
            &args.config,
            args.seed,
            StealthBot::with_profile(*profile),
            args.cap,
        ),
    };
    let (record, replay) = match captured {
        Ok(captured) => captured,
        Err(error) => {
            eprintln!("seed {}: generation failed: {error:?}", args.seed);
            return ExitCode::FAILURE;
        }
    };
    println!("{}", replay.to_json_line());
    eprintln!(
        "seed {}: {} in {} turns, {} inputs",
        args.seed,
        record.outcome.as_str(),
        record.turns,
        replay.inputs.len()
    );
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&argv) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    if args.emit_replay {
        return emit_replay(&args);
    }

    let seeds = args.seed..args.seed.saturating_add(args.runs);
    let batch = match &args.policy {
        Policy::Scripted(script) => run_batch_with(&args.config, seeds, args.cap, |_| {
            Scripted::new(script.clone())
        }),
        Policy::Bot(profile) => run_batch_with(&args.config, seeds, args.cap, |_| {
            StealthBot::with_profile(*profile)
        }),
    };
    let records = match batch {
        Ok(records) => records,
        Err((seed, error)) => {
            eprintln!("seed {seed}: generation failed: {error:?}");
            return ExitCode::FAILURE;
        }
    };

    for record in &records {
        println!("{}", record.to_json_line());
    }
    println!("{}", Summary::of(&records).to_json_line());
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use intrusion_core::{AbilityId, AlertTuning, IntelGate, LevelModifiers, Loadout};

    fn args(argv: &[&str]) -> Result<Args, String> {
        parse_args(&argv.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    /// `--profile` selects the temperament, and defaults to the baseline so an
    /// existing `--bot` command line keeps meaning exactly what it did (#198).
    #[test]
    fn the_profile_flag_picks_a_temperament_and_defaults_to_baseline() {
        for name in Profile::ALL.map(|p| p.name) {
            let parsed = args(&["--bot", "--profile", name]).expect("a known profile parses");
            let Policy::Bot(profile) = parsed.policy else {
                panic!("--bot must yield the bot policy");
            };
            assert_eq!(profile.name, name);
        }
        let Ok(Args {
            policy: Policy::Bot(profile),
            ..
        }) = args(&["--bot"])
        else {
            panic!("a bare --bot must yield the bot policy");
        };
        assert_eq!(profile, Profile::BASELINE, "the default is today's bot");
    }

    /// An unknown profile is refused **with the vocabulary**, never run as the
    /// baseline: rows attributed to a temperament that never played would be worse
    /// than a batch that did not start (§13.2).
    #[test]
    fn an_unknown_profile_is_refused_with_the_known_names() {
        let error = args(&["--bot", "--profile", "reckless"]).expect_err("unknown name");
        assert!(error.contains("reckless"), "{error}");
        assert!(error.contains(&Profile::names()), "{error}");
    }

    /// The usage text spells the profile vocabulary out by hand (the `const` cannot
    /// interpolate), which is exactly how a new profile ships undocumented. This is
    /// the tie that makes that a failing test rather than a silence.
    ///
    /// The config vocabularies are not pinned that way: [`usage`] reads them off the
    /// catalog, so this asserts the *mechanism* rather than a list — a new modifier
    /// or a newly shipped ability documents itself.
    #[test]
    fn the_usage_text_names_every_vocabulary() {
        let help = usage();
        for name in Profile::ALL.map(|p| p.name) {
            assert!(
                help.contains(name),
                "--help does not mention the {name} profile",
            );
        }
        for name in [modifier_names(), ability_names(), intel_gate_names()] {
            assert!(help.contains(&name), "--help does not list {name}");
        }
        // Read off the catalog, so this holds for a row that ships tomorrow.
        assert!(
            help.contains("lockdown"),
            "a shipped ability is unspellable"
        );
    }

    /// The config flags reach the batch (#256): each states a departure from the sim
    /// preset, and a command line with none of them is that preset unchanged.
    #[test]
    fn the_config_flags_describe_the_batch() {
        assert_eq!(
            args(&["--bot"]).expect("a bare batch").config,
            RunConfig::sim(),
            "no config flags is the sim preset",
        );
        let parsed = args(&[
            "--intel-gate",
            "all",
            "--modifier",
            "full-layout-known,always-show-vision-cones",
            "--abilities",
            "camouflage,decoy",
            "--without",
            "decoy",
            "--guards",
            "7",
        ])
        .expect("known values");
        assert_eq!(parsed.config.modifiers.intel_to_exit, IntelGate::All);
        assert!(parsed.config.modifiers.full_layout_known);
        assert!(parsed.config.modifiers.always_show_vision_cones);
        assert!(parsed.config.abilities.contains(AbilityId::Camouflage));
        assert!(!parsed.config.abilities.contains(AbilityId::Decoy));
        assert!(parsed.config.abilities.contains(AbilityId::Run), "innate");
        assert_eq!(parsed.config.facility.guards, 7);
    }

    /// #376: `--alert` reaches the batch, is repeatable and comma-separated like
    /// `--modifier`, and leaves the ladder shipped when it is not written — so an
    /// existing command line keeps measuring the game it always measured.
    #[test]
    fn the_alert_flag_sets_the_ladders_thresholds() {
        assert_eq!(
            args(&["--bot"]).expect("a bare batch").config.alert,
            AlertTuning::default(),
            "no --alert is the shipped §7.3 ladder",
        );
        let parsed = args(&[
            "--alert",
            "sighting-window-turns=14,sighting-contact-turns=2",
            "--alert",
            "dwell-turns-max=5",
        ])
        .expect("known knobs");
        assert_eq!(parsed.config.alert.sighting_window_turns, 14);
        assert_eq!(parsed.config.alert.sighting_contact_turns, 2);
        assert_eq!(parsed.config.alert.dwell_turns_max, 5);
        assert_eq!(
            parsed.config.alert.sightings_for_second_rung,
            AlertTuning::default().sightings_for_second_rung,
            "an unnamed threshold is untouched",
        );

        // Later wins, so a sweep script can append a setting rather than rewrite one.
        let swept = args(&[
            "--alert",
            "sighting-window-turns=4",
            "--alert",
            "sighting-window-turns=20",
        ])
        .expect("known knobs");
        assert_eq!(swept.config.alert.sighting_window_turns, 20);
    }

    /// An unknown knob, an unreadable value, and a ladder §7.3/§7.5 forbids are all
    /// refused **before the batch runs** — with the vocabulary, or with the rule.
    /// Numbers from a game the design does not admit answer nothing (§13.2).
    #[test]
    fn a_bad_alert_knob_is_refused_with_its_vocabulary_or_its_rule() {
        let error = args(&["--alert", "window=4"]).expect_err("no such knob");
        assert!(error.contains("--alert"), "{error}");
        assert!(error.contains("sighting-window-turns"), "{error}");

        let error = args(&["--alert", "sighting-window-turns=soon"]).expect_err("not a number");
        assert!(error.contains("not a number"), "{error}");

        let error = args(&["--alert", "dwell-turns-min=0"]).expect_err("the §7.5 floor");
        assert!(error.contains("never removed"), "{error}");

        let error = args(&[
            "--alert",
            "sighting-window-turns=2,sighting-contact-turns=5",
        ])
        .expect_err("unreachable");
        assert!(error.contains("can never hold"), "{error}");
    }

    /// The flags compose in a **fixed order**, not argv order: two command lines that
    /// name the same flags describe the same batch, so `--without` cannot be undone
    /// by an `--abilities` that happened to be typed after it.
    #[test]
    fn the_config_flags_compose_in_a_fixed_order() {
        let written_one_way = args(&["--abilities", "camouflage,decoy", "--without", "decoy"]);
        let written_the_other = args(&["--without", "decoy", "--abilities", "camouflage,decoy"]);
        assert_eq!(
            written_one_way.expect("known values").config,
            written_the_other.expect("known values").config,
        );
    }

    /// `--config` is a whole run (#245): its modifiers and loadout are the batch's,
    /// and its seed stands in as the first seed — so `--config TOKEN --runs 1`
    /// replays the run the token names. An explicit `--seed` still wins.
    #[test]
    fn a_config_token_carries_the_preset_and_the_first_seed() {
        let named = LevelSeed {
            seed: 8371,
            modifiers: LevelModifiers {
                intel_to_exit: IntelGate::All,
                ..LevelModifiers::default()
            },
            abilities: Loadout::innate().with(AbilityId::Vision),
        };
        let token = named.encode().expect("a holdable config");
        let parsed = args(&["--config", &token]).expect("its own token");
        assert_eq!(parsed.seed, 8371, "the token names the first seed");
        assert_eq!(parsed.config.level(8371), named, "…and the whole run");

        let overridden = args(&["--config", &token, "--seed", "5"]).expect("its own token");
        assert_eq!(overridden.seed, 5, "an explicit --seed wins");
        assert_eq!(overridden.config, parsed.config, "the preset is unchanged");
    }

    /// A malformed `--config` token is a **hard error** with the usage, never a
    /// silent fall to the default preset: rows attributed to a config that never ran
    /// are worse than a batch that did not start (§12.4/§13.2). The web surface's
    /// graceful fall to a fresh run (#110) is deliberately not copied here.
    #[test]
    fn a_malformed_config_token_is_a_hard_error() {
        // A bare decimal seed (#333), a wrong length, a non-alphabetic character, and
        // nothing at all — the four shapes a mistyped token arrives in.
        for bad in ["8371", "prbjdokbxcqgjnrnc", "prbjdokbxcqgjnrnc9", ""] {
            let error = args(&["--config", bad]).expect_err("not a token");
            assert!(error.contains("not a level-seed token"), "{bad}: {error}");
            assert!(error.contains("Usage:"), "{bad}: no usage text");
        }
    }

    /// Every config flag refuses an unknown value **with its vocabulary**, and the
    /// §8.3 cap is refused at the flag rather than run.
    #[test]
    fn an_unknown_config_value_is_refused_with_its_vocabulary() {
        let error = args(&["--intel-gate", "some"]).expect_err("no such gate");
        assert!(error.contains("none, one, all"), "{error}");
        let error = args(&["--modifier", "deafen-the-guards"]).expect_err("no such modifier");
        assert!(error.contains("--modifier"), "{error}");
        assert!(error.contains("full-layout-known"), "{error}");
        let error = args(&["--abilities", "smoke-grenade"]).expect_err("no such ability");
        assert!(error.contains("--abilities"), "{error}");
        assert!(error.contains("camouflage"), "{error}");
        let error = args(&["--abilities", "camouflage,decoy,vision,dephase"])
            .expect_err("over the §8.3 cap");
        assert!(error.contains("at most 3"), "{error}");
        let error = args(&["--without", "run"]).expect_err("Run is innate");
        assert!(error.contains("innate"), "{error}");
    }

    /// A profile is the *bot's* temperament: pairing it with a script (or with no
    /// policy at all) is a mistake to report, not a flag to drop on the floor.
    #[test]
    fn a_profile_without_the_bot_is_an_error() {
        assert!(args(&["--profile", "cautious"])
            .expect_err("no --bot")
            .contains("--profile needs --bot"));
        assert!(args(&["--script", "NE", "--profile", "cautious"])
            .expect_err("scripted")
            .contains("--profile needs --bot"));
    }
}
