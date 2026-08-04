//! Shared test scaffolding for the sim's own unit tests — never compiled into the
//! binary (`#[cfg(test)]`).
//!
//! Three helpers, all of them about the same thing: the sim's tests are the only
//! ones in the workspace that pay for **whole runs**, and a run is expensive —
//! generating a 40×40 facility costs ~30 ms and walking one to its ending costs
//! ~100 ms more. Left alone, a suite of run-walking tests grows into minutes.
//!
//! - [`boot`] memoises generation, so the same seed is carved once per test binary
//!   and handed out as a clone (~25 µs) thereafter.
//! - [`witness_sweep`] is the seed list an **existence** test walks: the pinned
//!   witness alone by default, the whole range under `INTRUSION_SLOW_TESTS` — the
//!   #60 pattern, applied to the sim's own shape of sweep (appendix 36).
//! - [`profile_batch`] is the one canonical batch per temperament, memoised, so the
//!   several tests that read a batch share a single walk instead of each buying
//!   its own.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Mutex, OnceLock};

use intrusion_core::{generate_level, Direction, LevelConfig, Placement, Rng, State};

use crate::{run_batch, Profile, RunRecord, StealthBot, DEFAULT_INPUT_CAP};

/// Booted levels, keyed by seed. Generation is deterministic (§12.4), so the level a
/// seed carves cannot differ between two calls — which is exactly what makes handing
/// out a clone of the first one honest rather than a shortcut.
fn booted() -> &'static Mutex<HashMap<u64, (State, Placement)>> {
    static BOOTED: OnceLock<Mutex<HashMap<u64, (State, Placement)>>> = OnceLock::new();
    BOOTED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Boot a real V1 level exactly as the harness does (§13.2), returning the state
/// and the placement — so a test can compare against the ground truth the bot must
/// *not* peek at.
///
/// **Memoised per test binary.** Carving a facility costs a thousand times what
/// cloning the booted state does, and the sim's tests boot the same low seeds over
/// and over; the cache is what stops the suite paying generation's price once per
/// test rather than once per seed. Each caller still gets its own `State` to step,
/// so runs stay independent.
pub fn boot(seed: u64) -> (State, Placement) {
    let mut cache = booted().lock().expect("no test panics holding the cache");
    cache
        .entry(seed)
        .or_insert_with(|| {
            let (layout, placement) = generate_level(
                &LevelConfig::V1,
                &intrusion_core::LevelModifiers::default(),
                &mut Rng::new(seed),
            )
            .expect("V1 generates");
            let guards = placement.guards(&layout);
            let state = State::new(
                layout,
                placement.player(),
                Direction::North,
                guards,
                placement.intel().iter().copied(),
                placement.exit(),
            );
            (state, placement)
        })
        .clone()
}

/// Whether to sweep every seed instead of the pinned witness. CI sets
/// `INTRUSION_SLOW_TESTS=1` so the full sweep still runs on every push — the same
/// switch, and the same bargain, as core's own seed sampler (#60).
pub fn exhaustive_seeds() -> bool {
    std::env::var_os("INTRUSION_SLOW_TESTS").is_some()
}

/// The seeds an **existence-shaped** sweep walks: a rule asserted on every turn of
/// every run, plus a `> 0` guard proving the sweep met the thing at all.
///
/// Locally that is the pinned `witness` alone — one run, which both exercises the
/// rule and satisfies the guard. Under `INTRUSION_SLOW_TESTS` it is the whole
/// `full` range, so CI keeps the universal half at its original width.
///
/// **A witness is pinned, never searched for** (appendix 36). Searching a range for
/// a seed that happens to exhibit a rare verb costs hundreds of runs on every gate
/// run to re-derive a fact that does not change between commits, and a range that
/// quietly empties reads as a policy regression (#387). So the seed is written into
/// the test. When generation moves and the witness stops exhibiting the thing, the
/// test fails naming its own remedy: sweep with `INTRUSION_SLOW_TESTS=1`, take a
/// seed that still does, and pin that one — the cost falls on the change that moved
/// it rather than on every run of the gate.
pub fn witness_sweep(witness: u64, full: Range<u64>) -> Vec<u64> {
    if exhaustive_seeds() {
        full.collect()
    } else {
        vec![witness]
    }
}

/// The seeds a **negative** sweep walks: "this temperament never does X".
///
/// A negative has no witness to pin — that is precisely what makes it a negative —
/// so it keeps a sweep, and the bargain is core's instead (#60): a small spread of
/// seeds locally, the whole range under `INTRUSION_SLOW_TESTS`. A sampled failure
/// still prints its seed, and the CI run reproduces it.
pub fn negative_sweep(full: Range<u64>) -> Vec<u64> {
    const SAMPLE: u64 = 8;
    let width = full.end - full.start;
    if exhaustive_seeds() || width <= SAMPLE {
        full.collect()
    } else {
        (0..SAMPLE)
            .map(|i| full.start + i * width / SAMPLE)
            .collect()
    }
}

/// The seeds a **statistical** sweep walks: a rate or a ratio over many runs, which no
/// single witness can stand for.
///
/// `pinned` is the narrow prefix that already clears the test's own
/// "enough-to-conclude-anything" guard — the same idea as a witness, sized to a claim
/// that needs a population rather than an instance. `full` is what CI sweeps.
///
/// It stays honest for the same reason a witness does: the guard is asserted, so a
/// prefix that stops producing enough runs to conclude from fails rather than quietly
/// concluding from three.
pub fn pinned_sweep(pinned: Range<u64>, full: Range<u64>) -> Vec<u64> {
    if exhaustive_seeds() {
        full.collect()
    } else {
        pinned.collect()
    }
}

/// The message an existence test fails with when its pinned witness stops being one.
///
/// Spelled once, here, because the remedy is the same every time and it is the half
/// a reader needs: a red line that only says "no decoy was dropped" invites the next
/// person to widen the sweep, which is the thing this whole seam exists to stop.
pub fn stale_witness(what: &str, witness: u64) -> String {
    format!(
        "seed {witness} no longer {what} — if this is deliberate, re-run with \
         INTRUSION_SLOW_TESTS=1 to sweep for a seed that still does and pin that one \
         as the witness; if it is not, the policy has regressed",
    )
}

/// The canonical seed range every shared batch is cut from. Wide enough for the
/// outcome-shape assertions that read a batch, and **one** range so those tests can
/// share a single walk.
pub const BATCH_SEEDS: Range<u64> = 0..60;

/// Batches already walked, keyed by temperament.
fn batches() -> &'static Mutex<HashMap<&'static str, Vec<RunRecord>>> {
    static BATCHES: OnceLock<Mutex<HashMap<&'static str, Vec<RunRecord>>>> = OnceLock::new();
    BATCHES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// One batch of [`BATCH_SEEDS`] under `profile`, walked once per test binary.
///
/// The several tests that assert on a batch's *shape* — that a temperament finishes
/// its runs, that the striking ones work the body chain, that two temperaments do
/// not play alike — were each walking their own 40–60 runs of the same four
/// profiles, which is the single largest cost in this suite and buys nothing: the
/// records are identical, only the field being read differs. This walks them once
/// and hands out clones.
///
/// Keyed by `profile.name` because that is the temperament's identity in every row
/// the harness emits (§13.2); two profiles sharing a name would be one profile.
pub fn profile_batch(profile: Profile) -> Vec<RunRecord> {
    let mut cache = batches().lock().expect("no test panics holding the cache");
    cache
        .entry(profile.name)
        .or_insert_with(|| {
            run_batch(BATCH_SEEDS, DEFAULT_INPUT_CAP, move |_| {
                StealthBot::with_profile(profile)
            })
            .expect("the sim preset generates")
        })
        .clone()
}
