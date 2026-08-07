//! **Repel** through the turn loop (§7.6/§8.3, #554).
//!
//! The ability is a wall you can put down in the open, and the things that have to be
//! true for it to be *that* rather than the thing §4.5 and §2.2 forbid are each a section
//! below:
//!
//! - **It raises a wall.** No guard steps into the field, in any mood, and a route that
//!   crosses it goes the long way round.
//! - **The wall stays where it was put.** The disc is a snapshot at the fired cell: the
//!   player walks out of their own field and it neither follows nor shrinks. A disc that
//!   travelled would be a disc no guard could ever reach the player in, which repeals
//!   §4.5's **[SETTLED]** capture-is-contact for the length of the window.
//! - **The wall is only ever temporary, and never a trap.** Every cell is released when
//!   the window ends — expiry or the free toggle-off — a guard with no route holds rather
//!   than deadlocking, and the player is never refused a cell of their own field.
//! - **It conceals nothing.** A guard that can see through the field sees, climbs §7.3's
//!   ladder and answers the radio exactly as it would over open floor.

use crate::alert::SIGHTING_CONTACT_TURNS;
use crate::guard::GuardState;
use crate::state::*;
use crate::test_support::{open_beat, open_room};

/// A player holding Repel in the middle of a big open room, with `guards` posted as
/// given. Open floor on purpose: this is the ability for the ground a Lockdown cannot
/// touch, so a fixture with doors in it would be testing the wrong room.
fn fielder(player: Cell, guards: Vec<Guard>) -> State {
    State::new(
        open_room(30, 30),
        player,
        Direction::East,
        guards,
        Vec::new(),
        Cell::new(28, 28),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Repel))
}

/// Fire the field, spending the turn (§4.4), and report the disc it stamped.
fn fire(state: &mut State) -> EffectArea {
    state
        .step(Input::Activate(AbilityId::Repel))
        .into_iter()
        .find_map(|e| match e {
            Event::RepelFired { field } => Some(field),
            _ => None,
        })
        .expect("the field went down")
}

/// The same room with the **ghost** debug switch on (§12.6/#507): no guard ever detects
/// the player, so a guard keeps the mood and the errand the fixture gave it.
///
/// It is used where the thing under test is the *router* — does a route cross the field,
/// does a sweep get stranded — and detection would otherwise turn every guard in an open
/// room into a chaser heading for the player's cell, which is a different test. Capture is
/// still contact under the ghost (§4.5), so nothing here is safe that would not be.
fn ghosted(state: State) -> State {
    state.with_debug(crate::DebugModifiers {
        ghost: true,
        ..crate::DebugModifiers::default()
    })
}

/// Repel's window, read off its own catalogue row rather than restated here.
fn window() -> u32 {
    AbilityId::Repel
        .def()
        .economy()
        .expect("Repel is an activated ability")
        .duration()
}

// ---------------------------------------------------------------------------
// The wall
// ---------------------------------------------------------------------------

/// **The ability, working**: the press spends the turn (§4.4), stamps the
/// [`REPEL_RADIUS`] box on the cell it fired from, and says so once (§11.7).
#[test]
fn firing_stamps_the_radius_box_on_the_cell_it_fired_from() {
    let fired_at = Cell::new(15, 15);
    let mut s = fielder(fired_at, Vec::new());

    let turn = s.turn();
    let field = fire(&mut s);
    assert_eq!(s.turn(), turn + 1, "activation spends the turn (§4.4)");
    assert_eq!(field.centre(), fired_at, "measured from where it fired");
    assert_eq!(field.radius(), REPEL_RADIUS, "[START] — a room's middle");
    assert_eq!(
        s.repel_field(),
        Some(field),
        "and the field is what is held"
    );

    // A box, not a disc (§6.1's metric), asserted against the rule in both directions
    // rather than against a hand-listed set: the diagonal corner is in, the cell one
    // past the edge is out.
    for dx in 0..=REPEL_RADIUS + 1 {
        for dy in 0..=REPEL_RADIUS + 1 {
            let cell = Cell::new(fired_at.x + dx, fired_at.y + dy);
            assert_eq!(
                s.repelled(cell),
                dx <= REPEL_RADIUS && dy <= REPEL_RADIUS,
                "{cell:?} is in the box iff both offsets are inside the radius",
            );
        }
    }
}

/// §7.6/§8.3: **no guard steps into the field, in any mood.** Asserted over the four the
/// game has, one scene each, so a future mood cannot quietly acquire an exemption — and
/// the rule that makes it true asks nothing about the mood at all
/// ([`repels`](State::repels)).
///
/// Each scene puts the guard in its mood the way the game does, then runs the whole
/// window with the guard's business on the far side of the field. Both halves are
/// asserted: that the mood was really exercised, and that the guard never stood inside.
#[test]
fn no_guard_enters_the_field_in_any_state() {
    let fired_at = Cell::new(15, 15);
    let scenes: Vec<(GuardState, Box<dyn Fn() -> State>)> = vec![
        // **Calm**, ghosted so it stays that way: a patrol whose errand is on the far
        // side of the field, walking straight at it.
        (
            GuardState::Calm,
            Box::new(move || {
                ghosted(fielder(
                    fired_at,
                    vec![Guard::patrolling_to(Cell::new(9, 15), Cell::new(25, 15))
                        .with_beat(open_beat(30, 30))],
                ))
            }),
        ),
        // **Chasing**: four cells north, inside its own certain zone (§7.6) and looking
        // straight down the column at the player it is chasing into the field.
        (
            GuardState::Chasing,
            Box::new(move || {
                fielder(
                    fired_at,
                    vec![Guard::patrolling(Cell::new(15, 10)).with_beat(open_beat(30, 30))],
                )
            }),
        ),
        // **Investigating**: the same column, further out — past the certain zone, so the
        // look is a glimpse and the mood is the softer one (§7.6).
        (
            GuardState::Investigating,
            Box::new(move || {
                fielder(
                    fired_at,
                    vec![Guard::patrolling(Cell::new(15, 6)).with_beat(open_beat(30, 30))],
                )
            }),
        ),
        // **Alerted**: a §7.6 sweep, opened the way the game opens one — a call answered
        // on the cell the guard is standing on, whose search area laps over the field.
        (
            GuardState::Alerted,
            Box::new(move || {
                let mut s = ghosted(fielder(
                    fired_at,
                    vec![Guard::patrolling(Cell::new(15, 19)).with_beat(open_beat(30, 30))],
                ));
                s.step(Input::Activate(AbilityId::Repel));
                let at = s.guards()[0].pos();
                s.call_guards_to_for_test(at, 1);
                s
            }),
        ),
        // **Responding**: called across the field to a cell on the far side of it (§7.7).
        (
            GuardState::Responding,
            Box::new(move || {
                let mut s = ghosted(fielder(
                    fired_at,
                    vec![Guard::patrolling(Cell::new(15, 21)).with_beat(open_beat(30, 30))],
                ));
                s.step(Input::Activate(AbilityId::Repel));
                s.call_guards_to_for_test(Cell::new(15, 9), 1);
                s
            }),
        ),
    ];

    for (mood, scene) in scenes {
        let mut s = scene();
        // The two scenes that had to fire early to set their mood up have a field
        // already; the rest fire here.
        let field = match s.repel_field() {
            Some(field) => field,
            None => fire(&mut s),
        };
        assert!(
            !field.contains(s.guards()[0].pos()),
            "{mood:?}: the fixture starts the guard outside the field",
        );

        let mut moods = Vec::new();
        // `window() - 1`: the activation turn is the window's first (§8.2), so these are
        // exactly the turns the field is still up — one more and the assertion would be
        // about a wall that had already been handed back.
        for _ in 0..window() - 1 {
            s.step(Input::Wait);
            let guard = &s.guards()[0];
            moods.push(guard.state());
            assert!(
                !field.contains(guard.pos()),
                "{mood:?}: a guard stood on {:?}, inside the field",
                guard.pos(),
            );
        }
        assert!(
            moods.contains(&mood),
            "{mood:?}: the scene never put the guard in the mood it is for: {moods:?}",
        );
    }
}

/// §7.6/§10.4: the wall is a **detour**, not a stall — a guard whose destination lies
/// beyond the field routes *around* it and arrives, which is the whole thing the ability
/// buys (*"a guard cannot get it open, so its route goes the long way round"*, read over
/// open ground).
#[test]
fn a_route_across_the_field_goes_the_long_way_round() {
    let fired_at = Cell::new(15, 15);
    // Ghosted (see [`ghosted`]): a Calm patrol that keeps its errand, so what is measured
    // is the route and not a guard turning to chase the player it walked past.
    let mut s = ghosted(fielder(
        fired_at,
        vec![Guard::patrolling_to(Cell::new(9, 15), Cell::new(25, 15)).with_beat(open_beat(30, 30))],
    ));
    let field = fire(&mut s);

    // The errand is due east down row 15, straight through the middle of the disc. What
    // is asserted is that the guard **keeps going** — off the line, round the box, and
    // closer to where it was headed at the end of the window than at the start. A guard
    // that had simply stopped would fail the last of these, which is the distinction
    // between a detour and a stall.
    let start = s.guards()[0].pos();
    let errand = Cell::new(25, 15);
    let mut walked = Vec::new();
    for _ in 0..window() - 1 {
        s.step(Input::Wait);
        walked.push(s.guards()[0].pos());
    }
    assert!(
        walked.iter().any(|c| c.y != 15),
        "it left the straight line to go round: {walked:?}",
    );
    assert!(
        walked.iter().all(|c| !field.contains(*c)),
        "and never through: {walked:?}",
    );
    // **It is walking, not held.** A detour costs ground — going round the box takes the
    // guard *further* from its errand for a while, which is the whole price the ability
    // charges — so what says "detour" rather than "stall" is that the guard keeps
    // covering new cells the whole time.
    let distinct: std::collections::HashSet<Cell> = walked.iter().copied().collect();
    assert!(
        distinct.len() >= walked.len() - 1,
        "it spent the window walking, not standing: {walked:?}",
    );
    assert_ne!(
        start,
        *walked.last().expect("the window is more than a turn")
    );

    // And the errand survives the wall: once the window is over it is on the far side of
    // where the field was, still going.
    for _ in 0..20 {
        s.step(Input::Wait);
    }
    assert!(
        s.guards()[0].pos().x > fired_at.x,
        "the detour ends where the errand always was: {:?} → {errand:?}",
        s.guards()[0].pos(),
    );
}

/// **The ticket's open question, pinned** (#554): a guard whose *only* destination is
/// inside the field — a chaser, whose destination is the player standing in the middle
/// of it — has no route at all, and **holds**. That is Lockdown's own answer (a sealed
/// door cuts a corridor for its window and the design accepted it), and what makes it a
/// wait rather than the soft-lock §2.2 forbids is the clock: the window ends, and the
/// guard walks in on the very next turn.
///
/// The alternative — walk up and wait *at* the boundary, facing in — reads better as a
/// cordon and is the recorded fallback (appendix 60); it is not what shipped.
#[test]
fn a_guard_with_no_route_holds_and_comes_on_when_the_window_ends() {
    let fired_at = Cell::new(15, 15);
    // Due **north**, five cells up: guards spawn facing south (§7.1), so this one is
    // looking straight down the column at a player it can see the whole time — the field
    // conceals nothing, and the chase this starts is genuinely live.
    let post = Cell::new(15, 15 - REPEL_RADIUS - 2);
    let mut s = fielder(
        fired_at,
        vec![Guard::patrolling(post).with_beat(open_beat(30, 30))],
    );
    // The §4.2 startup look has already happened, so the chase is live on the very turn
    // the field goes down — and the guard is one cell outside the disc, which is the
    // arrangement the ability is actually for.
    let field = fire(&mut s);
    let held_at = s.guards()[0].pos();
    assert!(!field.contains(held_at), "precondition: it is outside");
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Chasing,
        "precondition: it has the player",
    );
    for _ in 0..window() - 1 {
        s.step(Input::Wait);
        assert_eq!(
            s.guards()[0].pos(),
            held_at,
            "with no route in, the chase holds where it stood",
        );
        assert_eq!(
            s.guards()[0].state(),
            GuardState::Chasing,
            "and it is still a chase — the wall hides nobody",
        );
    }

    // The window closes at the end of this turn, and the very next one has the guard
    // walking again. **No permanent stall**: the field is released, not held open by
    // whatever the guard did or did not manage while it was up.
    s.step(Input::Wait);
    assert!(s.repel_field().is_none(), "the window is over");
    s.step(Input::Wait);
    let now = s.guards()[0].pos();
    assert!(
        now != held_at && field.contains(now),
        "and it comes straight on, over the ground the field held: {now:?}",
    );
}

// ---------------------------------------------------------------------------
// The boundary, from the inside
// ---------------------------------------------------------------------------

/// §8.3: a guard **already standing inside** when the disc lands is not moved and not
/// held. The field refuses a crossing *inward*, so this guard is bound by nothing: it
/// walks about inside and out — and only once it is out does the rule start to apply to
/// it.
///
/// All three halves of the ticket's row are one scene, because they are one rule read
/// three ways.
#[test]
fn a_guard_inside_is_left_alone_may_leave_and_cannot_come_back() {
    let fired_at = Cell::new(15, 15);
    // Two cells **south** of the player — inside the disc when it lands, looking away
    // (§7.1's spawn facing), with its errand due east along that row. So it crosses
    // several cells of field before it is out, which is the half of the rule that would
    // be invisible from a cell on the boundary; and its lane never runs through the
    // player's own cell, which would be §4.5 contact rather than anything this is about.
    let inside = Cell::new(15, 17);
    let beyond = Cell::new(25, 17);
    // Ghosted (see [`ghosted`]) so the guard keeps the errand that walks it out of the
    // field rather than turning on the player standing two cells away.
    let mut s = ghosted(fielder(
        fired_at,
        vec![Guard::patrolling_to(inside, beyond).with_beat(open_beat(30, 30))],
    ));
    let field = fire(&mut s);
    assert!(
        field.contains(s.guards()[0].pos()),
        "the stamp moves nobody — it is ground, and ground does not push: the guard is \
         still in the disc that landed on it, walking its own errand",
    );

    // It walks **within** the field and then out of it, unrefused throughout.
    let mut walked = Vec::new();
    for _ in 0..5 {
        s.step(Input::Wait);
        walked.push(s.guards()[0].pos());
    }
    assert!(
        walked.iter().any(|c| field.contains(*c)),
        "it stepped inside the field on its way out: {walked:?}",
    );
    assert!(
        walked.last().is_some_and(|c| !field.contains(*c)),
        "and it is out: {walked:?}",
    );

    // …and now it is one of the guards the field refuses — with nothing remembered about
    // where it has been, because *outside* is the whole of what the rule reads. Sent back
    // to the cell it started in, it never reaches it while the window holds.
    s.call_guards_to_for_test(inside, 1);
    for _ in 0..3 {
        s.step(Input::Wait);
        assert!(
            !field.contains(s.guards()[0].pos()),
            "out is out: once it has left it cannot come back in",
        );
    }
}

/// §4.4: **the player is never refused their own field.** They walk into it, across it
/// and out of it exactly as they walked the floor a moment before — the asymmetry the
/// ability is made of, and what stops a wall you can put down anywhere from ever boxing
/// its own owner in (§2.2).
#[test]
fn the_player_walks_their_own_field() {
    let fired_at = Cell::new(15, 15);
    let mut s = fielder(fired_at, Vec::new());
    let field = fire(&mut s);

    // Out through the boundary…
    for step in 0..=REPEL_RADIUS {
        let before = s.player();
        s.step(Input::Step(Direction::East));
        assert_eq!(
            s.player(),
            Cell::new(before.x + 1, before.y),
            "step {step}: the field refuses nobody but a guard",
        );
    }
    assert!(!field.contains(s.player()), "the player is outside it now");
    // …and back in.
    s.step(Input::Step(Direction::West));
    assert!(
        field.contains(s.player()),
        "and walks straight back into their own field",
    );
}

/// §2.3/§4.5: **the field is void against what is already on you.** The disc is stamped
/// around a guard as readily as around empty floor, and a guard inside it is bound by
/// nothing — so a player who waits until a chaser is at arm's length has spent the turn
/// and the whole lockout building a wall with the hunter on the inside, and is taken
/// exactly as §4.5 **[SETTLED]** says.
///
/// This is the ability's real "when would a good player not press this", so it is pinned
/// rather than left to be discovered: the press is never refused (§8.4 — refusing it
/// would make the key a proximity detector), which means the *only* thing standing
/// between the player and this mistake is knowing the rule.
#[test]
fn the_field_is_void_around_a_guard_already_inside_it() {
    let fired_at = Cell::new(15, 15);
    // Three cells north and looking south: well inside the disc when it lands, and close
    // enough that it is on the player in a couple of turns. (Any nearer and the §4.2
    // startup phase would take the player before the ability could be pressed at all,
    // which is a different lesson.)
    let mut s = fielder(
        fired_at,
        vec![Guard::patrolling(Cell::new(15, 12)).with_beat(open_beat(30, 30))],
    );
    let field = fire(&mut s);
    assert!(
        field.contains(s.guards()[0].pos()),
        "precondition: the wall went up around it",
    );

    for _ in 0..window() {
        if s.outcome() != Outcome::Playing {
            break;
        }
        s.step(Input::Wait);
    }
    assert_eq!(
        s.outcome(),
        Outcome::Lost,
        "the wall it is standing inside stops it from nothing (§4.5)",
    );
}

/// §7.5/§7.6: a §7.6 sweep beside the field **goes round it** rather than being called
/// off — the ticket's own warning that a field over a search target must not strand the
/// sweep.
///
/// The failure this pins is subtle and would have been silent: a searcher picks the
/// farthest cell of its area and reads *"no route to it"* as *"nothing left to poke at"*,
/// which ends the search. A player could then call a sweep off by laying a wall over the
/// corner of it — a §7.6 pressure release nobody asked for, and the opposite of what a
/// wall is supposed to cost. The area is narrowed to the ground the guard can reach
/// ([`step_search`](crate::Guard)), so the sweep simply works the side it is on.
#[test]
fn a_sweep_beside_the_field_works_round_it_instead_of_being_called_off() {
    let fired_at = Cell::new(15, 15);
    // Four cells south of the field's centre — just outside the disc, with a search area
    // that laps well over it.
    let searcher = Cell::new(15, 19);
    let mut s = ghosted(fielder(
        fired_at,
        vec![Guard::patrolling(searcher).with_beat(open_beat(30, 30))],
    ));
    let field = fire(&mut s);
    let at = s.guards()[0].pos();
    assert!(!field.contains(at), "precondition: it is outside the field");
    assert!(s.call_guards_to_for_test(at, 1), "a call it can answer");

    let mut moods = Vec::new();
    for _ in 0..window() - 1 {
        s.step(Input::Wait);
        moods.push(s.guards()[0].state());
        assert!(
            !field.contains(s.guards()[0].pos()),
            "the sweep never crosses the wall",
        );
    }
    assert!(
        moods.iter().all(|m| *m == GuardState::Alerted),
        "…and it is still sweeping: the wall did not call the search off: {moods:?}",
    );
}

// ---------------------------------------------------------------------------
// The snapshot, and the release
// ---------------------------------------------------------------------------

/// §8.3/§4.5: the field is a **snapshot** at the fired cell. Walking away neither moves
/// it, narrows it nor widens it — the property that keeps §4.5's **[SETTLED]**
/// capture-is-contact intact, since a disc centred on a moving player is a disc no guard
/// could ever reach him in.
#[test]
fn the_field_does_not_follow_the_player_or_shrink() {
    let fired_at = Cell::new(15, 15);
    let mut s = fielder(fired_at, Vec::new());
    let field = fire(&mut s);
    let cells = s.repel_field_cells();

    for _ in 0..5 {
        s.step(Input::Step(Direction::East));
        assert_eq!(s.repel_field(), Some(field), "the disc has not moved");
        assert_eq!(s.repel_field_cells(), cells, "nor changed size");
    }
    assert!(
        !s.repelled(s.player()),
        "the player has walked out of their own wall — it stayed where it was put",
    );
    // And the wall is still a wall over there, not here: the cell they are standing on
    // is ordinary ground again.
    assert!(s.repelled(fired_at), "the field still holds where it fell");
}

/// §8.2/§4.4: **every cell is released when the window ends**, however it ends — the
/// duration running out, and the free toggle-off — and the mark goes with it (#308).
/// This is what keeps a temporary wall from ever becoming the permanent one §2.2/§7.2
/// forbid.
#[test]
fn the_whole_field_is_released_at_expiry_and_at_toggle_off() {
    // Expiry.
    let mut s = fielder(Cell::new(15, 15), Vec::new());
    let field = fire(&mut s);
    // The activation turn is the window's first (§8.2's N-yields-N trap), so the field
    // survives `window() - 1` turns after the press and no more.
    for turn in 0..window() - 2 {
        s.step(Input::Wait);
        assert_eq!(
            s.repel_field(),
            Some(field),
            "turn {turn}: the window is still open",
        );
    }
    s.step(Input::Wait);
    assert!(s.repel_field().is_none(), "expiry hands the ground back");
    assert!(s.repel_field_cells().is_empty(), "no cell keeps the flag");
    assert!(
        !s.effect_cell_marks().any(|c| field.contains(c)),
        "and the mark goes with it (#308)",
    );

    // The early toggle-off (§4.4) — free, and it refunds nothing: the full lockout runs.
    let mut s = fielder(Cell::new(15, 15), Vec::new());
    fire(&mut s);
    let turn = s.turn();
    s.step(Input::Deactivate(AbilityId::Repel));
    assert_eq!(s.turn(), turn, "toggling off is free (§4.4)");
    assert!(
        s.repel_field().is_none(),
        "and hands the ground back at once"
    );
    assert!(s.repel_field_cells().is_empty(), "no cell keeps the flag");
    assert!(
        matches!(
            s.ability_state(AbilityId::Repel),
            AbilityState::Cooling { .. }
        ),
        "the lockout still runs (§8.2)",
    );
}

/// §4.4/§8.4/§11.7: a firing with **nobody in reach** is not refused, is not greyed, and
/// costs exactly what a firing in front of a chase costs — the turn and the lockout —
/// and it says so on the near line.
///
/// The reason is the one False Call's row gives: the only precondition worth asking here
/// would be *"is a guard near enough for this to be worth it?"*, and a refusal (or a
/// greyed entry, which answers it every frame for nothing) hands the player a proximity
/// read the §9 channels do not give them. This is terrain, not a detector.
#[test]
fn an_empty_firing_costs_the_turn_and_the_lockout_and_says_so() {
    let mut s = fielder(Cell::new(15, 15), Vec::new());
    assert_eq!(
        s.ability_state(AbilityId::Repel),
        AbilityState::Ready,
        "never greyed for want of a target (§11.4/#345)",
    );

    let turn = s.turn();
    let field = fire(&mut s);
    assert_eq!(s.turn(), turn + 1, "the turn is spent all the same");
    assert!(s.repelled(field.centre()), "and the ground is held");
    assert!(
        crate::status::message_for(Event::RepelFired { field }).is_some(),
        "a press that changed something says so (§11.7)",
    );

    // And the wording is the same one a firing with the facility bearing down would get:
    // one sentence, whatever was in reach, or the near line becomes the detector the
    // refusal was not allowed to be.
    let mut busy = fielder(
        Cell::new(15, 15),
        vec![Guard::patrolling(Cell::new(20, 15)).with_beat(open_beat(30, 30))],
    );
    let crowded = fire(&mut busy);
    assert_eq!(
        crate::status::message_for(Event::RepelFired { field }).map(|m| m.text),
        crate::status::message_for(Event::RepelFired { field: crowded }).map(|m| m.text),
        "the line says nothing about what was out there",
    );
}

// ---------------------------------------------------------------------------
// It conceals nothing
// ---------------------------------------------------------------------------

/// §7.3/§7.7/§8.3: **the field is a wall, not a cloak.** A guard with a line of sight
/// into it sees the player standing in the middle of it, that sighting climbs the
/// facility's alert ladder exactly as any other does, and the radio goes on sending
/// guards to cells the field covers — they simply arrive at its edge.
///
/// This is the ability's price rather than a gap in it (§2.3): what gathers outside a
/// wall nobody may cross is a ring of guards with nothing else to do, standing where the
/// player has to come out.
#[test]
fn it_conceals_nothing() {
    let fired_at = Cell::new(15, 15);
    // North of the field and looking south down the column (§7.1's spawn facing), so it
    // has the player in its certain zone across the whole of the wall.
    let watcher = Cell::new(15, 15 - REPEL_RADIUS - 2);
    // …and a second guard in the far corner with no sight of anything, because §7.7's own
    // rule is that a guard which has the live player is never pulled off it: the watcher
    // cannot be the one that answers the radio, and pinning the call on it would be
    // pinning the wrong rule.
    // Twelve cells off, past the §6.1 sight box entirely: a guard that had so much as
    // glimpsed the player would be Danger-category and so unrespondable itself (§7.7).
    let far = Cell::new(27, 27);
    let mut s = fielder(
        fired_at,
        vec![
            Guard::patrolling(watcher).with_beat(open_beat(30, 30)),
            Guard::patrolling(far).with_beat(open_beat(30, 30)),
        ],
    );
    let field = fire(&mut s);
    assert!(!field.contains(watcher), "precondition: it is outside");

    // Seen, from outside, through the wall that is not one.
    assert!(
        s.guard_detects_now(&s.guards()[0]),
        "a guard that can see you sees you, field or no field",
    );

    // The ladder still climbs: enough contact turns inside the window make the confirmed
    // sighting that raises the facility's condition (§7.3).
    for _ in 0..SIGHTING_CONTACT_TURNS {
        s.step(Input::Wait);
    }
    assert!(
        s.alert() >= 1,
        "the sighting stepped the ladder — the field silences nothing",
    );

    // And the radio still calls guards to ground the field covers. The errand is
    // accepted; where the responder can walk is the wall's business, not the net's.
    assert!(
        s.call_guards_to_for_test(fired_at, 1),
        "a call naming a cell inside the field is answered like any other (§7.7)",
    );
    assert_eq!(
        s.guards()[1].state(),
        GuardState::Responding,
        "and a guard that is free takes the errand — the wall does not silence the net",
    );
}

// ---------------------------------------------------------------------------
// Determinism (§12.4)
// ---------------------------------------------------------------------------

/// §12.4: same seed, same inputs, same field — **and the same guard routes around it**.
/// The field itself is drawn from no randomness at all, so what this really pins is the
/// half that could drift: the routing detour, which is a BFS over a blocked set built
/// per guard, per turn.
#[test]
fn the_same_inputs_reproduce_the_same_field_and_the_same_routes() {
    let script = || {
        let mut s = fielder(
            Cell::new(15, 15),
            vec![
                Guard::patrolling_to(Cell::new(11, 15), Cell::new(19, 15))
                    .with_beat(open_beat(30, 30)),
                Guard::patrolling_to(Cell::new(15, 11), Cell::new(15, 19))
                    .with_beat(open_beat(30, 30)),
            ],
        )
        .with_rng(crate::Rng::new(7));
        fire(&mut s);
        let mut walked = Vec::new();
        for turn in 0..window() {
            s.step(if turn % 3 == 0 {
                Input::Wait
            } else {
                Input::Step(Direction::East)
            });
            walked.push((
                s.repel_field(),
                s.guards().iter().map(Guard::pos).collect::<Vec<_>>(),
            ));
        }
        walked
    };
    assert_eq!(script(), script(), "same seed, same inputs, same board");
}
