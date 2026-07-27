//! Ability effects through the turn loop (§8.3).
//!
//! Each starting ability as the loop resolves it — Dephase's pass-through and the
//! eject-and-stun its rematerialisation costs, the decoy's lifetime and the
//! precedence that makes it work only on guards that have lost you, Camouflage's move-reveals rule, Run's
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase));
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase));
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase));
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

/// A 12×12 room with a wall at `(5,4)`, the player one cell west of it holding
/// Dephase, `guards` posted, and the run's stream seeded with `seed` (§12.4). The
/// scene every eject test phases into.
fn wall_to_phase_into(guards: Vec<Guard>, seed: u64) -> State {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        guards,
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase))
    .with_rng(crate::Rng::new(seed))
}

/// Phase east into the solid at `(5,4)` — a wall in [`wall_to_phase_into`], any other
/// solid in the terrain sweep — and let the duration run out in there, returning the
/// expiry turn's events.
fn phase_into_the_solid(s: &mut State) -> Vec<Event> {
    s.step(Input::Activate(AbilityId::Dephase)); // active turn 1
    s.step(Input::Step(Direction::East)); // turn 2: into the wall
    assert_eq!(s.player(), Cell::new(5, 4), "standing inside the solid");
    s.step(Input::Wait) // turn 3: the duration ends in there
}

/// §8.3/#329: the cost that keeps Dephase from being free — the duration running out
/// inside a wall throws the player clear and leaves them **stunned**, rather than
/// ending the run. §4.5 is `[SETTLED]` that a guard's touch is the only loss
/// condition, and the lethal version was a second one the player could not see
/// coming (§2.2).
#[test]
fn dephase_expiring_inside_a_wall_throws_you_clear_and_stuns() {
    let mut s = wall_to_phase_into(Vec::new(), 7);
    let events = phase_into_the_solid(&mut s);

    let to = match events.as_slice() {
        [Event::AbilityExpired {
            ability: AbilityId::Dephase,
        }, Event::Ejected { from, to, stunned }] => {
            assert_eq!(
                *stunned,
                phase_eject_stun(1),
                "one cell out, one cell's stun"
            );
            assert_eq!(
                *from,
                Cell::new(5, 4),
                "the event names the solid it threw them out of",
            );
            *to
        }
        other => panic!("expected the expiry and the eject, got {other:?}"),
    };
    assert_eq!(s.outcome(), Outcome::Playing, "the run survives the wall");
    assert_eq!(s.player(), to, "…standing where the eject put them");
    assert_ne!(to, Cell::new(5, 4), "…and no longer inside the wall");
    assert!(
        s.layout().facility().can_enter(to, ACTOR_FILL),
        "{to:?} must admit a solid body",
    );
    assert_eq!(s.stunned(), phase_eject_stun(1));
}

/// §8.3: the landing is the **nearest** legal cell — the wall in this room is one
/// step from open floor on every side, so the eject can only ever be a §6.1 radius
/// of one. Asserted against the predicate rather than a hand-picked cell, over a
/// spread of seeds so the random tie-break cannot smuggle a far landing past.
#[test]
fn the_eject_lands_on_a_nearest_legal_cell() {
    for seed in 0..24 {
        let mut s = wall_to_phase_into(Vec::new(), seed);
        phase_into_the_solid(&mut s);
        let landed = s.player();
        assert_eq!(
            Cell::new(5, 4).sight_distance(landed),
            1,
            "seed {seed}: open floor touches the wall, so the eject is one cell",
        );
        assert!(s.layout().facility().can_enter(landed, ACTOR_FILL));
    }
}

/// §12.4: the draw comes off the run's threaded stream, so a seed reproduces the
/// landing exactly — and §8.3's randomness is real: across seeds the eject lands on
/// more than one side, which is what stops "phase into a wall" being a reliable way
/// *through* one.
#[test]
fn the_eject_is_random_but_reproducible() {
    let landing = |seed| {
        let mut s = wall_to_phase_into(Vec::new(), seed);
        phase_into_the_solid(&mut s);
        s.player()
    };
    for seed in 0..8 {
        assert_eq!(landing(seed), landing(seed), "seed {seed} reproduces");
    }
    let spread: std::collections::HashSet<Cell> = (0..40).map(landing).collect();
    assert!(
        spread.len() > 1,
        "the eject is a draw, not a fixed side: {spread:?}",
    );
    // The cell the player phased in from is among the landings, so the wall can hand
    // them straight back the way they came — no free passage.
    assert!(
        spread.contains(&Cell::new(4, 4)),
        "the way back is on the table: {spread:?}",
    );
}

/// §8.3/#329: the stun is exactly as many turns of agency as it was priced at — that
/// many inputs are swallowed as spent turns, and the next one is the player's again.
#[test]
fn the_stun_swallows_exactly_its_turns() {
    let mut s = wall_to_phase_into(Vec::new(), 3);
    phase_into_the_solid(&mut s);
    let landed = s.player();

    let owed_at_first = s.stunned();
    for owed in (1..=owed_at_first).rev() {
        assert_eq!(s.stunned(), owed);
        let turn = s.turn();
        let events = s.step(Input::Step(Direction::West));
        assert!(events.is_empty(), "a stunned turn does nothing: {events:?}");
        assert_eq!(s.player(), landed, "…and moves nobody");
        assert_eq!(s.turn(), turn + 1, "…but the turn is spent, so guards act");
    }

    assert_eq!(s.stunned(), 0, "the stun is paid off");
    s.step(Input::Step(Direction::West));
    assert_eq!(s.player().x, landed.x - 1, "the next input is the player's");
}

/// §8.3/#329: *every* input is swallowed while stunned — the two free actions (§4.4)
/// included. "You cannot act" is one rule with no exceptions, so a helpless player
/// gets no free poke at the world.
#[test]
fn every_input_kind_is_swallowed_while_stunned() {
    for input in [
        Input::Wait,
        Input::Step(Direction::North),
        Input::Activate(AbilityId::Run),
        Input::Deactivate(AbilityId::Dephase),
    ] {
        let mut s = wall_to_phase_into(Vec::new(), 11);
        phase_into_the_solid(&mut s);
        let (landed, turn, owed) = (s.player(), s.turn(), s.stunned());

        let events = s.step(input);
        assert!(events.is_empty(), "{input:?} said something: {events:?}");
        assert_eq!(s.player(), landed, "{input:?} moved the player");
        assert_eq!(s.turn(), turn + 1, "{input:?} did not spend the turn");
        assert_eq!(s.stunned(), owed - 1, "{input:?}");
        assert!(
            matches!(s.ability_state(AbilityId::Run), AbilityState::Ready),
            "{input:?} switched Run on while stunned",
        );
    }
}

/// §2.3/§11.4: the usable line must never offer what the next press will not
/// deliver — and while stunned no press delivers anything, so it goes quiet.
#[test]
fn the_usable_line_is_empty_while_stunned() {
    // A console beside the player, so there is a real affordance to suppress.
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        [Cell::new(4, 3)],
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase))
    .with_rng(crate::Rng::new(5));
    assert!(
        !s.affordances().is_empty(),
        "the console is on offer before the phase",
    );

    phase_into_the_solid(&mut s);
    assert!(s.stunned() > 0);
    assert!(
        s.affordances().is_empty(),
        "a stunned player bumps nothing: {:?}",
        s.affordances(),
    );
}

/// §4.5 **[SETTLED]**: contact is still the loss. The stun is a real price precisely
/// because a guard can walk into you while you stand there unable to move — the
/// death did not go away, it moved to a threat the player can see coming.
#[test]
fn a_stunned_player_can_still_be_captured() {
    // A pocket: a wall block at (10,4) whose whole ring is solid but for (9,4), so
    // the eject has exactly one cell to pick and the scene does not hang on the draw.
    // A guard walks row 4 east into it, four cells behind the player.
    let mut layout = open_room(20, 12);
    for cell in [
        Cell::new(9, 3),
        Cell::new(10, 3),
        Cell::new(11, 3),
        Cell::new(10, 4),
        Cell::new(11, 4),
        Cell::new(9, 5),
        Cell::new(10, 5),
        Cell::new(11, 5),
    ] {
        layout.place(cell, Terrain::Wall);
    }
    let mut s = State::new(
        layout,
        Cell::new(9, 4),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(4, 4), Cell::new(9, 4))],
        Vec::new(),
        Cell::new(2, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase));
    s.set_guard_dwell_chance(0); // one cell per turn, so the scene is not a race

    s.step(Input::Activate(AbilityId::Dephase)); // guard → (5,4)
    s.step(Input::Step(Direction::East)); // into the wall; guard → (6,4)
    let events = s.step(Input::Wait); // the duration ends in there; guard → (7,4)
    assert!(
        events.iter().any(|e| matches!(e, Event::Ejected { .. })),
        "the wall let go: {events:?}",
    );
    assert_eq!(s.player(), Cell::new(9, 4), "the pocket's one legal cell");
    assert_eq!(s.stunned(), phase_eject_stun(1), "one cell out");

    s.step(Input::Wait); // swallowed; the guard closes to (8,4), now adjacent
    assert_eq!(s.stunned(), 1, "still owed a turn, still unable to move");

    // The guard steps in while the player is helpless. The loss is the ordinary one
    // — §4.5's only loss condition, once more the only one.
    let events = s.step(Input::Wait);
    assert!(
        events.contains(&Event::Captured {
            by: Cell::new(9, 4)
        }),
        "a helpless player is captured by contact: {events:?}",
    );
    assert_eq!(s.outcome(), Outcome::Lost);
}

/// §8.3: a body cannot be hauled through a wall — the eject drops it where it lies,
/// and the player lands with their hands free.
#[test]
fn the_eject_drops_a_dragged_body() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    layout.place(Cell::new(4, 4), Terrain::Hideout); // conceal the takedown
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        vec![Guard::stationary(Cell::new(4, 3))],
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase))
    .with_rng(crate::Rng::new(9));
    s.step(Input::Step(Direction::North)); // take the guard down: a body at (4,3)
    s.step(Input::Step(Direction::North)); // climb out onto the body
    s.step(Input::Step(Direction::East)); // step off it: take hold
    assert!(s.dragging().is_some(), "dragging the body");

    // Phase south into the wall, hauling. Half speed means a step can be spent
    // standing still, so run until the duration expires.
    s.step(Input::Activate(AbilityId::Dephase));
    let mut events = Vec::new();
    for _ in 0..4 {
        events = s.step(Input::Step(Direction::South));
        if events.iter().any(|e| matches!(e, Event::Ejected { .. })) {
            break;
        }
    }
    let released = events
        .iter()
        .find_map(|e| match e {
            Event::BodyReleased { at } => Some(*at),
            _ => None,
        })
        .expect("the eject lets the body go");
    assert!(s.dragging().is_none(), "hands free after the eject");
    assert_eq!(
        s.bodies()[0].cell(),
        released,
        "the body stays where it was let go",
    );
    assert_ne!(s.player(), released, "…and the player is elsewhere");
}

/// §8.3: expiry somewhere a solid body *can* stand is unchanged — no eject, no stun.
/// Open floor is the ordinary case; a **duct** is the non-obvious one (§10.7 admits
/// it as a legal place to rematerialize), and a crawling player must never be spat
/// out of the crawlspace they deliberately climbed into. An **empty cupboard** is the
/// third: it admits an actor's fill (that is what climbing into one is), so a phase
/// that ends inside one simply leaves the player hidden in it — the eject is about
/// having nowhere to be, not about being somewhere furnished.
#[test]
fn a_legal_expiry_neither_ejects_nor_stuns() {
    // On open floor: phase into the wall and back out before the duration ends.
    let mut s = wall_to_phase_into(Vec::new(), 1);
    s.step(Input::Activate(AbilityId::Dephase));
    s.step(Input::Step(Direction::East)); // into the wall
    let events = s.step(Input::Step(Direction::East)); // out the far side, then expiry
    assert_eq!(s.player(), Cell::new(6, 4));
    assert!(
        !events.iter().any(|e| matches!(e, Event::Ejected { .. })),
        "an expiry on floor is just the ability fading: {events:?}",
    );
    assert_eq!(s.stunned(), 0);

    // Inside a duct (§10.7): the crawl resumes, untouched.
    let mut s = super::ducts::duct_world()
        .with_loadout(Loadout::innate().with(AbilityId::Dephase))
        .with_rng(crate::Rng::new(4));
    s.step(Input::Step(Direction::North)); // climb into the duct
    assert!(s.in_duct());
    s.step(Input::Activate(AbilityId::Dephase));
    let inside = s.player();
    for _ in 0..4 {
        let events = s.step(Input::Wait);
        assert!(
            !events.iter().any(|e| matches!(e, Event::Ejected { .. })),
            "a duct is a legal place to solidify (§10.7): {events:?}",
        );
    }
    assert_eq!(s.player(), inside, "still in the crawlspace");
    assert!(s.in_duct());
    assert_eq!(s.stunned(), 0);
    assert_eq!(s.outcome(), Outcome::Playing);

    // Inside an empty cupboard (§10.3): a legal place to stand, so the phase just
    // fades and leaves the player concealed in it.
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase))
    .with_rng(crate::Rng::new(2));
    let events = phase_into_the_solid(&mut s);
    assert!(
        !events.iter().any(|e| matches!(e, Event::Ejected { .. })),
        "a cupboard admits an actor, so there is nothing to throw clear of: {events:?}",
    );
    assert_eq!(s.player(), Cell::new(5, 4), "left standing in the cupboard");
    assert!(s.hidden(), "…and concealed by it");
    assert_eq!(s.stunned(), 0);
}

/// §8.3: [`Event::Entombed`] survives as the **degenerate** case only — a facility
/// with nowhere at all to be thrown clear to. No generated level can be one (§10.6:
/// the player started somewhere standable), so this is the impossible board, kept
/// truthful rather than left to become an impossible state.
#[test]
fn with_nowhere_to_go_the_wall_still_takes_you() {
    let mut f = crate::facility::Facility::walled_box(9, 9);
    for y in 0..9 {
        for x in 0..9 {
            f.set_terrain(x, y, Terrain::Wall);
        }
    }
    let mut s = State::new(
        crate::Layout::from_facility(f),
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(7, 7),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase));

    s.step(Input::Activate(AbilityId::Dephase));
    let mut events = Vec::new();
    for _ in 0..4 {
        events = s.step(Input::Wait);
        if s.outcome() == Outcome::Lost {
            break;
        }
    }
    assert_eq!(
        events.last(),
        Some(&Event::Entombed {
            at: Cell::new(4, 4)
        }),
        "nowhere to be thrown: the run ends, truthfully",
    );
    assert_eq!(s.outcome(), Outcome::Lost);
    assert_eq!(s.stunned(), 0, "no stun to serve — there is no run left");
    assert!(s.step(Input::Wait).is_empty(), "the run is over");
}

/// §8.3: the eject is about **solidity**, not about walls — a phase that ends inside
/// a table, a cupboard, a console or a shut door throws you clear exactly the same
/// way. This is what the near line's wording has to survive: "the wall spits you out"
/// would be untrue for every one of these, which is why the message names the tech.
#[test]
fn any_solid_ejects_you_not_just_a_wall() {
    for terrain in [
        Terrain::Wall,
        Terrain::PartialCover,
        Terrain::Console,
        Terrain::DoorPanelClosed,
    ] {
        let mut layout = open_room(12, 12);
        layout.place(Cell::new(5, 4), terrain);
        let mut s = State::new(
            layout,
            Cell::new(4, 4),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(10, 10),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Dephase))
        .with_rng(crate::Rng::new(6));

        let events = phase_into_the_solid(&mut s);
        assert!(
            events.iter().any(|e| matches!(e, Event::Ejected { .. })),
            "{terrain:?} is solid, so the phase ends the same way: {events:?}",
        );
        assert_eq!(s.outcome(), Outcome::Playing, "{terrain:?}");
        assert_eq!(s.stunned(), phase_eject_stun(1), "{terrain:?}");
        assert_ne!(
            s.player(),
            Cell::new(5, 4),
            "{terrain:?}: no longer inside it"
        );
    }
}

/// §8.3/#329: **the stun is as long as the throw.** Burying yourself further inside a
/// block is further from anywhere to stand, and costs proportionally more helplessness
/// to undo — which is what prices recklessness rather than charging the near miss and
/// the deep dive the same flat rate.
///
/// A 5×5 wall block spanning x 5..=9, y 2..=6, with open floor either side. Phasing to
/// its western face `(5,4)` is one cell from floor; one deeper, `(6,4)`, is two. Two
/// depths, two prices, each asserted against the distance actually travelled.
///
/// Two is also as deep as this ability can put you from open ground: Dephase runs for
/// three turns counting its activation, so a phase begun outside buys exactly two
/// steps. The stun therefore tops out at `phase_eject_stun(2)` in ordinary play — the
/// arithmetic goes further, the ability does not.
#[test]
fn a_deeper_eject_stuns_for_longer() {
    let block = |player: Cell| {
        let mut layout = open_room(16, 12);
        for y in 2..=6 {
            for x in 5..=9 {
                layout.place(Cell::new(x, y), Terrain::Wall);
            }
        }
        State::new(
            layout,
            player,
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(13, 10),
        )
        .with_loadout(Loadout::innate().with(AbilityId::Dephase))
        .with_rng(crate::Rng::new(13))
    };

    let mut stuns = Vec::new();
    // Each start is two steps west of the cell the phase strands the player in, so the
    // expiry lands exactly there.
    for (start, stuck, depth) in [
        (Cell::new(3, 4), Cell::new(5, 4), 1u32), // the block's face
        (Cell::new(4, 4), Cell::new(6, 4), 2),    // one ring deeper
    ] {
        let mut s = block(start);
        s.step(Input::Activate(AbilityId::Dephase)); // turn 1
        s.step(Input::Step(Direction::East)); // turn 2
        let events = s.step(Input::Step(Direction::East)); // turn 3: the duration ends
        assert_eq!(
            events.first(),
            Some(&Event::Moved { to: stuck }),
            "the second step should strand the player at {stuck:?}",
        );

        let thrown = stuck.sight_distance(s.player());
        assert_eq!(
            thrown, depth,
            "from {stuck:?} the nearest cell that admits a body is {depth} away",
        );
        assert_eq!(
            s.stunned(),
            phase_eject_stun(thrown),
            "stuck at {stuck:?}, thrown {thrown} cells: the stun is priced off the throw",
        );
        assert!(
            events.contains(&Event::Ejected {
                from: stuck,
                to: s.player(),
                stunned: s.stunned(),
            }),
            "…and the event carries the same number: {events:?}",
        );
        stuns.push(s.stunned());
    }

    assert!(
        stuns[1] > stuns[0],
        "the deep dive must cost more than the clip: {stuns:?}",
    );
}

/// §8.3 **[START]**: the stun's own numbers, pinned so a later change is a visible
/// edit rather than a silent retune. The shortest eject there is — one cell — costs
/// two turns, and every further cell thrown costs one more.
#[test]
fn the_stun_length_is_pinned() {
    assert_eq!(PHASE_EJECT_STUN_BASE, 1);
    assert_eq!(phase_eject_stun(1), 2, "the ordinary clip");
    for cells in 1..6 {
        assert_eq!(
            phase_eject_stun(cells + 1),
            phase_eject_stun(cells) + 1,
            "one more cell thrown is one more turn owed",
        );
    }
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase));
    s.step(Input::Activate(AbilityId::Dephase));
    s.step(Input::Step(Direction::East)); // inside the wall
    let turn = s.turn();
    let events = s.step(Input::Deactivate(AbilityId::Dephase));
    // It says why (§11.7/#304): now that the toggle is reachable from a key, a
    // press that changes nothing cannot pass in silence — the player asked to
    // solidify and is still phased.
    assert_eq!(
        events,
        vec![Event::RematerializeRefused],
        "nowhere to solidify: refused, and said so",
    );
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase));
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase));
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
    let mut s = solo(Cell::new(7, 4)).with_loadout(Loadout::innate().with(AbilityId::Decoy));
    s.step(Input::Step(Direction::East)); // (8,4), facing the border wall
    let events = s.step(Input::Activate(AbilityId::Decoy));
    assert!(events.is_empty(), "a faced wall refuses: a free mis-input");
    assert_eq!(s.turn(), 1, "only the step spent a turn");
    assert_eq!(
        s.ability_state(AbilityId::Decoy),
        AbilityState::Unusable,
        "and the bar says so before the press, not after it (§11.4/#345)",
    );
    assert_eq!(s.decoy(), None);

    s.step(Input::Step(Direction::West)); // (7,4), facing open floor
    assert_eq!(
        s.ability_state(AbilityId::Decoy),
        AbilityState::Ready,
        "one step back and it is ready again: the refusal cost nothing (§4.4)",
    );
    let events = s.step(Input::Activate(AbilityId::Decoy));
    assert_eq!(
        events,
        vec![Event::AbilityActivated {
            ability: AbilityId::Decoy,
            uses_left: None,
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Decoy));
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Decoy));
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
    let mut s = solo(Cell::new(4, 4)).with_loadout(Loadout::innate().with(AbilityId::Decoy));
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Camouflage));
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Camouflage));
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
    )
    .with_loadout(Loadout::innate().with(AbilityId::Camouflage));
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
        AbilityId::Run.def().economy().unwrap().duration(),
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

/// #304, the case that surfaced the bug: you sprint to break a sightline, then want
/// the last cells walked quietly and precisely. Switching Run off mid-sprint takes
/// effect immediately — the very next step is one cell, on the same turn boundary as
/// any other move, with no half-step and no lost turn.
#[test]
fn switching_run_off_mid_sprint_returns_the_step_to_one_cell() {
    let mut s = State::new(
        open_room(20, 10),
        Cell::new(2, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 8),
    );
    s.step(Input::Activate(AbilityId::Run)); // protected turn 1
    s.step(Input::Step(Direction::East)); // two cells: 2 → 4
    assert_eq!(s.player(), Cell::new(4, 5), "precondition: sprinting");

    let turn = s.turn();
    assert_eq!(
        s.step(s.ability_input(AbilityId::Run)),
        vec![Event::AbilityDeactivated {
            ability: AbilityId::Run
        }],
        "the same key that started the sprint stops it",
    );
    assert_eq!(s.turn(), turn, "stopping costs no turn (§4.4)");
    assert_eq!(s.player(), Cell::new(4, 5), "and moves nobody");

    // The next step is an ordinary one: one cell, one Moved, one spent turn.
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(5, 5), "one cell, precisely");
    assert_eq!(
        events,
        vec![Event::Moved {
            to: Cell::new(5, 5)
        }],
    );
    assert_eq!(s.turn(), turn + 1, "a plain step, on the usual boundary");
}

/// §8.3/#304: a cancelled cloak stops cloaking **that turn** — no lingering
/// protection, no special case. Camouflage's effect is read from the deck each
/// sight phase, so switching it off is enough to expose the player on the very next
/// sweep, exactly as its expiry does.
#[test]
fn switching_camouflage_off_exposes_the_player_at_once() {
    let mut s = State::new(
        open_room(12, 12),
        Cell::new(5, 6),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 2))],
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Camouflage));
    s.step(Input::Activate(AbilityId::Camouflage));
    assert!(!s.guards()[0].detected_player(), "precondition: cloaked");

    // Free, so the sight phase does not run on this input — the cloak is off, and
    // the next turn the standing player is in the cone.
    s.step(s.ability_input(AbilityId::Camouflage));
    assert!(matches!(
        s.ability_state(AbilityId::Camouflage),
        AbilityState::Cooling { .. }
    ));
    s.step(Input::Wait);
    assert!(
        s.guards()[0].detected_player(),
        "the cloak ended when it was switched off, not when it would have faded",
    );
}
