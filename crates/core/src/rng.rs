//! The seeded random source for the whole run (§12.4).
//!
//! Determinism here is not a nice-to-have — it is what makes seed sharing, bug
//! repro, golden tests, and the headless sim (§13) possible at all. Two rules
//! keep it honest:
//!
//! 1. **One seed per run.** Everything random derives from it. Never construct a
//!    fresh source mid-run — thread this one through.
//! 2. **The algorithm is pinned.** We wrap `rand_pcg`'s PCG generator behind our
//!    own type and pin the dependency to an exact version, so the byte stream a
//!    seed produces can never drift between releases. (Rust's default `rand`
//!    generator explicitly does *not* guarantee this across versions.)

use rand_core::{RngCore, SeedableRng};
use rand_pcg::Pcg64Mcg;

/// The run's deterministic random source.
///
/// Clone it only when you deliberately want two identical streams (e.g. a
/// speculative lookahead); otherwise pass `&mut Rng` so a single stream advances
/// in a well-defined order.
#[derive(Clone)]
pub struct Rng {
    inner: Pcg64Mcg,
}

impl Rng {
    /// Create the run's random source from its seed. Same seed → same stream,
    /// forever, on every platform.
    pub fn new(seed: u64) -> Self {
        // `seed_from_u64` expands the 64-bit seed into PCG's larger state with a
        // fixed SplitMix64 pass defined by `rand_core`, which we pin exactly.
        Self {
            inner: Pcg64Mcg::seed_from_u64(seed),
        }
    }

    /// Next raw 32-bit value.
    pub fn next_u32(&mut self) -> u32 {
        self.inner.next_u32()
    }

    /// Next raw 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.inner.next_u64()
    }

    /// A uniformly distributed value in `0..bound`, without modulo bias.
    ///
    /// Uses Lemire's multiply-shift rejection method. Panics if `bound == 0`.
    pub fn below(&mut self, bound: u32) -> u32 {
        assert!(bound != 0, "Rng::below requires a non-zero bound");
        // Lemire: multiply into the high word and reject the small biased tail.
        let mut x = self.next_u32();
        let mut m = (x as u64) * (bound as u64);
        let mut low = m as u32;
        if low < bound {
            // `bound.wrapping_neg() % bound` is the rejection threshold.
            let threshold = bound.wrapping_neg() % bound;
            while low < threshold {
                x = self.next_u32();
                m = (x as u64) * (bound as u64);
                low = m as u32;
            }
        }
        (m >> 32) as u32
    }

    /// A uniformly distributed integer in the inclusive range `[lo, hi]`.
    /// Panics if `lo > hi`.
    pub fn range_inclusive(&mut self, lo: i32, hi: i32) -> i32 {
        assert!(lo <= hi, "Rng::range_inclusive requires lo <= hi");
        let span = (hi as i64 - lo as i64 + 1) as u32;
        lo + self.below(span) as i32
    }

    /// A fair coin flip.
    pub fn bool(&mut self) -> bool {
        self.next_u32() & 1 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The single property this whole crate is organised around: same seed →
    /// same stream. If this ever fails, seed sharing and every golden test built
    /// on it are void.
    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::new(8371);
        let mut b = Rng::new(8371);
        for _ in 0..1_000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        // Not a guarantee for a single draw, but over a run the streams differ.
        let a_seq: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let b_seq: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        assert_ne!(a_seq, b_seq);
    }

    #[test]
    fn below_stays_in_range() {
        let mut r = Rng::new(42);
        for _ in 0..10_000 {
            let v = r.below(7);
            assert!(v < 7);
        }
    }

    #[test]
    fn below_one_is_always_zero() {
        let mut r = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(r.below(1), 0);
        }
    }

    #[test]
    fn range_inclusive_covers_endpoints() {
        let mut r = Rng::new(99);
        let (lo, hi) = (-3, 4);
        let mut saw_lo = false;
        let mut saw_hi = false;
        for _ in 0..10_000 {
            let v = r.range_inclusive(lo, hi);
            assert!(v >= lo && v <= hi);
            saw_lo |= v == lo;
            saw_hi |= v == hi;
        }
        assert!(saw_lo && saw_hi, "range should reach both endpoints");
    }

    /// A tiny golden vector. If the pinned algorithm ever changes underneath us,
    /// these fixed numbers move and this test screams — which is the point.
    #[test]
    fn golden_stream_is_stable() {
        let mut r = Rng::new(0xC0FFEE);
        let got: Vec<u64> = (0..4).map(|_| r.next_u64()).collect();
        assert_eq!(
            got,
            vec![
                15_289_023_248_299_748_866,
                9_647_075_671_823_935_953,
                9_901_670_836_156_572_500,
                18_118_834_324_253_044_937,
            ]
        );
    }
}
