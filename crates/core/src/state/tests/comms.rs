//! The comms console through the turn loop (§7.3/§7.7).
//!
//! The facility-scale answer to the radio net: one bump on the comms console kills
//! all radio for the rest of the level, so control stops pinging downed guards and
//! both §7.7 cooperation call-ins stop firing. What is pinned here is the *whole*
//! contract — the bump and its one-way flag, the two seams it gates, the one thing it
//! deliberately does **not** stop (an errand already given), and how it reads
//! afterwards.

use crate::guard::GuardState;
use crate::state::*;
use crate::test_support::open_room;
use crate::{LevelModifiers, Rng};

/// The player next to a live comms console in an empty room, plus whatever guards the
/// caller wants. Facing south so the first `Step(South)` is the bump.
fn scene(guards: Vec<Guard>) -> State {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 6), Terrain::CommsConsole);
    State::new(
        layout,
        Cell::new(5, 5),
        Direction::South,
        guards,
        Vec::new(),
        Cell::new(10, 10),
    )
}

/// §4.3/§7.7: the console is bumped like everything else — the usable line offers
/// `comms: silence radio`, the bump spends the turn and reports it, and the net is
/// down. Before the bump the net is live; after it, permanently not.
#[test]
fn bumping_the_comms_console_kills_the_net_and_reports_it() {
    let mut s = scene(Vec::new());
    assert!(!s.radio_silenced(), "the net starts live");
    assert_eq!(s.comms_console(), Some(Cell::new(5, 6)));
    assert!(
        s.affordances()
            .iter()
            .any(|(_, a)| *a == Affordance::SilenceRadio),
        "the usable line offers the silence (§11.4)",
    );

    let turn = s.turn();
    let e = s.step(Input::Step(Direction::South));
    assert!(
        e.contains(&Event::CommsSilenced {
            at: Cell::new(5, 6)
        }),
        "the bump reports the kill",
    );
    assert!(s.radio_silenced(), "the net is down");
    assert_eq!(s.player(), Cell::new(5, 5), "a bump does not move you");
    assert!(
        s.turn() > turn,
        "silencing the radio spends the turn (§4.4)"
    );
}

/// §7.7: silencing is **one-way and permanent**. A second bump on the same console
/// is a plain free no-op — the usable line offers nothing, no event fires, and the
/// turn is not spent — and nothing in the loop ever brings the net back, however long
/// the run goes on.
#[test]
fn a_silenced_console_is_spent_scenery_for_the_rest_of_the_level() {
    let mut s = scene(vec![Guard::patrolling(Cell::new(9, 9))]);
    s.step(Input::Step(Direction::South));

    assert!(
        !s.affordances()
            .iter()
            .any(|(_, a)| *a == Affordance::SilenceRadio),
        "a dead net offers nothing to silence",
    );
    assert!(
        s.spent_consoles().any(|c| c == Cell::new(5, 6)),
        "a used comms console recolours as spent (§11.2)",
    );

    let turn = s.turn();
    let again = s.step(Input::Step(Direction::South));
    assert_eq!(
        again,
        vec![Event::Bumped {
            into: Cell::new(5, 6)
        }],
        "a second bump is the free §4.4 no-op",
    );
    assert_eq!(s.turn(), turn, "and spends nothing");

    for _ in 0..80 {
        s.step(Input::Wait);
        assert!(s.radio_silenced(), "the net never comes back");
    }
}

/// §7.3: with the net dead, a downed guard's pings are never missed — no responder is
/// dispatched and the facility alert never steps from that source. The same scene with
/// the console left alone runs the full dispatch-then-alert sequence, so this is the
/// console doing it, not a quiet body.
///
/// The victim carries a short 3-turn clock so both misses would land well inside the
/// window; the player stays in a cupboard so the takedown lands and nothing sees them
/// afterwards (§7.6).
#[test]
fn a_killed_net_never_pings_a_downed_guard() {
    // The victim is west of the player's cupboard, the console east of it, so one
    // scene serves both the takedown and the bump.
    let build = || {
        let mut layout = open_room(12, 30);
        layout.place(Cell::new(5, 5), Terrain::Hideout); // the player, unseen
        layout.place(Cell::new(6, 5), Terrain::CommsConsole);
        State::new(
            layout,
            Cell::new(5, 5),
            Direction::West,
            vec![
                Guard::stationary(Cell::new(4, 5))
                    .with_radio_clock(radio::RadioClock::from_period(3)),
                // The one control would send. Stationary and far off, so it can only
                // ever move because it was *dispatched* — it never wanders into the
                // body and reacts to that instead (§7.2), which would muddy the claim.
                Guard::stationary(Cell::new(5, 25)),
            ],
            Vec::new(),
            Cell::new(10, 28),
        )
    };

    // Baseline: the net is live, so the takedown runs its clock.
    let mut live = build();
    live.step(Input::Step(Direction::West)); // takedown
    let mut saw_silence = false;
    for _ in 0..20 {
        for e in live.step(Input::Wait) {
            saw_silence |= matches!(e, Event::RadioSilence { .. });
        }
    }
    assert!(saw_silence, "a live net pings the body (§7.3)");
    assert!(live.alert() > 0, "and escalates on the second miss");

    // The same scene, with the radio killed first.
    let mut dead = build();
    dead.step(Input::Step(Direction::East)); // bump the console
    assert!(dead.radio_silenced());
    dead.step(Input::Step(Direction::West)); // takedown
    assert_eq!(dead.bodies().len(), 1, "the body is there to be missed");
    for _ in 0..20 {
        for e in dead.step(Input::Wait) {
            assert!(
                !matches!(e, Event::RadioSilence { .. } | Event::AlertRaised { .. }),
                "control cannot ping what it has no radio for",
            );
        }
    }
    assert_eq!(dead.alert(), 0, "no escalation from a dead net");
    assert_eq!(
        dead.guards()[0].state(),
        GuardState::Calm,
        "and nobody was dispatched",
    );
}

/// §7.7: with the net dead, a lost confirmed sighting calls **nobody** — even with
/// the `sighting_lost_calls_a_guard` modifier on, which is the whole point: the
/// console is the counterplay to that modifier. The guard that lost the player still
/// searches on its own (§7.6), so a silenced facility is lonelier, never blind.
#[test]
fn a_killed_net_calls_nobody_on_a_lost_sighting() {
    // The scene the call-in tests share, with a console beside the player's dive.
    let mut s = super::guards::call_in_scene().with_modifiers(LevelModifiers {
        sighting_lost_calls_a_guard: true,
        ..LevelModifiers::default()
    });
    // The scene has no console of its own, so silence the net the way the loop does.
    s.silence_radio_for_test();

    s.step(Input::Step(Direction::West)); // dive into the cupboard, breaking sight
    let mut searched = false;
    for _ in 0..40 {
        for e in s.step(Input::Wait) {
            assert!(
                !matches!(e, Event::CalledIn { .. }),
                "a dead net carries no call",
            );
        }
        searched |= s.guards()[0].state() == GuardState::Alerted;
        assert_ne!(
            s.guards()[1].state(),
            GuardState::Responding,
            "the far guard is never sent",
        );
    }
    assert!(
        searched,
        "the guard that lost the player still searches (§7.6 is not a call)",
    );
}

/// §7.7: with the net dead, a found body calls **nobody** — again with the
/// `body_found_calls_two_guards` modifier on. The finder still reacts to what it
/// found (§7.2): the body is still reported found, and its finder still hunts.
#[test]
fn a_killed_net_calls_nobody_to_a_body() {
    let mut s = super::guards::body_call_scene().with_modifiers(LevelModifiers {
        body_found_calls_two_guards: true,
        ..LevelModifiers::default()
    });
    s.silence_radio_for_test();

    s.step(Input::Step(Direction::North)); // takedown — a body in the open
                                           // Who was in the building when the body was made — see the note below on why the
                                           // guards that walk in afterwards are held apart from the call this test is about.
    let incumbents = s.guards().len();
    let mut found = false;
    for _ in 0..40 {
        for e in s.step(Input::Wait) {
            assert!(
                !matches!(e, Event::BodyCalledIn { .. }),
                "a dead net carries no body call",
            );
            found |= matches!(e, Event::BodyFound { .. });
        }
    }
    assert!(found, "the body is still discovered — the net is what died");
    let responding = s
        .guards()
        .iter()
        .take(incumbents)
        .filter(|g| g.state() == GuardState::Responding)
        .count();
    assert_eq!(responding, 0, "nobody already here was called to it");

    // **Reinforcements are not stopped by a dead net** (§7.3/#374). The comms console's
    // effects are the enumerated ones — no pings, no dispatch, no §7.7 call-ins — and
    // the ladder's own rungs are not among them: a found body still takes the facility
    // to rung 3, and rung 3 still walks guards in from outside. So silencing the radio
    // buys you the *internal* net, not the escalation, which is why the guards standing
    // here are called to nothing while three more let themselves in.
    assert!(
        s.guards().len() > incumbents,
        "the ladder still reached the top and still sent its guards",
    );
}

/// §7.7, the documented choice: a guard **already on an errand** when the net dies
/// **finishes it**. Silencing stops the next wave; it does not recall the search
/// already bearing down on you (§2.3 — cost is load-bearing), and §7.7's own rule is
/// that a call, once made, is never queued or retried — there is no channel to
/// un-send one down either.
#[test]
fn a_guard_already_dispatched_finishes_its_errand() {
    let mut s = super::guards::call_in_scene().with_modifiers(LevelModifiers {
        sighting_lost_calls_a_guard: true,
        ..LevelModifiers::default()
    });
    s.step(Input::Step(Direction::West)); // dive, breaking the sighting

    // Run until the call actually lands and the far guard is on its way.
    let mut errand = None;
    for _ in 0..40 {
        s.step(Input::Wait);
        if s.guards()[1].state() == GuardState::Responding {
            errand = s.guards()[1].destination();
            break;
        }
    }
    let errand = errand.expect("the far guard was called in (§7.7)");

    // Now the net dies, mid-errand.
    s.silence_radio_for_test();
    assert_eq!(
        s.guards()[1].state(),
        GuardState::Responding,
        "the errand is not cancelled by the kill",
    );
    assert_eq!(
        s.guards()[1].destination(),
        Some(errand),
        "and it still heads for the cell it was sent to",
    );
    // It keeps walking: a step of progress toward the cell, not a stand-down.
    let before = s.guards()[1].pos().manhattan_distance(errand);
    for _ in 0..6 {
        s.step(Input::Wait);
    }
    assert!(
        s.guards()[1].pos().manhattan_distance(errand) < before,
        "the dispatched guard keeps closing on its errand",
    );
}

/// §11.5a: the comms console is **contents**, hidden until seen — the counterplay has
/// to be found. An unscouted console shows the plain floor standing in for it; once
/// seen it is remembered like an intel console.
#[test]
fn the_comms_console_is_hidden_until_seen() {
    // A console behind a wall, out of the player's first field of view.
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 8), Terrain::CommsConsole);
    for x in 1..11 {
        layout.place(Cell::new(x, 7), Terrain::Wall);
    }
    let s = State::new(
        layout,
        Cell::new(5, 3),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 1),
    );
    let grid = crate::render::render(&s);
    assert_ne!(
        grid.get(5, 8).glyph,
        Terrain::CommsConsole.glyph(),
        "an unseen comms console is masked as floor (§11.5a)",
    );

    // Walk down to the wall, open nothing — just prove that once it *is* in view the
    // glyph appears, so the mask above is fog and not a missing stamp.
    let mut seen = State::new(
        {
            let mut layout = open_room(12, 12);
            layout.place(Cell::new(5, 8), Terrain::CommsConsole);
            layout
        },
        Cell::new(5, 6),
        Direction::South,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 1),
    );
    seen.step(Input::Wait);
    assert_eq!(
        crate::render::render(&seen).get(5, 8).glyph,
        Terrain::CommsConsole.glyph(),
        "a console in view draws its own glyph",
    );
}

/// §12.4: the whole thing is deterministic — the same seed and the same inputs put
/// the console in the same cell and leave the net in the same state.
#[test]
fn silencing_is_deterministic() {
    let run = || {
        let mut rng = Rng::new(4242);
        let (layout, p) =
            crate::generate_level(&crate::LevelConfig::V1, &mut rng).expect("the v1 config places");
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
        for _ in 0..30 {
            s.step(Input::Wait);
        }
        (s.comms_console(), s.radio_silenced(), s.alert())
    };
    assert_eq!(run(), run(), "same seed, same inputs, same radio state");
}

/// §7.3/§7.5 — **what a silenced net costs the player.** With the radio dead there is
/// no coordination left to divide the building, so a Calm guard's territory stops being
/// its own slice of the §7.5 partition and becomes the whole level: it walks ground it
/// would never have been assigned.
///
/// Measured as *reach*. A guard's beat is a corner of the room; the far corner is
/// somewhere the partition would never send it. On a live net it stays home; on a dead
/// one it gets out there. The player is shut in a cupboard so the patrol is never
/// disturbed and the only difference between the two runs is the console.
#[test]
fn a_silenced_net_sends_a_patrol_over_the_whole_level() {
    let reach = |silence: bool| -> u32 {
        let mut layout = open_room(20, 20);
        layout.place(Cell::new(1, 1), Terrain::Hideout);
        layout.place(Cell::new(5, 6), Terrain::CommsConsole);
        // A beat of the north-west corner only — nothing beyond x or y of 6.
        let beat: Vec<Cell> = (1..7)
            .flat_map(|y| (1..7).map(move |x| Cell::new(x, y)))
            .collect();
        let mut s = State::new(
            layout,
            Cell::new(1, 1),
            Direction::North,
            vec![Guard::patrolling(Cell::new(3, 3)).with_beat(beat)],
            Vec::new(),
            Cell::new(18, 18),
        )
        .with_rng(Rng::new(11));
        if silence {
            s.silence_radio_for_test();
        }
        let mut farthest = 0;
        for _ in 0..120 {
            s.step(Input::Wait);
            farthest = farthest.max(s.guards()[0].pos().manhattan_distance(Cell::new(3, 3)));
        }
        farthest
    };

    let (live, dead) = (reach(false), reach(true));
    assert!(
        live <= 10,
        "a coordinated guard stays on its own beat (reached {live})",
    );
    assert!(
        dead > live,
        "a silenced net sends it further than its beat ever would ({dead} vs {live})",
    );
}

/// §7.3/§7.5: the loss of **predictability** is the price, and it is the whole price.
/// Two runs of the same silenced scene from *different* run seeds diverge, where two
/// runs on a live net are identical — a patrol you can no longer learn.
#[test]
fn a_silenced_patrol_is_no_longer_learnable() {
    let walk = |seed: u64, silence: bool| -> Vec<Cell> {
        let mut layout = open_room(20, 20);
        layout.place(Cell::new(1, 1), Terrain::Hideout);
        let beat: Vec<Cell> = (1..19)
            .flat_map(|y| (1..19).map(move |x| Cell::new(x, y)))
            .collect();
        let mut s = State::new(
            layout,
            Cell::new(1, 1),
            Direction::North,
            vec![Guard::patrolling(Cell::new(10, 10)).with_beat(beat)],
            Vec::new(),
            Cell::new(18, 18),
        )
        .with_rng(Rng::new(seed));
        if silence {
            s.silence_radio_for_test();
        }
        (0..60)
            .map(|_| {
                s.step(Input::Wait);
                s.guards()[0].pos()
            })
            .collect()
    };

    assert_eq!(
        walk(1, false),
        walk(2, false),
        "a coordinated sweep is a function of the board, not the seed — learnable",
    );
    assert_ne!(
        walk(1, true),
        walk(2, true),
        "a silenced sweep is drawn from the run's own stream — not learnable",
    );
}

/// §12.4 **[SETTLED]**: the wander draws from the run's own seeded stream, so a
/// silenced facility reproduces exactly like any other — same seed and inputs, same
/// walk, every time.
#[test]
fn a_silenced_patrol_is_still_deterministic() {
    let walk = || -> Vec<Cell> {
        let mut layout = open_room(20, 20);
        layout.place(Cell::new(1, 1), Terrain::Hideout);
        let beat: Vec<Cell> = (1..19)
            .flat_map(|y| (1..19).map(move |x| Cell::new(x, y)))
            .collect();
        let mut s = State::new(
            layout,
            Cell::new(1, 1),
            Direction::North,
            vec![Guard::patrolling(Cell::new(10, 10)).with_beat(beat)],
            Vec::new(),
            Cell::new(18, 18),
        )
        .with_rng(Rng::new(7));
        s.silence_radio_for_test();
        (0..40)
            .map(|_| {
                s.step(Input::Wait);
                s.guards()[0].pos()
            })
            .collect()
    };
    assert_eq!(walk(), walk(), "same seed, same silenced run (§12.4)");
}

/// §7.3's restraint, pinned: *"a silenced facility is lonelier, never blinder."* The
/// wander changes where a Calm guard chooses to walk and **nothing else** — a guard
/// that sees the player still chases at the same speed, and a reactive guard's
/// destination is its lead, never a random cell.
#[test]
fn a_silenced_net_changes_where_a_guard_walks_and_nothing_else() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 6), Terrain::CommsConsole);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::South,
        vec![Guard::stationary(Cell::new(5, 2)).with_state(GuardState::Calm)],
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_rng(Rng::new(3));
    s.step(Input::Step(Direction::South)); // bump the console
    assert!(s.radio_silenced());

    // The watcher above is looking south down the column at the player, who is stood
    // in the open: it detects, and chases the cell it saw them in — not a random one.
    for _ in 0..3 {
        s.step(Input::Wait);
        if s.outcome() != Outcome::Playing {
            break;
        }
    }
    let guard = &s.guards()[0];
    assert_ne!(
        guard.state(),
        GuardState::Calm,
        "a silenced facility is lonelier, never blinder — it still sees you",
    );
}
