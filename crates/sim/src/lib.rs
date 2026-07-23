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
//! fixed input list; the per-run metrics include the §13.2 ability-usage
//! histogram and the batch strategy-diversity score ([`UsageHistogram`],
//! [`diversity`]). The bot policy is its own ticket.
//!
//! The output schema is documented in `crates/sim/README.md` — the playtest
//! skill parses it, so changes there are breaking changes.
//!
//! [`Event`]: intrusion_core::Event

#![forbid(unsafe_code)]

mod harness;
mod policy;
mod report;
mod usage;

pub use harness::{run_batch, run_one, RunOutcome, RunRecord, DEFAULT_INPUT_CAP};
pub use policy::{PlayerPolicy, Scripted};
pub use report::Summary;
pub use usage::{diversity, UsageHistogram, Verb};
