//! The radio net (§7.3) — how a permanent takedown stays costly.
//!
//! Takedowns are permanent and free of cooldown (§7.2); the body is the cost.
//! The radio is what keeps that cost from being paid once: **control pings each
//! guard periodically, and a guard that is down does not answer.** A missed ping
//! dispatches the nearest still-active guard to **where that guard fell**
//! ([`Body::fell_at`](crate::body::Body::fell_at)) — control's last fix on it — to
//! search there, and the silence steps the facility alert ladder (§7.3, [`Alert`](crate::alert::Alert)). So every
//! takedown starts a clock — a future appointment — and three takedowns is three
//! clocks running at once, which is why a full clear collapses under its own weight
//! with no rule needed to ban it (§7.3).
//!
//! This module owns the *timing and selection* — the per-guard cadence and the
//! pure "who responds" query; the [`State`](crate::State) turn loop owns the
//! orchestration (which body is silent this turn, mutating the responder, stepping
//! the alert ladder), because that reaches across guards, bodies and the alert
//! together.
//! The tell is deliberately **visual** (§7.3/§9.3, sound is gone): the silence is a
//! near-line message and the responder is the player's own sensed dot peeling off
//! toward the takedown site — no ping the player has to hear.

use crate::category::Category;
use crate::cell::Cell;
use crate::facility::Facility;
use crate::guard::{self, Guard};
use crate::path;
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

/// How many times control calls a post that does not answer before giving up on it
/// (§7.3, **[START] = 2**): the **first** miss dispatches a responder and steps the
/// facility alert ladder (§7.3), and after the second the guard is presumed gone —
/// control has nothing left to try, so it stops pinging a corpse forever.
///
/// The escalation hangs off the *first* miss, not the second: the ladder's rung-3
/// trigger is two missed pings **across two bodies**
/// ([`SILENT_POSTS_FOR_THIRD_RUNG`](crate::alert::SILENT_POSTS_FOR_THIRD_RUNG)), so a
/// single post going quiet twice is one fact reported twice, not a louder one.
pub(crate) const MAX_MISSED_PINGS: u8 = 2;

/// How many guards a **lost confirmed sighting** calls in (§7.7, **[START] = 1**),
/// when the `sighting_lost_calls_a_guard` modifier is on (§12.6). This count *is*
/// the difficulty dial for the §7.7 net — the design deliberately expresses "how
/// loud was that" as "how many come", not as a reach or a priority — so it is the
/// first knob a tuning pass should sweep once the sim (§13.2) can measure it. One
/// is on purpose: the point is that *someone who was never chasing you* arrives,
/// not that the facility empties onto your last position.
pub(crate) const SIGHTING_CALL_GUARDS: usize = 1;

/// How many guards a **found body** calls in (§7.7, **[START] = 2**), when the
/// `body_found_calls_two_guards` modifier is on (§12.6). Finding a body is the
/// loudest event in the game (§7.2), and this count is the *only* sense in which
/// its call is louder than a sighting's — not a longer reach, not a priority that
/// outranks another lead. Keeping "how loud" and "how many come" the same quantity
/// is what stops cooperation growing a second dial.
pub(crate) const BODY_CALL_GUARDS: usize = 2;
// The §7.7 relation, held at compile time like the §7.2 body-vs-sighting alert:
// a body must always out-call a sighting, whatever either count is retuned to.
const _: () = assert!(BODY_CALL_GUARDS > SIGHTING_CALL_GUARDS);

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

/// The indices of the `count` guards control would send to `at`, nearest first
/// (§7.3/§7.7): the **nearest active** guards by **route distance**, ties broken by
/// spawn order so the choice is deterministic (§12.4). "Active" here means a guard
/// not already locked onto the live player — its state is not [`Category::Danger`]
/// (Chasing/Investigating), because a guard that has *you* does not break off to
/// walk toward a cold radio silence (§7.4). A guard already Calm, searching, or
/// responding is fair game.
///
/// Returns **fewer than `count`** when fewer are free — including empty when every
/// guard has the player, in which case the call simply goes unanswered (for a
/// missed ping the silence goes un-investigated and the alert steps regardless). A
/// call is never queued or retried: whoever is free at the moment it is made is who
/// comes.
///
/// This is the one seam every call in the game shares: control's dispatch to a
/// takedown site sends one, and the §7.7 cooperation call-ins send one on a lost
/// sighting and two on a found body — the same selection, a different `count`.
///
/// Distance is the **route** the guard would actually walk (§7.3/#409), not the
/// straight line it used to be. Manhattan distance made "the nearest active guard"
/// the one that merely *looks* closest: a guard on the far side of a wall with a
/// sixty-step way round outranked one two rooms down the corridor, so the dispatch
/// went to whoever would take longest to arrive. One flood from `at`
/// ([`path::route_lengths_from`]) prices every guard's journey in a single pass —
/// cheap enough on a 40×40 board (§10.2) to run per call, and as deterministic as
/// the walk itself.
///
/// A guard with **no route at all** sorts behind every guard that has one, ordered
/// among its peers by straight-line distance. It is still sent when nobody better is
/// free: §7.7's rule is that whoever is free answers, and control cannot know the
/// route is severed — the responder walks nowhere, cools out and stands down, which
/// is the backstop working rather than a case to special-case away.
pub(crate) fn nearest_respondable(
    guards: &[Guard],
    at: Cell,
    count: usize,
    facility: &Facility,
) -> Vec<usize> {
    let routes = path::route_lengths_from(at, |cell| guard::routable(facility, cell));
    let mut free: Vec<usize> = guards
        .iter()
        .enumerate()
        .filter(|(_, g)| g.state().category() != Category::Danger)
        .map(|(i, _)| i)
        .collect();
    // Sort by journey, then by spawn order — the same shape of total order the
    // Manhattan version had, so taking the first is still taking the nearest; only
    // what "nearest" measures has changed. An absent route is `u32::MAX`, which puts
    // the stranded behind every reachable guard, with the line as their tie-break.
    free.sort_by_key(|&i| {
        let pos = guards[i].pos();
        let route = routes.get(&pos).copied().unwrap_or(u32::MAX);
        (route, pos.manhattan_distance(at), i)
    });
    free.truncate(count);
    free
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facility::Terrain;
    use crate::guard::{Guard, GuardState};

    /// The bare board the selection tests measure over: an open walled box, where a
    /// route is the straight walk and the route order therefore matches the old
    /// Manhattan one — so these pin the *selection* rule, and the wall test below
    /// pins what changed.
    fn room() -> Facility {
        Facility::walled_box(20, 20)
    }

    /// The §7.3 timing knobs are **[START]** values a later tune must move
    /// deliberately — pinned here so the edit is visible. The jitter must keep the
    /// period positive, and the miss cap is exactly two (call, call again, give up).
    #[test]
    fn the_radio_constants_are_pinned() {
        assert_eq!(PING_INTERVAL, 20, "the [START] ping interval");
        assert_eq!(PING_JITTER, 3, "the [START] ping jitter");
        assert_eq!(
            MAX_MISSED_PINGS, 2,
            "control calls twice, then gives up on the post"
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
            Guard::stationary(Cell::new(2, 10)), // 8 away — ties with 0, later spawn
        ];
        assert_eq!(nearest_respondable(&guards, post, 1, &room()), vec![1]);
    }

    /// §7.7: the same selection serves a call for **more than one** guard — nearest
    /// first, ties by spawn order, and never more than are free. Asking for more
    /// than the facility holds yields everyone rather than failing, so a call on a
    /// thinly-staffed level still sends whoever there is (§7.7 — a call is never
    /// queued or retried).
    #[test]
    fn a_call_takes_the_n_nearest_in_order_and_no_more() {
        let at = Cell::new(10, 10);
        let guards = vec![
            Guard::stationary(Cell::new(10, 2)), // 8 away — ties with 2, earlier spawn
            Guard::stationary(Cell::new(10, 6)), // 4 away — nearest
            Guard::stationary(Cell::new(2, 10)), // 8 away
        ];

        assert_eq!(
            nearest_respondable(&guards, at, 2, &room()),
            vec![1, 0],
            "nearest first"
        );
        assert_eq!(
            nearest_respondable(&guards, at, 3, &room()),
            vec![1, 0, 2],
            "the tie at 8 goes to the earlier spawn (§12.4)",
        );
        assert_eq!(
            nearest_respondable(&guards, at, 9, &room()),
            vec![1, 0, 2],
            "asking for more than are free sends everyone free",
        );
        assert!(
            nearest_respondable(&guards, at, 0, &room()).is_empty(),
            "a call for nobody sends nobody",
        );
    }

    /// §7.3/#409: "the nearest active guard" is the one with the shortest **walk**,
    /// not the shortest straight line. A wall with a single gap at its foot puts a
    /// guard four cells away by line sixteen cells away by road, and the dispatch
    /// goes to the one that can actually get there — which under Manhattan it did
    /// not.
    #[test]
    fn dispatch_measures_the_road_not_the_line() {
        // A wall column at x=10 across the room, its one gap at y=18.
        let mut facility = Facility::walled_box(20, 20);
        for y in 1..18 {
            facility.set_terrain(10, y, Terrain::Wall);
        }
        let post = Cell::new(12, 2);
        let guards = vec![
            // Four cells west by line — but the far side of the wall, so the walk is
            // down to the gap at y=18, across and back up: 16 + 2 + 16 = 34.
            Guard::stationary(Cell::new(8, 2)),
            // Eight cells south by line, same side of the wall: an eight-step walk.
            Guard::stationary(Cell::new(12, 10)),
        ];

        assert_eq!(
            guards[0].pos().manhattan_distance(post),
            4,
            "the walled-off guard is the nearest by line",
        );
        assert!(guards[1].pos().manhattan_distance(post) > 4);
        assert_eq!(
            nearest_respondable(&guards, post, 1, &facility),
            vec![1],
            "and the one down the corridor is the nearest by road",
        );
        assert_eq!(
            nearest_respondable(&guards, post, 2, &facility),
            vec![1, 0],
            "the long way round still answers, just second",
        );
    }

    /// §7.7: a guard with **no route at all** sorts behind everyone who has one, and
    /// is still sent when nobody better is free — a call is answered by whoever is
    /// free, and control cannot know the way is severed. It walks nowhere, cools out
    /// and stands down, which is §7.6's backstop doing its job.
    #[test]
    fn a_guard_with_no_route_answers_last_but_still_answers() {
        // A sealed 3x3 cell in the north-west corner, one guard inside it.
        let mut facility = Facility::walled_box(20, 20);
        for i in 0..4 {
            facility.set_terrain(4, i, Terrain::Wall);
            facility.set_terrain(i, 4, Terrain::Wall);
        }
        let post = Cell::new(10, 10);
        let guards = vec![
            Guard::stationary(Cell::new(3, 3)),   // sealed in — no route
            Guard::stationary(Cell::new(18, 18)), // right across the room, but routable
        ];

        assert!(
            guards[0].pos().manhattan_distance(post) < guards[1].pos().manhattan_distance(post),
            "the stranded guard is nearer by line",
        );
        assert_eq!(
            nearest_respondable(&guards, post, 1, &facility),
            vec![1],
            "a guard that can get there outranks one that cannot",
        );
        assert_eq!(
            nearest_respondable(&guards, post, 2, &facility),
            vec![1, 0],
            "but the stranded one is still free, so it is still sent",
        );
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
            nearest_respondable(&guards, post, 1, &room()),
            vec![1],
            "skip the chaser"
        );

        let all_hunting = vec![
            Guard::stationary(Cell::new(2, 1)).with_state(GuardState::Chasing),
            Guard::stationary(Cell::new(3, 1)).with_state(GuardState::Investigating),
        ];
        assert!(
            nearest_respondable(&all_hunting, post, 2, &room()).is_empty(),
            "nobody free to send",
        );
    }
}
