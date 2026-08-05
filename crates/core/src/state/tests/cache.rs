//! The equipment cache through the turn loop (§2.2/§8.3/§14 v3/#209).
//!
//! The one way salvaged tech enters a run. What is pinned here is the whole of the
//! bump's contract: that it hands over an ability, that the ability is usable *this
//! turn* rather than at the end of the raid, that the crate is then spent, and that the
//! find leaves the facility on the verdict — which is the seam the campaign layer folds
//! into the loadout every later facility boots with.

use crate::state::*;
use crate::test_support::open_room;
use crate::{AbilityId, Loadout};

/// The player next to a crate holding `holds`, in an empty room. Facing south so the
/// first `Step(South)` is the bump, exactly as the comms-console fixture is built.
///
/// The loadout is innate-only, so the salvaged ability is genuinely new — which is what
/// the pickup is *for*, and what makes "usable immediately" a claim with something to
/// test.
fn scene(holds: AbilityId) -> State {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 6), Terrain::EquipmentCache);
    State::new(
        layout,
        Cell::new(5, 5),
        Direction::South,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate())
    .with_cache(Some(holds))
}

/// §4.3/§8.3: the crate is bumped like everything else — the usable line offers `cache:
/// take tech`, the bump spends the turn, reports the ability by name, and the run holds
/// it from that moment.
#[test]
fn bumping_a_cache_salvages_the_tech_and_reports_it() {
    let mut s = scene(AbilityId::Decoy);
    assert_eq!(s.equipment_cache(), Some(Cell::new(5, 6)));
    assert_eq!(s.salvaged(), None, "nothing is salvaged until it is taken");
    assert!(
        !s.loadout().contains(AbilityId::Decoy),
        "the run does not hold the tech before it finds it (§2.2)",
    );
    assert!(
        s.affordances()
            .iter()
            .any(|(_, a)| *a == Affordance::SalvageTech),
        "the usable line offers the salvage (§11.4)",
    );

    let turn = s.turn();
    let e = s.step(Input::Step(Direction::South));
    assert!(
        e.contains(&Event::TechSalvaged {
            id: AbilityId::Decoy
        }),
        "the bump names what was found",
    );
    assert_eq!(s.player(), Cell::new(5, 5), "a bump does not move you");
    assert!(s.turn() > turn, "salvaging spends the turn (§4.4)");
    assert!(s.loadout().contains(AbilityId::Decoy), "the tech is yours");
}

/// **Usable immediately** — the acceptance criterion, stated as a run rather than as a
/// flag: the very next input fires the ability that was in the crate.
///
/// This is what stops the find being a deposit slip. §14 v3 asks for a power curve
/// inside the run, and an ability that only switched on after extraction would make the
/// detour a payment toward a later facility rather than a tool for this one.
#[test]
fn salvaged_tech_is_usable_the_moment_it_is_found() {
    let mut s = scene(AbilityId::Camouflage);
    assert_eq!(
        s.ability_state(AbilityId::Camouflage),
        AbilityState::Unusable,
        "an ability the run does not hold is not on the bar (§11.4)",
    );

    s.step(Input::Step(Direction::South));
    assert_eq!(
        s.ability_state(AbilityId::Camouflage),
        AbilityState::Ready,
        "found tech arrives ready, not cooling (§8.2)",
    );
    s.step(Input::Activate(AbilityId::Camouflage));
    assert!(
        matches!(
            s.ability_state(AbilityId::Camouflage),
            AbilityState::Active { .. }
        ),
        "the ability out of the crate fires on the turn after it was found",
    );
}

/// A **use budget** arrives full (§8.2/#302): the crate hands over the ability the
/// catalogue describes, not a half-spent copy of it. Pierce Wall is the one with a
/// budget, so it is the one that can tell.
///
/// Read off the deck rather than off [`ability_state`](State::ability_state), which
/// answers with the *context* as well (§11.4): the fixture stands in the middle of an
/// open room, where a bore has no unique wall to aim at and the bar would grey the row
/// however full its budget was.
#[test]
fn salvaged_tech_arrives_with_its_whole_level_budget() {
    let mut s = scene(AbilityId::PierceWall);
    assert_eq!(
        s.abilities.state(AbilityId::PierceWall),
        AbilityState::Unusable,
        "an ability the run does not hold has no economy to drive (§8.2)",
    );
    s.step(Input::Step(Direction::South));
    assert_eq!(
        s.abilities.uses_left(AbilityId::PierceWall),
        AbilityId::PierceWall
            .def()
            .economy()
            .and_then(|e| e.uses_per_level()),
        "found tech arrives with the whole level's supply, not a spent one",
    );
    assert!(
        matches!(
            s.abilities.state(AbilityId::PierceWall),
            AbilityState::Limited { .. }
        ),
        "and ready to fire, not cooling",
    );
}

/// **An emptied crate is scenery.** A second bump is the free §4.4 no-op, the usable
/// line goes quiet, and nothing is handed over twice — the spent console's rule, over
/// the spent crate.
#[test]
fn an_opened_cache_offers_nothing_and_costs_nothing() {
    let mut s = scene(AbilityId::Dephase);
    s.step(Input::Step(Direction::South));
    assert_eq!(s.salvaged(), Some(AbilityId::Dephase));

    let turn = s.turn();
    let e = s.step(Input::Step(Direction::South));
    assert!(
        !e.iter().any(|e| matches!(e, Event::TechSalvaged { .. })),
        "an empty crate has nothing left to give",
    );
    assert_eq!(s.turn(), turn, "a dead bump is free (§4.4)");
    assert!(
        !s.affordances()
            .iter()
            .any(|(_, a)| *a == Affordance::SalvageTech),
        "the usable line must not offer what a bump will not do (§2.3)",
    );
    // …and it recolours to spent scenery, like a used console (§11.2).
    assert!(
        s.spent_consoles().any(|c| c == Cell::new(5, 6)),
        "an emptied crate reads as spent, not as a live reward",
    );
}

/// **The find leaves on the verdict** (§2.2): what the raid salvaged is part of what
/// the raid was worth, so the campaign layer folds it into the run's loadout from the
/// same value it banks the intel from — no second channel.
#[test]
fn the_run_stats_carry_the_find_out_of_the_facility() {
    let mut s = scene(AbilityId::Lockdown);
    assert_eq!(s.run_stats().salvaged, None);
    s.step(Input::Step(Direction::South));
    assert_eq!(s.run_stats().salvaged, Some(AbilityId::Lockdown));
}

/// **Skipping it is legal, and costs the run nothing but the crate** — the ticket's
/// "optional exploration reward". A facility left with its cache unopened reports no
/// find, and the exit is not holding it against you: the gate is intel's, and under the
/// campaign's `IntelGate::None` there is no gate at all.
#[test]
fn a_cache_left_alone_is_a_legal_run() {
    let s = scene(AbilityId::Confusion);
    assert_eq!(s.run_stats().salvaged, None);
    assert_eq!(
        s.intel_needed_to_exit(),
        0,
        "a crate is never an exit requirement (§4.5)",
    );
}

/// **A quiet find** (§7.3): opening a crate is not tampering with a terminal, so it
/// raises no alert. The cost of a cache is the detour and the turn, paid on the way
/// there — not a rung the facility climbs for having been robbed of it.
#[test]
fn salvaging_raises_no_alert() {
    let mut s = scene(AbilityId::Vision);
    let rung = s.alert();
    let e = s.step(Input::Step(Direction::South));
    assert_eq!(s.alert(), rung, "the crate says nothing to control");
    assert!(!e.iter().any(|e| matches!(e, Event::AlertRaised { .. })));
}

/// A crate with **nothing in it** is not a crate: a state handed no contents keeps the
/// terrain as scenery and offers no bump, which is how the pool-exhausted case
/// ([`cache_contents`](crate::cache_contents) answering `None`) reaches the board.
#[test]
fn a_cache_with_no_contents_is_scenery() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 6), Terrain::EquipmentCache);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::South,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate())
    .with_cache(None);

    assert_eq!(s.equipment_cache(), None);
    assert!(!s
        .affordances()
        .iter()
        .any(|(_, a)| *a == Affordance::SalvageTech));
    let turn = s.turn();
    let e = s.step(Input::Step(Direction::South));
    assert!(!e.iter().any(|e| matches!(e, Event::TechSalvaged { .. })));
    assert_eq!(s.turn(), turn, "a dead bump is free (§4.4)");
}
