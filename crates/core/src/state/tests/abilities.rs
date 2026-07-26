//! Ability effects through the turn loop (§8.3).
//!
//! Each starting ability as the loop resolves it — Dephase's pass-through and its
//! lethal rematerialisation, the decoy's lifetime and the precedence that makes it
//! work only on guards that have lost you, Camouflage's move-reveals rule, Run's
//! double step, and the drag half-speed with the stow that locks a cupboard. The
//! economy itself (duration, cooldown, the lockout) is pinned in
//! [`crate::ability`]; what is pinned here is the effect on the world.

use crate::guard::{GuardState, CERTAIN_RANGE, GLIMPSE_RANGE};
use crate::state::*;
use crate::test_support::{open_room, solo};

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
