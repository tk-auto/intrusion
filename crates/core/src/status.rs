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

use serde::{Deserialize, Serialize};

use crate::category::Category;
use crate::control::transfers_control;
use crate::state::{Event, State};

mod history;
pub use history::{MessageHistory, HISTORY_ACTIONS};

/// One §11.7 message: what the near line says, the §11.2 category that colours
/// its band, and its rung on the priority ladder. (A source cell joins when
/// modal source-anchored messages land.)
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Message {
    /// The words. Kept short: the near line is one grid row.
    pub text: String,
    /// What the message *means* (§11.2) — the shell colours the band from this.
    pub category: Category,
    /// The §11.7 ladder: routine self-narration ≤ 0, the §7.6 search's own quiet
    /// rung at 1, threat 2 → 4 → 10, objective feedback 20. Ambient status sits
    /// below everything at `i32::MIN` — it is the floor, not a message.
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
        // Flying is silent for [`Event::Moved`]'s reason exactly (§11.7): the machine
        // moved and you watched it move. What is *not* silent is the transfer at either
        // end of it, because that is the fact that changes what the next key does.
        Event::RemoteMoved { .. } => return None,
        Event::Bumped { .. } => ("blocked".to_string(), 0),
        Event::Crouched { .. } => ("you duck behind the table".to_string(), 0),
        Event::EnteredHideout { .. } => ("you slip into the cupboard".to_string(), 0),
        // Which crawlspace matters (§4.5/#466): one is a shortcut you found, the other
        // is the way home, and the near line says the same thing the usable line just
        // said the bump would do (`duct: enter` / `exit: enter`).
        Event::EnteredDuct { own_tunnel, .. } => (
            if own_tunnel {
                "you climb into your own tunnel".to_string()
            } else {
                "you climb into the duct".to_string()
            },
            0,
        ),
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
        // The campaign's power curve, one crate at a time (§2.2/§14 v3/#209). Ranked
        // with the other objective feedback and for the comms console's reason: it is
        // the payoff for a detour the player chose, and *what you now have* must not be
        // buried by a guard event on the same turn. The line names the ability, because
        // the whole find is which ability it was — and the bar has a new row on it this
        // very turn (§11.4), so the two say the same thing at once.
        Event::TechSalvaged { id } => (format!("{} salvaged — it is yours", id.bar_name()), 20),
        // The one refusal a crate has left (§8.3/#209/#266), ranked with the find it is
        // the other side of, so the crate left standing there reads as luck rather than
        // as a dead cell.
        //
        // **The whole crate family speaks in bar names** (§11.4/#266) — `Doors`, not
        // `Autodoors` — and this is the line that forced it: at eleven cells the longest
        // full names put these over the row's budget (`the_crate_lines_fit_the_near_line`).
        // The bar name is the spelling the player is reading two rows down at the very
        // moment the message lands, so it costs nothing to recognise and always fits.
        Event::SalvageRefused { id } => (format!("Already got: {}", id.bar_name()), 20),
        // The one payout a duplicate has (§8.2/#302/#266): the tech is the tech you are
        // already carrying, so what changed is the *number of presses* you have of it.
        // The count rides on the event and is deliberately not printed — the bar shows
        // `Bore(3)` on this very frame, which is where a number belongs (§11.4).
        Event::UsesRecharged { id, .. } => (format!("{} recharged", id.bar_name()), 20),
        // The exchange, all three beats of it (§8.3/#266), ranked with the salvage they
        // are a version of.
        //
        // The offer is the one line on this list that is an **instruction**: the game is
        // waiting on the player, and a near line that only announced the find would leave
        // them pressing keys that no longer do anything. Where to press is the usable
        // line's to say one row down — this says what is on the table and that a choice
        // is owed.
        Event::ExchangeOffered { id } => (format!("{} — drop one for it", id.bar_name()), 20),
        // A trade names **both** halves, in the order they happen to the run: what you
        // gave and what you got. Naming only the prize would make the loss something the
        // player had to notice for themselves on the bar, and it is the more expensive
        // half of the two.
        Event::Traded { taken, dropped } => (
            format!("traded {} for {}", dropped.bar_name(), taken.bar_name()),
            20,
        ),
        // The decline says the crate is still there, because that is the part worth
        // knowing: the tech was not lost, it was left, and it is exactly where it was for
        // a run that comes back having traded that piece away.
        Event::ExchangeDeclined { id } => (format!("{} left in the crate", id.bar_name()), 20),
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
        // The key off the belt (§10.4/#236). Ranked with the objective feedback rather
        // than with the takedown it travels beside, and above it, for the comms
        // console's reason: it is the payoff for a price the player just paid, and *the
        // locked room is open now* is the half of the turn they cannot read off the
        // board — the body they can see lying there.
        Event::KeyTaken { .. } => ("a key — the locked room opens".to_string(), 20),
        // The Saver firing (§4.5/§8.3/#243) — a hand on you, and the run not ending.
        // Ranked at 9: below the endings alone, because for one turn the player has to
        // know that the thing which ends runs just happened *and did not*, and no
        // guard event on the same turn may bury it.
        //
        // It names the **outcome**, not the ability: a player who has just survived the
        // one thing that ends runs needs to read what happened to them, and the bar is
        // already saying which of their things did it (`Saver` → `—`, §11.4). The body
        // is on this line as well as on the takedown travelling beside it, because at
        // priority 9 against that one's 0 this is the only half the near line is
        // guaranteed to speak — and a body you were not told about is the §7.2 cost
        // arriving as a surprise later.
        Event::CaptureSaved { .. } => ("capture evaded — body left".to_string(), 9),
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
        // states the fact, in the same words the tab does ([`alert_line`]) — and, where
        // nothing else on the turn says it, **why** it climbed follows underneath as
        // this event's subordinate message ([`alert_reason`]).
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
        // The control transfer (§8.1/#273), at both ends. It says *what the keys do
        // now*, because that is the only thing about this ability a player can get
        // wrong: pressing a direction and moving the wrong thing. The release also says
        // what you are left holding — the machine is still out there, still watching,
        // and the bar's `[N]` is now counting its life rather than your flight.
        Event::ControlTaken { .. } => ("you have the drone".to_string(), 0),
        Event::ControlReleased { .. } => ("the drone holds — you have your body".to_string(), 0),
        // A launch refused by the crawlspace (§10.7/#273): free, changed nothing, and it
        // has to say why — the rule it enforces is invisible (a body in a duct is a body
        // nothing can reach, so flying from one would cost nothing at all).
        Event::LaunchRefused => ("no room to fly from in here".to_string(), 0),
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
        // A **control-transfer** ability's activation is silent for a third version of
        // the same reason (§8.1/#273): the launch already speaks, through the
        // [`Event::ControlTaken`] it travels with, and that message says strictly more —
        // *the keys are the drone's* rather than *the ability is on*. Two lines for one
        // press would spend the near line's one row on the weaker half (§11.7).
        Event::AbilityActivated { ability, .. } if transfers_control(ability) => return None,
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
        // The §7.6 search, made legible in time (§11.7/#224). Both sit at **1** — a
        // new rung, deliberately below the threat ladder's bottom (a fresh detection
        // at 2) and above routine self-narration: a search opening is the
        // *consequence* of something that has usually already spoken this turn or a
        // few turns back (a lost sighting, a found body, a call-in), so it must never
        // push the louder fact off the one row it has.
        //
        // Neither line names a place, because neither knows one: the events are
        // facility-wide (§11.7 — one call-in puts three guards on one lead), and
        // **where** is what the `show_search_areas` overlay answers (§11.5). The
        // wording follows §11.8 and names the world: guards search, control calls a
        // search off — nothing here mentions a timer or a radius.
        Event::SearchBegan => ("a guard starts searching".to_string(), 1),
        Event::SearchEnded => ("the search is called off".to_string(), 1),
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
        Event::AlertRaised { trigger, .. } => alert_reason(trigger)?.to_string(),
        _ => return None,
    };
    Some(Message {
        text,
        category: headline.category,
        priority: headline.priority,
    })
}

/// **Why** the facility climbed (§7.3/§11.7, #418) — the §7.3 trigger table in the
/// player's own words, or `None` where the turn's own events already say it.
///
/// The raise tells the player the building got worse; a reason tells them what did it,
/// which is the only half of the news they could have acted on. **But half the ladder's
/// triggers fire on the same turn as the very event that reports them**, and there a
/// reason line is that message said twice, one row apart:
///
/// ```text
/// security condition 3 of 3
/// a guard found a body        ← the reason
/// a body has been found       ← Event::BodyFound, the same turn
/// ```
///
/// So a trigger says why **only when nothing else does**. The three that keep a line
/// are the ones the loop reports through the raise alone: a sighting window closing and
/// a console tampered with raise no event of their own that mentions the facility
/// noticing. The three that stay silent are each pushed into the same event vector as
/// their own report, one statement earlier — named in the arms below, so the
/// substitution is checkable rather than asserted.
///
/// It is **total over the enum**, so a seventh trigger cannot ship without an answer
/// either way — it fails the build instead. §11.8 holds as everywhere: these lines name
/// the *world*, never the mechanism.
fn alert_reason(trigger: crate::alert::AlertTrigger) -> Option<&'static str> {
    use crate::alert::AlertTrigger;
    match trigger {
        // The window closing is not a fresh look: the guard has been aware for the
        // whole of it, so `Event::Detected` fired turns ago and cleared (§7.6). What is
        // new is that *control* now knows, and only the raise says so.
        AlertTrigger::Sighting => Some("you were seen"),
        // The count is the shipped **[START]** threshold
        // (`alert::SIGHTINGS_FOR_SECOND_RUNG`), spelled as a word because a bare
        // numeral reads as a tally the player was supposed to have been keeping. A
        // §13.2 sweep may move that threshold; the sim never reads the near line, so
        // the wording follows the shipped ladder.
        AlertTrigger::RepeatSightings => Some("seen three times now"),
        // The take speaks the same turn — but about the *intel*, not about being
        // noticed for it ("intel in hand — 2 more to go"). Two different facts, so this
        // one still needs saying.
        AlertTrigger::ConsoleTampered => Some("they know the intel was touched"),
        // `Event::RadioSilence` — *"a guard has gone silent"* — is pushed immediately
        // before this raise, for the first quiet post and the second alike.
        AlertTrigger::MissedPing | AlertTrigger::SecondPostSilent => None,
        // `Event::BodyFound` — *"a body has been found"* — likewise, one statement
        // earlier in the same vector.
        AlertTrigger::BodyFound => None,
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
    let (text, category) = if let Some(offer) = state.exchange() {
        // **A crate is waiting on an answer** (§8.3/#266), and it outranks every other
        // ambient fact for the reason the stun below it does: there is no other decision
        // to weigh until it is made — the loop takes nothing else (`State::step`). It
        // belongs on the *floor* rather than only in the offer's own message, because it
        // is a standing state and not news: it has to keep saying itself for as long as
        // the question is up, however many keys are pressed at it.
        //
        // It cannot co-occur with the stun underneath: a stun comes only from the phase
        // eject, and a phased run cannot bump at all (§8.3), so it can never open an
        // offer. Placed first anyway, because if the two ever did meet, the question is
        // the one the player has to answer to get moving again.
        //
        // Interest, like the find it is about (§11.2) — and like the offer's own message,
        // so the row does not change colour under the player when the live line ages out.
        (
            format!("{} — drop one for it", offer.offered().bar_name()),
            Category::Interest,
        )
    } else if state.stunned() > 0 {
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
    } else if let Some(duct) = state.occupied_duct() {
        // Inside a crawlspace (§10.7), which is a state exactly like the cupboard above
        // it: concealed, contact-safe, and shaping every next decision while it lasts —
        // here because your sight is gone and only the mouth gives it back (§6.1).
        //
        // It is also **the line the run opens on** (§4.5/#466). Turn one has no action
        // behind it and so no message, and a run that begins inside its own tunnel needs
        // to say so; the floor is where a standing fact belongs, and it keeps saying it
        // for the length of the crawl rather than for one frame. The two ducts are
        // worded apart because they *are* apart: one is a shortcut you found, the other
        // is the way home.
        if duct.way_out().is_some() {
            ("your own tunnel — crawl out".to_string(), Category::Owned)
        } else {
            ("in the duct — memory only".to_string(), Category::Owned)
        }
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
mod tests;
