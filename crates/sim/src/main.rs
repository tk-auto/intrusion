//! The `sim` binary (§13.2): batch runner over the harness library.
//!
//! Parses a handful of flags, runs N seeded games under a scripted policy, and
//! prints one JSON line per run plus a final summary line to stdout — the
//! machine-readable stream the playtest skill consumes (schema:
//! `crates/sim/README.md`). Argument parsing is hand-rolled: four flags do not
//! buy a dependency.

use std::process::ExitCode;

use intrusion_core::{Direction, Input};
use intrusion_sim::{run_batch, Scripted, Summary, DEFAULT_INPUT_CAP};

const USAGE: &str = "\
Usage: sim [--runs N] [--seed S] [--cap N] [--script MOVES]

Run N seeded games headlessly and print JSON lines: one row per run, then a
summary row (schema: crates/sim/README.md).

  --runs N       how many runs; seeds are S, S+1, ... S+N-1   (default 100)
  --seed S       the first seed                               (default 0)
  --cap N        inputs issued per run before it is ruled a
                 timeout                                      (default 1000)
  --script MOVES inputs replayed from the start of every run:
                 N/E/S/W step, `.` waits; after the script the
                 player waits out the run                     (default: empty)

The empty default script is the idle baseline: how often patrols stumble onto
a player who never moves. A per-seed script is a replay (design §12.4) — with
--runs 1 it reproduces a single run exactly.";

/// The parsed flags, defaults filled in.
struct Args {
    runs: u64,
    seed: u64,
    cap: u32,
    script: Vec<Input>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut args = Args {
        runs: 100,
        seed: 0,
        cap: DEFAULT_INPUT_CAP,
        script: Vec::new(),
    };
    let mut it = argv.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .ok_or_else(|| format!("{flag} needs a value"))
                .cloned()
        };
        match flag.as_str() {
            "--runs" => args.runs = parse_number(&value()?, flag)?,
            "--seed" => args.seed = parse_number(&value()?, flag)?,
            "--cap" => args.cap = parse_number::<u32>(&value()?, flag)?,
            "--script" => args.script = parse_script(&value()?)?,
            "--help" | "-h" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown flag {other}\n\n{USAGE}")),
        }
    }
    Ok(args)
}

fn parse_number<T: std::str::FromStr>(text: &str, flag: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{flag}: not a number: {text}"))
}

/// The script notation: one input per character. `N`/`E`/`S`/`W` step (case
/// folded), `.` waits. Abilities are not spelt here yet — the [`Scripted`]
/// policy takes any [`Input`] in code; the notation grows when a batch needs
/// them.
fn parse_script(text: &str) -> Result<Vec<Input>, String> {
    text.chars()
        .map(|c| match c.to_ascii_uppercase() {
            'N' => Ok(Input::Step(Direction::North)),
            'E' => Ok(Input::Step(Direction::East)),
            'S' => Ok(Input::Step(Direction::South)),
            'W' => Ok(Input::Step(Direction::West)),
            '.' => Ok(Input::Wait),
            other => Err(format!("--script: unknown move {other:?} (want N/E/S/W/.)")),
        })
        .collect()
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

    let seeds = args.seed..args.seed.saturating_add(args.runs);
    let records = match run_batch(seeds, args.cap, |_| Scripted::new(args.script.clone())) {
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
