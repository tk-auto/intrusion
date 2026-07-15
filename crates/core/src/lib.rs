//! Pure, deterministic game logic for Intrusion.
//!
//! This crate is the load-bearing half of the architecture (§12.1): it knows
//! nothing about rendering, input, the DOM, the clock, or the platform. Its
//! whole contract is `state × input → state, events`, and it must be testable
//! natively in milliseconds with no browser.
//!
//! For now it holds only the seeded PRNG wrapper (§12.4) — the one primitive
//! every other system will build on. Game systems (generation, guards, vision,
//! …) land in their own tickets.

#![forbid(unsafe_code)]

mod rng;

pub use rng::Rng;
