//! What the near line said **before** this action (§11.7/#300).
//!
//! The near line itself is a status line, not a scrollback: it speaks the loudest
//! live message and is wiped by the player's next action. That rule is right for
//! one row and wrong for the panel behind the chevron — a radio silence, a call-in
//! and a body find landing on three consecutive turns is exactly the moment a
//! player wants to read back what set the facility off, and once the near line has
//! moved on the only record of the first two was a colour that flashed.
//!
//! So the deployed log grows a short, bounded scrollback and this is where it is
//! kept: a ring of the last [`HISTORY_ACTIONS`] **message-bearing** actions,
//! newest first, each one already resolved to the [`Message`]s that action raised
//! and in the very order [`live_messages`](super::live_messages) would have listed
//! them.
//!
//! # Why it lives on `State` and not in the shell
//!
//! §12.1 keeps the core pure game logic and §11.7 is presentation, which argues for
//! the shell — but the shell's view state ([`ScreenUi`](crate::ScreenUi)) is a
//! `Copy` value passed by value every frame, and the alternative is the shell
//! accumulating a *second* notion of "what happened this turn" beside the core's
//! events, which the sim (§13.2) and the replay viewer would then disagree about.
//! One record, written where the events are, read by everyone. The type still lives
//! in `status` rather than in the turn loop: which events are worth a row is a
//! §11.7 question, and [`State`](crate::State) only owns the ring and hands it back.
//!
//! It is a **pure function of the run's events**, so it costs determinism nothing
//! (§12.4): the same seed and inputs write the same history.

use super::{loudest_first, Message};
use crate::state::Event;
use std::collections::VecDeque;

/// How many past **message-bearing** actions the deployed log keeps (§11.7).
/// **[START]** — enough to read back the run of turns that set the facility off,
/// which is the case the scrollback exists for. Three was the first guess and it
/// proved short: a silence, a call-in and a body find is already three, so the turn
/// you actually want context for was the one falling off the back.
///
/// The block it feeds is bounded separately by `MAX_LOG_ROWS` (§11.7, in `render`)
/// and clamped to the board besides, so a loud run cannot turn this into a wall of text over the danger
/// overlay (§11.5) — this bound sets how far *back* the record goes, not how much of
/// the screen it may take.
///
/// Actions, not turns: a free bump raises messages and a plain step raises none, so
/// counting *turns* would let a corridor sprint quietly flush the history that
/// matters. An action that said nothing costs no slot.
pub const HISTORY_ACTIONS: usize = 5;

/// The last few actions' messages, newest first (§11.7/#300) — the scrollback the
/// deployed log shows under the current action's block, each older block behind its
/// own separator rule.
///
/// Holds only actions that actually said something: an action whose events raised no
/// message contributes no entry, which is what keeps a run of quiet steps from
/// flushing the record and what makes "no empty bands, no doubled rules" true by
/// construction rather than by filtering at draw time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageHistory {
    /// Newest first, at most [`HISTORY_ACTIONS`] long.
    blocks: VecDeque<Vec<Message>>,
}

impl MessageHistory {
    /// File the events of the action the near line is **done with** — called as the
    /// turn loop replaces its live set, so what goes in here is exactly what the near
    /// line stops showing. Silent actions are dropped, and the oldest block falls off
    /// the back once the ring is full.
    pub(crate) fn record(&mut self, events: &[Event]) {
        let block = loudest_first(events);
        if block.is_empty() {
            return;
        }
        self.blocks.push_front(block);
        self.blocks.truncate(HISTORY_ACTIONS);
    }

    /// The kept blocks, **newest first** — the order the log stacks them in, each one
    /// already loudest-first within itself.
    pub fn blocks(&self) -> impl Iterator<Item = &[Message]> {
        self.blocks.iter().map(Vec::as_slice)
    }

    /// Whether anything at all is remembered — what the near line's deploy control
    /// asks when only one message (or none) is live (§11.7).
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alert::AlertTrigger;
    use crate::category::Category;
    use crate::cell::Cell;

    /// A quiet action leaves no trace, a loud one is filed loudest-first, and the ring
    /// never grows past [`HISTORY_ACTIONS`] however long the run goes on.
    #[test]
    fn the_ring_keeps_the_last_few_loud_actions_newest_first() {
        let mut history = MessageHistory::default();

        // A plain step says nothing (§11.7), so it costs no slot.
        history.record(&[Event::Moved {
            to: Cell::new(1, 2),
        }]);
        assert!(history.is_empty(), "a silent action files nothing");

        // One more loud action than the ring can hold, oldest first — the alert step is
        // the loudest of each. Driven off the bound, so retuning it retunes the test.
        let filed = HISTORY_ACTIONS as u32 + 1;
        for rung in 1..=filed {
            history.record(&[
                Event::TakenDown {
                    at: Cell::new(rung, 1),
                },
                Event::AlertRaised {
                    rung,
                    trigger: AlertTrigger::Sighting,
                },
            ]);
        }

        let blocks: Vec<Vec<Message>> = history.blocks().map(<[Message]>::to_vec).collect();
        assert_eq!(blocks.len(), HISTORY_ACTIONS, "the ring is bounded");
        // Newest first: the last action filed leads, and the oldest has fallen off.
        for (i, block) in blocks.iter().enumerate() {
            let rung = filed - i as u32;
            assert_eq!(
                block[0].text,
                format!("security condition {rung} of {}", crate::alert::TOP_RUNG),
                "block {i} leads with its own loudest message"
            );
            assert_eq!(block[0].category, Category::Warning);
            assert_eq!(
                block[1].text, "you were seen",
                "block {i} remembers why it climbed, right under the headline (#418)"
            );
            assert_eq!(block.len(), 3, "and keeps the quiet half beneath them");
        }
    }
}
