//! The turn loop itself (§4.2/§4.4/§4.5).
//!
//! The three-phase order and the startup turn, the **turn-cost rule** — every action
//! that changes the world costs the turn, and the enumerated free exceptions — the
//! two win/lose conditions, the player-phase bumps that are not a subsystem of their
//! own (a hideout climbed into, a table crouched behind), the sight phase recomputing
//! from the player's *current* pose, and the determinism a replay rests on (§12.4).

use crate::guard::GuardState;
use crate::state::*;
use crate::targeting::Target;
use crate::test_support::{open_room, solo};
use crate::vision::field_of_view;
use crate::{LevelModifiers, Rng};

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
        s.begin_ability_targeting(AbilityId::Run).unwrap().confirm(),
        Target::Itself(Cell::new(4, 4)),
    );
    // Decoy is direction-targeted: defaults to the player's facing.
    assert_eq!(
        s.begin_ability_targeting(AbilityId::Decoy)
            .unwrap()
            .confirm(),
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
            ability: AbilityId::Run,
            uses_left: None,
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
    // A hand-built state holds the innate set and nothing else (§8.3): the line
    // lists exactly what the run holds, never a roster of what exists (#244).
    let innate: Vec<AbilityId> = AbilityId::ALL
        .into_iter()
        .filter(|id| id.is_innate())
        .collect();
    assert_eq!(ids, innate, "one row per held ability, in order");

    // A held passive earns a row of its own (#264) — the line lists what you hold,
    // whether or not you can press it. It draws no notation for now; the always-on
    // marker is left to the ability line/panel rework.
    let with_passive = solo(Cell::new(4, 4)).with_loadout(Loadout::full());
    let rows = with_passive.ability_statuses();
    assert_eq!(
        rows.iter().map(|st| st.id).collect::<Vec<_>>(),
        AbilityId::ALL.to_vec(),
        "the full loadout lists every ability",
    );
    let vision = rows
        .iter()
        .find(|st| st.id == AbilityId::Vision)
        .expect("the passive has a row");
    assert_eq!(vision.state, AbilityState::Passive);
    assert_eq!(
        vision.bar_entry(),
        "Sight(on)",
        "named, and marked always-on"
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
/// not advance and **no guard steps** — and it still pays the full cooldown (§8.2
/// refunds nothing). Free means free: cancelling must never hand the facility a
/// turn, or players would hold a doomed sprint rather than pay for stopping it.
#[test]
fn toggling_an_ability_off_is_free() {
    let mut s = State::new(
        open_room(20, 20),
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(14, 14), Cell::new(14, 4))],
        Vec::new(),
        Cell::new(18, 18),
    );
    s.step(Input::Activate(AbilityId::Run)); // turn 1, Run active
    let guard_was = s.guards()[0].pos();
    let events = s.step(Input::Deactivate(AbilityId::Run));
    assert_eq!(
        events,
        vec![Event::AbilityDeactivated {
            ability: AbilityId::Run
        }]
    );
    assert_eq!(s.turn(), 1, "toggling off does not spend the turn");
    assert_eq!(s.guards()[0].pos(), guard_was, "and no guard steps for it");
    assert_eq!(
        s.ability_state(AbilityId::Run),
        AbilityState::Cooling { remaining: 12 },
        "early cancel still pays the whole cooldown",
    );
}

/// §8.2, the exploit to design out (#304): cancelling early must not get the
/// ability back sooner. The cooldown is frozen for the whole duration and only
/// drains once the ability is inactive, so an early toggle-off starts the *full*
/// cooldown from that turn — the lockout after it is exactly the lockout after an
/// expiry, just reached earlier. Cancelling costs you effect turns and saves you
/// nothing.
#[test]
fn an_early_toggle_off_shortens_no_cooldown() {
    // Run: 5 turns of duration, 12 of cooldown (§8.3).
    let ready_again = |mut s: State, toggle_off_after: u32| {
        s.step(Input::Activate(AbilityId::Run));
        while s.turn() < toggle_off_after {
            s.step(Input::Wait);
        }
        if toggle_off_after > 0 {
            s.step(Input::Deactivate(AbilityId::Run)); // free — the turn does not move
        }
        // Wait until the slot is Ready again and report how many turns that took
        // from the activation.
        while s.ability_state(AbilityId::Run) != AbilityState::Ready {
            s.step(Input::Wait);
        }
        s.turn()
    };

    // The natural expiry: Ready again on turn 5 + 12 = 17, activation on turn 1.
    let expiry = ready_again(solo(Cell::new(4, 4)), 0);
    assert_eq!(expiry, 17, "duration + cooldown, as §8.2 defines it");

    // Cancelled on turn 2 of the sprint: the full 12-turn cooldown runs from that
    // turn (2 + 12 = 14), so the ability comes back exactly 3 turns earlier — the
    // 3 turns of sprint the player threw away, and not one turn of cooldown.
    let cancelled = ready_again(solo(Cell::new(4, 4)), 2);
    assert_eq!(cancelled, 14, "the whole 12-turn cooldown, from the cancel");
    assert_eq!(
        expiry - cancelled,
        3,
        "the only thing saved is the duration given up: {expiry} vs {cancelled}",
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
    assert_eq!(events, vec![Event::ExitRefused { still_needed: 1 }]);
    assert_eq!(s.outcome(), Outcome::Playing);
    assert_eq!(s.turn(), 0);

    // Take the intel by bumping the console to the east.
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::IntelTaken {
            remaining: 0,
            still_needed: 0
        }],
    );
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
    assert!(
        events.contains(&Event::ExitRefused { still_needed: 1 }),
        "refused empty-handed, wanting the one intel the gate asks for",
    );
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
        events.contains(&Event::ExitRefused { still_needed: 1 }),
        "the exit refuses a partial set under the all-intel gate, wanting the rest",
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
/// built [`State`] already carries the player's sight and every guard's cone — and a
/// guard that has not moved is looking **south**, its initial facing (§7.1).
///
/// That opening sight is the **wait's**, not the half-disc (#383): behind the spawn
/// facing is lit too. The half-disc is what the *next* frame draws, which the second
/// half of this test pins — so the posture is legible here as a thing that ends.
#[test]
fn the_startup_turn_establishes_sight() {
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(8, 8))],
        Vec::new(),
        Cell::new(10, 10),
    );

    // The opening look is 360° (§5/§8.3/§9.1): two ahead *and* two behind are lit.
    assert!(s.player_fov().contains(Cell::new(5, 3)));
    assert!(
        s.player_fov().contains(Cell::new(5, 7)),
        "the run opens looking all round the entry room",
    );

    // The stationary guard looks south from spawn (§7.1): its wedge covers two
    // south, not two north.
    let g = &s.guards()[0];
    assert_eq!(g.facing(), Direction::South);
    assert!(g.fov().contains(Cell::new(8, 10)));
    assert!(!g.fov().contains(Cell::new(8, 6)));

    // One spent step and sight is the ordinary half-disc again: from (5,4) facing
    // north, two ahead is lit and two behind is dark.
    s.step(Input::Step(Direction::North));
    assert!(s.player_fov().contains(Cell::new(5, 2)));
    assert!(
        !s.player_fov().contains(Cell::new(5, 6)),
        "the first spent action ends the opening posture",
    );
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

/// §12.4/#304: the toggle-off is an ordinary input in the replay stream. A script
/// that cancels an ability **mid-duration** — the case a player can now drive, and
/// the one that leaves the deck in a state no other stream reaches — parses back and
/// reproduces the same run twice over, guards included.
#[test]
fn a_replay_with_a_mid_duration_toggle_off_reproduces_exactly() {
    // "+r" sprint, one doubled step, "-r" stop it early, then walk and wait.
    let script = "+rE-rEE...";
    let inputs = crate::parse_script(script).expect("the notation covers the toggle");
    assert!(
        inputs.contains(&Input::Deactivate(AbilityId::Run)),
        "the stream really carries the cancel",
    );

    let run = || {
        let mut s = State::new(
            open_room(20, 12),
            Cell::new(3, 6),
            Direction::North,
            vec![Guard::patrolling(Cell::new(10, 2))],
            Vec::new(),
            Cell::new(18, 10),
        )
        .with_rng(Rng::new(0x304));
        let events: Vec<Event> = inputs.iter().flat_map(|&i| s.step(i)).collect();
        let guards: Vec<Cell> = s.guards().iter().map(|g| g.pos()).collect();
        (
            events,
            s.player(),
            s.turn(),
            guards,
            s.ability_state(AbilityId::Run),
        )
    };

    let first = run();
    assert_eq!(first, run(), "same seed + same inputs → the same run");
    assert!(
        matches!(first.4, AbilityState::Cooling { .. }),
        "and the cancel is in the state the replay reproduced",
    );
}
