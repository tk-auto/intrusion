//! Control transfer through the turn loop (§8.1/#273) — the drone.
//!
//! What is pinned here is the *mechanic*: who the keys move, what a flight costs, and
//! the single window that covers both flying and hovering. The economy's own numbers
//! are pinned in [`crate::ability`]; the mark the board draws is pinned in
//! [`super::effects`].

use crate::control::{RemoteKind, DRONE_SIGHT_RANGE};
use crate::guard::Guard;
use crate::state::*;
use crate::test_support::{captured_at, open_room, solo};

/// A lone player in a big open room holding the Drone, standing at `at`.
fn pilot(at: Cell) -> State {
    State::new(
        open_room(20, 20),
        at,
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Drone))
}

/// Launch, and the keys are the drone's: it starts on the player's own cell, and a
/// step moves **it** while the body stands exactly where it was left (§8.1/#273).
#[test]
fn launching_hands_the_keys_to_the_drone_and_the_body_stays_put() {
    let mut s = pilot(Cell::new(5, 5));
    let events = s.step(Input::Activate(AbilityId::Drone));
    assert!(s.piloting(), "the launch takes the controls");
    assert_eq!(
        s.remote().map(|r| (r.cell(), r.kind(), r.source())),
        Some((Cell::new(5, 5), RemoteKind::Drone, AbilityId::Drone)),
        "it lifts off from the cell you are standing on",
    );
    assert!(events.contains(&Event::ControlTaken {
        at: Cell::new(5, 5)
    }));

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        s.remote().map(|r| r.cell()),
        Some(Cell::new(6, 5)),
        "the step flew the drone",
    );
    assert_eq!(s.player(), Cell::new(5, 5), "and not the body");
    assert_eq!(
        events,
        vec![Event::RemoteMoved {
            to: Cell::new(6, 5)
        }],
        "flying is not moving — no `Moved`, which the stillness rules read (§8.3)",
    );
}

/// Every drone move is a **full turn** (§4.2/§4.4): the world runs while the body
/// stands still, which is the whole cost of the ability (§2.3).
#[test]
fn flying_spends_the_turn_and_the_world_runs() {
    let mut s = pilot(Cell::new(5, 5));
    s.step(Input::Activate(AbilityId::Drone));
    let before = s.turn();
    s.step(Input::Step(Direction::East));
    s.step(Input::Wait);
    assert_eq!(
        s.turn(),
        before + 2,
        "flying and hovering both spend the turn"
    );
}

/// The cost, stated as a test (§2.3/§4.5): a guard walking into the parked body ends
/// the run while the player is looking through a camera somewhere else.
#[test]
fn the_parked_body_can_still_be_captured() {
    let mut layout = open_room(20, 20);
    layout.place(Cell::new(5, 5), Terrain::Floor);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(7, 5), Cell::new(3, 5))],
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Drone));
    s.step(Input::Activate(AbilityId::Drone));
    // Fly away and keep flying: the body never moves, and the guard beside it walks in.
    let mut ended = false;
    for _ in 0..6 {
        let events = s.step(Input::Step(Direction::North));
        if captured_at(&events, Cell::new(5, 5)) {
            ended = true;
            break;
        }
    }
    assert!(ended, "the body is a capture target while you fly (§4.5)");
    assert_eq!(s.outcome(), Outcome::Lost);
}

/// Letting go is **free** (§4.4) and — the point of the whole arrangement — does not
/// end the window: the drone holds its cell and the same clock runs on (§8.2/#273).
#[test]
fn letting_go_is_free_and_the_window_runs_on() {
    let mut s = pilot(Cell::new(5, 5));
    s.step(Input::Activate(AbilityId::Drone));
    s.step(Input::Step(Direction::East));
    let (turn, left) = (s.turn(), s.ability_state(AbilityId::Drone));

    let events = s.step(Input::Deactivate(AbilityId::Drone));
    assert!(!s.piloting(), "the keys are the body's again");
    assert_eq!(s.turn(), turn, "letting go is free (§4.4)");
    assert_eq!(
        s.ability_state(AbilityId::Drone),
        left,
        "and it refunds nothing and ends nothing — the same window, still running",
    );
    assert_eq!(
        s.remote().map(|r| r.cell()),
        Some(Cell::new(6, 5)),
        "the drone holds the cell it was left in",
    );
    assert!(events.contains(&Event::ControlReleased {
        at: Cell::new(6, 5)
    }));

    // And now the keys move the body again.
    s.step(Input::Step(Direction::South));
    assert_eq!(s.player(), Cell::new(5, 6));
    assert_eq!(
        s.remote().map(|r| r.cell()),
        Some(Cell::new(6, 5)),
        "a hovering drone does not follow you (§8.3: it is a static camera)",
    );
}

/// The hovering drone keeps **watching** (§11.5a/#273): its camera is unioned into the
/// player's view whether or not anybody is flying it, and what it sees is remembered.
#[test]
fn a_hovering_drone_still_feeds_vision_and_memory() {
    let mut s = pilot(Cell::new(2, 2));
    // Somewhere the player's own ~180° north-facing cone cannot reach.
    let far = Cell::new(2, 12);
    assert!(!s.player_fov().contains(far), "not visible to begin with");

    s.step(Input::Activate(AbilityId::Drone));
    for _ in 0..10 {
        s.step(Input::Step(Direction::South));
    }
    assert_eq!(s.remote().map(|r| r.cell()), Some(far));
    assert!(s.player_fov().contains(far), "the camera is in the union");

    s.step(Input::Deactivate(AbilityId::Drone));
    s.step(Input::Step(Direction::East)); // the body walks away
    assert!(
        s.player_fov().contains(far),
        "and it keeps watching once you have your body back — the whole point of the \
         second half of the window",
    );
    assert!(s.memory().contains(far), "and it writes memory (§11.5a)");
}

/// The camera is 360° and **short** (§6.1/#273): it reaches [`DRONE_SIGHT_RANGE`] in
/// every direction, and no further — it goes places you cannot, it does not out-see
/// you.
#[test]
fn the_drone_sees_a_short_full_circle() {
    let mut s = State::new(
        open_room(60, 60),
        Cell::new(20, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(58, 58),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Drone));
    s.step(Input::Activate(AbilityId::Drone));
    // Fly it far south — behind the player's own north-facing half-disc and well past
    // its range — so what the union holds down there is the camera's alone.
    for _ in 0..25 {
        s.step(Input::Step(Direction::South));
    }
    let drone = s.remote().expect("deployed").cell();
    assert_eq!(drone, Cell::new(20, 30));
    for dir in Direction::ALL {
        let reach =
            |steps: u32| (0..steps).fold(drone, |cell, _| cell.step(dir).expect("in bounds"));
        assert!(
            s.player_fov().contains(reach(DRONE_SIGHT_RANGE)),
            "the full circle reaches {DRONE_SIGHT_RANGE} cells {dir:?}",
        );
        assert!(
            !s.player_fov().contains(reach(DRONE_SIGHT_RANGE + 1)),
            "and no further {dir:?} — the box is the reach (§6.1)",
        );
    }
}

/// Geometry stops it (§10.3/#273): a wall is a free mis-input, not a crossing — the
/// drone is unthreatened, not incorporeal.
#[test]
fn the_drone_cannot_cross_a_wall() {
    let mut layout = open_room(20, 20);
    layout.place(Cell::new(6, 5), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Drone));
    s.step(Input::Activate(AbilityId::Drone));
    let turn = s.turn();
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        s.remote().map(|r| r.cell()),
        Some(Cell::new(5, 5)),
        "the wall held it",
    );
    assert_eq!(s.turn(), turn, "and the refusal is free (§4.4)");
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(6, 5)
        }],
    );
}

/// A **table** and a **shut door** do not (§10.3/#273): the machine is hand-sized and
/// airborne, so it goes over the furniture and through the panel's vents — and through
/// the panel **without opening it**, which is the difference between reading a wing and
/// unlocking one.
#[test]
fn the_drone_flies_over_tables_and_through_shut_doors() {
    let mut layout = open_room(20, 20);
    layout.place(Cell::new(6, 5), Terrain::PartialCover);
    layout.place(Cell::new(7, 5), Terrain::DoorPanelClosed);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Drone));
    s.step(Input::Activate(AbilityId::Drone));

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        s.remote().map(|r| r.cell()),
        Some(Cell::new(6, 5)),
        "over the table",
    );
    assert_eq!(
        events,
        vec![Event::RemoteMoved {
            to: Cell::new(6, 5)
        }]
    );

    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        s.remote().map(|r| r.cell()),
        Some(Cell::new(7, 5)),
        "and through the shut door's vents",
    );
    assert_eq!(
        events,
        vec![Event::RemoteMoved {
            to: Cell::new(7, 5)
        }]
    );
    assert_eq!(
        s.layout().facility().terrain(Cell::new(7, 5)),
        Some(Terrain::DoorPanelClosed),
        "and the door is still shut — a drone has no hands (§4.3)",
    );

    // Out the far side, so the crossing is a crossing and not a cul-de-sac.
    s.step(Input::Step(Direction::East));
    assert_eq!(s.remote().map(|r| r.cell()), Some(Cell::new(8, 5)));
}

/// A door's **frame** stops it (§10.4/#273): the hinge is structure, not a door, and
/// there is nothing to fly through.
#[test]
fn a_door_frame_stops_the_drone() {
    let mut layout = open_room(20, 20);
    layout.place(Cell::new(6, 5), Terrain::DoorHinge);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Drone));
    s.step(Input::Activate(AbilityId::Drone));
    let turn = s.turn();
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(s.remote().map(|r| r.cell()), Some(Cell::new(5, 5)));
    assert_eq!(s.turn(), turn, "and the refusal is free (§4.4)");
    assert_eq!(
        events,
        vec![Event::Bumped {
            into: Cell::new(6, 5)
        }]
    );
}

/// The solid usables stop it too (§4.3/#273): a console is a thing you bump, not a
/// passage, at any scale.
#[test]
fn a_console_stops_the_drone() {
    let mut layout = open_room(20, 20);
    layout.place(Cell::new(6, 5), Terrain::Console);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Drone));
    s.step(Input::Activate(AbilityId::Drone));
    s.step(Input::Step(Direction::East));
    assert_eq!(
        s.remote().map(|r| r.cell()),
        Some(Cell::new(5, 5)),
        "solid furniture is solid to it",
    );
}

/// Actors do not (§4.3/#273): it flies straight over a guard, and the guard neither
/// notices nor is touched.
#[test]
fn the_drone_flies_over_actors_and_guards_never_perceive_it() {
    let mut s = State::new(
        open_room(20, 20),
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 2))],
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Drone));
    let facing_away = s.guards()[0].state();
    s.step(Input::Activate(AbilityId::Drone));
    for _ in 0..3 {
        s.step(Input::Step(Direction::North));
    }
    assert_eq!(
        s.remote().map(|r| r.cell()),
        Some(Cell::new(5, 2)),
        "it shares the guard's cell — a remote is not an actor",
    );
    assert_eq!(
        s.guards()[0].state(),
        facing_away,
        "and the facility cannot perceive it (§7.4: nothing about the guard changed)",
    );
    assert_eq!(s.outcome(), Outcome::Playing, "and it is no capture target");
}

/// Taking the keys **back** off a hovering drone costs the turn and re-parks the body
/// (§8.1/#273) — the ability is a remote eye you can jump into, for the same price the
/// launch charged.
#[test]
fn the_controls_can_be_taken_back_for_a_turn() {
    let mut s = pilot(Cell::new(5, 5));
    s.step(Input::Activate(AbilityId::Drone));
    s.step(Input::Step(Direction::East));
    s.step(Input::Deactivate(AbilityId::Drone));

    assert_eq!(
        s.ability_input(AbilityId::Drone),
        Input::Activate(AbilityId::Drone),
        "the key of an unattended remote takes the keys back, it does not toggle off",
    );
    let turn = s.turn();
    let events = s.step(Input::Activate(AbilityId::Drone));
    assert!(s.piloting());
    assert_eq!(s.turn(), turn + 1, "and it costs the turn (§4.4)");
    assert!(events.contains(&Event::ControlTaken {
        at: Cell::new(6, 5)
    }));
    s.step(Input::Step(Direction::East));
    assert_eq!(s.remote().map(|r| r.cell()), Some(Cell::new(7, 5)));
    assert_eq!(s.player(), Cell::new(5, 5), "the body is parked again");
}

/// While the keys are the drone's, the key of an *attended* remote is the toggle-off
/// and every **other** ability is greyed and refused (§11.4/#273) — your hands are on
/// the controls.
#[test]
fn flying_greys_every_other_ability() {
    let mut s = pilot(Cell::new(5, 5)).with_loadout(
        Loadout::innate()
            .with(AbilityId::Drone)
            .with(AbilityId::Camouflage),
    );
    s.step(Input::Activate(AbilityId::Drone));

    assert_eq!(
        s.ability_state(AbilityId::Camouflage),
        AbilityState::Unusable,
        "the bar never advertises a press the loop will swallow (§11.4)",
    );
    assert_eq!(s.ability_state(AbilityId::Run), AbilityState::Unusable);
    let turn = s.turn();
    let events = s.step(Input::Activate(AbilityId::Camouflage));
    assert!(
        events.is_empty(),
        "silent — the reason is on the board (§11.7)"
    );
    assert_eq!(s.turn(), turn, "and free (§4.4)");
    assert_eq!(
        s.ability_state(AbilityId::Camouflage),
        AbilityState::Unusable,
        "nothing switched on",
    );
    assert!(
        matches!(
            s.ability_state(AbilityId::Drone),
            AbilityState::Active { .. }
        ),
        "the one you are flying is still yours to press",
    );
    assert_eq!(
        s.ability_input(AbilityId::Drone),
        Input::Deactivate(AbilityId::Drone),
        "and its key is the way out of the mode",
    );
}

/// The usable line is empty while flying (§11.4/#273): the body bumps nothing and the
/// drone has no interaction verb of its own to offer in its place.
#[test]
fn the_usable_line_is_silent_while_flying() {
    let mut layout = open_room(20, 20);
    layout.place(Cell::new(6, 5), Terrain::Console);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::East,
        Vec::new(),
        vec![Cell::new(6, 5)],
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Drone));
    assert!(
        !s.affordances().is_empty(),
        "the console is offered on foot"
    );
    s.step(Input::Activate(AbilityId::Drone));
    assert!(s.affordances().is_empty(), "and not while flying");
}

/// The window is the machine's whole life (§8.2/#273): when the duration runs out the
/// drone dies and the keys come back, even mid-flight — and the cooldown starts there,
/// not when the player let go.
#[test]
fn the_window_ending_kills_the_drone_and_returns_the_keys() {
    let duration = AbilityId::Drone
        .def()
        .economy()
        .expect("activated")
        .duration();
    let mut s = pilot(Cell::new(5, 5));
    s.step(Input::Activate(AbilityId::Drone));
    // Hover in place for the rest of the window: the last one ends it.
    let mut expiry = Vec::new();
    for turn in 1..duration {
        expiry = s.step(Input::Wait);
        if turn < duration - 1 {
            assert!(s.piloting(), "still flying");
        }
    }
    assert!(
        expiry.contains(&Event::AbilityExpired {
            ability: AbilityId::Drone
        }),
        "the duration counts its own activation turn (§8.2)",
    );
    assert!(!s.piloting(), "the keys come back with it");
    assert!(s.remote().is_none(), "and the machine is gone");
    assert!(
        expiry.contains(&Event::ControlReleased {
            at: Cell::new(5, 5)
        }),
        "the release is reported before the expiry that caused it",
    );
    assert!(
        matches!(
            s.ability_state(AbilityId::Drone),
            AbilityState::Cooling { .. }
        ),
        "and only now does the cooldown start (§8.2)",
    );
}

/// A drone left hovering dies on the same clock (§8.2/#273) — the linger *is* the rest
/// of the duration, and there is no second timer to run past it.
#[test]
fn a_hovering_drone_dies_with_the_window() {
    let duration = AbilityId::Drone
        .def()
        .economy()
        .expect("activated")
        .duration();
    let mut s = pilot(Cell::new(5, 5));
    s.step(Input::Activate(AbilityId::Drone));
    s.step(Input::Deactivate(AbilityId::Drone)); // free: no turn spent
    for _ in 1..duration - 1 {
        s.step(Input::Wait);
        assert!(s.remote().is_some(), "still watching");
    }
    let events = s.step(Input::Wait);
    assert!(events.contains(&Event::AbilityExpired {
        ability: AbilityId::Drone
    }));
    assert!(s.remote().is_none(), "the camera dies with the window");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::ControlReleased { .. })),
        "and nobody was holding the keys to give back",
    );
}

/// A second press of the toggle-off cannot end the window early (§8.2/#273): there is
/// no early recall, so on a hovering drone the key is a refused free no-op — the same
/// window, the same machine, and the controls still there to resume. Silent, like every
/// dead key while flying: the reason is on the board (§11.7).
#[test]
fn toggling_off_a_hovering_drone_is_refused() {
    let mut s = pilot(Cell::new(5, 5));
    s.step(Input::Activate(AbilityId::Drone));
    s.step(Input::Step(Direction::East));
    s.step(Input::Deactivate(AbilityId::Drone)); // let go: free, the window runs on
    let (turn, left) = (s.turn(), s.ability_state(AbilityId::Drone));

    let events = s.step(Input::Deactivate(AbilityId::Drone));
    assert_eq!(events, vec![], "refused silently — the drone is the reason");
    assert_eq!(s.turn(), turn, "free, like every refusal (§4.4)");
    assert_eq!(
        s.ability_state(AbilityId::Drone),
        left,
        "the window cannot be ended early — expiry is its only clock (§8.2)",
    );
    assert_eq!(
        s.remote().map(|r| r.cell()),
        Some(Cell::new(6, 5)),
        "and the machine still hovers where it was left",
    );

    s.step(Input::Activate(AbilityId::Drone));
    assert!(s.piloting(), "and the controls can still be resumed");
}

/// **Trading the drone away takes the machine with it** (§8.3/#266): the window's life
/// is the slot that holds it, and the slot just left the run — a camera with no ability
/// behind it would be a zombie nothing could ever end.
#[test]
fn trading_the_drone_away_takes_the_machine_with_it() {
    let mut layout = open_room(20, 20);
    layout.place(Cell::new(5, 6), Terrain::EquipmentCache);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::South,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(
        // Full-handed (§8.3): the bump must open an *offer*, not a plain salvage.
        Loadout::innate()
            .with(AbilityId::Drone)
            .with(AbilityId::Camouflage)
            .with(AbilityId::Autodoors),
    )
    .with_caches([AbilityId::Dephase]);

    s.step(Input::Activate(AbilityId::Drone));
    s.step(Input::Deactivate(AbilityId::Drone)); // let go: hovering on the player's cell
    assert!(s.remote().is_some());

    s.step(Input::Step(Direction::South)); // the bump opens the offer
    assert!(s.exchange().is_some(), "the crate offers");
    s.step(Input::Discard(AbilityId::Drone));
    assert!(!s.loadout().contains(AbilityId::Drone));
    assert!(
        s.remote().is_none(),
        "the machine goes with the ability that was its life (§8.2/#273)",
    );
    assert!(
        !s.piloting(),
        "and there are no controls left to be holding",
    );
}

/// You launch, and take the controls back, **on your feet** (§10.7/#273): a crawlspace
/// hides the body, and this ability's whole cost is the body being exposed (§2.3).
#[test]
fn a_crawlspace_refuses_the_launch() {
    let mut s = solo(Cell::new(4, 4)).with_loadout(Loadout::innate().with(AbilityId::Drone));
    assert_ne!(
        s.ability_state(AbilityId::Drone),
        AbilityState::Unusable,
        "on foot the ladder lets it fire",
    );

    // The run's own opening state is inside the tunnel, which is the case that matters:
    // a level whose first frame could scout the whole facility for free.
    let mut crawling = crate::level_seed::start_level(&crate::LevelSeed {
        seed: 8371,
        modifiers: crate::LevelModifiers::default(),
        abilities: Loadout::innate().with(AbilityId::Drone),
    })
    .expect("the v1 footprint carves");
    assert!(
        crawling.in_duct(),
        "the run opens in the tunnel (§4.5/#466)"
    );
    assert_eq!(
        crawling.ability_state(AbilityId::Drone),
        AbilityState::Unusable,
        "and the bar says so rather than advertising it (§11.4)",
    );
    let turn = crawling.turn();
    let events = crawling.step(Input::Activate(AbilityId::Drone));
    assert_eq!(
        events,
        vec![Event::LaunchRefused],
        "and it says why (§11.7)"
    );
    assert_eq!(turn, crawling.turn(), "free, like a wall bump (§4.4)");
    assert!(crawling.remote().is_none());

    // The player's own room-level case is the same rule, asserted through the ladder.
    s.step(Input::Activate(AbilityId::Drone));
    assert!(s.piloting(), "and on foot it launches");
}
