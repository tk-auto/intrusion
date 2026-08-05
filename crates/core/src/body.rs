//! The body a takedown leaves behind (§7.2) — the cost made physical.
//!
//! A takedown is permanent and free of cooldown; **the body is the cost**. It is
//! an entity the level owns directly (§12.3), not terrain stamped into the grid,
//! and since #187 a loose one is **non-solid** (§7.2): it blocks neither movement
//! nor pathing, so you walk over it — and, since #451, take hold by **waiting**
//! while standing on it, which is what makes the pickup a decision rather than
//! something that happens to you on the way past. The
//! one place it counts as an occupant is the door-crush check — a door never
//! shuts on a body ([`actor_occupies`](crate::state)) — which is the exception
//! that makes the rule worth stating.
//!
//! What it never blocks is **sight**, and that is what makes it dangerous to
//! leave lying about. Any guard whose cone covers a body has *found* it, and
//! finding a body is the loudest event in the game (§7.2): it raises that guard's
//! alert harder than seeing the player does.
//!
//! The body also carries what the later systems need: it **can be moved** — the
//! drag (§8.3, #103) — and its [`fell_at`](Body::fell_at) remembers **where the
//! guard went down**, which is control's last fix on it and so the cell a
//! responder is dispatched to search when it stops answering the radio (§7.3).
//! That the two can differ is the point: drag the body elsewhere and the
//! responder searches a spot it is no longer at. The radio is what keeps the
//! takedown's permanence costly; this type is the seam it reads.

use serde::{Deserialize, Serialize};

use crate::cell::Cell;
use crate::radio::{RadioClock, MAX_MISSED_PINGS};

/// A downed guard (§7.2): where the body now lies, and what the world will want
/// to know about it later.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Body {
    /// Where the body lies. Moves only by being dragged (§8.3, #103).
    cell: Cell,
    /// Where the guard **went down** — control's last fix on it, and so the cell
    /// a missed radio ping dispatches a responder to search (§7.3). Fixed at the
    /// takedown: dragging the body moves [`cell`](Self::cell) and leaves this
    /// where control still believes the guard to be.
    fell_at: Cell,
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
    /// A fresh body at `cell`, fallen at turn `turn` from a guard whose radio
    /// cadence was `clock` (§7.2/§7.3). Where it falls is also where control last
    /// had the guard, so [`fell_at`](Self::fell_at) starts equal to `cell` and
    /// then stays put while the body itself can be dragged away. The first ping is
    /// scheduled one full period out, so the takedown buys a guaranteed window
    /// before control dispatches (§7.3 — the clock a takedown starts).
    pub(crate) fn new(cell: Cell, clock: RadioClock, turn: u32) -> Self {
        let period = clock.period();
        Self {
            cell,
            fell_at: cell,
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

    /// Where the guard went down — the cell a radio dispatch heads for and
    /// searches (§7.3).
    pub fn fell_at(&self) -> Cell {
        self.fell_at
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
    /// the cell the dragging player just vacated. [`fell_at`](Self::fell_at) stays
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

    /// §7.3/§8.3: dragging moves the body, never control's fix on it. The radio
    /// dispatch heads for where the guard *fell*, so hauling the body away is
    /// what makes the responder search a cell it is no longer at — the §7.3
    /// "hiding buys you a confused investigation" payoff, made literal.
    #[test]
    fn dragging_moves_the_body_but_not_where_it_fell() {
        let fell = Cell::new(3, 3);
        let mut body = Body::new(fell, RadioClock::from_period(4), 0);
        assert_eq!(body.cell(), fell);
        assert_eq!(body.fell_at(), fell, "a fresh body lies where it fell");

        body.move_to(Cell::new(3, 8));
        assert_eq!(body.cell(), Cell::new(3, 8), "the drag moved it");
        assert_eq!(body.fell_at(), fell, "control's fix does not move with it");
    }
}
