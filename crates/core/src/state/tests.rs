use super::*;
use crate::facility::Facility;
use crate::guard::{GuardState, CERTAIN_RANGE, GLIMPSE_RANGE, PATROL_RADIUS, SEARCH_RADIUS};
use crate::region::{DoorKind, RegionGraph, RegionKind};
use crate::targeting::Target;
use crate::test_support::{open_room, region_strip, solo};
use crate::vision::field_of_view;
use crate::{generate, generate_level, DoorId, Rng};

/// §7.3: a downed guard misses its radio ping a period after the takedown, and
/// control dispatches the nearest active guard to its last known post (→
/// [`Responding`](GuardState::Responding)); a second missed ping a period later
/// steps the facility-wide alert. Both surface on the near line (§11.4/§11.7).
/// A 1-wide corridor keeps the responder's patrol on a single predictable line.
#[test]
fn a_downed_guard_pings_a_dispatch_then_an_alert_step() {
    // The player starts in a cupboard so the adjacent victim's 360° touching
    // ring (§6.1) does not detect it — the takedown lands, and staying hidden
    // keeps the player safe while the radio ticks (contact is refused, §7.6).
    let mut layout = open_room(3, 30);
    layout.place(Cell::new(1, 1), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(1, 1),
        Direction::South,
        vec![
            // The victim, on a short 3-turn clock so the pings come quickly.
            Guard::stationary(Cell::new(1, 2)).with_radio_clock(radio::RadioClock::from_period(3)),
            // The only other guard: the one control will send.
            Guard::patrolling(Cell::new(1, 20)),
        ],
        Vec::new(),
        Cell::new(1, 28),
    );

    let e = s.step(Input::Step(Direction::South)); // take the victim down
    assert!(e.contains(&Event::TakenDown {
        at: Cell::new(1, 2)
    }));
    assert_eq!(s.guards().len(), 1, "only the responder remains");

    // A period's window, then the miss: no silence on the quiet turn before it.
    assert!(s
        .step(Input::Wait)
        .iter()
        .all(|e| !matches!(e, Event::RadioSilence { .. })));
    let dispatch = s.step(Input::Wait);
    assert!(
        dispatch.contains(&Event::RadioSilence {
            post: Cell::new(1, 2)
        }),
        "the first missed ping is a silence at the post",
    );
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Responding,
        "control dispatched the nearest active guard",
    );
    assert_eq!(s.alert(), 0, "one miss does not raise the alert");

    // Three more quiet turns: the second missed ping steps the facility alert.
    let mut stepped_to = None;
    for _ in 0..3 {
        for e in s.step(Input::Wait) {
            if let Event::AlertRaised { level } = e {
                stepped_to = Some(level);
            }
        }
    }
    assert_eq!(
        stepped_to,
        Some(radio::ALERT_STEP),
        "the second miss steps it"
    );
    assert_eq!(
        s.alert(),
        radio::ALERT_STEP,
        "the alert is written and readable"
    );
}

/// §7.3: the radio net bites only a guard that is *down*. A live guard answers
/// its pings, so a run with no takedown never dispatches and never steps the
/// alert, however long it runs.
#[test]
fn a_live_guard_answers_and_never_trips_the_net() {
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(2, 2),
        Direction::South,
        vec![Guard::patrolling(Cell::new(9, 9))],
        Vec::new(),
        Cell::new(10, 10),
    );
    for _ in 0..40 {
        for e in s.step(Input::Wait) {
            assert!(
                !matches!(e, Event::RadioSilence { .. } | Event::AlertRaised { .. }),
                "a live guard never trips the radio net",
            );
        }
    }
    assert_eq!(s.alert(), 0);
}

/// §7.3: a **hidden** body still misses its ping. Hiding a body confuses the
/// investigation — the responder walks to a post the body has left — but does
/// not cancel it: the radio never consults concealment. The body is dragged
/// into a cupboard (never found, cf. `a_body_dragged_into_a_hideout_is_gone`),
/// yet its ping still goes missed.
#[test]
fn a_hidden_body_still_misses_its_ping() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5), // start hidden, so the victim never sees the takedown coming
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4)).with_radio_clock(radio::RadioClock::from_period(5))],
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Step(Direction::North)); // takedown: body at (5,4)
    s.step(Input::Step(Direction::North)); // grab it
    s.step(Input::Step(Direction::South)); // step out: body follows into the cupboard (5,5)
    let body = Cell::new(5, 5);
    assert_eq!(s.bodies()[0].cell(), body);
    assert_eq!(
        s.layout().facility().terrain(body),
        Some(Terrain::Hideout),
        "the body is hidden in the cupboard",
    );

    let mut silenced = false;
    for _ in 0..4 {
        for e in s.step(Input::Wait) {
            if matches!(e, Event::RadioSilence { .. }) {
                silenced = true;
            }
            assert!(
                !matches!(e, Event::BodyFound { .. }),
                "a hidden body is never found",
            );
        }
    }
    assert!(silenced, "the hidden body still missed its ping (§7.3)");
    assert!(!s.bodies()[0].found(), "confusion, not cancellation");
}

/// §12.4: the radio net is deterministic — the same scenario under the same
/// inputs yields the identical event stream and alert level.
#[test]
fn the_radio_net_is_deterministic() {
    let build = || {
        let mut layout = open_room(3, 30);
        layout.place(Cell::new(1, 1), Terrain::Hideout);
        State::new(
            layout,
            Cell::new(1, 1),
            Direction::South,
            vec![
                Guard::stationary(Cell::new(1, 2))
                    .with_radio_clock(radio::RadioClock::from_period(3)),
                Guard::patrolling(Cell::new(1, 20)),
            ],
            Vec::new(),
            Cell::new(1, 28),
        )
    };
    let mut script = vec![Input::Step(Direction::South)];
    script.extend(std::iter::repeat_n(Input::Wait, 8));
    let run = |mut s: State| -> (Vec<Vec<Event>>, u32) {
        (script.iter().map(|&i| s.step(i)).collect(), s.alert())
    };
    assert_eq!(run(build()), run(build()), "same seed of events → same run");
}

#[test]
fn a_move_into_open_floor_spends_the_turn_and_turns_the_player() {
    let mut s = solo(Cell::new(4, 4));
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(5, 4)
        }]
    );
    assert_eq!(s.player(), Cell::new(5, 4));
    assert_eq!(s.facing(), Direction::East);
    assert_eq!(s.turn(), 1);
}

/// §4.4's load-bearing exception: bumping a wall is free — the turn does not
/// advance, the player does not move, and facing is unchanged (§5).
#[test]
fn bumping_a_wall_is_free_and_does_not_advance_the_turn() {
    let mut s = solo(Cell::new(1, 1));
    let events = s.step(Input::Step(Direction::West)); // into the west wall
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(0, 1)
        }]
    );
    assert_eq!(s.player(), Cell::new(1, 1), "no move");
    assert_eq!(s.facing(), Direction::North, "a blocked move keeps facing");
    assert_eq!(s.turn(), 0, "a free action does not spend the turn");
}

/// The §8.4 seam: opening a targeting session reads the ability's *declared*
/// mode (§8.1 catalog) and anchors it on the player's cell and facing (§5) —
/// Run self-targets, Decoy targets the faced cardinal — and a `Tile` mode hands
/// back a cursor on the player, never an auto-aim (§8.4's whole reason to exist).
#[test]
fn opening_a_targeting_session_reads_the_ability_mode_and_the_player() {
    // The solo player starts facing north.
    let s = solo(Cell::new(4, 4));
    // Run is self-targeted: resolves straight to the player's cell.
    assert_eq!(
        s.begin_ability_targeting(AbilityId::Run).confirm(),
        Target::Itself(Cell::new(4, 4)),
    );
    // Decoy is direction-targeted: defaults to the player's facing.
    assert_eq!(
        s.begin_ability_targeting(AbilityId::Decoy).confirm(),
        Target::Direction(Direction::North),
    );
    // A tile session (no v1 ability uses one) starts its cursor on the player.
    assert_eq!(
        s.begin_targeting(TargetingMode::Tile { range: 5 })
            .confirm(),
        Target::Tile(Cell::new(4, 4)),
    );
}

/// Waiting is a real action (§5): it spends the turn even though nothing moves.
#[test]
fn waiting_spends_the_turn() {
    let mut s = solo(Cell::new(4, 4));
    assert!(s.step(Input::Wait).is_empty());
    assert_eq!(s.turn(), 1);
    assert_eq!(s.player(), Cell::new(4, 4));
}

/// §4.4/§8.2: activating an ability is world-changing — it spends the turn and
/// reports it (§11.7). By the time the panel reads it, the activation turn's
/// end-of-turn tick has run, so 4 of Run's 5 remain — yet the activation turn
/// itself was protected (the §8.2 N-yields-N−1 trap, designed out).
#[test]
fn activating_an_ability_spends_the_turn() {
    let mut s = solo(Cell::new(4, 4));
    let events = s.step(Input::Activate(AbilityId::Run));
    assert_eq!(
        events,
        vec![Event::AbilityActivated {
            ability: AbilityId::Run
        }]
    );
    assert_eq!(s.turn(), 1, "activation spends the turn");
    assert_eq!(
        s.ability_state(AbilityId::Run),
        AbilityState::Active { remaining: 4 },
    );
}

/// The ability line/panel roster (§11.4): [`ability_statuses`](State::ability_statuses)
/// is exactly the economy deck, in deck order, each carrying its live slot state —
/// and the innate bump verbs Takedown and Drag are not in it (they speak through
/// the usable line, not the ability economy, §7.2/§8.3).
#[test]
fn ability_statuses_are_the_economy_deck_in_order() {
    let mut s = solo(Cell::new(4, 4));
    let ids: Vec<AbilityId> = s.ability_statuses().iter().map(|st| st.id).collect();
    assert_eq!(
        ids,
        AbilityId::ALL.to_vec(),
        "one row per economy ability, in order"
    );

    // Each row mirrors the live economy state.
    s.step(Input::Activate(AbilityId::Run));
    let run = s
        .ability_statuses()
        .into_iter()
        .find(|st| st.id == AbilityId::Run)
        .unwrap();
    assert_eq!(run.state, s.ability_state(AbilityId::Run));
    assert!(matches!(run.state, AbilityState::Active { .. }));
}

/// §4.4: toggling an ability off is one of the two free actions — the turn does
/// not advance — and it still pays the full cooldown (§8.2 refunds nothing).
#[test]
fn toggling_an_ability_off_is_free() {
    let mut s = solo(Cell::new(4, 4));
    s.step(Input::Activate(AbilityId::Run)); // turn 1, Run active
    let events = s.step(Input::Deactivate(AbilityId::Run));
    assert_eq!(
        events,
        vec![Event::AbilityDeactivated {
            ability: AbilityId::Run
        }]
    );
    assert_eq!(s.turn(), 1, "toggling off does not spend the turn");
    assert_eq!(
        s.ability_state(AbilityId::Run),
        AbilityState::Cooling { remaining: 12 },
        "early cancel still pays the whole cooldown",
    );
}

/// Activating an ability that is not ready is a mis-input — free, like a wall
/// bump (§4.4): nothing changes and the turn does not advance.
#[test]
fn activating_an_unavailable_ability_is_free() {
    let mut s = solo(Cell::new(4, 4));
    s.step(Input::Activate(AbilityId::Run)); // now active
    let events = s.step(Input::Activate(AbilityId::Run)); // already active
    assert!(events.is_empty(), "re-activating does nothing");
    assert_eq!(s.turn(), 1, "a mis-input is free");
}

/// The §8.2 timing convention through the whole loop: a freshly activated
/// N-turn ability is protected for N turns — the activation turn included —
/// then fades, and the full lockout is exactly `duration + cooldown` (Run: 5 +
/// 12 = 17 turns), Ready again on the 18th.
#[test]
fn an_ability_is_protected_for_its_full_duration_then_locked_out() {
    let mut s = solo(Cell::new(4, 4));
    s.step(Input::Activate(AbilityId::Run)); // protected turn 1; tick 1 of 17
    assert_eq!(
        s.ability_state(AbilityId::Run),
        AbilityState::Active { remaining: 4 }
    );

    // Protected turns 2–4 keep it active; the 4th wait's tick ends the duration.
    for expected in [3, 2, 1] {
        assert!(s.step(Input::Wait).is_empty());
        assert_eq!(
            s.ability_state(AbilityId::Run),
            AbilityState::Active {
                remaining: expected
            }
        );
    }
    let events = s.step(Input::Wait); // protected turn 5 ends here
    assert_eq!(
        events,
        vec![Event::AbilityExpired {
            ability: AbilityId::Run
        }]
    );
    assert_eq!(
        s.ability_state(AbilityId::Run),
        AbilityState::Cooling { remaining: 12 },
        "the frozen cooldown starts at its full 12",
    );

    // Cooldown drains one per turn: 11 more waits leave it locked, the 12th frees it.
    for _ in 0..11 {
        s.step(Input::Wait);
    }
    assert_ne!(
        s.ability_state(AbilityId::Run),
        AbilityState::Ready,
        "still cooling after 16 turns",
    );
    s.step(Input::Wait);
    assert_eq!(
        s.ability_state(AbilityId::Run),
        AbilityState::Ready,
        "Ready again after exactly duration + cooldown = 17 turns",
    );
}

/// Win path (§4.5): take every objective, then reach the exit. Bumping the exit
/// with intel still out refuses and is free.
#[test]
fn win_requires_all_intel_then_the_exit() {
    // Player at (4,4); one intel at (5,4); exit at (4,5).
    let mut s = State::new(
        open_room(10, 10),
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        [Cell::new(5, 4)],
        Cell::new(4, 5),
    );

    // Bumping the exit early: refused, free, still playing.
    let events = s.step(Input::Step(Direction::South));
    assert_eq!(events, vec![Event::ExitRefused]);
    assert_eq!(s.outcome(), Outcome::Playing);
    assert_eq!(s.turn(), 0);

    // Take the intel by bumping the console to the east.
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(events, vec![Event::IntelTaken { remaining: 0 }]);
    assert_eq!(s.objectives_remaining(), 0);
    assert_eq!(
        s.player(),
        Cell::new(4, 4),
        "taking intel is a bump, not a move"
    );

    // Now the exit accepts.
    let events = s.step(Input::Step(Direction::South));
    assert_eq!(events, vec![Event::Won]);
    assert_eq!(s.outcome(), Outcome::Won);

    // A finished run is inert.
    assert!(s.step(Input::Step(Direction::North)).is_empty());
}

/// Loss (§4.5): a guard moving into the player's cell captures. Contact, not
/// detection — the guard need not "see" anything.
#[test]
fn a_guard_stepping_into_the_player_captures() {
    // Guard at (6,4) heading west across the room; player at (4,4) in its path.
    // After the startup turn the guard is at (5,4); the player waits, and the
    // guard steps onto (4,4).
    let mut s = State::new(
        open_room(10, 10),
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(6, 4), Cell::new(1, 4))],
        Vec::new(),
        Cell::new(8, 8),
    );
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(5, 4),
        "startup turn moved the guard"
    );
    assert_eq!(s.outcome(), Outcome::Playing);

    // The wait turn: the guard's look from (5,4) freshly finds the adjacent
    // player (the touching ring, §6.1) — the Detected transition — and its
    // step onto them captures. Both facts are reported, in resolution order.
    let events = s.step(Input::Wait);
    assert_eq!(
        events,
        vec![
            Event::Detected {
                by: Cell::new(5, 4)
            },
            Event::Captured {
                by: Cell::new(4, 4)
            },
        ]
    );
    assert_eq!(s.outcome(), Outcome::Lost);
}

/// [`Event::Detected`] fires on the **transition** into being seen (§7.6),
/// not per turn of a held gaze: stepping into a guard's sight reports once,
/// staying in it reports nothing more, and only breaking contact re-arms it —
/// so the §13.2 sim counts broken stealth, never chase length.
#[test]
fn a_fresh_detection_is_reported_once_and_rearms_on_broken_contact() {
    // A stationary guard facing south; the player starts two cells to its
    // west — outside the ~90° wedge and past the touching ring — so the
    // startup turn sees nothing. (Directly behind would sit in the guard's
    // rear blind spot, §155, and never detect — hence the side approach.)
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(3, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 5))],
        Vec::new(),
        Cell::new(10, 10),
    );
    assert!(
        !s.guards()[0].detected_player(),
        "precondition: beside the cone at range, unseen"
    );

    // Step to the guard's side: the touching ring (§6.1) finds the player — a
    // side cell still detects (only the rear three do not, §155) — the
    // transition, reported.
    let events = s.step(Input::Step(Direction::East));
    assert!(
        events.contains(&Event::Detected {
            by: Cell::new(5, 5)
        }),
        "stepping into sight is a detection event: {events:?}"
    );

    // Held in sight: detected again this turn, but no *fresh* detection.
    let events = s.step(Input::Wait);
    assert!(s.guards()[0].detected_player(), "still seen");
    assert!(
        !events.iter().any(|e| matches!(e, Event::Detected { .. })),
        "a held gaze is not a new detection: {events:?}"
    );

    // Break contact — back out to the side, past the ring — then re-enter: a
    // second event.
    let events = s.step(Input::Step(Direction::West));
    assert!(!s.guards()[0].detected_player(), "contact broken");
    assert!(
        !events.iter().any(|e| matches!(e, Event::Detected { .. })),
        "losing the player is not a detection: {events:?}"
    );
    let events = s.step(Input::Step(Direction::East));
    assert!(
        events.contains(&Event::Detected {
            by: Cell::new(5, 5)
        }),
        "re-entering sight re-fires the event: {events:?}"
    );
}

/// Concealment gates the event exactly as it gates detection (§10.3): a
/// hidden player sweeps through a cone silently, and the event fires only
/// when they emerge into sight.
#[test]
fn concealment_suppresses_the_detection_event_until_the_player_emerges() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(4, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(4, 5), // in a cupboard beside the guard (a side cell)
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 5))],
        Vec::new(),
        Cell::new(10, 10),
    );
    assert!(s.hidden(), "precondition: concealed");

    let events = s.step(Input::Wait);
    assert!(
        !events.iter().any(|e| matches!(e, Event::Detected { .. })),
        "the cupboard conceals: no detection event: {events:?}"
    );

    // Climb out into the guard's forward view (a forward diagonal, in the wedge):
    // adjacent, exposed — the transition fires. Emerging into the rear blind spot
    // (§155) would not detect, so the exit is deliberately toward the front.
    let events = s.step(Input::Step(Direction::South));
    assert!(
        events.contains(&Event::Detected {
            by: Cell::new(5, 5)
        }),
        "emerging into the cone is a detection: {events:?}"
    );
}

/// §7.2: the takedown. Bumping an adjacent guard that has not detected the
/// player this turn removes it permanently, leaves a body at its cell, and
/// costs the full turn. Concealment is how adjacency is arranged undetected —
/// the touching ring otherwise always sees an adjacent player (§6.1) — so the
/// strike comes from inside a cupboard. The usable line offers exactly this.
#[test]
fn an_unaware_adjacent_guard_is_taken_down_leaving_a_body() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5), // in the cupboard: concealed from the start
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4))],
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(s.hidden(), "precondition: concealed");
    assert!(
        !s.guards()[0].detected_player(),
        "precondition: the guard's look missed the hidden player",
    );
    assert_eq!(
        s.affordances(),
        vec![(Direction::North, Affordance::Takedown)],
        "the usable line offers the takedown (§11.4)",
    );

    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::TakenDown {
            at: Cell::new(5, 4)
        }]
    );
    assert!(s.guards().is_empty(), "the takedown is permanent");
    assert_eq!(s.bodies().len(), 1, "a body is left behind");
    assert_eq!(s.bodies()[0].cell(), Cell::new(5, 4));
    assert_eq!(s.turn(), 1, "a takedown costs the full turn");
    assert_eq!(
        s.player(),
        Cell::new(5, 5),
        "a takedown is a bump, not a move"
    );
}

/// §7.2's gate, enforced: a guard that **has** detected the player this turn
/// refuses the takedown — the bump falls back to the free no-op, and the
/// usable line never offered it.
#[test]
fn an_aware_guard_refuses_the_takedown() {
    let mut s = State::new(
        open_room(10, 10),
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4))],
        Vec::new(),
        Cell::new(8, 8),
    );
    // The startup turn's touching ring saw the adjacent player (§6.1).
    assert!(s.guards()[0].detected_player(), "precondition: aware");
    assert_eq!(s.affordances(), Vec::new(), "no takedown is promised");

    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(5, 4)
        }]
    );
    assert_eq!(s.guards().len(), 1, "the guard stands");
    assert!(s.bodies().is_empty());
    assert_eq!(s.turn(), 0, "a refused takedown is a free bump");
}

/// §155 + §7.2: the behind-the-back takedown the rear blind spot exists for. A
/// guard faces south; the player stands directly behind it on **open floor** —
/// no cupboard, no decoy — and is undetected because the guard's rear three
/// cells no longer detect (§155). Bumping the guard from behind takes it down,
/// which the old 360° touching ring made impossible without concealment.
#[test]
fn a_guard_is_taken_down_from_directly_behind_on_open_floor() {
    let mut s = State::new(
        open_room(10, 10),
        Cell::new(5, 4), // directly behind the south-facing guard, exposed
        Direction::South,
        vec![Guard::stationary(Cell::new(5, 5))],
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(!s.hidden(), "precondition: on open floor, not concealed");
    assert!(
        !s.guards()[0].detected_player(),
        "precondition: the rear blind spot hides the player behind the guard",
    );
    assert_eq!(
        s.affordances(),
        vec![(Direction::South, Affordance::Takedown)],
        "the usable line offers the takedown from directly behind (§11.4)",
    );

    let events = s.step(Input::Step(Direction::South));
    assert_eq!(
        events,
        vec![Event::TakenDown {
            at: Cell::new(5, 5)
        }]
    );
    assert!(s.guards().is_empty(), "the takedown is permanent");
    assert_eq!(s.bodies().len(), 1, "a body is left behind");
    assert_eq!(s.bodies()[0].cell(), Cell::new(5, 5));
    assert_eq!(s.turn(), 1, "a takedown costs the full turn");
    assert_eq!(
        s.player(),
        Cell::new(5, 4),
        "a takedown is a bump, not a move"
    );
}

/// §7.2: a body does not block sight, so the first cone to cover it fires
/// the found-body event — exactly once, found is found — and the finder goes
/// hunting (the §7.6 search). The body is solid to the player: stepping into
/// it is a free bump.
#[test]
fn a_body_is_found_once_by_a_covering_cone() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5), // hidden, striking north
        Direction::North,
        vec![
            Guard::stationary(Cell::new(5, 4)), // the victim, adjacent
            // A witness two cells up the column, cone south straight over
            // the victim's cell: it sees the body the turn it appears.
            Guard::stationary(Cell::new(5, 2)),
        ],
        Vec::new(),
        Cell::new(8, 8),
    );

    let body = Cell::new(5, 4);
    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::TakenDown { at: body }, Event::BodyFound { at: body }],
        "the witness's cone covers the fresh body: found the same turn",
    );
    assert_eq!(s.bodies()[0].cell(), body);
    assert!(s.bodies()[0].found());
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Alerted,
        "the finder drops into the §7.6 search",
    );

    // Found is found: the cone keeps covering the body every turn, and the
    // loudest event in the game never repeats.
    for _ in 0..3 {
        let events = s.step(Input::Wait);
        assert!(
            !events.iter().any(|e| matches!(e, Event::BodyFound { .. })),
            "the found-body event fires exactly once per body",
        );
    }

    // Solid to the player: the step never moves onto the body — the bump is
    // the grab interaction instead (§8.3).
    let turn = s.turn();
    let events = s.step(Input::Step(Direction::North));
    assert_eq!(events, vec![Event::BodyGrabbed { at: body }]);
    assert_eq!(s.player(), Cell::new(5, 5), "no move onto a body");
    assert_eq!(s.turn(), turn + 1, "the grab spends the turn");
}

/// §7.2: a body is solid to guards too — it blocks their movement and their
/// pathing. A guard sent at the body's cell (and then hunting all around it)
/// never stands on it.
#[test]
fn a_guard_never_enters_a_bodys_cell() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(5, 4)), // the victim
            // A walker aimed straight at the victim's cell.
            Guard::patrolling_to(Cell::new(5, 1), Cell::new(5, 4)),
        ],
        Vec::new(),
        Cell::new(8, 8),
    );

    let body = Cell::new(5, 4);
    s.step(Input::Step(Direction::North)); // the takedown; the body lies
    assert_eq!(s.bodies()[0].cell(), body);

    // The walker finds the body, searches all around it — and can never
    // stand on it: not routed through (pathing) and never entered (move).
    for _ in 0..12 {
        s.step(Input::Wait);
        assert_ne!(s.guards()[0].pos(), body, "a body's cell admits no guard");
        assert_eq!(s.outcome(), Outcome::Playing, "hidden all along");
    }
}

/// §8.3 Dephase: while phased, solids are plain moves — the player walks
/// *into* a wall and *onto* a closed door panel without opening it — and
/// stepping back onto open floor before the duration ends is safe: the
/// expiry on floor is just the ability fading.
#[test]
fn dephased_movement_passes_through_solids_without_bumping() {
    // Through a wall (duration 3: activate, in, out — expiring on floor).
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Activate(AbilityId::Dephase));
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(5, 4)
        }],
        "a wall is a plain move while phased — no bump",
    );
    assert_eq!(s.player(), Cell::new(5, 4), "standing inside the wall");
    let events = s.step(Input::Step(Direction::East)); // out, onto floor
    assert_eq!(s.player(), Cell::new(6, 4));
    assert!(
        events.contains(&Event::AbilityExpired {
            ability: AbilityId::Dephase
        }),
        "the duration ends here",
    );
    assert_eq!(
        s.outcome(),
        Outcome::Playing,
        "expiry on open floor is safe"
    );

    // Onto a closed door panel: the door is not opened by a dephased step.
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::DoorPanelClosed);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Activate(AbilityId::Dephase));
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(5, 4)
        }],
        "no DoorOpened: you pass through, not into, the door",
    );
    assert_eq!(
        s.layout().facility().terrain(Cell::new(5, 4)),
        Some(Terrain::DoorPanelClosed),
        "the door stays closed",
    );
}

/// §8.3/§4.3: a guard is walk-through too — and the bump suppression means
/// no takedown fires on the way through: you pass straight through
/// everything, targets included.
#[test]
fn a_dephased_player_passes_through_a_guard_without_a_takedown() {
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4))],
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Activate(AbilityId::Dephase));
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(5, 4)
        }],
        "onto the guard's own cell: no takedown, no bump",
    );
    assert_eq!(s.guards().len(), 1, "the guard stands untouched");
    s.step(Input::Step(Direction::East)); // out the far side, expiry on floor
    assert_eq!(s.player(), Cell::new(6, 4));
    assert_eq!(s.outcome(), Outcome::Playing);
}

/// §8.3: the cost that keeps Dephase from being free — the duration running
/// out while the player stands inside a wall is **lethal**, a distinct loss
/// ([`Event::Entombed`], not the capture), with no auto-eject to safety.
#[test]
fn dephase_expiring_inside_a_wall_is_lethal() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Activate(AbilityId::Dephase)); // active turn 1
    s.step(Input::Step(Direction::East)); // turn 2: into the wall
    let events = s.step(Input::Wait); // turn 3: the duration ends in there
    assert_eq!(
        events,
        vec![
            Event::AbilityExpired {
                ability: AbilityId::Dephase
            },
            Event::Entombed {
                at: Cell::new(5, 4)
            },
        ]
    );
    assert_eq!(
        s.outcome(),
        Outcome::Lost,
        "rematerializing in a wall kills"
    );
    assert!(s.step(Input::Wait).is_empty(), "the run is over");
}

/// §8.3/§2.2: toggling Dephase off while inside a solid is **refused** — a
/// free no-op, because there is nowhere to rematerialize. The lethal
/// squeeze belongs to the duration alone, never to a mis-pressed key.
#[test]
fn toggling_dephase_off_inside_a_wall_is_refused() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Activate(AbilityId::Dephase));
    s.step(Input::Step(Direction::East)); // inside the wall
    let turn = s.turn();
    let events = s.step(Input::Deactivate(AbilityId::Dephase));
    assert!(events.is_empty(), "nowhere to solidify: refused");
    assert_eq!(s.turn(), turn, "and free, like every mis-input");
    assert!(
        matches!(
            s.ability_state(AbilityId::Dephase),
            AbilityState::Active { .. }
        ),
        "still phased",
    );
    s.step(Input::Step(Direction::East)); // out — the expiry lands on floor
    assert_eq!(s.outcome(), Outcome::Playing);
}

/// §8.3: dephased on the exit does **not** win — you cannot bump, so you
/// pass straight through the thing you came for. The tempting edge case,
/// pinned.
#[test]
fn a_dephased_player_cannot_win_by_standing_on_the_exit() {
    // No objectives: the exit is open — an ordinary bump here would win.
    let mut s = State::new(
        open_room(10, 10),
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(5, 4),
    );
    s.step(Input::Activate(AbilityId::Dephase));
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(5, 4)
        }],
        "onto the exit, not out by it: no Won while phased",
    );
    assert_eq!(s.outcome(), Outcome::Playing);
    s.step(Input::Step(Direction::East)); // step off before the squeeze
    assert_eq!(s.outcome(), Outcome::Playing, "expiry lands on open floor");
}

/// §8.3: Dephase does not conceal — a guard's cone still detects the
/// phased player — and §4.5 contact still captures: a guard walking into
/// the phased player ends the run with the ordinary capture, never the
/// entombment.
#[test]
fn dephase_conceals_nothing_and_contact_still_captures() {
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 6),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(5, 2), Cell::new(5, 9))],
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Activate(AbilityId::Dephase));
    assert!(
        s.guards()[0].detected_player(),
        "a dephased player in the cone is still seen — no concealment",
    );

    for _ in 0..4 {
        let events = s.step(Input::Wait);
        if s.outcome() == Outcome::Lost {
            assert!(
                events.contains(&Event::Captured {
                    by: Cell::new(5, 6)
                }),
                "the capture, not the entombment, is the loss here",
            );
            return;
        }
    }
    panic!("the guard should have walked into the phased player");
}

/// §8.3/§8.4: the decoy spawns in the **faced** cell (Direction targeting),
/// and a faced cell that could not hold an intruder — a wall — refuses the
/// activation as a free mis-input: no turn spent, no cooldown started.
#[test]
fn a_decoy_spawns_in_the_faced_cell_or_refuses() {
    let mut s = solo(Cell::new(7, 4));
    s.step(Input::Step(Direction::East)); // (8,4), facing the border wall
    let events = s.step(Input::Activate(AbilityId::Decoy));
    assert!(events.is_empty(), "a faced wall refuses: a free mis-input");
    assert_eq!(s.turn(), 1, "only the step spent a turn");
    assert_eq!(s.ability_state(AbilityId::Decoy), AbilityState::Ready);
    assert_eq!(s.decoy(), None);

    s.step(Input::Step(Direction::West)); // (7,4), facing open floor
    let events = s.step(Input::Activate(AbilityId::Decoy));
    assert_eq!(
        events,
        vec![Event::AbilityActivated {
            ability: AbilityId::Decoy
        }]
    );
    assert_eq!(s.decoy(), Some(Cell::new(6, 4)), "the faced cell");
    assert_eq!(s.turn(), 3, "a real activation spends the turn");
}

/// §8.3: a guard that has lost the player is drawn by the decoy — it flips
/// to Investigating toward the fake, walks in, and tramples it: the decoy
/// dies under its step, the ability pays the full cooldown, and the guard,
/// having found nothing, searches the area.
#[test]
fn a_guard_that_lost_the_player_investigates_and_tramples_the_decoy() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5), // concealed in the cupboard, facing north
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(2, 4), Cell::new(9, 4))],
        Vec::new(),
        Cell::new(10, 10),
    );
    assert_eq!(s.guards()[0].state(), GuardState::Calm, "nothing seen yet");

    s.step(Input::Activate(AbilityId::Decoy)); // the fake appears at (5,4)
    assert_eq!(s.decoy(), Some(Cell::new(5, 4)));
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Investigating,
        "the cone catches the fake: drawn to it, at chase-minus severity",
    );

    // It walks in and steps on it.
    let mut died = false;
    for _ in 0..4 {
        let events = s.step(Input::Wait);
        if events.iter().any(|e| matches!(e, Event::DecoyDied { .. })) {
            died = true;
            break;
        }
    }
    assert!(died, "anything stepping onto the decoy destroys it");
    assert_eq!(s.decoy(), None);
    assert!(
        matches!(
            s.ability_state(AbilityId::Decoy),
            AbilityState::Cooling { .. }
        ),
        "a trampled decoy still pays the full cooldown",
    );

    s.step(Input::Wait);
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Alerted,
        "the fake found out, the guard searches the area (§7.6)",
    );
}

/// §8.3's precedence, asserted: a guard that detected the player this turn
/// ignores the decoy entirely — decoys work on guards that have lost you,
/// never on guards that have you.
#[test]
fn a_guard_that_sees_the_player_ignores_the_decoy() {
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 6), // exposed, inside the stationary guard's cone
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 2))],
        Vec::new(),
        Cell::new(10, 10),
    );
    assert!(s.guards()[0].detected_player(), "precondition: it has you");
    assert_eq!(s.guards()[0].state(), GuardState::Chasing);

    s.step(Input::Activate(AbilityId::Decoy)); // the fake, inside its cone
    assert_eq!(s.decoy(), Some(Cell::new(5, 5)));
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Chasing,
        "a guard that can see you ignores the fake",
    );
}

/// §8.3: the maker's own step kills the decoy too, into the full cooldown —
/// and a decoy left alone fades with its ability's duration, the expiry
/// taking the fake with it.
#[test]
fn a_stepped_on_decoy_dies_and_an_expired_one_fades() {
    let mut s = solo(Cell::new(4, 4));
    s.step(Input::Step(Direction::East)); // (5,4), facing east
    s.step(Input::Activate(AbilityId::Decoy)); // decoy (6,4)
    let events = s.step(Input::Step(Direction::East)); // walk onto it
    assert_eq!(
        events,
        vec![
            Event::Moved {
                to: Cell::new(6, 4)
            },
            Event::DecoyDied {
                at: Cell::new(6, 4)
            },
        ]
    );
    assert_eq!(s.decoy(), None);
    assert!(
        matches!(
            s.ability_state(AbilityId::Decoy),
            AbilityState::Cooling { .. }
        ),
        "trampled: the full cooldown runs (§8.2 refunds nothing)",
    );

    // Wait out the cooldown, place a fresh one, and let it fade.
    for _ in 0..29 {
        s.step(Input::Wait);
    }
    assert_eq!(s.ability_state(AbilityId::Decoy), AbilityState::Ready);
    s.step(Input::Activate(AbilityId::Decoy)); // decoy (7,4), active turn 1
    assert_eq!(s.decoy(), Some(Cell::new(7, 4)));
    for _ in 0..18 {
        assert!(s.step(Input::Wait).is_empty());
    }
    let events = s.step(Input::Wait); // the 20th active turn ends here
    assert!(events.contains(&Event::AbilityExpired {
        ability: AbilityId::Decoy
    }));
    assert_eq!(s.decoy(), None, "expiry takes the fake with it");
}

/// The §8.2 golden test, through the whole loop (§8.3 Camouflage): a
/// standing player under a guard's cone is concealed for **exactly 10
/// turns, the activation turn included** — the "advertised 10, concealed 9,
/// visible on the activation turn" regression can never return silently —
/// and on the 11th the cone has them again.
#[test]
fn camouflage_conceals_for_its_full_duration_including_activation() {
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 6),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 2))],
        Vec::new(),
        Cell::new(10, 10),
    );
    // Control: exposed, the startup turn's cone detects the player.
    assert!(s.guards()[0].detected_player(), "precondition: in the cone");

    // Protected turn 1 is the activation itself.
    s.step(Input::Activate(AbilityId::Camouflage));
    assert!(
        !s.guards()[0].detected_player(),
        "the activation turn is protected — the old trap, designed out",
    );

    // Protected turns 2–10: still, swept every turn, never detected.
    for turn in 2..=10 {
        let events = s.step(Input::Wait);
        assert!(
            !s.guards()[0].detected_player(),
            "turn {turn}: still and unseen",
        );
        assert_eq!(
            events.contains(&Event::AbilityExpired {
                ability: AbilityId::Camouflage
            }),
            turn == 10,
            "the cloak fades at the end of protected turn 10, no earlier",
        );
    }

    // Turn 11: cooling, and the cone has the player again.
    s.step(Input::Wait);
    assert!(
        s.guards()[0].detected_player(),
        "advertised 10 yields 10 — and not an 11th",
    );
    assert!(matches!(
        s.ability_state(AbilityId::Camouflage),
        AbilityState::Cooling { .. }
    ));
}

/// §8.3: moving while camouflaged reveals the player **for that turn** —
/// the guard glimpses the movement — and stillness resumes the cloak the
/// very next turn.
#[test]
fn moving_while_camouflaged_reveals_for_that_turn_only() {
    // A tall room: the player cloaks beyond the cone's range, then walks in.
    let mut s = State::new(
        open_room(12, 20),
        Cell::new(5, 14),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 2))],
        Vec::new(),
        Cell::new(10, 18),
    );
    assert!(
        !s.guards()[0].detected_player(),
        "precondition: out of range"
    );
    s.step(Input::Activate(AbilityId::Camouflage));

    s.step(Input::Step(Direction::North)); // (5,13): moving, still out of range
    assert!(!s.guards()[0].detected_player());
    s.step(Input::Step(Direction::North)); // (5,12): in range, and moving
    assert!(
        s.guards()[0].detected_player(),
        "the turn you move, you are revealed",
    );

    s.step(Input::Wait);
    assert!(
        !s.guards()[0].detected_player(),
        "stillness resumes the cloak at once",
    );
}

/// §8.3/§4.5: camouflage does not stop capture. Capture is contact, not
/// detection — a guard walking into the cloaked player's cell catches them
/// without ever having seen them.
#[test]
fn camouflage_does_not_stop_capture_by_contact() {
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 6),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(5, 2), Cell::new(5, 9))],
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Activate(AbilityId::Camouflage));

    // The guard marches down the column into the standing, cloaked player.
    for _ in 0..4 {
        s.step(Input::Wait);
        if s.outcome() == Outcome::Lost {
            assert!(
                !s.guards()[0].detected_player(),
                "captured without ever being detected: invisible is not safe",
            );
            return;
        }
    }
    panic!("the guard should have walked into the cloaked player");
}

/// §7.6's designed relation, asserted so it can never silently drift: Run's
/// gain — one extra cell per active turn over its whole duration — is
/// exactly the certain→glimpse distance, the 5 cells that turn a Chasing
/// guard's certain track into a glimpse. Retuning Run means retuning the
/// zones, and vice versa; this test is the tripwire.
#[test]
fn runs_gain_is_the_certain_to_glimpse_distance() {
    assert_eq!(
        AbilityId::Run.def().duration(),
        GLIMPSE_RANGE - CERTAIN_RANGE,
        "Run's gain and the §7.6 zones are designed as a pair",
    );
}

/// The §8.3 golden loop: activating Run and stepping N times covers 2N
/// cells — both cells reported, one spent turn each — until the duration
/// expires at its §8.2 count (activation turn included), after which a step
/// covers 1 cell again and Run is cooling.
#[test]
fn run_doubles_steps_for_its_duration_then_reverts_and_cools() {
    let mut s = State::new(
        open_room(20, 10),
        Cell::new(2, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 8),
    );
    s.step(Input::Activate(AbilityId::Run)); // protected turn 1: no movement

    let mut x = 2;
    for _ in 0..4 {
        // Protected turns 2–5: every step is two cells, two Moved events.
        let turn = s.turn();
        let events = s.step(Input::Step(Direction::East));
        x += 2;
        assert_eq!(s.player(), Cell::new(x, 5));
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::Moved { .. }))
                .count(),
            2,
            "both cells of the sprint are reported",
        );
        assert_eq!(s.turn(), turn + 1, "a sprint step is one spent turn");
    }
    assert!(
        matches!(
            s.ability_state(AbilityId::Run),
            AbilityState::Cooling { .. }
        ),
        "5 protected turns (activation included) then the cooldown",
    );

    // Reverted: a step is one cell again.
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(11, 5));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(11, 5)
        }]
    );
}

/// The sprint's second cell must admit a **plain move**: anything else — a
/// wall, a cupboard — stops the sprint at one cell rather than auto-bumping.
/// A sprint never opens a door, never climbs into a cupboard, never touches
/// a guard the player didn't aim at (§8.4's no-auto-target spirit).
#[test]
fn the_sprint_stops_short_of_anything_it_would_bump() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(3, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Activate(AbilityId::Run));
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(4, 4), "the wall stops the sprint");
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(4, 4)
        }],
        "one move, and no bump against the wall ahead",
    );

    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 8), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(3, 8),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Activate(AbilityId::Run));
    s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(4, 8), "stops beside the cupboard");
    assert!(!s.hidden(), "a sprint never climbs in unasked");
}

/// §8.3/#103, the interaction stated and pinned: Run and Drag never stack.
/// While dragging, the extra step is suppressed — movement caps at the
/// drag's half speed, Run active or not.
#[test]
fn run_never_stacks_with_dragging() {
    let mut s = dragging_a_body(); // player (6,4), dragging the body at (5,4)
    s.step(Input::Activate(AbilityId::Run));

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(7, 4), "one cell — no fast dragging");
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(7, 4)
        }]
    );
    assert_eq!(s.bodies()[0].cell(), Cell::new(6, 4), "the body follows");

    s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(7, 4), "the haul debt holds under Run");
}

/// The drag scenario (§8.3): the cupboard takedown, then a walk out and
/// around to stand on open floor east of the body, and the grab — the bump
/// that takes hold. Ends with the player at (6,4) dragging the body at (5,4).
fn dragging_a_body() -> State {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4))],
        Vec::new(),
        Cell::new(8, 8),
    );
    s.step(Input::Step(Direction::North)); // takedown: the body at (5,4)
    s.step(Input::Step(Direction::East)); // out of the cupboard to (6,5)
    s.step(Input::Step(Direction::North)); // (6,4), beside the body
    let events = s.step(Input::Step(Direction::West)); // the grab
    assert_eq!(
        events,
        vec![Event::BodyGrabbed {
            at: Cell::new(5, 4)
        }]
    );
    assert_eq!(s.dragging(), Some(Cell::new(5, 4)));
    s
}

/// §8.3: "you move at half speed while dragging", by the documented debt
/// convention: a dragging move succeeds and leaves a haul debt, the next
/// step is spent but stationary, and the one after moves again — one cell
/// per two spent turns, with the body following into each vacated cell.
/// Grabbing itself costs the turn; releasing is free.
#[test]
fn dragging_moves_at_half_speed_and_the_body_follows() {
    let mut s = dragging_a_body();
    assert_eq!(s.turn(), 4, "takedown, two steps, and the grab all spend");

    // First drag-move: a full step, the body following into the vacated cell.
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(7, 4)
        }]
    );
    assert_eq!(s.bodies()[0].cell(), Cell::new(6, 4), "the body follows");

    // The next step owes the haul: spent, stationary, and silent.
    let events = s.step(Input::Step(Direction::East));
    assert!(events.is_empty(), "the debt turn narrates nothing");
    assert_eq!(s.player(), Cell::new(7, 4), "no movement on the debt turn");
    assert_eq!(s.turn(), 6, "but the turn is spent");

    // Debt paid: the next step moves — 2 cells in 4 turns, half speed.
    s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(8, 4));
    assert_eq!(s.bodies()[0].cell(), Cell::new(7, 4));
}

/// §8.3/§4.4: release is free and refunds nothing — the bump against the
/// held body lets it go where it lies, the turn does not advance, and the
/// player moves at full speed again while the body stays put.
#[test]
fn releasing_the_body_is_free_and_it_stays_where_it_lies() {
    let mut s = dragging_a_body();
    s.step(Input::Step(Direction::East)); // body to (6,4), player (7,4)
    let turn = s.turn();

    let events = s.step(Input::Step(Direction::West)); // bump the held body
    assert_eq!(
        events,
        vec![Event::BodyReleased {
            at: Cell::new(6, 4)
        }]
    );
    assert_eq!(s.turn(), turn, "release is free");
    assert_eq!(s.dragging(), None);

    // Full speed again — consecutive steps both move — and the body stays.
    s.step(Input::Step(Direction::North));
    s.step(Input::Step(Direction::North));
    assert_eq!(s.player(), Cell::new(7, 2), "no lingering debt");
    assert_eq!(s.bodies()[0].cell(), Cell::new(6, 4), "the body stays put");
}

/// While dragging, the usable line offers the release on the held body; a
/// second body reads as just solid (one body at a time), and a wall bump
/// stays free without moving anything (§4.4 — cannot drag through a wall).
#[test]
fn dragging_affordances_and_walls() {
    let mut s = dragging_a_body();
    assert_eq!(
        s.affordances(),
        vec![(Direction::West, Affordance::ReleaseBody)],
        "the held body offers the release",
    );

    // Haul north to the border wall: move (debt) — bump — move…
    s.step(Input::Step(Direction::North)); // (6,3), body (6,4)
    s.step(Input::Step(Direction::North)); // debt
    s.step(Input::Step(Direction::North)); // (6,2), body (6,3)
    s.step(Input::Step(Direction::North)); // debt
    s.step(Input::Step(Direction::North)); // (6,1), body (6,2)
    let turn = s.turn();
    let events = s.step(Input::Step(Direction::North)); // the border wall
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(6, 0)
        }]
    );
    assert_eq!(s.turn(), turn, "a wall bump while dragging is still free");
    assert_eq!(s.player(), Cell::new(6, 1));
    assert_eq!(s.bodies()[0].cell(), Cell::new(6, 2), "the body holds too");
}

/// §7.2's hide payoff, made possible here (§8.3): walk the body through the
/// cupboard — it follows into every vacated cell, so stepping out the far
/// side deposits it inside — then let go. A hidden body is *gone*: a guard
/// whose cone sweeps the cupboard finds nothing, ever.
#[test]
fn a_body_dragged_into_a_hideout_is_gone() {
    let mut layout = open_room(12, 24);
    layout.place(Cell::new(5, 5), Terrain::Hideout); // the body's cupboard
    layout.place(Cell::new(5, 7), Terrain::Hideout); // the player's own
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(5, 4)), // the victim
            // A witness marching up the column, far enough that the player
            // is hidden again before its cone arrives; it ends watching
            // both cupboards.
            Guard::patrolling_to(Cell::new(5, 21), Cell::new(5, 9)),
        ],
        Vec::new(),
        Cell::new(10, 22),
    );

    s.step(Input::Step(Direction::North)); // takedown: body at (5,4)
    s.step(Input::Step(Direction::North)); // grab it from the cupboard
    s.step(Input::Step(Direction::South)); // step out: body follows into (5,5)
    let body = Cell::new(5, 5);
    assert_eq!(s.bodies()[0].cell(), body, "deposited in the cupboard");
    assert_eq!(
        s.layout().facility().terrain(body),
        Some(Terrain::Hideout),
        "a body can occupy a hideout cell",
    );

    let events = s.step(Input::Step(Direction::North)); // let it go — free
    assert_eq!(events, vec![Event::BodyReleased { at: body }]);
    s.step(Input::Step(Direction::South)); // duck into the second cupboard
    assert!(s.hidden());

    // The witness arrives and sweeps both cupboards: the hidden body fires
    // nothing, the hidden player is not seen, and nothing ever escalates.
    for _ in 0..12 {
        let events = s.step(Input::Wait);
        assert!(
            !events.iter().any(|e| matches!(e, Event::BodyFound { .. })),
            "a body in a hideout is gone (§7.2) — no cone finds it",
        );
        assert_eq!(s.outcome(), Outcome::Playing);
    }
    assert!(!s.bodies()[0].found());
}

/// A patrolling guard with nowhere to sweep holds rather than wedging or
/// panicking (§7.5). A patrol routes *around* walls, so the old "march into the
/// wall forever" case cannot arise; the modern equivalent is a guard boxed into
/// a single cell — its territory is just itself, so it never leaves.
#[test]
fn a_boxed_in_guard_has_nowhere_to_patrol_and_holds() {
    // Wall a guard into the single cell (2,2): all four neighbours are solid.
    let mut layout = open_room(10, 10);
    for wall in [
        Cell::new(1, 2),
        Cell::new(3, 2),
        Cell::new(2, 1),
        Cell::new(2, 3),
    ] {
        layout.place(wall, Terrain::Wall);
    }
    let mut s = State::new(
        layout,
        Cell::new(6, 6),
        Direction::North,
        vec![Guard::patrolling(Cell::new(2, 2))],
        Vec::new(),
        Cell::new(8, 8),
    );
    // Startup already ran a decide; a few more waits never move it off (2,2).
    for _ in 0..3 {
        s.step(Input::Wait);
    }
    assert_eq!(s.guards()[0].pos(), Cell::new(2, 2));
    assert_eq!(s.outcome(), Outcome::Playing);
}

/// §7.5: a Calm guard genuinely paces across its territory rather than shuffling
/// by its spawn — over a patrol it reaches a cell well away from its station.
#[test]
fn a_calm_guard_paces_across_its_territory() {
    let station = Cell::new(15, 15);
    let mut s = State::new(
        open_room(30, 30),
        Cell::new(1, 28), // player parked in a far corner, out of the territory
        Direction::North,
        vec![Guard::patrolling(station)],
        Vec::new(),
        Cell::new(1, 1),
    );
    let mut farthest = 0;
    for _ in 0..40 {
        s.step(Input::Wait);
        farthest = farthest.max(station.manhattan_distance(s.guards()[0].pos()));
    }
    assert!(
        farthest > PATROL_RADIUS / 2,
        "the guard paced across its territory (reached {farthest} from station)",
    );
    assert_eq!(
        s.outcome(),
        Outcome::Playing,
        "the far player is never reached"
    );
}

/// §7.5/§153 end to end: through the real turn loop a Calm guard forced to dwell
/// (the playtest knob at 100) holds its cell without moving on the turns it
/// dwells; the same guard with the knob at 0 never dwells at all. The player is
/// parked in a far corner, out of the territory, so the guard stays Calm.
#[test]
fn a_calm_guard_dwells_through_the_turn_loop_and_the_knob_disables_it() {
    let build = |chance: u32| {
        // A small room so the guard reaches its patrol targets often (frequent
        // arrivals = frequent dwell rolls), and a concealed player in a cupboard so
        // the guard stays Calm no matter how close its sweep passes.
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(1, 1), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(1, 1), // hidden in the corner cupboard — never detected
            Direction::North,
            vec![Guard::patrolling(Cell::new(6, 6))],
            Vec::new(),
            Cell::new(10, 10),
        )
        .with_rng(Rng::new(5));
        s.set_guard_dwell_chance(chance);
        s
    };

    // Forced on: the guard dwells at some point, and every turn it dwells it holds
    // its cell (§5 — no move, no re-aim), staying Calm throughout.
    let mut s = build(100);
    let mut dwelt = false;
    for _ in 0..60 {
        let before = s.guards()[0].pos();
        s.step(Input::Wait);
        assert_eq!(
            s.guards()[0].state(),
            GuardState::Calm,
            "the concealed player never disturbs the patrol",
        );
        if s.guards()[0].is_dwelling() {
            dwelt = true;
            assert_eq!(
                s.guards()[0].pos(),
                before,
                "a dwelling guard does not move"
            );
        }
    }
    assert!(
        dwelt,
        "with the knob at 100 the guard dwells over its patrol"
    );

    // Forced off: the guard never dwells.
    let mut s = build(0);
    for _ in 0..60 {
        s.step(Input::Wait);
        assert!(
            !s.guards()[0].is_dwelling(),
            "the knob at 0 disables dwelling entirely",
        );
    }
}

/// §10.4: a closed door does not stop a guard — its route runs straight
/// through, and walking into the panel is the bump that opens it. The door is
/// the guard's whole action that turn; it steps through on the next. Guard traffic
/// opens the facility up over a level; the close-behind (#146) is exercised on its
/// own below, so this test turns it off to isolate the opening and pass-through.
#[test]
fn a_guard_walking_its_route_opens_the_door_and_passes_through() {
    let layout = region_strip();
    let panel = Cell::new(4, 2);
    let door = layout.regions().door_at(panel).unwrap();
    let mut s = State::new(
        layout,
        Cell::new(13, 4), // parked in corridor D, two closed doors away
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(2, 2), Cell::new(6, 2))],
        Vec::new(),
        Cell::new(13, 1),
    );
    s.set_guard_close_chance(0); // isolate opening/pass-through from the close (#146)
                                 // The startup turn walked the guard up against the closed panel.
    assert_eq!(s.guards()[0].pos(), Cell::new(3, 2));
    assert!(!s.layout().regions().door(door).is_open());

    // Its next step is *into* the panel: the walk-in opens the door instead.
    let events = s.step(Input::Wait);
    assert!(events.contains(&Event::DoorOpened { at: panel }));
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(3, 2),
        "the door was the turn"
    );
    assert!(s.layout().regions().door(door).is_open());

    // Then it walks through the doorway into the corridor, door left open.
    s.step(Input::Wait);
    assert_eq!(s.guards()[0].pos(), panel, "onto the open panel");
    s.step(Input::Wait);
    assert_eq!(s.guards()[0].pos(), Cell::new(5, 2), "into the corridor");
    assert!(
        s.layout().regions().door(door).is_open(),
        "close turned off: the door stays open behind the guard",
    );
    assert_eq!(s.outcome(), Outcome::Playing);
}

/// §7.5/§10.5 on a fixture level: a guard whose beat is room A, corridor C
/// and room B sweeps *through* the corridor into the far room — opening the
/// doors on its way — and never leaves its beat: over a bounded number of
/// turns its walk covers corridor cells between its rooms, and the door out
/// of the beat is never touched.
#[test]
fn a_region_beat_carries_the_patrol_across_corridors_and_rooms() {
    let layout = region_strip();
    let station = Cell::new(2, 2);
    let beat = crate::beat::beat_cells(layout.regions(), station, 3);
    let door_a = layout.regions().door_at(Cell::new(4, 2)).unwrap();
    let door_b = layout.regions().door_at(Cell::new(7, 2)).unwrap();
    let door_d = layout.regions().door_at(Cell::new(11, 2)).unwrap();
    let mut s = State::new(
        layout,
        Cell::new(13, 4), // parked in corridor D, outside the beat
        Direction::North,
        vec![Guard::patrolling(station).with_beat(beat.clone())],
        Vec::new(),
        Cell::new(13, 1),
    );
    s.set_guard_close_chance(0); // isolate beat coverage from the close-behind (#146)

    let (mut corridor, mut far_room) = (false, false);
    for _ in 0..80 {
        s.step(Input::Wait);
        let pos = s.guards()[0].pos();
        corridor |= (5..=6).contains(&pos.x);
        far_room |= (8..=10).contains(&pos.x);
        assert!(
            beat.contains(&pos) || pos == Cell::new(4, 2) || pos == Cell::new(7, 2),
            "the sweep stays on its beat (guard at {pos:?})",
        );
    }
    assert!(corridor, "the sweep covered the corridor between its rooms");
    assert!(far_room, "the sweep crossed into the far room");
    assert!(
        s.layout().regions().door(door_a).is_open() && s.layout().regions().door(door_b).is_open(),
        "the sweep opened the doors on its beat",
    );
    assert!(
        !s.layout().regions().door(door_d).is_open(),
        "the door out of the beat is never touched",
    );
    assert_eq!(s.outcome(), Outcome::Playing, "the parked player is safe");
}

/// §10.4/#146: a Calm guard that walks fully through a hinged door closes it
/// behind itself — the counter-pressure to guard traffic's monotonic opening, so
/// an open door stays evidence someone passed. The close-behind chance is forced
/// to 100 to make the *sometimes* certain for the assertion; the probability
/// itself is pinned in guard.rs and swept for determinism elsewhere. The shut
/// surfaces as a [`DoorClosed`](Event::DoorClosed) event the player can read.
#[test]
fn a_calm_guard_closes_the_door_behind_itself() {
    let layout = region_strip();
    let panel = Cell::new(4, 2);
    let door = layout.regions().door_at(panel).unwrap();
    let mut s = State::new(
        layout,
        Cell::new(13, 4), // parked in corridor D, well clear of the door
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(2, 2), Cell::new(6, 2))],
        Vec::new(),
        Cell::new(13, 1),
    );
    s.set_guard_close_chance(100); // make the "sometimes" certain for the test

    // Startup parked the guard against the closed panel (§10.4).
    assert_eq!(s.guards()[0].pos(), Cell::new(3, 2));

    s.step(Input::Wait); // the walk-in opens the door; the guard holds
    assert!(s.layout().regions().door(door).is_open());
    s.step(Input::Wait); // steps onto the open panel
    assert_eq!(s.guards()[0].pos(), panel);
    assert!(
        s.layout().regions().door(door).is_open(),
        "still in the throat: nothing to close behind yet",
    );

    // Stepping clear of the panel: the Calm guard shuts the door behind itself.
    let events = s.step(Input::Wait);
    assert_eq!(s.guards()[0].pos(), Cell::new(5, 2), "into the corridor");
    assert!(
        !s.layout().regions().door(door).is_open(),
        "the guard closed the door behind itself",
    );
    assert!(
        events.contains(&Event::DoorClosed { at: panel }),
        "the shut surfaces as an event",
    );
    assert_eq!(s.outcome(), Outcome::Playing);
}

/// §10.4/#146 end-to-end: on real generated geometry, with the close-behind
/// certain, Calm guards patrolling their beats do shut doors behind them — the
/// wiring fires on the corridor-first facility, not just the hand-built strip.
#[test]
fn guard_close_behind_fires_on_generated_levels() {
    use crate::test_support::seed_sweep;
    let mut any_close = false;
    for seed in seed_sweep(32) {
        let mut rng = Rng::new(seed);
        let (layout, placement) =
            generate_level(&crate::LevelConfig::V1, &mut rng).expect("v1 generates");
        let guards = placement.guards(&layout);
        let mut s = State::new(
            layout,
            placement.player(),
            Direction::North,
            guards,
            placement.intel().iter().copied(),
            placement.exit(),
        )
        .with_rng(rng);
        s.set_guard_close_chance(100);

        for _ in 0..200 {
            if s.outcome() != Outcome::Playing {
                break;
            }
            if s.step(Input::Wait)
                .iter()
                .any(|e| matches!(e, Event::DoorClosed { .. }))
            {
                any_close = true;
                break;
            }
        }
        if any_close {
            break;
        }
    }
    assert!(
        any_close,
        "a Calm patrol closes a door behind itself somewhere in the sweep",
    );
}

/// A hand-built state whose two rooms are joined by one **automatic** door
/// (§10.4/#147) with close `delay` — a frameless 3-panel span down wall column 3,
/// no hinges. The player starts in the left room facing the door; no guards. The
/// fixture for the auto-close timer in the running loop.
fn auto_door_state(delay: u32) -> (State, DoorId) {
    let cells = |xs: std::ops::Range<u32>| {
        xs.flat_map(|x| (1..4).map(move |y| Cell::new(x, y)))
            .collect::<Vec<_>>()
    };
    let mut f = Facility::walled_box(7, 5);
    let mut g = RegionGraph::new(7, 5);
    let left = g.add_region(RegionKind::Room, cells(1..3));
    let right = g.add_region(RegionKind::Room, cells(4..6));
    let panels: Vec<Cell> = (1..4).map(|y| Cell::new(3, y)).collect();
    for &p in &panels {
        f.set_terrain(p.x, p.y, Terrain::DoorPanelClosed);
    }
    let door = g.add_door(left, right, [], panels, DoorKind::Automatic { delay });
    let s = State::new(
        Layout::from_parts(f, g),
        Cell::new(2, 2), // left room, next to the panel at (3,2)
        Direction::East, // facing the door
        Vec::new(),
        Vec::new(),
        Cell::new(4, 3), // exit parked in the right room, unused
    );
    (s, door)
}

/// §10.4/#147 in the loop: an automatic door the player opens shuts itself a few
/// turns after the doorway is vacated, with no hand needed — and the shut reaches
/// the player as a [`DoorClosed`](Event::DoorClosed) event.
#[test]
fn an_automatic_door_closes_itself_in_the_loop() {
    let (mut s, door) = auto_door_state(3);
    let panel = Cell::new(3, 2);

    // Bump the closed panel: it opens (§4.3), and the countdown is armed.
    let opened = s.step(Input::Step(Direction::East));
    assert!(opened.contains(&Event::DoorOpened { at: panel }));
    assert!(s.layout().regions().door(door).is_open());
    assert_eq!(s.player(), Cell::new(2, 2), "the bump opened, did not move");

    // Waiting clear of the doorway, it times out and shuts on its own.
    let e1 = s.step(Input::Wait);
    assert!(s.layout().regions().door(door).is_open(), "still open");
    assert!(!e1.iter().any(|e| matches!(e, Event::DoorClosed { .. })));
    let e2 = s.step(Input::Wait);
    assert!(
        !s.layout().regions().door(door).is_open(),
        "the automatic door timed out and shut itself",
    );
    assert!(
        e2.iter().any(|e| matches!(e, Event::DoorClosed { .. })),
        "the shut reaches the player as an event",
    );
}

/// §10.4/#147: an automatic door never crushes — the player standing in the
/// doorway holds it open indefinitely, and it only times out once they step clear.
#[test]
fn an_automatic_door_never_shuts_on_the_player_in_the_doorway() {
    let (mut s, door) = auto_door_state(2);

    s.step(Input::Step(Direction::East)); // open it
    s.step(Input::Step(Direction::East)); // step into the doorway (onto the panel)
    assert_eq!(s.player(), Cell::new(3, 2), "standing on the panel");

    // Wait in the throat far longer than the delay: it will not close on the player.
    for _ in 0..8 {
        s.step(Input::Wait);
        assert!(
            s.layout().regions().door(door).is_open(),
            "the door is held open by the player in it",
        );
    }

    // Step clear into the far room, and it times out. Leaving the panel is itself
    // the first vacant tick (delay 2: 2 → 1), so it shuts on the next turn.
    s.step(Input::Step(Direction::East)); // onto (4,2): vacant tick 1
    assert!(s.layout().regions().door(door).is_open(), "one tick left");
    let shut = s.step(Input::Wait); // vacant tick 2 → closes
    assert!(!s.layout().regions().door(door).is_open());
    assert!(
        shut.iter().any(|e| matches!(e, Event::DoorClosed { .. })),
        "it times out once the doorway is finally clear",
    );
}

/// §10.4/#147: an automatic door offers **open** from the usable line when closed,
/// and *no close affordance* when open (there is no hinge to bump) — you simply
/// walk through it. The whole point of the frameless span.
#[test]
fn an_automatic_door_offers_open_but_never_close() {
    let (mut s, _door) = auto_door_state(3);

    // Closed and faced: the usable line offers "door: open" to the east.
    assert!(
        s.affordances()
            .contains(&(Direction::East, Affordance::OpenDoor)),
        "a closed automatic door offers open",
    );

    s.step(Input::Step(Direction::East)); // open it; the player stays put
                                          // Open now: the east cell is a walk-through, so no door affordance at all —
                                          // and a close is never offered, because an automatic door has no handle.
    let affs = s.affordances();
    assert!(
        !affs.iter().any(|(_, a)| *a == Affordance::CloseDoor),
        "an automatic door never offers close",
    );
    assert!(
        !affs
            .iter()
            .any(|(d, a)| *d == Direction::East && *a == Affordance::OpenDoor),
        "an open automatic door is walked through, not re-opened",
    );
}

/// §12.4: same seed → same beats, same sweeps. Two states built from the same
/// seed stay in lockstep through a long patrol — guard positions and door
/// states alike, turn for turn.
#[test]
fn beats_and_sweeps_are_deterministic_from_the_seed() {
    for seed in [3, 11] {
        let build = || {
            // Thread the one seed end to end (§12.4): the carve stream continues
            // into the loop, so the guard close-behind roll (#146) and the patrol
            // dwell roll (§153) are part of what this pins deterministic — same seed
            // → same closes and same dwells, turn for turn.
            let mut rng = Rng::new(seed);
            let (layout, p) =
                generate_level(&crate::LevelConfig::V1, &mut rng).expect("the v1 config generates");
            let guards = p.guards(&layout);
            State::new(
                layout,
                p.player(),
                Direction::North,
                guards,
                p.intel().iter().copied(),
                p.exit(),
            )
            .with_rng(rng)
        };
        let (mut a, mut b) = (build(), build());
        for turn in 0..60 {
            a.step(Input::Wait);
            b.step(Input::Wait);
            let pos = |s: &State| -> Vec<Cell> { s.guards().iter().map(|g| g.pos()).collect() };
            assert_eq!(pos(&a), pos(&b), "seed {seed}, turn {turn}: positions");
            let doors = |s: &State| -> Vec<bool> {
                s.layout()
                    .regions()
                    .doors()
                    .map(|(_, d)| d.is_open())
                    .collect()
            };
            assert_eq!(doors(&a), doors(&b), "seed {seed}, turn {turn}: doors");
        }
    }
}

/// Bumping a closed door opens it and spends the turn (§4.3, §10.4). Uses a
/// generated facility, which is where real doors live: stand on a floor cell next
/// to a panel and step into it.
#[test]
fn bumping_a_closed_door_opens_it() {
    let layout = generate(40, 40, &mut Rng::new(7)).unwrap();
    let (id, panel) = {
        let (id, door) = layout.regions().doors().next().unwrap();
        (id, door.panels()[0])
    };

    // One of the four orthogonal approaches stands on floor and bumps the panel.
    let opened = Direction::ALL.into_iter().any(|dir| {
        let Some(from) = panel.step(dir.opposite()) else {
            return false;
        };
        if !layout.facility().can_enter(from, ACTOR_FILL) {
            return false;
        }
        let mut s = State::new(
            layout.clone(),
            from,
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(1, 1),
        );
        let opened = s.step(Input::Step(dir)) == vec![Event::DoorOpened { at: panel }];
        if opened {
            assert!(s.layout().regions().door(id).is_open());
            assert_eq!(s.turn(), 1, "opening a door spends the turn");
        }
        opened
    });
    assert!(opened, "one approach must bump the panel open");
}

/// §10.4: **a door never closes on an actor** — doors don't crush. Standing on a
/// panel and bumping the hinge to shut the door must be refused, leaving the door
/// open and the panel walk-through. (Regression: the close check once consulted
/// only guards, so a player on a panel got shut in on themselves.)
#[test]
fn a_door_will_not_close_on_the_player() {
    // Find a door across seeds whose panel can be reached from a perpendicular
    // floor cell and has a hinge adjacent along the door line, then try to shut it
    // on ourselves.
    for seed in 0..64 {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let Some((id, from, into, panel, hinge_dir)) = crush_scenario(&layout) else {
            continue;
        };

        // Exit parked on the border corner (always wall, never walked): a valid
        // Cell we never touch, so stamping it can't disturb the door.
        let mut s = State::new(
            layout,
            from,
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(0, 0),
        );

        // Open the door, then step onto the now-open panel.
        assert_eq!(
            s.step(Input::Step(into)),
            vec![Event::DoorOpened { at: panel }]
        );
        assert_eq!(s.step(Input::Step(into)), vec![Event::Moved { to: panel }]);
        assert_eq!(s.player(), panel);

        // Bump the hinge to close: refused — we're on a panel. Nothing changes.
        let events = s.step(Input::Step(hinge_dir));
        assert!(events.is_empty(), "a refused close is a free no-op");
        assert!(
            s.layout().regions().door(id).is_open(),
            "seed {seed}: the door shut on the player"
        );
        assert_eq!(
            s.layout().facility().terrain(panel),
            Some(Terrain::DoorPanelOpen),
            "seed {seed}: the panel went solid under the player — crushed"
        );
        assert_eq!(s.player(), panel, "the player is unmoved and uncrushed");
        return;
    }
    panic!("no door with a reachable end panel found in 64 seeds");
}

/// A door setup for the crush test: a door id, the floor cell to start on, the
/// direction to step into the panel, the end panel itself, and the direction from
/// that panel to its adjacent hinge (what you bump to close).
fn crush_scenario(layout: &Layout) -> Option<(DoorId, Cell, Direction, Cell, Direction)> {
    for (id, door) in layout.regions().doors() {
        let panel = door.panels()[0];
        // The end panel abuts a hinge; the door line runs panel→hinge.
        let Some(&hinge) = door
            .hinges()
            .iter()
            .find(|&&h| panel.manhattan_distance(h) == 1)
        else {
            continue;
        };
        let Some(hinge_dir) = Direction::between(panel, hinge) else {
            continue;
        };
        // Approach the panel perpendicular to the door line, from floor.
        for perp in hinge_dir.perpendicular() {
            let Some(from) = panel.step(perp) else {
                continue;
            };
            let f = layout.facility();
            if f.terrain(from) == Some(Terrain::Floor) && f.can_enter(from, ACTOR_FILL) {
                return Some((id, from, perp.opposite(), panel, hinge_dir));
            }
        }
    }
    None
}

/// #148: bumping a *closed hinge* from beside the frame opens the door and turns
/// the player to face **along the door line, toward the panels**, so the #121
/// head-lean peek reads through the doorway from cover — you crack the door and
/// see the room beyond without ever stepping into the new sightline.
#[test]
fn a_frame_bump_opens_the_door_and_auto_faces_the_peek() {
    // region_strip: a vertical door in column 4 joins room A (cols 1–3) to
    // corridor C (cols 5–6); hinges at (4,1) and (4,3), panel at (4,2).
    let hinge = Cell::new(4, 1);
    let panel = Cell::new(4, 2);
    let mut s = State::new(
        region_strip(),
        Cell::new(3, 1), // beside the top hinge, in room A
        Direction::East, // arbitrary prior facing — the frame bump overrides it
        Vec::new(),
        Vec::new(),
        Cell::new(0, 0), // exit parked on the border corner, never touched
    );

    // With the door closed, the corridor beyond it is unseen.
    assert!(
        !s.player_fov().contains(Cell::new(5, 2)),
        "the closed door hides the corridor",
    );

    // The usable line predicts the frame open (§11.4): the closed hinge to the
    // east now offers `door: open`, in step with what the bump will do.
    assert!(
        s.affordances()
            .iter()
            .any(|&(dir, a)| dir == Direction::East && a == Affordance::OpenDoor),
        "a closed hinge offers door: open on the usable line",
    );

    // Bump the closed hinge to the east: the door opens, spending the turn.
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(events, vec![Event::DoorOpened { at: hinge }]);
    assert_eq!(s.turn(), 1, "opening spends the turn (§4.3)");
    assert_eq!(
        s.player(),
        Cell::new(3, 1),
        "the player did not move to open"
    );
    assert_eq!(
        s.layout().facility().terrain(panel),
        Some(Terrain::DoorPanelOpen),
        "every panel swung open",
    );

    // Facing turned along the door line, toward the panel (south).
    assert_eq!(
        s.facing(),
        Direction::South,
        "the frame bump faces the player along the door line, toward the panels",
    );

    // The recomputed FOV + #121 peek now leans through the doorway: the open
    // panel and a corridor cell on the far side are both seen (#121-style).
    assert!(s.player_fov().contains(panel), "the open doorway is seen");
    assert!(
        s.player_fov().contains(Cell::new(5, 2)),
        "the peek reads through the doorway into the corridor",
    );
}

/// §4.3/§10.3: a hideout is **bump-to-enter**, not a cell you drift onto. Stepping
/// into an empty cupboard climbs in — the player occupies the cell, the turn is
/// spent, and they are now [`hidden`](State::hidden). Entry auto-faces *out* of the
/// cupboard, back toward the corridor (§7.6, #89) — the opposite of the entry bump —
/// not into the wall the cupboard is recessed in.
#[test]
fn bumping_an_empty_hideout_enters_it_and_spends_the_turn() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 4), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(!s.hidden(), "the player starts in the open");

    let events = s.step(Input::Step(Direction::East)); // bump the cupboard east
    assert_eq!(
        events,
        vec![Event::EnteredHideout {
            at: Cell::new(5, 4)
        }]
    );
    assert_eq!(s.player(), Cell::new(5, 4), "the player climbed in");
    assert_eq!(
        s.facing(),
        Direction::West,
        "entry faces out toward the corridor (§7.6), the opposite of the bump"
    );
    assert_eq!(s.turn(), 1, "entering spends the turn");
    assert!(s.hidden(), "the player is now concealed");
}

/// §7.6/§10.3/#89: a recessed cupboard's entry auto-faces the exit — the corridor
/// side — so the ~180° half-disc (§6.2, arc 3) watches the flight path the moment
/// you hide instead of the wall behind you. Fixture: a cupboard recessed into the
/// top wall of a corridor, its only open face (the mouth) pointing south into the
/// corridor. The player bumps in from the mouth (heading north) and must end facing
/// south, seeing the corridor cells on *both* sides of the mouth.
#[test]
fn entering_a_hideout_faces_out_and_watches_the_corridor() {
    // Recess the cupboard at (5,3): walls on three sides, mouth (5,4) open to the
    // corridor row below.
    let mut layout = open_room(11, 11);
    for wall in [Cell::new(4, 3), Cell::new(6, 3), Cell::new(5, 2)] {
        layout.place(wall, Terrain::Wall);
    }
    layout.place(Cell::new(5, 3), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 4), // in the corridor, at the cupboard mouth
        Direction::East, // arbitrary prior facing — entry must override it
        Vec::new(),
        Vec::new(),
        Cell::new(9, 9),
    );

    let events = s.step(Input::Step(Direction::North)); // bump north into the cupboard
    assert_eq!(
        events,
        vec![Event::EnteredHideout {
            at: Cell::new(5, 3)
        }]
    );
    assert!(s.hidden(), "the player is concealed");
    assert_eq!(
        s.facing(),
        Direction::South,
        "entry faces out (south) toward the corridor, not north into the wall"
    );

    // The 180° half-disc, facing the corridor, covers the mouth and the cells on
    // both sides of it — the sweep the hiding game is built around.
    for corridor_cell in [
        Cell::new(5, 4), // the mouth
        Cell::new(4, 4), // west of the mouth
        Cell::new(6, 4), // east of the mouth
        Cell::new(5, 5), // straight down the corridor
    ] {
        assert!(
            s.player_fov().contains(corridor_cell),
            "hiding must watch the corridor cell {corridor_cell:?}"
        );
    }

    // The auto-peek (#121): facing out means the head leans through the
    // mouth, so the corridor reads far past the flanking walls' wedge —
    // both directions — with no hideout special-case. The plain cast from
    // inside the recess cannot see these; the live FOV (peek-aware) must.
    let plain = field_of_view(
        s.layout.facility(),
        s.player(),
        s.facing(),
        PLAYER_SIGHT_ARC,
        PLAYER_SIGHT_RANGE,
    );
    for far_cell in [Cell::new(1, 4), Cell::new(9, 4)] {
        assert!(
            !plain.contains(far_cell),
            "{far_cell:?} is beyond the mouth's wedge for the plain cast"
        );
        assert!(
            s.player_fov().contains(far_cell),
            "the peek must read the corridor to {far_cell:?}"
        );
        assert!(
            s.memory().contains(far_cell),
            "peeked cells feed tile memory like any seen cell (§11.5a)"
        );
    }
}

/// #121: the auto-peek is the player's alone — one-sided by design. Around
/// an L-corner the player reads the guard (**Seen**, the full picture — the
/// lean is a real line of sight), while the guard's own plain cone cannot
/// see the player back: no detection, no state change. A corner the player
/// can read still breaks the guard's line, which is what keeps corners the
/// player's flight tool (§7.6).
#[test]
fn the_peek_is_the_players_alone_a_guard_never_peeks() {
    let mut layout = open_room(11, 11);
    layout.place(Cell::new(4, 4), Terrain::Wall); // the corner block
    let mut guard = Guard::stationary(Cell::new(6, 3));
    // Face the guard straight at the corner — the worst case for the player.
    guard.advance_to(Cell::new(6, 3), Direction::West, layout.facility());
    let s = State::new(
        layout,
        Cell::new(3, 4), // one short of the corner, facing along it
        Direction::North,
        vec![guard],
        Vec::new(),
        Cell::new(9, 9),
    );

    let guard = &s.guards()[0];
    assert!(
        s.player_fov().contains(guard.pos()),
        "the peek shows the guard around the corner"
    );
    let plain = field_of_view(
        s.layout.facility(),
        s.player(),
        s.facing(),
        PLAYER_SIGHT_ARC,
        PLAYER_SIGHT_RANGE,
    );
    assert!(
        !plain.contains(guard.pos()),
        "the corner hides the guard from the body's own cast — the delta is the peek"
    );
    assert_eq!(
        s.perceive_guard(guard),
        Some(GuardPerception::Seen),
        "a peeked guard is Seen, cone and all, not the sensed dot"
    );
    assert!(
        !guard.fov().contains(s.player()),
        "the guard's plain cone must not read around the corner"
    );
    assert_eq!(
        guard.state(),
        GuardState::Calm,
        "seeing a guard through the peek is information, never detection"
    );
}

/// §4.3/§10.3: "move off to climb out." Stepping from a hideout onto floor is an
/// ordinary move that clears the hidden state — no special key, no special event.
#[test]
fn moving_off_a_hideout_climbs_out() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 4), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 4), // start already inside the cupboard
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(s.hidden(), "starting inside the cupboard is concealed");

    let events = s.step(Input::Step(Direction::West)); // step out onto floor
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(4, 4)
        }],
        "climbing out is an ordinary move"
    );
    assert_eq!(s.player(), Cell::new(4, 4));
    assert_eq!(
        s.facing(),
        Direction::West,
        "climbing out follows the step (§5) — only entry auto-faces (#89)"
    );
    assert!(!s.hidden(), "leaving clears the concealment");
}

/// §4.5/§7.6/§10.3: a concealed player is contact-safe. A guard patrolling into the
/// player's cell captures in the open, but a cupboard is the one place contact is
/// refused — the guard cannot enter, holds, and the run goes on. This is the
/// "watch the cone sweep past" payoff; the same guard *would* capture if the player
/// were not hidden (see [`a_guard_stepping_into_the_player_captures`]).
#[test]
fn a_guard_cannot_capture_a_hidden_player() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(4, 4), Terrain::Hideout);
    // Guard at (6,4) sent to the cupboard cell (4,4) where the player hides.
    // After the startup turn the guard is at (5,4), one step from the player's
    // cell — the destination it will be refused entry to.
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(6, 4), Cell::new(4, 4))],
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(s.hidden());
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(5, 4),
        "startup moved the guard"
    );

    // The guard tries to step onto the player's cell: contact refused. It holds at
    // (5,4), no capture, still playing.
    let events = s.step(Input::Wait);
    assert!(
        !events.iter().any(|e| matches!(e, Event::Captured { .. })),
        "a hidden player is not captured"
    );
    assert_eq!(s.outcome(), Outcome::Playing, "the run continues");
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(5, 4),
        "the guard cannot enter the occupied cupboard"
    );
}

/// §7.6 fix 2 (Lost → Hunted → Released): a guard that loses sight of the player
/// walks to the last-known cell, **searches** the area (Alerted) rather than snapping
/// back to patrol, and only then releases to Calm and moves on. The player waits it
/// out concealed in a cupboard: it is never captured, watches the guard search, and
/// watches it leave — the payoff §14 exists to test.
#[test]
fn a_hidden_player_waits_out_a_search_and_watches_the_guard_leave() {
    let mut layout = open_room(16, 12);
    layout.place(Cell::new(4, 5), Terrain::Hideout); // a cupboard beside the player
                                                     // Guard at (5,1) facing south: its cone covers the player at (5,5), four cells
                                                     // down — the certain zone — so it detects and chases at the startup turn.
    let guards = vec![Guard::patrolling(Cell::new(5, 1))];
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        guards,
        Vec::new(),
        Cell::new(14, 10),
    );
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Chasing,
        "the guard spots the player at spawn",
    );

    // The player ducks west into the cupboard and holds. The guard loses sight.
    s.step(Input::Step(Direction::West));
    assert!(s.hidden(), "the player is concealed");

    let focus = Cell::new(5, 5); // where the guard last knew the player
    let (mut searched, mut released, mut left_the_area) = (false, false, false);
    for _ in 0..60 {
        s.step(Input::Wait);
        assert_eq!(
            s.outcome(),
            Outcome::Playing,
            "a hidden player is never caught"
        );
        match s.guards()[0].state() {
            GuardState::Alerted => searched = true,
            GuardState::Calm if searched => {
                released = true;
                if s.guards()[0].pos().sight_distance(focus) > SEARCH_RADIUS {
                    left_the_area = true;
                }
            }
            _ => {}
        }
    }
    assert!(
        searched,
        "the guard searched the area (Alerted) instead of giving up"
    );
    assert!(released, "the search released back to Calm patrol");
    assert!(
        left_the_area,
        "after releasing, the guard leaves the search area"
    );
    assert!(
        s.hidden(),
        "the player rode the whole search out from cover"
    );
}

/// §7.8: guards are solid to each other but **path around** a colleague instead of
/// pathing through, failing the step, and stalling — the old deadlock. Two guards
/// sweep a 2-wide corridor toward destinations past one another; they must pass
/// (one drops to the parallel lane) without ever sharing a cell. The player waits,
/// concealed off the corridor, so the sweep runs untouched.
#[test]
fn two_guards_meeting_in_a_corridor_pass_without_deadlock() {
    // A 2-wide corridor (rows 1–2) across a box; row 3 is wall except a recessed
    // cupboard the player hides in, off the guards' lanes.
    let mut layout = open_room(12, 5);
    for x in 1..=10 {
        layout.place(Cell::new(x, 3), Terrain::Wall);
    }
    layout.place(Cell::new(5, 3), Terrain::Hideout);
    let guards = vec![
        Guard::patrolling_to(Cell::new(1, 1), Cell::new(10, 1)),
        Guard::patrolling_to(Cell::new(10, 1), Cell::new(1, 1)),
    ];
    let mut s = State::new(
        layout,
        Cell::new(5, 3), // concealed in the cupboard, out of the lanes
        Direction::North,
        guards,
        Vec::new(),
        Cell::new(1, 3),
    );
    assert!(s.hidden(), "the player watches from cover");

    let mut passed = false;
    for turn in 0..40 {
        s.step(Input::Wait);
        let (a, b) = (s.guards()[0].pos(), s.guards()[1].pos());
        assert_ne!(a, b, "turn {turn}: guards must never share a cell (§7.8)");
        assert_eq!(s.outcome(), Outcome::Playing, "turn {turn}: no capture");
        // They start with a.x < b.x; passing swaps that order — the proof the
        // head-on meet resolved instead of deadlocking.
        if a.x > b.x {
            passed = true;
        }
    }
    assert!(
        passed,
        "the guards deadlocked instead of pathing around each other (§7.8)"
    );
}

/// §10.3: **bumping a table is the crouch** — ducking is a decision aimed at
/// a specific table, like the cupboard's bump-to-enter. It spends the turn,
/// reports once as the crouch engages, does not move the player, and
/// re-bumping the same table is a free no-op. Waiting holds the pose; a
/// plain wait away from cover crouches nothing.
#[test]
fn bumping_a_table_crouches_once() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 4), Terrain::PartialCover);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(!s.crouched(), "standing until the table is bumped");
    s.step(Input::Wait);
    assert!(!s.crouched(), "waiting beside a table no longer crouches");

    let turn = s.turn();
    let events = s.step(Input::Step(Direction::East)); // bump the table
    assert_eq!(
        events,
        vec![Event::Crouched {
            behind: Cell::new(5, 4)
        }]
    );
    assert!(s.crouched());
    assert_eq!(s.crouched_behind(), Some(Cell::new(5, 4)));
    assert_eq!(s.player(), Cell::new(4, 4), "the crouch does not move you");
    assert_eq!(s.turn(), turn + 1, "the crouch spends the turn");

    // Waiting on: still crouched, nothing repeated.
    assert!(s.step(Input::Wait).is_empty());
    assert!(s.crouched());

    // Re-bumping the table you are already behind is a free no-op (§4.4).
    let turn = s.turn();
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(5, 4)
        }]
    );
    assert_eq!(s.turn(), turn, "a re-bump is free");
    assert!(s.crouched(), "and it does not break the crouch");
}

/// §10.3: a spent action other than a wait or a crouch-walk stands the
/// player up — the crouch survives *plain movement along its cover*, never
/// an interaction — while a *free* action (a wall bump) changes nothing,
/// not even posture (§4.4): the world does not move, so neither does the
/// crouch.
#[test]
fn an_interaction_stands_up_but_a_free_bump_does_not() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(1, 2), Terrain::PartialCover);
    let mut s = State::new(
        layout,
        Cell::new(1, 1), // in the corner: west and north are wall
        Direction::North,
        Vec::new(),
        vec![Cell::new(2, 1)], // a console east of the player
        Cell::new(8, 8),
    );
    s.step(Input::Step(Direction::South)); // bump the table below: crouch
    assert!(s.crouched());

    // A mis-input into the wall is free: still crouched, turn unspent.
    let turn = s.turn();
    s.step(Input::Step(Direction::West));
    assert_eq!(s.turn(), turn, "a wall bump is free");
    assert!(s.crouched(), "a free action does not break the crouch");

    // A spent interaction stands up — taking the intel is not a crouch-walk,
    // even though the player never left the table's side.
    s.step(Input::Step(Direction::East));
    assert!(!s.crouched(), "a spent interaction stands the player up");
}

/// §10.3: the **crouch-walk** — plain movement that keeps hugging the
/// anchored run holds the crouch, including the diagonal corner past the
/// bench's end, so the player can round the furniture without standing.
/// The first step that leaves the run's side is an ordinary move and
/// stands them up.
#[test]
fn a_crouch_walk_hugs_the_bench_and_rounds_its_end() {
    let mut layout = open_room(12, 12);
    for y in 3..=5 {
        layout.place(Cell::new(5, y), Terrain::PartialCover); // a vertical bench
    }
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Step(Direction::East)); // bump mid-bench: crouch
    assert!(s.crouched());

    // Walk the bench's west flank, round its south end on the diagonal,
    // and come up its east flank: crouched the whole way.
    for (dir, at) in [
        (Direction::South, Cell::new(4, 5)), // flush beside the end table
        (Direction::South, Cell::new(4, 6)), // the corner: diagonal contact
        (Direction::East, Cell::new(5, 6)),  // square-on below the end
        (Direction::East, Cell::new(6, 6)),  // the far corner
        (Direction::North, Cell::new(6, 5)), // up the east flank
    ] {
        s.step(Input::Step(dir));
        assert_eq!(s.player(), at);
        assert!(s.crouched(), "the walk to {at:?} must hold the crouch");
    }
    // The anchor still names the originally bumped table; the cover is the run.
    assert_eq!(s.crouched_behind(), Some(Cell::new(5, 4)));
    let mut run = s.crouch_cover();
    run.sort_by_key(|c| c.y);
    assert_eq!(run, vec![Cell::new(5, 3), Cell::new(5, 4), Cell::new(5, 5)]);
    // Cover crossed sides with the player: the bench now blinds the west.
    assert!(
        s.concealed_from(Cell::new(2, 5)),
        "across the bench: covered"
    );
    assert!(
        !s.concealed_from(Cell::new(9, 5)),
        "the open east flank: seen"
    );

    // One step away from the furniture is an ordinary move: stand up.
    s.step(Input::Step(Direction::East));
    assert!(!s.crouched(), "leaving the run's side stands the player up");
}

/// The #141 report, pinned: a crouched player must not be seen by a guard
/// whose sight line crosses *any* table of the bench they are behind. The
/// old single-table quarter-plane let a viewer oblique to the anchor look
/// straight past it — through the bench's other tables — and see the
/// player. The run is the cover now; the flank past its end stays open.
#[test]
fn a_bench_conceals_across_its_whole_run() {
    let mut layout = open_room(12, 12);
    for y in 3..=5 {
        layout.place(Cell::new(5, y), Terrain::PartialCover);
    }
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Step(Direction::East)); // crouch, anchored mid-bench
    assert!(s.crouched());

    // Oblique viewers the anchor's own quarter-plane never covered, but
    // whose line to the player crosses the bench's outer tables: concealed.
    assert!(s.concealed_from(Cell::new(6, 7)), "across the south table");
    assert!(s.concealed_from(Cell::new(6, 1)), "across the north table");
    // No table on the line: still seen — the bench is cover, not a cloak.
    assert!(!s.concealed_from(Cell::new(4, 1)), "past the bench's end");
    assert!(!s.concealed_from(Cell::new(1, 4)), "behind the player");
}

/// §10.3: crouch concealment is **directional** — cover blinds only the
/// viewers whose sight line crosses it. Behind a lone table that is the
/// quarter-plane it faces: a viewer across the cover (straight or leaning
/// up to the 45° graze) is blinded; a viewer on the flank or behind the
/// player is not; and without the crouch the same table conceals nothing.
#[test]
fn crouch_conceals_only_across_the_cover() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::PartialCover); // east of the player
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );

    // Standing, the table conceals from no one.
    assert!(!s.concealed_from(Cell::new(7, 4)));

    s.step(Input::Step(Direction::East)); // bump the table: crouch
    assert!(s.crouched());
    // Straight across the table, near and far: concealed.
    assert!(s.concealed_from(Cell::new(6, 4)));
    assert!(s.concealed_from(Cell::new(9, 4)));
    // Leaning, within the quarter-plane (along ≥ across): concealed —
    // including the exact 45° diagonal, which grazes the table's corner.
    assert!(s.concealed_from(Cell::new(6, 3)));
    assert!(s.concealed_from(Cell::new(6, 2)));
    // Past the diagonal — the flank, the perpendicular, and behind: seen.
    assert!(!s.concealed_from(Cell::new(5, 2)));
    assert!(!s.concealed_from(Cell::new(4, 2)));
    assert!(!s.concealed_from(Cell::new(2, 4)));
}

/// §10.3: the cupboard is the stronger hide — omnidirectional. A hidden
/// player is concealed from every direction, cover or none.
#[test]
fn a_hidden_player_is_concealed_from_every_direction() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(4, 4), Terrain::Hideout);
    let s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(s.hidden());
    for viewer in [
        Cell::new(4, 1),
        Cell::new(7, 4),
        Cell::new(4, 7),
        Cell::new(1, 4),
        Cell::new(6, 6),
    ] {
        assert!(
            s.concealed_from(viewer),
            "hidden must conceal from {viewer:?}"
        );
    }
}

/// §4.5: the crouch hides you from *sight*, not from *contact* — unlike the
/// cupboard, a guard walking into a crouched player still captures. Being
/// unseen is not being safe.
#[test]
fn a_crouched_player_is_still_captured_by_contact() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(4, 3), Terrain::PartialCover); // cover to the north
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(6, 4), Cell::new(1, 4))],
        Vec::new(),
        Cell::new(8, 8),
    );
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(5, 4),
        "startup moved the guard"
    );

    // The bump crouches the player — and hands the guard its step into them.
    let events = s.step(Input::Step(Direction::North));
    assert!(events.contains(&Event::Crouched {
        behind: Cell::new(4, 3)
    }));
    assert!(
        events.contains(&Event::Captured {
            by: Cell::new(4, 4)
        }),
        "contact captures a crouched player"
    );
    assert_eq!(s.outcome(), Outcome::Lost);
}

/// §4.2: the startup turn establishes sight before the first input. A freshly
/// built [`State`] already carries the player's half-disc and every guard's cone
/// — and a guard that has not moved is looking **south**, its initial facing
/// (§7.1).
#[test]
fn the_startup_turn_establishes_sight() {
    let s = State::new(
        open_room(12, 12),
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(8, 8))],
        Vec::new(),
        Cell::new(10, 10),
    );

    // The player faces north: two ahead is lit, two directly behind is not (§6.2).
    assert!(s.player_fov().contains(Cell::new(5, 3)));
    assert!(!s.player_fov().contains(Cell::new(5, 7)));

    // The stationary guard looks south from spawn (§7.1): its wedge covers two
    // south, not two north.
    let g = &s.guards()[0];
    assert_eq!(g.facing(), Direction::South);
    assert!(g.fov().contains(Cell::new(8, 10)));
    assert!(!g.fov().contains(Cell::new(8, 6)));
}

/// §8.3: **Wait grants 360° vision for that turn** — the only way to see behind
/// you (§5). The widened arc lasts until the next spent turn narrows it again.
#[test]
fn waiting_widens_sight_to_the_full_circle() {
    let mut s = solo(Cell::new(5, 5));
    s.step(Input::Step(Direction::North)); // now at (5,4), facing north

    let behind = Cell::new(5, 6); // two cells directly behind
    assert!(
        !s.player_fov().contains(behind),
        "the half-disc does not see directly behind"
    );

    s.step(Input::Wait);
    assert!(
        s.player_fov().contains(behind),
        "a turn spent waiting sees behind"
    );

    s.step(Input::Step(Direction::West)); // at (4,4), facing west; behind is east
    assert!(
        !s.player_fov().contains(Cell::new(6, 4)),
        "moving narrows the arc back to the half-disc"
    );
}

/// §11.5a: tile memory is the running union of every FOV the player has had —
/// seeded by the startup turn, grown each sight phase, and never forgetting a
/// cell that has since fallen out of view. It is derived purely from the FOV
/// sequence, so it is as deterministic as the loop itself.
#[test]
fn tile_memory_accumulates_and_never_forgets() {
    let mut s = solo(Cell::new(5, 5)); // facing north
    let ahead = Cell::new(5, 3);
    assert!(s.player_fov().contains(ahead));
    assert!(s.memory().contains(ahead), "the startup turn seeds memory");

    // Turn around: (5,3) falls out of the FOV but stays in memory.
    s.step(Input::Step(Direction::South)); // to (5,6), facing south
    assert!(
        !s.player_fov().contains(ahead),
        "now behind, out of the FOV"
    );
    assert!(s.memory().contains(ahead), "memory keeps what the FOV lost");
}

/// §4.2's design note, honoured: there is **no one-turn sensory lag**. The sight
/// phase runs after the player's move, so the stored FOV is always from the
/// player's current position and facing.
#[test]
fn sight_is_recomputed_from_the_players_new_position_and_facing() {
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    );
    // Facing north, the side line runs west: (2,5) is lit.
    assert!(s.player_fov().contains(Cell::new(2, 5)));

    s.step(Input::Step(Direction::East)); // now at (6,5), facing east
    assert!(
        s.player_fov().contains(Cell::new(9, 5)),
        "the cone points down the new facing"
    );
    assert!(
        !s.player_fov().contains(Cell::new(2, 5)),
        "what fell directly behind went dark this same turn"
    );
}

/// Guards: **facing follows the successful step** (§5, for guards as for the
/// player), and a moved guard's stored cone is current when the turn ends — the
/// frame never shows a guard in one place with its sight in another (§11.5).
#[test]
fn a_moved_guards_cone_is_current_when_the_turn_ends() {
    let mut s = State::new(
        open_room(12, 12),
        // Parked in the north-east, well behind the westbound guard's cone, so
        // detection (§7.6) never derails the patrol whose cone this test measures.
        Cell::new(10, 1),
        Direction::South,
        vec![Guard::patrolling_to(Cell::new(8, 8), Cell::new(1, 8))],
        Vec::new(),
        Cell::new(10, 10),
    );
    // The startup turn already walked the guard one west and turned it.
    let g = &s.guards()[0];
    assert_eq!(g.pos(), Cell::new(7, 8));
    assert_eq!(g.facing(), Direction::West);
    assert!(g.fov().contains(Cell::new(5, 8)), "the wedge points west");
    assert!(!g.fov().contains(Cell::new(9, 8)), "not behind it");

    s.step(Input::Wait);
    let g = &s.guards()[0];
    assert_eq!(g.pos(), Cell::new(6, 8));
    assert!(
        g.fov().contains(Cell::new(4, 8)) && !g.fov().contains(Cell::new(8, 8)),
        "the cone moved with the guard this very turn"
    );
}

/// The §7.4 colour column, pinned in §11.2's vocabulary: Calm is the unaware
/// threat, Alerted and Responding are hunting, Chasing and Investigating have
/// you. If a state's category moves, this test is where the change is owned.
#[test]
fn guard_states_declare_the_7_4_categories() {
    use crate::category::Category;
    assert_eq!(GuardState::Calm.category(), Category::Caution);
    assert_eq!(GuardState::Alerted.category(), Category::Warning);
    assert_eq!(GuardState::Responding.category(), Category::Warning);
    assert_eq!(GuardState::Chasing.category(), Category::Danger);
    assert_eq!(GuardState::Investigating.category(), Category::Danger);
    // A guard carries its state: Calm by default, overridable for scenarios.
    assert_eq!(Guard::stationary(Cell::new(1, 1)).state(), GuardState::Calm);
    let chasing = Guard::stationary(Cell::new(1, 1)).with_state(GuardState::Chasing);
    assert_eq!(chasing.state().category(), Category::Danger);
}

/// Events speak the same §11.2 table as the glyphs, so the message ticket can
/// colour its bar without inventing meanings: taking intel is Interest, the
/// capture is Danger, a step is routine Neutral.
#[test]
fn events_declare_their_message_category() {
    use crate::category::Category;
    let at = Cell::new(2, 3);
    assert_eq!(Event::Moved { to: at }.category(), Category::Neutral);
    assert_eq!(Event::Bumped { into: at }.category(), Category::Neutral);
    assert_eq!(Event::EnteredHideout { at }.category(), Category::Owned);
    assert_eq!(Event::Crouched { behind: at }.category(), Category::Owned);
    assert_eq!(Event::DoorOpened { at }.category(), Category::System);
    assert_eq!(Event::DoorClosed { at }.category(), Category::System);
    assert_eq!(
        Event::IntelTaken { remaining: 1 }.category(),
        Category::Interest
    );
    assert_eq!(Event::ExitRefused.category(), Category::Interest);
    assert_eq!(Event::Won.category(), Category::Interest);
    assert_eq!(Event::Captured { by: at }.category(), Category::Danger);
    assert_eq!(Event::TakenDown { at }.category(), Category::Owned);
    assert_eq!(Event::BodyFound { at }.category(), Category::Warning);
}

/// §12.4: the loop is pure and deterministic. The same starting state and the same
/// input sequence produce an identical event stream and identical final state —
/// the property that makes a run a `(seed, [inputs])` replay. The loop's only
/// randomness is the seeded stream carried in the state (the guard close-behind,
/// #146), which two identically-built states share turn for turn, so this stays a
/// clean replay; the test pins it against a future change (a stray `HashMap`
/// order, a clock read, a fresh RNG source) that would break it.
#[test]
fn same_state_and_inputs_replay_identically() {
    let inputs = [
        Input::Step(Direction::East), // bump the console east: take the intel
        Input::Step(Direction::North),
        Input::Wait,
        Input::Step(Direction::West),
        Input::Step(Direction::South),
        Input::Step(Direction::South),
    ];

    let run = || {
        // Player, one intel to the east, a patrolling guard, exit to the south.
        let mut s = State::new(
            open_room(12, 12),
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::patrolling(Cell::new(8, 5))],
            [Cell::new(6, 5)],
            Cell::new(5, 6),
        );
        let events: Vec<Event> = inputs.iter().flat_map(|&i| s.step(i)).collect();
        (
            events,
            s.player(),
            s.facing(),
            s.turn(),
            s.outcome(),
            s.objectives_remaining(),
            s.guards()[0].pos(),
            s.player_fov().clone(),
            s.memory().clone(),
        )
    };

    assert_eq!(run(), run(), "same state + inputs must replay identically");
}

/// The usable line's contract (§11.4): [`State::affordances`] offers exactly
/// what a bump would do. A live console reads `TakeIntel`; once taken it is
/// just solid and offers nothing; the exit answers by whether the intel is
/// in hand; an empty cupboard offers `Hide`.
#[test]
fn affordances_mirror_what_a_bump_would_do() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(4, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        [Cell::new(6, 5)], // a console east
        Cell::new(5, 4),   // the exit north
    );

    // Console east, exit north (intel still out), cupboard west — each with
    // the direction to bump it.
    assert_eq!(
        s.affordances(),
        vec![
            (Direction::North, Affordance::ExitRefused),
            (Direction::East, Affordance::TakeIntel),
            (Direction::West, Affordance::Hide)
        ],
        "Direction::ALL order: north, east, … west"
    );

    // Take the intel: the console goes solid and the exit opens up.
    s.step(Input::Step(Direction::East));
    assert_eq!(
        s.affordances(),
        vec![
            (Direction::North, Affordance::Leave),
            (Direction::West, Affordance::Hide)
        ],
        "a spent console offers nothing; the exit now offers the win"
    );

    // In the middle of open floor, the line is empty.
    let s = solo(Cell::new(4, 4));
    assert_eq!(s.affordances(), Vec::new());
}

/// An adjacent **aware** guard offers nothing: its bump is a free no-op
/// (§7.2's gate — the unaware case is the takedown test above), and the
/// usable line must never promise what a bump will not do (§2.3). An
/// occupied cupboard is likewise just solid.
#[test]
fn affordances_skip_guards_and_occupied_hideouts() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(6, 5))], // east of the player
        Vec::new(),
        Cell::new(10, 10),
    );
    // Enter the cupboard north; the guard east never shows.
    assert_eq!(s.affordances(), vec![(Direction::North, Affordance::Hide)]);
    s.step(Input::Step(Direction::North));
    assert!(s.hidden());

    // From inside, the cupboard's own cell is the player's — and stepping
    // back out is a plain move, not an affordance.
    assert_eq!(s.affordances(), Vec::new());
}

/// Door affordances speak the §10.4 door graph: a closed panel offers the
/// open; an open hinge offers the close — except while an actor stands on a
/// panel, when the close would be refused (doors never crush) and so is
/// never offered.
#[test]
fn door_affordances_track_pose_and_obstruction() {
    for seed in 0..64 {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let Some((_, from, into, panel, hinge_dir)) = crush_scenario(&layout) else {
            continue;
        };
        // The hinge's floor neighbour on the player's side of the wall: the
        // cell to close the door from. `side` steps off the door line back
        // toward `from`'s side.
        let hinge = panel.step(hinge_dir).expect("hinge adjacent to panel");
        let side = Direction::between(panel, from).expect("from is beside the panel");
        let Some(beside_hinge) = hinge.step(side) else {
            continue;
        };
        let f = layout.facility();
        if f.terrain(beside_hinge) != Some(Terrain::Floor) || !f.can_enter(beside_hinge, ACTOR_FILL)
        {
            continue;
        }

        let mut s = State::new(
            layout,
            from,
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(0, 0), // border corner: never walked, never bumped
        );
        let offers = |s: &State, want: Affordance| s.affordances().iter().any(|&(_, a)| a == want);
        assert!(
            offers(&s, Affordance::OpenDoor),
            "seed {seed}: a closed panel offers the open"
        );
        assert!(!offers(&s, Affordance::CloseDoor));

        // Open it, then stand on the panel: the close would be refused, so
        // the hinge offers nothing.
        s.step(Input::Step(into));
        s.step(Input::Step(into));
        assert_eq!(s.player(), panel);
        assert!(
            !offers(&s, Affordance::CloseDoor),
            "seed {seed}: no close offered while standing on the panel"
        );

        // Step back off the panel, then along the wall to sit beside the
        // hinge: now the close is a real offer.
        s.step(Input::Step(side));
        s.step(Input::Step(hinge_dir));
        assert_eq!(s.player(), beside_hinge);
        assert!(
            offers(&s, Affordance::CloseDoor),
            "seed {seed}: an open hinge offers the close"
        );
        assert!(!offers(&s, Affordance::OpenDoor));
        return;
    }
    panic!("no usable door scenario found in 64 seeds");
}

/// §9 **[SETTLED]**: guards detect on **vision only** — they do not hear. A player
/// who scrambles into a cupboard right beside a guard, concealed from its sight
/// (§10.3), is *not* detected: with the old hearing branch gone, a guard the player
/// could once "give away a footstep to" now stays **Calm**. This is the inverse of
/// the deleted hearing test, pinning the new rule.
#[test]
fn a_guard_that_cannot_see_the_hidden_player_stays_calm() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 4), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::stationary(Cell::new(6, 4))],
        Vec::new(),
        Cell::new(8, 8),
    );
    assert_eq!(s.guards()[0].state(), GuardState::Calm);

    // Step East into the cupboard at (5,4), one cell from the guard: the hideout
    // conceals the player from its sight, and nothing is heard — so it stays Calm.
    s.step(Input::Step(Direction::East));
    assert!(s.hidden(), "the player scrambled into the cupboard");
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Calm,
        "guards detect on vision only — a hidden player is not seen, and not heard",
    );
}

/// A player out of every cone alerts no one: standing two cells behind a
/// south-facing guard's back — past the touching ring and out of its wedge — the
/// player is not seen, so the guard stays Calm however they act (§9 — there is no
/// hearing to give them away either).
#[test]
fn an_unseen_player_alerts_no_one() {
    let mut s = State::new(
        open_room(10, 10),
        Cell::new(5, 2), // two north of the south-facing guard: directly behind it
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4))],
        Vec::new(),
        Cell::new(8, 8),
    );
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Calm,
        "unseen at the start"
    );
    s.step(Input::Wait);
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Calm,
        "a player the guard cannot see stays undetected",
    );
}

/// §7.6 wired end to end: the two detection zones flip a guard between Chasing and
/// Investigating as the player's distance crosses the certain→glimpse boundary. A
/// stationary fixture isolates the state machine from patrol movement; detection is
/// sight's alone (§9 — guards do not hear).
#[test]
fn detection_flips_between_chasing_and_investigating_by_zone() {
    // Guard looking straight down a long cone from (6,2); the player starts four
    // cells in — the certain zone — so the startup turn already has it Chasing.
    let mut s = State::new(
        open_room(13, 15),
        Cell::new(6, 6),
        Direction::North,
        vec![Guard::stationary(Cell::new(6, 2))],
        Vec::new(),
        Cell::new(11, 12),
    );
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Chasing,
        "seen in the certain zone → Chasing",
    );

    // One step down the cone is still within the certain zone (5): still Chasing.
    s.step(Input::Step(Direction::South)); // (6,7): 5 cells
    assert_eq!(s.guards()[0].state(), GuardState::Chasing);

    // A second step crosses into the glimpse zone (6): drops to Investigating —
    // Run's five-cell gain is exactly this certain→glimpse distance (§7.6).
    s.step(Input::Step(Direction::South)); // (6,8): 6 cells
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Investigating,
        "backed out to the glimpse zone → Investigating",
    );
}

/// Guards do not detect *each other* — detection reads the player alone (§7.8:
/// guards cannot hurt each other). Two adjacent guards with the player far out of
/// every cone both stay Calm turn after turn.
#[test]
fn guards_do_not_detect_each_other() {
    let mut s = State::new(
        open_room(10, 10),
        Cell::new(1, 1),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(5, 5)),
            Guard::stationary(Cell::new(5, 6)),
        ],
        Vec::new(),
        Cell::new(8, 8),
    );
    for _ in 0..3 {
        s.step(Input::Wait);
    }
    assert!(
        s.guards().iter().all(|g| g.state() == GuardState::Calm),
        "a guard never reacts to another guard",
    );
}

/// Determinism (§12.4) with detection in play: the same start and inputs reproduce
/// the same guard states and positions, reactions included.
#[test]
fn detection_is_deterministic() {
    let inputs = [
        Input::Step(Direction::East),
        Input::Step(Direction::East),
        Input::Wait,
        Input::Step(Direction::South),
    ];
    let run = || {
        let mut s = State::new(
            open_room(12, 12),
            Cell::new(3, 3),
            Direction::North,
            vec![
                Guard::patrolling(Cell::new(6, 6)),
                Guard::patrolling(Cell::new(9, 3)),
            ],
            Vec::new(),
            Cell::new(11, 11),
        );
        inputs
            .iter()
            .map(|&i| {
                s.step(i);
                s.guards()
                    .iter()
                    .map(|g| (g.pos(), g.state()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

/// §9.2 classification: from the player's position and facing, each guard is
/// **Seen** (in the FOV), **Sensed** (within the guard-sense box but out of FOV),
/// or neither (out of range → `None`). A guard in view is Seen even though it also
/// sits inside the box — Seen wins, so the dot never doubles the full guard.
#[test]
fn guards_classify_as_seen_sensed_or_neither() {
    let seen = Cell::new(20, 16); // 4 north: in the forward half-disc
    let sensed = Cell::new(20, 25); // 5 south: behind the player, inside the box
    let gone = Cell::new(20, 33); // 13 south: behind and past the 10-box
    let s = State::new(
        open_room(40, 40),
        Cell::new(20, 20),
        Direction::North,
        vec![
            Guard::stationary(seen),
            Guard::stationary(sensed),
            Guard::stationary(gone),
        ],
        Vec::new(),
        Cell::new(38, 38),
    );

    assert!(
        s.player_fov().contains(seen),
        "precondition: seen guard in FOV"
    );
    assert!(
        !s.player_fov().contains(sensed),
        "precondition: sensed guard out of FOV"
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Seen)
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[1]),
        Some(GuardPerception::Sensed),
        "in the box but out of view → position only",
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[2]),
        None,
        "past the box → nothing"
    );
}

/// §9.1's headline: the sense **passes through walls** — it is not line of sight.
/// A guard sealed behind a wall, with no line to the player but inside the box, is
/// **Sensed** (position only), not hidden. A walled-off fixture pins this.
#[test]
fn the_sense_passes_through_walls() {
    let mut layout = open_room(20, 20);
    // Wall the whole row y=8 across the interior, sealing the north strip from the
    // player's line of sight.
    for x in 1..=18 {
        layout.place(Cell::new(x, 8), Terrain::Wall);
    }
    let guard = Cell::new(10, 6); // 4 north of the player, behind the wall
    let s = State::new(
        layout,
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(guard)],
        Vec::new(),
        Cell::new(18, 18),
    );

    assert!(
        !s.player_fov().contains(guard),
        "precondition: the wall blocks line of sight to the guard",
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Sensed),
        "no line of sight but inside the box → sensed through the wall",
    );
}

/// §9.1 **[START]**: the sense box is **10**, widening to **20** on a turn the
/// player spent waiting. Both are pinned so a later change is visible. A walled-off
/// guard 11 cells away — just outside the box, no line of sight — is *not* sensed;
/// the same guard becomes Sensed the turn the player waits (10 → 20).
#[test]
fn the_sense_range_is_ten_and_twenty_on_wait() {
    assert_eq!(PLAYER_SENSE_RANGE, 10, "the [START] sense range");
    assert_eq!(
        PLAYER_SENSE_RANGE_WAITING, 20,
        "the [START] wait sense range"
    );

    let mut layout = open_room(40, 40);
    // A full wall row seals the guard from sight, so it can only ever be *sensed*,
    // never seen — even under the 360° look a wait grants (§8.3).
    for x in 1..=38 {
        layout.place(Cell::new(x, 12), Terrain::Wall);
    }
    let guard = Cell::new(20, 9); // 11 north of the player: just past the 10-box
    let mut s = State::new(
        layout,
        Cell::new(20, 20),
        Direction::North,
        vec![Guard::stationary(guard)],
        Vec::new(),
        Cell::new(38, 38),
    );

    assert_eq!(s.sense_range(), 10, "no wait yet: the base box");
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        None,
        "11 cells away is just outside the 10-box",
    );

    s.step(Input::Wait);
    assert_eq!(s.sense_range(), 20, "waiting widens the box");
    assert!(
        !s.player_fov().contains(guard),
        "still walled off from sight"
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Sensed),
        "the wait pulls the guard into the widened box → sensed",
    );
}
