//! The loud-message pop-in's **clock** (§11.7/#576) — the shell's half of the box.
//!
//! Which messages earn a box, what it says and where on the board it goes are all the
//! core's ([`intrusion_core::pop_in`], `core::render::pop_in`). What cannot be the
//! core's is *when it goes away*: `crates/core` is pure and turn-based (§12.1) and two
//! seconds is not a number the rules may know — a wall clock in there would put the
//! shape of the frame outside `(seed, [inputs])` and cost the §12.4 replay its identity.
//!
//! So the shell holds the box on its view state and arms a timer, and the two rules the
//! surface has fall out of that placement rather than being enforced anywhere:
//!
//! - **it rides out its life** — a turn resolving does not touch
//!   [`ScreenUi::pop_in`](intrusion_core::ScreenUi), so the near line's copy clears on
//!   the next action (§11.7) and the box does not, which is the case it exists for: a
//!   player who acts inside two seconds is exactly the player who missed the row;
//!   and
//! - **it never queues** — there is one field and one timer, so a second loud message
//!   overwrites the first and re-arms rather than stacking a second box behind it
//!   ([`crate::timer`], whose arming replaces).

use std::cell::RefCell;
use std::rc::Weak;

use crate::timer::{Timer, WindowTimer};

/// How long the box stays up, in milliseconds. **[START]** — long enough to catch the
/// eye after the player has already committed to their next key, short enough not to
/// sit over the board while they plan the one after it. Expect to move it by playtest:
/// it is a guess, and the first thing to reach for if the box reads as noise.
pub(crate) const POP_IN_MS: i32 = 2_000;

/// The shell's pop-in clock, wired to the browser.
pub(crate) fn clock(game: Weak<RefCell<crate::Game>>) -> Box<dyn Timer> {
    Box::new(WindowTimer::new(game, |game| game.expire_pop_in()))
}

/// The pop-in half of [`Game`](crate::Game): the two moments the box changes.
impl crate::Game {
    /// A turn resolved — raise a box if it was loud enough (§11.7/#576), and start its
    /// clock again from now.
    ///
    /// A turn that raised nothing leaves whatever is up alone: `None` from the core
    /// means *this action said nothing loud*, never *take the box down*. Taking it down
    /// is the timer's word and the timer's alone.
    pub(crate) fn raise_pop_in(&mut self) {
        let Some(raised) = intrusion_core::pop_in(&self.state) else {
            return;
        };
        self.ui.pop_in = Some(raised);
        self.pop_in.arm(POP_IN_MS);
    }

    /// The clock ran out: the box goes, and the frame is drawn again without it.
    ///
    /// The redraw is the point — nothing else is going to happen between turns, so a
    /// box removed from the view state without a paint would simply stay on the canvas
    /// until the player's next key.
    pub(crate) fn expire_pop_in(&mut self) {
        self.ui.pop_in = None;
        self.draw();
    }

    /// A fresh world replacing this one: the last run's loud message is not this run's,
    /// and the clock it was counting on is moot.
    ///
    /// [`ScreenUi::for_fresh_run`](intrusion_core::ScreenUi) already drops the box —
    /// what this adds is cancelling the pending fire, so an expiry armed by the old run
    /// cannot land a frame into the new one and repaint over its opening card.
    pub(crate) fn reset_pop_in(&mut self) {
        self.ui.pop_in = None;
        self.pop_in.cancel();
    }
}
