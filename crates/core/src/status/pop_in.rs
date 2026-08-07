//! Which messages are too loud for the near line alone (§11.7/#576) — the *gate*,
//! beside the ladder that decides it, and what the row says while the box has one of
//! its messages. How the box is drawn is [`render::pop_in`](crate::render); when it
//! goes is the shell's clock.
//!
//! Some events are too important to be lost on the near line. The player's eye is on
//! the board — on the guard two rooms over and on the cell they are about to step into
//! — not on the top row of the screen, and a one-row band at the far edge of their
//! attention is exactly what gets missed. The near line is the right home for *what is
//! around you*; it is the wrong home for *the thing you came here for just happened*,
//! or for *the building just got worse*. So the loud rungs get a second surface, drawn
//! over the board next to what it is about, which appears, is read, and goes.
//!
//! **Which messages qualify is derived from the ladder, never hand-flagged.** Nothing
//! gets a "loud" bit set at the raise site: the gate is a threshold on the §11.7
//! priority every message already carries, so a future loud event inherits the pop-in
//! for free and cannot be forgotten, and the ladder stays the single place importance
//! is decided.
//!
//! # The box *takes* the message; it does not copy it
//!
//! While a box is up, the message in it comes off the near line and out of the live
//! block of the deployed log ([`live_messages_beside`]): the same words in two places at
//! once is one fact wearing two surfaces, and the row is better spent on the *next*
//! thing the turn had to say — a guard that has seen you, say, while the box carries the
//! intel you just took. Nothing is lost by it. The moment the box goes, the message is
//! back in its expected place on both surfaces, and the history the log stacks under the
//! rule was never touched.

use super::{ambient, loudest_first, message_for, subordinate_for, Message};
use crate::state::{Event, State};

/// The rung from which a message earns a pop-in (§11.7/#576).
///
/// **Where the line falls, and why there.** Below it, a message is one guard's business
/// — a look that found you, a radio gone quiet, a body found — and the *board already
/// draws it*: the cone, the §9 sense mark, the watcher line. Those are facts the player
/// reads by looking where they are already looking. At this rung and above, the message
/// is the **facility's** or the **run's** — a rung of the §7.3 ladder, a body reported,
/// the phase safety firing, a capture evaded, an ending, an objective — and the board
/// has no way to say any of it. That is the honest split between what needs a second
/// surface and what does not.
///
/// That it derives from the ladder is **[SETTLED]**; the threshold itself is
/// **[START]**. It began at 20 — objective feedback alone — and came down to 5 the first
/// time the alert ladder was watched climbing in play: *the security condition just
/// changed* is the fact that most changes what the next ten turns should be, and it was
/// arriving on the row the player was not reading.
pub const POP_IN_PRIORITY: i32 = 5;

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
    /// The headline this box shows: its words, and the §11.2 category its border is
    /// drawn in.
    pub fn message(self) -> Message {
        message_for(self.0).expect("a pop-in is only ever raised from an event that speaks")
    }

    /// The §11.7 **subordinate** line under that headline (#418), where the event has
    /// one — *why* the facility climbed, under the rung it climbed to.
    ///
    /// The box carries the pair because §11.7 makes the pair inseparable: nothing the
    /// turn does may come between a fact and its reason. With the headline taken off the
    /// near line and out of the live log ([`live_messages_beside`]), a reason left behind
    /// would be exactly that — half a block, on its own, explaining a fact that is no
    /// longer on the surface with it. So the block moves whole, and comes back whole.
    ///
    /// The near line still never shows a subordinate (§11.7): it is one row, and it
    /// speaks the headline alone. The box is not that row.
    pub fn reason(self) -> Option<Message> {
        subordinate_for(self.0, &self.message())
    }

    /// The pop-in `event` would raise, or `None` if it is too quiet to leave the row —
    /// **the gate itself, over one event**. [`pop_in`] is this applied to a turn and the
    /// loudest survivor taken, so there is one place the threshold is read.
    pub(crate) fn of(event: Event) -> Option<Self> {
        (message_for(event)?.priority >= POP_IN_PRIORITY).then_some(Self(event))
    }

    /// Whether this box is the one speaking `event` — the filter behind
    /// [`live_messages_beside`].
    fn speaks(self, event: Event) -> bool {
        self.0 == event
    }
}

/// The pop-in this action raises, if it raised one (§11.7/#576): the loudest message
/// of the player's last action, if it reaches [`POP_IN_PRIORITY`].
///
/// **Ties resolve as the near line's do** — the later event leads
/// ([`near_line`](super::near_line), whose `max_by_key` keeps the later of equal keys)
/// — so on a turn that lands two loud facts, the box takes the one the near line would
/// have led with, and [`live_messages_beside`] hands the row the next one down.
/// Subordinate messages are not consulted: they wear their headline's priority and would
/// only ever tie with the headline they belong under, which the box carries anyway
/// ([`PopIn::reason`]).
///
/// The shell asks once per action and **keeps the answer** for the life of its clock:
/// `None` here means *this action raised nothing*, never *take the box down*. That is
/// the ride-out rule (§11.7) — a player who acts inside two seconds is the very case
/// the box exists for.
pub fn pop_in(state: &State) -> Option<PopIn> {
    state
        .last_events()
        .iter()
        .filter_map(|&event| Some((PopIn::of(event)?, message_for(event)?.priority)))
        .max_by_key(|&(_, priority)| priority)
        .map(|(popped, _)| popped)
}

/// Every live message the *rest of the screen* still has to say (§11.7/#576), given the
/// box that is up: [`live_messages`](super::live_messages) less the block the box is
/// already speaking.
///
/// The whole near line and the whole deployed log are built from this, so the
/// suppression is one filter rather than a rule each surface has to remember — and the
/// two can never disagree about whether the box has taken a message. With `None` it *is*
/// [`live_messages`](super::live_messages), which is what every caller outside a live
/// frame gets.
///
/// The block goes whole: an event's subordinate travels with its headline through
/// [`loudest_first`], so filtering the event takes the reason with it and the log is
/// never left holding an orphaned *why*.
pub fn live_messages_beside(state: &State, popped: Option<PopIn>) -> Vec<Message> {
    let Some(popped) = popped else {
        return loudest_first(state.last_events());
    };
    let left: Vec<Event> = state
        .last_events()
        .iter()
        .copied()
        .filter(|&event| !popped.speaks(event))
        .collect();
    loudest_first(&left)
}

/// What the near line says while `popped` is up (§11.4/§11.7/#576) — the loudest message
/// the box has *not* taken, or the ambient floor when the box has taken the only one
/// there was.
///
/// That fall-through is the point rather than a consequence. A turn where the box says
/// *the security condition just changed* and the row underneath says *a guard has seen
/// you* is two facts on two surfaces; the same sentence twice, an inch apart, is one
/// fact and a wasted row.
pub fn near_line_beside(state: &State, popped: Option<PopIn>) -> Message {
    live_messages_beside(state, popped)
        .into_iter()
        .next()
        .unwrap_or_else(|| ambient(state))
}
