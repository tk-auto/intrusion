//! The near line's message system (§11.7) — what the bottom-but-one row says.
//!
//! The loop reports facts as [`Event`]s; this module turns them into the
//! messages the **near line** (§11.4) shows: each event becomes at most one
//! [`Message`] carrying its §11.2 category and its rung on the §11.7 priority
//! ladder. [`live_messages`] is the whole set from the *last* step, loudest
//! first — what the near line deploys when more than one is live (§11.7) — and
//! [`near_line`] speaks only its loudest. Messages clear on the player's next
//! action (§11.7), a status line, not a scrollback. When none is live the line
//! does not sit empty: it falls back to [`ambient`] status — the quiet floor
//! below every message — so the row always says something true about now.
//!
//! The **usable line** below it is deliberately *not* here: it is no message at
//! all but a pure derived view of adjacency
//! ([`State::affordances`](crate::State::affordances)), recomputed every frame
//! with no plumbing to clear.

use crate::category::Category;
use crate::state::{Event, State};

/// One §11.7 message: what the near line says, the §11.2 category that colours
/// its band, and its rung on the priority ladder. (A source cell joins when
/// modal source-anchored messages land.)
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Message {
    /// The words. Kept short: the near line is one grid row.
    pub text: String,
    /// What the message *means* (§11.2) — the shell colours the band from this.
    pub category: Category,
    /// The §11.7 ladder: routine self-narration ≤ 0, threat 2 → 4 → 10,
    /// objective feedback 20. Ambient status sits below everything at
    /// `i32::MIN` — it is the floor, not a message.
    pub priority: i32,
}

/// The message an event shows on the near line, if it shows one at all.
///
/// [`Event::Moved`] is the one silent event: narrating every step would bury
/// the line in noise, and the move is already visible — the `@` moved. Category
/// comes from [`Event::category`], the single place meaning is declared, so a
/// red near line and a red `g` reinforce (§11.2).
pub fn message_for(event: Event) -> Option<Message> {
    let (text, priority) = match event {
        Event::Moved { .. } => return None,
        Event::Bumped { .. } => ("blocked".to_string(), 0),
        Event::Crouched { .. } => ("you duck behind the table".to_string(), 0),
        Event::EnteredHideout { .. } => ("you slip into the cupboard".to_string(), 0),
        Event::DoorOpened { .. } => ("the door opens".to_string(), 0),
        Event::IntelTaken { remaining: 0 } => ("intel in hand — the exit is open".to_string(), 20),
        Event::IntelTaken { remaining } => (format!("intel taken — {remaining} to go"), 20),
        Event::ExitRefused => ("the exit refuses — intel is still out".to_string(), 20),
        Event::Won => ("you slip away — the run is won".to_string(), 20),
        Event::Captured { .. } => ("caught".to_string(), 10),
        // The other death (§8.3): rematerializing inside something solid. The
        // top of the threat ladder, like the capture — it ends the run.
        Event::Entombed { .. } => ("the wall takes you".to_string(), 10),
        // Your one offensive verb (§7.2): quiet self-narration, like a crouch —
        // the loud half is what happens if the body is ever seen.
        Event::TakenDown { .. } => ("the guard drops — a body is left".to_string(), 0),
        // The bottom rung of the §11.7 threat ladder: a guard's look freshly
        // found you (§7.6). Quieter than a found body, far below being caught —
        // but a threat message, never self-narration.
        Event::Detected { .. } => ("a guard has seen you".to_string(), 2),
        // The loudest event in the game (§7.2): a hunting-threat message, on the
        // §11.7 threat ladder above a glimpse but below being caught.
        Event::BodyFound { .. } => ("a body has been found".to_string(), 4),
        // A guard stopped answering the radio (§7.3): control is dispatching a
        // responder. A hunting-threat message — above a fresh glimpse, below a
        // found body: a silence is suspicion, a body is proof.
        Event::RadioSilence { .. } => ("a guard has gone silent".to_string(), 3),
        // The facility alert stepped (§7.3): the loudest radio event, a
        // facility-wide escalation — above a found body, below being caught.
        Event::AlertRaised { level } => (format!("the facility is on alert — level {level}"), 5),
        // Handling the body (§8.3): quiet self-narration, like the crouch. The
        // held state itself lives on the ambient floor, not in a message.
        Event::BodyGrabbed { .. } => ("you take hold of the body".to_string(), 0),
        Event::BodyReleased { .. } => ("you let the body go".to_string(), 0),
        // Your fake, trampled (§8.3) — quiet Owned narration; the fade-out by
        // duration reads as the ability's own expiry message.
        Event::DecoyDied { .. } => ("the decoy is trampled".to_string(), 0),
        // Your own tools (§8), routine self-narration like a bump or a crouch —
        // low priority, Owned band (from `Event::category`).
        Event::AbilityActivated { ability } => (format!("{} active", ability.name()), 0),
        Event::AbilityDeactivated { ability } => (format!("{} off", ability.name()), 0),
        Event::AbilityExpired { ability } => (format!("{} fades", ability.name()), 0),
    };
    Some(Message {
        text,
        category: event.category(),
        priority,
    })
}

/// Every live message from the player's last action (§11.7), **loudest first** —
/// the full set the deployable near line lists, of which [`near_line`] speaks
/// only the first. Empty when the last action said nothing: the ambient floor is
/// not a message (§11.4) and never joins the list. Ties resolve as the near line
/// does — the later event leads — so the first entry is exactly the band the near
/// line paints, and the two can never disagree.
pub fn live_messages(state: &State) -> Vec<Message> {
    let mut messages: Vec<Message> = state
        .last_events()
        .iter()
        .filter_map(|&e| message_for(e))
        .collect();
    // The near line shows the *last* of the top-priority events (`max_by_key`
    // keeps the later of equal keys). Lead the list with that same message:
    // reverse to later-first, then a **stable** descending sort by priority keeps
    // later-first order within each rung.
    messages.reverse();
    messages.sort_by_key(|m| std::cmp::Reverse(m.priority));
    messages
}

/// What the near line shows right now (§11.4/§11.7): the highest-priority
/// message from the player's last action — ties go to the later event, matching
/// resolution order — or the [`ambient`] floor when the last action said
/// nothing. The loudest of [`live_messages`], so the band and the deployed list
/// never disagree. Once the run ends the loop goes inert and the final message
/// (the win, or `caught`) simply stays.
pub fn near_line(state: &State) -> Message {
    live_messages(state)
        .into_iter()
        .next()
        .unwrap_or_else(|| ambient(state))
}

/// The ambient floor (§11.4): the quiet status the near line rests on between
/// messages, so it never sits empty. Concealment first — while hidden, crouched
/// or dragging, *that* is the fact shaping the player's next decision (and the
/// Owned band matches the recoloured cupboard or table, §10.3). Otherwise, when
/// the facility is on alert (§7.3), the standing alert level — a raised alert is
/// the thing reshaping every choice out in the open, and this is where it stays
/// *visible* rather than written-but-unseen (§2.3). Failing all that, the
/// objective tally.
fn ambient(state: &State) -> Message {
    let (text, category) = if state.hidden() {
        (
            "hidden — the cupboard conceals you".to_string(),
            Category::Owned,
        )
    } else if state.crouched() {
        ("crouched behind cover".to_string(), Category::Owned)
    } else if state.dragging().is_some() {
        // The held state (§8.3): what shapes every next step while it lasts —
        // and the standing explanation of the half-speed turns.
        (
            "dragging the body — half speed".to_string(),
            Category::Owned,
        )
    } else if state.alert() > 0 {
        // The alert indicator (§7.3/§11.4): the escalation the radio net wrote,
        // read here whenever no louder message is live — a Warning-band fact, not
        // a threat that has you (Danger).
        (
            format!("facility alert — level {}", state.alert()),
            Category::Warning,
        )
    } else {
        match state.objectives_remaining() {
            0 => (
                "all intel in hand — reach the exit".to_string(),
                Category::Interest,
            ),
            n => (format!("intel remaining: {n}"), Category::Interest),
        }
    };
    Message {
        text,
        category,
        priority: i32::MIN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, Direction};
    use crate::facility::Terrain;
    use crate::guard::Guard;
    use crate::state::Input;
    use crate::test_support::open_room;

    /// A walled box with the player at `player`, one intel console at `intel`,
    /// and the exit far away — enough state to generate real messages.
    fn state(player: Cell, intel: Cell) -> State {
        State::new(
            open_room(12, 12),
            player,
            Direction::North,
            Vec::new(),
            [intel],
            Cell::new(10, 10),
        )
    }

    /// §11.7: the near line shows the **highest-priority** message of the last
    /// action. Taking intel also moves nothing and bumps nothing, so build the
    /// contest directly: a turn whose events include both routine narration and
    /// objective feedback shows the objective.
    #[test]
    fn the_highest_priority_message_wins() {
        let mut s = state(Cell::new(5, 6), Cell::new(5, 5));
        s.step(Input::Step(Direction::North)); // bump the console: intel taken
        let line = near_line(&s);
        assert_eq!(line.text, "intel in hand — the exit is open");
        assert_eq!(line.category, Category::Interest);
        assert_eq!(line.priority, 20);
    }

    /// §11.7: messages **clear on the player's next action** — to the ambient
    /// floor, never to an empty row.
    #[test]
    fn a_message_clears_to_ambient_on_the_next_action() {
        let mut s = state(Cell::new(5, 6), Cell::new(3, 3));
        s.step(Input::Step(Direction::West)); // a plain move: narrates nothing
        assert_eq!(near_line(&s).priority, i32::MIN, "a move narrates nothing");
        assert_eq!(near_line(&s).text, "intel remaining: 1");

        let mut s = state(Cell::new(3, 4), Cell::new(3, 3));
        s.step(Input::Step(Direction::North)); // take the intel: a loud message
        assert_eq!(near_line(&s).priority, 20);
        s.step(Input::Step(Direction::South)); // next action: the message clears
        let line = near_line(&s);
        assert_eq!(line.text, "all intel in hand — reach the exit");
        assert_eq!(line.category, Category::Interest);
    }

    /// The ambient floor tracks concealment first (§10.3): hidden and crouched
    /// read as Owned — the same vocabulary as the recoloured cupboard and table.
    #[test]
    fn ambient_reports_concealment_as_owned() {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        layout.place(Cell::new(8, 7), Terrain::PartialCover);
        let mut s = State::new(
            layout,
            Cell::new(5, 6),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(10, 10),
        );

        s.step(Input::Step(Direction::North)); // climb into the cupboard
        s.step(Input::Wait); // a quiet turn inside: the entry message has cleared
        let line = near_line(&s);
        assert_eq!(line.text, "hidden — the cupboard conceals you");
        assert_eq!(line.category, Category::Owned);

        let mut s = State::new(
            open_room(12, 12),
            Cell::new(5, 6),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(10, 10),
        );
        s.step(Input::Wait); // no cover adjacent: waiting narrates nothing
        assert_eq!(near_line(&s).text, "all intel in hand — reach the exit");
    }

    /// A crouch engaging is a message (Owned, §10.3); holding the crouch on the
    /// next wait repeats nothing and the ambient takes over.
    #[test]
    fn a_crouch_reports_once_then_reads_as_ambient() {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(6, 6), Terrain::PartialCover);
        let mut s = State::new(
            layout,
            Cell::new(5, 6),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(10, 10),
        );
        s.step(Input::Step(Direction::East)); // bump the table: the crouch engages
        let line = near_line(&s);
        assert_eq!(line.text, "you duck behind the table");
        assert_eq!(line.category, Category::Owned);

        s.step(Input::Wait); // holding: no new event, the ambient shows the state
        assert_eq!(near_line(&s).text, "crouched behind cover");
    }

    /// §8.3: while dragging on open ground, the ambient floor names the held
    /// state and its cost — the standing explanation of every half-speed turn.
    #[test]
    fn ambient_reports_dragging_as_owned() {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::stationary(Cell::new(5, 4))],
            Vec::new(),
            Cell::new(10, 10),
        );
        s.step(Input::Step(Direction::North)); // takedown
        s.step(Input::Step(Direction::East)); // out of the cupboard
        s.step(Input::Step(Direction::North)); // beside the body
        s.step(Input::Step(Direction::West)); // grab: the message turn
        assert_eq!(near_line(&s).text, "you take hold of the body");

        s.step(Input::Wait); // the message clears to the held state
        let line = near_line(&s);
        assert_eq!(line.text, "dragging the body — half speed");
        assert_eq!(line.category, Category::Owned);
    }

    /// The §11.7 threat ladder's bottom rung: a fresh detection (§7.6) is a
    /// Danger message at priority 2 — above self-narration, below a found body
    /// (4) and the capture (10) — matching the hunting `g` it announces.
    #[test]
    fn a_fresh_detection_reads_as_the_lowest_threat_rung() {
        let msg = message_for(Event::Detected {
            by: Cell::new(5, 5),
        })
        .expect("a detection is never silent");
        assert_eq!(msg.text, "a guard has seen you");
        assert_eq!(msg.category, Category::Danger);
        assert_eq!(msg.priority, 2);
    }

    /// §7.3/§11.7: the radio events read as Warning-band threat messages — a
    /// silence (a guard stopped answering) below a found body, an alert step (the
    /// facility-wide escalation) above it, both below being caught.
    #[test]
    fn the_radio_events_read_on_the_threat_ladder() {
        let silence = message_for(Event::RadioSilence {
            post: Cell::new(3, 3),
        })
        .expect("a radio silence is never silent");
        assert_eq!(silence.text, "a guard has gone silent");
        assert_eq!(silence.category, Category::Warning);
        assert_eq!(silence.priority, 3);

        let alert = message_for(Event::AlertRaised { level: 2 }).expect("an alert step speaks");
        assert_eq!(alert.text, "the facility is on alert — level 2");
        assert_eq!(alert.category, Category::Warning);
        assert_eq!(alert.priority, 5);
    }

    /// §7.3/§11.4: once the radio has stepped the facility alert, the value is
    /// *readable* — with no louder message live, the ambient floor surfaces it in
    /// the Warning band, never written-but-invisible (§2.3).
    #[test]
    fn ambient_surfaces_the_facility_alert() {
        use crate::radio::RadioClock;
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 5), Terrain::Hideout); // conceal the takedown
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            // A lone victim on a 2-turn clock: no responder needed for the alert
            // to step (both missed pings still land).
            vec![Guard::stationary(Cell::new(5, 4)).with_radio_clock(RadioClock::from_period(2))],
            Vec::new(),
            Cell::new(10, 10),
        );
        s.step(Input::Step(Direction::North)); // take the victim down (concealed)
        s.step(Input::Step(Direction::South)); // step out of the cupboard: no longer hidden
        for _ in 0..6 {
            s.step(Input::Wait); // wait out both pings; the last waits are quiet
        }
        assert!(s.alert() >= 1, "the second missed ping stepped the alert");
        assert!(
            !s.hidden(),
            "out of the cupboard, so the alert is the ambient fact"
        );

        // No message is live after the quiet waits: the near line rests on the alert.
        let line = near_line(&s);
        assert_eq!(line.text, format!("facility alert — level {}", s.alert()));
        assert_eq!(line.category, Category::Warning);
        assert_eq!(
            line.priority,
            i32::MIN,
            "it is the ambient floor, not a message"
        );
    }

    /// Once the run ends the loop is inert (§4.5) and the final message stays —
    /// `caught` on a capture, in Danger, at the top of the threat ladder.
    #[test]
    fn the_final_message_persists_after_the_run_ends() {
        // A guard sent straight down the column into the player.
        let s = {
            let mut s = State::new(
                open_room(12, 12),
                Cell::new(5, 5),
                Direction::North,
                vec![Guard::patrolling_to(Cell::new(5, 3), Cell::new(5, 10))],
                Vec::new(),
                Cell::new(10, 10),
            );
            s.step(Input::Wait); // guard steps to (5,4)
            s.step(Input::Wait); // guard steps into the player: captured
            s
        };
        let line = near_line(&s);
        assert_eq!(line.text, "caught");
        assert_eq!(line.category, Category::Danger);
        assert_eq!(line.priority, 10);

        // Inert: further input changes nothing, the message included.
        let mut s = s;
        s.step(Input::Wait);
        assert_eq!(near_line(&s).text, "caught");
    }

    /// §11.7: a single step can raise more than one message — a takedown seen by
    /// a second guard is `TakenDown` *and* `BodyFound` — and [`live_messages`]
    /// returns them all, **loudest first**, leading with exactly what
    /// [`near_line`] speaks so the deployed list and the band never disagree.
    #[test]
    fn live_messages_lists_the_whole_step_loudest_first() {
        // A hidden strike north on an adjacent victim, with a witness two cells up
        // whose cone covers the fresh body: the same turn yields the takedown
        // (self-narration, priority 0) and the found body (a threat, priority 4).
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            vec![
                Guard::stationary(Cell::new(5, 4)),
                Guard::stationary(Cell::new(5, 2)),
            ],
            Vec::new(),
            Cell::new(8, 8),
        );
        s.step(Input::Step(Direction::North));

        let live = live_messages(&s);
        assert_eq!(
            live.iter().map(|m| m.priority).collect::<Vec<_>>(),
            vec![4, 0],
            "loudest first: the found body outranks the takedown narration",
        );
        assert_eq!(live[0].text, "a body has been found");
        assert_eq!(live[1].text, "the guard drops — a body is left");
        assert_eq!(
            live.first().cloned(),
            Some(near_line(&s)),
            "the list leads with exactly the near line's band",
        );
    }

    /// A quiet action raises no message: [`live_messages`] is empty and the near
    /// line rests on the ambient floor (§11.4), which is never a list entry.
    #[test]
    fn live_messages_is_empty_when_the_action_is_quiet() {
        let mut s = state(Cell::new(5, 6), Cell::new(3, 3));
        s.step(Input::Step(Direction::West)); // a plain move narrates nothing
        assert!(live_messages(&s).is_empty());
        assert_eq!(near_line(&s).priority, i32::MIN, "the ambient floor");
    }
}
