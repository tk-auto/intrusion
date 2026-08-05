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
use crate::modifiers::{LevelModifiers, ModifierSources};
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
fn extracted(intel: usize, alert_peak: u32) -> Verdict {
    Verdict {
        ending: Ending::Escaped,
        stats: RunStats {
            turns: 40,
            intel,
            intel_total: 3,
            alert_peak,
            ..RunStats::default()
        },
    }
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
fn walk_on(run: &mut Campaign, intel: usize) {
    run.complete(&extracted(intel, 0));
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

/// **A loud raid does not follow the run out of the facility — yet** (#210). The
/// §7.3 ladder is per-facility and dies with it; the run-level alert is a seam with
/// no rule behind it, so every facility starts at base alert and this pins that the
/// campaign is not quietly inventing a difficulty curve of its own.
#[test]
fn a_raids_loudness_does_not_scale_the_next_facility_yet() {
    let mut run = Campaign::to_depth(66, 3);
    run.enter();
    run.complete(&extracted(1, TOP_RUNG));
    assert_eq!(run.alert(), 0, "the alert contribution is #210's to define");

    // Against a run of the same country that was **quiet** in the same facility and
    // walked the same way: the facility it arrives at must be identical.
    let mut quiet = Campaign::to_depth(66, 3);
    quiet.enter();
    quiet.complete(&extracted(1, 0));
    let node = run.offers()[0].node;
    assert!(run.choose(node) && quiet.choose(node));
    assert_eq!(
        run.enter(),
        quiet.enter(),
        "so the facility after a loud raid is the facility after a quiet one",
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
    let mut verdict = extracted(2, 0);
    verdict.stats.salvaged = Some(AbilityId::Confusion);
    run.complete(&verdict);

    assert!(run.loadout().contains(AbilityId::Confusion), "the find");
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
    verdict.stats.salvaged = Some(AbilityId::Confusion);
    assert_eq!(run.complete(&verdict), CampaignStage::Lost);
    assert_eq!(run.loadout(), Loadout::innate(), "the run is over");
}

/// **A Workshop is the facility its offer said it was** (§2.3/§14 v3/#209): the flavour
/// the map names resolves into the level-seed the run walks into, crate and all — and
/// it *pays* for it, one console fewer, so the offer is a trade rather than a bonus.
#[test]
fn the_workshop_flavour_plants_the_crate_it_advertises() {
    let modifiers = Flavour::Workshop.modifiers();
    assert!(modifiers.equipment_cache, "the crate the row promises");
    assert_eq!(
        modifiers.intel_count,
        crate::modifiers::IntelCount::Fewer,
        "and the console it costs (§2.3)",
    );
    // No other flavour hides one: the map's tech axis is a *choice*, not something
    // every facility hands over.
    for flavour in Flavour::ALL {
        assert_eq!(
            flavour.modifiers().equipment_cache,
            flavour == Flavour::Workshop,
            "{flavour:?}",
        );
    }
    // And it reaches the facility through the one seam (§12.6): walk a run until it
    // stands on a Workshop, and the level-seed it would boot carries the crate — which
    // is what makes the campaign facility's *token* the facility as it was played
    // (§12.7), rather than a label the map printed.
    let mut run = Campaign::new(PLAYED_SEED);
    let mut stood_on_one = false;
    while !run.stage().is_over() {
        let workshop = run.flavour() == Flavour::Workshop;
        assert_eq!(
            run.next_level().modifiers.equipment_cache,
            workshop,
            "the config disagrees with the row that offered it",
        );
        stood_on_one |= workshop;
        run.enter();
        walk_on(&mut run, 1);
    }
    assert!(
        stood_on_one,
        "a run that never meets a Workshop cannot test one — pick another seed",
    );
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
