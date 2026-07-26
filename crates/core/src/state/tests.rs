use super::*;
use crate::facility::Facility;
use crate::guard::{GuardState, CERTAIN_RANGE, GLIMPSE_RANGE, PATROL_RADIUS, SEARCH_RADIUS};
use crate::region::{DoorKind, RegionGraph, RegionKind};
use crate::targeting::Target;
use crate::test_support::{open_room, region_strip, solo};
use crate::vision::field_of_view;
use crate::{generate, generate_level, DoorId, LevelModifiers, Rng};

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
    layout.place(Cell::new(5, 5), Terrain::Hideout); // the player's start cupboard
    layout.place(Cell::new(5, 2), Terrain::Hideout); // the stow cupboard
    let mut s = State::new(
        layout,
        Cell::new(5, 5), // start hidden, so the victim never sees the takedown coming
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4)).with_radio_clock(radio::RadioClock::from_period(5))],
        Vec::new(),
        Cell::new(10, 10),
    );
    s.step(Input::Step(Direction::North)); // takedown: body at (5,4)
    s.step(Input::Step(Direction::North)); // climb out onto the body
    s.step(Input::Step(Direction::North)); // step off to (5,3) — take hold
    s.step(Input::Step(Direction::North)); // stow the body in the cupboard at (5,2)
    let body = Cell::new(5, 2);
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
/// advance, the player does not move, and facing is unchanged (§5). Bumped here
/// from mid-wall, where **both** laterals are open floor: the #57 auto-slide only
/// fires on an *unambiguous* single open side, so this ambiguous bump stays the
/// free mis-input §4.4 protects (the slide cases are pinned by the `#57` tests).
#[test]
fn bumping_a_wall_is_free_and_does_not_advance_the_turn() {
    let mut s = solo(Cell::new(2, 1)); // both (1,1) and (3,1) are open floor
    let events = s.step(Input::Step(Direction::North)); // into the north wall
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(2, 0)
        }]
    );
    assert_eq!(s.player(), Cell::new(2, 1), "no move");
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
    // A **reactive** guard (Responding) turns fast (§229), so its startup step
    // west both moves and re-aims — reaching (5,4) with its back-then-cone never
    // having caught the player, the stale-latch that makes the arriving step's
    // detection and capture land the same turn. (A Calm guard would telegraph the
    // corner a turn early and detect at range, splitting the two events.)
    let mut guard = Guard::patrolling(Cell::new(6, 4));
    guard.respond_to(Cell::new(1, 4));
    let mut s = State::new(
        open_room(10, 10),
        Cell::new(4, 4),
        Direction::North,
        vec![guard],
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

/// §7.2/§6.1 regression: a guard that steps **adjacent to and facing** the player
/// during its own move phase (phase 3, §4.2) cannot be taken down from the front on
/// the next turn. The takedown gate must read the guard's *live* cone, not the
/// [`detected`](Guard::detected_player) latch phase 2 leaves behind — that latch is
/// a turn stale for a guard whose cone was refreshed by its move but whose detection
/// was not, so reading it would admit a takedown from directly in front (forbidden:
/// beside or in front is never a valid takedown, only the rear blind spot, §155).
///
/// A guard is driven up a forced L-corridor so that its **arriving** step turns it
/// to face the player: it walks east with its back to the player's column (the
/// player sits at perpendicular range 2, outside the ~90° wedge, unseen), then turns
/// north onto the cell directly below the player — now facing it point-blank, yet
/// its pre-move look never saw it. The next turn the player's bump must be refused.
///
/// The mover is a **reactive** guard (Responding): under §229 only a reactive guard
/// turns *and* steps in one action — a Calm guard would telegraph the corner by
/// rotating in place a turn early and so detect the player before arriving. A fast
/// arriving turn is exactly what leaves the detection latch a turn stale, which is
/// the case under test.
#[test]
fn a_guard_that_stepped_adjacent_facing_the_player_cannot_be_taken_from_the_front() {
    // A one-cell-wide elbow forces the guard's path: with the corner cell walled,
    // the only route from (4,5) to (5,4) is east-then-north, so the guard reaches
    // (5,4) by turning north — facing the player at (5,3) — on its final step.
    let mut layout = open_room(9, 9);
    layout.place(Cell::new(4, 4), Terrain::Wall); // the elbow: no north-then-east cut

    // A Responding guard walking to (5,4): reactive, so it turns fast at the corner
    // (§229) — the arriving north step both moves and re-aims, and reactive `decide`
    // draws no RNG, so the walk is deterministic (§12.4).
    let mut guard = Guard::patrolling(Cell::new(4, 5));
    guard.respond_to(Cell::new(5, 4));
    let mut s = State::new(
        layout,
        Cell::new(5, 3), // on open floor, north of the guard's approach
        Direction::North,
        vec![guard],
        Vec::new(),
        Cell::new(7, 7),
    );

    // The §4.2 startup turn already walked the guard its first step, east to (5,5)
    // with its back to the player. One more turn turns it north onto (5,4) — now
    // directly below and facing the player, yet its pre-move look (facing east) saw
    // only perpendicular range-2, outside the wedge, so it never detected them.
    s.step(Input::Wait);

    let guard = &s.guards()[0];
    assert_eq!(guard.pos(), Cell::new(5, 4), "the guard is now adjacent");
    assert_eq!(
        guard.facing(),
        Direction::North,
        "facing the player point-blank"
    );
    assert!(
        !guard.detected_player(),
        "the stale latch: its pre-move look (facing east) never saw the player",
    );
    assert!(
        guard.fov().contains(Cell::new(5, 3)),
        "yet its refreshed cone covers the player — a live look detects",
    );
    assert_eq!(
        s.affordances(),
        Vec::new(),
        "the usable line offers no takedown against a guard now facing you",
    );

    // The bump into that guard is refused as a free no-op, not a front takedown.
    let turn_before = s.turn();
    let events = s.step(Input::Step(Direction::South));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(5, 4)
        }],
        "a guard facing you refuses the takedown (§6.1/§155)",
    );
    assert_eq!(s.guards().len(), 1, "the guard still stands");
    assert!(s.bodies().is_empty(), "no body — nothing was taken down");
    assert_eq!(s.turn(), turn_before, "a refused takedown is a free bump");
    assert_eq!(s.player(), Cell::new(5, 3), "a refused bump does not move");
}

/// §7.2/§15 Q5: a body does not block sight, so the first cone to cover it fires the
/// found-body event — exactly once, found is found. And, because a found body is loud
/// evidence the intruder is close, the finder's §7.6 search **checks the cupboard beside
/// it**: the player hid in the very cupboard next to the guard they dropped, so the
/// finder flushes and catches them — hiding beside a body you left is the readable
/// mistake (§2.2, #185's found-a-body-nearby second earn-entry path).
#[test]
fn a_found_body_is_registered_once_and_flushes_the_cupboard_beside_it() {
    // A one-wide corridor along x=5, so the finder pacing down it cannot wander off the
    // column its cone covers — it reliably finds the body and reaches the cupboard.
    let mut layout = open_room(11, 11); // interior x 1..9, y 1..9
    for y in 1..10 {
        layout.place(Cell::new(4, y), Terrain::Wall); // west wall
        layout.place(Cell::new(6, y), Terrain::Wall); // east wall
    }
    layout.place(Cell::new(5, 7), Terrain::Hideout); // the player's cupboard
    let mut s = State::new(
        layout,
        Cell::new(5, 7), // hidden, striking north
        Direction::North,
        vec![
            Guard::stationary(Cell::new(5, 6)), // the victim, one north
            // A finder pacing south down the corridor, cone straight over the victim's
            // cell: it sees the body the turn it appears. It patrols, so once it has
            // checked the cupboard it walks to the mouth and captures.
            Guard::patrolling_to(Cell::new(5, 1), Cell::new(5, 5)),
        ],
        Vec::new(),
        Cell::new(5, 9),
    );

    let body = Cell::new(5, 6);
    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::TakenDown { at: body }, Event::BodyFound { at: body }],
        "the finder's cone covers the fresh body: found the same turn",
    );
    assert_eq!(s.bodies()[0].cell(), body);
    assert!(s.bodies()[0].found());
    // §15 Q5, the found-a-body-nearby half: a body one cell from the occupied cupboard
    // is loud evidence the intruder hid close, so the finder's §7.6 search checks that
    // cupboard — earning entry the way a witness does (#185), the capture gate opening
    // for it alone (`witnessed_hideout`).
    assert_eq!(
        s.guards()[0].witnessed_hideout(),
        Some(Cell::new(5, 7)),
        "the finder checks the cupboard beside the body it found",
    );

    // It walks to the mouth and captures — a cupboard is no refuge one cell from a body
    // a guard has found — and the loudest event never repeats on the way (found is found).
    let mut captured = false;
    for _ in 0..6 {
        let events = s.step(Input::Wait);
        assert!(
            !events.iter().any(|e| matches!(e, Event::BodyFound { .. })),
            "the found-body event fires exactly once per body",
        );
        if events.iter().any(|e| matches!(e, Event::Captured { .. })) {
            captured = true;
            break;
        }
    }
    assert!(captured, "hiding beside the found body is caught (§15 Q5)");
    assert_eq!(s.outcome(), Outcome::Lost);
}

/// §7.2 (revised): a body is **non-solid** to guards too — a guard routes and
/// steps straight over one. A walker aimed at the body's cell reaches it and
/// stands on it, rather than being refused.
#[test]
fn a_guard_walks_over_a_bodys_cell() {
    // A one-wide vertical corridor along x=4, so a guard crossing the body's cell has
    // no way around it — the cleanest proof the body is non-solid to guards. The player
    // takes the victim from behind on open floor (never a hidden cupboard occupant, so
    // no §15 Q5 check applies) and ducks aside into a side duct.
    let mut layout = open_room(9, 11); // interior x 1..7, y 1..9
    for y in 1..10 {
        if y != 4 {
            layout.place(Cell::new(3, y), Terrain::Wall); // west wall, gap at y=4
        }
        layout.place(Cell::new(5, y), Terrain::Wall); // east wall
    }
    layout.place(Cell::new(3, 4), Terrain::DuctEntry); // the duck, through the west gap
    layout.place(Cell::new(2, 4), Terrain::Wall);
    let layout = layout.with_ducts(vec![crate::Duct::new(vec![
        Cell::new(3, 4),
        Cell::new(2, 4),
    ])]);
    let mut s = State::new(
        layout,
        Cell::new(4, 4), // on open floor, directly behind the south-facing victim
        Direction::South,
        vec![
            Guard::stationary(Cell::new(4, 5)), // the victim, one south
            // A walker at the south end, pacing north up the corridor: its only route
            // over the body's cell.
            Guard::patrolling_to(Cell::new(4, 8), Cell::new(4, 1)),
        ],
        Vec::new(),
        Cell::new(4, 9),
    );

    let body = Cell::new(4, 5);
    s.step(Input::Step(Direction::South)); // the takedown; the body lies at (4,5)
    assert_eq!(s.bodies()[0].cell(), body);
    s.step(Input::Step(Direction::West)); // duck into the duct, out of the way
    assert!(s.in_duct(), "safe in the duct, not a flushable cupboard");

    // The walker paces up the one-wide corridor, so it must stand on the non-solid
    // body to pass — and the player stays safe in the duct throughout.
    let mut stood_on_body = false;
    for _ in 0..16 {
        s.step(Input::Wait);
        stood_on_body |= s.guards()[0].pos() == body;
        assert_eq!(s.outcome(), Outcome::Playing, "safe in the duct all along");
    }
    assert!(
        stood_on_body,
        "a non-solid body's cell admits a guard (§7.2)"
    );
}

/// #182 (§7.2/§7.8): a body dropped in a chokepoint must not freeze a guard. The
/// only route between the two sides of a 1-wide gap runs through the body's cell;
/// because the body is non-solid, a guard routes over it and reaches the far side,
/// rather than stalling forever adjacent to it.
#[test]
fn a_body_in_a_chokepoint_does_not_freeze_a_guard() {
    let mut layout = open_room(9, 11); // interior x 1..7, y 1..9
                                       // A solid wall row at y=5, with a single gap at (4,5): the chokepoint.
    for x in 1..8 {
        if x != 4 {
            layout.place(Cell::new(x, 5), Terrain::Wall);
        }
    }
    // The player's duck, off the route: a duct, not a cupboard — a hideout one cell
    // from the body would now be *checked* by the investigator's found-body search
    // (§15 Q5), but a duct is contact-safe and never flushed (§10.7).
    layout.place(Cell::new(3, 4), Terrain::DuctEntry);
    layout.place(Cell::new(2, 4), Terrain::Wall);
    let layout = layout.with_ducts(vec![crate::Duct::new(vec![
        Cell::new(3, 4),
        Cell::new(2, 4),
    ])]);
    let mut s = State::new(
        layout,
        Cell::new(4, 4), // just north of the gap
        Direction::North,
        vec![
            Guard::stationary(Cell::new(4, 5)), // the victim, in the gap
            // An investigator on the south side, patrolling to the north — its only
            // route is up through the gap the body will occupy. It faces south on turn
            // one, so it does not find the body until it has turned north — by when the
            // player has slipped into the duct.
            Guard::patrolling_to(Cell::new(4, 8), Cell::new(4, 2)),
        ],
        Vec::new(),
        Cell::new(7, 9),
    );

    let gap = Cell::new(4, 5);
    s.step(Input::Step(Direction::South)); // take down the victim in the gap
    assert_eq!(s.bodies()[0].cell(), gap, "the body lies in the chokepoint");
    s.step(Input::Step(Direction::West)); // duck into the duct, out of the way
    assert!(s.in_duct());

    // The investigator makes it to the north side: it routes over the non-solid
    // body instead of freezing on the far side of the gap. If the body were a
    // wall, the only route would be sealed and it could never reach y <= 5.
    let mut crossed = false;
    for _ in 0..16 {
        s.step(Input::Wait);
        crossed |= s.guards()[0].pos().y <= 5;
        assert_eq!(
            s.outcome(),
            Outcome::Playing,
            "the player is hidden and safe"
        );
    }
    assert!(crossed, "the guard crosses the body-blocked gap (#182)");
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
    let mut s = dragging_a_body(); // player (6,4), dragging the body at (5,4), debt owed
    s.step(Input::Activate(AbilityId::Run)); // a spent turn — it also pays the pending debt

    // Run is active, but dragging pins movement to half speed: one cell, not two —
    // the sprint's extra step never fires while a body is in hand.
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        s.player(),
        Cell::new(7, 4),
        "one cell only, Run notwithstanding"
    );
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(7, 4)
        }]
    );
    assert_eq!(s.bodies()[0].cell(), Cell::new(6, 4), "the body follows");

    // And the next step owes the haul again — still half speed under Run.
    s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(7, 4), "the debt turn holds under Run");
}

/// The drag scenario (§8.3): the cupboard takedown, then climb out onto the body
/// and step off it to **take hold** — a body is non-solid, so the grab is walking
/// over it and off its cell, not a bump. Ends with the player at (6,4) dragging the
/// body at (5,4), a haul debt owed (the pickup rode a full step).
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
    s.step(Input::Step(Direction::North)); // climb out of the cupboard onto the body
    let events = s.step(Input::Step(Direction::East)); // step off — take hold
    assert_eq!(
        events,
        vec![
            Event::Moved {
                to: Cell::new(6, 4)
            },
            Event::BodyGrabbed {
                at: Cell::new(5, 4)
            }
        ]
    );
    assert_eq!(s.dragging(), Some(Cell::new(5, 4)));
    assert_eq!(s.player(), Cell::new(6, 4));
    s
}

/// §8.3: "you move at half speed while dragging", by the documented debt
/// convention: a dragging move succeeds and leaves a haul debt, the next
/// step is spent but stationary, and the one after moves again — one cell
/// per two spent turns, with the body following into each vacated cell.
/// Taking hold rides a full step and owes the first debt; releasing is free.
#[test]
fn dragging_moves_at_half_speed_and_the_body_follows() {
    let mut s = dragging_a_body();
    assert_eq!(
        s.turn(),
        3,
        "takedown, the climb-out, and the grab all spend"
    );

    // The grab's debt: the first step is spent but stationary and silent.
    let events = s.step(Input::Step(Direction::East));
    assert!(events.is_empty(), "the debt turn narrates nothing");
    assert_eq!(s.player(), Cell::new(6, 4), "no movement on the debt turn");
    assert_eq!(s.turn(), 4, "but the turn is spent");

    // Debt paid: a full step, the body following into the vacated cell.
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(7, 4)
        }]
    );
    assert_eq!(s.player(), Cell::new(7, 4));
    assert_eq!(s.bodies()[0].cell(), Cell::new(6, 4), "the body follows");

    // Half speed holds: the next step owes the debt, the one after moves again.
    s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(7, 4), "the debt turn holds");
    s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(8, 4));
    assert_eq!(s.bodies()[0].cell(), Cell::new(7, 4));
}

/// §8.3/§4.4: release is free and refunds nothing — the bump against the
/// held body lets it go where it lies, the turn does not advance, and the
/// player moves at full speed again while the body stays put.
#[test]
fn releasing_the_body_is_free_and_it_stays_where_it_lies() {
    let mut s = dragging_a_body(); // player (6,4), holding the body at (5,4)
    let turn = s.turn();

    let events = s.step(Input::Step(Direction::West)); // bump the held body behind
    assert_eq!(
        events,
        vec![Event::BodyReleased {
            at: Cell::new(5, 4)
        }]
    );
    assert_eq!(s.turn(), turn, "release is free");
    assert_eq!(s.dragging(), None);

    // Full speed again — consecutive steps both move — and the body stays put.
    s.step(Input::Step(Direction::North));
    s.step(Input::Step(Direction::North));
    assert_eq!(s.player(), Cell::new(6, 2), "no lingering debt");
    assert_eq!(
        s.bodies()[0].cell(),
        Cell::new(5, 4),
        "the body stays where it lay"
    );
}

/// While dragging, the usable line offers the release on the held body behind
/// you, and a wall bump stays free without moving anything (§4.4 — cannot drag
/// through a wall).
#[test]
fn dragging_affordances_and_walls() {
    let mut s = dragging_a_body(); // player (6,4), body (5,4) to the west, debt owed
    assert_eq!(
        s.affordances(),
        vec![(Direction::West, Affordance::ReleaseBody)],
        "the held body offers the release",
    );

    // Haul north to the border wall: debt — move — debt — move…
    s.step(Input::Step(Direction::North)); // debt
    s.step(Input::Step(Direction::North)); // (6,3), body (6,4)
    s.step(Input::Step(Direction::North)); // debt
    s.step(Input::Step(Direction::North)); // (6,2), body (6,3)
    s.step(Input::Step(Direction::North)); // debt
    s.step(Input::Step(Direction::North)); // (6,1), body (6,2)
    s.step(Input::Step(Direction::North)); // debt
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

/// §7.2's hide payoff, on the new deposit model (§8.3/§10.3): drag the body to a
/// cupboard and **bump it to stow the body inside** — a spent turn that leaves the
/// player outside, hands free. A stowed body is *gone*: a guard whose cone sweeps
/// the cupboard finds nothing, ever.
#[test]
fn a_stowed_body_is_gone() {
    let mut layout = open_room(12, 24);
    layout.place(Cell::new(5, 5), Terrain::Hideout); // the player's start cupboard
    layout.place(Cell::new(5, 2), Terrain::Hideout); // the stow cupboard
    layout.place(Cell::new(6, 3), Terrain::Hideout); // the player's duck
    let mut s = State::new(
        layout,
        Cell::new(5, 5), // hidden, so the victim never sees the takedown coming
        Direction::North,
        vec![
            Guard::stationary(Cell::new(5, 4)), // the victim, adjacent
            // A witness marching up the column, far enough that the player is
            // hidden again before its cone arrives; it ends watching the cupboards.
            Guard::patrolling_to(Cell::new(5, 21), Cell::new(5, 4)),
        ],
        Vec::new(),
        Cell::new(10, 22),
    );

    s.step(Input::Step(Direction::North)); // takedown from the cupboard: body at (5,4)
    s.step(Input::Step(Direction::North)); // climb out onto the body
    s.step(Input::Step(Direction::North)); // step off to (5,3) — take hold
    assert_eq!(s.dragging(), Some(Cell::new(5, 4)));
    let stow = Cell::new(5, 2);
    let events = s.step(Input::Step(Direction::North)); // bump the cupboard: stow it
    assert_eq!(events, vec![Event::BodyStored { at: stow }]);
    assert_eq!(s.bodies()[0].cell(), stow, "stowed in the cupboard");
    assert_eq!(
        s.layout().facility().terrain(stow),
        Some(Terrain::Hideout),
        "a body can occupy a hideout cell",
    );
    assert_eq!(s.dragging(), None, "hands free after stowing");
    assert_eq!(s.player(), Cell::new(5, 3), "the player stays outside");

    s.step(Input::Step(Direction::East)); // duck into the player's own cupboard
    assert!(s.hidden());

    // The witness arrives and sweeps the stow cupboard: the stowed body fires
    // nothing, the hidden player is not seen, and nothing ever escalates.
    let mut swept = false;
    for _ in 0..14 {
        let events = s.step(Input::Wait);
        swept |= s.guards()[0].fov().contains(stow);
        assert!(
            !events.iter().any(|e| matches!(e, Event::BodyFound { .. })),
            "a stowed body is gone (§7.2) — no cone finds it",
        );
        assert_eq!(s.outcome(), Outcome::Playing);
    }
    assert!(
        swept,
        "precondition: a guard's cone did sweep the stow cupboard"
    );
    assert!(!s.bodies()[0].found());
}

/// #170 (§7.2/§10.3): a takedown from inside a cupboard can drop the body onto
/// the cupboard's only mouth — its sole exit. Because the body is non-solid, the
/// player walks straight out over it, so the run is never soft-locked.
#[test]
fn a_takedown_from_a_cupboard_never_traps_the_player() {
    let mut layout = open_room(10, 10);
    // A recessed cupboard at (5,5): solid on three sides, one mouth to the south.
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    layout.place(Cell::new(4, 5), Terrain::Wall);
    layout.place(Cell::new(6, 5), Terrain::Wall);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),                          // the player, hidden inside
        Direction::South,                         // facing the mouth
        vec![Guard::stationary(Cell::new(5, 6))], // a guard on the mouth, facing away
        Vec::new(),
        Cell::new(8, 8),
    );

    let mouth = Cell::new(5, 6);
    s.step(Input::Step(Direction::South)); // takedown: the body drops on the mouth
    assert!(s.hidden(), "still in the cupboard");
    assert_eq!(
        s.bodies()[0].cell(),
        mouth,
        "the body lies on the only exit"
    );

    // The escape: step onto the non-solid body, out of the cupboard.
    s.step(Input::Step(Direction::South));
    assert_eq!(s.player(), mouth, "walked out over the body");
    assert!(!s.hidden(), "no longer trapped");

    // And free to carry on — the run is not soft-locked (the body comes along).
    s.step(Input::Step(Direction::South));
    assert_eq!(
        s.player(),
        Cell::new(5, 7),
        "moving freely away from the cupboard"
    );
}

/// §7.2/§10.3: stowing a body in a cupboard **locks** it — it is no longer a
/// hideout. The player cannot climb into a cupboard that holds a body, and the
/// usable line stops offering the hide.
#[test]
fn stowing_a_body_locks_the_cupboard() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 7), Terrain::Hideout); // the stow cupboard
    let mut s = State::new(
        layout,
        Cell::new(5, 4), // north of the victim
        Direction::South,
        vec![Guard::stationary(Cell::new(5, 5))], // faces south, away from the player
        Vec::new(),
        Cell::new(8, 8),
    );

    s.step(Input::Step(Direction::South)); // takedown at (5,5)
    s.step(Input::Step(Direction::South)); // climb onto the body
    s.step(Input::Step(Direction::South)); // step off to (5,6) — take hold
    assert_eq!(s.dragging(), Some(Cell::new(5, 5)));
    assert_eq!(s.player(), Cell::new(5, 6));

    let stow = Cell::new(5, 7);
    let events = s.step(Input::Step(Direction::South)); // bump the cupboard: stow
    assert_eq!(events, vec![Event::BodyStored { at: stow }]);
    assert_eq!(s.bodies()[0].cell(), stow);
    assert_eq!(s.dragging(), None, "hands free");
    assert_eq!(s.player(), Cell::new(5, 6), "the player stayed outside");

    // The cupboard is locked: bumping it does nothing, and no hide is offered.
    let events = s.step(Input::Step(Direction::South));
    assert_eq!(events, vec![Event::Bumped { into: stow }]);
    assert!(!s.hidden(), "cannot climb into a locked cupboard");
    assert_eq!(s.player(), Cell::new(5, 6));
    assert!(
        !s.affordances().iter().any(|(_, a)| *a == Affordance::Hide),
        "the usable line no longer offers the hide",
    );
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

    // The startup turn is a quarter-turn in place (§229): heading east off its south
    // spawn facing, the Calm guard rotates east without moving; a Wait then walks it
    // up against the closed panel.
    assert_eq!(s.guards()[0].pos(), Cell::new(2, 2));
    assert_eq!(s.guards()[0].facing(), Direction::East);
    s.step(Input::Wait);
    assert_eq!(s.guards()[0].pos(), Cell::new(3, 2));
    assert!(!s.layout().regions().door(door).is_open());

    // Its next step is *into* the panel: the walk-in opens the door instead.
    let events = s.step(Input::Wait);
    assert!(events.contains(&Event::DoorOpened {
        at: panel,
        by_player: false,
    }));
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

    // The startup turn is a quarter-turn in place (§229): heading east off its south
    // spawn facing, the Calm guard rotates east without moving; a Wait then walks it
    // up against the closed panel (§10.4).
    assert_eq!(s.guards()[0].pos(), Cell::new(2, 2));
    assert_eq!(s.guards()[0].facing(), Direction::East);
    s.step(Input::Wait);
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
        events.contains(&Event::DoorClosed {
            at: panel,
            by_player: false,
        }),
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
    assert!(opened.contains(&Event::DoorOpened {
        at: panel,
        by_player: true,
    }));
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
        let opened = s.step(Input::Step(dir))
            == vec![Event::DoorOpened {
                at: panel,
                by_player: true,
            }];
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
            vec![Event::DoorOpened {
                at: panel,
                by_player: true,
            }]
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
    assert_eq!(
        events,
        vec![Event::DoorOpened {
            at: hinge,
            by_player: true,
        }]
    );
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
    // A guard dispatched (Responding) to the cupboard cell (4,4) where the player
    // hides. Reactive, so it turns fast (§229): after the startup step it is at
    // (5,4), one step from the player's cell — the destination it will be refused
    // entry to. (Any state routes around an occupied hideout, §10.3.)
    let mut guard = Guard::patrolling(Cell::new(6, 4));
    guard.respond_to(Cell::new(4, 4));
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![guard],
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
///
/// The dive is **unwitnessed** (§15 Q5): a wall recesses the cupboard into the guard's
/// sight-shadow, so the guard never sees the player climb in and cannot flush them —
/// exactly the "break sight *first*, then hide" the hiding game now rewards. Diving in
/// while the chaser's cone still covered the cupboard would (correctly) be caught,
/// which is its own test below.
#[test]
fn a_hidden_player_waits_out_a_search_and_watches_the_guard_leave() {
    let mut layout = open_room(16, 12);
    layout.place(Cell::new(4, 5), Terrain::Hideout); // a cupboard beside the player
                                                     // A wall recessing the cupboard: it blocks the guard's line to (4,5), so the dive
                                                     // west lands in the guard's sight-shadow and is never witnessed (§15 Q5).
    layout.place(Cell::new(4, 4), Terrain::Wall);
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

    // The player ducks west into the cupboard and holds. The guard loses sight — and,
    // with the cupboard walled into its shadow, never saw the dive, so it cannot flush.
    s.step(Input::Step(Direction::West));
    assert!(s.hidden(), "the player is concealed");
    assert_eq!(
        s.guards()[0].witnessed_hideout(),
        None,
        "the occluded dive is unwitnessed (§15 Q5)",
    );

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

/// The `guards_always_search_hideouts` level modifier (§12.6), directional at the
/// run level: on the *same* scene and input path that
/// [`a_hidden_player_waits_out_a_search_and_watches_the_guard_leave`] leaves the
/// player safe, turning the modifier on flushes them. Baseline, the unwitnessed
/// dive rides out the lost-chase search (§10.3, the §7.6 wait-out); with the
/// modifier the same search checks the cupboard inside its disc and captures — at
/// least as much pressure as baseline, the anti-facade proof the modifier bites
/// (§2.3). Only the modifier differs between the two runs.
#[test]
fn the_always_search_hideouts_modifier_flushes_a_wait_out() {
    // The shared scene: a cupboard walled into the guard's sight-shadow so the dive
    // west is never witnessed (§15 Q5) — the guard only *lost* the player, which
    // baseline never flushes.
    let scene = || {
        let mut layout = open_room(16, 12);
        layout.place(Cell::new(4, 5), Terrain::Hideout);
        layout.place(Cell::new(4, 4), Terrain::Wall);
        let guards = vec![Guard::patrolling(Cell::new(5, 1))];
        State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            guards,
            Vec::new(),
            Cell::new(14, 10),
        )
    };

    // Baseline (modifier off): the player dives west and rides out the search.
    let mut baseline = scene();
    baseline.step(Input::Step(Direction::West));
    assert!(baseline.hidden(), "the player is concealed");
    for _ in 0..60 {
        baseline.step(Input::Wait);
    }
    assert_eq!(
        baseline.outcome(),
        Outcome::Playing,
        "baseline: the unwitnessed dive is a safe wait-out",
    );
    assert!(baseline.hidden(), "baseline: the player is never flushed");

    // Modifier on: the identical dive and input path is flushed by the lost-chase
    // search that now checks the cupboard inside its disc.
    let mut modified = scene().with_modifiers(LevelModifiers {
        guards_always_search_hideouts: true,
        ..LevelModifiers::default()
    });
    modified.step(Input::Step(Direction::West));
    assert!(modified.hidden(), "the player is concealed");
    let mut captured = false;
    for _ in 0..60 {
        let events = modified.step(Input::Wait);
        if events.iter().any(|e| matches!(e, Event::Captured { .. })) {
            captured = true;
            break;
        }
    }
    assert!(
        captured,
        "modifier: the lost-chase search flushed the hidden player",
    );
    assert_eq!(
        modified.outcome(),
        Outcome::Lost,
        "modifier: the wait-out is no longer safe",
    );
}

/// §15 Q5: **a guard that saw you climb in flushes you out.** A chasing guard whose
/// cone covers the cupboard on the entry turn witnessed the dive — so it re-engages
/// the alcove, walks to the mouth, and captures the hidden player, rather than being
/// refused forever (§10.3). This is the loophole the ticket closes: diving in while a
/// hunter watches is not a free escape.
#[test]
fn a_guard_that_saw_the_dive_flushes_the_hidden_player() {
    let mut layout = open_room(16, 12);
    layout.place(Cell::new(4, 5), Terrain::Hideout); // a cupboard beside the player
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
        "precondition: the guard is hunting the player",
    );

    // The player dives west into the cupboard — in plain sight of the chaser, whose
    // cone covers (4,5) this turn. That is a witnessed dive (§15 Q5).
    s.step(Input::Step(Direction::West));
    assert!(s.hidden(), "the player is in the cupboard");
    assert_eq!(
        s.guards()[0].witnessed_hideout(),
        Some(Cell::new(4, 5)),
        "the chaser witnessed the dive and is flushing this cupboard",
    );

    // It walks to the mouth and captures — a cupboard is no refuge from a guard that
    // saw you climb in.
    let mut captured = false;
    for _ in 0..10 {
        let events = s.step(Input::Wait);
        if events.iter().any(|e| matches!(e, Event::Captured { .. })) {
            captured = true;
            break;
        }
    }
    assert!(
        captured,
        "the witnessing guard flushed and caught the player"
    );
    assert_eq!(s.outcome(), Outcome::Lost, "a witnessed dive is not safe");
}

/// §15 Q5, the other half: **only an *alerted* guard checks.** A **Calm** patrol whose
/// cone merely grazes the cupboard as the player climbs in is not hunting — it never
/// saw the player (they were concealed throughout), so it does not check, and the
/// cupboard stays the safe room it is (§10.3). The distinction is the guard's *mood*,
/// not the geometry: the cone covers the entry either way.
#[test]
fn a_calm_patrol_that_sees_the_cupboard_does_not_check() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 5), Terrain::Hideout); // where the player starts, concealed
    layout.place(Cell::new(6, 5), Terrain::Hideout); // the cupboard they slip across into
    let mut s = State::new(
        layout,
        Cell::new(5, 5), // start hidden, so the guard never detects the player
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 1))], // faces south; cone falls over both cupboards
        Vec::new(),
        Cell::new(10, 10),
    );
    assert!(s.hidden(), "precondition: the player begins concealed");
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Calm,
        "precondition: a Calm patrol that never saw the player",
    );
    assert!(
        s.guards()[0].fov().contains(Cell::new(6, 5)),
        "precondition: the Calm guard's cone does cover the entry cell",
    );

    // Slip from one cupboard straight into the adjacent one — a fresh entry, in the
    // Calm guard's cone. It is not alerted, so it does not witness or flush.
    s.step(Input::Step(Direction::East));
    assert!(s.hidden(), "the player is in the second cupboard");
    assert_eq!(
        s.guards()[0].witnessed_hideout(),
        None,
        "a Calm guard does not check a cupboard it saw the player enter",
    );

    // It stays Calm and never captures — the cupboard is still a safe room.
    for _ in 0..20 {
        s.step(Input::Wait);
        assert_eq!(s.outcome(), Outcome::Playing, "the hidden player is safe");
    }
}

/// §10.2 [START]: the exit opens once **at least one** intel is in hand — one objective
/// is a complete run. Taking a single console of several and reaching the exit wins.
#[test]
fn one_intel_opens_the_exit() {
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        [Cell::new(5, 4), Cell::new(8, 8)], // two objectives; one is enough
        Cell::new(5, 6),
    );
    assert!(!s.exit_ready(), "empty-handed: the exit is not yet open");
    // Bumping the exit with no intel refuses (free, §4.5).
    let events = s.step(Input::Step(Direction::South));
    assert!(events.contains(&Event::ExitRefused), "refused empty-handed");
    assert_eq!(s.outcome(), Outcome::Playing);

    // Take one console (bump north), leaving the other out.
    s.step(Input::Step(Direction::North));
    assert_eq!(s.objectives_remaining(), 1, "one intel still out");
    assert!(s.exit_ready(), "one intel in hand opens the exit");

    // Reach the exit and leave — a win on a single objective.
    let events = s.step(Input::Step(Direction::South));
    assert!(events.contains(&Event::Won), "one intel + exit is a win");
    assert_eq!(s.outcome(), Outcome::Won);
}

/// #244: the intel gate is a level modifier. Under [`IntelGate::All`] — quick
/// play's objective (§10.2) — the exit refuses until **every** console is taken,
/// where the [`IntelGate::AtLeastOne`] baseline opened on the first. Same facility,
/// a different objective: exactly the seam #244 asks for.
#[test]
fn the_all_intel_gate_requires_the_full_set() {
    use crate::modifiers::IntelGate;
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        [Cell::new(5, 4), Cell::new(6, 5)], // two objectives; both now required
        Cell::new(5, 6),
    )
    .with_modifiers(LevelModifiers {
        intel_to_exit: IntelGate::All,
        ..LevelModifiers::default()
    });

    // Take the first console (bump north), leaving the second out.
    s.step(Input::Step(Direction::North));
    assert_eq!(s.objectives_remaining(), 1, "one intel still out");
    assert!(
        !s.exit_ready(),
        "the all-intel gate holds the exit shut on a partial set",
    );
    let events = s.step(Input::Step(Direction::South));
    assert!(
        events.contains(&Event::ExitRefused),
        "the exit refuses a partial set under the all-intel gate",
    );
    assert_eq!(s.outcome(), Outcome::Playing);

    // Take the second console (bump east): now the whole set is in hand.
    s.step(Input::Step(Direction::East));
    assert_eq!(s.objectives_remaining(), 0, "the full set is in hand");
    assert!(
        s.exit_ready(),
        "the all-intel gate opens once every intel is taken"
    );
    let events = s.step(Input::Step(Direction::South));
    assert!(events.contains(&Event::Won), "all intel + exit is a win");
    assert_eq!(s.outcome(), Outcome::Won);
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

    // A reactive guard (Responding) turns fast (§229): its startup step west reaches
    // (5,4) adjacent to the player without a telegraphed corner-turn.
    let mut guard = Guard::patrolling(Cell::new(6, 4));
    guard.respond_to(Cell::new(1, 4));
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![guard],
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
    // The startup turn is a quarter-turn in place (§229): heading west off its south
    // spawn facing, the guard rotates west without moving, its cone re-aimed at once.
    let g = &s.guards()[0];
    assert_eq!(g.pos(), Cell::new(8, 8), "the quarter-turn did not move it");
    assert_eq!(g.facing(), Direction::West);
    assert!(g.fov().contains(Cell::new(6, 8)), "the wedge points west");
    assert!(!g.fov().contains(Cell::new(10, 8)), "not behind it");

    // Now aligned, each turn steps west and the stored cone moves with the guard.
    s.step(Input::Wait);
    let g = &s.guards()[0];
    assert_eq!(g.pos(), Cell::new(7, 8));
    assert!(g.fov().contains(Cell::new(5, 8)) && !g.fov().contains(Cell::new(9, 8)));

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
    assert_eq!(
        Event::DoorOpened {
            at,
            by_player: true,
        }
        .category(),
        Category::System
    );
    assert_eq!(
        Event::DoorClosed {
            at,
            by_player: false,
        }
        .category(),
        Category::System
    );
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

/// §12.4 + §12.6: the active level modifiers are part of the reproducible config.
/// Same seed + **same modifiers** + same inputs → identical run (determinism holds
/// with a non-default set threaded in), and the same seed + inputs under a
/// *different* modifier set yields a *different* run — proving the modifiers
/// genuinely feed the run rather than riding along inert (§2.3). The scene is the
/// §12.6 hideout flush: baseline rides out the lost-chase search, the harder
/// modifier is caught.
#[test]
fn a_run_is_reproducible_from_its_seed_modifiers_and_inputs() {
    const SEED: u64 = 0x125;
    let mut inputs = vec![Input::Step(Direction::West)];
    inputs.extend(std::iter::repeat_n(Input::Wait, 60));

    let run = |modifiers: LevelModifiers| {
        let mut layout = open_room(16, 12);
        layout.place(Cell::new(4, 5), Terrain::Hideout);
        layout.place(Cell::new(4, 4), Terrain::Wall);
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            vec![Guard::patrolling(Cell::new(5, 1))],
            Vec::new(),
            Cell::new(14, 10),
        )
        .with_rng(Rng::new(SEED))
        .with_modifiers(modifiers);
        let events: Vec<Event> = inputs.iter().flat_map(|&i| s.step(i)).collect();
        (events, s.outcome(), s.player(), s.hidden(), s.turn())
    };

    let harder = LevelModifiers {
        guards_always_search_hideouts: true,
        ..LevelModifiers::default()
    };

    // Same seed + same modifiers + same inputs → byte-identical run, twice over.
    assert_eq!(
        run(harder),
        run(harder),
        "a run is deterministic given its seed, modifiers, and inputs",
    );
    // Same seed + inputs, different modifiers → a different run: the set is config,
    // not decoration.
    assert_ne!(
        run(LevelModifiers::default()).1,
        run(harder).1,
        "the modifier set changes the run's outcome (it is part of the config)",
    );
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

/// §11.5/#223: [`visible_cone_cells`] is *exactly* a seen guard's cone (no
/// concealment here), and [`in_visible_danger`] mirrors membership — a cell in the
/// cone is danger, a cell outside it is not. This is the set the shell's
/// held-movement guard reads, the same one the renderer paints.
#[test]
fn the_visible_cone_set_matches_a_seen_guards_cone() {
    // Player at (10,10) facing north; guard adjacent at (9,9), in the FOV, looking
    // south so its wedge covers the player — the same scene the overlay golden uses.
    let s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(Cell::new(9, 9))],
        Vec::new(),
        Cell::new(18, 18),
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Seen),
        "precondition: the guard is in view",
    );
    let cone: std::collections::HashSet<Cell> = s.guards()[0].fov().cells().collect();
    let visible: std::collections::HashSet<Cell> = s.visible_cone_cells().collect();
    assert_eq!(
        visible, cone,
        "the visible-danger set is exactly the seen cone"
    );

    let watched = Cell::new(9, 11); // straight down the wedge
    assert!(cone.contains(&watched));
    assert!(s.in_visible_danger(watched), "a watched cell is danger");
    let clear = Cell::new(0, 0); // a corner the cone never reaches
    assert!(!cone.contains(&clear));
    assert!(
        !s.in_visible_danger(clear),
        "a cell outside the cone is not danger"
    );
}

/// §11.5 [SETTLED] held through the new query: an **unseen** guard's cone is unknown
/// information, so it is not visible danger — [`visible_cone_cells`] is empty and
/// [`in_visible_danger`] is false over the guard's own (out-of-view) cone.
#[test]
fn an_unseen_guards_cone_is_not_visible_danger() {
    // Guard behind the north-facing player at (10,14), looking south (spawn) — away
    // from the player, so it never detects, and unseen, so its cone paints nothing.
    let guard = Cell::new(10, 14);
    let s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(guard)],
        Vec::new(),
        Cell::new(18, 18),
    );
    assert!(
        !s.player_fov().contains(guard),
        "precondition: the guard is unseen"
    );
    assert_eq!(
        s.visible_cone_cells().count(),
        0,
        "an unseen guard contributes no visible cone",
    );
    assert_eq!(s.spot_flash().count(), 0, "it faces away — no fresh spot");
    // A cell the guard's cone genuinely covers (south of it, out of the FOV) is not
    // danger: knowledge the player does not have.
    let in_cone = Cell::new(10, 16);
    assert!(
        s.guards()[0].fov().contains(in_cone),
        "the cell is in the cone"
    );
    assert!(
        !s.player_fov().contains(in_cone),
        "but out of the player's FOV"
    );
    assert!(
        !s.in_visible_danger(in_cone),
        "an unseen cone is not visible danger"
    );
}

/// #223 with #250: a guard the player cannot see that **freshly** detects them
/// contributes no cone (it is unseen), but its momentary spot-flash sightline *is*
/// visible danger — the "you just got spotted, don't blindly march on" case. The
/// shell's held-movement guard reads it through the same query.
#[test]
fn a_fresh_spot_from_an_unseen_guard_is_visible_danger() {
    // Guard at (10,5) facing south; player five south at (10,10) facing south too —
    // the guard is directly behind, unseen, but its cone runs down over the player,
    // so at level start it freshly detects a player it is unseen by (§9.2).
    let s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::South,
        vec![Guard::stationary(Cell::new(10, 5))],
        Vec::new(),
        Cell::new(18, 18),
    );
    assert!(
        !s.player_fov().contains(Cell::new(10, 5)),
        "precondition: the spotter is behind the player, unseen",
    );
    assert_eq!(
        s.visible_cone_cells().count(),
        0,
        "the unseen cone paints no danger on its own",
    );
    // The fresh spot-flash sightline (10,6)..=(10,10) is danger the player can act on.
    for y in 6..=10 {
        assert!(
            s.in_visible_danger(Cell::new(10, y)),
            "the spot sightline at (10,{y}) is visible danger",
        );
    }
    assert!(
        !s.in_visible_danger(Cell::new(12, 8)),
        "off the sightline stays clear — a line, not the whole cone",
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

// --- Sensing doors (§9.4/§10.4) -----------------------------------------------

/// A hand-built wide strip: a left room and a right room joined by one **manual**
/// door at column 6 (hinges at `(6,1)`/`(6,3)`, panel at `(6,2)`), with a guard
/// patrolling from the left room through the door — so on its beat it walks the
/// closed panel open (§10.4), a change the player did not cause. The player starts at
/// `player` **facing east**, ahead of the door, and the drive below walks it further
/// east each turn: the guard opens the door *behind* the eastward-facing player, so
/// the changed cell is reliably out of the forward FOV (a Wait's 360° look would
/// otherwise see straight through the open doorway — sight and door-sense share the
/// same range, §9.1/§10.4). The close-behind is disabled so the open is isolated.
/// Returns the state and the door's panel cell.
fn guard_door_strip(width: u32, player: Cell) -> (State, Cell) {
    let mut f = Facility::walled_box(width, 6);
    let mut g = RegionGraph::new(width, 6);
    let column =
        |x0: u32, x1: u32| (1..5).flat_map(move |y| (x0..x1).map(move |x| Cell::new(x, y)));
    let left = g.add_region(RegionKind::Room, column(1, 6));
    let right = g.add_region(RegionKind::Room, column(7, width - 1));
    for y in 1..5 {
        f.set_terrain(6, y, Terrain::Wall);
    }
    f.set_terrain(6, 1, Terrain::DoorHinge);
    f.set_terrain(6, 2, Terrain::DoorPanelClosed);
    f.set_terrain(6, 3, Terrain::DoorHinge);
    g.add_door(
        left,
        right,
        [Cell::new(6, 1), Cell::new(6, 3)],
        [Cell::new(6, 2)],
        DoorKind::Manual,
    );
    let mut s = State::new(
        Layout::from_parts(f, g),
        player,
        Direction::East,
        vec![Guard::patrolling_to(Cell::new(4, 2), Cell::new(8, 2))],
        Vec::new(),
        Cell::new(width - 2, 4),
    );
    s.set_guard_close_chance(0); // isolate the open from the close-behind (#146)
    (s, Cell::new(6, 2))
}

/// Walk the player east until the patrolling guard opens the door behind them (the
/// first `by_player: false` open), returning once it has. Stepping east keeps the
/// player facing *away* from the door so the changed cell stays out of the forward
/// FOV. Panics if the guard never opens it.
fn drive_until_guard_opens(s: &mut State) {
    for _ in 0..8 {
        let e = s.step(Input::Step(Direction::East));
        if e.iter().any(|ev| {
            matches!(
                ev,
                Event::DoorOpened {
                    by_player: false,
                    ..
                }
            )
        }) {
            return;
        }
    }
    panic!("the patrolling guard never opened the door");
}

/// §9.4/§10.4 **[START]**: a door change is a louder, coarser event than a guard's
/// exact position, so it is felt **farther** — `DOOR_SENSE_RANGE > PLAYER_SENSE_RANGE`
/// — and both it and the cue decay are pinned so a later change is a visible edit.
#[test]
fn the_door_sense_range_exceeds_the_guard_sense() {
    assert_eq!(DOOR_SENSE_RANGE, 15, "the [START] door-sense range");
    assert_eq!(DOOR_CUE_DECAY_TURNS, 3, "the [START] cue decay");
    // The `DOOR_SENSE_RANGE > PLAYER_SENSE_RANGE` asymmetry itself is pinned at compile
    // time beside the constant (§9.4/§10.4), so it cannot silently invert.
}

/// §9.4/§10.4: unlike the guard sense, the door sense is **not** widened by a Wait —
/// a door change is already loud enough — but it **shrinks inside a duct** with the
/// rest of the crawlspace's degraded perception (§10.7).
#[test]
fn the_door_sense_is_not_widened_by_wait_but_shrinks_in_a_duct() {
    let mut s = solo(Cell::new(5, 5));
    assert_eq!(
        s.door_sense_range(),
        DOOR_SENSE_RANGE,
        "the base door-sense box"
    );
    s.step(Input::Wait);
    assert_eq!(
        s.door_sense_range(),
        DOOR_SENSE_RANGE,
        "a wait does not widen the door sense",
    );

    let mut d = duct_world();
    d.step(Input::Step(Direction::North)); // climb into the duct
    assert!(d.in_duct());
    assert_eq!(
        d.door_sense_range(),
        DUCT_SENSE_RANGE,
        "the door sense shrinks inside a duct, like the guard sense",
    );
}

/// §9.4/§10.4: a door a **guard** opens out of the player's FOV, within
/// `DOOR_SENSE_RANGE`, lights the door's **whole footprint** as a `Category::Sensed`
/// background — the same "sensed through a wall" channel as a guard, readable around
/// the corner — and the generic "the door opens" near-line self-narration is gone for
/// a door the player did not operate.
#[test]
fn a_guard_door_lights_a_cue_out_of_fov() {
    let (mut s, panel) = guard_door_strip(30, Cell::new(7, 2));
    drive_until_guard_opens(&mut s);

    // A door the player did not operate raises no near-line word — the grid cue is
    // the durable evidence instead (§11.7).
    assert_ne!(
        crate::status::near_line(&s).text,
        "the door opens",
        "a guard-opened door no longer narrates on the near line",
    );

    // The eastward-facing player has the door behind them: out of the forward FOV,
    // yet the cue is on the grid.
    assert!(
        !s.player_fov().contains(panel),
        "precondition: the changed door is out of the player's forward FOV",
    );
    assert!(
        s.door_cues().any(|c| c == panel),
        "a guard-opened door lights a cue read out of FOV",
    );

    // The cue lights the door's *whole* footprint — both hinges and the panel — in
    // the Sensed channel, not just the single panel the event named (§9.4).
    let door_cells: Vec<Cell> = {
        let regions = s.layout().regions();
        let id = regions.door_at(panel).expect("the panel belongs to a door");
        regions.door(id).cells().collect()
    };
    assert!(
        door_cells.len() >= 3,
        "the strip door is two hinges and a panel",
    );
    let g = crate::render::render(&s);
    for cell in door_cells {
        assert_eq!(
            g.get(cell.x, cell.y).bg,
            Some(crate::category::Category::Sensed),
            "the whole door is lit in the sensed channel, cell {cell:?}",
        );
    }
}

/// §9.4/§10.4: the cue is a **fading** mark — a door change is a discrete event, not
/// a standing position — so it stays lit for `DOOR_CUE_DECAY_TURNS` turns and is then
/// gone. Checked on `door_cues` directly, independent of FOV.
#[test]
fn a_door_cue_fades_over_its_decay() {
    let (mut s, panel) = guard_door_strip(30, Cell::new(7, 2));
    drive_until_guard_opens(&mut s);
    assert!(
        s.door_cues().any(|c| c == panel),
        "the cue is lit the turn the door changes",
    );

    // No further door events fire (one door, close disabled), so the single cue decays
    // cleanly: lit for DOOR_CUE_DECAY_TURNS turns, then gone.
    for n in 1..DOOR_CUE_DECAY_TURNS {
        s.step(Input::Step(Direction::East));
        assert!(
            s.door_cues().any(|c| c == panel),
            "the cue is still lit {n} turn(s) on",
        );
    }
    s.step(Input::Step(Direction::East));
    assert!(
        !s.door_cues().any(|c| c == panel),
        "the cue has faded after DOOR_CUE_DECAY_TURNS turns",
    );
}

/// §9.4/§10.4: a door change **beyond** `DOOR_SENSE_RANGE` is felt as nothing — the
/// same guard-opened door, but with the player parked out past the door-sense box.
#[test]
fn a_door_change_beyond_range_lights_no_cue() {
    let (mut s, panel) = guard_door_strip(30, Cell::new(24, 4));
    assert!(
        s.player().sight_distance(panel) > DOOR_SENSE_RANGE,
        "precondition: the player is beyond the door-sense box",
    );
    drive_until_guard_opens(&mut s);
    assert_eq!(
        s.door_cues().count(),
        0,
        "a change beyond DOOR_SENSE_RANGE lights no cue",
    );
}

/// §9.4: the door cue and a guard felt through a wall share **one sense channel** —
/// the same orange `Category::Sensed` background — so they read as one thing, not two
/// colours to tell apart. The guard opens the door, then steps onto the open panel:
/// the panel (guard-sensed) and a hinge (door-cued only) both render `Sensed`.
#[test]
fn the_door_cue_and_the_guard_sense_share_one_channel() {
    let (mut s, panel) = guard_door_strip(30, Cell::new(7, 2));
    drive_until_guard_opens(&mut s);
    let hinge = Cell::new(panel.x, panel.y - 1); // a frame cell of the same door
    s.step(Input::Step(Direction::East)); // the guard steps onto the open panel

    assert_eq!(
        s.guards()[0].pos(),
        panel,
        "the guard steps through onto the open panel",
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Sensed),
        "the guard on the panel is sensed through the wall, not seen",
    );
    assert!(
        s.door_cues().any(|c| c == hinge),
        "the door cue lights the frame cell too (whole door)",
    );

    let g = crate::render::render(&s);
    let sensed = Some(crate::category::Category::Sensed);
    assert_eq!(
        g.get(panel.x, panel.y).bg,
        sensed,
        "the guard-sensed panel reads Sensed",
    );
    assert_eq!(
        g.get(hinge.x, hinge.y).bg,
        sensed,
        "the door-cued frame reads the *same* Sensed channel — one colour, not two",
    );
}

/// §11.7/§10.4: a door **you** operate keeps its quiet near-line self-narration and
/// lights **no** on-grid cue — the cue is only for doors the player did not operate,
/// which is where the durable "someone passed" evidence belongs.
#[test]
fn a_door_you_open_keeps_its_word_and_lights_no_cue() {
    // Player in room A of the strip, next to the closed door; bump it open.
    let mut s = State::new(
        region_strip(),
        Cell::new(3, 2),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(13, 1),
    );
    let events = s.step(Input::Step(Direction::East)); // bump the panel at (4,2)
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DoorOpened {
                by_player: true,
                ..
            }
        )),
        "the player's bump opens the door",
    );
    assert_eq!(
        crate::status::near_line(&s).text,
        "the door opens",
        "a door you operate keeps its quiet self-narration (§11.7)",
    );
    assert_eq!(
        s.door_cues().count(),
        0,
        "a door you operate lights no on-grid cue",
    );
}

// --- Ducts (§10.7) ------------------------------------------------------------

/// A hand-built duct fixture: a wall band under the top border with a 4-cell duct
/// threaded through it — entries at `(2,1)` and `(5,1)`, interior `(3,1)`/`(4,1)` —
/// opening (mouths `(2,2)`/`(5,2)`) into an open room below (rows 2..7). The player
/// starts on the near mouth `(2,2)`, facing the entry.
///
/// ```text
///   #########   row 0 (border)
///   #.=..=..#   row 1: wall band, = are entries at x=2,5; interior wall at x=3,4
///   #.......#   rows 2..7: open room; mouths at (2,2) and (5,2)
///   ...
/// ```
fn duct_world() -> State {
    let mut f = Facility::walled_box(9, 9);
    for x in 1..=7 {
        f.set_terrain(x, 1, Terrain::Wall); // the wall band the duct hides in
    }
    f.set_terrain(2, 1, Terrain::DuctEntry);
    f.set_terrain(5, 1, Terrain::DuctEntry);
    let duct = crate::Duct::new(vec![
        Cell::new(2, 1),
        Cell::new(3, 1),
        Cell::new(4, 1),
        Cell::new(5, 1),
    ]);
    let layout = crate::Layout::from_facility(f).with_ducts(vec![duct]);
    State::new(
        layout,
        Cell::new(2, 2),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(7, 7),
    )
}

/// §10.7: bump the mouth to climb in, crawl the path one cell per turn, climb out the
/// far mouth — every step a spent turn (§4.4), concealment on the whole time inside.
#[test]
fn enter_crawl_and_climb_out_of_a_duct() {
    let mut s = duct_world();
    assert!(!s.in_duct());
    let t0 = s.turn();

    // Bump the entry from the mouth: a decision, the turn spent, now concealed inside.
    let e = s.step(Input::Step(Direction::North));
    assert!(e.contains(&Event::EnteredDuct {
        at: Cell::new(2, 1)
    }));
    assert!(s.in_duct());
    assert_eq!(s.player(), Cell::new(2, 1));
    assert_eq!(s.turn(), t0 + 1, "entering spends the turn");

    // Crawl the path east, one cell per spent turn.
    for x in [3, 4, 5] {
        let before = s.turn();
        let e = s.step(Input::Step(Direction::East));
        assert!(e.contains(&Event::DuctCrawled {
            to: Cell::new(x, 1)
        }));
        assert_eq!(s.player(), Cell::new(x, 1));
        assert!(s.in_duct());
        assert_eq!(s.turn(), before + 1, "each crawl spends the turn");
    }

    // At the far entry, step out through its mouth: an ordinary move onto the floor.
    let e = s.step(Input::Step(Direction::South));
    assert!(e.contains(&Event::Moved {
        to: Cell::new(5, 2)
    }));
    assert!(!s.in_duct(), "climbing out the far mouth leaves the duct");
    assert_eq!(s.player(), Cell::new(5, 2));
}

/// §10.7 confinement: inside a duct the only way off the path is the mouth of the
/// entry you stand on. A step into the surrounding wall — or off an interior cell
/// toward the floor it happens to touch — is a solid bump that changes nothing.
#[test]
fn a_duct_confines_the_player_to_its_path() {
    let mut s = duct_world();
    s.step(Input::Step(Direction::North)); // enter at (2,1)
    s.step(Input::Step(Direction::East)); // crawl to interior (3,1)
    assert_eq!(s.player(), Cell::new(3, 1));

    // (3,1) touches floor at (3,2), but it is not an entry — stepping there is walled.
    let e = s.step(Input::Step(Direction::South));
    assert!(e.contains(&Event::Bumped {
        into: Cell::new(3, 2)
    }));
    assert_eq!(s.player(), Cell::new(3, 1), "no exit from a mid-duct cell");
    assert!(s.in_duct());

    // Into the top wall: also a solid bump.
    let e = s.step(Input::Step(Direction::North));
    assert!(e.contains(&Event::Bumped {
        into: Cell::new(3, 0)
    }));
    assert_eq!(s.player(), Cell::new(3, 1));
}

/// §6.1/§10.7 perception: an **entry** cell casts the live mouth peek out into the
/// room, while a **mid-duct** cell has no live vision at all — memory only.
#[test]
fn an_entry_peeks_but_a_mid_duct_cell_is_blind() {
    let mut s = duct_world();
    s.step(Input::Step(Direction::North)); // enter at (2,1)

    // The entry peeks down its mouth into the room below.
    assert!(
        s.player_fov().contains(Cell::new(2, 4)),
        "the entry-cell peek reads down the room"
    );
    assert!(
        !s.player_fov().contains(Cell::new(2, 0)),
        "no live vision back through the top wall"
    );

    // Crawl to an interior cell: FOV collapses to the occupied cell — no live window,
    // not even the floor the cell touches (§10.7's information cost).
    s.step(Input::Step(Direction::East)); // (3,1)
    assert!(s.player_fov().contains(Cell::new(3, 1)));
    assert!(
        !s.player_fov().contains(Cell::new(3, 2)),
        "a mid-duct cell sees no live world"
    );
}

/// §10.7: the guard sense shrinks inside a duct and **Wait does not widen it** —
/// unlike the open floor, where waiting buys the larger box (§9.1).
#[test]
fn the_in_duct_sense_is_reduced_and_wait_does_not_widen_it() {
    let mut s = duct_world();
    s.step(Input::Step(Direction::North)); // enter the duct

    assert!(s.in_duct());
    assert_eq!(s.sense_range(), DUCT_SENSE_RANGE, "reduced inside a duct");
    s.step(Input::Wait);
    assert_eq!(
        s.sense_range(),
        DUCT_SENSE_RANGE,
        "waiting inside a duct does not widen the sense",
    );

    // Climb out onto the floor (crawl to the far entry, step out): the sense is normal
    // again, and there waiting *does* widen it (§9.1).
    for _ in 0..3 {
        s.step(Input::Step(Direction::East));
    }
    s.step(Input::Step(Direction::South)); // out at (5,2)
    assert!(!s.in_duct());
    assert_eq!(s.sense_range(), PLAYER_SENSE_RANGE, "normal on the floor");
    s.step(Input::Wait);
    assert_eq!(
        s.sense_range(),
        PLAYER_SENSE_RANGE_WAITING,
        "waiting on the floor widens the sense as usual",
    );
}

/// §4.5/§10.7: a guard on the mouth can never follow the player into a duct —
/// contact is refused like an occupied cupboard, and the concealed crawler is never
/// detected. The guard is sent to the player's duct cell and holds at the mouth.
#[test]
fn a_guard_cannot_capture_or_enter_a_duct() {
    let mut f = Facility::walled_box(9, 9);
    for x in 1..=7 {
        f.set_terrain(x, 1, Terrain::Wall);
    }
    f.set_terrain(2, 1, Terrain::DuctEntry);
    f.set_terrain(5, 1, Terrain::DuctEntry);
    let duct_cells = [
        Cell::new(2, 1),
        Cell::new(3, 1),
        Cell::new(4, 1),
        Cell::new(5, 1),
    ];
    let layout =
        crate::Layout::from_facility(f).with_ducts(vec![crate::Duct::new(duct_cells.to_vec())]);
    // Player at the near mouth; a guard patrolling up to it, its cone falling on the
    // entry cell every time it stands there. The player climbs into the duct on the
    // first step — "in a duct" is entered state now (§10.7), not a placement.
    let mut s = State::new(
        layout,
        Cell::new(2, 2),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(2, 6), Cell::new(2, 2))],
        Vec::new(),
        Cell::new(7, 7),
    );
    s.step(Input::Step(Direction::North)); // bump the mouth to climb in at (2,1)
    assert!(s.in_duct());

    // Over the patrol the guard reaches the mouth (2,2), adjacent to the entry with
    // its cone on it — yet the concealed crawler is never seen, the guard never steps
    // onto a duct cell (they are wall to it), and there is no capture (§10.7).
    let mut reached_mouth = false;
    for _ in 0..12 {
        let e = s.step(Input::Wait);
        assert!(
            !e.iter().any(|e| matches!(e, Event::Captured { .. })),
            "a player in a duct is never captured",
        );
        assert_eq!(s.outcome(), Outcome::Playing);
        let g = s.guards()[0].pos();
        assert!(
            !duct_cells.contains(&g),
            "a guard never enters a duct cell (it is wall to guards)",
        );
        assert!(
            !s.guards()[0].detected_player(),
            "a concealed crawler is never detected, even from the mouth",
        );
        reached_mouth |= g == Cell::new(2, 2);
    }
    assert!(
        reached_mouth,
        "the guard did come adjacent to the entry, so concealment was tested at contact range",
    );
}

/// §10.7 cross-room routing: a duct interior may now overlie **room floor** the guards
/// walk. A guard crossing that floor must behave as if the duct were not there (nothing
/// guard-facing changes): it walks straight over the concealed crawler's cell — neither
/// blocked by them nor capturing them — and never detects the crawler.
///
/// The fixture: rows 1–2 are a solid wall band, rows 4–6 another, so **row 3 is a
/// one-wide floor corridor** the guard is confined to and must sweep end to end. A
/// duct runs entry `(2,2)` → `(3,2)` → `(3,3)` → `(4,3)` → `(5,3)` → `(5,2)` → entry
/// `(6,2)`, so its middle three cells `(3,3)`/`(4,3)`/`(5,3)` cross the corridor floor.
/// The player crawls to `(3,3)`; the guard patrols row 3 straight through it.
#[test]
fn a_guard_walks_over_a_crawler_on_a_floor_duct_cell() {
    let mut f = Facility::walled_box(9, 9);
    for x in 1..=7 {
        f.set_terrain(x, 1, Terrain::Wall);
        f.set_terrain(x, 2, Terrain::Wall); // the wall band the entries recess in
        f.set_terrain(x, 4, Terrain::Wall); // seal the corridor to one row so the
        f.set_terrain(x, 5, Terrain::Wall); // guard is confined to row 3 and must
        f.set_terrain(x, 6, Terrain::Wall); // sweep across the crawler's cell
    }
    f.set_terrain(2, 2, Terrain::DuctEntry); // mouth (2,3)
    f.set_terrain(6, 2, Terrain::DuctEntry); // mouth (6,3)
    let path = vec![
        Cell::new(2, 2),
        Cell::new(3, 2), // wall backing
        Cell::new(3, 3), // floor — crosses the room
        Cell::new(4, 3), // floor
        Cell::new(5, 3), // floor
        Cell::new(5, 2), // wall backing
        Cell::new(6, 2),
    ];
    let layout = crate::Layout::from_facility(f).with_ducts(vec![crate::Duct::new(path)]);
    // Player at the near mouth; a guard patrolling row 3, straight across the duct's
    // floor cells (its start (5,3) is itself one of them — floor to the guard).
    let mut s = State::new(
        layout,
        Cell::new(2, 3),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(5, 3), Cell::new(1, 3))],
        Vec::new(),
        Cell::new(7, 7),
    );
    s.step(Input::Step(Direction::North)); // climb in at (2,2)
    s.step(Input::Step(Direction::East)); // crawl to backing (3,2)
    s.step(Input::Step(Direction::South)); // crawl onto the floor cell (3,3)
    assert!(s.in_duct());
    assert_eq!(s.player(), Cell::new(3, 3));

    // The guard oscillates along row 3 and passes through the crawler's floor cell.
    let mut guard_stood_on_crawler = false;
    for _ in 0..16 {
        let e = s.step(Input::Wait);
        assert!(
            !e.iter().any(|e| matches!(e, Event::Captured { .. })),
            "a crawler on a floor duct cell is never captured",
        );
        assert_eq!(s.outcome(), Outcome::Playing);
        assert!(s.in_duct(), "the crawler stays in the duct throughout");
        assert_eq!(
            s.player(),
            Cell::new(3, 3),
            "and stays put on the floor cell"
        );
        assert!(
            !s.guards()[0].detected_player(),
            "the concealed crawler is never detected, even underfoot",
        );
        guard_stood_on_crawler |= s.guards()[0].pos() == Cell::new(3, 3);
    }
    assert!(
        guard_stood_on_crawler,
        "the guard walked straight over the crawler's floor cell — not blocked by it",
    );
}

/// §10.7: a body cannot follow the player into the walls, so a **dragging** player is
/// refused entry — the entry reads solid until the body is let go.
#[test]
fn you_cannot_enter_a_duct_while_dragging_a_body() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    layout.place(Cell::new(7, 4), Terrain::DuctEntry);
    layout.place(Cell::new(8, 4), Terrain::Wall);
    let layout = layout.with_ducts(vec![crate::Duct::new(vec![
        Cell::new(7, 4),
        Cell::new(8, 4),
    ])]);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4))],
        Vec::new(),
        Cell::new(8, 8),
    );
    // Take the guard down from concealment, then pick up the body and carry it east.
    s.step(Input::Step(Direction::North)); // takedown at (5,4)
    s.step(Input::Step(Direction::North)); // climb out onto the body at (5,4)
    s.step(Input::Step(Direction::East)); // step off — take hold; player (6,4)
    assert!(s.dragging().is_some());
    assert_eq!(s.player(), Cell::new(6, 4));

    // Bump the duct entry to the east: refused while a body is in hand.
    let e = s.step(Input::Step(Direction::East));
    assert!(!s.in_duct(), "a dragging player cannot climb into a duct");
    assert_eq!(s.player(), Cell::new(6, 4), "the bump changed nothing");
    assert!(e.contains(&Event::Bumped {
        into: Cell::new(7, 4)
    }));
    assert!(s.dragging().is_some(), "still holding the body");
}

/// §11.4: the usable line offers "duct: enter" at a mouth, and nothing once inside
/// (crawling is movement, not an offered interaction).
#[test]
fn the_usable_line_offers_duct_enter_at_a_mouth() {
    let mut s = duct_world();
    assert!(
        s.affordances()
            .iter()
            .any(|&(dir, a)| dir == Direction::North && a == Affordance::EnterDuct),
        "the mouth offers duct: enter",
    );
    s.step(Input::Step(Direction::North)); // climb in
    assert!(
        !s.affordances()
            .iter()
            .any(|(_, a)| *a == Affordance::EnterDuct),
        "inside, there is nothing to enter",
    );
}

// ---------------------------------------------------------------------------
// Auto lateral-shift past an obstacle (#57) — the traversal experiment.
// ---------------------------------------------------------------------------

/// §57/§4.4: a step blocked by a pillar with **exactly one** open lateral slides
/// one cell that way instead of dead-stopping. The slide is a successful move —
/// it spends the turn (§4.4) and sets facing to the shift direction (§5).
#[test]
fn a_blocked_step_with_one_open_side_slides_past() {
    let mut layout = open_room(6, 6);
    layout.place(Cell::new(3, 1), Terrain::Wall); // the pillar dead ahead (east)
                                                  // Player hard against the north wall so its two laterals are: north border wall
                                                  // (blocked) and south floor (open) — exactly one, unambiguous.
    let mut s = State::new(
        layout,
        Cell::new(2, 1),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(4, 4),
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(2, 2)
        }],
        "the bump into the pillar slid south into the one open lateral",
    );
    assert_eq!(s.player(), Cell::new(2, 2), "shifted one cell south");
    assert_eq!(
        s.facing(),
        Direction::South,
        "facing follows the successful step (§5), not the aimed east",
    );
    assert_eq!(
        s.turn(),
        turn_before + 1,
        "the slide is a move — it spends the turn"
    );
}

/// §57/§4.4: a blocked step with **both** laterals open *and* both forward-diagonals
/// open — a lone pillar, clear either way — is genuinely ambiguous, so it stays the
/// free wall-bump it has always been (§4.4). The forward-diagonal tiebreak refuses
/// (both open), nothing moves, no turn is spent.
#[test]
fn a_blocked_step_with_both_sides_open_does_not_slide() {
    let mut layout = open_room(6, 6);
    layout.place(Cell::new(3, 2), Terrain::Wall); // lone pillar dead ahead (east)
    let mut s = State::new(
        layout,
        // Both laterals — north (2,1), south (2,3) — and both forward-diagonals —
        // north-east (3,1), south-east (3,3) — are open floor: nothing to round.
        Cell::new(2, 2),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(4, 4),
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(3, 2)
        }],
        "a lone pillar, open both ways round, is ambiguous — a free bump, no slide",
    );
    assert_eq!(s.player(), Cell::new(2, 2), "nothing moved");
    assert_eq!(s.turn(), turn_before, "an ambiguous bump stays free");
}

/// §57/§4.4: a blocked step with **neither** lateral open (a dead end) stays a
/// free bump — there is nowhere to slide.
#[test]
fn a_blocked_step_with_no_open_side_does_not_slide() {
    let mut layout = open_room(6, 6);
    layout.place(Cell::new(2, 1), Terrain::Wall); // dead ahead (north)
    layout.place(Cell::new(3, 2), Terrain::Wall); // east lateral blocked
    layout.place(Cell::new(1, 2), Terrain::Wall); // west lateral blocked
    let mut s = State::new(
        layout,
        Cell::new(2, 2),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(4, 4),
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(2, 1)
        }],
        "boxed in on both sides — a free bump, no slide",
    );
    assert_eq!(s.player(), Cell::new(2, 2), "nothing moved");
    assert_eq!(s.turn(), turn_before, "a dead-end bump stays free");
}

/// §57: the slide targets **plain floor only** — a lateral that is an interactable
/// (here a hideout) does not qualify, so it is never auto-entered. With a cupboard
/// on one side and floor on the other, the one *plain* lateral wins and the player
/// slides onto the floor, away from the cupboard — proof the cupboard was excluded
/// (had it counted, the two open sides would have been ambiguous and nothing would
/// have moved).
#[test]
fn the_slide_never_auto_enters_an_interactable_lateral() {
    let mut layout = open_room(6, 6);
    layout.place(Cell::new(3, 2), Terrain::Wall); // pillar dead ahead (east)
    layout.place(Cell::new(2, 1), Terrain::Hideout); // north lateral: a cupboard
                                                     // south lateral (2,3) stays plain floor
    let mut s = State::new(
        layout,
        Cell::new(2, 2),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(4, 4),
    );

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(2, 3)
        }],
        "slid onto the plain floor, not into the cupboard",
    );
    assert_eq!(s.player(), Cell::new(2, 3), "on the floor south");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::EnteredHideout { .. })),
        "the cupboard was never climbed into",
    );
}

/// §57/§4.5 — detection is deliberately *not* guarded: a slide may land in a
/// guard's cone, because being seen is not losing (§4.5) and the player chose to
/// press into the obstacle. The guard's straight-south cone covers the one open
/// lateral, yet the slide proceeds — dodging cones is a separate follow-up ticket.
#[test]
fn the_slide_proceeds_into_a_guards_cone_being_seen_is_not_losing() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(4, 3), Terrain::Wall); // pillar dead ahead (north)
    layout.place(Cell::new(3, 4), Terrain::Wall); // west lateral blocked
                                                  // east lateral (5,4) is plain floor — a guard two cells north watches it.
    let dest = Cell::new(5, 4);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 2))], // faces south, cone down the column
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(
        s.guards()[0].fov().contains(dest),
        "precondition: the guard's cone covers the destination",
    );
    assert!(
        s.guards()[0].pos().manhattan_distance(dest) > 1,
        "precondition: the guard is not adjacent — only the cone is at stake",
    );
    let turn_before = s.turn();

    s.step(Input::Step(Direction::North));
    assert_eq!(
        s.player(),
        dest,
        "the slide proceeds into the cone — detection is not the slide's concern",
    );
    assert_eq!(s.turn(), turn_before + 1, "the slide spends the turn");
}

/// §57/§2.2/§4.5 — the fairness guard, capture clause: a slide is refused when a
/// guard stands orthogonally adjacent to the destination, even when its cone does
/// *not* cover it (here the destination is in the guard's rear blind spot, §155) —
/// nothing may auto-step the player into a cell a guard can capture next phase.
#[test]
fn the_slide_is_refused_next_to_a_guard_that_could_capture() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(4, 3), Terrain::Wall); // pillar dead ahead (north)
    layout.place(Cell::new(3, 4), Terrain::Wall); // west lateral blocked
                                                  // east lateral (5,4) is plain floor — a guard sits directly south of it.
    let dest = Cell::new(5, 4);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 5))], // faces south; dest is behind it
        Vec::new(),
        Cell::new(8, 8),
    );
    assert!(
        !s.guards()[0].fov().contains(dest),
        "precondition: the destination is in the guard's rear blind spot, not its cone",
    );
    assert_eq!(
        s.guards()[0].pos().manhattan_distance(dest),
        1,
        "precondition: the guard is orthogonally adjacent to the destination",
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(4, 3)
        }],
        "a guard could step into the destination — the slide is refused",
    );
    assert_eq!(
        s.player(),
        Cell::new(4, 4),
        "the player did not slide next to the guard"
    );
    assert_eq!(s.turn(), turn_before, "a refused slide stays free");
}

/// §57: with **both** laterals open, the forward-diagonal breaks the tie — the
/// slide rounds the obstacle toward the side where the path continues. A pillar
/// dead ahead that also blocks the north-east forward-diagonal, with the
/// south-east one open, slides *south* (not north): the obstacle extends left, so
/// you round it right. Still one orthogonal step — the diagonal is only read.
#[test]
fn both_sides_open_slides_round_the_obstacle_by_the_open_diagonal() {
    let mut layout = open_room(8, 8);
    layout.place(Cell::new(4, 3), Terrain::Wall); // pillar dead ahead (east)
    layout.place(Cell::new(4, 2), Terrain::Wall); // north-east forward-diagonal blocked
                                                  // south-east forward-diagonal (4,4) stays open floor; both laterals (3,2)/(3,4) open
    let mut s = State::new(
        layout,
        Cell::new(3, 3),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(6, 6),
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(3, 4)
        }],
        "the obstacle blocks the NE diagonal — round it south toward the open SE",
    );
    assert_eq!(s.player(), Cell::new(3, 4), "slid one cell south");
    assert_eq!(
        s.facing(),
        Direction::South,
        "facing follows the slide (§5)"
    );
    assert_eq!(s.turn(), turn_before + 1, "the slide spends the turn");
}

/// §57: the reported playtest case (#57) — crouched against the long edge of a
/// two-cell table, pressing *into* it (a held-crouch dead bump) with both laterals
/// open. The table's second cell blocks the near forward-diagonal while the far one
/// is open, so the slide rounds the table toward the open corridor rather than
/// dead-stopping on "blocked".
///
/// ```text
///   . . π      the table's far cell blocks the NE forward-diagonal
///   . @ π   →  press east into the held table
///   . . .      the SE forward-diagonal is open — round south
/// ```
#[test]
fn crouched_against_a_two_cell_table_slides_round_it() {
    let mut layout = open_room(8, 8);
    layout.place(Cell::new(4, 2), Terrain::PartialCover); // the two-cell table…
    layout.place(Cell::new(4, 3), Terrain::PartialCover); // …the player crouches behind
    let mut s = State::new(
        layout,
        Cell::new(3, 3),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(6, 6),
    );

    // First east bump crouches behind the table run {(4,2),(4,3)} (§10.3).
    let crouch = s.step(Input::Step(Direction::East));
    assert!(
        crouch.contains(&Event::Crouched {
            behind: Cell::new(4, 3)
        }),
        "the first bump crouches behind the table",
    );
    assert_eq!(
        s.player(),
        Cell::new(3, 3),
        "crouching does not move the player"
    );
    let turn_after_crouch = s.turn();

    // Second east bump is a held-crouch dead bump: both laterals (3,2)/(3,4) are
    // open, the table's second cell (4,2) blocks the NE forward-diagonal, and the
    // SE (4,4) is open — so it rounds the table south.
    let slide = s.step(Input::Step(Direction::East));
    assert_eq!(
        slide,
        vec![Event::Moved {
            to: Cell::new(3, 4)
        }],
        "the held-crouch bump rounds the two-cell table south",
    );
    assert_eq!(
        s.player(),
        Cell::new(3, 4),
        "slid one cell south, round the table"
    );
    assert_eq!(
        s.facing(),
        Direction::South,
        "facing follows the slide (§5)"
    );
    assert_eq!(s.turn(), turn_after_crouch + 1, "the slide spends the turn");
}

/// §57/§4.4: with both laterals open but **both** forward-diagonals blocked (the
/// obstacle walls off the path both ways round), the tiebreak has no answer — it
/// stays a free bump, nothing moves.
#[test]
fn both_sides_open_but_both_diagonals_blocked_does_not_slide() {
    let mut layout = open_room(8, 8);
    layout.place(Cell::new(4, 3), Terrain::Wall); // dead ahead (east)
    layout.place(Cell::new(4, 2), Terrain::Wall); // NE forward-diagonal blocked
    layout.place(Cell::new(4, 4), Terrain::Wall); // SE forward-diagonal blocked
    let mut s = State::new(
        layout,
        Cell::new(3, 3), // laterals (3,2)/(3,4) open, but neither leads onward
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(6, 6),
    );
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(4, 3)
        }],
        "blocked both ways round — no unambiguous side, a free bump",
    );
    assert_eq!(s.player(), Cell::new(3, 3), "nothing moved");
    assert_eq!(s.turn(), turn_before, "a refused slide stays free");
}

/// §57: the kill-switch. With the auto-slide turned off, the one-open-side case
/// that would slide (`a_blocked_step_with_one_open_side_slides_past`) reverts to
/// the plain §4.4 free bump — nothing moves, no turn is spent.
#[test]
fn the_auto_slide_kill_switch_restores_the_free_bump() {
    let mut layout = open_room(6, 6);
    layout.place(Cell::new(3, 1), Terrain::Wall); // the pillar dead ahead (east)
    let mut s = State::new(
        layout,
        Cell::new(2, 1), // north border wall blocked, south floor open — would slide
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(4, 4),
    );
    s.set_auto_slide(false);
    let turn_before = s.turn();

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(3, 1)
        }],
        "with the slide off, the dead bump is the free §4.4 no-op",
    );
    assert_eq!(s.player(), Cell::new(2, 1), "nothing moved");
    assert_eq!(s.turn(), turn_before, "a free bump does not spend the turn");
}

// --- Autodoors (§8.3/§7.6/§10.4, #241) ----------------------------------------

/// A hand-built strip: a left room (cols 1–5) and a right room (cols 7…) joined by
/// one **manual** door at column 6 — hinges at `(6,1)`/`(6,3)`, a single panel at
/// `(6,2)` — with the player starting `player`, **facing east**, ahead of the door
/// and holding the full loadout (Autodoors included). No guards, so the flight
/// mechanics read clean. Returns the state and the door's panel cell.
fn autodoor_strip(width: u32, player: Cell) -> (State, Cell) {
    let mut f = Facility::walled_box(width, 6);
    let mut g = RegionGraph::new(width, 6);
    let column =
        |x0: u32, x1: u32| (1..5).flat_map(move |y| (x0..x1).map(move |x| Cell::new(x, y)));
    let left = g.add_region(RegionKind::Room, column(1, 6));
    let right = g.add_region(RegionKind::Room, column(7, width - 1));
    for y in 1..5 {
        f.set_terrain(6, y, Terrain::Wall);
    }
    f.set_terrain(6, 1, Terrain::DoorHinge);
    f.set_terrain(6, 2, Terrain::DoorPanelClosed);
    f.set_terrain(6, 3, Terrain::DoorHinge);
    g.add_door(
        left,
        right,
        [Cell::new(6, 1), Cell::new(6, 3)],
        [Cell::new(6, 2)],
        DoorKind::Manual,
    );
    let s = State::new(
        Layout::from_parts(f, g),
        player,
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(width - 2, 4),
    );
    (s, Cell::new(6, 2))
}

/// The core edge (§8.3/§7.6, AC #241): while Autodoors is active a closed door in
/// the path **opens as the player steps into it** — no manual bump — and the same
/// step carries them onto the panel in one turn, then the door **shuts behind** them
/// the turn they clear the throat. Both changes are the player's own
/// (`by_player: true`), so they self-narrate and light no sensed cue (§9.4/§11.7).
#[test]
fn autodoors_opens_ahead_and_shuts_behind() {
    let (mut s, panel) = autodoor_strip(12, Cell::new(5, 2));
    let door = s
        .layout()
        .regions()
        .door_at(panel)
        .expect("the strip's door");

    // Turn 1: switch the ability on (a spent turn; the player does not move).
    s.step(Input::Activate(AbilityId::Autodoors));
    assert!(matches!(
        s.ability_state(AbilityId::Autodoors),
        AbilityState::Active { .. }
    ));
    assert!(!s.layout().regions().door(door).is_open(), "still closed");

    // Turn 2: step east into the closed panel. It opens *and* the player walks
    // through in the one turn — the open-turn the manual bump would cost is saved.
    let e = s.step(Input::Step(Direction::East));
    assert_eq!(
        e,
        vec![
            Event::DoorOpened {
                at: panel,
                by_player: true,
            },
            Event::Moved { to: panel },
        ],
        "one step both opens the door and moves onto the panel",
    );
    assert_eq!(s.player(), panel, "the player stands in the throat");
    assert!(s.layout().regions().door(door).is_open(), "opened ahead");
    assert!(
        s.autodoors_pending.contains(&door),
        "the door is armed to shut behind",
    );

    // Turn 3: step clear of the throat. Once vacated, the door swings shut behind —
    // the crush-safe §10.4 close, reported as the player's own change.
    let e = s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(7, 2), "through to the far side");
    assert_eq!(
        e,
        vec![
            Event::Moved {
                to: Cell::new(7, 2)
            },
            Event::DoorClosed {
                at: panel,
                by_player: true,
            },
        ],
        "clearing the throat shuts the door behind",
    );
    assert!(!s.layout().regions().door(door).is_open(), "shut behind");
    assert_eq!(
        s.layout().facility().terrain_at(panel.x, panel.y),
        Some(Terrain::DoorPanelClosed),
        "the panel restamps solid, exactly as any close leaves it",
    );
    assert!(
        s.autodoors_pending.is_empty(),
        "a shut door is no longer owed a close",
    );
}

/// AC #241, the §7.6 payoff: a door shut behind the fleeing player **breaks a
/// pursuer's line of sight** (§10.3). Verified from a west vantage looking east:
/// through the open doorway the far cell is visible; once the door shuts behind the
/// player, the closed panel occludes it.
#[test]
fn autodoors_breaks_a_pursuers_sightline() {
    let (mut s, panel) = autodoor_strip(12, Cell::new(5, 2));
    let vantage = Cell::new(2, 2); // a pursuer back in the left room
    let far = Cell::new(7, 2); // where the player flees to, beyond the door

    s.step(Input::Activate(AbilityId::Autodoors));
    s.step(Input::Step(Direction::East)); // onto the panel; the door is open

    let through_open = field_of_view(
        s.layout().facility(),
        vantage,
        Direction::East,
        crate::vision::WAIT_SIGHT_ARC,
        20,
    );
    assert!(
        through_open.contains(far),
        "the open doorway lets the line reach the far cell",
    );

    s.step(Input::Step(Direction::East)); // clear the throat → the door shuts

    let through_shut = field_of_view(
        s.layout().facility(),
        vantage,
        Direction::East,
        crate::vision::WAIT_SIGHT_ARC,
        20,
    );
    assert!(
        !through_shut.contains(far),
        "the shut panel breaks the pursuer's line (§10.3)",
    );
    // The vantage still sees its own side of the now-closed door — the break is at
    // the panel, not a blanked view.
    assert!(through_shut.contains(panel.step(Direction::West).unwrap()));
}

/// AC #241 (never crushing, §10.4): an armed door **waits for a dragged body** to
/// clear the throat before it shuts. A body on the panel is an occupant to the
/// crush rule (§7.2), so the auto-close refuses until it is gone — the door does not
/// clip the body the player is hauling through.
#[test]
fn an_armed_autodoor_never_shuts_on_a_body_in_the_throat() {
    let (mut s, panel) = autodoor_strip(12, Cell::new(5, 2));
    let door = s
        .layout()
        .regions()
        .door_at(panel)
        .expect("the strip's door");

    s.step(Input::Activate(AbilityId::Autodoors));
    s.step(Input::Step(Direction::East)); // onto the panel; the door is open + armed
    assert!(s.layout().regions().door(door).is_open());

    // The player is hauling a body: place it under them in the throat and take hold,
    // so the next step hauls it into the panel behind them as they clear it. Set up
    // directly to isolate the crush interaction from the full takedown-and-grab dance
    // (`haul_body_to` threads the haul itself).
    s.bodies.push(crate::body::Body::new(
        panel,
        panel,
        crate::radio::RadioClock::from_period(4),
        s.turn(),
    ));
    s.dragging = Some(0);
    s.drag_debt = false;

    let e = s.step(Input::Step(Direction::East)); // player clears; the body follows in
    assert_eq!(s.player(), Cell::new(7, 2));
    assert_eq!(
        s.bodies[0].cell(),
        panel,
        "the hauled body is now in the throat"
    );
    assert!(
        !e.iter().any(|ev| matches!(ev, Event::DoorClosed { .. })),
        "the door does not shut on the body",
    );
    assert!(
        s.layout().regions().door(door).is_open(),
        "the body in the throat holds the door open (§10.4 never crushes)",
    );
    assert!(s.autodoors_pending.contains(&door), "still owed a close");

    // The body hauled clear of the throat, the next world turn shuts the door.
    s.bodies[0].move_to(Cell::new(3, 2));
    let e = s.step(Input::Wait);
    assert!(
        e.contains(&Event::DoorClosed {
            at: panel,
            by_player: true,
        }),
        "the throat clear, the armed door finally shuts",
    );
    assert!(!s.layout().regions().door(door).is_open());
}

/// The gate (§8.3): with Autodoors **inactive**, a closed door is the ordinary
/// bump-to-open (§10.4/#148) — a spent turn that opens the door but does *not* move
/// the player through, and arms no close-behind. The edge is the ability's alone.
#[test]
fn without_the_ability_a_closed_door_is_a_plain_bump_open() {
    let (mut s, panel) = autodoor_strip(12, Cell::new(5, 2));
    let door = s
        .layout()
        .regions()
        .door_at(panel)
        .expect("the strip's door");

    let e = s.step(Input::Step(Direction::East));
    assert_eq!(
        e,
        vec![Event::DoorOpened {
            at: panel,
            by_player: true,
        }],
        "a plain bump opens the door and nothing more",
    );
    assert_eq!(
        s.player(),
        Cell::new(5, 2),
        "the bump does not move the player"
    );
    assert!(s.layout().regions().door(door).is_open());
    assert!(
        s.autodoors_pending.is_empty(),
        "no close is armed without the ability",
    );

    // And with the door left open, it never auto-shuts — a Wait passes and it stays.
    s.step(Input::Wait);
    assert!(
        s.layout().regions().door(door).is_open(),
        "a manually opened door does not close itself",
    );
}

/// Autodoors offers only a **panel** (§8.3): a closed **hinge** opens the door too
/// (#148), but the hinge cell stays solid, so there is nothing to step onto — the
/// bump is the ordinary door op (open in place), not a walk-through, and arms no
/// close.
#[test]
fn autodoors_leaves_a_closed_hinge_a_plain_door_op() {
    // Player west of the lower hinge (6,3), facing east into it (the fixture facing).
    let (mut s, _) = autodoor_strip(12, Cell::new(5, 3));
    let hinge = Cell::new(6, 3);
    let door = s
        .layout()
        .regions()
        .door_at(hinge)
        .expect("the strip's door");

    s.step(Input::Activate(AbilityId::Autodoors));
    let e = s.step(Input::Step(Direction::East));
    assert!(
        e.iter().any(|ev| matches!(
            ev,
            Event::DoorOpened {
                at,
                by_player: true,
            } if *at == hinge
        )),
        "the hinge opens the door",
    );
    assert_eq!(
        s.player(),
        Cell::new(5, 3),
        "a hinge is solid — the player does not walk onto it",
    );
    assert!(s.layout().regions().door(door).is_open());
    assert!(
        s.autodoors_pending.is_empty(),
        "the hinge path arms no auto-close — only the walk-through panel does",
    );
}

/// Once armed, a door owes its close-behind regardless of the ability's live state:
/// toggling Autodoors **off** is free (§4.4) and does not cancel the pending close —
/// a door opened while active still shuts as the player steps clear.
#[test]
fn an_armed_autodoor_shuts_even_after_the_ability_is_toggled_off() {
    let (mut s, panel) = autodoor_strip(12, Cell::new(5, 2));
    let door = s
        .layout()
        .regions()
        .door_at(panel)
        .expect("the strip's door");

    s.step(Input::Activate(AbilityId::Autodoors));
    s.step(Input::Step(Direction::East)); // onto the panel; armed
    let turn_before = s.turn();

    // Toggle off — a free action (§4.4): the turn does not advance.
    let e = s.step(Input::Deactivate(AbilityId::Autodoors));
    assert_eq!(
        e,
        vec![Event::AbilityDeactivated {
            ability: AbilityId::Autodoors,
        }],
    );
    assert_eq!(s.turn(), turn_before, "toggling off is free");
    assert!(
        s.autodoors_pending.contains(&door),
        "the close is still owed"
    );

    // Stepping clear still shuts the door the ability opened.
    let e = s.step(Input::Step(Direction::East));
    assert!(
        e.contains(&Event::DoorClosed {
            at: panel,
            by_player: true,
        }),
        "the armed door shuts behind even with the ability off",
    );
    assert!(!s.layout().regions().door(door).is_open());
}

/// Determinism (§12.4, AC #241): the whole open-ahead/shut-behind flow draws no
/// randomness, so the same fixture and inputs reproduce identical events and frame.
#[test]
fn the_autodoors_flow_is_deterministic() {
    let script = [
        Input::Activate(AbilityId::Autodoors),
        Input::Step(Direction::East),
        Input::Step(Direction::East),
        Input::Step(Direction::East),
    ];
    let run = || {
        let (mut s, _) = autodoor_strip(12, Cell::new(5, 2));
        let events: Vec<Vec<Event>> = script.iter().map(|&i| s.step(i)).collect();
        (events, crate::render(&s))
    };
    let (events_a, frame_a) = run();
    let (events_b, frame_b) = run();
    assert_eq!(events_a, events_b, "same inputs → same events");
    assert_eq!(frame_a, frame_b, "same inputs → same frame");
}

/// The flight edge on **automatic** doors (§7.6, #241): a door opened via Autodoors
/// is shut *promptly* behind the player — the turn they clear the throat — rather
/// than lingering open for its full `delay` (§10.4/#147). Here the delay is a long
/// 5, so without the ability the door would idle open for several turns; the ability
/// makes the break immediate, exactly as it does for a manual door.
#[test]
fn autodoors_shuts_an_automatic_door_promptly_behind() {
    let (mut s, door) = auto_door_state(5); // a deliberately slow self-close timer
    let panel = Cell::new(3, 2);

    s.step(Input::Activate(AbilityId::Autodoors));

    // Step through: the automatic door opens ahead and the player walks onto it in
    // the one turn, exactly as a manual door does.
    s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), panel, "walked into the doorway in one turn");
    assert!(s.layout().regions().door(door).is_open());
    assert!(
        s.autodoors_pending.contains(&door),
        "the automatic door is armed too"
    );

    // Clear the throat: the ability shuts it at once — its delay-5 timer has barely
    // begun, so this is far sooner than #147 alone would.
    let e = s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(4, 2), "through to the far room");
    assert!(
        e.iter().any(|ev| matches!(
            ev,
            Event::DoorClosed {
                by_player: true,
                ..
            }
        )),
        "the automatic door shuts behind at once, not on its slow timer",
    );
    assert!(
        !s.layout().regions().door(door).is_open(),
        "shut promptly, not left to idle open",
    );
}

// ---------------------------------------------------------------------------
// Confusion (§8.3/§9/#240): blind and freeze guards in a radius, through walls.
// ---------------------------------------------------------------------------

/// The core of #240: a guard inside the bubble is **frozen in its tracks** while
/// Confusion is active — a chaser bearing down on the player stops advancing the
/// moment it is suppressed, and takes no step for the whole window. When the window
/// ends it **resumes cleanly**, stepping toward the player again on the very next
/// turn. (The level-start world phase, §4.2, already sent this patroller one step
/// into a chase, so it enters the window at (10, 8).)
#[test]
fn confusion_freezes_a_hunting_guard_then_it_resumes() {
    let mut s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::patrolling(Cell::new(10, 7))],
        Vec::new(),
        Cell::new(18, 18),
    );
    // The startup chase has closed the gap by one: the guard is two cells north and
    // already Chasing — a live threat, well inside the bubble.
    assert_eq!(s.guards()[0].pos(), Cell::new(10, 8));
    assert_eq!(s.guards()[0].state(), GuardState::Chasing);

    // Activation turn: frozen this very turn (§8.2 covers the activation turn). Then
    // two more Waits, all inside the 3-turn window — the guard never advances.
    let events = s.step(Input::Activate(AbilityId::Confusion));
    assert!(
        events.contains(&Event::AbilityActivated {
            ability: AbilityId::Confusion
        }),
        "the ability switched on: {events:?}",
    );
    for turn in 1..=3 {
        assert_eq!(s.outcome(), Outcome::Playing, "turn {turn}: still playing");
        assert_eq!(
            s.guards()[0].pos(),
            Cell::new(10, 8),
            "turn {turn}: a frozen chaser does not advance",
        );
        if turn < 3 {
            s.step(Input::Wait);
        }
    }

    // The window has ticked out. The guard resumes at once — stepping toward the
    // player it was held off, its chase intact (no lost state, §8.2).
    s.step(Input::Wait);
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(10, 9),
        "the guard resumes its advance the moment the window ends",
    );
    assert_eq!(
        s.guards()[0].state(),
        GuardState::Chasing,
        "with its chase preserved, not reset",
    );
}

/// The §4.5 capture edge (#240): a **frozen adjacent** guard cannot step into the
/// player, so it cannot capture while suppressed — but Confusion is a stay of
/// execution, not a reprieve. The moment it lapses, capture-is-contact resumes and
/// the adjacent guard takes the player.
#[test]
fn a_frozen_adjacent_guard_cannot_capture_until_confusion_lapses() {
    // The startup chase (§4.2) walks this patroller from (10, 8) to (10, 9): adjacent
    // and Chasing. Without Confusion its next step is into the player — a capture.
    let mut s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::patrolling(Cell::new(10, 8))],
        Vec::new(),
        Cell::new(18, 18),
    );
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(10, 9),
        "adjacent after startup"
    );

    s.step(Input::Activate(AbilityId::Confusion));
    for turn in 1..=3 {
        assert_eq!(
            s.outcome(),
            Outcome::Playing,
            "turn {turn}: the frozen adjacent guard cannot capture",
        );
        assert_eq!(
            s.guards()[0].pos(),
            Cell::new(10, 9),
            "turn {turn}: it holds its cell, one step from the player",
        );
        if turn < 3 {
            s.step(Input::Wait);
        }
    }

    // Confusion lapses: the adjacent guard is contact, and contact is capture (§4.5).
    let events = s.step(Input::Wait);
    assert_eq!(s.outcome(), Outcome::Lost, "the reprieve is over");
    assert!(
        events.contains(&Event::Captured {
            by: Cell::new(10, 10)
        }),
        "capture-is-contact resumes: {events:?}",
    );
}

/// The bubble is a **box through walls** (§9), exactly like the guard sense, and it
/// stops at [`CONFUSION_RADIUS`]: a guard one cell past the edge is untouched, and a
/// wall between the player and a guard inside the edge does not spare it. Read off
/// the one [`guard_confused`](State::guard_confused) query both the phase and the
/// renderer use.
#[test]
fn confusion_reaches_through_walls_and_stops_at_its_radius() {
    // Pin the [START] radius so a later change is a visible edit.
    assert_eq!(CONFUSION_RADIUS, 4);

    let mut layout = open_room(24, 12);
    // A wall between the player (6,6) and the near guard (10,6), to prove the bubble
    // ignores line of sight.
    layout.place(Cell::new(8, 6), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(6, 6),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(10, 6)), // distance 4 == radius, behind a wall
            Guard::stationary(Cell::new(6, 11)), // distance 5 > radius
        ],
        Vec::new(),
        Cell::new(22, 10),
    );

    // Before activation nothing is confused.
    assert!(!s.guard_confused(&s.guards()[0]));
    assert!(!s.guard_confused(&s.guards()[1]));

    s.step(Input::Activate(AbilityId::Confusion));
    assert!(
        s.guard_confused(&s.guards()[0]),
        "a guard at the radius edge is frozen even through a wall (§9)",
    );
    assert!(
        !s.guard_confused(&s.guards()[1]),
        "a guard one cell past the edge is untouched — the bubble is conservative",
    );
}

/// The "cone off (§11.5)" half of the acceptance: a confused guard the player can
/// see stops painting the danger overlay. Its cone is dropped from
/// [`visible_cone_cells`](State::visible_cone_cells) the moment it is frozen, so the
/// overlay reads honestly — a blinded guard detects nothing, and nothing red follows.
#[test]
fn a_confused_guards_cone_leaves_the_danger_overlay() {
    // Guard three cells north, in the player's forward view, so it is *seen* and its
    // cone paints the overlay.
    let mut s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::stationary(Cell::new(10, 7))],
        Vec::new(),
        Cell::new(18, 18),
    );
    // A spent turn establishes sight: the seen guard's cone now paints red.
    s.step(Input::Wait);
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Seen),
        "precondition: the guard is seen, so its cone is overlay-eligible",
    );
    assert!(
        s.visible_cone_cells().next().is_some(),
        "precondition: a seen guard paints a danger cone",
    );

    s.step(Input::Activate(AbilityId::Confusion));
    assert!(
        s.guard_confused(&s.guards()[0]),
        "the guard is inside the bubble",
    );
    assert_eq!(
        s.visible_cone_cells().count(),
        0,
        "a confused guard's cone is off — no danger overlay from a blinded guard",
    );
}
