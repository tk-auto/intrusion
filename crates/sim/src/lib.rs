//! Headless simulation harness (§13.2): run *N* seeded games natively — no
//! browser, no canvas — with a player policy behind a trait, and emit
//! machine-readable metrics per run.
//!
//! This is the point of the whole rebuild (§13): the class of failure that
//! killed the old game (a free win button, deaf guards, an inert alert) is
//! invisible to a human playtester and obvious over 500 seeds. The harness
//! leans entirely on the pure, deterministic core (§12.1/§12.4): a run is
//! `(seed, [inputs])`, every metric is counted from the core's [`Event`]
//! stream, and the same `(seed, policy)` twice produces byte-identical rows.
//!
//! The harness reports **honest numbers, never verdicts** (§13.4): it is a
//! smoke detector, not a judge. The scripted policy ([`Scripted`]) replays a
//! fixed input list — all determinism testing needs — while the baseline stealth
//! bot ([`StealthBot`]) actually *plays* — putting each moment to every held
//! ability's [cue](crate::cue), so no verb can go dead by omission — which is what
//! turns the per-run metrics
//! (the §13.2 ability-usage histogram and the batch strategy-diversity score,
//! [`UsageHistogram`], [`diversity`]) from replay checksums into balance signals.
//! The bot plays one [`Profile`]'s temperament — the same policy at different
//! settings — so the same seeds can be raided cautiously and aggressively, which
//! is what makes §13.2's strategy-diversity signal visible at all.
//!
//! What a batch *boots* is an input too ([`RunConfig`], #256): the facility recipe,
//! the level modifiers and the ability loadout are a batch parameter rather than a
//! preset compiled into the harness, so a playtest can ask what one toggle changes
//! instead of only what the one shipped configuration does. The sim preset (§13.3 —
//! the baseline rules and a bare, innate-only loadout) is its [`Default`].
//!
//! The output schema is documented in `crates/sim/README.md` — the playtest
//! skill parses it, so changes there are breaking changes.
//!
//! [`Event`]: intrusion_core::Event

#![forbid(unsafe_code)]

mod bot;
mod config;
pub mod cue;
mod harness;
mod policy;
mod profile;
mod replay;
mod report;
#[cfg(test)]
mod test_support;
mod usage;

pub use bot::StealthBot;
pub use config::{ability_names, intel_gate_named, intel_gate_names, modifier_names, RunConfig};
pub use cue::{Bid, Intent, Moment};
pub use harness::{
    capture_one, capture_one_with, run_batch, run_batch_with, run_one, run_one_with, RunOutcome,
    RunRecord, DEFAULT_INPUT_CAP,
};
pub use policy::{PlayerPolicy, Recording, Scripted};
pub use profile::{Descent, Profile};
pub use replay::Replay;
pub use report::Summary;
pub use usage::{diversity, UsageHistogram, Verb};
