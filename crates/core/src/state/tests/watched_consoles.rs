//! The `guards_watch_consoles` modifier through the turn loop (§7.5/§12.6/#319).
//!
//! A **harder** level modifier: a Calm guard prefers a cell beside a console its beat
//! touches over §7.5's farthest-uninspected sweep, and cycles them so every console in
//! a beat is stood beside in bounded time rather than by luck.
//!
//! The **picker** is pinned next door, in `guard::tests::watched_consoles` — the leg
//! it chooses, the alternation, the cycle, the moods it does not apply to. What is
//! pinned here is everything that only exists once a level is running: the seam the
//! modifier is read at, the silenced net that takes it away, and the three properties
//! §2.3 will not let it ship without — coverage is bounded, nobody camps, and the
//! pressure it adds over baseline is real.
//!
//! Every run here is **idle** (`Input::Wait` throughout). The modifier is Calm-only, so
//! a player pushing the guards around would be measuring the reaction rather than the
//! patrol; a player who stays put is never seen from a spawn no guard eyes (§10.6), and
//! the guards patrol undisturbed for as long as the measurement needs.

use crate::facility::Facility;
use crate::guard::{PatrolStyle, CONSOLE_CYCLE_TURNS, GUARD_DWELL_TURNS_MAX};
use crate::level_seed::{start_level_with, LevelSeed};
use crate::state::*;
use crate::test_support::{open_room, seed_sweep};
use crate::{LevelConfig, LevelModifiers, Loadout, Rng};
use std::collections::{HashMap, HashSet};

/// Whether `cell` is a cell to watch `console` from — orthogonally adjacent to it, the
/// bump range a console is used at (§4.3/§10.3).
fn beside(cell: Cell, console: Cell) -> bool {
    cell.manhattan_distance(console) == 1
}

/// Every console cell of a facility, in reading order — the watched set, read off the
/// terrain exactly as a guard reads it (§10.3).
fn consoles_of(facility: &Facility) -> Vec<Cell> {
    (0..facility.height())
        .flat_map(|y| (0..facility.width()).map(move |x| Cell::new(x, y)))
        .filter(|&cell| {
            matches!(
                facility.terrain(cell),
                Some(Terrain::Console | Terrain::CommsConsole)
            )
        })
        .collect()
}

/// §12.3/§12.6/#319: the modifier is read at **one** seam — how a Calm guard chooses
/// where to walk — and a silenced net outranks it there (§7.3).
#[test]
fn the_modifier_resolves_into_the_patrol_style_and_nothing_else() {
    assert_eq!(scene(false).patrol_style(), PatrolStyle::Beat);
    assert_eq!(scene(true).patrol_style(), PatrolStyle::WatchedConsoles);

    let mut silenced = scene(true);
    silenced.step(Input::Step(Direction::South)); // the bump that kills the net
    assert!(silenced.radio_silenced());
    assert_eq!(
        silenced.patrol_style(),
        PatrolStyle::Wander,
        "a dead net has no beats to hang the cycle on (§7.3)",
    );
}

/// §7.3/#319: **a silenced net takes the console watch with it.** The cycle is over the
/// consoles a guard's *beat* touches, and killing the radio leaves no partition to
/// divide the building with — every Calm guard takes the whole level and draws at
/// random. So with the net down a watched run is the baseline run, cell for cell: one
/// more thing the comms console buys, priced by the detour §7.3 already charges for it.
#[test]
fn a_silenced_net_takes_the_console_watch_with_it() {
    let walked = |watched: bool, silence: bool| {
        let mut state = scene(watched);
        if silence {
            state.step(Input::Step(Direction::South));
            assert!(state.radio_silenced(), "the net is down");
        }
        (0..80)
            .map(|_| {
                state.step(Input::Wait);
                state.guards()[0].pos()
            })
            .collect::<Vec<Cell>>()
    };

    assert_eq!(
        walked(true, true),
        walked(false, true),
        "with the net dead the modifier changes nothing",
    );
    assert_ne!(
        walked(true, false),
        walked(false, false),
        "…and with it live it changes plenty — or the comparison above proves nothing",
    );
}

/// §10.6/§2.3/#319: **no console is watched by nobody.** The coverage bound below is
/// stated over the consoles inside a beat, so it would be a facade if some console
/// belonged to no beat at all — a level could satisfy the bound with a console standing
/// unwatched for the whole run. Beats partition the level (§7.5/§10.5); this is what
/// holds that partition to covering the objectives.
#[test]
fn every_console_stands_beside_some_guards_beat() {
    for seed in seed_sweep(64) {
        let state = level(seed, true);
        let claimed: HashSet<Cell> = state
            .guards()
            .iter()
            .flat_map(|guard| guard.beat())
            .copied()
            .collect();
        for console in consoles_of(state.layout().facility()) {
            assert!(
                claimed.iter().any(|&cell| beside(cell, console)),
                "seed {seed}: the console at {console:?} is in no guard's beat",
            );
        }
    }
}

/// §12.6/§2.3/#319: **coverage is bounded, not lucky.** On a generated level, from the
/// level's start, every console has a guard **stand orthogonally beside it** within
/// [`CONSOLE_CYCLE_TURNS`] turns of Calm patrol — with the shipped dwell, because the
/// pause is part of what a cycle costs.
///
/// Folded in here rather than given a run of its own, because it wants exactly the same
/// turns: **no camping** (§7.6/§2.3). A guard arriving beside a console takes the
/// ordinary §7.5 dwell and then leaves, so no cell beside a console is held longer than
/// a dwell and the slow turn out of it. Nothing here lets a guard become the fixture
/// parked on the objective that §7.6 exists to prevent.
#[test]
fn every_console_in_a_beat_is_stood_beside_within_the_cycle_bound() {
    /// A dwell (≤ [`GUARD_DWELL_TURNS_MAX`]) plus the arrival turn, the slow quarter
    /// out of it (§7.5) and a turn or two held up by a colleague (§7.8). The measured
    /// worst over the swept seeds is 12; a guard that has stopped leaving is unbounded,
    /// which is the failure this catches.
    const LONGEST_HONEST_STOP: u32 = GUARD_DWELL_TURNS_MAX + 8;

    for seed in seed_sweep(64) {
        let mut state = level(seed, true);
        let consoles = consoles_of(state.layout().facility());
        let mut first: HashMap<Cell, u32> = HashMap::new();
        let mut held: Vec<(u32, Cell)> = state.guards().iter().map(|g| (0, g.pos())).collect();
        for turn in 1..=CONSOLE_CYCLE_TURNS {
            state.step(Input::Wait);
            for (i, guard) in state.guards().iter().enumerate() {
                if guard.pos() == held[i].1 {
                    held[i].0 += 1;
                } else {
                    held[i] = (1, guard.pos());
                }
                for &console in &consoles {
                    if !beside(guard.pos(), console) {
                        continue;
                    }
                    first.entry(console).or_insert(turn);
                    assert!(
                        held[i].0 <= LONGEST_HONEST_STOP,
                        "seed {seed}, turn {turn}: the guard has stood beside the console \
                         at {console:?} for {} turns — that is a post, not a patrol (§7.6)",
                        held[i].0,
                    );
                }
            }
        }
        for console in consoles {
            assert!(
                first.contains_key(&console),
                "seed {seed}: nobody stood beside the console at {console:?} within \
                 {CONSOLE_CYCLE_TURNS} turns",
            );
        }
    }
}

/// §12.6/§2.3/#319 — **the directional assertion**, in the strongest frame it can be
/// stated in. The modifier is read at the patrol-destination seam, so the two arms are
/// the same building with the same guards walking different legs.
///
/// Pressure is measured as **turns with a guard's cone on a console**: it is the thing
/// the rule claims to raise, and it is what a player has to time their two fixed
/// errands against (§4.5/§7.3). Direction only, never a leaderboard (§13.4) — the claim
/// is *at least as much as baseline*, per seed, and it holds on every seed of the sweep
/// (measured: 5603 turns against 3403 over 64 seeds, with no seed inverting).
///
/// The second half is the same comparison read the other way, and it is why this is one
/// test rather than two: the plain ground of a beat must still be swept. A modifier
/// that bought its console coverage by abandoning the corridors would show up here as a
/// collapse against the baseline's coverage of those very cells.
#[test]
fn the_watched_run_puts_more_cone_on_the_consoles_without_starving_the_sweep() {
    /// Long enough for several patrol legs on either arm, short enough to sweep the
    /// seeds twice over inside the gate.
    const TURNS: u32 = 300;
    /// How much of the baseline's plain-ground coverage the watched arm must keep. The
    /// measured floor over the swept seeds is 0.87 and the mean is 1.00 — the sweep is
    /// interleaved, not replaced — so this is a wide margin around "not starved".
    const KEPT: f64 = 0.75;

    for seed in seed_sweep(32) {
        let watched = swept(level(seed, true), TURNS);
        let baseline = swept(level(seed, false), TURNS);
        assert!(
            watched.console_cone_turns >= baseline.console_cone_turns,
            "seed {seed}: watching the consoles put *less* cone on them than baseline \
             ({} < {})",
            watched.console_cone_turns,
            baseline.console_cone_turns,
        );
        assert!(
            watched.plain_ground_seen as f64 >= baseline.plain_ground_seen as f64 * KEPT,
            "seed {seed}: the ordinary sweep is starved — {} of the beats' plain cells \
             looked at against the baseline's {}",
            watched.plain_ground_seen,
            baseline.plain_ground_seen,
        );
    }
}

/// §12.4/#319: determinism, with the modifier and without it — same seed, same
/// modifiers, same inputs, the same run cell for cell. The console cycle draws no RNG
/// of its own (it is a deterministic pick over terrain the carve already stamped), so
/// it cannot perturb the run's stream either.
#[test]
fn a_watched_run_replays_exactly() {
    for seed in seed_sweep(8) {
        for watched in [false, true] {
            let walked = |seed: u64| {
                let mut state = level(seed, watched);
                (0..120)
                    .map(|_| {
                        state.step(Input::Wait);
                        state.guards().iter().map(Guard::pos).collect::<Vec<Cell>>()
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(walked(seed), walked(seed), "seed {seed}, watched {watched}");
        }
    }
}

/// A generated v1 facility for `seed`, with the consoles watched or not — booted
/// through the one boot path the web shell, the replay viewer and the sim share, so the
/// level under test is the level a player would be handed. The modifier is read at the
/// patrol seam and nowhere near generation (§12.6), so the two arms are the same carve.
fn level(seed: u64, watched: bool) -> State {
    let level = LevelSeed {
        seed,
        modifiers: LevelModifiers {
            guards_watch_consoles: watched,
            ..LevelModifiers::default()
        },
        abilities: Loadout::innate(),
    };
    start_level_with(&LevelConfig::V1, &level).expect("the v1 recipe carves")
}

/// A hand-built room for the seam tests: one patrolling guard on the northern rows, an
/// intel console in its beat, and the player far enough south — further than a guard
/// can see (§7.1) — to stay unseen, standing on the comms console so a single step
/// south is the bump that kills the net (§7.3).
fn scene(watched: bool) -> State {
    const W: u32 = 13;
    const H: u32 = 20;
    let player = Cell::new(6, 18);
    let mut layout = open_room(W, H);
    layout.place(Cell::new(6, 3), Terrain::Console);
    layout.place(Cell::new(player.x, player.y + 1), Terrain::CommsConsole);
    let beat = (1..=5)
        .flat_map(|y| (1..W - 1).map(move |x| Cell::new(x, y)))
        .collect();
    let mut state = State::new(
        layout,
        player,
        Direction::South,
        vec![Guard::patrolling(Cell::new(1, 1)).with_beat(beat)],
        Vec::new(),
        Cell::new(1, H - 2),
    )
    .with_rng(Rng::new(7))
    .with_modifiers(LevelModifiers {
        guards_watch_consoles: watched,
        ..LevelModifiers::default()
    });
    state.set_guard_dwell_chance(0);
    state
}

/// What one idle run is measured by — two numbers, both about coverage (§13.4).
struct Swept {
    /// Turns in which some guard's cone covered some console: the pressure the modifier
    /// claims to raise.
    console_cone_turns: u32,
    /// How many of the beats' **plain** cells — everything not beside a console — had a
    /// cone on them at some point: the coverage it must not buy that pressure with.
    plain_ground_seen: usize,
}

fn swept(mut state: State, turns: u32) -> Swept {
    let consoles = consoles_of(state.layout().facility());
    let mut console_cone_turns = 0;
    let mut looked_at: HashSet<Cell> = HashSet::new();
    for _ in 0..turns {
        state.step(Input::Wait);
        let mut on_a_console = false;
        for guard in state.guards() {
            on_a_console |= consoles.iter().any(|&cell| guard.fov().contains(cell));
            looked_at.extend(guard.fov().cells());
        }
        console_cone_turns += u32::from(on_a_console);
    }
    let plain_ground_seen = state
        .guards()
        .iter()
        .flat_map(|guard| guard.beat())
        .filter(|&&cell| !consoles.iter().any(|&console| beside(cell, console)))
        .filter(|&&cell| looked_at.contains(&cell))
        .count();
    Swept {
        console_cone_turns,
        plain_ground_seen,
    }
}
