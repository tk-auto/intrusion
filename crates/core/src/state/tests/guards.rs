//! Guards through the turn loop (§7).
//!
//! Phase 3 as the loop runs it: detection and its one-shot [`Event::Detected`]
//! transition (§7.6), capture on contact (§4.5), the takedown and the body it leaves
//! (§7.2), the radio net that misses a downed guard's pings (§7.3), the Calm patrol
//! and its beat (§7.5), the cupboard wait-out and the §15 Q5 flush (§10.3), and
//! Confusion suppressing the whole phase for the guards it catches (§8.3).

use crate::guard::{GuardState, PATROL_RADIUS, SEARCH_RADIUS};
use crate::state::*;
use crate::test_support::{open_room, region_strip};
use crate::{generate_level, LevelModifiers, Rng};

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
            at: Cell::new(1, 2)
        }),
        "the first missed ping is a silence where the guard fell",
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

/// §7.3: control's last fix on a guard is **where it fell**, not the post it was
/// assigned to. A guard taken down far from its station is reported — and searched
/// for — at the takedown site; the station never enters into it.
#[test]
fn a_dispatch_heads_for_the_takedown_site_not_the_station() {
    let fell = Cell::new(1, 2);
    let station = Cell::new(1, 25); // its post, right across the level
    let mut layout = open_room(3, 30);
    layout.place(Cell::new(1, 1), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(1, 1),
        Direction::South,
        vec![
            Guard::stationary(fell)
                .with_station(station)
                .with_radio_clock(radio::RadioClock::from_period(3)),
            Guard::patrolling(Cell::new(1, 20)),
        ],
        Vec::new(),
        Cell::new(1, 28),
    );

    s.step(Input::Step(Direction::South)); // take it down at `fell`
    assert_eq!(
        s.bodies()[0].fell_at(),
        fell,
        "the body records where it fell"
    );

    s.step(Input::Wait); // the window
    let dispatch = s.step(Input::Wait); // the first missed ping
    assert!(
        dispatch.contains(&Event::RadioSilence { at: fell }),
        "the silence names the takedown site, not the station {station:?}",
    );
    assert_eq!(s.guards()[0].state(), GuardState::Responding);
    assert_eq!(
        s.guards()[0].destination(),
        Some(fell),
        "the responder walks to where the guard fell",
    );
}

/// §7.3/§8.3: hauling a body away is what makes the dispatch *confused*. The
/// responder is sent to the cell the guard fell in — which the body has since left
/// — so dragging buys the investigation looking in the wrong place, exactly the
/// §7.3 payoff. (It is not cancellation: the ping is still missed, cf.
/// `a_hidden_body_still_misses_its_ping`.)
#[test]
fn dragging_a_body_sends_the_responder_to_the_empty_takedown_site() {
    let fell = Cell::new(5, 4);
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 5), Terrain::Hideout); // the player's start cupboard
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![
            Guard::stationary(fell).with_radio_clock(radio::RadioClock::from_period(8)),
            Guard::patrolling(Cell::new(9, 9)),
        ],
        Vec::new(),
        Cell::new(10, 10),
    );

    s.step(Input::Step(Direction::North)); // takedown at (5,4)
    s.step(Input::Step(Direction::North)); // climb out onto the body
    s.step(Input::Step(Direction::North)); // step off to (5,3) — take hold
    s.step(Input::Step(Direction::North)); // the hold costs its turn (§8.3)
    s.step(Input::Step(Direction::North)); // haul: player to (5,2), body to (5,3)

    let body = s.bodies()[0].cell();
    assert_ne!(
        body, fell,
        "the body has been dragged off the takedown site"
    );
    assert_eq!(
        s.bodies()[0].fell_at(),
        fell,
        "control's fix stayed where the guard went down",
    );

    let mut dispatched = None;
    for _ in 0..20 {
        for e in s.step(Input::Wait) {
            if let Event::RadioSilence { at } = e {
                dispatched = Some(at);
            }
        }
        if dispatched.is_some() {
            break;
        }
    }
    assert_eq!(
        dispatched,
        Some(fell),
        "the responder is sent to the empty takedown site, not to the body",
    );
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Confusion));
    // The startup chase has closed the gap by one: the guard is two cells north and
    // already Chasing — a live threat, well inside the bubble.
    assert_eq!(s.guards()[0].pos(), Cell::new(10, 8));
    assert_eq!(s.guards()[0].state(), GuardState::Chasing);

    // Activation turn: frozen this very turn (§8.2 covers the activation turn). Then
    // five more Waits, all inside the 6-turn window — the guard never advances.
    let events = s.step(Input::Activate(AbilityId::Confusion));
    assert!(
        events.contains(&Event::AbilityActivated {
            ability: AbilityId::Confusion
        }),
        "the ability switched on: {events:?}",
    );
    for turn in 1..=6 {
        assert_eq!(s.outcome(), Outcome::Playing, "turn {turn}: still playing");
        assert_eq!(
            s.guards()[0].pos(),
            Cell::new(10, 8),
            "turn {turn}: a frozen chaser does not advance",
        );
        if turn < 6 {
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Confusion));
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(10, 9),
        "adjacent after startup"
    );

    s.step(Input::Activate(AbilityId::Confusion));
    for turn in 1..=6 {
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
        if turn < 6 {
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
    assert_eq!(CONFUSION_RADIUS, 6);

    let mut layout = open_room(24, 20);
    // A wall between the player (6,6) and the near guard (12,6), to prove the bubble
    // ignores line of sight.
    layout.place(Cell::new(9, 6), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(6, 6),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(12, 6)), // distance 6 == radius, behind a wall
            Guard::stationary(Cell::new(6, 13)), // distance 7 > radius
        ],
        Vec::new(),
        Cell::new(22, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Confusion));

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
        "a guard one cell past the edge is untouched — the bubble still has an edge",
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Confusion));
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

/// **One reading governs every pass** (§4.2/§8.3, #275). Phase 3 resolves whether
/// each guard is confused *once*, before any guard is touched, and all five passes
/// read that same snapshot. Here a single suppressed guard is denied three of them in
/// the same turn — it does not find the body dropped in its cone (§7.2), does not
/// check the cupboard beside it (§15 Q5), and does not move (§7.5) — and then wins
/// all three back together the turn the window lapses.
///
/// This is the invariant that used to be argued for in a comment: the movement pass
/// re-asked [`State::guard_confused`] live rather than reading the snapshot the other
/// four shared. Two readings of one fact that merely happen to agree are the shape of
/// #199/#200, so the agreement is pinned here instead of asserted in prose.
#[test]
fn one_confusion_reading_governs_every_pass_of_the_phase() {
    // The §7.2 found-body scenario, run inside a Confusion bubble. A one-wide corridor
    // along x=5 keeps the finder's cone straight down the column.
    let mut layout = open_room(11, 11);
    for y in 1..10 {
        layout.place(Cell::new(4, y), Terrain::Wall);
        layout.place(Cell::new(6, y), Terrain::Wall);
    }
    layout.place(Cell::new(5, 7), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(5, 7), // hidden in the cupboard, striking north
        Direction::North,
        vec![
            Guard::stationary(Cell::new(5, 6)), // the victim
            Guard::patrolling_to(Cell::new(5, 1), Cell::new(5, 5)), // the finder
        ],
        Vec::new(),
        Cell::new(5, 9),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Confusion));
    // The startup turn (§4.2) walks the finder one step down the corridor, to well
    // inside the bubble — this whole test hangs on it being suppressed.
    let finder_start = s.guards()[1].pos();
    assert!(
        Cell::new(5, 7).sight_distance(finder_start) <= CONFUSION_RADIUS,
        "the finder starts inside the bubble at {finder_start:?}",
    );

    s.step(Input::Activate(AbilityId::Confusion));
    let frozen_at = s.guards()[1].pos();
    let body = Cell::new(5, 6);
    let events = s.step(Input::Step(Direction::North));
    assert_eq!(
        events,
        vec![Event::TakenDown { at: body }],
        "pass 2: a blind guard does not find the body dropped in its cone",
    );

    // The victim is gone, so the finder is now the only guard left.
    assert_eq!(s.guards().len(), 1);

    // Three passes denied, all from the one reading, for the whole window.
    for turn in 0..3 {
        assert!(!s.bodies()[0].found(), "turn {turn}: pass 2 still skipped");
        assert_eq!(
            s.guards()[0].witnessed_hideout(),
            None,
            "turn {turn}: pass 3 skipped — no cupboard check from a frozen guard",
        );
        assert_eq!(
            s.guards()[0].pos(),
            frozen_at,
            "turn {turn}: pass 5 skipped — the same reading freezes the step",
        );
        s.step(Input::Wait);
    }

    // The window lapses (duration 6) and every pass resumes together — which is what
    // makes the skips above a suppression rather than a cancellation (§8.2).
    let mut found = false;
    for _ in 0..3 {
        if s.step(Input::Wait)
            .iter()
            .any(|e| matches!(e, Event::BodyFound { at } if *at == body))
        {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "pass 2 resumes: the body is found once the guard can see"
    );
    assert_eq!(
        s.guards()[0].witnessed_hideout(),
        Some(Cell::new(5, 7)),
        "pass 3 resumes: the cupboard beside the found body is checked (§15 Q5)",
    );
}

/// The snapshot is taken **before any guard moves**, and a guard is judged by where
/// it stood as the phase opened — not by where its own step leaves it (§8.3/#240).
///
/// A hunter one cell outside the bubble therefore still takes its step this turn,
/// even though that step carries it *inside*; it is frozen from the next turn on.
/// Re-deriving suppression after the step would freeze it a turn early, which is the
/// concrete way a second reading of the same fact would show up.
#[test]
fn a_guard_is_judged_where_the_phase_found_it_not_where_its_step_lands() {
    // The startup world phase (§4.2) walks this patroller one step toward the player,
    // to (10, 3) — a sight_distance of 7, one clear of the bubble. At that range it is
    // a §7.6 *glimpse*, so the guard Investigates rather than Chases; either way it
    // closes, which is all this test needs.
    let mut s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        vec![Guard::patrolling(Cell::new(10, 2))],
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Confusion));
    assert_eq!(s.guards()[0].pos(), Cell::new(10, 3));
    assert_ne!(s.guards()[0].state(), GuardState::Calm, "a live lead");
    assert!(
        !s.guard_confused(&s.guards()[0]),
        "one cell outside the bubble as the turn opens",
    );

    // Activation turn: the phase found it outside, so it acts — and its step lands it
    // inside. Both halves matter: it moved, and it is now suppressed.
    s.step(Input::Activate(AbilityId::Confusion));
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(10, 4),
        "judged where the phase found it, so its step still happens",
    );
    assert!(
        s.guard_confused(&s.guards()[0]),
        "and that step carried it into the bubble",
    );

    // From here the snapshot reads it as suppressed, so it is frozen.
    s.step(Input::Wait);
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(10, 4),
        "frozen from the next phase on",
    );
}

/// The scene the §7.7 call-in tests share: a guard that gets a **certain**-zone
/// sighting of the player (§7.6, ≤ 5) and then loses it to a cupboard walled into
/// its sight-shadow, plus a second guard far enough away to have seen nothing. The
/// chase therefore ends in a search — the moment a lost sighting is called in — and
/// there is somebody free to answer.
fn call_in_scene() -> State {
    let mut layout = open_room(30, 12);
    layout.place(Cell::new(4, 5), Terrain::Hideout); // the dive
    layout.place(Cell::new(4, 4), Terrain::Wall); // …unwitnessed (§15 Q5)
    State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![
            Guard::patrolling(Cell::new(5, 1)),  // 4 away — the certain zone
            Guard::patrolling(Cell::new(25, 9)), // far out of sight, free to be sent
        ],
        Vec::new(),
        Cell::new(28, 10),
    )
}

/// §7.7: with the modifier on, a guard that had the player in the certain zone and
/// then lost sight **calls it in** — one other guard converges on the cell where
/// contact broke and searches it. The reported cell is stale by construction: it is
/// where the player *was* when the chase ended, which is exactly where they are not.
#[test]
fn a_lost_confirmed_sighting_calls_one_guard_to_the_reported_cell() {
    let mut s = call_in_scene().with_modifiers(LevelModifiers {
        sighting_lost_calls_a_guard: true,
        ..LevelModifiers::default()
    });
    s.step(Input::Step(Direction::West)); // dive into the cupboard, breaking sight
    assert!(s.hidden(), "the player is concealed");

    let mut reported = None;
    for _ in 0..40 {
        for e in s.step(Input::Wait) {
            if let Event::CalledIn { at } = e {
                reported = Some(at);
            }
        }
        if reported.is_some() {
            break;
        }
    }
    let at = reported.expect("the lost chase was called in (§7.7)");
    assert_ne!(
        at,
        s.player(),
        "the reported cell is where contact broke, not where the player is",
    );
    assert_eq!(
        s.guards()[1].state(),
        GuardState::Responding,
        "the far guard was called in",
    );
    assert_eq!(
        s.guards()[1].destination(),
        Some(at),
        "it converges on the reported cell",
    );
}

/// The `sighting_lost_calls_a_guard` modifier (§12.6), directional at the run level
/// (§2.3's anti-facade rule): on the *same* scene and inputs, baseline calls nobody
/// — and, crucially, the guard that lost the player **still searches on its own**.
/// The modifier adds the calling of others, nothing else.
#[test]
fn baseline_calls_nobody_but_the_loser_still_searches() {
    let mut s = call_in_scene();
    s.step(Input::Step(Direction::West));

    let mut searched = false;
    for _ in 0..40 {
        for e in s.step(Input::Wait) {
            assert!(
                !matches!(e, Event::CalledIn { .. }),
                "baseline never calls anyone in",
            );
        }
        if s.guards()[0].state() == GuardState::Alerted {
            searched = true;
        }
        assert_ne!(
            s.guards()[1].state(),
            GuardState::Responding,
            "the far guard is never sent",
        );
    }
    assert!(
        searched,
        "the guard that lost the player searches regardless of the modifier",
    );
}

/// §7.7/§7.6: only a **confirmed** sighting is worth reporting. A guard that had no
/// more than a glimpse (the outer zone, 6–10 — Investigating, imprecise by design)
/// searches when its lead runs out like any other, but calls nobody: there is no
/// position it is sure enough of to report.
#[test]
fn a_glimpse_that_is_lost_calls_nobody() {
    let mut layout = open_room(30, 12);
    layout.place(Cell::new(4, 9), Terrain::Hideout);
    layout.place(Cell::new(4, 8), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(5, 9), // 8 from the guard — the glimpse zone, never the certain one
        Direction::North,
        vec![
            Guard::patrolling(Cell::new(5, 1)),
            Guard::patrolling(Cell::new(25, 2)),
        ],
        Vec::new(),
        Cell::new(28, 10),
    )
    .with_modifiers(LevelModifiers {
        sighting_lost_calls_a_guard: true,
        ..LevelModifiers::default()
    });

    // Whatever the guard makes of the glimpse, it never reaches the certain zone:
    // the player dives on the first turn and stays hidden.
    s.step(Input::Step(Direction::West));
    assert!(s.hidden());
    let mut glimpsed = false;
    let mut searched = false;
    for _ in 0..40 {
        for e in s.step(Input::Wait) {
            assert!(
                !matches!(e, Event::CalledIn { .. }),
                "a glimpse is never called in (§7.7)",
            );
        }
        match s.guards()[0].state() {
            GuardState::Investigating => glimpsed = true,
            GuardState::Alerted => searched = true,
            GuardState::Chasing => panic!("the fixture leaked a certain-zone sighting"),
            _ => {}
        }
    }
    // Without these the test would pass vacuously on a guard that saw nothing at
    // all: the point is that a guard which *did* react, and *did* end in a search,
    // still called nobody.
    assert!(glimpsed, "the guard did glimpse the player");
    assert!(
        searched,
        "and its lead ended in a search, which called nobody"
    );
}

/// §12.4: the call-in is deterministic — the same scene, modifiers and inputs pick
/// the same guard and report the same cell, every time.
#[test]
fn the_call_in_is_deterministic() {
    let run = || {
        let mut s = call_in_scene().with_modifiers(LevelModifiers {
            sighting_lost_calls_a_guard: true,
            ..LevelModifiers::default()
        });
        let mut seen = Vec::new();
        seen.push(s.step(Input::Step(Direction::West)));
        for _ in 0..40 {
            seen.push(s.step(Input::Wait));
        }
        (seen, s.guards()[1].destination())
    };
    assert_eq!(run(), run(), "same seed + same modifiers + same inputs");
}

/// §7.7: **"silence it before it reports"**, which the design gets for free. The
/// call fires when a chase *ends*, so a chaser taken down before that never reports
/// — there is no report timer to interrupt, just a guard that no longer exists to
/// decide anything. The window is real in play: the guard arrives on the cell it
/// last saw you at and only opens its search on the *following* turn, and while it
/// cannot see into the cupboard it is not aware, so the bump is a takedown (§7.2).
#[test]
fn taking_the_chaser_down_before_it_searches_suppresses_the_call() {
    let mut s = call_in_scene().with_modifiers(LevelModifiers {
        sighting_lost_calls_a_guard: true,
        ..LevelModifiers::default()
    });
    let broke_at = Cell::new(5, 5);
    s.step(Input::Step(Direction::West)); // dive west into the cupboard
    assert!(s.hidden());

    // Wait for the chaser to arrive on the cell it last had us — the turn before
    // it would open its search and call it in.
    let mut adjacent = false;
    for _ in 0..20 {
        for e in s.step(Input::Wait) {
            assert!(
                !matches!(e, Event::CalledIn { .. }),
                "nothing is reported while it is still walking",
            );
        }
        if s.guards()[0].pos() == broke_at {
            adjacent = true;
            break;
        }
    }
    assert!(adjacent, "the chaser reached the cell contact broke at");

    // Strike from the cupboard before it can turn round and report.
    let e = s.step(Input::Step(Direction::East));
    assert!(
        e.contains(&Event::TakenDown { at: broke_at }),
        "a guard that cannot see you is not aware, so the bump lands (§7.2)",
    );
    assert_eq!(s.guards().len(), 1, "only the far guard is left");

    for _ in 0..40 {
        for e in s.step(Input::Wait) {
            assert!(
                !matches!(e, Event::CalledIn { .. }),
                "a guard taken down before its search never reports (§7.7)",
            );
        }
    }
    assert_ne!(
        s.guards()[0].state(),
        GuardState::Responding,
        "nobody was ever called in",
    );
}

/// The scene the §7.7 body-call tests share: the player takes down a guard from a
/// cupboard, leaving a body in the open, with a patrolling guard that will walk
/// into sight of it and two more far away and free to be called.
fn body_call_scene() -> State {
    let mut layout = open_room(30, 12);
    layout.place(Cell::new(5, 5), Terrain::Hideout); // the player's cupboard
    State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(5, 4)), // the victim
            Guard::patrolling_to(Cell::new(5, 10), Cell::new(5, 6)), // walks up and finds it
            Guard::patrolling(Cell::new(25, 2)), // free
            Guard::patrolling(Cell::new(25, 9)), // free
        ],
        Vec::new(),
        Cell::new(28, 10),
    )
}

/// §7.7/§7.2: with the modifier on, discovering a body calls **two** guards to
/// converge on it and search — twice a sighting's one, which is the only sense in
/// which a body is "louder". The finder is not one of them: it is already there.
#[test]
fn a_found_body_calls_two_guards_that_are_not_the_finder() {
    let mut s = body_call_scene().with_modifiers(LevelModifiers {
        body_found_calls_two_guards: true,
        ..LevelModifiers::default()
    });
    s.step(Input::Step(Direction::North)); // takedown — a body at (5,4)
    let body = s.bodies()[0].cell();

    let mut called = None;
    for _ in 0..40 {
        for e in s.step(Input::Wait) {
            if let Event::CalledIn { at } = e {
                called = Some(at);
            }
        }
        if called.is_some() {
            break;
        }
    }
    assert_eq!(called, Some(body), "the call names the body's cell");

    let responding: Vec<usize> = s
        .guards()
        .iter()
        .enumerate()
        .filter(|(_, g)| g.state() == GuardState::Responding)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        responding.len(),
        radio::BODY_CALL_GUARDS,
        "a body calls exactly two (§7.7)",
    );
    for i in responding {
        assert_eq!(
            s.guards()[i].destination(),
            Some(body),
            "each converges on the body",
        );
    }
}

/// The `body_found_calls_two_guards` modifier (§12.6), directional at the run level
/// (§2.3): the same scene and inputs, baseline calls nobody — while the guard that
/// *found* the body still reacts exactly as it always did (§7.2's harder alert and
/// its own search). The modifier adds the calling of others, nothing else.
#[test]
fn baseline_calls_nobody_to_a_body_but_the_finder_still_reacts() {
    let mut s = body_call_scene();
    s.step(Input::Step(Direction::North));

    let mut found = false;
    for _ in 0..40 {
        for e in s.step(Input::Wait) {
            if matches!(e, Event::BodyFound { .. }) {
                found = true;
            }
            assert!(
                !matches!(e, Event::CalledIn { .. }),
                "baseline never calls anyone to a body",
            );
        }
        // The two far guards are never sent — only the finder ever reacts. The
        // victim left the vec at the takedown (§7.2), so they sit at 1 and 2.
        for far in [1, 2] {
            assert_ne!(
                s.guards()[far].state(),
                GuardState::Responding,
                "guard {far} was never called",
            );
        }
    }
    assert!(found, "the body was found regardless of the modifier");
    assert!(
        s.bodies()[0].found(),
        "and the finder's own §7.2 reaction is untouched",
    );
}

/// §7.2/§7.7: a body stowed in a cupboard is *gone* — no cone finds it, so it calls
/// nobody however long the run goes on. Hiding a body still buys you that (the
/// radio's missed ping is the separate §7.3 clock).
#[test]
fn a_stowed_body_calls_nobody() {
    let mut layout = open_room(30, 12);
    layout.place(Cell::new(5, 5), Terrain::Hideout); // the player's cupboard
    layout.place(Cell::new(5, 2), Terrain::Hideout); // where the body is stowed
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(5, 4)),
            Guard::patrolling(Cell::new(25, 2)),
            Guard::patrolling(Cell::new(25, 9)),
        ],
        Vec::new(),
        Cell::new(28, 10),
    )
    .with_modifiers(LevelModifiers {
        body_found_calls_two_guards: true,
        ..LevelModifiers::default()
    });

    s.step(Input::Step(Direction::North)); // takedown at (5,4)
    s.step(Input::Step(Direction::North)); // climb out onto the body
    s.step(Input::Step(Direction::North)); // step off to (5,3) — take hold
    s.step(Input::Step(Direction::North)); // the hold costs its turn (§8.3)
    s.step(Input::Step(Direction::North)); // stow it in the cupboard at (5,2)
    assert_eq!(
        s.layout().facility().terrain(s.bodies()[0].cell()),
        Some(Terrain::Hideout),
        "the body is stowed",
    );

    for _ in 0..40 {
        for e in s.step(Input::Wait) {
            assert!(
                !matches!(e, Event::CalledIn { .. } | Event::BodyFound { .. }),
                "a stowed body is never found and so never called in (§7.2)",
            );
        }
    }
}

/// §7.7: the call fires **once per body**, at discovery — a body that stays in a
/// guard's cone does not re-call every turn it remains visible.
#[test]
fn a_body_is_called_in_once_not_every_turn() {
    let mut s = body_call_scene().with_modifiers(LevelModifiers {
        body_found_calls_two_guards: true,
        ..LevelModifiers::default()
    });
    s.step(Input::Step(Direction::North));

    let mut calls = 0;
    for _ in 0..60 {
        for e in s.step(Input::Wait) {
            if matches!(e, Event::CalledIn { .. }) {
                calls += 1;
            }
        }
    }
    assert_eq!(calls, 1, "exactly one call for the one body");
}
