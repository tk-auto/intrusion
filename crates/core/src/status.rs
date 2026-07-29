//! The near line's message system (§11.7) — what the screen's top row says.
//!
//! The loop reports facts as [`Event`]s; this module turns them into the
//! messages the **near line** (§11.4) shows: each event becomes at most one
//! headline [`Message`] carrying its §11.2 category and its rung on the §11.7
//! priority ladder — and, where the fact has a *why* worth carrying, a
//! **subordinate** message spliced directly under it that never sorts on its own
//! ([`loudest_first`]). [`live_messages`] is the whole set from the *last* step, loudest
//! first — what the near line deploys when more than one is live (§11.7) — and
//! [`near_line`] speaks only its loudest. Messages clear on the player's next
//! action (§11.7), a status line, not a scrollback. When none is live the line
//! does not sit empty: it falls back to [`ambient`] status — the quiet floor
//! below every message — so the row always says something true about now.
//!
//! What the near line has *finished* saying is not thrown away: [`MessageHistory`]
//! keeps the last few message-bearing actions so the **deployed** log can show them
//! under a separator rule (#300). The clear-on-action rule is the near line's, not
//! the panel's.
//!
//! The **usable line** below it is deliberately *not* here: it is no message at
//! all but a pure derived view of adjacency
//! ([`State::affordances`](crate::State::affordances)), recomputed every frame
//! with no plumbing to clear.

use crate::category::Category;
use crate::state::{Event, State};

mod history;
pub use history::{MessageHistory, HISTORY_ACTIONS};

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
    ///
    /// A **subordinate** message ([`subordinate_for`]) wears its headline's priority
    /// and is never sorted by it: [`loudest_first`] orders headlines and splices the
    /// subordinate back underneath, so the two carry the same number because they are
    /// the same fact, not because the ladder has an entry for reasons.
    pub priority: i32,
}

impl Message {
    /// Whether this is the [`ambient`] floor rather than a live message
    /// (§11.4/§11.7) — the one distinction the near line's *band* turns on (#420): an
    /// ambient band paints the quiet fill, a message band the full one.
    ///
    /// Sitting below every message at `i32::MIN` **is** what makes the floor the floor,
    /// so this reads that rather than adding a second way to say the same thing.
    pub fn is_ambient(&self) -> bool {
        self.priority == i32::MIN
    }
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
        // The facility alert climbed a rung (§7.3): the loudest radio event, a
        // facility-wide escalation — above a found body, below being caught. What the
        // rung *does* to the player is the Level info tab's job (#375); the near line
        // states the fact, in the same words the tab does ([`alert_line`]) — and **why**
        // it climbed follows underneath as this event's subordinate message
        // ([`alert_reason`]), which is the half the player could have acted on.
        Event::AlertRaised { rung, .. } => (alert_line(rung), 5),
        // Guards walking in on rung 2 or 3 (§7.3/#374) say **nothing** here, and that
        // is deliberate. The escalation itself already speaks — the `AlertRaised` above
        // fires on the same turn — and what a rung *does* to the player is the Level
        // info tab's job (#375), which owns the ladder's legibility. A line per arrival
        // would also drown the near line three-deep on the turn a run jumps to rung 3,
        // pushing whatever else happened out of a one-row surface (§11.7).
        //
        // The arrival is not thereby hidden: the rung is stated, and the guards
        // themselves show up on the §9.1 sense the moment they come within reach of it,
        // as any guard does.
        Event::ReinforcementArrived { .. } => return None,
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
        // Silent, for [`Event::WallBored`]'s reason exactly (§11.7): the sealed doors
        // are *marked on the board* the moment they seal, and which doors they are is
        // the fact the player is playing off — a count in the near line restates a
        // picture that says it better. The window itself still announces, through the
        // ordinary "Lockdown active" beside it.
        Event::DoorsSealed { .. } => return None,
        // A refused lockdown (§8.3/#242): free, changed nothing, and — like the refused
        // bore and the refused rematerialization beside it — has to say why, or a press
        // that did nothing reads as a dropped key.
        Event::LockdownRefused => ("no door in reach to seal".to_string(), 0),
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

/// The **subordinate message** an event adds under its headline (§11.7/#418), or
/// `None` for the events whose headline says the whole fact — which is nearly all of
/// them, so this stays out of [`message_for`]'s table rather than making every arm
/// carry a `None`.
///
/// It is a second [`Message`], not a second *event*: it wears its headline's category
/// and priority, and [`loudest_first`] splices it back underneath after the ordering is
/// done. That is what makes the pair inseparable. A reason emitted as a free-floating
/// message and left to sort with the rest would be pulled away from its headline —
/// below it by anything at an intermediate priority (a `BodyCalledIn` at 5, a
/// `BodyFound` at 4), and the turn an alert climbs is exactly a turn with other loud
/// events — or, at an equal priority, flipped *above* it by the later-first reversal.
fn subordinate_for(event: Event, headline: &Message) -> Option<Message> {
    let text = match event {
        Event::AlertRaised { trigger, .. } => alert_reason(trigger).to_string(),
        _ => return None,
    };
    Some(Message {
        text,
        category: headline.category,
        priority: headline.priority,
    })
}

/// **Why** the facility climbed (§7.3/§11.7, #418) — the §7.3 trigger table in the
/// player's own words, one line per [`AlertTrigger`](crate::alert::AlertTrigger).
///
/// The raise tells the player the building got worse; this tells them what did it,
/// which is the only half of the news they could have acted on. It is **total over the
/// enum**, so a seventh trigger cannot ship unnamed — it fails the build instead.
///
/// §11.8 holds here as everywhere: these lines name the *world* — a post, a body, a
/// sighting — and never the mechanism.
fn alert_reason(trigger: crate::alert::AlertTrigger) -> &'static str {
    use crate::alert::AlertTrigger;
    match trigger {
        AlertTrigger::Sighting => "you were seen",
        AlertTrigger::MissedPing => "a post stopped answering",
        // The count is the shipped **[START]** threshold
        // (`alert::SIGHTINGS_FOR_SECOND_RUNG`), spelled as a word because a bare
        // numeral reads as a tally the player was supposed to have been keeping. A
        // §13.2 sweep may move that threshold; the sim never reads the near line, so
        // the wording follows the shipped ladder.
        AlertTrigger::RepeatSightings => "seen three times now",
        AlertTrigger::ConsoleTampered => "they know the intel was touched",
        AlertTrigger::BodyFound => "a guard found a body",
        AlertTrigger::SecondPostSilent => "a second post stopped answering",
    }
}

/// Every live message from the player's last action (§11.7), **loudest first** —
/// the full set the deployable near line lists, of which [`near_line`] speaks
/// only the first. Empty when the last action said nothing: the ambient floor is
/// not a message (§11.4) and never joins the list. Ties resolve as the near line
/// does — the later event leads — so the first entry is exactly the band the near
/// line paints, and the two can never disagree.
pub fn live_messages(state: &State) -> Vec<Message> {
    loudest_first(state.last_events())
}

/// One action's events resolved to its messages, **loudest first** — the §11.7
/// ordering itself, with no opinion about *which* action it is given.
///
/// Shared by [`live_messages`] and the [`MessageHistory`] ring behind the deployed
/// log (#300), so a block that has scrolled into the history reads in exactly the
/// order it read in while it was live. Two orderings would have been two chances to
/// disagree about which message is the loudest.
///
/// **Only headlines are ordered.** An event's subordinate message
/// ([`subordinate_for`]) travels with its headline through the sort and is spliced back
/// directly beneath it, so no event landing on the same turn can come between a fact
/// and its reason however loud that event is (#418). The first entry is therefore
/// always a headline, which is what lets [`near_line`] simply take it.
pub(crate) fn loudest_first(events: &[Event]) -> Vec<Message> {
    let mut blocks: Vec<(Message, Option<Message>)> = events
        .iter()
        .filter_map(|&event| {
            let headline = message_for(event)?;
            let subordinate = subordinate_for(event, &headline);
            Some((headline, subordinate))
        })
        .collect();
    // The near line shows the *last* of the top-priority events (`max_by_key`
    // keeps the later of equal keys). Lead the list with that same message:
    // reverse to later-first, then a **stable** descending sort by priority keeps
    // later-first order within each rung.
    blocks.reverse();
    blocks.sort_by_key(|(headline, _)| std::cmp::Reverse(headline.priority));
    blocks
        .into_iter()
        .flat_map(|(headline, subordinate)| std::iter::once(headline).chain(subordinate))
        .collect()
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

/// How the near line names the rung the facility has reached (§7.3/§11.4, #375) —
/// **one phrase, two callers**: the [`Event::AlertRaised`] message that announces a
/// step, and the [`ambient`] floor that keeps stating it for the rest of the level.
/// They were worded separately and read as two different facts.
///
/// The player-facing noun is **condition**, never *rung* — a rung is the shape of the
/// ladder in the code, and naming the mechanism on screen is exactly the un-diegetic
/// tell to avoid. It carries the number and the ceiling ("2 of 3"), which is what the
/// metaphor was buying: how bad it is, and how much worse it can still get.
///
/// It is the same `condition N of 3` the help panel's ALERT section prints
/// (`render::alert::condition_line`), because the panel is where the rung's *effects*
/// are written and a line that sends the player there must name the thing they will
/// find. This one wears **security** in front of it, since the near line has no heading
/// to say what the number counts.
///
/// It is also the rewording the §11.7 width bound was waiting on: *"the facility is on
/// alert — level 3"* was 34 cells in a 29-cell row and reached the screen clipped, so
/// the one message about escalation was the one the player could not read.
fn alert_line(rung: u32) -> String {
    format!("security condition {rung} of {}", crate::alert::TOP_RUNG)
}

/// How many objectives the facility holds in all — taken plus still out. The
/// denominator of the ambient line's fraction (#421).
fn objectives(state: &State) -> usize {
    state.intel_in_hand() + state.objectives_remaining()
}

/// The ambient floor's standing line (§11.4/#421): what you have of what there is, and
/// — once the facility has noticed you — how bad it has got.
///
/// ```text
/// objectives: 1/3                 a quiet facility
/// objectives: 1/3 - security: 2   once the ladder has stepped
/// ```
///
/// **Why a bare tally is honest here, and why #310 does not bite.** #310 forbade the
/// near line reporting the count of consoles still out, on the ground that *what is
/// still out is not what is still needed*: under [`IntelGate::AtLeastOne`] three consoles
/// can be out while exactly one is needed, so a tally implies the wrong goal. But
/// `AtLeastOne` is the **sim's** baseline (§13.3) and the sim never reads the near line.
/// Quick play sets [`IntelGate::All`] (#244), where the tally and the requirement are the
/// same number said differently and `3/3` *is* the exit-open signal; the campaign sets
/// [`IntelGate::None`] (§14 v3), where intel is currency (§2.2) and a progress fraction
/// is exactly the right thing to show. **If a human-facing mode ever ships on
/// `AtLeastOne`, this line reverts** — that is the condition it stays truthful under,
/// and it is written into §11.4 beside it.
///
/// The security half is a **label**, not the [`alert_line`] phrase: *"security condition
/// 2 of 3"* is 24 cells and cannot share a 32-cell row with the objectives. The ceiling
/// it drops is still stated where it changes — the raise announces itself in full, and
/// says why (#418) — and the help panel carries the effects. What a standing row owes
/// the player is the number, every turn, without spending the row on it.
///
/// [`IntelGate::All`]: crate::IntelGate::All
/// [`IntelGate::None`]: crate::IntelGate::None
/// [`IntelGate::AtLeastOne`]: crate::IntelGate::AtLeastOne
fn objective_and_security(taken: usize, total: usize, alert: u32) -> String {
    let objectives = format!("objectives: {taken}/{total}");
    if alert == 0 {
        objectives
    } else {
        format!("{objectives} - security: {alert}")
    }
}

/// The ambient floor (§11.4): the quiet status the near line rests on between
/// messages, so it never sits empty.
///
/// **The momentary states come first** — stunned, hidden, crouched, dragging. Each is a
/// state the player is *in* rather than a fact about the run, each ends, and while one
/// lasts it is the thing shaping the next decision (and the Owned band matches the
/// recoloured cupboard or table, §10.3). They own the line for as long as they hold.
///
/// **Underneath them is the standing pair** ([`objective_and_security`], #421): what you
/// have of what there is, and how bad the facility has got. Neither expires and the
/// player needs both, so the floor carries them together rather than choosing — which is
/// what it used to do, dropping the objective the moment the ladder stepped and never
/// mentioning the facility before it did.
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
    } else {
        // **Both standing facts, in one row** (§11.4/#421). Neither of them expires and
        // the player needs both, so the line no longer chooses between them: it used to
        // state the alert and drop the objective at rung ≥ 1, and state the objective
        // and leave the facility unmentioned at rung 0.
        //
        // The band follows the rung — Interest while the facility has not noticed you,
        // the §7.3 ladder's own colour once it has — which is what makes this row the
        // always-visible alert indicator (#420: an ambient band paints the quiet fill,
        // so a standing condition-3 row never spends the danger overlay's own shade).
        let alert = state.alert();
        (
            objective_and_security(state.intel_in_hand(), objectives(state), alert),
            if alert > 0 {
                crate::alert::rung_category(alert)
            } else {
                Category::Interest
            },
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
    use crate::alert::AlertTrigger;
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
        assert_eq!(near_line(&s).text, "objectives: 0/1");

        let mut s = state(Cell::new(3, 4), Cell::new(3, 3));
        s.step(Input::Step(Direction::North)); // take the intel: a loud message
        assert_eq!(near_line(&s).priority, 20);
        s.step(Input::Step(Direction::South)); // next action: the message clears
        let line = near_line(&s);
        assert_eq!(line.text, "objectives: 1/1");
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
        // facility has no consoles at all, so the fraction is honest about that too
        // rather than claiming intel the player never took (#310).
        s.step(Input::Wait);
        assert_eq!(near_line(&s).text, "objectives: 0/0");
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

    /// #421, the whole point of the row: **both standing facts, together.** At rung 0
    /// the objective alone; once the ladder has stepped, the objective *and* the
    /// condition — neither expires, and the player needs both.
    #[test]
    fn the_ambient_floor_carries_the_objective_and_the_security_together() {
        assert_eq!(objective_and_security(1, 3, 0), "objectives: 1/3");
        assert_eq!(
            objective_and_security(1, 3, 2),
            "objectives: 1/3 - security: 2",
        );
        // Every objective taken: the fraction *is* the exit-open signal under the gate
        // quick play ships on (§4.5/#244), so the case is not silently lost.
        assert_eq!(objective_and_security(3, 3, 0), "objectives: 3/3");
        assert_eq!(
            objective_and_security(3, 3, crate::alert::TOP_RUNG),
            "objectives: 3/3 - security: 3",
        );
        // A facility with no consoles at all still says something true.
        assert_eq!(objective_and_security(0, 0, 0), "objectives: 0/0");
    }

    /// #421: the band **is** the alert indicator. Interest while the facility has not
    /// noticed you, and the §7.3 ladder's own colour once it has — read off
    /// [`rung_category`](crate::alert::rung_category), the same declaration the help
    /// panel's condition line reads, so the row and the panel can never disagree.
    #[test]
    fn the_ambient_band_follows_the_rung() {
        let mut s = state(Cell::new(5, 6), Cell::new(3, 3));
        s.step(Input::Wait);
        assert_eq!(s.alert(), 0);
        assert_eq!(near_line(&s).category, Category::Interest);

        // The mapping itself, over every rung the ladder can reach — the arm this row
        // takes is `rung_category` and nothing hand-written beside it.
        for rung in 1..=crate::alert::TOP_RUNG {
            assert_ne!(
                crate::alert::rung_category(rung),
                Category::Interest,
                "a raised facility never wears the quiet objective colour",
            );
        }
    }

    /// §11.4/#421: **both forms fit the row**, measured against the capacity the layout
    /// computes rather than a number written down beside it — worst case, with the
    /// deploy control up, which it can be while the ambient floor shows (the history
    /// outlives the action that cleared the near line).
    ///
    /// Walked over two-digit counts as well, since `objectives: 10/12 - security: 3` is
    /// the longest the line can get and is the one that would clip first.
    #[test]
    fn both_ambient_forms_fit_the_near_line() {
        let max = crate::render::near_line_text_max(crate::LevelConfig::V1.width);
        for total in [0, 3, 9, 12, 99] {
            for taken in [0, total] {
                for alert in 0..=crate::alert::TOP_RUNG {
                    let line = objective_and_security(taken, total, alert);
                    let len = line.chars().count();
                    assert!(
                        len <= max,
                        "{line:?} is {len} cells, over the {max} the near line leaves \
                         beside its controls (§11.4)",
                    );
                }
            }
        }
    }

    /// §11.4/#421: the momentary states still **pre-empt** the standing pair, and every
    /// live message still pre-empts all of them. The floor gained a second fact; it did
    /// not gain a claim on the row.
    #[test]
    fn the_momentary_states_still_pre_empt_the_standing_pair() {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 6),
            Direction::North,
            Vec::new(),
            [Cell::new(8, 8)],
            Cell::new(10, 10),
        );
        s.step(Input::Step(Direction::North)); // into the cupboard
        s.step(Input::Wait); // the entry message clears: hidden is the floor's arm
        assert_eq!(near_line(&s).text, "hidden — the cupboard conceals you");
        assert!(
            !near_line(&s).text.contains("objectives"),
            "a momentary state owns the row while it lasts",
        );

        // And a live message outranks even that — the floor is the floor (§11.7).
        s.step(Input::Step(Direction::South)); // out of the cupboard
        assert!(!near_line(&s).is_ambient() || near_line(&s).text.contains("objectives"));
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

        let alert = message_for(Event::AlertRaised {
            rung: 2,
            trigger: AlertTrigger::RepeatSightings,
        })
        .expect("an alert step speaks");
        assert_eq!(alert.text, "security condition 2 of 3");
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
        let alert = message_for(Event::AlertRaised {
            rung: 3,
            trigger: AlertTrigger::BodyFound,
        })
        .expect("an alert step speaks");
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
                <= message_for(Event::AlertRaised {
                    rung: 1,
                    trigger: AlertTrigger::Sighting,
                })
                .expect("an alert speaks")
                .priority,
            "…but never over the facility-wide alert",
        );
    }

    /// §11.8: **the near line never says "rung"** — the ladder's steps are *conditions*
    /// on screen, because a rung names the shape of the system and the player is reading
    /// this without the design doc beside them. Walked over every rung the ladder can
    /// reach, so a later reword cannot let the mechanism's name back out; the help
    /// panel's half of the same pair is pinned in `render::alert`.
    #[test]
    fn the_near_line_speaks_conditions_not_rungs() {
        for rung in 1..=crate::alert::TOP_RUNG {
            let line = alert_line(rung);
            assert!(
                !line.to_lowercase().contains("rung"),
                "{line:?} says rung — the design's word, not the player's (§11.8)",
            );
            assert_eq!(line, format!("security condition {rung} of 3"));
        }
        // The same trip-wire over the reason lines (#418): they are the other half of
        // the same fact, and a mechanism word smuggled in underneath the headline is no
        // better than one in it. `trigger` is the design's word too, and so is the
        // ladder's own name.
        for trigger in AlertTrigger::ALL {
            let reason = alert_reason(trigger).to_lowercase();
            for word in ["rung", "trigger", "ladder", "alert level"] {
                assert!(
                    !reason.contains(word),
                    "{reason:?} says {word:?} — the design's word, not the player's (§11.8)",
                );
            }
        }
    }

    /// §7.3/§11.4: once the radio has stepped the facility alert, the value is
    /// *readable* — with no louder message live, the ambient floor surfaces it beside
    /// the objectives, never written-but-invisible (§2.3), and the band takes the rung's
    /// own colour (#421), which is what makes this row the standing alert indicator.
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

        // No message is live after the quiet waits: the near line rests on both standing
        // facts, and the band is the rung's own colour — the same mapping the help
        // panel's condition line reads (#375), so the two can never disagree.
        let line = near_line(&s);
        assert_eq!(line.text, "objectives: 0/0 - security: 1");
        assert_eq!(line.category, crate::alert::rung_category(s.alert()));
        assert_eq!(line.category, Category::Caution, "rung 1 is the low rung");
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

        let alert = message_for(Event::AlertRaised {
            rung: 3,
            trigger: AlertTrigger::BodyFound,
        })
        .expect("an alert speaks");
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

    /// §11.7: a single step can raise more than one message — a takedown seen by a
    /// second guard is `TakenDown`, `BodyFound` *and* the `AlertRaised` the find sends
    /// up the §7.3 ladder — and [`live_messages`] returns them all, **loudest first**,
    /// leading with exactly what [`near_line`] speaks so the deployed list and the
    /// band never disagree.
    #[test]
    fn live_messages_lists_the_whole_step_loudest_first() {
        // A hidden strike north on an adjacent victim, with a witness two cells up
        // whose cone covers the fresh body: the same turn yields the takedown
        // (self-narration, priority 0), the found body (a threat, priority 4) and the
        // facility-wide escalation it causes (priority 5).
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
            vec![5, 5, 4, 0],
            "loudest first: the escalation and its reason, the found body, the narration",
        );
        assert_eq!(live[0].text, "security condition 3 of 3");
        assert_eq!(
            live[1].text, "a guard found a body",
            "the reason, riding under it"
        );
        assert_eq!(live[2].text, "a body has been found");
        assert_eq!(live[3].text, "the guard drops — a body is left");
        assert_eq!(
            live.first().cloned(),
            Some(near_line(&s)),
            "the list leads with exactly the near line's band",
        );
    }

    /// §7.3/§11.7/#418: **every** trigger names itself. Walked over
    /// [`AlertTrigger::ALL`], so a seventh trigger shipping without a reason fails here
    /// — and the reason follows its own headline immediately, at the same priority and
    /// in the same category, because the two are one fact.
    #[test]
    fn every_alert_trigger_says_what_raised_it() {
        for trigger in AlertTrigger::ALL {
            let rung = trigger.rung();
            let pair = loudest_first(&[Event::AlertRaised { rung, trigger }]);
            assert_eq!(
                pair.len(),
                2,
                "{trigger:?} raised a headline with no reason under it",
            );
            assert_eq!(pair[0].text, alert_line(rung), "{trigger:?}");
            assert_eq!(pair[1].text, alert_reason(trigger), "{trigger:?}");
            assert!(
                !pair[1].text.is_empty(),
                "{trigger:?} has an empty reason line",
            );
            assert_eq!(pair[1].category, pair[0].category, "{trigger:?}: one fact");
            assert_eq!(pair[1].priority, pair[0].priority, "{trigger:?}: one fact");
        }
        // The six of them are six *different* lines: a reason shared by two triggers
        // would tell the player the wrong thing about one of them.
        let reasons: std::collections::BTreeSet<&str> =
            AlertTrigger::ALL.iter().map(|&t| alert_reason(t)).collect();
        assert_eq!(reasons.len(), AlertTrigger::ALL.len(), "{reasons:?}");
    }

    /// #418's whole reason for the splice: **nothing gets between a fact and its
    /// reason**. The turn an alert climbs is exactly a turn with other loud events, and
    /// they sort around the pair — never through it — however loud they are.
    #[test]
    fn no_event_can_come_between_the_raise_and_its_reason() {
        let at = Cell::new(3, 3);
        // Everything the same turn could plausibly raise, on both sides of the raise's
        // own rung: the capture (10), the eject (6), a body call (5, the raise's equal),
        // a found body (4) and the takedown's self-narration (0).
        let events = [
            Event::TakenDown { at },
            Event::BodyFound { at },
            Event::AlertRaised {
                rung: 3,
                trigger: AlertTrigger::BodyFound,
            },
            Event::BodyCalledIn { at },
            Event::Ejected {
                from: at,
                to: Cell::new(3, 4),
                stunned: crate::phase_eject_stun(1),
            },
            Event::Captured { by: at },
        ];
        let texts: Vec<String> = loudest_first(&events).into_iter().map(|m| m.text).collect();
        let headline = texts
            .iter()
            .position(|t| t == "security condition 3 of 3")
            .expect("the raise speaks");
        assert_eq!(
            texts.get(headline + 1).map(String::as_str),
            Some("a guard found a body"),
            "the reason follows its headline immediately: {texts:?}",
        );

        // …and it holds for every rotation of the same turn's events, so the guarantee
        // is the splice and not the order the loop happened to report them in.
        for shift in 0..events.len() {
            let mut rotated = events;
            rotated.rotate_left(shift);
            let texts: Vec<String> = loudest_first(&rotated)
                .into_iter()
                .map(|m| m.text)
                .collect();
            let i = texts
                .iter()
                .position(|t| t == "security condition 3 of 3")
                .expect("the raise speaks");
            assert_eq!(
                texts.get(i + 1).map(String::as_str),
                Some("a guard found a body"),
                "rotation {shift}: {texts:?}",
            );
        }
    }

    /// §11.4/#418: the **near line is unchanged** by the reason. It is one grid row and
    /// it speaks the loudest message only — the condition, in the words it always used.
    #[test]
    fn the_near_line_still_speaks_the_condition_alone() {
        let mut s = state(Cell::new(5, 6), Cell::new(3, 3));
        s.step(Input::Wait);
        let quiet = near_line(&s).text;

        for trigger in AlertTrigger::ALL {
            let rung = trigger.rung();
            let line = loudest_first(&[Event::AlertRaised { rung, trigger }])
                .into_iter()
                .next()
                .expect("the raise speaks");
            assert_eq!(line.text, alert_line(rung), "{trigger:?}");
            assert_eq!(line.category, Category::Warning);
            assert_eq!(line.priority, 5);
        }
        assert_eq!(near_line(&s).text, quiet, "and a quiet turn is untouched");
    }

    /// #418: the pair survives into [`MessageHistory`] and reads there in the order it
    /// read live — the record and the near line cannot tell different stories about the
    /// same turn.
    #[test]
    fn the_reason_follows_its_headline_into_the_history() {
        let at = Cell::new(4, 4);
        let events = [
            Event::BodyFound { at },
            Event::AlertRaised {
                rung: 3,
                trigger: AlertTrigger::BodyFound,
            },
        ];
        let mut history = MessageHistory::default();
        history.record(&events);

        let filed: Vec<String> = history
            .blocks()
            .next()
            .expect("a loud action is filed")
            .iter()
            .map(|m| m.text.clone())
            .collect();
        let live: Vec<String> = loudest_first(&events).into_iter().map(|m| m.text).collect();
        assert_eq!(filed, live, "remembered exactly as it read live");
        assert_eq!(
            filed,
            vec![
                "security condition 3 of 3".to_string(),
                "a guard found a body".to_string(),
                "a body has been found".to_string(),
            ],
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

    /// Under [`IntelGate::AtLeastOne`] the **take message** is unchanged — one intel does
    /// open the exit, and the two still out are optional extra. This is the gate the
    /// ambient fraction would misread (#310/#421): it says `objectives: 1/3` while the
    /// exit is already open, because the fraction reports progress and not the gate.
    ///
    /// That is why the line is only truthful under the gates a **human** actually plays:
    /// `AtLeastOne` is the sim's baseline (§13.3) and the sim never reads the near line.
    /// Pinned here so the day a human-facing mode ships on this gate, the mismatch is
    /// already written down rather than discovered on screen.
    #[test]
    fn the_at_least_one_gate_is_the_one_the_ambient_fraction_cannot_speak_for() {
        let mut s = gated(IntelGate::AtLeastOne);
        s.step(Input::Wait); // a quiet turn: the near line rests on the floor
        assert_eq!(near_line(&s).text, "objectives: 0/3");
        assert_eq!(
            s.objectives_remaining(),
            3,
            "…with three consoles still out"
        );

        s.step(Input::Step(Direction::North));
        assert_eq!(
            near_line(&s).text,
            "intel in hand — the exit is open (2 more out)",
            "the take still says the gate's own answer",
        );
        s.step(Input::Wait); // the message clears back to the floor
        assert!(s.exit_ready(), "one intel opened the exit under this gate");
        assert_eq!(
            near_line(&s).text,
            "objectives: 1/3",
            "…and the floor reports progress, which under this gate is not the gate",
        );
    }

    /// Under [`IntelGate::None`] (§14 v3's campaign, where intel is currency, §2.2) the
    /// exit never refuses, so no message ever asks for intel — and with nothing in hand
    /// the floor says the exit is open without claiming intel the player has not taken.
    #[test]
    fn the_none_gate_never_asks_for_intel() {
        let mut s = gated(IntelGate::None);
        s.step(Input::Wait);
        // Intel is currency here (§2.2), never an exit key, so a progress fraction is
        // exactly the right thing for the floor to show (#421).
        assert_eq!(near_line(&s).text, "objectives: 0/3");

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
        assert_eq!(unbudgeted.text, "Phase Out active");
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
