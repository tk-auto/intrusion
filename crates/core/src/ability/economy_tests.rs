use super::*;

/// The §8.3 [START] catalog, pinned value by value: duration, cooldown,
/// targeting, and the declared effect. A retune of any number must be a
/// deliberate edit here, never a silent drift — and a moved number will move
/// the emergent lockout with it (§8.2).
#[test]
fn the_catalog_matches_the_design_activated() {
    for (id, cost, targeting, duration, cooldown, effect) in [
        (
            AbilityId::Run,
            1,
            TargetingMode::Itself,
            5,
            12,
            Effect::ExtraStep,
        ),
        (
            AbilityId::Camouflage,
            1,
            TargetingMode::Itself,
            10,
            20,
            Effect::ConcealWhileStill,
        ),
        (
            AbilityId::Decoy,
            1,
            TargetingMode::Direction,
            20,
            30,
            Effect::SpawnDecoy,
        ),
        (
            AbilityId::Dephase,
            1,
            TargetingMode::Itself,
            // 4 since #449 — three steps into a solid, counting the activation.
            4,
            30,
            Effect::Phase,
        ),
        (
            AbilityId::Autodoors,
            1,
            TargetingMode::Itself,
            16,
            40,
            Effect::AutoDoors,
        ),
        // Instant since #325 — the blast fires once and the guards carry the
        // time it bought, so there is no player-side window here to state.
        (
            AbilityId::Confusion,
            1,
            TargetingMode::Itself,
            0,
            45,
            Effect::Confuse,
        ),
        (
            AbilityId::Lockdown,
            1,
            TargetingMode::Itself,
            8,
            40,
            Effect::SealDoors,
        ),
    ] {
        let def = id.def();
        let economy = def
            .economy()
            .unwrap_or_else(|| panic!("{} is an activated ability", id.name()));
        assert_eq!(def.id(), id);
        assert_eq!(economy.cost(), cost, "{}", id.name());
        assert_eq!(economy.targeting(), targeting, "{}", id.name());
        assert_eq!(economy.duration(), duration, "{}", id.name());
        assert_eq!(economy.cooldown(), cooldown, "{}", id.name());
        match def.behaviour() {
            Behaviour::Effects(effects) => {
                assert_eq!(effects, &[effect][..], "{}", id.name())
            }
            Behaviour::Coded => panic!("{} should be data-driven", id.name()),
        }
    }
}

/// The **coded** catalog (§8.1's escape hatch, #303), pinned separately because
/// it is the arm the other pin cannot reach: Pierce Wall declares no effects at
/// all, so its row is `cost 1`, self-targeted, **instant** (`duration: 0`), with
/// **no cooldown** and a per-level budget instead — the scarcity is the budget,
/// not the clock (§8.2/#302). Every number here is [START].
#[test]
fn the_catalog_matches_the_design_coded() {
    let def = AbilityId::PierceWall.def();
    let economy = def.economy().expect("Pierce Wall is activated");
    assert_eq!(economy.cost(), 1, "activation costs the turn (§4.4)");
    assert_eq!(economy.targeting(), TargetingMode::Itself);
    assert_eq!(economy.duration(), 0, "instant — no window to manage");
    assert_eq!(
        economy.cooldown(),
        0,
        "no clock: the budget is the scarcity"
    );
    assert_eq!(def.uses_per_level(), Some(PIERCE_WALL_USES));
    assert_eq!(PIERCE_WALL_USES, 3, "[START]");
    assert!(
        matches!(def.behaviour(), Behaviour::Coded),
        "turning a solid into floor is not a primitive (§8.1)",
    );
    assert_eq!(AbilityId::PierceWall.name(), "Pierce Wall");
    assert_eq!(
        AbilityId::PierceWall.bar_name(),
        "Bore",
        "§11.4 fits 5 cells"
    );
    assert_eq!(AbilityId::PierceWall.script_letter(), 'b');
}

/// The **Drone**'s row (§8.1's escape hatch, #273), the second coded ability and the
/// one the design names when it reserves the hatch. Every number here is [START].
///
/// The single duration is the load-bearing part: it covers **both** halves of the
/// ability — the turns spent flying and the turns the machine hovers after the player
/// hands the controls back — so there is one number to read and the §11.4 bar's `[N]`
/// means *turns of machine*, start to finish.
#[test]
fn the_catalog_matches_the_design_drone() {
    let def = AbilityId::Drone.def();
    let economy = def.economy().expect("the Drone is activated");
    assert_eq!(economy.cost(), 1, "activation costs the turn (§4.4)");
    assert_eq!(
        economy.targeting(),
        TargetingMode::Itself,
        "you launch it from your own cell"
    );
    assert_eq!(economy.duration(), 30, "[START] — flying *and* hovering");
    assert_eq!(economy.cooldown(), 40, "[START]");
    assert_eq!(
        economy.duration() + economy.cooldown(),
        70,
        "the longest lockout in the catalogue (§8.2), for the strongest information tool",
    );
    assert_eq!(
        def.uses_per_level(),
        None,
        "the clock is the whole economy here (§8.2)"
    );
    assert!(
        matches!(def.behaviour(), Behaviour::Coded),
        "transferring control is not a primitive the effect vocabulary has (§8.1)",
    );
    assert_eq!(AbilityId::Drone.name(), "Drone");
    assert_eq!(AbilityId::Drone.bar_name(), "Drone", "§11.4 fits 5 cells");
    assert_eq!(AbilityId::Drone.script_letter(), 'o');
}

/// **Every** activated ability is pinned by one of the catalog tests — the
/// guard against a row being added and quietly escaping the value-by-value pin,
/// which a hand-written list of tuples otherwise invites.
#[test]
fn every_activated_ability_is_pinned_by_a_catalog_test() {
    for id in AbilityId::ALL.into_iter().filter(|id| !id.is_passive()) {
        let pinned = match id.def().behaviour() {
            // The data rows are covered by `..._activated`, which walks a literal
            // list; a row missing from it fails here rather than silently.
            Behaviour::Effects(_) => PINNED_ACTIVATED.contains(&id),
            // The coded rows are covered one by one by `..._coded`.
            Behaviour::Coded => matches!(id, AbilityId::PierceWall | AbilityId::Drone),
        };
        assert!(pinned, "{} is in no catalog pin", id.name());
    }
}

/// The data-driven rows [`the_catalog_matches_the_design_activated`] walks — kept
/// beside it so the completeness check above reads off the same list.
const PINNED_ACTIVATED: [AbilityId; 7] = [
    AbilityId::Run,
    AbilityId::Camouflage,
    AbilityId::Decoy,
    AbilityId::Dephase,
    AbilityId::Autodoors,
    AbilityId::Confusion,
    AbilityId::Lockdown,
];

/// The **passive** catalog (#264/#265), pinned: Vision is the one passive, it
/// declares [`Effect::EnhancedSight`], and — the property that matters — it has
/// **no economy at all**. Not a zeroed one: `economy()` is `None`, so there is no
/// duration or cooldown for the deck to run and none for a future edit to set by
/// accident (§8.2's extension, #264).
#[test]
fn the_catalog_matches_the_design_passive() {
    let passives: Vec<AbilityId> = AbilityId::ALL
        .into_iter()
        .filter(|id| id.is_passive())
        .collect();
    assert_eq!(
        passives,
        vec![AbilityId::Vision, AbilityId::Saver],
        "the shipped passive set",
    );

    let def = AbilityId::Vision.def();
    assert!(def.is_passive());
    assert_eq!(def.mode(), AbilityMode::Passive);
    assert_eq!(def.economy(), None, "a passive spends no time (§8.2/#264)");
    assert_eq!(
        def.uses_per_level(),
        None,
        "the slot is Vision's whole price (§8.2/#264)",
    );
    match def.behaviour() {
        Behaviour::Effects(effects) => {
            assert_eq!(effects, &[Effect::EnhancedSight][..]);
        }
        Behaviour::Coded => panic!("Vision should be data-driven (§8.1)"),
    }
}

/// Every ability is exactly one of the two modes, and the two accessors agree —
/// [`Ability::is_passive`] is `true` precisely when there is no economy. A row
/// that claimed both (or neither) would leave the deck without a rule to run it
/// by; the type makes that unrepresentable and this pins that it stays so.
#[test]
fn every_ability_is_activated_or_passive_and_never_both() {
    for id in AbilityId::ALL {
        let def = id.def();
        assert_eq!(
            def.is_passive(),
            def.economy().is_none(),
            "{} disagrees with itself about its mode",
            id.name(),
        );
        match def.mode() {
            AbilityMode::Activated(economy) => {
                assert_eq!(def.economy(), Some(economy), "{}", id.name());
                assert_eq!(
                    economy.cost(),
                    1,
                    "{} costs the turn it is activated on (§4.4)",
                    id.name(),
                );
            }
            AbilityMode::Passive => assert!(def.is_passive(), "{}", id.name()),
        }
    }
}

/// [`AbilityId::ALL`] and [`AbilityId::index`] must agree — the deck indexes
/// slots by `index`, so a drift would alias two abilities onto one slot.
#[test]
fn all_and_index_agree() {
    for (i, id) in AbilityId::ALL.into_iter().enumerate() {
        assert_eq!(id.index(), i, "{}", id.name());
    }
}

/// The replay script's letter comes from the identity map (§12.4), reachable from
/// the id — the one spelling of an ability that is *not* a fact about the run it
/// is held in (#359).
#[test]
fn each_id_carries_its_script_letter() {
    assert_eq!(AbilityId::Run.script_letter(), 'r');
    assert_eq!(AbilityId::Camouflage.script_letter(), 'c');
    assert_eq!(AbilityId::Decoy.script_letter(), 'd');
    assert_eq!(AbilityId::Dephase.script_letter(), 'x');
    assert_eq!(AbilityId::Autodoors.script_letter(), 'a');
    assert_eq!(AbilityId::Confusion.script_letter(), 'z');
}

/// A fresh deck is available from the start (§8.3: the v1 set is), in whichever
/// way each ability *is* available — plain [`Ready`](AbilityState::Ready), or a
/// **full budget** for one that carries one (§8.2/#302), or [`Passive`] for one
/// that is simply on because it is held (#264). None of the three is a lockout,
/// which is the property this pins.
///
/// [`Passive`]: AbilityState::Passive
#[test]
fn a_fresh_deck_is_all_ready() {
    let deck = Deck::new(Loadout::full());
    for id in AbilityId::ALL {
        let expected = match id.def().mode() {
            AbilityMode::Passive => AbilityState::Passive,
            AbilityMode::Activated(_) => AbilityState::Ready,
        };
        // A budget shows through either mode (#243): what a fresh deck offers is the
        // full supply, not the mode's own resting state.
        let expected = match (expected, id.def().uses_per_level()) {
            (state, None) => state,
            (_, Some(uses)) => AbilityState::Limited { uses },
        };
        assert_eq!(deck.state(id), expected, "{}", id.name());
    }
}

/// [`Loadout::activated`] is exactly [`Loadout::full`] minus the passives — the
/// default a hand-built state boots with, so no test acquires a passive's
/// run-long perception change without asking for it (#264/#265).
#[test]
fn the_activated_loadout_is_the_full_one_without_passives() {
    let activated = Loadout::activated();
    for id in AbilityId::ALL {
        assert_eq!(activated.contains(id), !id.is_passive(), "{}", id.name());
    }
    assert_ne!(
        activated,
        Loadout::full(),
        "a passive ships, so they differ"
    );
}

/// Activation moves a Ready ability to Active for its **whole** duration — the
/// number the bar shows before the first end-of-turn tick (§8.2 timing).
#[test]
fn activation_sets_the_full_duration() {
    let mut deck = Deck::new(Loadout::full());
    assert!(deck.activate(AbilityId::Dephase));
    assert_eq!(
        deck.state(AbilityId::Dephase),
        AbilityState::Active { remaining: 4 },
        "the bar shows the full duration, not duration − 1",
    );
    // Re-activating an active ability is a free no-op — nothing changes.
    assert!(!deck.activate(AbilityId::Dephase));
    assert_eq!(
        deck.state(AbilityId::Dephase),
        AbilityState::Active { remaining: 4 }
    );
}

/// The §8.2 timing convention, at the economy level: an N-turn ability is
/// **Active for exactly N ticks including activation**, then flips to cooling —
/// so a freshly activated N yields N protected turns, the activation turn
/// covered. (Dephase, N = 4.)
#[test]
fn an_n_turn_ability_is_active_for_n_ticks_including_activation() {
    let mut deck = Deck::new(Loadout::full());
    deck.activate(AbilityId::Dephase); // the activation turn is protected turn 1
    let mut active_ticks = 1;
    loop {
        let mut expired = Vec::new();
        deck.tick(&mut expired);
        if matches!(deck.state(AbilityId::Dephase), AbilityState::Active { .. }) {
            active_ticks += 1;
        } else {
            // The tick that ended the duration reports it exactly once.
            assert_eq!(expired, vec![AbilityId::Dephase]);
            break;
        }
    }
    assert_eq!(active_ticks, 4, "N protected turns, activation included");
}

/// The full `duration + cooldown` lockout (§8.2), emergent from the rules:
/// Run (dur 5 / cd 12) is unusable for 5 + 12 = 17 ticks and Ready again on the
/// 18th, with the cooldown **frozen** for the whole duration (it never drains
/// while Active).
#[test]
fn the_lockout_is_duration_plus_cooldown() {
    let mut deck = Deck::new(Loadout::full());
    deck.activate(AbilityId::Run);

    let mut seen_active = 0;
    let mut seen_cooling = 0;
    for tick in 1..=17 {
        // Cooldown is frozen while active: the first 5 ticks are still Active,
        // and the cooling that follows starts at the *full* 12, never partway.
        match deck.state(AbilityId::Run) {
            AbilityState::Active { .. } => seen_active += 1,
            AbilityState::Cooling { remaining } => {
                seen_cooling += 1;
                if seen_cooling == 1 {
                    assert_eq!(remaining, 12, "cooldown was frozen through the duration");
                }
            }
            other => panic!("tick {tick}: still locked out, got {other:?}"),
        }
        let mut expired = Vec::new();
        deck.tick(&mut expired);
    }
    assert_eq!(seen_active, 5, "5 active turns");
    assert_eq!(seen_cooling, 12, "12 cooling turns");
    assert_eq!(
        deck.state(AbilityId::Run),
        AbilityState::Ready,
        "Ready again on the 18th turn — lockout is exactly duration + cooldown",
    );
}

/// Toggling off early is free and refunds nothing: the ability drops straight
/// into its **full** cooldown (§8.2 — cancelling saves you nothing).
#[test]
fn toggling_off_early_pays_the_full_cooldown() {
    let mut deck = Deck::new(Loadout::full());
    deck.activate(AbilityId::Camouflage); // dur 10 / cd 20
    let mut expired = Vec::new();
    deck.tick(&mut expired); // one turn of duration used (Active 10 → 9)
    assert_eq!(
        deck.state(AbilityId::Camouflage),
        AbilityState::Active { remaining: 9 }
    );
    assert!(deck.deactivate(AbilityId::Camouflage));
    assert_eq!(
        deck.state(AbilityId::Camouflage),
        AbilityState::Cooling { remaining: 20 },
        "early cancel still pays the whole cooldown",
    );
    // Toggling off a non-active ability is a no-op.
    assert!(!deck.deactivate(AbilityId::Run));
}

/// The **escape hatch** (§8.1): a `Coded` ability rides the *identical* economy.
/// The transitions read only the numbers ([`Slot::activated`]/[`Slot::ticked`]
/// take no [`Ability`]), so a coded ability with the same duration/cooldown
/// steps through activation, its active window, and cooldown exactly as a data
/// ability does — only its effect *application* (elsewhere) would differ.
#[test]
fn the_economy_is_blind_to_behaviour() {
    // A hypothetical coded ability whose behaviour the vocabulary can't express.
    const CODED: Ability = Ability {
        id: AbilityId::Run, // id is irrelevant to the economy; reuse one
        mode: activated(1, TargetingMode::Itself, 2, 3),
        uses: None,
        behaviour: Behaviour::Coded,
    };
    // A data ability with the *same* numbers steps identically.
    let data_duration = 2;
    let data_cooldown = 3;

    assert!(matches!(CODED.behaviour(), Behaviour::Coded));

    let coded_economy = CODED.economy().expect("the coded ability is activated");
    let coded = Slot::activated(coded_economy.duration(), coded_economy.cooldown());
    let data = Slot::activated(data_duration, data_cooldown);
    assert_eq!(coded, data, "activation ignores behaviour");

    // Walk both through the full lockout in lockstep.
    let (mut c, mut d) = (coded, data);
    for _ in 0..(2 + 3 + 1) {
        let (cn, _) = c.ticked(coded_economy.cooldown());
        let (dn, _) = d.ticked(data_cooldown);
        assert_eq!(cn, dn, "each tick ignores behaviour");
        c = cn;
        d = dn;
    }
    assert_eq!(c, Slot::Ready);
}

/// The loadout seam (#244): a deck built from a partial [`Loadout`] holds only
/// the granted abilities. An ability the run does not have reads as
/// [`Unusable`](AbilityState::Unusable) and refuses activation as a **free**
/// no-op (§4.4), while a held one activates normally — so a key press for an
/// ability you were not granted does nothing, exactly like bumping a wall.
#[test]
fn a_partial_loadout_holds_only_its_granted_abilities() {
    // Innate-only: Run is held, the tech is not.
    let mut deck = Deck::new(Loadout::innate());
    assert_eq!(
        deck.state(AbilityId::Run),
        AbilityState::Ready,
        "Run is held"
    );
    for tech in AbilityId::TECH {
        assert_eq!(
            deck.state(tech),
            AbilityState::Unusable,
            "{} is not in the loadout",
            tech.name(),
        );
        assert!(
            !deck.activate(tech),
            "{} cannot activate — a free no-op",
            tech.name(),
        );
        assert_eq!(deck.state(tech), AbilityState::Unusable, "still not yours");
    }
    // The held ability activates as usual.
    assert!(deck.activate(AbilityId::Run), "the held ability activates");
    assert!(matches!(
        deck.state(AbilityId::Run),
        AbilityState::Active { .. }
    ));
}

/// A **passive** is on because it is *held* (#264) — there is no activation to
/// perform and no toggle to pull. Both inputs are refused as **free** no-ops
/// (§4.4, exactly like pressing a key for an ability you weren't granted), and
/// the effect is in force the whole time regardless.
#[test]
fn a_passive_is_in_effect_because_it_is_held() {
    let mut deck = Deck::new(Loadout::innate().with(AbilityId::Vision));

    assert_eq!(deck.state(AbilityId::Vision), AbilityState::Passive);
    assert!(
        deck.effect_active(Effect::EnhancedSight),
        "held is on — no activation needed",
    );

    assert!(!deck.activate(AbilityId::Vision), "nothing to switch on");
    assert!(!deck.deactivate(AbilityId::Vision), "nothing to switch off");
    assert_eq!(
        deck.state(AbilityId::Vision),
        AbilityState::Passive,
        "neither input moved it",
    );
    assert!(deck.effect_active(Effect::EnhancedSight));
}

/// A passive the run does **not** hold is [`Unusable`](AbilityState::Unusable)
/// like any ungranted ability, and — the half that matters — its effect is not
/// in force. Holding it is the whole switch, so not holding it is the off state.
#[test]
fn a_passive_not_held_is_unusable_and_out_of_effect() {
    let deck = Deck::new(Loadout::innate());
    assert_eq!(deck.state(AbilityId::Vision), AbilityState::Unusable);
    assert!(!deck.effect_active(Effect::EnhancedSight));
}

/// A passive is **never stepped by the clock** (#264): ticking a deck that holds
/// one leaves it `Passive` forever and never reports it as expired, so it cannot
/// silently run out mid-run the way a duration does.
#[test]
fn a_passive_never_ticks_and_never_expires() {
    let mut deck = Deck::new(Loadout::full());
    for turn in 0..100 {
        let mut expired = Vec::new();
        deck.tick(&mut expired);
        assert_eq!(
            deck.state(AbilityId::Vision),
            AbilityState::Passive,
            "turn {turn}",
        );
        assert!(
            !expired.contains(&AbilityId::Vision),
            "turn {turn}: a passive has no duration to end",
        );
        assert!(deck.effect_active(Effect::EnhancedSight), "turn {turn}");
    }
}

/// **The activated economy is untouched by the passive's arrival** (#264). Every
/// activated ability, in a deck that also holds the passive, walks its exact
/// §8.2 lockout — `duration` turns active then `cooldown` turns cooling, Ready on
/// the turn after — with the passive ticking alongside and changing nothing.
#[test]
fn a_passive_in_the_deck_changes_no_activated_ability_timing() {
    for id in AbilityId::ALL.into_iter().filter(|id| !id.is_passive()) {
        let economy = id.def().economy().expect("activated");
        let (duration, cooldown) = (economy.duration(), economy.cooldown());

        let mut deck = Deck::new(Loadout::full());
        assert!(deck.activate(id), "{}", id.name());

        let mut active = 0;
        let mut cooling = 0;
        for _ in 0..(duration + cooldown) {
            match deck.state(id) {
                AbilityState::Active { .. } => active += 1,
                AbilityState::Cooling { .. } => cooling += 1,
                other => panic!("{}: locked out, got {other:?}", id.name()),
            }
            let mut expired = Vec::new();
            deck.tick(&mut expired);
        }
        assert_eq!(active, duration, "{} active turns", id.name());
        assert_eq!(cooling, cooldown, "{} cooling turns", id.name());
        // Available again — for a budgeted ability that means one use lighter
        // (§8.2/#302), which is the budget doing its job, not the clock failing.
        let expected = match id.def().uses_per_level() {
            Some(uses) => AbilityState::Limited { uses: uses - 1 },
            None => AbilityState::Ready,
        };
        assert_eq!(deck.state(id), expected, "{}", id.name());
    }
}

/// An **instant** ability (duration 0) has no active window: it activates
/// straight into its cooldown — the machinery the innate/instant abilities
/// (their own tickets) can lean on without a special case here.
#[test]
fn an_instant_ability_skips_straight_to_cooldown() {
    assert_eq!(Slot::activated(0, 4), Slot::Cooling { remaining: 4 });
    // Instant with no cooldown loops right back to Ready.
    assert_eq!(Slot::activated(0, 0), Slot::Ready);
}

// -----------------------------------------------------------------------
// The per-level use budget (§8.2/#302)
// -----------------------------------------------------------------------

/// A deck in which `id` carries a per-level budget of `uses` — byte for byte the
/// runtime a catalog row declaring [`Ability::uses_per_level`] produces, seeded
/// here by hand because **no shipping row declares one yet**: #302 lands the axis
/// and #303 is the ability that spends it. Everything else is exactly
/// [`Deck::new`]'s deck, so these tests exercise the real activate/tick/state
/// paths rather than a parallel model of them.
fn deck_budgeting(loadout: Loadout, id: AbilityId, uses: u32) -> Deck {
    let mut deck = Deck::new(loadout);
    deck.uses[id.index()] = Some(uses);
    deck
}

/// Run one full lockout so a budgeted ability is ready to be used again.
fn wait_out_the_lockout(deck: &mut Deck, id: AbilityId) {
    let economy = id.def().economy().expect("activated");
    for _ in 0..(economy.duration() + economy.cooldown()) {
        deck.tick(&mut Vec::new());
    }
}

/// A fresh deck seeds **every** budget from the catalog, and only from there
/// (§8.2/#302). This is the level-start boot path — [`Deck::new`] is called once
/// per level and nowhere else — so "set at level start" is a property of where
/// this code lives, and the assertion is that the seeding reads the row rather
/// than any number written down twice.
#[test]
fn a_fresh_deck_seeds_every_use_budget_from_the_catalog() {
    let deck = Deck::new(Loadout::full());
    for id in AbilityId::ALL {
        assert_eq!(
            deck.uses_left(id),
            id.def().uses_per_level(),
            "{}",
            id.name(),
        );
    }
}

/// **Uses deplete and never come back** (§8.2/#302's fence). Each use costs one,
/// the last one leaves the ability [`Exhausted`](AbilityState::Exhausted), and no
/// amount of time moves it off that: a hundred turns of ticking is a whole level
/// of waiting, and there is nothing to wait for.
#[test]
fn uses_deplete_and_never_recharge_across_a_level() {
    let id = AbilityId::Dephase; // dur 3 / cd 30
    let mut deck = deck_budgeting(Loadout::full(), id, 2);

    assert_eq!(deck.state(id), AbilityState::Limited { uses: 2 });
    assert!(deck.activate(id));
    assert_eq!(deck.uses_left(id), Some(1), "one use spent");
    wait_out_the_lockout(&mut deck, id);
    assert_eq!(
        deck.state(id),
        AbilityState::Limited { uses: 1 },
        "off cooldown, and the budget is what is left to report",
    );

    assert!(deck.activate(id), "the last use is a use like any other");
    assert_eq!(deck.uses_left(id), Some(0));
    wait_out_the_lockout(&mut deck, id);

    assert_eq!(deck.state(id), AbilityState::Exhausted);
    for turn in 0..100 {
        deck.tick(&mut Vec::new());
        assert_eq!(deck.state(id), AbilityState::Exhausted, "turn {turn}");
        assert_eq!(deck.uses_left(id), Some(0), "turn {turn}");
        assert!(!deck.activate(id), "turn {turn}: nothing to activate");
    }
}

/// **A use is spent only when the ability actually switches on** (§8.2/#302).
/// Every refusal the deck can give — not held, already cooling, already spent —
/// leaves the count exactly where it was, so a mis-pressed key never costs a use
/// any more than it costs a turn (§4.4).
#[test]
fn a_refused_activation_consumes_no_use() {
    let id = AbilityId::Dephase;

    // Refused for want of the ability itself: the run does not hold it.
    let mut ungranted = deck_budgeting(Loadout::innate(), id, 3);
    assert!(!ungranted.activate(id));
    assert_eq!(ungranted.uses_left(id), Some(3), "not yours, and not spent");

    // Refused because it is mid-lockout: one use bought the window, and
    // hammering the key through it buys nothing more.
    let mut deck = deck_budgeting(Loadout::full(), id, 3);
    assert!(deck.activate(id));
    assert_eq!(deck.uses_left(id), Some(2));
    for _ in 0..5 {
        assert!(!deck.activate(id), "already running");
        assert_eq!(deck.uses_left(id), Some(2), "a refused press costs nothing");
    }

    // Refused because the budget is gone: the count cannot go below zero, and
    // pressing again does not try to.
    let mut spent = deck_budgeting(Loadout::full(), id, 1);
    assert!(spent.activate(id));
    wait_out_the_lockout(&mut spent, id);
    assert!(!spent.activate(id));
    assert_eq!(
        spent.uses_left(id),
        Some(0),
        "no underflow, no second spend"
    );
}

/// **The two lockouts coexist without contradicting each other** (§8.2/#302).
/// While the clock runs it is the clock that is reported — it is the nearer gate,
/// and it is true. The moment the clock clears, the budget takes over. A spent
/// budget outranks the *cooldown*, because a cooldown on an ability that is never
/// coming back would be a countdown to nothing — but never the **duration**: the
/// window your last use bought is still running, and hiding its clock would be
/// the one lie §8.2's timing rule names.
#[test]
fn a_cooldown_and_a_budget_report_the_nearer_gate() {
    let id = AbilityId::Dephase; // dur 4 / cd 30
    let mut deck = deck_budgeting(Loadout::full(), id, 2);

    assert!(deck.activate(id));
    assert_eq!(deck.state(id), AbilityState::Active { remaining: 4 });
    for _ in 0..4 {
        deck.tick(&mut Vec::new());
    }
    assert_eq!(
        deck.state(id),
        AbilityState::Cooling { remaining: 30 },
        "the clock leads while it runs — the budget is not the wait",
    );
    for _ in 0..30 {
        deck.tick(&mut Vec::new());
    }
    assert_eq!(
        deck.state(id),
        AbilityState::Limited { uses: 1 },
        "clock clear, so the budget is what stands between you and the next use",
    );

    // The last use. It spends the budget to zero the instant it is pressed — but
    // the window it bought is running, and that is what the player is playing
    // off, so the duration keeps the entry for as long as it lasts.
    assert!(deck.activate(id));
    assert_eq!(deck.uses_left(id), Some(0));
    assert_eq!(
        deck.state(id),
        AbilityState::Active { remaining: 4 },
        "a spent budget never hides the window it just bought",
    );
    for _ in 0..4 {
        deck.tick(&mut Vec::new());
    }
    assert_eq!(
        deck.state(id),
        AbilityState::Exhausted,
        "spent outranks the cooldown: there is no use left for it to lead to",
    );
}

/// A **fresh level** restores the budget (§8.2/#302): the only thing that ever
/// gives one back is a new deck, and a new deck is what a new level builds.
/// Nothing inside a level can reach this.
#[test]
fn a_fresh_level_restores_the_budget() {
    let id = AbilityId::Dephase;
    let mut deck = deck_budgeting(Loadout::full(), id, 1);
    assert!(deck.activate(id));
    wait_out_the_lockout(&mut deck, id);
    assert_eq!(deck.state(id), AbilityState::Exhausted);

    // The next facility is a new deck off the same catalog row.
    let next = deck_budgeting(Loadout::full(), id, 1);
    assert_eq!(next.state(id), AbilityState::Limited { uses: 1 });
}

/// **An ability with no budget behaves exactly as it did before #302.** Every
/// shipping row is one of these, so this is the compatibility statement: the
/// states are the clock's alone, `uses_left` is `None`, and no number of
/// activations ever exhausts anything.
#[test]
fn an_unbudgeted_ability_is_untouched_by_the_axis() {
    let unbudgeted = AbilityId::ALL
        .into_iter()
        .filter(|id| id.def().economy().is_some() && id.def().uses_per_level().is_none());
    for id in unbudgeted {
        let mut deck = Deck::new(Loadout::full());
        assert_eq!(deck.uses_left(id), None, "{}", id.name());
        assert_eq!(deck.state(id), AbilityState::Ready, "{}", id.name());
        for _ in 0..3 {
            assert!(deck.activate(id), "{}", id.name());
            wait_out_the_lockout(&mut deck, id);
            assert_eq!(
                deck.state(id),
                AbilityState::Ready,
                "{} is never Limited and never Exhausted",
                id.name(),
            );
            assert_eq!(deck.uses_left(id), None, "{}", id.name());
        }
    }
}

// -----------------------------------------------------------------------
// A budgeted **passive** (§8.2/#302 × #264, the pair #243 needed)
// -----------------------------------------------------------------------

/// The catalog row: the Saver is a passive that also carries a per-level budget, and
/// the two axes are declared in different places on purpose — the mode says there is
/// no clock, [`Ability::uses_per_level`] says how much of the level there is. Neither
/// is inside the other, which is what makes this combination expressible at all.
#[test]
fn the_saver_is_a_passive_with_a_budget() {
    let def = AbilityId::Saver.def();
    assert!(def.is_passive(), "held is on (§8.2/#264)");
    assert_eq!(
        def.economy(),
        None,
        "no turn cost, no duration, no cooldown"
    );
    assert_eq!(def.uses_per_level(), Some(SAVER_USES));
    assert_eq!(SAVER_USES, 1, "[START] — one capture a facility (§4.5)");
    match def.behaviour() {
        Behaviour::Effects(effects) => {
            assert_eq!(effects, &[Effect::ReverseCapture][..]);
        }
        Behaviour::Coded => panic!("turning a capture over is a vocabulary row (§8.1)"),
    }
    assert_eq!(AbilityId::Saver.bar_name(), "Saver", "§11.4 fits 5 cells");
}

/// **A budgeted passive reads its budget, not its passivity.** `(on)` is the right
/// answer only for a passive nothing can use up; this one can be, so the bar shows
/// what is left and then that there is none — and `Exhausted` is where it stops,
/// never `Ready` and never a cooldown (§8.2's fence).
#[test]
fn a_budgeted_passive_reads_limited_then_exhausted() {
    let mut deck = Deck::new(Loadout::full());
    assert_eq!(
        deck.state(AbilityId::Saver),
        AbilityState::Limited { uses: SAVER_USES },
    );
    assert_eq!(
        deck.state(AbilityId::Vision),
        AbilityState::Passive,
        "an unbudgeted passive is unchanged — nothing counts it down",
    );

    assert!(
        deck.spend_effect(Effect::ReverseCapture),
        "it was in effect"
    );
    assert_eq!(deck.state(AbilityId::Saver), AbilityState::Exhausted);
    assert_eq!(deck.uses_left(AbilityId::Saver), Some(0));
}

/// **Spent is off.** The one property a budgeted passive must have that an activated
/// budgeted ability does not: it has no window to be inside, so an empty budget stops
/// the effect itself. The bar's `Exhausted` and the world's behaviour are the same
/// fact, so no caller can read one without the other.
#[test]
fn a_spent_passive_is_no_longer_in_effect() {
    let mut deck = Deck::new(Loadout::full());
    assert!(deck.effect_active(Effect::ReverseCapture));

    assert!(deck.spend_effect(Effect::ReverseCapture));
    assert!(
        !deck.effect_active(Effect::ReverseCapture),
        "a spent save is not quietly still working",
    );
    assert!(
        !deck.spend_effect(Effect::ReverseCapture),
        "and there is nothing left to spend",
    );
}

/// The budget follows the **loadout**, like every other permission (#244): an ability
/// the run does not hold is not in effect, so its supply is never reachable — and
/// holding it is what makes it spendable, mid-level pickup included (#209).
#[test]
fn a_saver_the_run_does_not_hold_can_never_fire() {
    let mut deck = Deck::new(Loadout::innate());
    assert_eq!(deck.state(AbilityId::Saver), AbilityState::Unusable);
    assert!(
        !deck.spend_effect(Effect::ReverseCapture),
        "not yours to spend"
    );
    assert_eq!(
        deck.uses_left(AbilityId::Saver),
        Some(SAVER_USES),
        "and the untouched supply is still sitting in the deck",
    );

    deck.grant(AbilityId::Saver);
    assert_eq!(
        deck.state(AbilityId::Saver),
        AbilityState::Limited { uses: SAVER_USES },
        "salvaged tech arrives with the whole level's supply (§8.3/#209)",
    );
    assert!(deck.spend_effect(Effect::ReverseCapture));
}

/// A passive is still un-pressable and un-toggleable with a budget on it (#264): the
/// two verbs the deck offers are for abilities that have an activation moment, and a
/// budget does not give one. A press must not burn the save.
#[test]
fn a_budgeted_passive_still_answers_to_no_key() {
    let mut deck = Deck::new(Loadout::full());
    assert!(
        !deck.activate(AbilityId::Saver),
        "there is nothing to switch on"
    );
    assert!(!deck.deactivate(AbilityId::Saver), "or off");
    assert_eq!(
        deck.uses_left(AbilityId::Saver),
        Some(SAVER_USES),
        "a mis-input is free (§4.4) — it must not cost the level's save",
    );
}
