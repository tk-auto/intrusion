//! What the campaign layer owes the run (§14 v3 / §2.2): the sequence, what carries
//! between its facilities, the four transitions, and determinism across the lot.
//!
//! The played tests here boot **real generated facilities** and raid them to
//! extraction rather than asserting over hand-built fixtures, because the one thing
//! this layer can get wrong invisibly is the seam to the level below it: a config that
//! does not carve, a gate that refuses the way out, a second facility that is secretly
//! the first.

use super::*;
use crate::alert::TOP_RUNG;
use crate::cell::{Cell, Direction};
use crate::facility::Terrain;
use crate::guard::GuardState;
use crate::level_seed::start_level;
use crate::modifiers::{LevelModifiers, ModifierDirection, ModifierSources};
use crate::path::first_step_toward;
use crate::place::LevelConfig;
use crate::render::render;
use crate::state::{Input, State};
use crate::verdict::EndExit;

/// A seed whose facilities are raided and left without the raider walking into a
/// guard — the played tests want a *completed* campaign, and which seeds give one is
/// an artefact of generation, not a property under test.
const PLAYED_SEED: u64 = 8371;

/// The wander every played raid makes before turning for home: enough turns inside the
/// facility that the run is a run and not a step in and straight back out, and few
/// enough that it is not a stealth test in disguise.
const WANDER: [Direction; 6] = [
    Direction::South,
    Direction::South,
    Direction::East,
    Direction::East,
    Direction::North,
    Direction::West,
];

/// A finished raid: the inputs it pressed, in order, and the state they left.
struct Raid {
    inputs: Vec<Input>,
    state: State,
}

/// Play one facility of a campaign to extraction, recording every input pressed.
///
/// Crawl out of the tunnel, wander, walk back to `E`, and step off the board (§4.5) —
/// the shape of every real raid, driven deterministically so a run of it is a
/// **function of the level alone**. The recorded inputs are what makes the §12.4
/// assertion an honest one: the second pass replays *them*, not this routine.
fn raid(level: &LevelSeed) -> Raid {
    let mut state = start_level(level).expect("a campaign facility carves");
    let mut inputs = Vec::new();
    let mut press = |state: &mut State, input: Input| {
        inputs.push(input);
        state.step(input);
    };

    // Out of the tunnel the run starts in (§4.5/#466): crawl to the mouth, then climb
    // out onto the floor beside it.
    let tunnel: Vec<Cell> = state
        .layout()
        .exit_duct()
        .expect("a generated facility has the player's own tunnel")
        .cells()
        .to_vec();
    let mouth = tunnel[0];
    for pair in tunnel.windows(2).rev() {
        let dir = Direction::between(pair[1], pair[0]).expect("the tunnel is contiguous");
        press(&mut state, Input::Step(dir));
    }
    let out = Direction::between(mouth, tunnel[1])
        .expect("the tunnel is contiguous")
        .opposite();
    press(&mut state, Input::Step(out));

    for dir in WANDER {
        press(&mut state, Input::Step(dir));
    }

    // Home. The route is the plain §10.3 player routing, so a closed door on it is
    // bumped open and stepped through — two presses at the same cell, which the loop
    // makes no special case of.
    for _ in 0..400 {
        if state.in_duct() || state.outcome() != Outcome::Playing {
            break;
        }
        let facility = state.layout().facility();
        let passable = |cell: Cell| facility.terrain(cell).is_some_and(Terrain::routes_player);
        let Some(dir) = first_step_toward(state.player(), mouth, passable) else {
            break;
        };
        press(&mut state, Input::Step(dir));
    }

    // And out: crawl back along the tunnel and step off the board.
    for _ in 0..64 {
        if state.outcome() != Outcome::Playing {
            break;
        }
        let at = state.player();
        let i = tunnel
            .iter()
            .position(|&c| c == at)
            .expect("the way home ends on the tunnel");
        let dir = if i + 1 < tunnel.len() {
            Direction::between(at, tunnel[i + 1])
        } else {
            // On the border cell: the last step is off the grid, so it is named by the
            // one before it rather than by a neighbour (§4.5).
            Direction::between(tunnel[i - 1], at)
        }
        .expect("the tunnel is contiguous");
        press(&mut state, Input::Step(dir));
    }

    Raid { inputs, state }
}

/// Replay a recorded raid on a freshly booted level — the §12.4 half of [`raid`].
fn replay(level: &LevelSeed, inputs: &[Input]) -> State {
    let mut state = start_level(level).expect("a campaign facility carves");
    for &input in inputs {
        state.step(input);
    }
    state
}

/// A verdict as a finished raid would hand one back, without playing one — the fixture
/// for the transitions, whose subject is what the campaign does with a verdict rather
/// than how one is reached.
///
/// `held` is what the raid walked out **holding** (#266), and it is a parameter rather
/// than a default because the campaign *assigns* it: a raid that found and traded
/// nothing still reports the set it carried in, and a fixture that reported the empty
/// set would be a raid claiming to have dropped everything.
fn extracted_holding(held: Loadout, intel: usize, alert_peak: u32) -> Verdict {
    Verdict {
        ending: Ending::Escaped,
        stats: RunStats {
            turns: 40,
            intel,
            intel_total: 3,
            alert_peak,
            held,
            ..RunStats::default()
        },
    }
}

/// The same, for a run that is carrying nothing but the innate set — every campaign
/// starts there (§8.3), so it is what most of these transitions want.
fn extracted(intel: usize, alert_peak: u32) -> Verdict {
    extracted_holding(Loadout::innate(), intel, alert_peak)
}

/// A capture, anywhere, by anyone.
fn captured() -> Verdict {
    Verdict {
        ending: Ending::Captured {
            guard: 0,
            state: GuardState::Chasing,
            at: Cell::new(4, 4),
        },
        stats: RunStats::default(),
    }
}

/// Complete the current raid and walk on: bank a modest haul, then take the **first
/// open offer** the map makes.
///
/// The choice is deliberately the dullest possible one — these tests are about the
/// transitions and what carries across them, and *which* successor is taken is the
/// map's own business, tested in [`map`](super::map).
///
/// The raid ends at **condition 1** for the same reason: a raid that was noticed and
/// nothing more is the one loudness the campaign alert carries nothing out of (#210), so
/// these tests keep measuring the transitions rather than the alert's mapping — which
/// has tests of its own, here and in [`loudness`](super::loudness).
fn walk_on(run: &mut Campaign, intel: usize) {
    // The raid walks out holding what it walked in with: these tests are about the
    // transitions, not about the tech axis, so nothing is found and nothing traded.
    let verdict = extracted_holding(run.loadout(), intel, 1);
    run.complete(&verdict);
    if run.stage() == CampaignStage::Choosing {
        let next = run.offers()[0].node;
        assert!(run.choose(next), "an offered node is takeable");
    }
}

/// **A run starts with nothing** (§2.2): the innate verbs, an empty wallet, a facility
/// that has not noticed anybody — and it starts standing on the first facility of the
/// country, with the raid ahead of it.
#[test]
fn a_fresh_run_carries_nothing_it_did_not_walk_in_with() {
    let run = Campaign::new(11);
    assert_eq!(run.stage(), CampaignStage::Approach);
    assert_eq!(run.outcome(), Outcome::Playing);
    assert_eq!(run.path(), [run.map().start()]);
    assert_eq!(run.node().depth(), 0);
    assert_eq!(run.map().depth(), DEPTH_TO_ARCHIVE);
    assert_eq!(run.intel(), 0);
    assert_eq!(run.alert(), 0);
    assert_eq!(
        run.loadout(),
        Loadout::innate(),
        "salvaged tech is found within a run, never brought into one (§2.2)",
    );
}

/// **Every facility's seed comes from the run seed** (§12.4) — never a fresh source.
///
/// Three properties, and the golden values pin all three at once: the derivation is
/// stable, distinct per node, and narrow enough that every campaign facility is a
/// **sayable level** — a token you can hand to the sim or play by hand (§13.1).
#[test]
fn every_facility_derives_its_seed_from_the_run_seed() {
    let golden: Vec<u64> = (0..4)
        .map(|lane| facility_seed(8371, NodeId::at(0, lane)))
        .collect();
    assert_eq!(
        golden,
        vec![63_589, 128_467, 124_555, 97_859],
        "the derivation moved — every campaign ever seeded is a different run",
    );

    // Stable, and a different run seed is a different campaign.
    assert_eq!(facility_seed(8371, NodeId::at(0, 2)), golden[2]);
    assert_ne!(facility_seed(8372, NodeId::at(0, 2)), golden[2]);

    for depth in 0..=DEPTH_TO_ARCHIVE {
        for lane in 0..map::LANES {
            let seed = facility_seed(4242, NodeId::at(depth, lane));
            assert_eq!(
                LevelSeed::quick_play(seed).encode().map(|t| t.len()),
                Some(crate::level_seed::TOKEN_LEN),
                "the facility at ({depth}, {lane}) must fit the level-seed token",
            );
        }
    }
}

/// **Entering hands out the facility and starts the raid**, once: a second ask while a
/// raid is under way would mint a second `State` for one facility, and two answers to
/// how it went.
#[test]
fn entering_starts_the_raid_and_does_not_start_it_twice() {
    let mut run = Campaign::new(7);
    let node = run.node();
    let level = run.enter().expect("the first facility is ahead");
    assert_eq!(run.stage(), CampaignStage::Inside);
    assert_eq!(run.enter(), None, "a raid is already under way");

    assert_eq!(level.seed, facility_seed(7, node));
    assert_eq!(
        level.modifiers.intel_to_exit,
        IntelGate::None,
        "intel is currency in the campaign, not an exit key (§4.5/§2.2)",
    );
    assert_eq!(level.abilities, run.loadout(), "the run's own loadout");
    assert_eq!(
        level,
        run.next_level(),
        "the config is readable without starting the raid",
    );
}

/// **What the map offered is what the run walks into** (§2.3/§14 v3). The offer's
/// flavour resolves into the facility's modifiers through the §12.6 seam, so the row
/// that said *Vault* and the building with the extra console are one statement.
///
/// It also **travels**: the flavour rides in the level-seed token's modifier slots, so
/// a campaign facility handed to someone else is the facility as it was played (§12.7).
#[test]
fn the_facility_is_the_flavour_the_map_offered() {
    // Walk the country until a run has stood on one of each offered flavour, and check
    // each facility against its own flavour's contribution.
    let mut seen: Vec<Flavour> = Vec::new();
    for seed in 0..40 {
        let mut run = Campaign::new(seed);
        while run.stage() != CampaignStage::Won {
            let flavour = run.flavour();
            let level = run.enter().expect("a facility to raid");
            let expected = ModifierSources {
                chosen: LevelModifiers {
                    intel_to_exit: IntelGate::None,
                    ..LevelModifiers::default()
                },
                alert: None,
                flavour: Some(flavour.modifiers()),
            }
            .resolve();
            assert_eq!(level.modifiers, expected, "{flavour:?} was not honoured");
            assert_eq!(
                LevelSeed::decode(&level.encode().expect("a campaign facility is sayable")),
                Some(level),
                "{flavour:?} must survive the token round-trip (§12.7)",
            );
            if !seen.contains(&flavour) {
                seen.push(flavour);
            }
            walk_on(&mut run, 1);
        }
    }
    for flavour in Flavour::OFFERED.into_iter().chain([Flavour::Archive]) {
        assert!(seen.contains(&flavour), "{flavour:?} was never played");
    }
}

/// **A Vault really is richer and a Vault really is watched** (§2.3's anti-facade rule).
/// The flavours differ where they say they differ — in the facility that gets built —
/// rather than only in the word on the map row.
#[test]
fn the_flavours_build_different_facilities() {
    let recipe = |flavour: Flavour| {
        let modifiers = flavour.modifiers();
        let config = LevelConfig::V1
            .with_guard_count(modifiers.guard_count)
            .with_intel_count(modifiers.intel_count);
        (config.guards, config.intel)
    };
    let (base_guards, base_intel) = recipe(Flavour::Depot);
    assert_eq!(
        (base_guards, base_intel),
        (LevelConfig::V1.guards, LevelConfig::V1.intel),
        "a Depot is the §10.2 recipe, untouched",
    );
    assert_eq!(recipe(Flavour::Vault), (base_guards + 1, base_intel + 1));
    assert_eq!(recipe(Flavour::Outpost), (base_guards - 1, base_intel - 1));
    assert_eq!(
        recipe(Flavour::Archive).0,
        base_guards + 1,
        "the last raid is the hard one",
    );
    assert!(Flavour::Archive.modifiers().guards_always_search_hideouts);
}

/// **Every flavour actually carves.** The recipe arithmetic above is one thing; a
/// facility asking placement for four consoles and five guards on a 40×40 board is
/// another, and a campaign that hit `RetriesExhausted` on its richest node would be a
/// shipped bug nothing else here would catch (§10.6).
///
/// So: boot a **real** facility of each flavour and count what stands in it.
#[test]
fn every_flavour_carves_the_facility_it_promises() {
    for flavour in Flavour::OFFERED.into_iter().chain([Flavour::Archive]) {
        let modifiers = ModifierSources {
            chosen: LevelModifiers {
                intel_to_exit: IntelGate::None,
                ..LevelModifiers::default()
            },
            alert: None,
            flavour: Some(flavour.modifiers()),
        }
        .resolve();
        let expected = LevelConfig::V1
            .with_guard_count(modifiers.guard_count)
            .with_intel_count(modifiers.intel_count);
        for seed in 0..8 {
            let level = LevelSeed {
                seed,
                modifiers,
                abilities: Loadout::innate(),
            };
            let state = start_level(&level)
                .unwrap_or_else(|e| panic!("a {flavour:?} at seed {seed} must carve: {e:?}"));
            assert_eq!(state.guards().len(), expected.guards, "{flavour:?}");
            assert_eq!(
                state.objectives_remaining(),
                expected.intel,
                "{flavour:?} did not seat the consoles it promised",
            );
        }
    }
}

/// **Completing a raid banks the haul and puts the run at a choice point** — and the
/// facility it then walks to is a different one.
#[test]
fn completing_a_raid_banks_the_haul_and_offers_the_way_on() {
    let mut run = Campaign::to_depth(99, 3);
    let first = run.enter().expect("a facility to raid");

    assert_eq!(run.complete(&extracted(2, 1)), CampaignStage::Choosing);
    assert_eq!(run.intel(), 2, "the raid's consoles bank");
    assert_eq!(
        run.enter(),
        None,
        "a run at a choice point has not chosen yet",
    );

    let next = run.offers()[0].node;
    assert!(run.choose(next));
    assert_eq!(run.stage(), CampaignStage::Approach);
    assert_eq!(run.node(), next);
    let second = run.enter().expect("the chosen facility");
    assert_ne!(second.seed, first.seed, "a new facility, not the last one");

    // Intel accumulates across facilities — that is the whole of §2.2's currency row.
    run.complete(&extracted(1, 0));
    assert_eq!(run.intel(), 3);
}

/// **The run only ever moves at a choice point, and only along an edge the map offered**
/// (§2.2's forward-only arc, §14 v3's real edges). Everything else is refused with the
/// run left exactly where it was.
#[test]
fn the_run_moves_only_along_an_offer() {
    let mut run = Campaign::to_depth(4, 3);
    let start = run.node();
    assert!(
        run.offers().is_empty(),
        "there is nothing to choose before the first raid is done",
    );
    assert!(!run.choose(NodeId::at(1, 0)), "and nothing to choose it on");

    run.enter();
    assert!(
        run.offers().is_empty(),
        "nor from inside a facility (§2.2: no backtracking mid-raid)",
    );
    run.complete(&extracted(1, 0));

    let offers = run.offers();
    let locked = offers.iter().find(|o| o.locked).expect("a locked edge");
    assert!(
        !run.choose(locked.node),
        "the intel-locked edge is inert until #212 opens it",
    );
    assert!(
        !run.choose(NodeId::at(2, 0)),
        "and a node the map never offered is not reachable by asking",
    );
    assert!(
        !run.choose(start),
        "least of all the facility just emptied (§2.2: no backtracking)",
    );
    assert_eq!(run.node(), start, "every refusal left the run where it was");
    assert_eq!(run.stage(), CampaignStage::Choosing);
}

/// A run that has raided one facility of `seed`'s country and left it at `condition`,
/// standing at the choice point with the noise of that raid on the ground ahead.
fn after_a_raid_at(seed: u64, condition: u32) -> Campaign {
    let mut run = Campaign::to_depth(seed, 3);
    run.enter().expect("the first facility");
    run.complete(&extracted(1, condition));
    assert_eq!(run.stage(), CampaignStage::Choosing);
    run
}

/// How many of a resolved set's modifiers bend each way — the §2.3 directional
/// assertion is stated in these two counts, exactly as the difficulty axis states its
/// own.
fn by_direction(modifiers: LevelModifiers) -> (usize, usize) {
    let active = modifiers.active();
    let count = |want| active.iter().filter(|m| m.direction == want).count();
    (
        count(ModifierDirection::Harder),
        count(ModifierDirection::Easier),
    )
}

/// **The campaign alert is the last raid's condition, replaced and never added to**
/// (§14 v3/#210) — the relief valve §2.2 asks for, held by the shape of the field
/// rather than by a decay rate.
#[test]
fn the_alert_is_the_last_raids_condition_and_nothing_older() {
    let mut run = Campaign::to_depth(66, 3);
    assert_eq!(run.alert(), 0);
    assert_eq!(
        run.loudness(),
        None,
        "a run that has raided nothing is not a run that slipped through unnoticed",
    );

    run.enter();
    run.complete(&extracted(1, TOP_RUNG));
    assert_eq!(run.alert(), TOP_RUNG);
    assert_eq!(run.loudness(), Some(Loudness::Hunted));

    // Walk on and leave the next facility quietly: the alert comes all the way back
    // down, because what carries is the last raid's noise and nothing older. A campaign
    // that added instead would have no way back from the top rung (§2.2).
    let node = run.offers()[0].node;
    assert!(run.choose(node));
    run.enter();
    run.complete(&extracted(1, 0));
    assert_eq!(run.alert(), 0);
    assert_eq!(run.loudness(), Some(Loudness::Unnoticed));

    // A capture banks nothing, the alert included — there is no later facility for it
    // to reach.
    let mut lost = Campaign::to_depth(66, 3);
    lost.enter();
    lost.complete(&captured());
    assert_eq!(lost.alert(), 0);
}

/// **How far a raid's noise carries** (§14 v3/#210): one open road ahead at condition
/// 2, every one of them at condition 3, one the *other* way after a raid nobody
/// noticed, and none at condition 1.
///
/// It is stated over a spread of countries because the shape of a choice point is the
/// seed's to decide — two open edges against the side of the map, three in the middle,
/// and the intel-locked one either way.
#[test]
fn a_loud_raid_alerts_the_roads_ahead_and_a_ghost_raid_eases_one() {
    for seed in [1, 66, 8371, 123_456] {
        for (condition, wanted) in [
            (0, Some(ModifierDirection::Easier)),
            (1, None),
            (ALERTS_ONE, Some(ModifierDirection::Harder)),
            (ALERTS_ALL, Some(ModifierDirection::Harder)),
        ] {
            let run = after_a_raid_at(seed, condition);
            let offers = run.offers();
            let open: Vec<&Offer> = offers.iter().filter(|offer| !offer.locked).collect();
            let reached: Vec<&&Offer> = open
                .iter()
                .filter(|offer| run.alert_reaches(offer.node).is_some())
                .collect();
            let expected = match (wanted, condition) {
                (None, _) => 0,
                (Some(_), c) if c >= ALERTS_ALL => open.len(),
                (Some(_), _) => 1,
            };
            assert_eq!(
                reached.len(),
                expected,
                "seed {seed} at condition {condition}",
            );
            for offer in &reached {
                assert_eq!(run.alert_reaches(offer.node), wanted);
            }
            // The alternative route is never the road the noise *settled on* — at
            // condition 2 the play is finding the unwatched one, and it must not cost
            // intel (#212). At condition 3 it is swept with everything else: what the top
            // of the ladder takes away is the route around it, and a road you bought is
            // still a road ahead.
            for offer in offers.iter().filter(|offer| offer.locked) {
                let on_the_lock = if condition >= ALERTS_ALL {
                    wanted
                } else {
                    None
                };
                assert_eq!(
                    run.alert_reaches(offer.node),
                    on_the_lock,
                    "seed {seed} at condition {condition}",
                );
            }
        }
    }
}

/// **The §2.3 bite check, and the loop §14 v3 exists for**: a louder raid on facility
/// *k* yields a harder facility *k+1* than a quiet raid does, from the same run seed
/// and the same choice path.
///
/// Two runs of one country, differing in **nothing** but how loudly the first raid
/// ended, walking to the same node. The facility the loud run walks into carries a rule
/// the quiet one's does not, and that rule is one the pool documents *harder* — so the
/// alert is not a number on a screen, it is the building.
#[test]
fn a_louder_raid_makes_the_next_facility_harder() {
    let mut bit_somewhere = false;
    for seed in [1, 66, 8371, 123_456] {
        let loud = after_a_raid_at(seed, ALERTS_ALL);
        let quiet = after_a_raid_at(seed, 1);
        for offer in quiet.offers().into_iter().filter(|offer| !offer.locked) {
            let (mut loud, mut quiet) = (loud.clone(), quiet.clone());
            assert!(loud.choose(offer.node) && quiet.choose(offer.node));
            let loud = loud.enter().expect("the facility ahead");
            let quiet = quiet.enter().expect("the facility ahead");

            // Same country, same node: the *building* is the same one either way, and
            // only the rules bending it differ (§12.4 — the alert draws from its own
            // stream and cannot move a facility's seed).
            assert_eq!(loud.seed, quiet.seed, "seed {seed}");

            let (harder_loud, easier_loud) = by_direction(loud.modifiers);
            let (harder_quiet, easier_quiet) = by_direction(quiet.modifiers);
            assert!(
                harder_loud >= harder_quiet,
                "seed {seed}: a loud raid made the next facility easier",
            );
            assert_eq!(
                easier_loud, easier_quiet,
                "seed {seed}: a loud raid bent a rule the player's way",
            );
            bit_somewhere |= harder_loud > harder_quiet;

            // The witness: whatever the alert drew is *active* in the facility the run
            // walked into, whether or not the flavour had already asked for something
            // like it. This is the half that can never be true by luck.
            let country = quiet_map(seed);
            let drawn = Loudness::Hunted
                .contribution(country, country.start(), offer.node)
                .expect("condition 3 reaches every open road");
            for rule in drawn.active() {
                assert!(
                    loud.modifiers.active().contains(&rule),
                    "seed {seed}: the alert drew {} and the facility does not play it",
                    rule.name,
                );
            }
        }
    }
    assert!(
        bit_somewhere,
        "the alert never once added a rule the flavour had not already asked for",
    );
}

/// The country a [`after_a_raid_at`] run raids — its map, for a test that wants to ask
/// the mapping directly.
fn quiet_map(seed: u64) -> FacilityMap {
    FacilityMap::to_depth(seed, 3)
}

/// **The noise reaches one hop and no further** (§14 v3/#210): being loud in facility 2
/// makes facility 3 harder, and it says nothing whatever about facility 4.
///
/// The counterweight to the spiral §2.2 warns against, stated as the property that
/// makes it impossible: after a quiet raid the facilities ahead are byte for byte the
/// ones a run that had never been loud would walk into.
#[test]
fn the_noise_does_not_outlive_the_hop_it_was_made_on() {
    let mut loud = after_a_raid_at(8371, ALERTS_ALL);
    let mut quiet = after_a_raid_at(8371, 1);
    let first = quiet.offers()[0].node;
    assert!(loud.choose(first) && quiet.choose(first));

    // The second facility differs — that is the loop closing.
    assert_ne!(loud.next_level(), quiet.next_level());

    // Both now raid it and leave at the same condition. From here on the two runs are
    // the same run: the noise of the *first* raid is gone, and nothing about it can
    // still be reaching the third facility.
    loud.enter();
    quiet.enter();
    loud.complete(&extracted(1, 1));
    quiet.complete(&extracted(1, 1));
    let second = quiet.offers()[0].node;
    assert!(loud.choose(second) && quiet.choose(second));
    assert_eq!(
        loud.next_level(),
        quiet.next_level(),
        "a raid two facilities back is still bending the run",
    );
}

/// **Capture is terminal for the run** (§2.2), at any facility: no retry, no snapshot,
/// nothing banked — there is no later facility to spend it in.
#[test]
fn capture_ends_the_whole_run() {
    for ending in [
        captured().ending,
        Ending::Entombed {
            at: Cell::new(2, 2),
        },
    ] {
        let mut run = Campaign::to_depth(5, 4);
        run.enter();
        walk_on(&mut run, 2);
        let reached = run.node();
        run.enter();
        let verdict = Verdict {
            ending,
            stats: RunStats {
                intel: 3,
                ..RunStats::default()
            },
        };

        assert_eq!(run.complete(&verdict), CampaignStage::Lost);
        assert_eq!(run.outcome(), Outcome::Lost);
        assert!(run.stage().is_over());
        assert_eq!(run.enter(), None, "a lost run has no next facility");
        assert!(run.offers().is_empty(), "and nowhere left to go");
        assert_eq!(run.node(), reached, "it got no further than it got");
        assert_eq!(run.intel(), 2, "the last raid banked nothing");
    }
}

/// **Reaching the archive and leaving it wins the run.** What the archive holds and what
/// arriving there concludes are #217's; that the graph *ends* at a distinguished node,
/// and that leaving it ends the run won, is this layer's.
#[test]
fn leaving_the_archive_wins_the_run() {
    let mut run = Campaign::to_depth(3, 2);
    run.enter();
    assert_eq!(run.complete(&extracted(1, 0)), CampaignStage::Choosing);
    let next = run.offers()[0].node;
    run.choose(next);
    run.enter();
    assert_eq!(run.complete(&extracted(1, 0)), CampaignStage::Choosing);

    let last = run.offers();
    assert_eq!(last.len(), 1, "every route converges on the archive");
    assert_eq!(last[0].flavour, Flavour::Archive);
    assert_eq!(last[0].node, run.map().archive());
    run.choose(last[0].node);
    run.enter();
    assert_eq!(run.complete(&extracted(1, 0)), CampaignStage::Won);

    assert_eq!(run.outcome(), Outcome::Won);
    assert!(run.stage().is_over());
    assert!(run.offers().is_empty(), "there is nothing past the archive");
    assert_eq!(run.enter(), None);
}

/// **Salvaged tech rides into the next facility** (§2.2) — the accumulation the
/// campaign exists for, and the seam an equipment cache writes (#209).
#[test]
fn salvaged_tech_rides_into_the_next_facility() {
    let mut run = Campaign::to_depth(21, 3);
    let first = run.enter().expect("a facility to raid");
    assert!(!first.abilities.contains(AbilityId::Dephase));

    run.salvage(AbilityId::Dephase);
    walk_on(&mut run, 1);
    let second = run.enter().expect("the next facility");
    assert!(
        second.abilities.contains(AbilityId::Dephase),
        "what the run salvaged, the run carries",
    );
    assert!(
        second.abilities.contains(AbilityId::Run),
        "and the innate set is still under it (§8.3)",
    );

    // Nothing carries *across* runs (§2.2): a fresh campaign at the same seed holds
    // the innate set and nothing else.
    assert_eq!(Campaign::new(21).loadout(), Loadout::innate());
}

/// **A raid that *traded* carries the set it walked out with** (§2.2/§8.3/#266) — the
/// half the accumulation could not express while a loadout could only grow.
///
/// The campaign takes the raid's held set outright rather than folding its finds in, so
/// the tech given up at a crate is gone from the next facility too. Fold-the-finds would
/// have carried the dropped ability forward and quietly undone the choice.
#[test]
fn a_raid_that_traded_tech_carries_the_trade_forward() {
    let mut run = Campaign::to_depth(21, 3);
    run.salvage(AbilityId::Camouflage);
    run.enter().expect("a facility to raid");

    // The raid found Lockdown in a crate and gave up Camouflage for it.
    let mut verdict = extracted_holding(run.loadout(), 1, 0);
    verdict.stats.salvaged = Loadout::empty().with(AbilityId::Lockdown);
    verdict.stats.held = verdict
        .stats
        .held
        .without(AbilityId::Camouflage)
        .with(AbilityId::Lockdown);
    run.complete(&verdict);

    assert!(run.loadout().contains(AbilityId::Lockdown), "what it took");
    assert!(
        !run.loadout().contains(AbilityId::Camouflage),
        "and what it gave up stays given up — the choice survives the raid",
    );
    let next = run.offers()[0].node;
    run.choose(next);
    let level = run.enter().expect("the next facility");
    assert!(level.abilities.contains(AbilityId::Lockdown));
    assert!(!level.abilities.contains(AbilityId::Camouflage));
    assert!(
        level.abilities.contains(AbilityId::Run),
        "the innate set is under it either way (§8.3)",
    );
}

/// **A raid's find rides out on its verdict** (§2.2/#209) — the whole seam, end to
/// end: the campaign banks the salvage the same way it banks the intel, and the next
/// facility boots holding it.
///
/// This is the acceptance criterion "usable for the rest of the run" said at the layer
/// that owns the rest of the run. [`salvaged_tech_rides_into_the_next_facility`] pins
/// the same accumulation driven by hand; what this adds is that a *finished raid* is
/// what drives it, with nothing in between.
#[test]
fn a_completed_raid_banks_the_tech_it_salvaged() {
    let mut run = Campaign::to_depth(21, 3);
    run.enter().expect("a facility to raid");
    let mut verdict = extracted_holding(run.loadout(), 2, 0);
    verdict.stats.salvaged = Loadout::empty()
        .with(AbilityId::Confusion)
        .with(AbilityId::Decoy);
    // What the facility gave, and what the run therefore walks out holding — the two
    // halves a raid reports, and it is the second one the campaign takes (#266).
    verdict.stats.held = verdict
        .stats
        .held
        .with(AbilityId::Confusion)
        .with(AbilityId::Decoy);
    run.complete(&verdict);

    assert!(run.loadout().contains(AbilityId::Confusion), "the find");
    assert!(
        run.loadout().contains(AbilityId::Decoy),
        "and the second crate's — a facility may hide three (§14 v3)",
    );
    assert_eq!(run.intel(), 2, "and the haul, from the same value");
    let next = run.offers()[0].node;
    run.choose(next);
    assert!(
        run.enter()
            .expect("the next facility")
            .abilities
            .contains(AbilityId::Confusion),
        "the facility after boots holding what the one before handed over",
    );
}

/// **A captured run banks nothing, tech included** (§2.2). There is no later facility
/// to carry a find to, and a run that kept its salvage past a capture would be keeping
/// something past the run — the "start over stronger" §2.2 rules out.
#[test]
fn a_capture_carries_no_salvage_out_of_the_facility() {
    let mut run = Campaign::to_depth(21, 3);
    run.enter().expect("a facility to raid");
    let mut verdict = captured();
    verdict.stats.salvaged = Loadout::empty().with(AbilityId::Confusion);
    assert_eq!(run.complete(&verdict), CampaignStage::Lost);
    assert_eq!(run.loadout(), Loadout::innate(), "the run is over");
}

/// **A Workshop is the facility its offer said it was** (§2.3/§14 v3/#209): the flavour
/// the map names resolves into the level-seed the run walks into, crate and all — and
/// it *pays* for it, one console fewer, so the offer is a trade rather than a bonus.
#[test]
fn each_flavour_hides_the_crates_its_row_promises() {
    use crate::modifiers::{CacheCount, IntelCount};

    // The ladder the map is a choice over (§14 v3/#209): richer flavours hide more, and
    // each pays for its crates in a different currency.
    for (flavour, caches) in [
        (Flavour::Outpost, CacheCount::None),
        (Flavour::Depot, CacheCount::One),
        (Flavour::Workshop, CacheCount::Two),
        (Flavour::Vault, CacheCount::Three),
        (Flavour::Archive, CacheCount::None),
    ] {
        assert_eq!(flavour.modifiers().caches, caches, "{flavour:?}");
    }
    // …and the Workshop's price is a console, where the Vault's is a guard: the two
    // rich flavours differ in what they charge, which is what makes the choice between
    // them a decision rather than a ranking (§2.3).
    assert_eq!(
        Flavour::Workshop.modifiers().intel_count,
        IntelCount::Fewer,
        "the console a Workshop's crates cost",
    );
    assert_eq!(
        Flavour::Vault.modifiers().guard_count,
        crate::modifiers::GuardCount::More,
        "the guard a Vault's crates cost",
    );
    // And it reaches the facility through the one seam (§12.6): walk a run until it
    // stands on a Workshop, and the level-seed it would boot carries the crate — which
    // is what makes the campaign facility's *token* the facility as it was played
    // (§12.7), rather than a label the map printed.
    let mut run = Campaign::new(PLAYED_SEED);
    let mut stood_on_one = false;
    while !run.stage().is_over() {
        let flavour = run.flavour();
        assert_eq!(
            run.next_level().modifiers.caches,
            flavour.modifiers().caches,
            "the config disagrees with the row that offered it",
        );
        stood_on_one |= flavour == Flavour::Workshop;
        run.enter();
        walk_on(&mut run, 1);
    }
    assert!(
        stood_on_one,
        "a run that never meets a Workshop cannot test one — pick another seed",
    );
}

/// **Intel is spent through the wallet and nowhere else** (§2.2/§14 v3/#211): the balance
/// is what raids banked minus what the hub took, a refused spend leaves both the balance
/// and the run untouched, and the two refusals are told apart.
#[test]
fn the_hub_debits_the_wallet_and_a_refusal_costs_nothing() {
    let mut run = Campaign::to_depth(99, 3);
    assert_eq!(run.intel(), 0, "a run banks nothing before it raids");

    run.enter();
    run.complete(&extracted(4, 1));
    assert_eq!(run.intel(), 4);

    // A price the run cannot meet is refused, by name, with nothing taken.
    assert_eq!(
        run.spend(9),
        Outlay::Short {
            cost: 9,
            balance: 4
        },
    );
    assert_eq!(run.intel(), 4, "a refused spend is not a partial payment");
    assert!(!run.affords(9) && run.affords(4));

    // One it can meet is paid, and the balance is what is left to spend on the next
    // thing — the currency row of §2.2's table, both halves of it.
    assert_eq!(
        run.spend(3),
        Outlay::Paid {
            cost: 3,
            balance: 1
        },
    );
    assert_eq!(run.intel(), 1);

    // And the next raid tops it up: the wallet accumulates across facilities.
    let next = run.offers()[0].node;
    assert!(run.choose(next));
    run.enter();
    walk_on(&mut run, 2);
    assert_eq!(run.intel(), 3, "banked on top of what was not spent");
}

/// **There is no in-level spending** (§14 v3/#211/appendix 47). The hub is the map between
/// facilities, so a spend from inside a raid — or after the run is over — is refused as
/// *closed* rather than as unaffordable, and takes nothing however rich the run is.
#[test]
fn intel_is_spendable_only_at_the_map() {
    let mut run = Campaign::to_depth(99, 3);
    run.enter();
    run.complete(&extracted(9, 1));

    // At a choice point: open.
    assert_eq!(run.stage(), CampaignStage::Choosing);
    assert!(run.spend(1).paid());

    // Standing on the next facility, not yet inside it: still the map, still open — the
    // approach is a hub stage too, which is what lets a run buy the ground it is about to
    // walk onto.
    let next = run.offers()[0].node;
    assert!(run.choose(next));
    assert_eq!(run.stage(), CampaignStage::Approach);
    assert!(run.spend(1).paid());

    // Inside: closed, and the balance says so.
    let before = run.intel();
    run.enter();
    assert_eq!(run.stage(), CampaignStage::Inside);
    assert_eq!(run.spend(1), Outlay::Closed);
    assert_eq!(run.intel(), before, "a raid cannot dip into the wallet");

    // And once the run is over there is nothing left to spend on.
    run.complete(&captured());
    assert!(run.stage().is_over());
    assert_eq!(run.spend(1), Outlay::Closed);
    assert_eq!(run.intel(), before);
}

/// The locked edge on offer from where the run stands — every choice point has exactly
/// one (§14 v3), and it is what #212's sink is priced against.
fn locked_offer(run: &Campaign) -> NodeId {
    let locked: Vec<Offer> = run.offers().into_iter().filter(|o| o.locked).collect();
    assert_eq!(locked.len(), 1, "a choice point offers one locked edge");
    locked[0].node
}

/// Bank enough for `n` route unlocks by walking a raid out with a fat haul.
fn bank_for_unlocks(run: &mut Campaign, n: u32) {
    run.enter();
    let haul = (ROUTE_UNLOCK_COST * n) as usize;
    run.complete(&extracted(haul, 1));
    assert_eq!(run.stage(), CampaignStage::Choosing);
}

/// **Intel buys ground** (§14 v3's first sink, #212): paying [`ROUTE_UNLOCK_COST`] at a
/// choice point turns the map's intel-locked successor into an ordinary offer the run may
/// take — and the wallet is debited exactly once for it.
#[test]
fn buying_the_alternative_route_makes_it_takeable() {
    let mut run = Campaign::to_depth(99, 3);
    bank_for_unlocks(&mut run, 1);
    let locked = locked_offer(&run);
    assert!(!run.is_unlocked(locked));
    assert!(!run.choose(locked), "an unbought road is refused (§14 v3)");

    assert_eq!(
        run.unlock(locked),
        Outlay::Paid {
            cost: ROUTE_UNLOCK_COST,
            balance: 0,
        },
    );
    assert!(run.is_unlocked(locked));

    // It is an ordinary offer now, flavour and all — the purchase bought ground, not a
    // facility chosen for the run.
    let bought = run
        .offers()
        .into_iter()
        .find(|o| o.node == locked)
        .expect("the bought road is still offered");
    assert!(!bought.locked);
    assert_eq!(bought.flavour, run.map().flavour(locked));

    // And it reaches a lane two across, which no open edge from here could (§14 v3).
    let from = run.node();
    assert_eq!(locked.lane().abs_diff(from.lane()), 2);
    assert_eq!(locked.depth(), from.depth() + 1);

    assert!(run.choose(locked), "the run may now walk down it");
    assert_eq!(run.node(), locked);
}

/// **A run that cannot pay does not get the road** (#211's spend seam, #212's price), and
/// is told which fact it is: too poor is not the same as nothing to buy.
#[test]
fn an_unaffordable_route_is_refused_and_costs_nothing() {
    let mut run = Campaign::to_depth(99, 3);
    run.enter();
    run.complete(&extracted((ROUTE_UNLOCK_COST - 1) as usize, 1));
    let locked = locked_offer(&run);

    assert_eq!(
        run.unlock(locked),
        Outlay::Short {
            cost: ROUTE_UNLOCK_COST,
            balance: ROUTE_UNLOCK_COST - 1,
        },
    );
    assert_eq!(run.intel(), ROUTE_UNLOCK_COST - 1, "nothing was taken");
    assert!(!run.is_unlocked(locked));
    assert!(!run.choose(locked), "and the road is still shut");
}

/// **Only a locked road on offer can be bought.** An open successor, a facility elsewhere
/// in the country, a road already paid for, and any call made away from a choice point are
/// all refused as *nothing to buy here* — with the wallet untouched, however rich the run.
#[test]
fn only_a_locked_offer_is_for_sale() {
    let mut run = Campaign::to_depth(99, 3);
    bank_for_unlocks(&mut run, 3);
    let rich = run.intel();
    let locked = locked_offer(&run);

    // An open successor is not for sale: it is already takeable, and charging for it
    // would be selling the player something they own.
    let open = run.offers()[0].node;
    assert!(!run.offers()[0].locked);
    assert_eq!(run.unlock(open), Outlay::Closed);

    // Nor is a facility the map is not offering from here.
    assert_eq!(run.unlock(run.map().archive()), Outlay::Closed);
    assert_eq!(run.intel(), rich, "no refusal charged anything");

    // Bought once, it is not for sale again — the second press is not a second charge.
    assert!(run.unlock(locked).paid());
    let after = run.intel();
    assert_eq!(run.unlock(locked), Outlay::Closed);
    assert_eq!(run.intel(), after);

    // And there is nothing to buy anywhere but a choice point: on the approach the map
    // offers only the facility under the run's feet, and inside a raid it offers nothing.
    assert!(run.choose(open));
    assert_eq!(run.stage(), CampaignStage::Approach);
    assert_eq!(run.unlock(locked), Outlay::Closed);
    run.enter();
    assert_eq!(run.unlock(locked), Outlay::Closed);
    assert_eq!(run.intel(), after);
}

/// **The bought road is alerted at the top of the ladder and only there** (§14 v3/#210).
///
/// Condition 3's escalation is *breadth* — what it takes away is the route around it — so
/// an alternative route would be intel buying immunity from the alert if the noise skipped
/// it. At condition 2 it is never the marked road: finding the unwatched one is the play
/// there, and it must not cost intel.
#[test]
fn the_alternative_route_is_alerted_only_when_every_road_is() {
    for seed in 0..12 {
        for condition in [ALERTS_ONE, ALERTS_ALL] {
            let mut run = Campaign::to_depth(seed, 3);
            run.enter();
            run.complete(&extracted(ROUTE_UNLOCK_COST as usize, condition));
            let locked = locked_offer(&run);
            let reached = run.alert_reaches(locked).is_some();
            assert_eq!(
                reached,
                condition == ALERTS_ALL,
                "seed {seed} at condition {condition}",
            );

            // Buying it changes nothing about that: the line says the same thing before
            // and after the money changes hands.
            assert!(run.unlock(locked).paid());
            assert_eq!(run.alert_reaches(locked).is_some(), reached);
        }
    }
}

/// **The price is one knob** (§14 v3 **[START]**), and it is pinned here so a change to it
/// is a visible change rather than a number that drifted.
#[test]
fn the_route_costs_one_start_number() {
    assert_eq!(ROUTE_UNLOCK_COST, 1);

    // **Well under a facility's whole haul** at the §10.2 recipe, and that is the point:
    // the map draws unbought ground as `?`, so what is being sold is a road nobody has
    // seen. A price has to be proportionate to what the buyer knows (appendix 48).
    let recipe = LevelConfig::V1.intel as u32;
    assert!(
        ROUTE_UNLOCK_COST < recipe,
        "a road bought blind must not cost a facility's raid",
    );

    // It still asks for something real: nothing is banked until a raid is walked out of,
    // so the **first** choice point of every run cannot afford one.
    let mut fresh = Campaign::to_depth(99, 3);
    fresh.enter();
    fresh.complete(&extracted(0, 1));
    assert!(!fresh.affords(ROUTE_UNLOCK_COST));
    assert_eq!(
        fresh.unlock(locked_offer(&fresh)),
        Outlay::Short {
            cost: ROUTE_UNLOCK_COST,
            balance: 0,
        },
    );

    let mut run = Campaign::to_depth(99, 3);
    bank_for_unlocks(&mut run, 1);
    let locked = locked_offer(&run);
    assert_eq!(run.intel(), ROUTE_UNLOCK_COST);
    assert!(run.affords(ROUTE_UNLOCK_COST));
    assert!(run.unlock(locked).paid());
    assert_eq!(run.intel(), 0, "exactly the price, and no more");
}

/// **Determinism holds across purchases** (§12.4): the same run seed and the same sequence
/// of choices *and spends* grows the same country, unlocks the same roads and boots the
/// same facilities. Buying a road is an input, not a new source.
#[test]
fn a_run_that_buys_a_route_replays_identically() {
    let walk = |seed: u64| -> Vec<(NodeId, u64, u32)> {
        let mut run = Campaign::to_depth(seed, 3);
        let mut trail = Vec::new();
        for _ in 0..3 {
            let level = run.enter().expect("a facility to raid");
            trail.push((run.node(), level.seed, run.intel()));
            run.complete(&extracted(ROUTE_UNLOCK_COST as usize, 1));
            if run.stage() != CampaignStage::Choosing {
                break;
            }
            // Buy the alternative route wherever there is one to buy and the wallet can
            // cover it — the last hop before the archive offers none, and that is the
            // graph converging rather than a case to special-case.
            let bought = run
                .offers()
                .into_iter()
                .find(|offer| offer.locked)
                .filter(|offer| run.unlock(offer.node).paid())
                .map(|offer| offer.node);
            let next = bought.unwrap_or_else(|| run.offers()[0].node);
            assert!(run.choose(next));
        }
        trail
    };
    for seed in [1, 42, 8371] {
        assert_eq!(walk(seed), walk(seed), "seed {seed}");
    }
}

/// **A campaign facility's exit never refuses** (§4.5/§14 v3): intel is currency, so every
/// console in the building is *surplus* and extraction is voluntary — a run may walk out
/// of any facility the turn it walked in.
///
/// The gate is stated on the config every facility boots with, whatever the run is
/// carrying, so this is the model rather than a property of one lucky seed.
#[test]
fn every_campaign_facility_lets_the_run_leave_empty_handed() {
    let mut run = Campaign::to_depth(PLAYED_SEED, 3);
    for _ in 0..=3 {
        assert_eq!(
            run.next_level().modifiers.intel_to_exit,
            IntelGate::None,
            "the campaign exit is not an intel gate (§4.5)",
        );
        run.enter();
        // Nothing taken, and the run still walks on: an empty-handed raid is *allowed*,
        // and what makes it a bad idea is that the run is now poorer at a facility the
        // alert may have made harder — not a penalty anyone coded (appendix 47).
        walk_on(&mut run, 0);
    }
    assert_eq!(run.intel(), 0, "nothing taken is nothing banked");
}

/// **The campaign offers no way to play the run again** (§2.2/appendix 31). The gate
/// has been in the code since the end screen shipped; this is the first thing that
/// actually stands behind it.
#[test]
fn the_campaign_offers_no_way_to_play_the_run_again() {
    let options = Campaign::new(1).run_options();
    assert_eq!(options.mode, RunMode::Campaign);
    assert_eq!(options.mode.exits(), &[EndExit::Menu]);
}

/// **A depth-zero campaign is the game v1 already ships**: the start node *is* the
/// archive, so it is enter, raid, leave, and the run is over — a strict superset of the
/// turn loop, not a rewrite of it.
#[test]
fn a_one_facility_campaign_is_a_single_raid() {
    let mut run = Campaign::to_depth(PLAYED_SEED, 0);
    assert_eq!(run.node(), run.map().archive());
    assert_eq!(run.flavour(), Flavour::Archive);
    let level = run.enter().expect("the one facility");
    let played = raid(&level);

    assert_eq!(
        played.state.outcome(),
        Outcome::Won,
        "the raider got out (§4.5)",
    );
    assert!(played.state.turn() > 0, "and played the facility to do it",);
    assert_eq!(
        run.complete(&played.state.verdict().expect("the raid ended")),
        CampaignStage::Won,
    );
}

/// **A whole run reproduces from its seed and its choices** (§12.4), across more than
/// one facility — the property every golden test above the level rests on, and the one
/// the player never gets to use (§2.2: the run is one-shot).
///
/// On a graph the replay's input is wider than it was on a list: `(run seed, [choices],
/// [inputs])`. That is exactly §12.4's promise about a campaign — the path is a function
/// of the player's inputs, so the run is a function of the seed and the inputs — and the
/// second pass here replays the *recorded* choices and inputs rather than re-deriving
/// either.
#[test]
fn a_two_facility_run_replays_byte_for_byte() {
    let mut first = Campaign::to_depth(PLAYED_SEED, 1);
    let mut script: Vec<(NodeId, LevelSeed, Vec<Input>)> = Vec::new();
    let mut grids: Vec<Vec<String>> = Vec::new();

    while let Some(level) = first.enter() {
        let node = first.node();
        let played = raid(&level);
        assert_eq!(
            played.state.outcome(),
            Outcome::Won,
            "the facility at depth {} was raided and left",
            node.depth(),
        );
        grids.push(render(&played.state).to_text());
        script.push((node, level, played.inputs));
        first.complete(&played.state.verdict().expect("the raid ended"));
        if first.stage() == CampaignStage::Choosing {
            let next = first.offers()[0].node;
            first.choose(next);
        }
    }
    assert_eq!(script.len(), 2, "both facilities were played");
    assert_eq!(first.stage(), CampaignStage::Won);
    assert_ne!(
        script[0].1.seed, script[1].1.seed,
        "two facilities, not the same one twice",
    );

    // Same run seed, same choices, same inputs — same run, from the campaign's carried
    // state down to the last glyph of each facility's final frame.
    let mut second = Campaign::to_depth(PLAYED_SEED, 1);
    for (facility, (node, level, inputs)) in script.iter().enumerate() {
        assert_eq!(
            second.node(),
            *node,
            "facility {facility} is a different one"
        );
        assert_eq!(second.enter().as_ref(), Some(level), "facility {facility}");
        let state = replay(level, inputs);
        assert_eq!(
            render(&state).to_text(),
            grids[facility],
            "facility {facility} replayed to a different board",
        );
        second.complete(&state.verdict().expect("the raid ended"));
        // Replay the *recorded* choice rather than picking one again: what is under
        // test is that the same choices grow the same graph, not that the map hands
        // back a stable first row.
        if let Some((next, _, _)) = script.get(facility + 1) {
            assert!(
                second.choose(*next),
                "the recorded choice is still an offer"
            );
        }
    }
    assert_eq!(second, first, "the whole run, carried state and all");
}

/// Bank enough for `n` scouts by walking a raid out with a fat haul.
fn bank_for_scouts(run: &mut Campaign, n: u32) {
    run.enter();
    run.complete(&extracted((SCOUT_COST * n) as usize, 1));
    assert_eq!(run.stage(), CampaignStage::Choosing);
}

/// The first facility on offer that is not behind a locked road, and can take the rule.
fn scoutable_offer(run: &Campaign) -> NodeId {
    run.ahead()
        .into_iter()
        .find(|offer| run.scoutable(offer.node))
        .expect("a choice point offers a facility that can be scouted")
        .node
}

/// **Intel buys a plan of the building** (§14 v3's pre-level sink, #215): paying
/// [`SCOUT_COST`] at a choice point marks the facility scouted, debits the wallet exactly
/// once, and — once the run walks in — the facility it boots carries the rule.
#[test]
fn scouting_a_facility_puts_its_contents_in_the_run_it_boots() {
    let mut run = Campaign::to_depth(99, 3);
    bank_for_scouts(&mut run, 1);
    let node = scoutable_offer(&run);
    assert!(!run.is_scouted(node));
    assert!(
        !run.level_at(node, false).modifiers.scouted,
        "precondition: unbought, the facility hides its contents (§11.5a)",
    );

    assert_eq!(
        run.scout(node),
        Outlay::Paid {
            cost: SCOUT_COST,
            balance: 0,
        },
    );
    assert!(run.is_scouted(node));

    // Buying does not commit the run: it is still choosing, and still standing where it
    // was — which is what makes the scout a purchase made *before* the choice (#207).
    assert_eq!(run.stage(), CampaignStage::Choosing);
    assert_ne!(run.node(), node);

    assert!(run.choose(node));
    assert!(
        run.next_level().modifiers.scouted,
        "the facility the run walks into is the one it paid to know",
    );
}

/// **A scout the run cannot pay for is refused and costs nothing** (#211's spend seam),
/// and the refusal says which fact it is: too poor is not the same as nothing to buy.
#[test]
fn an_unaffordable_scout_is_refused_and_costs_nothing() {
    let mut run = Campaign::to_depth(99, 3);
    run.enter();
    run.complete(&extracted((SCOUT_COST - 1) as usize, 1));
    let node = run
        .ahead()
        .into_iter()
        .find(|offer| !offer.locked)
        .expect("an open road")
        .node;

    assert_eq!(
        run.scout(node),
        Outlay::Short {
            cost: SCOUT_COST,
            balance: SCOUT_COST - 1,
        },
    );
    assert_eq!(run.intel(), SCOUT_COST - 1, "nothing was taken");
    assert!(!run.is_scouted(node));
    // The row is still *offered*, unlike one the token has no room for: a price you
    // cannot meet today is one you can save up for.
    assert!(run.scoutable(node));
}

/// **Only a facility the run may walk into is for sale.** A locked road it has not bought,
/// a facility elsewhere in the country, one already scouted, and any call made from inside
/// a raid are all refused as *nothing to buy here* — with the wallet untouched, however
/// rich the run.
#[test]
fn only_a_facility_on_offer_can_be_scouted() {
    let mut run = Campaign::to_depth(99, 3);
    bank_for_scouts(&mut run, 3);
    let rich = run.intel();

    let locked = locked_offer(&run);
    assert_eq!(run.scout(locked), Outlay::Closed, "an unbought road");
    let elsewhere = NodeId::at(0, 0);
    assert_eq!(run.scout(elsewhere), Outlay::Closed, "somewhere else");
    assert_eq!(run.intel(), rich, "nothing was taken for either");

    let node = scoutable_offer(&run);
    assert!(run.scout(node).paid());
    assert_eq!(
        run.scout(node),
        Outlay::Closed,
        "a plan you already hold is not something to sell twice",
    );
    assert_eq!(run.intel(), rich - SCOUT_COST, "and it cost one scout");

    // No spending inside a facility (§14 v3): the run is in a building, not at the hub.
    assert!(run.choose(node));
    run.enter();
    let inside = run
        .map()
        .successors(run.node())
        .first()
        .expect("a node ahead")
        .node;
    assert_eq!(run.scout(inside), Outlay::Closed);
    assert_eq!(run.intel(), rich - SCOUT_COST);
}

/// **A scout is bought for a facility, not for the run** (#215): the intel spent on a road
/// the run then declines is spent, and the facility it takes instead is as fogged as it
/// ever was. That is the sink's teeth — scouting the road you are not sure of is exactly
/// the purchase that can be wrong.
#[test]
fn a_scout_does_not_follow_the_run_onto_another_road() {
    let mut run = Campaign::to_depth(99, 3);
    bank_for_scouts(&mut run, 1);
    let open: Vec<NodeId> = run
        .ahead()
        .into_iter()
        .filter(|offer| !offer.locked)
        .map(|offer| offer.node)
        .collect();
    assert!(open.len() > 1, "a choice point offers more than one road");

    assert!(run.scout(open[0]).paid());
    assert_eq!(run.intel(), 0, "the intel is gone");

    assert!(run.choose(open[1]), "the run takes the other road anyway");
    assert!(
        !run.next_level().modifiers.scouted,
        "the facility it took is not the one it paid to know",
    );
    assert!(
        run.is_scouted(open[0]),
        "and the plan it bought is still its"
    );
}

/// **A facility with no room left in its level-seed token is never offered the rule**
/// (#215/§12.7). Selling it would hand the player a facility that cannot be written down,
/// shared or replayed — so the hub refuses the sale outright rather than taking the intel
/// and quietly dropping the token.
#[test]
fn a_facility_that_cannot_carry_the_rule_is_not_for_sale() {
    let mut run = Campaign::to_depth(99, 3);
    bank_for_scouts(&mut run, 3);
    let rich = run.intel();
    for offer in run.ahead() {
        let sayable = run.level_at(offer.node, true).is_sayable();
        assert_eq!(
            run.scoutable(offer.node),
            !offer.locked && sayable,
            "{:?}: a facility is for sale exactly when it is open and can carry the rule",
            offer.node,
        );
        if !sayable {
            assert_eq!(run.scout(offer.node), Outlay::Closed);
            assert_eq!(run.intel(), rich, "and nothing was taken for the refusal");
        }
    }
}

/// **Determinism (§12.4)**: the same run seed and the same spends boot the same facility,
/// down to the board — a scout changes what the player *knows*, and nothing about what the
/// building is.
#[test]
fn the_same_seed_and_the_same_spends_boot_the_same_scouted_facility() {
    let booted = |scout: bool| {
        let mut run = Campaign::to_depth(99, 3);
        bank_for_scouts(&mut run, 1);
        let node = scoutable_offer(&run);
        if scout {
            assert!(run.scout(node).paid());
        }
        assert!(run.choose(node));
        let level = run.enter().expect("the facility boots");
        (level, start_level(&level).expect("the v1 recipe places"))
    };

    let (level, state) = booted(true);
    let (again, twice) = booted(true);
    assert_eq!(level, again, "same seed, same spends, same config");
    assert_eq!(render(&state).to_text(), render(&twice).to_text());

    let (unbought, fogged) = booted(false);
    assert_eq!(
        level.seed, unbought.seed,
        "the building is the same building either way",
    );
    assert_ne!(
        render(&state).to_text(),
        render(&fogged).to_text(),
        "and the scout is visible on the board it boots (§2.3)",
    );
}

/// **The scout's price is one knob** (§14 v3 **[START]**), pinned here so a change to it is
/// a visible change rather than a number that drifted.
#[test]
fn the_scout_costs_a_facilitys_whole_haul() {
    assert_eq!(SCOUT_COST, 3);

    // **A facility's whole haul** at the §10.2 recipe, and that is the point: what is
    // being sold is the §10 exploration of an entire building, answered before turn one,
    // so it is priced at what a building is worth. It is deliberately the expensive end of
    // the hub, against the road bought blind (#212) at a third of it.
    let recipe = LevelConfig::V1.intel as u32;
    assert_eq!(
        SCOUT_COST, recipe,
        "a building known costs a building robbed"
    );
    // A const block: both prices are constants, so this is a claim about the *build*
    // rather than about a run — the expensive end of the hub must stay the expensive end.
    const { assert!(SCOUT_COST > ROUTE_UNLOCK_COST) };

    // No run scouts its first facility: nothing is banked until a raid is walked out of,
    // and one clean raid is exactly the price — so the earliest a scout can be had is the
    // *second* choice point, and only by a run that took everything and spent nothing.
    let mut fresh = Campaign::to_depth(99, 3);
    fresh.enter();
    fresh.complete(&extracted(0, 1));
    assert!(!fresh.affords(SCOUT_COST));
    assert_eq!(
        fresh.scout(scoutable_offer(&fresh)),
        Outlay::Short {
            cost: SCOUT_COST,
            balance: 0,
        },
    );
}

/// A facility on offer that hides crates — what the manifest sink is priced against.
fn crated_offer(run: &Campaign) -> NodeId {
    run.ahead()
        .into_iter()
        .find(|offer| run.manifest_on_sale(offer.node))
        .expect("a choice point offers a facility with crates")
        .node
}

/// **The reveal can never lie** (#550) — the §2.3 assertion this sink owes, and the reason
/// it is stated against a *booted* facility rather than against a second copy of the
/// stocking rule: what the hub says is [`cache_contents`] itself, called on the very seed
/// the raid boots from (§8.3).
#[test]
fn the_manifest_is_the_tech_the_facility_actually_holds() {
    let mut run = Campaign::to_depth(99, 3);
    run.enter();
    run.complete(&extracted(MANIFEST_COST as usize, 1));
    let node = crated_offer(&run);

    assert!(
        run.manifest(node).is_none(),
        "unbought, the hub says nothing"
    );
    assert!(run.buy_manifest(node).paid());
    let promised = run.manifest(node).expect("bought");
    assert!(!promised.is_empty());

    assert!(run.choose(node));
    let level = run.enter().expect("the facility boots");
    let state = start_level(&level).expect("the v1 recipe places");
    assert_eq!(
        promised,
        state.cache_contents(),
        "the hub promised what the building holds",
    );
}

/// **What, never where** (§11.5a/#550): the manifest is a set of abilities and hands over no
/// cell — the crates stay fogged until seen, exactly as they are for a run that bought
/// nothing. Buying #215's scout is the only thing that moves them, and the two compose.
#[test]
fn the_manifest_reveals_no_position() {
    let mut run = Campaign::to_depth(99, 3);
    run.enter();
    run.complete(&extracted(MANIFEST_COST as usize, 1));
    let node = crated_offer(&run);
    assert!(run.buy_manifest(node).paid());
    assert!(!run.is_scouted(node), "the manifest is not a scout");

    assert!(run.choose(node));
    let level = run.enter().expect("the facility boots");
    assert!(
        !level.modifiers.scouted,
        "a manifest must not reach the level at all (#550: no modifier, no token slot)",
    );
    let state = start_level(&level).expect("the v1 recipe places");
    for cell in state.equipment_caches() {
        assert!(
            !state.memory().contains(cell) || state.player_fov().contains(cell),
            "{cell:?} was remembered without anyone paying for a position",
        );
    }
}

/// **Only a facility on offer that hides crates is for sale**, and only once. An Outpost, a
/// locked road, a facility elsewhere and a second purchase are all refused as *nothing to
/// buy here*, with the wallet untouched.
#[test]
fn only_a_crated_facility_on_offer_sells_its_manifest() {
    let mut run = Campaign::to_depth(99, 3);
    run.enter();
    run.complete(&extracted((MANIFEST_COST * 3) as usize, 1));
    let rich = run.intel();

    assert_eq!(run.buy_manifest(locked_offer(&run)), Outlay::Closed);
    assert_eq!(run.buy_manifest(NodeId::at(0, 0)), Outlay::Closed);
    for offer in run.ahead() {
        let empty = run.map().flavour(offer.node).modifiers().caches.crates() == 0;
        if empty && !offer.locked {
            assert!(!run.manifest_on_sale(offer.node), "nothing to sell");
            assert_eq!(run.buy_manifest(offer.node), Outlay::Closed);
        }
    }
    assert_eq!(run.intel(), rich, "no refusal took anything");

    let node = crated_offer(&run);
    assert!(run.buy_manifest(node).paid());
    assert_eq!(
        run.buy_manifest(node),
        Outlay::Closed,
        "a list you have already been read is not something to sell twice",
    );
    assert_eq!(run.intel(), rich - MANIFEST_COST);

    // And it survives the walk onto the facility it was bought for.
    assert!(run.choose(node));
    assert!(run.has_manifest(node));
}

/// **An unaffordable manifest is refused and costs nothing**, and says which fact it is.
#[test]
fn an_unaffordable_manifest_is_refused_and_costs_nothing() {
    let mut run = Campaign::to_depth(99, 3);
    run.enter();
    run.complete(&extracted((MANIFEST_COST - 1) as usize, 1));
    let node = crated_offer(&run);
    assert_eq!(
        run.buy_manifest(node),
        Outlay::Short {
            cost: MANIFEST_COST,
            balance: MANIFEST_COST - 1,
        },
    );
    assert_eq!(run.intel(), MANIFEST_COST - 1);
    assert!(run.manifest(node).is_none());
}

/// **The manifest's price is one knob** (§14 v3 **[START]**), pinned so a change to it is a
/// visible change — and pinned *against its neighbours*, which is where its meaning is: the
/// cheap sink beside the expensive one.
#[test]
fn the_manifest_costs_less_than_the_building_and_more_than_a_road() {
    assert_eq!(MANIFEST_COST, 2);
    const { assert!(MANIFEST_COST < SCOUT_COST) };
    const { assert!(MANIFEST_COST > ROUTE_UNLOCK_COST) };
}

/// **Determinism (§12.4)**: the same run seed and the same spends read the same manifest.
#[test]
fn the_same_seed_and_the_same_spends_read_the_same_manifest() {
    let read = || {
        let mut run = Campaign::to_depth(99, 3);
        run.enter();
        run.complete(&extracted(MANIFEST_COST as usize, 1));
        let node = crated_offer(&run);
        assert!(run.buy_manifest(node).paid());
        run.manifest(node).expect("bought")
    };
    assert_eq!(read(), read());
}
