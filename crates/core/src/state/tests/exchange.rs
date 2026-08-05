//! The **exchange** through the turn loop (§8.3/§8.4/#266).
//!
//! [`cache`](super::cache) pins the crate: what it hands over, what it refuses, and that
//! a full run is *offered* rather than turned away. This pins what happens next — the
//! four candidates, the trade, the decline, and the one rule that makes an offer a
//! decision rather than a suggestion: while it is open, the loop answers nothing else.

use crate::state::*;
use crate::test_support::open_room;
use crate::{AbilityId, Loadout};

/// The three pieces of tech a full run carries here, in bar order.
const HELD: [AbilityId; 3] = [
    AbilityId::Camouflage,
    AbilityId::Decoy,
    AbilityId::Autodoors,
];

/// What the crate in these scenes holds — a fourth piece of tech, so the bump is an
/// offer rather than a duplicate refusal.
const OFFERED: AbilityId = AbilityId::Dephase;

/// A full-handed run standing north of a crate, facing it: the first `Step(South)` is
/// the bump that opens the exchange.
fn at_the_crate() -> State {
    let held = HELD.into_iter().fold(Loadout::innate(), Loadout::with);
    assert_eq!(held.tech_held(), AbilityId::MAX_TECH_HELD);
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
    .with_loadout(held)
    .with_caches([OFFERED])
}

/// The same run, with the offer already open.
fn offering() -> State {
    let mut s = at_the_crate();
    s.step(Input::Step(Direction::South));
    assert!(s.exchange().is_some(), "the bump opens the offer");
    s
}

/// **The four candidates are the three you hold and the one in the box** — and the
/// crate's is last, so the new thing is always in the same slot whatever the run is
/// carrying (§11.4).
///
/// This is the whole of the UI's source: the bar draws this row, the digits fire it and
/// a tap hit-tests it, all through [`State::bar_statuses`].
#[test]
fn the_bar_becomes_the_four_candidates() {
    let s = offering();
    let row: Vec<AbilityId> = s.bar_statuses().iter().map(|status| status.id).collect();
    assert_eq!(row, [HELD[0], HELD[1], HELD[2], OFFERED]);
    assert!(
        !row.contains(&AbilityId::Run),
        "an innate ability is never traded, so it is not on the row (§8.3)",
    );
    assert_eq!(
        s.bar_statuses().last().map(|status| status.state),
        Some(AbilityState::Offered),
        "exactly one entry reads as the new one",
    );
    for status in s.bar_statuses().iter().take(HELD.len()) {
        assert_eq!(
            status.state,
            AbilityState::Ready,
            "a candidate carries no clock — the row is a choice, not a readout",
        );
    }
}

/// **Every held candidate is a legal trade**, and each yields a valid loadout: the one
/// named goes, the crate's arrives, the count is still the §8.3 cap. The acceptance
/// criterion, walked over all three choices rather than the one that happened to be
/// first.
#[test]
fn each_choice_yields_a_valid_three_ability_loadout() {
    for dropped in HELD {
        let mut s = offering();
        let turn = s.turn();
        let e = s.step(Input::Discard(dropped));

        assert!(
            e.contains(&Event::Traded {
                taken: OFFERED,
                dropped,
            }),
            "the trade names both halves",
        );
        assert!(s.loadout().contains(OFFERED), "the crate's tech is yours");
        assert!(!s.loadout().contains(dropped), "and the traded one is gone");
        assert_eq!(
            s.loadout().tech_held(),
            AbilityId::MAX_TECH_HELD,
            "still exactly three, never four (§8.3)",
        );
        assert!(
            s.loadout().contains(AbilityId::Run),
            "the innate set is untouched by a trade",
        );
        assert_eq!(s.turn(), turn + 1, "the trade spends the turn (§4.4)");
        assert_eq!(s.exchange(), None, "and the offer is answered");
        assert_eq!(
            s.salvaged(),
            Loadout::empty().with(OFFERED),
            "the crate is opened, and the facility's haul says what it gave",
        );
    }
}

/// **Declining leaves everything exactly as it was** (§4.4/§11.6): the loadout is
/// unchanged, no turn is spent, and the crate still stands with its tech in it — so a run
/// that comes back having traded that piece away finds it unopened.
#[test]
fn declining_changes_nothing_and_leaves_the_crate() {
    let mut s = offering();
    let before = s.loadout();
    let turn = s.turn();

    let e = s.step(Input::Discard(OFFERED));
    assert!(e.contains(&Event::ExchangeDeclined { id: OFFERED }));
    assert_eq!(s.loadout(), before, "the unchanged loadout");
    assert_eq!(s.turn(), turn, "a decline is free");
    assert_eq!(s.exchange(), None, "the offer is closed");
    assert_eq!(s.salvaged(), Loadout::empty(), "the crate is untouched");
    assert!(
        s.affordances()
            .iter()
            .any(|(_, a)| *a == Affordance::SalvageSwap),
        "and it is still standing there, still offering",
    );
}

/// **The offer can be re-opened after a decline.** Nothing is consumed by walking away
/// from a decision, so a player who declined by reflex is one bump from the same choice
/// — which is most of what makes the decline safe to press.
#[test]
fn a_declined_crate_offers_again() {
    let mut s = offering();
    s.step(Input::Discard(OFFERED));
    s.step(Input::Step(Direction::South));
    assert_eq!(s.exchange().map(|x| x.offered()), Some(OFFERED));
}

/// **Nothing else happens while an offer is open** (§8.3/#266) — the rule that makes the
/// exchange a decision the facility waits on rather than a panel the world ignores.
///
/// A step, a wait, an activation: each is a free no-op, so the turn does not advance and
/// no guard moves. Pinned in the **core**, because that is where the rule lives: a shell
/// can only make its own input path obey it.
#[test]
fn an_open_offer_takes_nothing_but_its_answer() {
    let mut s = offering();
    let (turn, at) = (s.turn(), s.player());
    for input in [
        Input::Step(Direction::North),
        Input::Step(Direction::South),
        Input::Wait,
        Input::Activate(HELD[0]),
        Input::Deactivate(HELD[0]),
    ] {
        let e = s.step(input);
        assert!(e.is_empty(), "{input:?} did something: {e:?}");
        assert_eq!(s.turn(), turn, "{input:?} spent a turn");
        assert_eq!(s.player(), at, "{input:?} moved the player");
        assert!(s.exchange().is_some(), "{input:?} answered the offer");
        assert_eq!(
            s.ability_state(HELD[0]),
            AbilityState::Ready,
            "{input:?} reached the deck",
        );
    }
}

/// **The question survives the keys pressed at it** (§11.7/#266) — the bug this closes:
/// pressing an arrow at a crate used to wipe the very line saying what was being asked.
///
/// Two things keep it up. A swallowed input returns before the message bookkeeping, so
/// it files nothing and un-says nothing — it never reached the loop. And the **ambient
/// floor** carries the offer, so even once the live message has aged out the row is still
/// stating the standing question, which is where a standing state belongs (§11.4).
#[test]
fn the_near_line_keeps_asking_while_the_offer_is_open() {
    let mut s = offering();
    let asked = crate::near_line(&s).text;
    assert!(
        asked.contains(OFFERED.name()),
        "the offer speaks: {asked:?}"
    );

    for input in [
        Input::Step(Direction::North),
        Input::Step(Direction::West),
        Input::Wait,
    ] {
        s.step(input);
        assert_eq!(
            crate::near_line(&s).text,
            asked,
            "{input:?} changed what the near line is asking",
        );
    }

    // …and it stops the moment the offer is answered, rather than lingering as a
    // question about a crate that is no longer asking.
    s.step(Input::Discard(OFFERED));
    assert_ne!(crate::near_line(&s).text, asked);
}

/// **A discard naming something that is not a candidate is a free mis-input** (§4.4):
/// an ability the run does not hold, an innate one, or any discard at all with no offer
/// open. Silent and free, exactly like activating a cooling ability.
#[test]
fn a_discard_off_the_row_is_a_free_no_op() {
    let mut s = offering();
    let before = s.loadout();
    for id in [AbilityId::Run, AbilityId::Lockdown] {
        let e = s.step(Input::Discard(id));
        assert!(e.is_empty(), "{id:?} said something: {e:?}");
        assert_eq!(s.loadout(), before);
        assert!(s.exchange().is_some(), "{id:?} answered the offer");
    }

    // …and with no offer open at all, on a run that holds the ability named.
    let mut s = at_the_crate();
    let before = s.loadout();
    let turn = s.turn();
    assert!(s.step(Input::Discard(HELD[0])).is_empty());
    assert_eq!(
        s.loadout(),
        before,
        "nothing is dropped outside an exchange"
    );
    assert_eq!(s.turn(), turn);
}

/// **The traded ability stops working the moment it goes** (§8.2/#266). An activated
/// ability is *in effect* by its slot rather than by loadout membership, so trading one
/// away mid-window has to switch it off — or the run would keep the effect of a tool it
/// no longer holds and the bar would have nowhere to say so.
#[test]
fn trading_away_a_running_ability_ends_it() {
    let mut s = at_the_crate();
    s.step(Input::Activate(AbilityId::Camouflage));
    assert!(matches!(
        s.ability_state(AbilityId::Camouflage),
        AbilityState::Active { .. }
    ));

    s.step(Input::Step(Direction::South));
    s.step(Input::Discard(AbilityId::Camouflage));
    assert!(!s.loadout().contains(AbilityId::Camouflage));
    assert_eq!(
        s.ability_state(AbilityId::Camouflage),
        AbilityState::Unusable,
        "an ability the run no longer holds is not on the bar, let alone running",
    );
}

/// **A trade does not launder a clock.** The slot the traded ability leaves behind is
/// untouched, so a run that gives one up and finds the same tech again in another crate
/// picks it up exactly as cool as it put it down — drop-and-refind is not a free recharge
/// (§8.2).
#[test]
fn a_traded_ability_keeps_its_cooldown_if_it_comes_back() {
    // Two crates: the one south holds the offer, the one west holds the very tech this
    // run is about to trade away. They are stocked in the grid's own order.
    let held = HELD.into_iter().fold(Loadout::innate(), Loadout::with);
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(4, 5), Terrain::EquipmentCache);
    layout.place(Cell::new(5, 6), Terrain::EquipmentCache);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::South,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(held)
    .with_caches([AbilityId::Camouflage, OFFERED]);

    // Spend Camouflage and switch it off early, so it is left cooling (§8.2).
    s.step(Input::Activate(AbilityId::Camouflage));
    s.step(Input::Deactivate(AbilityId::Camouflage));
    let cooling = s.ability_state(AbilityId::Camouflage);
    assert!(
        matches!(cooling, AbilityState::Cooling { .. }),
        "{cooling:?}"
    );

    // Trade the cooling Camouflage away for the southern crate's tech…
    s.step(Input::Step(Direction::South));
    assert_eq!(s.exchange().map(|x| x.offered()), Some(OFFERED));
    s.step(Input::Discard(AbilityId::Camouflage));
    assert!(!s.loadout().contains(AbilityId::Camouflage));

    // …then find Camouflage again in the western crate and trade back for it.
    s.step(Input::Step(Direction::West));
    assert_eq!(
        s.exchange().map(|x| x.offered()),
        Some(AbilityId::Camouflage)
    );
    s.step(Input::Discard(OFFERED));
    assert!(s.loadout().contains(AbilityId::Camouflage), "found again");
    let regained = s.ability_state(AbilityId::Camouflage);
    assert!(
        matches!(regained, AbilityState::Cooling { .. }),
        "it came back still cooling, not reset: {regained:?}",
    );
}

/// **A traded loadout still round-trips through the level-seed token** (§12.4/#245).
///
/// The exchange is the first thing that can *shrink* a loadout mid-run, so the set a run
/// ends up holding is one no start-of-run draw would ever have produced. The token
/// encodes a subset of at most [`AbilityId::MAX_TECH_HELD`] pieces of tech, and a trade
/// leaves exactly that — which is the property worth pinning: a run that traded at a
/// crate is still a run that can be shared, reproduced and replayed.
#[test]
fn a_traded_loadout_still_encodes_as_a_level_seed() {
    for dropped in HELD {
        let mut s = offering();
        s.step(Input::Discard(dropped));
        let level = crate::LevelSeed {
            abilities: s.loadout(),
            ..crate::LevelSeed::quick_play(8371)
        };
        let token = level.encode().expect("a holdable loadout encodes");
        assert_eq!(
            crate::LevelSeed::decode(&token),
            Some(level),
            "the loadout left by trading {dropped:?} away did not round-trip",
        );
    }
}

/// **The usable line stops offering bumps while an offer is open** (§11.4): the row's
/// promise is *what the next press does*, and while the exchange is up the only presses
/// that do anything are its own.
#[test]
fn the_screen_says_how_to_answer() {
    // A board as wide as the real one (§10.2), so the row is measured rather than
    // clipped by a test fixture narrower than any level.
    let held = HELD.into_iter().fold(Loadout::innate(), Loadout::with);
    let mut layout = open_room(40, 12);
    layout.place(Cell::new(5, 6), Terrain::EquipmentCache);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::South,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(held)
    .with_caches([OFFERED]);
    s.step(Input::Step(Direction::South));
    assert!(s.exchange().is_some());
    let width = s.layout().facility().width();
    let frame = crate::render_screen(&s, crate::ScreenUi::default());
    let row: String = (0..width).map(|x| frame.get(x, 1).glyph).collect();
    assert!(
        row.contains("drop one") && row.contains("decline"),
        "the usable line names both answers, got {row:?}",
    );
    assert!(
        !row.contains("cache:"),
        "and stops offering the bump that got us here, got {row:?}",
    );
}
