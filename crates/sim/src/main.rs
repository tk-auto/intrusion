//! The `sim` binary (§13.2): batch runner over the harness library.
//!
//! Parses a handful of flags, runs N seeded games under a scripted policy, and
//! prints one JSON line per run plus a final summary line to stdout — the
//! machine-readable stream the playtest skill consumes (schema:
//! `crates/sim/README.md`). Argument parsing is hand-rolled: a handful of flags
//! do not buy a dependency.

use std::process::ExitCode;

use intrusion_core::{parse_script, Input, LevelConfig};
use intrusion_sim::{
    capture_one_with, run_batch_with, Profile, Scripted, StealthBot, Summary, DEFAULT_INPUT_CAP,
};

const USAGE: &str = "\
Usage: sim [--runs N] [--seed S] [--cap N] [--guards N] [--bot [--profile NAME] | --script MOVES] [--emit-replay]

Run N seeded games headlessly and print JSON lines: one row per run, then a
summary row (schema: crates/sim/README.md).

  --runs N       how many runs; seeds are S, S+1, ... S+N-1   (default 100)
  --seed S       the first seed                               (default 0)
  --cap N        inputs issued per run before it is ruled a
                 timeout                                      (default 1000)
  --guards N     guards to place per facility — the §10.2
                 recipe knob the balance sweep drives         (default 4)
  --bot          play each run with the baseline stealth bot
                 instead of a script (design §13.2)           (default: off)
  --profile NAME the bot's playstyle temperament (§13.2):
                 baseline, cautious or aggressive; needs --bot (default: baseline)
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

--emit-replay captures the chosen policy's issued inputs for the single seed S
and prints the `(seed, inputs)` replay: with --bot, the exact run the bot
played, ready to hand to the web viewer or bake into an Artifact (#197).";

/// Which player drives the batch.
#[derive(Debug)]
enum Policy {
    /// Replay a fixed input list per run (design §12.4).
    Scripted(Vec<Input>),
    /// The baseline stealth bot (§13.2), playing one profile's temperament.
    Bot(Profile),
}

/// The parsed flags, defaults filled in.
#[derive(Debug)]
struct Args {
    runs: u64,
    seed: u64,
    cap: u32,
    /// The facility recipe the batch carves from (§10.2) — v1 with the guard count
    /// overridden by `--guards`, so the sweep varies guards and holds the rest.
    config: LevelConfig,
    policy: Policy,
    /// Emit the single-seed captured replay (§12.4) instead of the metrics batch.
    emit_replay: bool,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut runs = 100;
    let mut seed = 0;
    let mut cap = DEFAULT_INPUT_CAP;
    let mut guards = LevelConfig::V1.guards;
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
            "--seed" => seed = parse_number(&value()?, flag)?,
            "--cap" => cap = parse_number::<u32>(&value()?, flag)?,
            "--guards" => guards = parse_number::<usize>(&value()?, flag)?,
            "--script" => script = Some(parse_script(&value()?)?),
            "--bot" => bot = true,
            "--profile" => profile = Some(value()?),
            "--emit-replay" => emit_replay = true,
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown flag {other}\n\n{USAGE}")),
        }
    }
    let policy = match (bot, script) {
        (true, Some(_)) => return Err(format!("--bot and --script are exclusive\n\n{USAGE}")),
        (true, None) => Policy::Bot(resolve_profile(profile.as_deref())?),
        (false, _) if profile.is_some() => {
            // A profile is the *bot's* temperament; silently ignoring it under a
            // script would emit rows attributed to a profile that never played.
            return Err(format!("--profile needs --bot\n\n{USAGE}"));
        }
        (false, script) => Policy::Scripted(script.unwrap_or_default()),
    };
    Ok(Args {
        runs,
        seed,
        cap,
        config: LevelConfig {
            guards,
            ..LevelConfig::V1
        },
        policy,
        emit_replay,
    })
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
        assert!(error.contains("baseline, cautious, aggressive"), "{error}");
    }

    /// The usage text spells the vocabulary out by hand (it is a `const`, so it
    /// cannot interpolate), which is exactly how a new profile ships undocumented.
    /// This is the tie that makes that a failing test rather than a silence.
    #[test]
    fn the_usage_text_names_every_shipped_profile() {
        for name in Profile::ALL.map(|p| p.name) {
            assert!(
                USAGE.contains(name),
                "--help does not mention the {name} profile",
            );
        }
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
