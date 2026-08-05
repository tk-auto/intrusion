//! **False Call** through the turn loop (§7.7/§8.3, #504).
//!
//! The ability is a small change to the guard side and mostly a new §8.3 row, and that
//! is exactly the claim these tests exist to hold. §7.7 says cooperation has **one**
//! verb — *a call sends guards to search a cell* — and this ability does not add a
//! second: it hands the player the one that exists. So what is pinned here is less the
//! ability's own machinery than the properties it inherits, and the two it adds.
//!
//! - **It is the same call.** The responders **search**; a guard that has the live
//!   player is never pulled off it; a guard already on an errand is redirected like any
//!   other free one; and the facility alert never steps, because nothing was seen and
//!   no ping was missed.
//! - **It is a vacuum, not a trap.** The cell is a snapshot taken at the firing, so
//!   walking away does not take the search with you — which is the whole play, and the
//!   whole way to be caught.
//! - **The reach is the transmitter's, not the eyes'.** It is a radio, so it is not
//!   clamped by the guard sense the way a blast is — which a duct, a Wait and a guard
//!   past the sense each pin from a different side.
//! - **It dies with the net** (§7.3). The console's trade, refused at the press rather
//!   than discovered as a turn that bought nothing.

use crate::guard::GuardState;
use crate::state::*;
use crate::test_support::open_room;

/// A player holding False Call in the middle of a big open room, with `guards` posted
/// as given. Big enough that the whole [`FALSE_CALL_RADIUS`] box fits around the
/// player, so what a firing does or does not reach is the ability's doing and never the
/// wall's.
fn spoofer(guards: Vec<Guard>) -> State {
    State::new(
        open_room(40, 40),
        Cell::new(20, 20),
        Direction::East,
        guards,
        Vec::new(),
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::FalseCall))
}

/// Fire the call, spending the turn (§4.4), and report what answered.
fn fire(state: &mut State) -> u32 {
    let events = state.step(Input::Activate(AbilityId::FalseCall));
    events
        .iter()
        .find_map(|e| match e {
            Event::FalseCallFired { answered, .. } => Some(*answered),
            _ => None,
        })
        .expect("the call went out")
}

/// The reach the last firing broadcast over, straight off the event.
fn last_reach(state: &State) -> EffectArea {
    state
        .last_events()
        .iter()
        .find_map(|e| match e {
            Event::FalseCallFired { reach, .. } => Some(*reach),
            _ => None,
        })
        .expect("the call went out")
}

// ---------------------------------------------------------------------------
// It is the same call §7.7 already had
// ---------------------------------------------------------------------------

/// §7.7/§7.6: a called guard **searches** the cell it was sent to. It answers as
/// `Responding`, walks the whole way, and opens the ordinary §7.6 search there —
/// which is what a call means and the only thing it means. Nothing here chases: the
/// call carries no information about where the player is *now*, and being seen on the
/// way is what would start a chase, by the ordinary rules.
#[test]
fn a_called_guard_walks_to_the_cell_and_searches_it() {
    // The **duct** fixture (§10.7), because this ability makes its own test awkward: the
    // cell it names is the cell the player is standing in, so a responder that walks the
    // whole way walks onto them. The crawlspace is the way out that leaves the called
    // cell walkable — fire from the mouth, climb into the wall, and watch the search
    // arrive at the floor you left. That is the play the ability is *for*, so the
    // fixture is the mechanic rather than a contrivance around it.
    //
    // The guard's beat is the room's south strip so it is facing away at the firing;
    // otherwise it simply sees the player across an open 9×9 room and this measures a
    // chase instead of a call.
    let post = Cell::new(2, 2);
    let mut s = super::ducts::duct_world_with(vec![Guard::patrolling(Cell::new(5, 6))
        .with_beat(vec![Cell::new(5, 7), Cell::new(6, 7), Cell::new(7, 7)])])
    .with_loadout(Loadout::innate().with(AbilityId::FalseCall));
    s.set_guard_close_chance(0);
    assert_eq!(s.player(), post, "precondition: standing on the duct mouth");

    assert_eq!(fire(&mut s), 1, "the one guard in reach answered");
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Responding,
        "it answered the call, it did not spot anybody",
    );
    assert_eq!(
        s.guards()[0].destination(),
        Some(post),
        "and it is walking to the cell the call named",
    );

    s.step(Input::Step(Direction::North)); // into the wall, out of the way
    assert!(s.in_duct(), "the player is gone; the search is not");
    for _ in 0..40 {
        if s.guards()[0].state() != GuardState::Responding {
            break;
        }
        s.step(Input::Wait);
    }
    assert_eq!(
        s.guards()[0].pos(),
        post,
        "the responder walked the whole way (§7.7)",
    );
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Alerted,
        "and opened the §7.6 search the call was for — it never chased",
    );
    assert_eq!(
        s.guards()[0].focus(),
        Some(post),
        "centred on the cell the forged call named",
    );
}

/// §7.4/§7.7: a guard that **has the live player** is never pulled off it, by the very
/// filter every other call is subject to ([`radio::nearest_respondable`]). The forged
/// call gets no privilege the facility's own calls do not have — so a chaser standing
/// well inside the reach ignores it, and the call reports the honest zero rather than
/// claiming a responder it did not get.
#[test]
fn a_chaser_is_never_called_off_the_player() {
    let mut s = spoofer(vec![
        Guard::stationary(Cell::new(20, 24)).with_state(GuardState::Chasing)
    ]);
    let reach = s.false_call_area();
    assert!(
        reach.contains(Cell::new(20, 24)),
        "precondition: the chaser is inside the reach",
    );

    assert_eq!(fire(&mut s), 0, "nobody was free to answer");
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Chasing,
        "the one guard that has you keeps you (§7.4)",
    );
}

/// §7.7/#504: a guard already **on an errand** is redirected. `Responding` is a free
/// state — it is not the Danger band — so the spoofer takes it off the cell control
/// sent it to and puts it on the cell the player named. The stronger tool, and the
/// more surprising one, pinned so it cannot drift into a silent no-op.
#[test]
fn a_guard_already_on_an_errand_is_redirected() {
    let elsewhere = Cell::new(4, 4);
    let mut s = spoofer(vec![Guard::stationary(Cell::new(20, 26))]);
    assert!(
        s.call_guards_to_for_test(elsewhere, 1),
        "precondition: control sent it somewhere first",
    );
    assert_eq!(s.guards()[0].destination(), Some(elsewhere));

    assert_eq!(fire(&mut s), 1, "it answered the forged call too");
    assert_eq!(
        s.guards()[0].destination(),
        Some(Cell::new(20, 20)),
        "and the errand it is now running is the player's (§7.7)",
    );
}

/// §7.3: the facility alert does **not** step. Nothing was seen and no ping was
/// missed, so a forged call is — to the facility — an ordinary call, and a run cannot
/// climb the ladder by a side door. Asserted over a firing that pulls a crowd, which
/// is the case a rung would be most tempting for.
#[test]
fn a_forged_call_never_steps_the_alert() {
    let mut s = spoofer(vec![
        Guard::stationary(Cell::new(20, 24)),
        Guard::stationary(Cell::new(24, 20)),
        Guard::stationary(Cell::new(16, 20)),
    ]);
    let before = s.alert();

    let events = s.step(Input::Activate(AbilityId::FalseCall));
    assert_eq!(fired_count(&events), 3, "all three answered");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::AlertRaised { .. })),
        "a forged call is not an escalation (§7.3)",
    );
    assert_eq!(s.alert(), before, "and the rung is where it was");
}

/// How many guards the `FalseCallFired` in `events` says answered.
fn fired_count(events: &[Event]) -> u32 {
    events
        .iter()
        .find_map(|e| match e {
            Event::FalseCallFired { answered, .. } => Some(*answered),
            _ => None,
        })
        .expect("the call went out")
}

// ---------------------------------------------------------------------------
// A vacuum, not a trap
// ---------------------------------------------------------------------------

/// §7.7/§8.3: the cell is a **snapshot**. Walking away after firing does not take the
/// search with you — the responders keep coming to where you *were*, which is the
/// whole ability and, stood still on, the whole way to be caught by it.
#[test]
fn the_called_cell_is_where_you_fired_not_where_you_are() {
    let fired_from = Cell::new(20, 20);
    let mut s = spoofer(vec![Guard::stationary(Cell::new(20, 26))]);
    assert_eq!(fire(&mut s), 1);

    for _ in 0..5 {
        s.step(Input::Step(Direction::West));
    }
    assert_eq!(s.player(), Cell::new(15, 20), "the player has left");
    assert_eq!(
        s.guards()[0].destination(),
        Some(fired_from),
        "and the search is still bearing down on the cell that was named",
    );
}

// ---------------------------------------------------------------------------
// The reach is the player's picture
// ---------------------------------------------------------------------------

/// The ability's own numbers, pinned so a retune is a visible edit (§8.3 **[START]**):
/// the broadcast cap, the lockout, and that it is **instant** — a message is over the
/// moment it is sent, so there is no window to switch off.
#[test]
fn the_false_call_numbers_are_pinned() {
    assert_eq!(FALSE_CALL_RADIUS, 10, "[START] — a wing, not the building");
    let economy = AbilityId::FalseCall
        .def()
        .economy()
        .expect("False Call is an activated ability");
    assert_eq!(economy.duration(), 0, "instant: sent, not carried");
    assert_eq!(economy.cooldown(), 30, "[START]");
    assert_eq!(AbilityId::FalseCall.name(), "False Call");
    assert_eq!(
        AbilityId::FalseCall.bar_name(),
        "Call",
        "§11.4 fits 5 cells"
    );
    assert_eq!(AbilityId::FalseCall.script_letter(), 'f');
}

/// **The reach is the transmitter's, not the eyes'** (§8.3/§9/#504) — Lockdown's answer
/// rather than Confusion's, and the one place this ability departs from the blast it is
/// otherwise shaped like.
///
/// A guard past the guard sense but inside [`FALSE_CALL_RADIUS`] **is** called, which is
/// what makes the thing a radio. It has a readout that a blast does not: the responder
/// walks toward the player and arrives inside the §9 sense long before it arrives
/// anywhere else, and the near line says how many answered on the turn it is fired.
#[test]
fn the_broadcast_reaches_past_what_the_player_can_sense() {
    // A **duct** (§10.7), because that is where the two channels actually come apart:
    // on open floor the sense (10) is wider than the reach (8) and the claim cannot be
    // made at all, while a crawler senses 5 and still broadcasts 8. Inside, the player
    // is concealed too, so nothing here is a sighting.
    let mut s = super::ducts::duct_world_with(vec![Guard::stationary(Cell::new(7, 7))])
        .with_loadout(Loadout::innate().with(AbilityId::FalseCall));
    s.step(Input::Step(Direction::North)); // bump the mouth and climb in
    s.step(Input::Step(Direction::East)); // and on to an interior cell, which is blind
    assert!(s.in_duct(), "precondition: the player is crawling");

    let guard = s.guards()[0].pos();
    let gap = s.player().sight_distance(guard);
    assert!(
        gap > DUCT_SENSE_RANGE && gap <= FALSE_CALL_RADIUS,
        "precondition: the guard is past the crawler's sense and inside the broadcast",
    );
    assert!(
        s.perceive_guard(&s.guards()[0]).is_none(),
        "precondition: so the player cannot perceive it at all",
    );

    assert_eq!(fire(&mut s), 1, "it answered anyway — the radio carried");
    assert_eq!(
        s.guards()[0].destination(),
        Some(s.player()),
        "and it is walking toward the player, which is its own readout",
    );
}

/// §9.1/#325: a **Wait** does not widen the call. The 360° listen widens the *sense*,
/// and the sense is not what this ability is measured in — so unlike Confusion there is
/// no read-moment question here at all, and the bar's every-frame answer (§11.4/#345) is
/// the same on the frame after a Wait as on any other.
#[test]
fn a_wait_does_not_widen_the_call() {
    let mut s = spoofer(Vec::new());
    assert_eq!(s.false_call_area().radius(), FALSE_CALL_RADIUS);

    s.step(Input::Wait);
    assert_eq!(
        s.sense_range(),
        PLAYER_SENSE_RANGE_WAITING,
        "precondition: the listen widened the sense (§9.1)",
    );
    assert_eq!(
        s.false_call_area().radius(),
        FALSE_CALL_RADIUS,
        "and the broadcast is exactly what it was",
    );
}

/// §10.7/#504: a **crawling** player broadcasts in full. The crawlspace's cost is
/// degraded *perception* — the sense shrinks to [`DUCT_SENSE_RANGE`] — and a
/// transmitter does not perceive, so the one narrows and the other does not.
///
/// That is a real thing a duct is now good for, and it is the sharpest consequence of
/// measuring this ability in its own radius rather than in the guard sense. Pinned, so
/// a later tune that quietly reintroduces the clamp has to say so here.
#[test]
fn a_crawling_player_still_broadcasts_in_full() {
    let mut s = super::ducts::duct_world_with(Vec::new())
        .with_loadout(Loadout::innate().with(AbilityId::FalseCall));
    let on_foot = s.false_call_area().radius();
    assert_eq!(on_foot, FALSE_CALL_RADIUS, "on the floor, the transmitter");

    s.step(Input::Step(Direction::North)); // bump the mouth and climb in
    assert!(s.in_duct(), "precondition: the player is crawling (§10.7)");
    assert_eq!(
        s.sense_range(),
        DUCT_SENSE_RANGE,
        "precondition: the crawl narrowed the picture",
    );
    assert_eq!(
        s.false_call_area().radius(),
        on_foot,
        "the radio does not care that you are in a wall",
    );
}

/// **A transmitter, not a detector** (§8.4/§9/#504) — the rule that separates this
/// ladder from Confusion's, and the reason the ability is *always* pressable.
///
/// Confusion may refuse an empty blast because its reach is clamped inside the guard
/// sense: everything it could have caught was already drawn, so refusing tells the
/// player nothing new. This reach is **not** clamped — it covers guards behind the §5
/// cone, and in a duct well past the §9 sense — so a refusal would answer *"is anyone
/// within ten cells of me?"*, and a greyed bar entry would answer it **every frame, for
/// free, without spending the turn**. That is a detector.
///
/// So a firing into an empty facility goes out anyway, costs its turn and its lockout,
/// and reports nothing about what it did or did not find.
#[test]
fn a_call_with_nobody_in_reach_still_fires_and_still_costs() {
    let mut s = spoofer(Vec::new());
    let turn = s.turn();
    assert_eq!(
        s.ability_state(AbilityId::FalseCall),
        AbilityState::Ready,
        "the bar never greys it for want of guards — that would be the leak (§11.4)",
    );

    assert_eq!(fire(&mut s), 0, "nobody answered, and it fired regardless");
    assert!(s.turn() > turn, "the turn is spent (§4.4)");
    assert!(
        matches!(
            s.ability_state(AbilityId::FalseCall),
            AbilityState::Cooling { .. }
        ),
        "and so is the lockout — a press that tells you nothing still costs (§8.2)",
    );
}

/// §11.7/§9: **the near line carries no count.** Confusion says how many it caught
/// because its blast is clamped inside the sense and every one of them was already on
/// the player's picture; this reach is not, so a count would name guards the player
/// cannot perceive. The line reports the transmission, and what answered is learned the
/// way everything about guards is learned — by watching the §9 dots turn (§9.3).
#[test]
fn the_near_line_never_says_how_many_answered() {
    let mut s = spoofer(vec![
        Guard::stationary(Cell::new(20, 24)),
        Guard::stationary(Cell::new(24, 20)),
        Guard::stationary(Cell::new(16, 20)),
    ]);
    assert_eq!(fire(&mut s), 3, "three answered, on the event");

    let line = crate::message_for(
        *s.last_events()
            .iter()
            .find(|e| matches!(e, Event::FalseCallFired { .. }))
            .expect("the call went out"),
    )
    .expect("it speaks");
    assert!(
        !line.text.chars().any(|c| c.is_ascii_digit()),
        "no number reaches the player: {:?}",
        line.text,
    );
    assert_eq!(
        line.category,
        Category::Warning,
        "and it reads as the warning it is, not as a confirmation (§11.2)",
    );
}

// ---------------------------------------------------------------------------
// It dies with the net
// ---------------------------------------------------------------------------

/// §7.3/§7.7/#504 — **the decision this ability makes about the comms console**: the
/// spoofer is radio, so a silenced facility has nothing listening and the press is
/// refused for free.
///
/// That turns the console from a pure upgrade into a genuine trade — silence the net
/// and you lose your own best repositioning tool along with their coordination — which
/// is the shape §7.3 already gives it ("the trade is coordination for
/// predictability"). Refused at the press rather than fired into a dead net, so the
/// player is told once instead of discovering it as a turn and a 30-turn lockout that
/// bought nothing.
#[test]
fn a_silenced_net_leaves_nothing_to_spoof() {
    let mut s = spoofer(vec![Guard::stationary(Cell::new(20, 24))]);
    assert_eq!(
        s.ability_state(AbilityId::FalseCall),
        AbilityState::Ready,
        "precondition: with the net live it is a press worth making",
    );

    s.silence_radio_for_test();
    assert_eq!(
        s.ability_state(AbilityId::FalseCall),
        AbilityState::Unusable,
        "the bar greys it the moment the net dies (§11.4)",
    );

    let turn = s.turn();
    let events = s.step(Input::Activate(AbilityId::FalseCall));
    assert!(
        events.contains(&Event::FalseCallDead),
        "and says why, rather than refusing in silence (§11.7)",
    );
    assert_eq!(s.turn(), turn, "free (§4.4)");
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Calm,
        "nothing went out and nobody moved",
    );
}

// ---------------------------------------------------------------------------
// What the board says
// ---------------------------------------------------------------------------

/// §11.5/#338: the wash the renderer paints **is** the box the call was measured over,
/// carried through on the event rather than re-derived — so the picture cannot claim a
/// reach the mechanic did not have. And it is the wash **alone**: a called guard is
/// walking, not held, so the §7.7 legibility tell is its own sensed dot peeling off,
/// not a recolour that would outlive its search.
#[test]
fn the_wash_is_the_reach_that_fired_and_nothing_rides_the_guards() {
    let mut s = spoofer(vec![Guard::stationary(Cell::new(20, 24))]);
    fire(&mut s);

    assert_eq!(
        s.effect_cell_marks().collect::<Vec<_>>(),
        last_reach(&s).cells(s.layout().facility()),
        "the lit cells are the reach that broadcast",
    );
    assert!(
        s.effect_thing_marks().next().is_none(),
        "a called guard wears no mark — it is on an errand, not held",
    );
}
