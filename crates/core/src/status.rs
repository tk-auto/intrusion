//! The near line's message system (§11.7) — what the screen's top row says.
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
/// Whether `ability` is **instant** (§8.2): it resolves the turn it is pressed and has
/// no active window at all — Pierce Wall's bore, Confusion's blast (#303/#325). Read
/// off the catalog rather than listed here, so a later ability that ships instant is
/// covered without an edit. A passive has no clock and is never activated, so it is not
/// instant either.
fn is_instant(ability: crate::AbilityId) -> bool {
    ability
        .def()
        .economy()
        .is_some_and(|economy| economy.duration() == 0)
}

pub fn message_for(event: Event) -> Option<Message> {
    let (text, priority) = match event {
        Event::Moved { .. } => return None,
        Event::Bumped { .. } => ("blocked".to_string(), 0),
        Event::Crouched { .. } => ("you duck behind the table".to_string(), 0),
        Event::EnteredHideout { .. } => ("you slip into the cupboard".to_string(), 0),
        Event::EnteredDuct { .. } => ("you climb into the duct".to_string(), 0),
        // A crawl is silent like a plain step — narrating every cell would bury the
        // near line (§11.7).
        Event::DuctCrawled { .. } => return None,
        // A door *you* operate keeps its quiet self-narration (§11.7), like a bump or
        // a crouch. A door that changes **away** from you (a guard walking through,
        // an automatic door timing shut) says nothing on the near line — the durable
        // "someone passed here" evidence is the on-grid door cue instead (§9.4/§10.4,
        // the `Sensed` channel), which is positional and survives the next action.
        Event::DoorOpened {
            by_player: true, ..
        } => ("the door opens".to_string(), 0),
        Event::DoorClosed {
            by_player: true, ..
        } => ("a door swings shut".to_string(), 0),
        Event::DoorOpened {
            by_player: false, ..
        }
        | Event::DoorClosed {
            by_player: false, ..
        } => return None,
        // How much intel is *enough* is the run's gate (§4.5/§12.6), never a fixed
        // count — so what a take announces comes from `still_needed` (the gate's own
        // answer, carried on the event) and not from the tally. At zero the exit is
        // genuinely open and any consoles left are optional extra; above zero the take
        // is progress, and the line says what is still owed instead of promising an
        // exit that would refuse (#310).
        Event::IntelTaken {
            remaining: 0,
            still_needed: 0,
        } => ("all the intel — the exit is open".to_string(), 20),
        Event::IntelTaken {
            remaining,
            still_needed: 0,
        } => (
            format!("intel in hand — the exit is open ({remaining} more out)"),
            20,
        ),
        // Only `IntelGate::All` reaches here — the other gates are met by the first
        // take, or never gate at all — so what is still needed is also all that is
        // still out, and one number says both.
        Event::IntelTaken { still_needed, .. } => {
            (format!("intel in hand — {still_needed} more to go"), 20)
        }
        // The refusal names the requirement it enforces, so it can never contradict a
        // take message from the same run: under `All` that is the rest of the set,
        // under `AtLeastOne` the single console the player has yet to reach.
        Event::ExitRefused { still_needed } => {
            (format!("the exit needs {still_needed} more intel"), 20)
        }
        // The §7.7 counterplay landing (§7.3): the whole net is down for the rest of
        // the level. Ranked with the objective feedback rather than on the threat
        // ladder — it is the payoff for a detour the player chose, and it must not be
        // buried by a guard event on the same turn, because "did that work?" is the one
        // question the bump raises. Nothing ever unsays it: the flag is one-way.
        Event::CommsSilenced { .. } => ("the radio net goes dead".to_string(), 20),
        Event::Won => ("you slip away — the run is won".to_string(), 20),
        Event::Captured { .. } => ("caught".to_string(), 10),
        // The phase safety firing (§8.3/#329). Ranked at the top of the threat ladder
        // short of the run ending: the player has been moved somewhere they did not
        // choose and cannot act for the next turns, and no guard event on the same
        // turn may bury either fact. How *long* the stun lasts is not repeated here —
        // the ambient floor carries the countdown for every turn it runs (§11.4),
        // which is where a standing state belongs and what keeps this line inside the
        // row's budget.
        //
        // It names the **tech**, not the terrain, because the terrain varies: a phase
        // can end inside a wall, a shut door, a table, a cupboard or a console, and a
        // line that said "the wall" would be plainly untrue for most of them. The
        // salvaged tech throwing you clear is also the fiction for why this is
        // survivable at all.
        Event::Ejected { .. } => ("safety eject — stunned".to_string(), 6),
        // The degenerate case (§8.3): nowhere in the facility to be thrown clear to.
        // The top of the ladder, like the capture — it ends the run.
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
        // The §7.7 net closing: you broke contact, and it reported where you were.
        // Ranked with the radio silence — both say "they are converging on a place
        // you were", above a bare glimpse and below the proof a body gives.
        Event::CalledIn { .. } => ("your position was called in".to_string(), 3),
        // A body call reports *a body*, not you — and it lands above the bare find
        // (4): reported is worse than merely found, because guards are now on their
        // way to it. Below the facility-wide alert step (5), which on the rare
        // shared turn is pushed first and so keeps the line (§11.7). Kept short:
        // the near line is one row minus its corner controls (see
        // `NEAR_LINE_TEXT_MAX`), and *that guards are coming* is what "reported"
        // already means — spelling it out only cost the words that fit.
        Event::BodyCalledIn { .. } => ("a body has been reported".to_string(), 5),
        // The facility alert stepped (§7.3): the loudest radio event, a
        // facility-wide escalation — above a found body, below being caught.
        Event::AlertRaised { level } => (format!("the facility is on alert — level {level}"), 5),
        // Handling the body (§8.3): quiet self-narration, like the crouch. The
        // held state itself lives on the ambient floor, not in a message.
        Event::BodyGrabbed { .. } => ("you take hold of the body".to_string(), 0),
        Event::BodyReleased { .. } => ("you let the body go".to_string(), 0),
        Event::BodyStored { .. } => ("you stow the body — the cupboard is sealed".to_string(), 0),
        // Your fake, trampled (§8.3) — quiet Owned narration; the fade-out by
        // duration reads as the ability's own expiry message.
        Event::DecoyDied { .. } => ("the decoy is trampled".to_string(), 0),
        // Your own tools (§8), routine self-narration like a bump or a crouch —
        // low priority, Owned band (from `Event::category`).
        // A **budgeted** ability's activation is silent (§8.2/#302). The count it
        // would have narrated is already on the bar, live and permanent
        // (`Bore(2)`), so saying it again buys a duplicate and costs the row — and
        // a budgeted ability is typically instant, so there is no "active" window to
        // announce either. The near line is a status line, not a receipt.
        Event::AbilityActivated {
            uses_left: Some(_), ..
        } => return None,
        // An **instant** ability's activation is silent for the other half of the same
        // reason (§8.2/#325): there is no active window, so "… active" would be a claim
        // about a state that was over before the message was drawn. What an instant
        // ability *did* is reported by its own event — the bore, the blast — which is
        // the fact worth the row.
        Event::AbilityActivated { ability, .. } if is_instant(ability) => return None,
        Event::AbilityActivated {
            ability,
            uses_left: None,
        } => (format!("{} active", ability.name()), 0),
        Event::AbilityDeactivated { ability } => (format!("{} off", ability.name()), 0),
        Event::AbilityExpired { ability } => (format!("{} fades", ability.name()), 0),
        // A refused toggle-off (§8.3/#304): the same quiet band as the tools it
        // belongs to, because that is what it is — a free press that changed nothing,
        // like a bump. It still has to be said: the player asked to solidify and is
        // still phased, and silence would read as a dropped key.
        Event::RematerializeRefused => ("no room to rematerialize".to_string(), 0),
        // Silent, like [`Event::Moved`] and for the same reason (§11.7): the wall is
        // *gone from the screen* the moment it is bored, so narrating it tells the
        // player something they have already seen and spends the one row the near
        // line has. That the hole serves the guards too is real and load-bearing
        // (§2.3) — but it is taught by a guard walking through it, which is a lesson
        // the near line cannot deliver as well as the board can.
        Event::WallBored { .. } => return None,
        // A refused bore (§8.4/#303): free, changed nothing, and — like the refused
        // rematerialization beside it — has to say why. The reason *is* the message:
        // each one names a different thing to do about it.
        Event::BoreRefused { reason } => (reason.message().to_string(), 0),
        // The blast, reported (§8.3/§11.7/#325). Confusion is the most expensive press
        // in the game — a 45-turn lockout — and most of what it catches is behind a
        // wall, felt as a dot rather than seen, so *what it bought* is exactly the kind
        // of fact the board cannot show and the near line must. It says the count, not
        // the reach: how far it went is what the flash paints, this turn, over the very
        // box it fired with.
        Event::ConfusionFired { caught: 1, .. } => ("the blast dazes a guard".to_string(), 0),
        Event::ConfusionFired { caught, .. } => (format!("the blast dazes {caught} guards"), 0),
        // The refusal (§8.3/#325): free, changed nothing, and — like the refused bore
        // beside it — has to say why, because a press that silently did nothing reads
        // as a dropped key. It names the rule the player is learning: the blast only
        // reaches what you can already sense.
        Event::ConfusionMissed => ("nothing near enough to daze".to_string(), 0),
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
/// *visible* rather than written-but-unseen (§2.3). Failing all that, the objective:
/// what the run's intel gate still wants, or — once it is met — the exit (§4.5/#310).
/// Never a bare tally of consoles: what is still *out* is not what is still *needed*.
fn ambient(state: &State) -> Message {
    let (text, category) = if state.stunned() > 0 {
        // Stunned (§8.3/#329) outranks every other ambient fact, because it is the
        // only one that removes the decision entirely: there is nothing to weigh
        // until the count reaches zero. Derived straight off the counter, so the line
        // and the rule cannot disagree and there is nothing to clear (§11.4).
        let turns = state.stunned();
        (
            format!(
                "stunned — {turns} more {}",
                if turns == 1 { "turn" } else { "turns" }
            ),
            Category::Warning,
        )
    } else if state.hidden() {
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
    } else if !state.exit_ready() {
        // The gate is not met: state the **requirement**, not the tally. Under
        // `AtLeastOne` with three consoles out, three are remaining but only one is
        // needed, and a bare "intel remaining: 3" implies the wrong goal (#310).
        (
            format!("{} more intel to leave", state.intel_needed_to_exit()),
            Category::Interest,
        )
    } else if state.intel_in_hand() == 0 {
        // The gate is met with nothing in hand — [`IntelGate::None`] (§14 v3's campaign,
        // where intel is currency, §2.2) or a facility with no consoles at all. Say the
        // exit is open and claim nothing about what is held (#310).
        ("the exit is open".to_string(), Category::Interest)
    } else if state.objectives_remaining() == 0 {
        (
            "all intel in hand — reach the exit".to_string(),
            Category::Interest,
        )
    } else {
        // The gate is met; anything still out is optional extra.
        (
            "intel in hand — reach the exit".to_string(),
            Category::Interest,
        )
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
    use crate::ability::AbilityId;
    use crate::cell::{Cell, Direction};
    use crate::facility::Terrain;
    use crate::guard::Guard;
    use crate::modifiers::{IntelGate, LevelModifiers};
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
        assert_eq!(line.text, "all the intel — the exit is open");
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
        assert_eq!(near_line(&s).text, "1 more intel to leave");

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
        // No cover adjacent, so waiting narrates nothing and the floor shows. This
        // facility has no consoles: the exit is open with nothing in hand, and the line
        // says exactly that rather than claiming intel the player never took (#310).
        s.step(Input::Wait);
        assert_eq!(near_line(&s).text, "the exit is open");
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
        s.step(Input::Step(Direction::North)); // climb out onto the body
        s.step(Input::Step(Direction::East)); // step off — take hold: the message turn
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
            at: Cell::new(3, 3),
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

    /// §7.7/§11.7: killing the radio net at the comms console is **objective
    /// feedback**, not a threat message — the Interest band, on the same rung as
    /// taking intel (20), well above every guard event. That rung is the point: the
    /// bump raises exactly one question — *did that work?* — and a detection or a
    /// found body landing the same turn must not bury the answer.
    #[test]
    fn killing_the_radio_net_reads_as_objective_feedback() {
        let msg = message_for(Event::CommsSilenced {
            at: Cell::new(3, 3),
        })
        .expect("silencing the net is never silent");
        assert_eq!(msg.text, "the radio net goes dead");
        assert_eq!(msg.category, Category::Interest);
        assert_eq!(msg.priority, 20);

        // Louder than the loudest thing the net itself could have said.
        let alert = message_for(Event::AlertRaised { level: 9 }).expect("an alert step speaks");
        assert!(
            msg.priority > alert.priority,
            "the answer to \"did that work?\" outranks a guard event on the same turn",
        );
    }

    /// §7.7/§11.7: the two call-ins sit on the same Warning band, at the rungs
    /// their content earns. A **sighting** call reports *your* position and reads
    /// with the radio silence (3). A **body** call reports the *body's* — never the
    /// player's, who may be nowhere near — and outranks the bare find (4), because
    /// it says everything the find says and adds that guards are on their way. Both
    /// stay under the facility-wide alert step (5→) and being caught.
    #[test]
    fn the_call_ins_read_on_the_threat_ladder() {
        let at = Cell::new(3, 3);

        let sighting = message_for(Event::CalledIn { at }).expect("a call-in speaks");
        assert_eq!(sighting.text, "your position was called in");
        assert_eq!(sighting.category, Category::Warning);
        assert_eq!(sighting.priority, 3);

        let found = message_for(Event::BodyFound { at }).expect("a find speaks");
        let body = message_for(Event::BodyCalledIn { at }).expect("a body call speaks");
        assert_eq!(body.text, "a body has been reported");
        assert_eq!(body.category, Category::Warning);
        assert!(
            body.priority > found.priority,
            "a reported body outranks a merely found one ({} vs {})",
            body.priority,
            found.priority,
        );
        assert!(
            body.priority
                <= message_for(Event::AlertRaised { level: 1 })
                    .expect("an alert speaks")
                    .priority,
            "…but never over the facility-wide alert",
        );
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

    /// §8.3/#329: the safety eject is a Warning-band message ranked above every
    /// guard event short of the capture — the player has been moved somewhere they
    /// did not choose and cannot act, and neither fact may be buried by a detection
    /// landing the same turn. It names the **tech**, never the terrain: the same
    /// eject fires out of a table or a shut door, so "the wall" would be a lie in
    /// most of the cases it covers.
    #[test]
    fn the_eject_outranks_every_guard_event_but_the_capture() {
        let msg = message_for(Event::Ejected {
            from: Cell::new(3, 4),
            to: Cell::new(3, 3),
            stunned: crate::phase_eject_stun(1),
        })
        .expect("the safety eject is never silent");
        assert_eq!(msg.text, "safety eject — stunned");
        assert_eq!(msg.category, Category::Warning);

        let alert = message_for(Event::AlertRaised { level: 3 }).expect("an alert speaks");
        let caught = message_for(Event::Captured {
            by: Cell::new(3, 3),
        })
        .expect("the capture speaks");
        assert!(
            msg.priority > alert.priority,
            "being thrown clear outranks the facility alert ({} vs {})",
            msg.priority,
            alert.priority,
        );
        assert!(
            msg.priority < caught.priority,
            "…and never outranks the run ending",
        );
    }

    /// §8.3/§11.4: the stun is a *standing state*, so it lives on the ambient floor
    /// and counts down there — derived straight off the counter, above every other
    /// ambient fact because it is the one that removes the decision entirely.
    #[test]
    fn ambient_counts_the_stun_down() {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 4), Terrain::Wall);
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(10, 10),
        )
        .with_loadout(crate::Loadout::innate().with(crate::AbilityId::Dephase));
        s.step(Input::Activate(crate::AbilityId::Dephase));
        s.step(Input::Step(Direction::East)); // into the wall
        s.step(Input::Wait); // the duration ends: thrown clear and stunned

        // The eject's own turn speaks the message; the floor carries the rest.
        assert_eq!(near_line(&s).text, "safety eject — stunned");
        assert_eq!(s.stunned(), 2);

        s.step(Input::Wait); // swallowed, and nothing else is live
        let line = near_line(&s);
        assert_eq!(line.text, "stunned — 1 more turn");
        assert_eq!(line.category, Category::Warning);
        assert_eq!(line.priority, i32::MIN, "the floor, not a message");

        s.step(Input::Wait); // the last owed turn
        assert_eq!(s.stunned(), 0);
        assert_ne!(
            near_line(&s).text,
            "stunned — 0 more turns",
            "the floor stops reporting a stun that is paid off",
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

    /// A facility with three consoles ringing the player and the exit one step
    /// south, gated by `gate` (§4.5/#244). Bumping north, east and west takes the
    /// three intel in turn — a bump moves nobody — and bumping south answers the
    /// exit.
    fn gated(gate: IntelGate) -> State {
        State::new(
            open_room(12, 12),
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            [Cell::new(5, 4), Cell::new(6, 5), Cell::new(4, 5)],
            Cell::new(5, 6),
        )
        .with_modifiers(LevelModifiers {
            intel_to_exit: gate,
            ..LevelModifiers::default()
        })
    }

    /// The directions that take the three consoles of [`gated`], in order.
    const TAKES: [Direction; 3] = [Direction::North, Direction::East, Direction::West];

    /// Every phrase the near line uses about the intel gate, checked against the
    /// state that produced it: a claim that the exit is open must have
    /// [`State::exit_ready`] behind it, a claim that intel is still owed must not, and
    /// nothing claims intel the player is not holding. This is the #310 invariant — the
    /// objective band derives from the gate, never from a fixed intel count — so a
    /// future gate or rewording that re-breaks it fails here rather than on screen.
    fn assert_objective_messages_are_honest(s: &State) {
        let mut lines: Vec<String> = live_messages(s).into_iter().map(|m| m.text).collect();
        lines.push(near_line(s).text);
        lines.push(ambient(s).text);
        for text in lines {
            if text.contains("the exit is open") || text.contains("reach the exit") {
                assert!(
                    s.exit_ready(),
                    "{text:?} promises an exit that would refuse",
                );
            }
            if text.contains("more intel") || text.contains("more to go") {
                assert!(
                    !s.exit_ready(),
                    "{text:?} asks for intel the gate no longer wants",
                );
            }
            if text.contains("intel in hand") {
                assert!(
                    s.intel_in_hand() > 0,
                    "{text:?} claims intel that is not in hand",
                );
            }
        }
    }

    /// #310, the invariant the bug violated: across **all three gates** and every
    /// amount of intel in hand, no message ever claims the exit is open unless it is —
    /// take messages, refusals and the ambient floor alike.
    #[test]
    fn no_message_promises_an_exit_that_would_refuse() {
        for gate in [IntelGate::None, IntelGate::AtLeastOne, IntelGate::All] {
            for takes in 0..=TAKES.len() {
                let mut s = gated(gate);
                assert_objective_messages_are_honest(&s); // before touching anything
                for dir in TAKES.iter().take(takes) {
                    s.step(Input::Step(*dir));
                    assert_objective_messages_are_honest(&s);
                }
                // Answer the exit: a win if the gate is met, a refusal if it is not —
                // and either way the line that says so has to be true.
                let ready = s.exit_ready();
                let events = s.step(Input::Step(Direction::South));
                assert_eq!(
                    events.iter().any(|e| matches!(e, Event::Won)),
                    ready,
                    "{gate:?} with {takes} taken: the exit answered against its gate",
                );
                assert_objective_messages_are_honest(&s);
            }
        }
    }

    /// #310's headline case: under [`IntelGate::All`] — quick play, every real player's
    /// first run — taking the first of three intel is **progress**, not a green light.
    /// The old message announced an open exit that then refused the walk across the
    /// facility; now the take says what is still owed, and only the last one opens it.
    #[test]
    fn the_all_intel_gate_does_not_announce_the_exit_on_the_first_take() {
        let mut s = gated(IntelGate::All);

        s.step(Input::Step(Direction::North));
        assert_eq!(near_line(&s).text, "intel in hand — 2 more to go");
        s.step(Input::Step(Direction::East));
        assert_eq!(near_line(&s).text, "intel in hand — 1 more to go");

        s.step(Input::Step(Direction::West));
        assert_eq!(
            near_line(&s).text,
            "all the intel — the exit is open",
            "the last take is the one that opens the exit",
        );
        assert!(s.exit_ready());
    }

    /// Under [`IntelGate::AtLeastOne`] the take message is **unchanged** — one intel
    /// does open the exit, and the two still out are optional extra — but the ambient
    /// floor no longer reports the tally as the requirement: three consoles are out,
    /// one is needed, and that is what the line says (#310).
    #[test]
    fn the_at_least_one_gate_states_the_requirement_not_the_tally() {
        let mut s = gated(IntelGate::AtLeastOne);
        s.step(Input::Wait); // a quiet turn: the near line rests on the floor
        assert_eq!(near_line(&s).text, "1 more intel to leave");
        assert_eq!(
            s.objectives_remaining(),
            3,
            "…with three consoles still out"
        );

        s.step(Input::Step(Direction::North));
        assert_eq!(
            near_line(&s).text,
            "intel in hand — the exit is open (2 more out)",
        );
    }

    /// Under [`IntelGate::None`] (§14 v3's campaign, where intel is currency, §2.2) the
    /// exit never refuses, so no message ever asks for intel — and with nothing in hand
    /// the floor says the exit is open without claiming intel the player has not taken.
    #[test]
    fn the_none_gate_never_asks_for_intel() {
        let mut s = gated(IntelGate::None);
        s.step(Input::Wait);
        assert_eq!(near_line(&s).text, "the exit is open");

        let events = s.step(Input::Step(Direction::South));
        assert!(
            events.contains(&Event::Won),
            "the exit accepts empty hands under `None`: {events:?}",
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::ExitRefused { .. })),
            "`None` never refuses",
        );
    }

    /// §4.5/#310: a refusal names the **real** requirement — how many more consoles the
    /// gate wants — so it can never contradict a take message from the same run. The
    /// bug's compounding half: told the exit was open, the player crossed the facility
    /// and was told it "needs intel in hand first" while holding intel.
    #[test]
    fn a_refusal_names_what_the_gate_still_wants() {
        let mut s = gated(IntelGate::All);
        let refused = s.step(Input::Step(Direction::South));
        assert_eq!(refused, vec![Event::ExitRefused { still_needed: 3 }]);
        assert_eq!(near_line(&s).text, "the exit needs 3 more intel");

        s.step(Input::Step(Direction::North)); // one in hand, two still owed
        let take = near_line(&s).text;
        let refused = s.step(Input::Step(Direction::South));
        assert_eq!(refused, vec![Event::ExitRefused { still_needed: 2 }]);
        assert_eq!(near_line(&s).text, "the exit needs 2 more intel");
        assert!(
            !take.contains("the exit is open"),
            "the take and the refusal cannot disagree about the same gate: {take:?}",
        );

        // The baseline gate asks for the one console it needs, not the three that are
        // out — the tally is not the requirement.
        let mut s = gated(IntelGate::AtLeastOne);
        assert_eq!(
            s.step(Input::Step(Direction::South)),
            vec![Event::ExitRefused { still_needed: 1 }],
        );
        assert_eq!(near_line(&s).text, "the exit needs 1 more intel");
    }

    /// §8.2/§4.4-adjacent bookkeeping this ticket must not disturb: a refusal is free
    /// and changes nothing, so a player who bumps the exit early loses no turn to the
    /// corrected message.
    #[test]
    fn a_refusal_still_costs_nothing() {
        let mut s = gated(IntelGate::All);
        s.step(Input::Step(Direction::South));
        assert_eq!(s.turn(), 0, "a refused exit is free (§4.5)");
        assert_eq!(s.player(), Cell::new(5, 5), "and moves nobody");
    }

    /// A **budgeted** ability's activation is **silent** (§8.2/#302). The count it
    /// could narrate is already on the bar, live and permanent, so a message would be
    /// a duplicate paid for with the near line's one row — and a budgeted ability is
    /// typically instant, so there is no "active" window to announce either. An
    /// unbudgeted ability keeps the wording it has always had, which is every other
    /// ability in the catalog.
    #[test]
    fn a_budgeted_activation_says_nothing_and_leaves_the_others_alone() {
        let spoken = |uses_left| {
            message_for(Event::AbilityActivated {
                ability: AbilityId::Dephase,
                uses_left,
            })
        };
        for left in [0, 1, 2, 9] {
            assert_eq!(spoken(Some(left)), None, "the bar already says {left}");
        }
        let unbudgeted = spoken(None).expect("an ordinary activation still speaks");
        assert_eq!(unbudgeted.text, "Dephase active");
        assert_eq!(unbudgeted.category, Category::Owned, "your own tool");
        assert_eq!(unbudgeted.priority, 0, "quiet self-narration, like a bump");
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
