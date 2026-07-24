//! The `sim` binary (§13.2): batch runner over the harness library.
//!
//! Parses a handful of flags, runs N seeded games under a scripted policy, and
//! prints one JSON line per run plus a final summary line to stdout — the
//! machine-readable stream the playtest skill consumes (schema:
//! `crates/sim/README.md`). Argument parsing is hand-rolled: a handful of flags
//! do not buy a dependency.

use std::process::ExitCode;

use intrusion_core::{parse_script, Input};
use intrusion_sim::{capture_one, run_batch, Scripted, StealthBot, Summary, DEFAULT_INPUT_CAP};

const USAGE: &str = "\
Usage: sim [--runs N] [--seed S] [--cap N] [--bot | --script MOVES] [--emit-replay]

Run N seeded games headlessly and print JSON lines: one row per run, then a
summary row (schema: crates/sim/README.md).

  --runs N       how many runs; seeds are S, S+1, ... S+N-1   (default 100)
  --seed S       the first seed                               (default 0)
  --cap N        inputs issued per run before it is ruled a
                 timeout                                      (default 1000)
  --bot          play each run with the baseline stealth bot
                 instead of a script (design §13.2)           (default: off)
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

--emit-replay captures the chosen policy's issued inputs for the single seed S
and prints the `(seed, inputs)` replay: with --bot, the exact run the bot
played, ready to hand to the web viewer or bake into an Artifact (#197).";

/// Which player drives the batch.
enum Policy {
    /// Replay a fixed input list per run (design §12.4).
    Scripted(Vec<Input>),
    /// The baseline stealth bot (§13.2).
    Bot,
}

/// The parsed flags, defaults filled in.
struct Args {
    runs: u64,
    seed: u64,
    cap: u32,
    policy: Policy,
    /// Emit the single-seed captured replay (§12.4) instead of the metrics batch.
    emit_replay: bool,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut runs = 100;
    let mut seed = 0;
    let mut cap = DEFAULT_INPUT_CAP;
    let mut script: Option<Vec<Input>> = None;
    let mut bot = false;
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
            "--script" => script = Some(parse_script(&value()?)?),
            "--bot" => bot = true,
            "--emit-replay" => emit_replay = true,
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown flag {other}\n\n{USAGE}")),
        }
    }
    let policy = match (bot, script) {
        (true, Some(_)) => return Err(format!("--bot and --script are exclusive\n\n{USAGE}")),
        (true, None) => Policy::Bot,
        (false, script) => Policy::Scripted(script.unwrap_or_default()),
    };
    Ok(Args {
        runs,
        seed,
        cap,
        policy,
        emit_replay,
    })
}

fn parse_number<T: std::str::FromStr>(text: &str, flag: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{flag}: not a number: {text}"))
}

/// Capture and print the single-seed replay (§12.4): play seed S under the chosen
/// policy, print its `(seed, inputs)` pair on stdout, and a one-line human summary
/// on stderr so the pipe stays clean for a consumer (slice C's assemble.py).
fn emit_replay(args: &Args) -> ExitCode {
    let captured = match &args.policy {
        Policy::Scripted(script) => capture_one(args.seed, Scripted::new(script.clone()), args.cap),
        Policy::Bot => capture_one(args.seed, StealthBot::new(), args.cap),
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
        Policy::Scripted(script) => run_batch(seeds, args.cap, |_| Scripted::new(script.clone())),
        Policy::Bot => run_batch(seeds, args.cap, |_| StealthBot::new()),
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
