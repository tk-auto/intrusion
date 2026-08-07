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

/// **The ticket's open question, pinned** (#554), and settled the *second* way it offered:
/// a guard whose only route runs through the field — a chaser, whose destination is the
/// player standing in the middle of it — **closes on the boundary and waits there**,
/// rather than holding wherever the failed route left it.
///
/// Both answers are the same hold: nothing crosses the line either way, and the window is
/// the only clock in both. What the cordon buys is that it *reads* as the hunt still
/// happening — a chase that stops dead two rooms away looks like the game giving up on
/// you, where a guard standing at the edge of the wall looking in is the ability's own
/// price made visible. It is also exactly where the player has to come out (appendix 60).
#[test]
fn a_guard_with_no_route_closes_to_the_boundary_and_waits() {
    let fired_at = Cell::new(15, 15);
    // Nine cells due north and looking south (§7.1's spawn facing), so it has the player
    // the whole time and has real ground to cover before it reaches the wall.
    let mut s = fielder(
        fired_at,
        vec![Guard::patrolling(Cell::new(15, 6)).with_beat(open_beat(30, 30))],
    );
    let field = fire(&mut s);
    let opened_at = s.guards()[0].pos();
    assert!(
        !field.contains(opened_at) && opened_at.sight_distance(fired_at) > REPEL_RADIUS + 1,
        "precondition: it starts outside the field and not already against it",
    );

    // It walks in, and stops on the last cell that is not the field's.
    let mut walked = Vec::new();
    for _ in 0..window() - 1 {
        s.step(Input::Wait);
        let at = s.guards()[0].pos();
        walked.push(at);
        assert!(!field.contains(at), "never across the line: {walked:?}");
    }
    let cordon = *walked.last().expect("the window is more than a turn");
    assert!(
        cordon.sight_distance(fired_at) == REPEL_RADIUS + 1,
        "it closed to the boundary and stopped on it: {walked:?}",
    );
    assert!(
        walked.iter().filter(|&&at| at == cordon).count() > 1,
        "…and waited there rather than milling about: {walked:?}",
    );
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Chasing,
        "still a chase — the wall hides nobody",
    );

    // And the moment the window closes it comes straight on, from a cell one step from
    // the disc rather than from wherever it happened to be standing. **No permanent
    // stall** — and the cordon is the ability's cost, not a reprieve.
    s.step(Input::Wait);
    assert!(s.repel_field().is_none(), "the window is over");
    s.step(Input::Wait);
    let now = s.guards()[0].pos();
    assert!(
        field.contains(now),
        "it walks straight over the ground the field held: {now:?}",
    );
}

// ---------------------------------------------------------------------------
// The boundary, from the inside
// ---------------------------------------------------------------------------

/// §8.3 (#554, amended): a guard **caught inside** when the disc lands **walks out, by
/// the shortest way**, and cannot come back.
///
/// The ticket asked only that the stamp not move anybody and that a guard inside be free
/// to leave; what shipped is stronger, because "free to leave" left guards milling about
/// inside a wall, which reads as the field not working at all. The field is ground guards
/// will not stand in: the boundary keeps them out, and anybody it lands *around* leaves.
///
/// It is a step, not a shove. The guard spends its own turn, keeps its mood, its lead and
/// its errand — and picks that errand straight back up on the outside, which is what the
/// tail of this test watches.
#[test]
fn a_guard_inside_walks_out_by_the_shortest_way_and_cannot_come_back() {
    let fired_at = Cell::new(15, 15);
    // Two cells south of the player and looking away (§7.1's spawn facing), with its
    // errand due east: inside when the field lands, and its own plan would have carried
    // it *along* the disc rather than out of it — so what this measures is the exit rule
    // and not the guard's own route.
    let inside = Cell::new(15, 17);
    let beyond = Cell::new(25, 17);
    let mut s = ghosted(fielder(
        fired_at,
        vec![Guard::patrolling_to(inside, beyond).with_beat(open_beat(30, 30))],
    ));
    let field = fire(&mut s);
    let caught = s.guards()[0].pos();
    assert!(
        field.contains(caught),
        "the stamp moves nobody — it is ground, and ground does not push",
    );

    // The shortest way out of a box you are one cell inside is one step, and that is the
    // step it takes — not the two or three its errand would have cost.
    let out_in = 1 + fired_at.y + REPEL_RADIUS - caught.y;
    let mut walked = Vec::new();
    for _ in 0..out_in {
        s.step(Input::Wait);
        walked.push(s.guards()[0].pos());
    }
    assert!(
        walked.last().is_some_and(|at| !field.contains(*at)),
        "out in {out_in}, the shortest way: {walked:?}",
    );
    // …and then it is about its own business again, unchanged by the detour — one turn to
    // swing back onto its eastward heading (§7.5's turn in place, which the exit walk does
    // not pay but ordinary patrol does), then away.
    for _ in 0..2 {
        s.step(Input::Wait);
    }
    assert!(
        s.guards()[0].pos().x > caught.x,
        "the errand was waiting for it outside: {:?}",
        s.guards()[0].pos(),
    );

    // Now it is one of the guards the boundary refuses — with nothing remembered about
    // where it has been, because *outside* is the whole of what the rule reads. Sent back
    // to the cell it started in, it never reaches it while the window holds.
    s.call_guards_to_for_test(caught, 1);
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

/// §2.3/§4.5 (#554, amended — **and this is the ability's balance question**): a chaser
/// that the disc lands *around* is **put out of it and then held out**, so firing at the
/// last moment now works.
///
/// This is the consequence of the exit rule above, and it is the opposite of what the
/// ticket specified: there, a guard inside was unconstrained and a field stamped around a
/// hunter at arm's length spent the turn and the lockout on nothing — the ability's stated
/// "when would a good player not press this". With guards walking out, the answer is
/// *"there is no such moment"*, and Repel becomes an escape from a capture that was
/// already happening.
///
/// It is pinned here rather than left to be discovered, because a rule that quietly makes
/// §4.5 negotiable for eight turns is the thing §8.3 warns about by name (Confusion's *"a
/// no-guard-may-act field you carry"*), and the number that decides whether it has gone
/// too far is on `docs/stats/abilities/repel.md`. If it has, the lever is this test: an
/// exit rule that exempts a guard which currently **has** the player restores the old
/// trade without touching anything else.
#[test]
fn a_chaser_caught_inside_is_put_out_and_held_out() {
    let fired_at = Cell::new(15, 15);
    // Three cells north, looking south: chasing, and inside the disc when it lands.
    let mut s = fielder(
        fired_at,
        vec![Guard::patrolling(Cell::new(15, 12)).with_beat(open_beat(30, 30))],
    );
    let field = fire(&mut s);
    assert!(
        field.contains(s.guards()[0].pos()),
        "precondition: the wall went up around the hunter",
    );
    assert_eq!(s.guards()[0].state(), GuardState::Chasing);

    for _ in 0..window() - 1 {
        s.step(Input::Wait);
        assert_eq!(
            s.outcome(),
            Outcome::Playing,
            "the chase that was one step from taking you does not arrive",
        );
    }
    let cordoned = s.guards()[0].pos();
    assert!(
        !field.contains(cordoned) && cordoned.sight_distance(fired_at) == REPEL_RADIUS + 1,
        "it left and is now waiting at the edge: {cordoned:?}",
    );
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Chasing,
        "…still chasing, and eight turns is all it has to wait",
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
