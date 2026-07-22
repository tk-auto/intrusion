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
}

impl Body {
    /// A fresh body at `cell`, fallen from a guard whose station was `post`.
    pub(crate) fn new(cell: Cell, post: Cell) -> Self {
        Self {
            cell,
            post,
            found: false,
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
}
