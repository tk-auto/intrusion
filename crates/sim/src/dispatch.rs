//! Does a dispatch ever get there? (§7.3/#409)
//!
//! §7.3 sells a takedown as **a future appointment you cannot dodge**: the missed
//! ping sends a guard to the site, where it searches. The clock that bounds the
//! *investigation* was being spent on the *commute*, so the further from the patrols
//! you struck, the less the radio cost you — and the fix (a responder does not burn
//! its lead while it is still on its way) is only worth anything if dispatches were
//! actually evaporating. This is the counter that says so, before and after.
//!
//! It watches the one transition that is unambiguous from outside the core: a guard
//! that leaves [`GuardState::Responding`] for **Calm** gave its errand up, because
//! the only way out of the responding arm to Calm is the lead running cold (§7.6's
//! backstop). Leaving it for Alerted is the §7.6 search the call was for; leaving it
//! for the Danger band is the responder spotting the player en route, which is a
//! better outcome than either.
//!
//! Deliberately **not** a [`RunRecord`](crate::RunRecord) field: this measures a fix,
//! not the game's balance, and the §13.2 table (and the schema the playtest skill
//! parses) should not grow a row for every audit. It is a durable instrument rather
//! than a script, so the same numbers can be taken again the next time the radio's
//! timing moves.

use std::collections::HashMap;

use intrusion_core::{start_level_with, Cell, Event, GenError, GuardState, Outcome, State};

use crate::config::RunConfig;
use crate::policy::PlayerPolicy;

/// What became of one guard's errand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DispatchEnd {
    /// It got there and opened its §7.6 search — the errand as designed.
    Searched,
    /// Its lead ran cold on the road: it stood down having looked at nothing. **The
    /// failure #409 is about.**
    Expired,
    /// It found the player on the way and stopped being a responder — neither an
    /// arrival nor a failure, and not attributable to the lead either way.
    Diverted,
}

/// A tally of errands and how they ended, plus how far each responder was sent.
#[derive(Clone, Default, Debug)]
pub struct DispatchTally {
    /// One entry per errand that has finished, in the order they finished.
    pub ends: Vec<DispatchEnd>,
    /// The straight-line distance from each responder to the cell it was called to,
    /// for the errands whose site could be attributed (see [`DispatchWatch`]). Never
    /// longer than [`ends`](Self::ends), and not aligned with it.
    pub distances: Vec<u32>,
}

impl DispatchTally {
    /// How many errands finished.
    pub fn resolved(&self) -> usize {
        self.ends.len()
    }

    /// How many stood down without arriving (§7.3/#409).
    pub fn expired(&self) -> usize {
        self.count(DispatchEnd::Expired)
    }

    /// How many errands ended `end`.
    pub fn count(&self, end: DispatchEnd) -> usize {
        self.ends.iter().filter(|&&e| e == end).count()
    }

    /// The share of finished errands that evaporated on the road, in `0.0..=1.0`.
    /// Zero when nothing was ever dispatched, so an empty batch reads as "no failures
    /// seen" rather than dividing by nothing.
    pub fn expiry_rate(&self) -> f64 {
        if self.ends.is_empty() {
            return 0.0;
        }
        self.expired() as f64 / self.ends.len() as f64
    }

    /// Fold another run's tally in, so a batch is one tally.
    pub fn absorb(&mut self, other: &DispatchTally) {
        self.ends.extend_from_slice(&other.ends);
        self.distances.extend_from_slice(&other.distances);
    }
}

/// Watches a run's guards for errands starting and ending (§7.3/§7.7).
///
/// Call [`observe`](Self::observe) once per turn with the state **after** the step and
/// the events that step produced. Errands are matched to their site through the events
/// that announce one — a missed ping, a §7.7 call-in, a reinforcement — and only when
/// the turn named exactly one site, since two calls in a turn make the pairing a guess.
/// The count of errands is exact regardless; only the distance sample is skipped.
///
/// One known blind spot, stated rather than worked around: a responder **re-called to
/// a different cell while still responding** is one errand here, not two. It cannot
/// distort the expiry rate — the errand still ends the way it ends — and it is rare
/// enough that a correction would cost more clarity than it buys.
#[derive(Clone, Default, Debug)]
pub struct DispatchWatch {
    /// Where each currently-travelling guard was standing when its errand began,
    /// keyed by index into [`State::guards`] (which only ever grows, §7.3/#374).
    travelling: HashMap<usize, Cell>,
    tally: DispatchTally,
}

impl DispatchWatch {
    /// A watch that has seen nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one stepped turn in: `state` as it now stands, `events` as that step
    /// emitted them.
    pub fn observe(&mut self, state: &State, events: &[Event]) {
        let site = lone_call_site(events);
        for (index, guard) in state.guards().iter().enumerate() {
            let responding = guard.state() == GuardState::Responding;
            match (self.travelling.contains_key(&index), responding) {
                // A fresh errand: remember where it started, and price it if the
                // turn named exactly one place to go.
                (false, true) => {
                    self.travelling.insert(index, guard.pos());
                    if let Some(site) = site {
                        self.tally
                            .distances
                            .push(guard.pos().manhattan_distance(site));
                    }
                }
                // The errand is over — how it ended is written in the state it left for.
                (true, false) => {
                    self.travelling.remove(&index);
                    self.tally.ends.push(match guard.state() {
                        GuardState::Calm => DispatchEnd::Expired,
                        GuardState::Alerted => DispatchEnd::Searched,
                        _ => DispatchEnd::Diverted,
                    });
                }
                _ => {}
            }
        }
    }

    /// What has been seen so far. Errands still in flight when the run ends are
    /// simply not counted: an unfinished walk is neither an arrival nor a failure.
    pub fn tally(&self) -> &DispatchTally {
        &self.tally
    }
}

/// The one cell this turn's events called somebody to, or `None` when the turn named
/// none or more than one.
///
/// A **reinforcement** is deliberately absent: `ReinforcementArrived` carries where
/// the newcomer walked in, not the errand it was given, so counting it would price
/// every walk-in at zero. Its errand still counts as an errand — only the distance
/// sample skips it.
fn lone_call_site(events: &[Event]) -> Option<Cell> {
    let mut sites = events.iter().filter_map(|event| match *event {
        Event::RadioSilence { at } | Event::CalledIn { at } | Event::BodyCalledIn { at } => {
            Some(at)
        }
        _ => None,
    });
    let first = sites.next()?;
    sites.next().is_none().then_some(first)
}

/// Play one seeded run under `policy` and return what became of its errands
/// (§7.3/§7.7).
///
/// The [`run_one_with`](crate::run_one_with) loop with a [`DispatchWatch`] folded in:
/// the harness counts the §13.2 metrics from the event stream and has no reason to
/// carry this one, so the loop is repeated here rather than the record grown. It boots
/// through the same [`start_level_with`] path, so a watched run is the run the batch
/// plays, seed for seed.
pub fn watch_one(
    config: &RunConfig,
    seed: u64,
    policy: &mut dyn PlayerPolicy,
    input_cap: u32,
) -> Result<DispatchTally, GenError> {
    let mut state =
        start_level_with(&config.facility, &config.level(seed))?.with_alert_tuning(config.alert);
    let mut watch = DispatchWatch::new();
    for _ in 0..input_cap {
        let input = policy.decide(&state);
        let events = state.step(input);
        watch.observe(&state, &events);
        if state.outcome() != Outcome::Playing {
            break;
        }
    }
    Ok(watch.tally().clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::pinned_sweep;
    use crate::{Profile, StealthBot, DEFAULT_INPUT_CAP};

    /// The instrument itself: an errand that ends in a search reads `Searched`, one
    /// that stands down reads `Expired`, and the rate is the ratio between them.
    #[test]
    fn the_tally_counts_what_it_says_it_counts() {
        let tally = DispatchTally {
            ends: vec![
                DispatchEnd::Searched,
                DispatchEnd::Expired,
                DispatchEnd::Searched,
                DispatchEnd::Diverted,
            ],
            distances: vec![4, 40],
        };
        assert_eq!(tally.resolved(), 4);
        assert_eq!(tally.expired(), 1);
        assert_eq!(tally.count(DispatchEnd::Searched), 2);
        assert!((tally.expiry_rate() - 0.25).abs() < f64::EPSILON);

        let mut merged = DispatchTally::default();
        assert_eq!(
            merged.expiry_rate(),
            0.0,
            "nothing dispatched, nothing lost"
        );
        merged.absorb(&tally);
        merged.absorb(&tally);
        assert_eq!(merged.resolved(), 8);
        assert_eq!(merged.distances.len(), 4);
    }

    /// §7.3/#409 — **the gate the fix earns**. Over a seed sweep across all four
    /// §13.2 temperaments, a dispatch that *can* get there does. Before the fix a
    /// responder spent its lead on the commute and stood down on the road — 19% of
    /// finished errands over the wider sweep, and half of them on `cautious`; after
    /// it, the only errands that expire are the ones genuinely blocked or unreachable,
    /// which on a generated 40×40 board is a small tail rather than the common case.
    ///
    /// All four profiles, not just the striking ones: the errands a **§7.7 call-in**
    /// and a **reinforcement** start go through the same arm as a radio dispatch, so
    /// the avoidance-first temperaments contribute errands even though they leave no
    /// bodies — and `cautious` is where the failure was worst.
    ///
    /// The threshold is loose and direction-only (§13.4) — what is being pinned is
    /// the collapse from "a fifth of dispatches evaporate" to "none do", not a
    /// particular rate. A number is asserted at all so that re-coupling the lead to
    /// the journey shows up as a red test rather than as a quiet balance drift.
    #[test]
    fn a_dispatch_that_can_get_there_does() {
        // A **rate** cannot be witnessed by one seed (appendix 36), so this pins a
        // prefix instead: `0..15` across the four temperaments already resolves 15
        // errands, comfortably over the "enough to conclude anything" floor asserted
        // below, and `INTRUSION_SLOW_TESTS` restores the full sweep the 19%-to-none
        // collapse was measured over.
        let config = RunConfig::sim();
        let mut tally = DispatchTally::default();
        for profile in [
            Profile::BALANCED,
            Profile::CAUTIOUS,
            Profile::AGGRESSIVE,
            Profile::CARELESS,
        ] {
            for seed in pinned_sweep(0..15, 0..50) {
                let mut bot = StealthBot::with_profile(profile);
                let run = watch_one(&config, seed, &mut bot, DEFAULT_INPUT_CAP)
                    .expect("the sim config generates");
                tally.absorb(&run);
            }
        }

        assert!(
            tally.resolved() >= 10,
            "the sweep produced only {} finished errands — too few to conclude \
             anything, so the radio is no longer dispatching anybody (#260)",
            tally.resolved(),
        );
        assert!(
            tally.expiry_rate() < 0.10,
            "{} of {} dispatches stood down without arriving ({:.0}%): the lead is \
             being spent on the commute again (§7.3)",
            tally.expired(),
            tally.resolved(),
            tally.expiry_rate() * 100.0,
        );
    }
}
