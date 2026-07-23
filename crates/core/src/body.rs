//! The body a takedown leaves behind (§7.2) — the cost made physical.
//!
//! A takedown is permanent and free of cooldown; **the body is the cost**. It is
//! a solid entity (fill 1.0, like every actor — §4.3) the level owns directly
//! (§12.3), not terrain stamped into the grid: it blocks movement and guards
//! route around it, but it does not block sight — which is exactly what makes it
//! dangerous to leave lying about. Any guard whose cone covers a body has
//! *found* it, and finding a body is the loudest event in the game (§7.2): it
//! raises that guard's alert harder than seeing the player does.
//!
//! The body also carries what the later systems need: it **can be moved** — the
//! drag (§8.3, #103) — and its [`post`](Body::post) remembers the downed guard's
//! station, the "last known post" control dispatches a responder to when the
//! guard stops answering the radio (§7.3, #107). The radio is what keeps the
//! takedown's permanence costly; this type is the seam it reads.

use crate::cell::Cell;
use crate::radio::{RadioClock, MAX_MISSED_PINGS};

/// A downed guard (§7.2): where the body now lies, and what the world will want
/// to know about it later.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    /// Where the body lies. Moves only by being dragged (§8.3, #103).
    cell: Cell,
    /// The downed guard's station — the "last known post" a missed radio ping
    /// dispatches a responder to (§7.3). Fixed at the takedown; dragging the
    /// body does not change what control believes.
    post: Cell,
    /// Whether a guard's cone has ever covered this body (§7.2). Set once —
    /// found is found — so the loudest event in the game fires exactly once
    /// per body.
    found: bool,
    /// The downed guard's radio ping period (§7.3), inherited from the guard so
    /// the schedule stays deterministic (§12.4). The gap between successive
    /// missed pings, and the window this takedown bought before the first.
    period: u32,
    /// The absolute turn of this body's next radio ping (§7.3). Set at the
    /// takedown to one full period out — so every takedown buys a full window
    /// before control notices — and pushed a period further on each miss.
    next_ping: u32,
    /// How many pings this body has missed (§7.3): the first dispatches a
    /// responder, the second steps the alert, and control stops at
    /// [`MAX_MISSED_PINGS`]. A hidden body still counts them up — hiding
    /// confuses the investigation, it does not cancel it (§7.3).
    misses: u8,
}

impl Body {
    /// A fresh body at `cell`, fallen at turn `turn` from a guard whose station
    /// was `post` and whose radio cadence was `clock` (§7.2/§7.3). The first
    /// ping is scheduled one full period out, so the takedown buys a guaranteed
    /// window before control dispatches (§7.3 — the clock a takedown starts).
    pub(crate) fn new(cell: Cell, post: Cell, clock: RadioClock, turn: u32) -> Self {
        let period = clock.period();
        Self {
            cell,
            post,
            found: false,
            period,
            next_ping: turn.saturating_add(period),
            misses: 0,
        }
    }

    /// Where the body lies.
    pub fn cell(&self) -> Cell {
        self.cell
    }

    /// The downed guard's station — the post a radio dispatch heads for (§7.3).
    pub fn post(&self) -> Cell {
        self.post
    }

    /// Whether any guard has found this body (§7.2).
    pub fn found(&self) -> bool {
        self.found
    }

    /// Record that a guard's cone covered the body (§7.2). Idempotent by
    /// construction — the flag only ever goes one way.
    pub(crate) fn mark_found(&mut self) {
        self.found = true;
    }

    /// Move the body to `cell` — the drag (§8.3, #103): the loop hauls it into
    /// the cell the dragging player just vacated. The [`post`](Self::post) stays
    /// where control believes it: dragging fools the radio, not the record.
    pub(crate) fn move_to(&mut self, cell: Cell) {
        self.cell = cell;
    }

    /// How many radio pings this body has missed (§7.3) — for the render/tests
    /// and the loop's cap check.
    pub fn missed_pings(&self) -> u8 {
        self.misses
    }

    /// Whether a radio ping comes due for this body on `turn` (§7.3): its
    /// scheduled ping has arrived and it has not already been escalated to the
    /// [`MAX_MISSED_PINGS`] cap (control stops calling a guard it has given up
    /// on). Independent of whether the body has been found or hidden — a hidden
    /// body still misses its ping (§7.3).
    pub(crate) fn ping_due(&self, turn: u32) -> bool {
        self.misses < MAX_MISSED_PINGS && turn >= self.next_ping
    }

    /// Record a missed ping (§7.3): count it and schedule the next one a full
    /// period out. Returns the new miss count so the loop can act on the first
    /// (dispatch) and the second (alert step). Only called when
    /// [`ping_due`](Self::ping_due) held, so it never runs past the cap.
    pub(crate) fn miss_ping(&mut self) -> u8 {
        self.misses += 1;
        self.next_ping = self.next_ping.saturating_add(self.period);
        self.misses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §7.3: a takedown buys a full period before the first ping comes due, and
    /// each missed ping schedules the next one a period further on — the clock a
    /// takedown starts (§7.3), until control gives up at [`MAX_MISSED_PINGS`].
    #[test]
    fn the_ping_schedule_counts_two_misses_a_period_apart_then_stops() {
        let mut body = Body::new(
            Cell::new(3, 3),
            Cell::new(3, 3),
            RadioClock::from_period(4),
            10, // downed on turn 10
        );

        // A full period's window before the first ping: due at 10 + 4 = 14.
        assert!(!body.ping_due(13), "the window has not closed yet");
        assert!(body.ping_due(14), "the first ping comes due a period out");
        assert_eq!(body.miss_ping(), 1, "first miss");

        // The second ping is one further period on: turn 18.
        assert!(!body.ping_due(17));
        assert!(body.ping_due(18));
        assert_eq!(body.miss_ping(), 2, "second miss");

        // Control has escalated as far as it will — it stops pinging the corpse.
        assert!(
            !body.ping_due(1_000),
            "no more pings after the cap ({MAX_MISSED_PINGS})",
        );
        assert_eq!(body.missed_pings(), 2);
    }
}
