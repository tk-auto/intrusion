//! The shell's **one-shot clock** (§12.1): the browser timer the two timed surfaces
//! share, and the seam that keeps both of them testable without a page.
//!
//! `crates/core` is pure and turn-based — no clock, no `Date::now` — so anything that
//! happens *after a while* rather than *on the next turn* is the shell's, and there are
//! two of them: the autosave's trailing write ([`crate::save`], §12.5/#514) and the
//! loud-message pop-in's couple of seconds ([`crate::pop_in`], §11.7/#576).
//!
//! They want the same clock and the same one rule about it — **arming replaces rather
//! than stacks** — so it lives here once. That rule is not an implementation detail in
//! either case: it is what makes the autosave a debounce rather than a queue of writes,
//! and it is what makes the pop-in *replace* rather than *stack up* when a second loud
//! message lands. One pending handle, cancelled before the next is armed, is the whole
//! of both.
//!
//! [`Timer`] is a trait rather than a `setTimeout` call at each site so the policies
//! above can be driven natively: a test counts what crossed this boundary and never
//! goes near a browser.

use std::cell::{Cell, RefCell};
use std::rc::Weak;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// A **one-shot timer**: the clock half of anything the shell does on a delay. Arming
/// replaces any pending fire, so a burst re-arms one timer rather than queueing one
/// each.
pub(crate) trait Timer {
    /// Fire `ms` from now, cancelling any pending fire.
    fn arm(&self, ms: i32);
    /// Cancel any pending fire.
    fn cancel(&self);
}

/// The browser's `setTimeout`, holding the pending handle so arming replaces rather
/// than stacks.
///
/// It reaches back into the shell through a **weak** handle, exactly as the clipboard
/// callback does: the timer is owned by the game it calls, and an upgrade that fails
/// simply means the page is gone and there is nothing left to do.
pub(crate) struct WindowTimer {
    handle: Cell<Option<i32>>,
    /// The callback the timer fires, built once and kept: it outlives every arming,
    /// so the shell hands the browser the same function each time rather than leaking
    /// a fresh closure per turn.
    fire: Closure<dyn FnMut()>,
}

impl WindowTimer {
    /// A timer that calls `fire` on the shell when it goes off.
    ///
    /// `fire` is a plain function pointer rather than a closure, which is the whole
    /// reason one type serves both callers: what differs between the autosave's clock
    /// and the pop-in's is *which method of the shell runs*, and nothing else.
    pub(crate) fn new(game: Weak<RefCell<crate::Game>>, fire: fn(&mut crate::Game)) -> Self {
        let fire = Closure::<dyn FnMut()>::new(move || {
            if let Some(game) = game.upgrade() {
                fire(&mut game.borrow_mut());
            }
        });
        Self {
            handle: Cell::new(None),
            fire,
        }
    }
}

impl Timer for WindowTimer {
    fn arm(&self, ms: i32) {
        self.cancel();
        let Some(win) = web_sys::window() else { return };
        // The handle is what makes this a *replacement* rather than a queue: the next
        // arming cancels this fire before scheduling its own.
        if let Ok(id) = win.set_timeout_with_callback_and_timeout_and_arguments_0(
            self.fire.as_ref().unchecked_ref(),
            ms,
        ) {
            self.handle.set(Some(id));
        }
        // Nothing to do if the browser refused a timer. What that costs is the caller's
        // to bound: the autosave still writes on page-hide and at its turn cap, so a
        // save is late rather than lost, and a pop-in simply stays up until the next
        // one replaces it.
    }

    fn cancel(&self) {
        if let (Some(id), Some(win)) = (self.handle.take(), web_sys::window()) {
            win.clear_timeout_with_handle(id);
        }
    }
}
