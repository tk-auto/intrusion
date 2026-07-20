//! Shared test-only helpers.
//!
//! Home of the seed-sweep sampler (#60). The generation-driven property tests in
//! [`generate`](crate::generate), [`place`](crate::place), and [`door`](crate::door)
//! each sweep many seeds building full 40×40 facilities — corridor-first partition
//! plus the §10.6 reachability flood-fill — and those sweeps dominated `cargo test`
//! wall-clock, drifting from §12.1's "testable natively in milliseconds" goal. By
//! default a sweep runs a small spread of seeds so the routine gate stays fast; CI
//! sets `INTRUSION_SLOW_TESTS=1` to run every seed and preserve the full coverage —
//! the seeds are not dropped, just deferred off the every-`cargo test` path.

/// The default sampled sweep width — small enough to keep the routine gate fast,
/// wide enough to spread across each sweep's range.
pub(crate) const SAMPLE_SEEDS: u64 = 12;

/// Whether to sweep every seed instead of the [`SAMPLE_SEEDS`] sample. CI sets
/// `INTRUSION_SLOW_TESTS=1` so the exhaustive sweep still runs on every push.
pub(crate) fn exhaustive_seeds() -> bool {
    std::env::var_os("INTRUSION_SLOW_TESTS").is_some()
}

/// The seeds a property test sweeps whose exhaustive range is `0..full`.
///
/// Full range under `INTRUSION_SLOW_TESTS`; otherwise a spread of at most
/// [`SAMPLE_SEEDS`] seeds sampled across the whole range, so low *and* high seeds
/// are still exercised. A sampled failure still prints its seed, and the exhaustive
/// CI run (or `INTRUSION_SLOW_TESTS=1` locally) reproduces it.
pub(crate) fn seed_sweep(full: u64) -> Vec<u64> {
    if exhaustive_seeds() || full <= SAMPLE_SEEDS {
        (0..full).collect()
    } else {
        (0..SAMPLE_SEEDS).map(|i| i * full / SAMPLE_SEEDS).collect()
    }
}
