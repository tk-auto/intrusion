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
use crate::path::first_step_toward;
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

/// **A run starts with nothing** (§2.2): the innate verbs, an empty wallet, a facility
/// that has not noticed anybody — and it starts *between* facilities, on the approach
/// to the first one.
#[test]
fn a_fresh_run_carries_nothing_it_did_not_walk_in_with() {
    let run = Campaign::new(11);
    assert_eq!(run.stage(), CampaignStage::Approach);
    assert_eq!(run.outcome(), Outcome::Playing);
    assert_eq!(run.position(), 0);
    assert_eq!(run.route().len(), CAMPAIGN_LENGTH);
    assert_eq!(run.node(), Some(NodeId::new(0)));
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
        .map(|id| facility_seed(8371, NodeId::new(id)))
        .collect();
    assert_eq!(
        golden,
        vec![63_589, 128_467, 124_555, 97_859],
        "the derivation moved — every campaign ever seeded is a different run",
    );

    // Stable, and a different run seed is a different campaign.
    assert_eq!(facility_seed(8371, NodeId::new(2)), golden[2]);
    assert_ne!(facility_seed(8372, NodeId::new(2)), golden[2]);

    for id in 0..64 {
        let seed = facility_seed(4242, NodeId::new(id));
        assert_eq!(
            LevelSeed::quick_play(seed).encode().map(|t| t.len()),
            Some(crate::level_seed::TOKEN_LEN),
            "facility {id}'s seed must fit the level-seed token",
        );
    }
}

/// **Entering hands out the facility and starts the raid**, once: a second ask while a
/// raid is under way would mint a second `State` for one facility, and two answers to
/// how it went.
#[test]
fn entering_starts_the_raid_and_does_not_start_it_twice() {
    let mut run = Campaign::new(7);
    let level = run.enter().expect("the first facility is ahead");
    assert_eq!(run.stage(), CampaignStage::Inside);
    assert_eq!(run.enter(), None, "a raid is already under way");

    assert_eq!(level.seed, facility_seed(7, NodeId::new(0)));
    assert_eq!(
        level.modifiers.intel_to_exit,
        IntelGate::None,
        "intel is currency in the campaign, not an exit key (§4.5/§2.2)",
    );
    assert_eq!(level.abilities, run.loadout(), "the run's own loadout");
    assert_eq!(
        Some(level),
        run.next_level(),
        "the config is readable without starting the raid",
    );
}

/// **Completing a raid banks the haul and moves the run on** — and the next facility is
/// a different one.
#[test]
fn completing_a_raid_banks_the_haul_and_moves_on() {
    let mut run = Campaign::of_length(99, 3);
    let first = run.enter().expect("a facility to raid");

    assert_eq!(run.complete(&extracted(2, 1)), CampaignStage::Approach);
    assert_eq!(run.position(), 1);
    assert_eq!(run.intel(), 2, "the raid's consoles bank");

    let second = run.enter().expect("the next facility is ahead");
    assert_ne!(second.seed, first.seed, "a new facility, not the last one");

    // Intel accumulates across facilities — that is the whole of §2.2's currency row.
    run.complete(&extracted(1, 0));
    assert_eq!(run.intel(), 3);
}

/// **A loud raid does not follow the run out of the facility — yet** (#210). The
/// §7.3 ladder is per-facility and dies with it; the run-level alert is a seam with
/// no rule behind it, so every facility starts at base alert and this pins that the
/// campaign is not quietly inventing a difficulty curve of its own.
#[test]
fn a_raids_loudness_does_not_scale_the_next_facility_yet() {
    let mut run = Campaign::of_length(66, 3);
    let quiet = run.enter().expect("a facility to raid");
    run.complete(&extracted(1, TOP_RUNG));
    assert_eq!(run.alert(), 0, "the alert contribution is #210's to define");

    let after = run.enter().expect("the next facility");
    assert_eq!(
        after.modifiers, quiet.modifiers,
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
        let mut run = Campaign::of_length(5, 4);
        run.enter();
        run.complete(&extracted(2, 1));
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
        assert_eq!(run.position(), 1, "and it got no further than it got");
        assert_eq!(run.intel(), 2, "the last raid banked nothing");
    }
}

/// **Reaching the end of the sequence wins the run.** The archive that will stand at
/// the far end, and the ending it earns, are #217's; that the sequence *ends* is this
/// layer's.
#[test]
fn the_end_of_the_sequence_wins_the_run() {
    let mut run = Campaign::of_length(3, 2);
    run.enter();
    assert_eq!(run.complete(&extracted(1, 0)), CampaignStage::Approach);
    run.enter();
    assert_eq!(run.complete(&extracted(1, 0)), CampaignStage::Won);

    assert_eq!(run.outcome(), Outcome::Won);
    assert!(run.stage().is_over());
    assert_eq!(run.node(), None, "the route is walked out");
    assert_eq!(run.next_level(), None);
    assert_eq!(run.enter(), None);
}

/// **Salvaged tech rides into the next facility** (§2.2) — the accumulation the
/// campaign exists for, and the seam an equipment cache writes (#209).
#[test]
fn salvaged_tech_rides_into_the_next_facility() {
    let mut run = Campaign::of_length(21, 3);
    let first = run.enter().expect("a facility to raid");
    assert!(!first.abilities.contains(AbilityId::Dephase));

    run.salvage(AbilityId::Dephase);
    run.complete(&extracted(1, 0));
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

/// **The campaign offers no way to play the run again** (§2.2/appendix 31). The gate
/// has been in the code since the end screen shipped; this is the first thing that
/// actually stands behind it.
#[test]
fn the_campaign_offers_no_way_to_play_the_run_again() {
    let options = Campaign::new(1).run_options();
    assert_eq!(options.mode, RunMode::Campaign);
    assert_eq!(options.mode.exits(), &[EndExit::Menu]);
}

/// **A one-facility campaign is the game v1 already ships**: enter, raid, leave, and
/// the run is over — a strict superset, not a rewrite of the turn loop.
#[test]
fn a_one_facility_campaign_is_a_single_raid() {
    let mut run = Campaign::of_length(PLAYED_SEED, 1);
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

/// **A whole run reproduces from its seed and its inputs** (§12.4), across more than
/// one facility — the property every golden test above the level rests on, and the one
/// the player never gets to use (§2.2: the run is one-shot).
///
/// The second pass replays the *recorded inputs*, so what is asserted is the engine's
/// determinism rather than the test routine's.
#[test]
fn a_two_facility_run_replays_byte_for_byte() {
    let mut first = Campaign::of_length(PLAYED_SEED, 2);
    let mut script: Vec<(LevelSeed, Vec<Input>)> = Vec::new();
    let mut grids: Vec<Vec<String>> = Vec::new();

    while let Some(level) = first.enter() {
        let played = raid(&level);
        assert_eq!(
            played.state.outcome(),
            Outcome::Won,
            "facility {} was raided and left",
            first.position(),
        );
        grids.push(render(&played.state).to_text());
        script.push((level, played.inputs));
        first.complete(&played.state.verdict().expect("the raid ended"));
    }
    assert_eq!(script.len(), 2, "both facilities were played");
    assert_eq!(first.stage(), CampaignStage::Won);
    assert_ne!(
        script[0].0.seed, script[1].0.seed,
        "two facilities, not the same one twice",
    );

    // Same run seed, same inputs — same run, from the campaign's carried state down to
    // the last glyph of each facility's final frame.
    let mut second = Campaign::of_length(PLAYED_SEED, 2);
    for (facility, (level, inputs)) in script.iter().enumerate() {
        assert_eq!(second.enter().as_ref(), Some(level), "facility {facility}");
        let state = replay(level, inputs);
        assert_eq!(
            render(&state).to_text(),
            grids[facility],
            "facility {facility} replayed to a different board",
        );
        second.complete(&state.verdict().expect("the raid ended"));
    }
    assert_eq!(second, first, "the whole run, carried state and all");
}
