//! The radio net (§7.3) — how a permanent takedown stays costly.
//!
//! Takedowns are permanent and free of cooldown (§7.2); the body is the cost.
//! The radio is what keeps that cost from being paid once: **control pings each
//! guard periodically, and a guard that is down does not answer.** A missed ping
//! dispatches the nearest still-active guard to the silent guard's last known
//! post ([`Body::post`](crate::body::Body::post)); a second missed ping steps a
//! facility-wide alert. So every takedown starts a clock — a future appointment —
//! and three takedowns is three clocks running at once, which is why a full clear
//! collapses under its own weight with no rule needed to ban it (§7.3).
//!
//! This module owns the *timing and selection* — the per-guard cadence and the
//! pure "who responds" query; the [`State`](crate::State) turn loop owns the
//! orchestration (which body is silent this turn, mutating the responder, stepping
//! the alert), because that reaches across guards, bodies and the alert together.
//! The tell is deliberately **visual** (§7.3/§9.3, sound is gone): the silence is a
//! near-line message and the responder is the player's own sensed dot peeling off
//! toward the post — no ping the player has to hear.

use crate::category::Category;
use crate::cell::Cell;
use crate::guard::Guard;
use crate::rng::Rng;

/// The nominal radio ping interval (§7.3, **[START] = 20**): control pings each
/// downed guard's post roughly every this-many turns. The per-guard cadence
/// jitters ±[`PING_JITTER`] around it so guards do not answer in lockstep and no
/// single global metronome is countable — the jitter is drawn once per guard from
/// the run seed ([`RadioClock::draw`]), never the wall clock (§12.4 forbids
/// `Date`/per-call randomness).
pub(crate) const PING_INTERVAL: u32 = 20;

/// How far a guard's ping cadence may stray from [`PING_INTERVAL`] (§7.3,
/// **[START] = 3**): a period is drawn from `PING_INTERVAL ± PING_JITTER`
/// (17..=23), so the clock a takedown starts is ~20 turns but not exactly, and it
/// differs between guards. This is the "jittered" in "every ~20 turns per guard,
/// jittered". Small on purpose: the window a takedown buys must stay a real,
/// roughly-known appointment (§7.3 "roughly when it fires"), not a coin flip.
pub(crate) const PING_JITTER: u32 = 3;
// The jittered period must stay positive whatever these [START]s are retuned to —
// held at compile time, like the §7.2 body-vs-sighting alert relation (guard.rs).
const _: () = assert!(PING_JITTER < PING_INTERVAL);

/// How many pings a downed guard misses before control stops calling (§7.3): the
/// **first** miss dispatches a responder, the **second** steps the facility alert,
/// and after that the guard is presumed gone — control has escalated as far as the
/// design specifies, so it stops pinging a corpse forever. This is the cap both the
/// dispatch and the alert step count against.
pub(crate) const MAX_MISSED_PINGS: u8 = 2;

/// How much a second missed ping raises the facility alert (§7.3, **[START] = 1**):
/// the escalation the alert system was always meant to provide, from a concrete,
/// explainable source (a guard stopped answering) rather than a global number
/// (§2.3 — "Alert: never written to, never read" was the old failure). The alert
/// this steps is a real value, written here and read by the near line (§11.4).
pub(crate) const ALERT_STEP: u32 = 1;

/// A guard's radio ping cadence (§7.3): the period of its personal clock, drawn
/// once from the run seed so the whole schedule is deterministic (§12.4). Carried
/// by the guard and handed to the [`Body`](crate::body::Body) it becomes at a
/// takedown, where the clock finally has an effect — a live guard always answers,
/// so its cadence is unobservable until it is down.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RadioClock {
    period: u32,
}

impl RadioClock {
    /// The default cadence — exactly [`PING_INTERVAL`], no jitter. Fixture guards
    /// (hand-placed, [`Guard::stationary`](crate::Guard) and friends) get this;
    /// generated guards get a seed-jittered one from [`draw`](Self::draw).
    pub(crate) const DEFAULT: Self = Self {
        period: PING_INTERVAL,
    };

    /// Draw a jittered cadence from the run's seeded source (§12.4): a period in
    /// `PING_INTERVAL ± PING_JITTER`. Placement calls this once per guard from the
    /// same stream that carved the level, so the same seed always yields the same
    /// radio schedule — never a fresh source, never the clock (§7.3 note).
    pub(crate) fn draw(rng: &mut Rng) -> Self {
        let lo = (PING_INTERVAL - PING_JITTER) as i32;
        let hi = (PING_INTERVAL + PING_JITTER) as i32;
        Self {
            period: rng.range_inclusive(lo, hi) as u32,
        }
    }

    /// A cadence with an exact period — the seam tests and fixtures use to pin a
    /// short, known clock without going through the seeded [`draw`](Self::draw).
    #[cfg(test)]
    pub(crate) fn from_period(period: u32) -> Self {
        Self { period }
    }

    /// The cadence period in turns — the gap between a downed guard's successive
    /// missed pings, and the window a takedown buys before the first one.
    pub(crate) fn period(self) -> u32 {
        self.period
    }
}

impl Default for RadioClock {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The index of the guard control would dispatch to a silent `post` (§7.3): the
/// **nearest active** guard, by Manhattan distance to the post, ties broken by
/// spawn order so the choice is deterministic (§12.4). "Active" here means a guard
/// not already locked onto the live player — its state is not [`Category::Danger`]
/// (Chasing/Investigating), because a guard that has *you* does not break off to
/// walk toward a cold radio silence (§7.4). A guard already Calm, searching, or
/// responding is fair game. `None` when every guard has the player — nobody is
/// free to send, and the silence simply goes un-investigated this ping (the second
/// miss still steps the alert).
///
/// Distance is Manhattan **[START]**, not path length: cheap and deterministic,
/// and the dispatched guard routes there properly regardless — refining the
/// *choice* to true path distance is a later tune, pinned by no rule today.
pub(crate) fn nearest_respondable(guards: &[Guard], post: Cell) -> Option<usize> {
    guards
        .iter()
        .enumerate()
        .filter(|(_, g)| g.state().category() != Category::Danger)
        .min_by_key(|(i, g)| (g.pos().manhattan_distance(post), *i))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::{Guard, GuardState};

    /// The §7.3 timing knobs are **[START]** values a later tune must move
    /// deliberately — pinned here so the edit is visible. The jitter must keep the
    /// period positive, and the miss cap is exactly two (dispatch, then alert).
    #[test]
    fn the_radio_constants_are_pinned() {
        assert_eq!(PING_INTERVAL, 20, "the [START] ping interval");
        assert_eq!(PING_JITTER, 3, "the [START] ping jitter");
        assert_eq!(ALERT_STEP, 1, "the [START] alert step");
        assert_eq!(
            MAX_MISSED_PINGS, 2,
            "dispatch on the first miss, alert on the second"
        );
        // (That the jittered period stays positive is a compile-time assert above.)
    }

    /// §7.3/§12.4: a drawn cadence stays in `PING_INTERVAL ± PING_JITTER`, and the
    /// same seed draws the same period — the schedule is deterministic.
    #[test]
    fn a_drawn_cadence_is_bounded_and_deterministic() {
        let mut rng = Rng::new(2026);
        for _ in 0..1_000 {
            let p = RadioClock::draw(&mut rng).period();
            assert!(
                (PING_INTERVAL - PING_JITTER..=PING_INTERVAL + PING_JITTER).contains(&p),
                "period {p} out of the jitter window",
            );
        }
        let a: Vec<u32> = (0..8)
            .scan(Rng::new(7), |r, _| Some(RadioClock::draw(r).period()))
            .collect();
        let b: Vec<u32> = (0..8)
            .scan(Rng::new(7), |r, _| Some(RadioClock::draw(r).period()))
            .collect();
        assert_eq!(a, b, "same seed → same schedule (§12.4)");
    }

    /// §7.3: control dispatches the **nearest** active guard to the silent post,
    /// ties broken by spawn order — deterministic (§12.4).
    #[test]
    fn dispatch_picks_the_nearest_active_guard() {
        let post = Cell::new(10, 10);
        let guards = vec![
            Guard::stationary(Cell::new(10, 2)), // 8 away
            Guard::stationary(Cell::new(10, 6)), // 4 away — nearest
            Guard::stationary(Cell::new(2, 10)), // 8 away
        ];
        assert_eq!(nearest_respondable(&guards, post), Some(1));
    }

    /// §7.4: a guard that has the live player (Chasing/Investigating — the Danger
    /// band) is **not** pulled off to answer a cold radio silence; a Calm, Alerted
    /// or Responding guard is fair game. With every guard on the player, nobody is
    /// free and control sends no one.
    #[test]
    fn a_guard_on_the_player_is_never_dispatched() {
        let post = Cell::new(1, 1);
        // The nearest guard is Chasing — skip it for the farther Calm one.
        let guards = vec![
            Guard::stationary(Cell::new(2, 1)).with_state(GuardState::Chasing),
            Guard::stationary(Cell::new(9, 9)).with_state(GuardState::Calm),
        ];
        assert_eq!(
            nearest_respondable(&guards, post),
            Some(1),
            "skip the chaser"
        );

        let all_hunting = vec![
            Guard::stationary(Cell::new(2, 1)).with_state(GuardState::Chasing),
            Guard::stationary(Cell::new(3, 1)).with_state(GuardState::Investigating),
        ];
        assert_eq!(
            nearest_respondable(&all_hunting, post),
            None,
            "nobody free to send",
        );
    }
}
