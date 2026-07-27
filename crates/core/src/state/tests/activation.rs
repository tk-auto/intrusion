//! The activation precondition ladder (§8.4/§11.4, #345) — the one predicate behind
//! *"would this press do anything?"*.
//!
//! Two things are pinned here, and they are different things. That the ladder is
//! **shared**: the bar greys exactly the presses the turn loop refuses, so the two can
//! never drift into disagreeing about whether an ability is on. And the **precedence
//! rule**: which of the economy and the context wins when both have something to say,
//! decided once for every pair rather than per ability.
//!
//! The per-ability preconditions themselves are pinned where they live — [`bore`] for
//! Pierce Wall's geometry, [`lockdown`] for the door box, [`guards`] for the blast,
//! [`abilities`] for the decoy's faced cell. This file is about the ladder that asks
//! them.
//!
//! [`bore`]: super::bore
//! [`lockdown`]: super::lockdown
//! [`guards`]: super::guards
//! [`abilities`]: super::abilities

use crate::ability::PIERCE_WALL_USES;
use crate::state::*;
use crate::test_support::open_room;

/// A player holding `id` in a 12×12 room, at `player`, facing `facing`.
fn armed(layout: Layout, player: Cell, facing: Direction, id: AbilityId) -> State {
    State::new(
        layout,
        player,
        facing,
        Vec::new(),
        Vec::new(),
        Cell::new(1, 1),
    )
    .with_loadout(Loadout::innate().with(id))
}

/// **The bar stops advertising a press that cannot fire** (§11.4/#345) — the hole this
/// ticket closes. The catalog has always documented `Unusable` as *"no adjacent target
/// … discoverable, but greyed"*, and nothing produced it: a decoy facing a wall drew
/// `Decoy`, plain and ready, over a key that was a guaranteed free no-op.
///
/// One step is the whole difference, and it is the same [`decoy_spawn_cell`] verdict
/// the activation gates on — which is what makes the entry flickering as the player
/// turns a *teaching* signal rather than noise.
///
/// [`decoy_spawn_cell`]: State::decoy_spawn_cell
#[test]
fn the_bar_greys_a_press_that_cannot_fire() {
    // Facing the room's own border wall: nowhere for a fake intruder to stand.
    let mut s = armed(
        open_room(12, 12),
        Cell::new(5, 1),
        Direction::North,
        AbilityId::Decoy,
    );
    assert_eq!(s.decoy_spawn_cell(), None, "precondition: a wall is faced");
    assert_eq!(s.ability_state(AbilityId::Decoy), AbilityState::Unusable);

    // Turn to face the open room, and the very same key is ready again.
    s.step(Input::Step(Direction::South));
    assert_eq!(s.decoy_spawn_cell(), Some(Cell::new(5, 3)));
    assert_eq!(s.ability_state(AbilityId::Decoy), AbilityState::Ready);
}

/// **The key never becomes inert** (§4.4/§11.4). `Unusable` changes what the player is
/// *told*, never what the press costs: it still resolves to the activation
/// ([`ability_input`]), that activation still refuses for free — no turn, no use, no
/// cooldown — and it still speaks, because the rule is the thing the player is
/// learning and silence teaches nothing (§11.7).
///
/// Pierce Wall is the case that shows all four at once: greyed on open floor, with a
/// full budget behind the grey, and a refusal that names which rule it broke.
///
/// [`ability_input`]: State::ability_input
#[test]
fn an_unusable_press_is_still_the_free_refusal_that_speaks() {
    let mut s = armed(
        open_room(12, 12),
        Cell::new(5, 5),
        Direction::North,
        AbilityId::PierceWall,
    );
    assert_eq!(
        s.ability_state(AbilityId::PierceWall),
        AbilityState::Unusable,
        "no wall touches the player, so the press cannot fire",
    );
    assert_eq!(
        s.ability_input(AbilityId::PierceWall),
        Input::Activate(AbilityId::PierceWall),
        "the key still activates — greyed is not disabled",
    );

    let events = s.step(s.ability_input(AbilityId::PierceWall));
    assert_eq!(
        events,
        vec![Event::BoreRefused {
            reason: BoreRefusal::NothingToBore
        }],
        "and says which rule it broke (§11.7)",
    );
    assert_eq!(s.turn(), 0, "free: the turn is not spent (§4.4)");
    assert_eq!(
        s.abilities.uses_left(AbilityId::PierceWall),
        Some(PIERCE_WALL_USES),
        "nor a use (§8.2/#302)",
    );
}

/// **The precedence rule, for every pair** (§11.4/#345): the economy is asked first,
/// and the context only speaks when the economy has nothing left to say. `Active`,
/// `Cooling` and `Exhausted` survive a missing target untouched; only `Ready` and
/// `Limited` — the two that mean *press it now* — are overruled to `Unusable`.
///
/// One rule, not a per-ability judgement call, and the reasoning is the economy's own:
/// each state reports the fact that actually governs the ability right now. A running
/// window is what the player is playing off; a cooldown carries a number where
/// `Unusable` carries a dash; a spent budget is the deeper fact than a wrong cell. Only
/// where the bar would otherwise promise a press that cannot fire does the context win.
#[test]
fn the_economy_outranks_the_context_unless_it_says_press_me() {
    // A wall standing north of (5,4), so a player who steps up to it faces a cell no
    // decoy can occupy — while the deck behind it is driven through every state.
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 3), Terrain::Wall);
    let mut s = armed(layout, Cell::new(5, 5), Direction::East, AbilityId::Decoy);

    // Ready + no target → Unusable. (Facing east is open; face the wall first.)
    s.step(Input::Step(Direction::North));
    assert_eq!(
        s.player(),
        Cell::new(5, 4),
        "up against the wall, facing it"
    );
    assert!(!s.would_fire(AbilityId::Decoy), "precondition: no target");
    assert_eq!(s.ability_state(AbilityId::Decoy), AbilityState::Unusable);

    // Active + no target → Active. Switch it on facing the open room, then step back
    // up to the wall: the window it bought is running and the bar must keep saying so.
    s.step(Input::Step(Direction::South));
    s.step(Input::Activate(AbilityId::Decoy));
    assert!(s.decoy().is_some(), "precondition: the fake is out");
    s.step(Input::Step(Direction::North));
    assert!(!s.would_fire(AbilityId::Decoy), "precondition: no target");
    assert!(
        matches!(
            s.ability_state(AbilityId::Decoy),
            AbilityState::Active { .. }
        ),
        "a running window is not blanked by a wrong cell: {:?}",
        s.ability_state(AbilityId::Decoy),
    );

    // Cooling + no target → Cooling. The early toggle-off is free, so nothing has
    // moved and the faced cell is still the wall.
    s.step(Input::Deactivate(AbilityId::Decoy));
    assert!(!s.would_fire(AbilityId::Decoy), "precondition: no target");
    assert!(
        matches!(
            s.ability_state(AbilityId::Decoy),
            AbilityState::Cooling { .. }
        ),
        "a number the player can count beats a dash: {:?}",
        s.ability_state(AbilityId::Decoy),
    );

    // Limited + no target → Unusable, and Exhausted + no target → Exhausted. Both on
    // Pierce Wall, the one budgeted ability (§8.2/#302), from open floor where its
    // geometry refuses.
    let mut s = armed(
        open_room(20, 20),
        Cell::new(5, 5),
        Direction::North,
        AbilityId::PierceWall,
    );
    assert_eq!(
        s.abilities.state(AbilityId::PierceWall),
        AbilityState::Limited {
            uses: PIERCE_WALL_USES
        },
        "precondition: the budget is full underneath",
    );
    assert_eq!(
        s.ability_state(AbilityId::PierceWall),
        AbilityState::Unusable,
        "a supply is no promise where the press cannot fire",
    );

    // Spend the budget against three wall faces, then walk back into the open.
    for y in [3, 6, 9] {
        s.layout.place(Cell::new(6, y), Terrain::Wall);
    }
    for (i, y) in [3, 6, 9].into_iter().enumerate() {
        s.player = Cell::new(5, y);
        s.step(Input::Activate(AbilityId::PierceWall));
        assert_eq!(
            s.abilities.uses_left(AbilityId::PierceWall),
            Some(PIERCE_WALL_USES - 1 - i as u32),
        );
    }
    s.player = Cell::new(5, 15);
    assert!(!s.would_fire(AbilityId::PierceWall), "no wall, no uses");
    assert_eq!(
        s.ability_state(AbilityId::PierceWall),
        AbilityState::Exhausted,
        "the spent budget is the deeper fact, and draws the same dash",
    );
}

/// **One aimed target per ability** — the assumption
/// [`Aimed`](crate::state::activation::Aimed) is a choice rather than a set. An ability
/// declaring two would silently get only the first arm of the ladder, so the catalog is
/// checked rather than trusted: adding such a row fails here, where the fix (widen
/// `Aimed`) is obvious, instead of shipping half an activation.
#[test]
fn no_ability_needs_two_targets() {
    for id in AbilityId::ALL {
        let aimed = usize::from(declares(id, Effect::SpawnDecoy))
            + usize::from(declares(id, Effect::SealDoors))
            + usize::from(declares(id, Effect::Confuse))
            + usize::from(id == AbilityId::PierceWall);
        assert!(
            aimed <= 1,
            "{} declares {aimed} aimed effects — `Aimed` would drop all but the first",
            id.name(),
        );
    }
}
