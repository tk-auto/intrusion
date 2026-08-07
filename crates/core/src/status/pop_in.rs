//! Which messages are too loud for the near line alone (§11.7/#576) — the *gate*,
//! beside the ladder that decides it. How the box is drawn is
//! [`render::pop_in`](crate::render); when it goes is the shell's clock.
//!
//! Some events are too important to be lost on the near line. The player's eye is on
//! the board — on the guard two rooms over and on the cell they are about to step into
//! — not on the top row of the screen, and a one-row band at the far edge of their
//! attention is exactly what gets missed. The near line is the right home for *what is
//! around you*; it is the wrong home for *the thing you came here for just happened*.
//! So the loudest rung gets a second surface, drawn over the board next to what it is
//! about, which appears, is read, and goes.
//!
//! **Which messages qualify is derived from the ladder, never hand-flagged.** Nothing
//! gets a "loud" bit set at the raise site: the gate is a threshold on the §11.7
//! priority every message already carries, so a future objective event inherits the
//! pop-in for free and cannot be forgotten, and the ladder stays the single place
//! importance is decided.

use super::{message_for, Message};
use crate::state::{Event, State};

/// The rung from which a message earns a pop-in (§11.7/#576): **objective feedback**,
/// the top of the ladder — intel in hand, a crate salvaged, the exit opening, the run
/// won.
///
/// That it derives from the ladder is **[SETTLED]**; the threshold itself is
/// **[START]**. Lowering it to 10 would hand the box to the two endings (a capture, the
/// wall taking you), which already have the verdict card drawn over the whole board a
/// frame later; raising it above 20 would empty the surface, since 20 is the top rung.
pub const POP_IN_PRIORITY: i32 = 20;

/// A message loud enough to leave the near line (§11.7/#576) — what the shell holds
/// for the ~2 s the box is up.
///
/// It carries the **event**, not the [`Message`] built from it, and that is what keeps
/// it `Copy`: the view state a shell hands to
/// [`render_screen`](crate::render_screen) is a plain copied value
/// ([`ScreenUi`](crate::ScreenUi)), and a `String` in it would turn every one of those
/// call sites into a clone. The words are re-derived through [`message_for`] — the
/// ladder's single source — so the box and the near line's own copy cannot word the
/// same fact two ways.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PopIn(Event);

impl PopIn {
    /// The message this box shows: its words, and the §11.2 category its border is
    /// drawn in.
    pub fn message(self) -> Message {
        message_for(self.0).expect("a pop-in is only ever raised from an event that speaks")
    }
}

/// The pop-in this action raises, if it raised one (§11.7/#576): the loudest message
/// of the player's last action, if it reaches [`POP_IN_PRIORITY`].
///
/// **Ties resolve as the near line's do** — the later event leads
/// ([`near_line`](super::near_line), whose `max_by_key` keeps the later of equal keys)
/// — so on a turn that lands two objective facts, the band and the box name the same
/// one. Subordinate messages (§11.7's spliced *why* lines) are not consulted: they wear
/// their headline's priority and would only ever tie with the headline they belong
/// under, which is the message the box is already showing.
///
/// The shell asks once per action and **keeps the answer** for the life of its clock:
/// `None` here means *this action raised nothing*, never *take the box down*. That is
/// the ride-out rule (§11.7) — a player who acts inside two seconds is the very case
/// the box exists for.
pub fn pop_in(state: &State) -> Option<PopIn> {
    state
        .last_events()
        .iter()
        .filter_map(|&event| Some((event, message_for(event)?.priority)))
        .filter(|&(_, priority)| priority >= POP_IN_PRIORITY)
        .max_by_key(|&(_, priority)| priority)
        .map(|(event, _)| PopIn(event))
}
