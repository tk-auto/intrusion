//! The facility alert ladder through the turn loop (§7.3).
//!
//! [`crate::alert`] owns the ladder's arithmetic and pins it in isolation; what is
//! pinned here is the ladder **wired into the game** — every trigger fired by the
//! thing that is supposed to fire it, the rung-1 retaliation actually changing how a
//! guard patrols, and the two rules the ladder must never break: no decay, and no
//! guard ever moves faster (§7.1 **[SETTLED]**).

use crate::alert::{ALERT_DWELL_TURNS_MAX, SIGHTING_CONTACT_TURNS, SIGHTING_WINDOW_TURNS};
use crate::guard::{GuardState, GUARD_DWELL_TURNS_MIN};
use crate::state::*;
use crate::test_support::open_room;
use crate::{generate_level, AlertTrigger, AlertTuning, Rng};

/// A player in a cupboard with a stationary guard staring down at the cell in front
/// of it, plus whatever else the caller stamps in first.
///
/// ```text
///   (5,2)  g   the watcher, facing south, cone down the column
///   (5,5)  .   the open cell — 3 away, inside CERTAIN_RANGE
///   (5,6)  ▯   the cupboard the player starts in, concealed (§10.3)
/// ```
///
/// Stepping north stands the player in the certain zone; stepping south bumps back
/// into the cupboard and out of sight. One step per turn, so contact turns can be
/// counted out exactly. The watcher never patrols, so it never walks over and ends
/// the experiment.
fn watched_cupboard(layout: Layout) -> State {
    State::new(
        layout,
        Cell::new(5, 6),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 2))],
        Vec::new(),
        Cell::new(10, 10),
    )
}

/// The bare version of [`watched_cupboard`]'s room.
fn watched_room() -> Layout {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 6), Terrain::Hideout);
    layout
}

/// Every rung the events of one step reported, with the trigger that got it there.
fn escalations(events: &[Event]) -> Vec<(u32, AlertTrigger)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::AlertRaised { rung, trigger } => Some((*rung, *trigger)),
            _ => None,
        })
        .collect()
}

/// Stand in the watcher's certain zone for `turns` turns, then duck back into the
/// cupboard, returning every escalation the whole excursion reported. The step out
/// is itself a contact turn — the guard's cone is recomputed before it senses (§4.2)
/// — so `turns` is the number of contact turns, not the number of waits.
fn show_yourself(state: &mut State, turns: u32) -> Vec<(u32, AlertTrigger)> {
    let mut seen = escalations(&state.step(Input::Step(Direction::North)));
    for _ in 1..turns {
        seen.extend(escalations(&state.step(Input::Wait)));
    }
    seen.extend(escalations(&state.step(Input::Step(Direction::South))));
    seen
}

/// Wait out the sighting window from inside the cupboard, so the next run of contact
/// counts as a fresh sighting (§7.6).
fn wait_out_the_window(state: &mut State) -> Vec<(u32, AlertTrigger)> {
    (0..SIGHTING_WINDOW_TURNS)
        .flat_map(|_| escalations(&state.step(Input::Wait)))
        .collect()
}

/// §7.3/§7.6: [`SIGHTING_CONTACT_TURNS`] turns of certain-zone contact make a
/// **confirmed sighting**, and that is the ladder's rung-1 trigger. Two contact turns
/// are not a sighting, however long the facility then waits.
#[test]
fn a_confirmed_sighting_steps_the_ladder_to_rung_one() {
    let mut s = watched_cupboard(watched_room());
    assert_eq!(s.alert(), 0, "a clean entry starts at rung 0");

    // Two contact turns and back into the dark: not a sighting, then or ever.
    assert!(show_yourself(&mut s, SIGHTING_CONTACT_TURNS - 1).is_empty());
    assert!(wait_out_the_window(&mut s).is_empty());
    assert_eq!(s.alert(), 0, "two contact turns are not a sighting");

    // Three is.
    assert_eq!(
        show_yourself(&mut s, SIGHTING_CONTACT_TURNS),
        vec![(1, AlertTrigger::Sighting)],
        "three contact turns inside the window are one sighting",
    );
    assert_eq!(s.alert(), 1);
}

/// §7.6: three *separate* sightings reach rung 2 — and they have to be separate. A
/// chase that holds the player in the certain zone turn after turn is one sighting,
/// so the window must fall back to zero before another can be counted.
#[test]
fn three_separate_sightings_reach_rung_two() {
    let mut s = watched_cupboard(watched_room());

    assert_eq!(
        show_yourself(&mut s, SIGHTING_CONTACT_TURNS * 4),
        vec![(1, AlertTrigger::Sighting)],
        "one long look is one sighting, not four",
    );

    wait_out_the_window(&mut s);
    assert!(
        show_yourself(&mut s, SIGHTING_CONTACT_TURNS).is_empty(),
        "the second sighting is counted, but rung 1 is already reached",
    );
    assert_eq!(s.alert(), 1);

    wait_out_the_window(&mut s);
    assert_eq!(
        show_yourself(&mut s, SIGHTING_CONTACT_TURNS),
        vec![(2, AlertTrigger::RepeatSightings)],
        "the third sighting is the rung-2 trigger",
    );
    assert_eq!(s.alert(), 2);
}

/// §7.3: **rung 0 is safe.** Tampering with an intel console while the facility has
/// no idea you are there triggers nothing at all — that is the whole incentive to
/// stay unseen. The same bump once you have been seen reaches rung 2.
#[test]
fn tampering_costs_nothing_until_the_facility_knows_you_are_there() {
    let mut layout = watched_room();
    // Two consoles, either side of the cupboard the player starts in.
    layout.place(Cell::new(4, 6), Terrain::Console);
    layout.place(Cell::new(6, 6), Terrain::Console);
    let mut s = State::new(
        layout,
        Cell::new(5, 6),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 2))],
        [Cell::new(4, 6), Cell::new(6, 6)],
        Cell::new(10, 10),
    );

    let quiet = s.step(Input::Step(Direction::West));
    assert!(
        quiet.iter().any(|e| matches!(e, Event::IntelTaken { .. })),
        "the console is taken",
    );
    assert!(escalations(&quiet).is_empty(), "rung 0 hears nothing");
    assert_eq!(s.alert(), 0);

    show_yourself(&mut s, SIGHTING_CONTACT_TURNS);
    assert_eq!(s.alert(), 1, "now the facility knows");

    assert_eq!(
        escalations(&s.step(Input::Step(Direction::East))),
        vec![(2, AlertTrigger::ConsoleTampered)],
        "the same bump, once seen, says what you came for",
    );
    assert_eq!(s.alert(), 2);
}

/// A 5×30 corridor with the player in a cupboard and a guard on either side of it,
/// each on its own short radio clock — two takedowns from cover, and then two posts
/// that stop answering (§7.2/§7.3).
///
/// ```text
///   (1, 9)  g   the northern victim, facing south — the player's cell is concealed
///   (1,10)  ▯   the cupboard the player starts in
///   (1,11)  g   the southern victim, facing south — its back to the strike
/// ```
///
/// Both guards face **south** (§7.1), so neither one's cone covers the other's cell:
/// the bodies are never found, and what the ladder hears is the radio alone.
fn two_victims(north: u32, south: u32) -> State {
    let mut layout = open_room(5, 30);
    layout.place(Cell::new(1, 10), Terrain::Hideout);
    State::new(
        layout,
        Cell::new(1, 10),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(1, 9))
                .with_radio_clock(radio::RadioClock::from_period(north)),
            Guard::stationary(Cell::new(1, 11))
                .with_radio_clock(radio::RadioClock::from_period(south)),
        ],
        Vec::new(),
        Cell::new(1, 28),
    )
}

/// §7.3: one missed ping is rung 1; a **second post** falling silent is rung 3. The
/// trigger counts bodies, not pings — which is why the two clocks here are different
/// lengths and the escalations land on different turns.
#[test]
fn a_second_silent_post_takes_the_ladder_to_the_top() {
    let mut s = two_victims(3, 9);
    s.step(Input::Step(Direction::North)); // the northern victim
    s.step(Input::Step(Direction::South)); // the southern one
    assert_eq!(s.bodies().len(), 2, "two posts to go quiet");
    assert!(s.guards().is_empty(), "and nobody left to hear it");
    assert!(
        s.bodies().iter().all(|b| !b.found()),
        "no cone covers either body: the radio is the only voice here",
    );

    let mut ladder = Vec::new();
    for _ in 0..12 {
        ladder.extend(escalations(&s.step(Input::Wait)));
    }
    assert_eq!(
        ladder,
        vec![
            (1, AlertTrigger::MissedPing),
            (3, AlertTrigger::SecondPostSilent),
        ],
        "one quiet post is a fault; two is an intruder",
    );
    assert_eq!(s.alert(), 3);
}

/// §7.3: a silenced net misses no pings, so **no** ping-driven rung can fire after
/// the comms console has been bumped — however long the bodies lie there.
#[test]
fn a_silenced_radio_can_never_step_the_ladder_again() {
    let mut layout = open_room(5, 30);
    layout.place(Cell::new(1, 1), Terrain::Hideout);
    layout.place(Cell::new(1, 3), Terrain::CommsConsole);
    let mut s = State::new(
        layout,
        Cell::new(1, 1),
        Direction::North,
        vec![Guard::stationary(Cell::new(1, 2)).with_radio_clock(radio::RadioClock::from_period(6))],
        Vec::new(),
        Cell::new(1, 28),
    );

    s.step(Input::Step(Direction::South)); // take the victim down…
    s.step(Input::Step(Direction::South)); // …step onto its cell, dragging nothing
    let killed = s.step(Input::Step(Direction::South)); // …and bump the comms console
    assert!(
        killed
            .iter()
            .any(|e| matches!(e, Event::CommsSilenced { .. })),
        "the net is down",
    );

    for _ in 0..40 {
        assert!(
            escalations(&s.step(Input::Wait)).is_empty(),
            "control cannot notice a silence it is not listening for",
        );
    }
    assert_eq!(s.alert(), 0, "the ladder never left the ground");
}

/// **Rung 1 has teeth** (§7.3/§7.5). The same scene, the same guards, the same
/// inputs — the player either standing where the watcher can see them or waiting it
/// out in the cupboard — and the patrolling guard on the far side of the level holds
/// **measurably** less ground once the facility is at rung 1.
///
/// The measured guard is walled off from the whole business: it never sees the
/// player, is never called anywhere, and its own mood is Calm throughout. The one
/// thing that differs between the two runs is the rung, which is what makes the
/// difference in its patrol the rung's doing.
///
/// The pause is shortened, never removed: the alerted guard still dwells, because a
/// dwell of zero would take the Takedown (§7.2) off the table for the rest of every
/// level that ever reaches rung 1 — which is nearly all of them, there being no decay.
#[test]
fn rung_one_shortens_the_patrol_dwell_without_removing_it() {
    // `seen` decides only where the player stands: the layout, the guards and the
    // RNG stream are identical either way.
    let held = |seen: bool| -> (u32, u32) {
        let mut layout = open_room(40, 12);
        layout.place(Cell::new(5, 6), Terrain::Hideout);
        // A full-height wall down the middle: the measured patrol's territory, its
        // cone and its routes all stop here, so the east half never learns anything.
        for y in 0..12 {
            layout.place(Cell::new(20, y), Terrain::Wall);
        }
        let mut s = State::new(
            layout,
            Cell::new(5, 6),
            Direction::North,
            vec![
                Guard::stationary(Cell::new(5, 2)),
                // The east half of the room is its beat: a fixture guard is handed
                // one explicitly, since a hand-built room has no region graph.
                Guard::patrolling(Cell::new(30, 6)).with_beat(
                    (1..11)
                        .flat_map(|y| (21..39).map(move |x| Cell::new(x, y)))
                        .collect(),
                ),
            ],
            Vec::new(),
            Cell::new(1, 10),
        );
        if seen {
            // Stand up into the watcher's certain zone, and stay there.
            s.step(Input::Step(Direction::North));
        }
        let (mut dwelt, mut longest, mut run) = (0, 0, 0);
        for _ in 0..80 {
            s.step(Input::Wait);
            assert_eq!(
                s.guards()[1].state(),
                GuardState::Calm,
                "the measured guard is walled off from all of it",
            );
            if s.guards()[1].is_dwelling() {
                dwelt += 1;
                run += 1;
                longest = u32::max(longest, run);
            } else {
                run = 0;
            }
        }
        assert_eq!(
            s.alert(),
            u32::from(seen),
            "the scene reached the rung it meant to",
        );
        (dwelt, longest)
    };

    let (calm_turns, calm_longest) = held(false);
    let (alerted_turns, alerted_longest) = held(true);

    assert!(
        alerted_turns < calm_turns,
        "a guard that is never calm holds less ground: {alerted_turns} vs {calm_turns} turns",
    );
    assert!(
        alerted_turns > 0,
        "…but it still pauses — the Takedown window survives (§7.5/§7.2)",
    );
    assert!(
        alerted_longest <= ALERT_DWELL_TURNS_MAX,
        "no alerted dwell outlasts the shortened range: {alerted_longest}",
    );
    assert!(
        calm_longest >= GUARD_DWELL_TURNS_MIN,
        "…and a quiet facility still gives the full §7.5 window: {calm_longest}",
    );
}

/// §7.1 **[SETTLED]**: **guards never accelerate.** The tempting wrong answer to
/// "make the alert matter" is to make the hunt faster, so it gets an assertion: over
/// a full generated level driven to rung 3, no guard ever moves more than one cell in
/// one turn, at any rung.
///
/// §7.3 **no decay** rides along in the same run: the rung is watched every turn and
/// never falls.
#[test]
fn no_rung_ever_speeds_a_guard_up_or_falls_back_down() {
    let mut rng = Rng::new(11);
    let (layout, p) =
        generate_level(&crate::LevelConfig::V1, &mut rng).expect("the v1 config generates");
    let guards = p.guards(&layout);
    let mut s = State::new(
        layout,
        p.player(),
        Direction::North,
        guards,
        p.intel().iter().copied(),
        p.exit(),
    )
    .with_rng(rng);

    let positions = |s: &State| -> Vec<Cell> { s.guards().iter().map(|g| g.pos()).collect() };
    let mut before = positions(&s);
    let mut rung = s.alert();
    let mut peak = rung;
    for turn in 0..300 {
        if s.outcome() != Outcome::Playing {
            break;
        }
        // Walk about rather than waiting: being seen is what drives the ladder.
        s.step(Input::Step(if turn % 2 == 0 {
            Direction::East
        } else {
            Direction::South
        }));
        let after = positions(&s);
        for (a, b) in before.iter().zip(after.iter()) {
            assert!(
                a.manhattan_distance(*b) <= 1,
                "turn {turn}: a guard moved {a:?} → {b:?} in one turn at rung {}",
                s.alert(),
            );
        }
        before = after;
        assert!(
            s.alert() >= rung,
            "turn {turn}: the rung fell from {rung} to {} — the ladder has no decay",
            s.alert(),
        );
        rung = s.alert();
        peak = peak.max(rung);
    }
    assert!(peak > 0, "a walk this careless is seen at some point");
}

/// §12.4: the ladder is a pure function of turn-ordered world facts, so the same seed
/// and the same inputs reach the same rungs on the same turns — the escalations are
/// part of the replay, not a second stream beside it.
#[test]
fn the_same_seed_and_inputs_reach_the_same_rungs_on_the_same_turns() {
    let ladder = |seed: u64| -> Vec<(u32, u32, AlertTrigger)> {
        let mut rng = Rng::new(seed);
        let (layout, p) =
            generate_level(&crate::LevelConfig::V1, &mut rng).expect("the v1 config generates");
        let guards = p.guards(&layout);
        let mut s = State::new(
            layout,
            p.player(),
            Direction::North,
            guards,
            p.intel().iter().copied(),
            p.exit(),
        )
        .with_rng(rng);
        let mut climbed = Vec::new();
        for turn in 0..200 {
            if s.outcome() != Outcome::Playing {
                break;
            }
            let dir = if turn % 3 == 0 {
                Direction::North
            } else {
                Direction::West
            };
            for (rung, trigger) in escalations(&s.step(Input::Step(dir))) {
                climbed.push((s.turn(), rung, trigger));
            }
        }
        climbed
    };

    for seed in [4, 19] {
        assert_eq!(ladder(seed), ladder(seed), "seed {seed}");
    }
}

/// #376/§13.2: a swept [`AlertTuning`] reaches the **real turn loop**, not just the
/// ladder's arithmetic. One turn of certain-zone contact is nothing at all under the
/// shipped ladder (three are needed) and a whole confirmed sighting under a tuning
/// that asks for one — and a second such glance reaches rung 2 where the shipped
/// ladder wants a third.
///
/// This is the knob the sim sweeps (#376). If any of it were still read from the
/// constants it replaced, a swept batch would report a curve for a threshold that
/// never moved — the §13.4 failure of measuring the instrument instead of the game.
#[test]
fn a_swept_tuning_reaches_the_turn_loop() {
    let mut shipped = watched_cupboard(watched_room());
    assert!(
        show_yourself(&mut shipped, 1).is_empty(),
        "one contact turn is no sighting under the shipped ladder",
    );
    assert_eq!(shipped.alert(), 0);

    let mut swept = watched_cupboard(watched_room()).with_alert_tuning(AlertTuning {
        sighting_contact_turns: 1,
        sightings_for_second_rung: 2,
        ..AlertTuning::default()
    });
    assert_eq!(
        show_yourself(&mut swept, 1),
        vec![(1, AlertTrigger::Sighting)],
        "…and a whole sighting under a tuning that asks for one",
    );
    wait_out_the_window(&mut swept);
    assert_eq!(
        show_yourself(&mut swept, 1),
        vec![(2, AlertTrigger::RepeatSightings)],
        "the *second* sighting reaches rung 2 under this tuning, not the third",
    );
    assert_eq!(swept.alert(), 2);
}
