use super::*;
use crate::facility::Facility;
use crate::test_support::open_beat;
use crate::vision::{field_of_view, FULL_SIGHT_ARC, GUARD_SIGHT_RANGE};

/// §7.5: a guard with **no beat** has no territory and holds. There is no box
/// fallback any more — a flood around a remembered spawn cell was the half of
/// §7.5's named weakness that survived the region beat, and a guard that has
/// walked away from that cell would be sweeping a phantom.
#[test]
fn a_guard_without_a_beat_has_no_territory() {
    // A room big enough that any radius box would have found plenty of ground.
    let facility = Facility::walled_box(60, 60);
    let mut guard = Guard::patrolling(Cell::new(30, 30));

    assert!(!guard.has_beat());
    assert!(
        guard.territory(&facility, PatrolStyle::Beat).is_empty(),
        "no beat, no territory",
    );
    assert_eq!(
        guard.next_target_in(
            &guard.territory(&facility, PatrolStyle::Beat),
            PatrolStyle::Beat,
            &mut Rng::new(0),
        ),
        None,
        "and so nothing to walk to — the guard holds",
    );
}

/// §7.5/§10.5: a guard carrying a region beat sweeps *it* — a beat cell far
/// across the map is territory, a cell beside the guard that is not on the beat
/// is not, and unsweepable terrain (furniture) is filtered out at sweep time
/// rather than baked in.
#[test]
fn a_beat_guard_sweeps_its_beat() {
    let mut facility = Facility::walled_box(40, 5);
    facility.set_terrain(20, 1, Terrain::PartialCover);
    let anchor = Cell::new(1, 1);
    let far = Cell::new(35, 1);

    let beat = vec![anchor, Cell::new(2, 1), Cell::new(20, 1), far];
    let territory = Guard::patrolling(anchor)
        .with_beat(beat)
        .territory(&facility, PatrolStyle::Beat);
    assert!(territory.contains(&far), "the beat bounds the territory");
    assert!(
        !territory.contains(&Cell::new(20, 1)),
        "furniture on the beat is not a sweep target",
    );
    assert!(
        !territory.contains(&Cell::new(3, 1)),
        "off-beat cells are not territory, however close to the guard",
    );
}

/// A guard part-way through the §7.6 **watch** that follows a released search:
/// Calm again, centred on `focus`, and carrying `beat` as its own territory.
#[cfg(test)]
fn watcher(pos: Cell, focus: Cell, beat: Vec<Cell>) -> Guard {
    let mut guard = Guard::patrolling(pos).with_beat(beat);
    guard.focus = Some(focus);
    guard.watch = WATCH_DURATION;
    guard
}

/// §7.6/§7.7 — the clustering a player sees **during** a hunt. Two guards answering
/// one call share a `focus`, which is correct (a call carries its own cell, §7.7).
/// What they must not share is a *territory*: handed the same watch disc they pace
/// the same region for the whole 20-turn window, and `pick_farthest` being
/// deterministic they keep choosing the same cell and walk in lockstep.
///
/// Clipped to each guard's own beat, the same call leaves them watching different
/// halves of the same area — which is what "watched harder" was supposed to mean.
#[test]
fn two_guards_watching_one_cell_cover_disjoint_ground() {
    let facility = Facility::walled_box(20, 8);
    let focus = Cell::new(10, 4);
    // Two beats meeting at the focus: west of it and east of it.
    let half = |xs: std::ops::Range<u32>| -> Vec<Cell> {
        xs.flat_map(|x| (1..7).map(move |y| Cell::new(x, y)))
            .collect::<Vec<_>>()
    };
    let west = watcher(Cell::new(6, 4), focus, half(1..11));
    let east = watcher(Cell::new(14, 4), focus, half(11..19));

    let (a, b) = (
        west.territory(&facility, PatrolStyle::Beat),
        east.territory(&facility, PatrolStyle::Beat),
    );
    assert!(!a.is_empty() && !b.is_empty(), "both watch something");
    assert!(
        !a.iter().any(|cell| b.contains(cell)),
        "two responders to one cell watch disjoint ground",
    );
    // …and each is watching near the focus, not merely somewhere else.
    for territory in [&a, &b] {
        assert!(
            territory
                .iter()
                .all(
                    |c| path::reachable_within(focus, WATCH_RADIUS, |cell| patrollable(
                        &facility, cell
                    ))
                    .contains(c)
                ),
            "the watch stays inside the disc it is centred on",
        );
    }
}

/// The solo case is **unchanged**: a guard whose beat contains the whole disc
/// watches exactly the disc it always did. This must not quietly retune the
/// one-guard behaviour while fixing the two-guard one.
#[test]
fn a_lone_watchers_disc_is_unchanged() {
    let facility = Facility::walled_box(20, 8);
    let focus = Cell::new(10, 4);
    let whole: Vec<Cell> = (1..19)
        .flat_map(|x| (1..7).map(move |y| Cell::new(x, y)))
        .collect();
    let guard = watcher(Cell::new(10, 4), focus, whole);

    let disc = path::reachable_within(focus, WATCH_RADIUS, |cell| patrollable(&facility, cell));
    let mut watched = guard.territory(&facility, PatrolStyle::Beat);
    let mut expected = disc;
    watched.sort_by_key(|c| (c.y, c.x));
    expected.sort_by_key(|c| (c.y, c.x));
    assert_eq!(
        watched, expected,
        "a beat containing the disc watches the disc"
    );
}

/// §7.6's fallback: a guard called clear across the facility has no beat cells
/// anywhere near the focus, and an empty intersection would leave it with nothing
/// to sweep. It watches the plain disc instead — a responder must always watch
/// *something*.
#[test]
fn a_watcher_called_off_its_beat_falls_back_to_the_plain_disc() {
    let facility = Facility::walled_box(40, 8);
    let focus = Cell::new(35, 4);
    // A beat at the far west; the focus is 25 cells east of its nearest cell.
    let far: Vec<Cell> = (1..8)
        .flat_map(|x| (1..7).map(move |y| Cell::new(x, y)))
        .collect();
    let guard = watcher(Cell::new(35, 4), focus, far);

    let watched = guard.territory(&facility, PatrolStyle::Beat);
    let disc = path::reachable_within(focus, WATCH_RADIUS, |cell| patrollable(&facility, cell));
    assert!(!watched.is_empty(), "it still watches something");
    assert_eq!(watched.len(), disc.len(), "…and it is the plain disc");
}

/// §7.3: on a dead net there is no partition to clip against, so the watch is the
/// plain disc — and that is correct rather than a gap, since call-ins do not fire
/// on a silenced net and no two guards are sent to one cell in the first place.
#[test]
fn a_silenced_nets_watch_is_the_plain_disc() {
    let facility = Facility::walled_box(20, 8);
    let focus = Cell::new(10, 4);
    let half: Vec<Cell> = (1..11)
        .flat_map(|x| (1..7).map(move |y| Cell::new(x, y)))
        .collect();
    let guard = watcher(Cell::new(6, 4), focus, half);

    let disc = path::reachable_within(focus, WATCH_RADIUS, |cell| patrollable(&facility, cell));
    assert_eq!(
        guard.territory(&facility, PatrolStyle::Wander).len(),
        disc.len(),
        "a dead net watches the whole disc",
    );
    assert!(
        guard.territory(&facility, PatrolStyle::Beat).len() < disc.len(),
        "…where a live one clips it to the guard's own half",
    );
}

/// §7.5/§10.5 on generated levels: every placed guard's Calm territory is its
/// region beat — every cell of it walkable from where the guard stands (no
/// territory straddles a wall into a space it cannot reach), and the corridors
/// adjacent to its rooms are genuinely part of the beat, not ground crossed
/// incidentally.
///
/// **On the carve as generated, which is a weaker claim than it reads as** (#477).
/// [`generate_level`](crate::generate_level) hands back a *bare* board: the intel
/// consoles, the comms console and the exit are stamped in later, by
/// [`State::new`](crate::State::new), so on the board this test sees they are all
/// still plain floor. They are the one terrain that blocks a guard's route without
/// blocking pathing (§10.3), and one of them landing in a one-cell throat is exactly
/// what strands a beat cell — which is why this test passed over the whole seed range
/// while guards froze in play. The property **on the board a run is played on** is
/// [`a_calm_guard_never_walks_at_ground_it_cannot_reach`]; keep both, they check
/// different boards.
#[test]
fn placed_guard_territories_are_reachable_and_cover_corridors() {
    use crate::generate::generate_level;
    use crate::place::LevelConfig;
    use crate::rng::Rng;
    use crate::test_support::seed_sweep;
    use std::collections::HashSet;

    for seed in seed_sweep(32) {
        let (layout, placement) = generate_level(
            &LevelConfig::V1,
            &crate::LevelModifiers::default(),
            &mut Rng::new(seed),
        )
        .expect("v1 generates");
        let facility = layout.facility();
        for guard in placement.guards(&layout) {
            let territory = guard.territory(facility, PatrolStyle::Beat);
            assert!(!territory.is_empty(), "seed {seed}: an empty beat");

            let reached: HashSet<Cell> =
                path::flood_from(guard.pos(), facility.width(), facility.height(), |c| {
                    routable(facility, c)
                })
                .into_iter()
                .collect();
            for &cell in &territory {
                assert!(
                    reached.contains(&cell),
                    "seed {seed}: territory cell {cell:?} is not walkable from \
                         the guard at {:?}",
                    guard.pos(),
                );
            }

            // Corridors are patrolled ground, not space crossed incidentally — but
            // under a partition (§7.5) that is a property of the *level*, not of
            // every beat: a part can legitimately be two rooms joined by a door.
            // The level-wide form is asserted in `place`'s partition test, which
            // pins that every region — corridors included — is in some beat.
        }
    }
}

/// The sibling that closes the blind spot above (#481): the **same claim on the
/// stamped board** — every cell a guard's beat leaves it to sweep is walkable from
/// where the guard stands, with the intel consoles, the comms console and the exit
/// all solid.
///
/// [`placed_guard_territories_are_reachable_and_cover_corridors`] cannot see this,
/// because [`generate_level`](crate::generate_level) hands back a bare carve and the
/// usables are stamped by [`State::new`](crate::State::new) afterwards. It is the
/// weaker claim documented rather than fixed when #477 landed, and it passed over the
/// whole seed range while 17% of seeds shipped a pocket nobody could enter.
///
/// The territory is what a guard actually sweeps, so it is already filtered to the
/// [`patrollable`] cells — a console cell is never a target, whoever's beat it fell
/// in. What this asserts is the part that filter cannot fix: that everything left
/// **is** reachable. The board-wide property it rests on — that no stamp orphaned any
/// ground at all — is `place`'s `no_placed_usable_seals_walkable_ground_off`.
#[test]
fn placed_guard_territories_are_reachable_on_the_stamped_board() {
    use crate::level_seed::{start_level, LevelSeed};
    use crate::test_support::seed_sweep;
    use std::collections::HashSet;

    for seed in seed_sweep(32) {
        let state = start_level(&LevelSeed::quick_play(seed)).expect("quick play generates");
        let facility = state.layout().facility();
        for guard in state.guards() {
            let territory = guard.territory(facility, PatrolStyle::Beat);
            assert!(!territory.is_empty(), "seed {seed}: an empty beat");

            let reached: HashSet<Cell> =
                path::flood_from(guard.pos(), facility.width(), facility.height(), |c| {
                    routable(facility, c)
                })
                .into_iter()
                .collect();
            for &cell in &territory {
                assert!(
                    reached.contains(&cell),
                    "seed {seed}: territory cell {cell:?} is not walkable from the \
                     guard at {:?} once the usables are stamped",
                    guard.pos(),
                );
            }
        }
    }
}

/// §7.5/#477 on the board a run is actually played on: **a Calm guard is never
/// walking at ground it cannot reach.**
///
/// The freeze this pins was a permanent one. A solid usable stamped into a one-cell
/// throat seals the cells behind it off from every guard (§10.3 — it blocks a route
/// without blocking pathing) while the region beat cut from the graph still claims
/// them. The sealed cell is then the farthest thing in the beat from the opposite
/// corner and can never be marked inspected, so the sweep picked it, could not walk a
/// step toward it, kept it, and re-picked it — and the guard stood on one cell for the
/// rest of the run. Measured before the fix: 38 of 300 quick-play seeds froze a guard
/// for 100+ unbroken turns, several for the whole 500-turn window.
///
/// Booted through [`start_level`](crate::start_level), deliberately — the one boot path
/// the web shell, the replay viewer and the sim all share, and the only one that has
/// the usables stamped in. Run idle, because the freeze is a property of the patrol and
/// wants no player pushing the guards around.
#[test]
fn a_calm_guard_never_walks_at_ground_it_cannot_reach() {
    use crate::level_seed::{start_level, LevelSeed};
    use crate::test_support::seed_sweep;
    use crate::Input;
    use std::collections::HashSet;

    /// Long enough that every guard has finished several patrol legs — the freeze
    /// only appears once a sweep reaches the corner the sealed pocket is farthest
    /// from, which on the reported seed took ~150 turns.
    const TURNS: usize = 300;
    /// A Calm guard holds for a dwell (3–7, §7.5) plus up to two turns of slow
    /// turning, and longer when a colleague seals the only route (§7.8). Well past
    /// all of that, and far short of the permanent hold this exists to catch.
    const LONGEST_HONEST_HOLD: u32 = 60;

    for seed in seed_sweep(64) {
        let mut state = start_level(&LevelSeed::quick_play(seed)).expect("quick play generates");
        let mut held: Vec<(u32, Cell)> = state.guards().iter().map(|g| (0, g.pos())).collect();
        for turn in 0..TURNS {
            state.step(Input::Wait);
            let facility = state.layout().facility();
            for (i, guard) in state.guards().iter().enumerate() {
                let Some(destination) = guard.destination() else {
                    held[i] = (0, guard.pos());
                    continue;
                };
                if guard.state() == GuardState::Calm {
                    let walkable: HashSet<Cell> =
                        path::flood_from(guard.pos(), facility.width(), facility.height(), |c| {
                            routable(facility, c)
                        })
                        .into_iter()
                        .collect();
                    assert!(
                        walkable.contains(&destination),
                        "seed {seed}, turn {turn}: the guard at {:?} is walking at \
                         {destination:?}, which no route reaches",
                        guard.pos(),
                    );
                }
                // The symptom as a player sees it, independent of the mechanism: a
                // guard standing on one cell with somewhere it still means to be.
                if guard.pos() == held[i].1 && destination != guard.pos() {
                    held[i].0 += 1;
                    assert!(
                        held[i].0 <= LONGEST_HONEST_HOLD,
                        "seed {seed}, turn {turn}: the guard at {:?} has held one cell \
                         for {} turns with {destination:?} still to walk to",
                        guard.pos(),
                        held[i].0,
                    );
                } else {
                    held[i] = (0, guard.pos());
                }
            }
        }
    }
}

/// §7.5/#477, the mechanism in one room: the sweep will not target ground sealed off
/// by a **solid usable**, however far away — and being farthest is exactly what used
/// to get such a cell picked.
///
/// ```text
///   0        9 11
///   ############   row 0
///   #@........$.#   $ = the console sealing the alcove behind it
///   ############   row 2
/// ```
///
/// The guard stands at the west end with the whole strip as its beat. `(11, 1)` is the
/// farthest uninspected cell in it and the one [`pick_farthest`] would take; the
/// console at `(10, 1)` is the only way to it, and a console is solid to a guard
/// (§10.3), so it must take the farthest cell on *its* side instead.
#[test]
fn the_sweep_will_not_target_ground_a_console_has_sealed_off() {
    let mut facility = Facility::walled_box(13, 3);
    facility.set_terrain(10, 1, Terrain::Console);
    let beat: Vec<Cell> = (1..12).map(|x| Cell::new(x, 1)).collect();
    let mut guard = Guard::patrolling(Cell::new(1, 1)).with_beat(beat);

    guard.repick_patrol_target(&facility, PatrolStyle::Beat, &mut Rng::new(0));

    assert_eq!(
        guard.destination(),
        Some(Cell::new(9, 1)),
        "the farthest cell it can actually walk to, not the farthest cell",
    );
}

/// §7.8 kept intact: a colleague standing in the only route is a **this turn**
/// problem, and the guard holds and retries rather than throwing its target away.
///
/// This is the case the #477 fix must not swallow. The route test it added is drawn
/// over bare terrain for exactly this reason — fold colleagues into it and a guard
/// would discard a perfectly good destination every time one crossed the corridor
/// ahead of it, which is a patrol that never gets anywhere.
#[test]
fn a_colleague_in_the_way_does_not_cost_the_guard_its_target() {
    // A one-cell-wide corridor: there is no way round a colleague standing in it.
    let facility = Facility::walled_box(10, 3);
    let far = Cell::new(8, 1);
    let mut guard = Guard::patrolling_to(Cell::new(1, 1), far).with_beat(open_beat(10, 3));
    let colleague = [Cell::new(2, 1)];

    let step = guard.decide(
        &facility,
        &colleague,
        &mut Rng::new(0),
        Dwell::NEVER,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    );

    assert_eq!(step, None, "sealed in this turn, the guard holds");
    assert_eq!(
        guard.destination(),
        Some(far),
        "…and keeps the target it was walking to",
    );
}

/// §7.6: the post-search raised-coverage watch overrides the beat exactly as
/// it overrode the old radius box — while the watch runs, the sweep draws the
/// tighter [`WATCH_RADIUS`] disc around the focus, beat or no beat, and the
/// beat returns once the watch cools.
#[test]
fn the_released_watch_overrides_the_beat() {
    let facility = Facility::walled_box(40, 40);
    let focus = Cell::new(30, 30);
    let mut guard =
        Guard::patrolling(Cell::new(5, 5)).with_beat(vec![Cell::new(5, 5), Cell::new(6, 5)]);
    guard.focus = Some(focus);
    guard.watch = 1;

    let watched = guard.territory(&facility, PatrolStyle::Beat);
    assert!(
        watched
            .iter()
            .all(|&c| focus.manhattan_distance(c) <= WATCH_RADIUS),
        "the watch disc, not the beat",
    );
    assert!(watched.contains(&focus));

    guard.watch = 0;
    assert_eq!(
        guard.territory(&facility, PatrolStyle::Beat),
        vec![Cell::new(5, 5), Cell::new(6, 5)],
        "the beat returns once the watch cools",
    );
}

/// §7.5: with no destination a Calm guard walks to the **farthest** uninspected
/// cell in its territory — *farthest*, not nearest, so patrols pace across
/// distances. Ties resolve toward the north-west, deterministically (§12.4).
#[test]
fn patrol_picks_the_farthest_uninspected_cell() {
    let nothing_seen = VisibleSet::default();
    let origin = Cell::new(1, 1);

    // (1,4) at distance 3 beats (3,1) at distance 2 — farthest, not nearest.
    let spread = [Cell::new(3, 1), Cell::new(1, 4)];
    assert_eq!(
        pick_farthest(&spread, &nothing_seen, origin),
        Some(Cell::new(1, 4)),
    );

    // Equidistant cells (both at distance 3) break toward the smaller y, then x.
    let tied = [Cell::new(1, 4), Cell::new(4, 1)];
    assert_eq!(
        pick_farthest(&tied, &nothing_seen, origin),
        Some(Cell::new(4, 1)),
    );
}

/// §7.5: when every cell in reach has been looked at, the inspected-cell memory
/// is wiped and the sweep starts over — a Calm guard never runs out of ground.
#[test]
fn patrol_memory_wipes_when_the_territory_is_exhausted() {
    let facility = Facility::walled_box(5, 5); // a 3×3 interior
    let mut guard = Guard::patrolling(Cell::new(2, 2)).with_beat(open_beat(5, 5));
    // The guard has looked at its whole territory: fold a full-circle view in.
    let whole_room = field_of_view(
        &facility,
        Cell::new(2, 2),
        Direction::South,
        FULL_SIGHT_ARC,
        2,
    );
    guard.inspected.absorb(&whole_room);

    let territory = guard.territory(&facility, PatrolStyle::Beat);
    assert!(
        pick_farthest(&territory, &guard.inspected, guard.pos()).is_none(),
        "precondition: nothing is left uninspected",
    );

    // Asking for the next target wipes the exhausted memory and finds one again.
    assert!(
        {
            let territory = guard.territory(&facility, PatrolStyle::Beat);
            guard
                .next_target_in(&territory, PatrolStyle::Beat, &mut Rng::new(0))
                .is_some()
        },
        "the sweep restarts instead of stalling",
    );
    assert!(
        pick_farthest(
            &guard.territory(&facility, PatrolStyle::Beat),
            &guard.inspected,
            guard.pos()
        )
        .is_some(),
        "memory was wiped — cells read as uninspected again",
    );
}

/// §7.5/§153: a Calm guard that reaches a patrol target dwells in place for a
/// bounded window — holding, position and facing unchanged (§5) — then resumes
/// the sweep. Forced on (`dwell_chance` 100) with a fixed seed so the length is
/// deterministic; it must land inside the [START] range and the guard must move
/// again once it elapses.
#[test]
fn a_calm_guard_dwells_on_arrival_then_resumes() {
    let facility = Facility::walled_box(9, 9);
    // Already standing on its patrol target (destination == pos): the fixture
    // for "just arrived", the moment a dwell is rolled. (A fresh guard with no
    // target picks one and walks without pausing.)
    let mut guard =
        Guard::patrolling_to(Cell::new(4, 4), Cell::new(4, 4)).with_beat(open_beat(9, 9));
    guard.look(&facility, GuardSight::REAR_CARVE);
    let (start, facing) = (guard.pos(), guard.facing());
    let mut rng = Rng::new(7);

    // On arrival, with the chance forced to 100, it begins a dwell rather than
    // immediately picking the next target.
    let first = guard.decide(
        &facility,
        &[],
        &mut rng,
        Dwell::CALM,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    );
    assert!(
        first.is_none() && guard.is_dwelling(),
        "reaching a target begins a dwell",
    );

    // Hold for the rest of the window: each Calm turn holds, unmoved and
    // un-re-aimed, until the dwell elapses and the guard steps off.
    let mut holds = 1;
    loop {
        let step = guard.decide(
            &facility,
            &[],
            &mut rng,
            Dwell::CALM,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE,
        );
        if !guard.is_dwelling() {
            // The dwell has ended and the sweep resumes: the guard is active
            // again. It may first spend a turn or two rotating toward its new
            // heading (§229) before it steps, but a step lands within a couple of
            // turns — never a permanent hold.
            let mut resumed = step.is_some();
            for _ in 0..3 {
                if resumed {
                    break;
                }
                resumed = guard
                    .decide(
                        &facility,
                        &[],
                        &mut rng,
                        Dwell::CALM,
                        PatrolStyle::Beat,
                        GuardSight::REAR_CARVE,
                    )
                    .is_some();
            }
            assert!(resumed, "the sweep resumes once the dwell ends");
            break;
        }
        assert_eq!(step, None, "a dwelling guard holds");
        assert_eq!(guard.pos(), start, "a dwell does not move");
        assert_eq!(guard.facing(), facing, "a dwell does not re-aim (§5)");
        holds += 1;
    }
    assert_eq!(
        (GUARD_DWELL_TURNS_MIN, GUARD_DWELL_TURNS_MAX),
        (3, 7),
        "the [START] dwell range",
    );
    assert!(
            (GUARD_DWELL_TURNS_MIN..=GUARD_DWELL_TURNS_MAX).contains(&holds),
            "the dwell lasted {holds} turns, outside the [START] {GUARD_DWELL_TURNS_MIN}..={GUARD_DWELL_TURNS_MAX} range",
        );
}

/// The pause a player actually *sees* (§7.5/§153), at the shipped
/// [`GUARD_DWELL_CHANCE_PERCENT`] rather than a forced 100: **every** Calm
/// arrival pauses, and the pause is the whole [START] window — never the two
/// turns a 180° about-face costs on its own.
///
/// The measured symptom this pins against. Over twelve seeded runs of the real
/// generator, **92% of every stationary spell a patrolling guard took was one or
/// two turns** — [`commit_step`](Guard::commit_step)'s slow 90° turn and the
/// two-rotation reversal — and 42% of the two-turn ones were immediately
/// followed by the guard walking back the way it came. "Reach the end of the
/// corridor, spin, come straight back" was the patrol's whole visible rhythm,
/// and the 3–5 turn dwell, firing on under 8% of stops, was lost inside it.
/// Sweeping the seed here is the point: a single lucky draw proves nothing about
/// a behaviour that used to be a coin flip.
#[test]
fn every_calm_arrival_pauses_for_the_whole_dwell_window() {
    let facility = Facility::walled_box(9, 9);
    for seed in 0..64u64 {
        // Standing on its own target: the "just arrived" fixture.
        let mut guard = Guard::patrolling_to(Cell::new(4, 4), Cell::new(4, 4));
        guard.look(&facility, GuardSight::REAR_CARVE);
        let (start, facing) = (guard.pos(), guard.facing());
        let mut rng = Rng::new(seed);
        let chance = GUARD_DWELL_CHANCE_PERCENT;

        let first = guard.decide(
            &facility,
            &[],
            &mut rng,
            Dwell::with_chance(chance),
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE,
        );
        assert!(
            first.is_none() && guard.is_dwelling(),
            "seed {seed}: an arrival must pause, not pick the next target and walk",
        );

        let mut holds = 1;
        loop {
            let step = guard.decide(
                &facility,
                &[],
                &mut rng,
                Dwell::with_chance(chance),
                PatrolStyle::Beat,
                GuardSight::REAR_CARVE,
            );
            if !guard.is_dwelling() {
                break; // the window elapsed and the sweep resumed this turn
            }
            assert_eq!(step, None, "seed {seed}: a dwelling guard holds");
            assert_eq!(guard.pos(), start, "seed {seed}: a dwell does not move");
            assert_eq!(guard.facing(), facing, "seed {seed}: no re-aim (§5)");
            holds += 1;
        }
        assert!(
            (GUARD_DWELL_TURNS_MIN..=GUARD_DWELL_TURNS_MAX).contains(&holds),
            "seed {seed}: held {holds} turns, outside the [START] \
                 {GUARD_DWELL_TURNS_MIN}..={GUARD_DWELL_TURNS_MAX} range",
        );
    }
}

/// §153: a detection cancels an in-progress dwell the same turn — a reactive
/// guard never pauses (§7.1). The guard is dwelling; a sighting flips it to
/// Chasing via `sense`, and the next `decide` chases instead of holding the
/// dwell out.
#[test]
fn a_detection_cancels_an_in_progress_dwell() {
    let facility = Facility::walled_box(15, 15);
    // Arrived at its target (faces south, §7.1), so the first decide dwells.
    let mut guard = Guard::patrolling_to(Cell::new(7, 2), Cell::new(7, 2));
    guard.look(&facility, GuardSight::REAR_CARVE);
    let mut rng = Rng::new(3);

    guard.decide(
        &facility,
        &[],
        &mut rng,
        Dwell::CALM,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    );
    assert!(guard.is_dwelling(), "precondition: dwelling");

    // A player appears down the cone (certain zone): the guard turns reactive.
    let player = Cell::new(7, 5);
    assert!(guard.fov().contains(player), "precondition: in the cone");
    guard.sense(player, false, GuardSight::BASELINE);
    assert_eq!(guard.state(), GuardState::Chasing);

    // The next decision clears the dwell and steps toward the player.
    let step = guard.decide(
        &facility,
        &[],
        &mut rng,
        Dwell::CALM,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    );
    assert!(!guard.is_dwelling(), "going reactive cancels the dwell");
    assert!(step.is_some(), "a chasing guard moves, it does not dwell");
}

/// §7.5 slow turn (#229): a **Calm** guard that must change heading by 90° spends
/// a turn rotating in place — position unchanged, the cone re-aimed the new way —
/// before it steps, and continuing straight pays nothing. The first decide turns
/// (returns no step, facing swung east, still on its cell, its cone now honestly
/// facing east); the second, now aligned, steps.
#[test]
fn a_calm_guard_spends_a_turn_rotating_before_a_quarter_turn() {
    let facility = Facility::walled_box(12, 12);
    // Faces south (§7.1); its patrol target is due east, a 90° turn away.
    let mut guard = Guard::patrolling_to(Cell::new(5, 5), Cell::new(8, 5));
    guard.look(&facility, GuardSight::REAR_CARVE);
    assert!(
        guard.fov().contains(Cell::new(5, 7)) && !guard.fov().contains(Cell::new(7, 5)),
        "precondition: the cone starts facing south",
    );

    // Turn one: rotate in place. No step, unmoved, but the cone now faces east —
    // the overlay a frame paints is honest about the mid-turn facing (§11.5).
    let first = guard.decide(
        &facility,
        &[],
        &mut Rng::new(0),
        Dwell::NEVER,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    );
    assert_eq!(first, None, "the quarter-turn spends the whole turn");
    assert_eq!(
        guard.pos(),
        Cell::new(5, 5),
        "a turn in place does not move"
    );
    assert_eq!(guard.facing(), Direction::East, "it swung a quarter east");
    assert!(
        guard.fov().contains(Cell::new(7, 5)) && !guard.fov().contains(Cell::new(5, 7)),
        "the cone re-aimed east at once",
    );

    // Turn two: now aligned, it steps — straight ahead costs nothing.
    let second = guard.decide(
        &facility,
        &[],
        &mut Rng::new(0),
        Dwell::NEVER,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    );
    assert_eq!(
        second,
        Some(Direction::East),
        "aligned, it walks the new way"
    );
}

/// §7.1/§7.5 (#229): a **Calm** guard continuing straight ahead pays no turn tax —
/// its first decision on a target dead ahead is the step itself, no rotation.
#[test]
fn a_calm_guard_walking_straight_pays_no_turn_tax() {
    let facility = Facility::walled_box(12, 12);
    // Faces south (§7.1); the target is due south — straight ahead.
    let mut guard = Guard::patrolling_to(Cell::new(5, 5), Cell::new(5, 9));
    guard.look(&facility, GuardSight::REAR_CARVE);
    assert_eq!(
        guard.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE
        ),
        Some(Direction::South),
        "a guard already facing its heading steps at once",
    );
}

/// §7.1/§7.6 (#229): a **reactive** guard turns fast — no turn tax. A Responding
/// guard (any reactive state) heading 90° off its facing steps immediately,
/// re-aiming as it goes, where a Calm guard on the same line would first rotate in
/// place. A hunt is never slowed.
#[test]
fn a_reactive_guard_turns_without_a_tax() {
    let facility = Facility::walled_box(12, 12);
    let post = Cell::new(8, 5); // due east, a 90° turn from the south spawn facing

    // Calm on this line first rotates in place (no step).
    let mut calm = Guard::patrolling_to(Cell::new(5, 5), post);
    calm.look(&facility, GuardSight::REAR_CARVE);
    assert_eq!(
        calm.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE
        ),
        None,
        "the Calm guard spends the turn rotating",
    );

    // Reactive on the same line steps at once — the fast turn re-aims with the step.
    let mut reactive = Guard::patrolling(Cell::new(5, 5));
    reactive.look(&facility, GuardSight::REAR_CARVE);
    reactive.respond_to(post); // Responding, walking to the post, lead warm
    assert_eq!(
        reactive.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE
        ),
        Some(Direction::East),
        "a reactive guard turns fast and steps the same turn",
    );
}

/// §7.3/§7.6/§7.7: a responder does not merely *stand* on the cell it was called
/// to — on arrival it opens the bounded §7.6 sweep, the same one a lost chase
/// ends in. A call is a lead whose trail is already cold, so without this every
/// call in the game would resolve as a walk rather than a hunt.
#[test]
fn a_responder_searches_the_cell_it_was_called_to() {
    let facility = Facility::walled_box(12, 12);
    let called_to = Cell::new(5, 9);
    let mut guard = Guard::patrolling(Cell::new(5, 5));
    guard.look(&facility, GuardSight::REAR_CARVE);
    guard.respond_to(called_to);

    // Walk it in — `decide` returns the heading, the loop applies it (§4.2), so
    // the test moves the guard itself. The lead ([`ALERT_DURATION`]) far
    // outlasts the four steps.
    for _ in 0..8 {
        if guard.pos == called_to {
            break;
        }
        let step = guard
            .decide(
                &facility,
                &[],
                &mut Rng::new(0),
                Dwell::NEVER,
                PatrolStyle::Beat,
                GuardSight::REAR_CARVE,
            )
            .expect("the responder is still walking");
        let next = guard.pos.step(step).expect("in bounds");
        guard.advance_to(next, step, &facility, GuardSight::REAR_CARVE);
    }
    assert_eq!(guard.pos, called_to, "it reached the cell it was called to");
    assert_eq!(
        guard.state,
        GuardState::Responding,
        "still on the errand until the arriving turn resolves",
    );

    // The turn it has nowhere further to walk, the errand becomes a search.
    guard.decide(
        &facility,
        &[],
        &mut Rng::new(0),
        Dwell::NEVER,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    );
    assert_eq!(guard.state, GuardState::Alerted, "arrival opens a search");
    assert_eq!(guard.search, SEARCH_DURATION);
    assert_eq!(
        guard.focus,
        Some(called_to),
        "the sweep centres on the called cell",
    );
}

/// One whole guard turn against a player it cannot possibly see: the §4.2 phase-3
/// sense pass (which cools every reactive timer) followed by the decision, with any
/// step applied as the turn loop would. Returns the direction the guard committed
/// to, so a caller can tell a step from a hold.
///
/// The lead tests **must** go through this rather than calling `decide` alone: the
/// cooling they are about happens in [`sense`](Guard::sense), so a fixture that
/// skips it is measuring nothing.
fn take_turn(guard: &mut Guard, facility: &Facility, blocked: &[Cell]) -> Option<Direction> {
    // Concealed from everyone, so no sighting can refresh the lead under the test.
    guard.sense(Cell::new(0, 0), true, GuardSight::BASELINE);
    let step = guard.decide(
        facility,
        blocked,
        &mut Rng::new(0),
        Dwell::NEVER,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    );
    if let Some(dir) = step {
        if let Some(next) = guard.pos.step(dir) {
            guard.advance_to(next, dir, facility, GuardSight::REAR_CARVE);
        }
    }
    step
}

/// #410 — **the tail case, the manoeuvre the experiment exists for.** You walk in
/// a patrol's blind spot; it reaches a corner and turns 90°. Baseline that puts
/// you at its side (§6.2 tier 3), which detects, so tailing a guard is impossible
/// — the one manoeuvre that should be the reward for reading a patrol. Under
/// [`GuardSight::BASELINE`] the turn no longer catches you.
///
/// Both arms in one test, because the claim is a *difference*: the same scene, the
/// same turn, one knob.
#[test]
fn a_ninety_degree_turn_catches_a_tail_only_in_the_control_arm() {
    let facility = Facility::walled_box(11, 11);
    let tail = Cell::new(4, 5); // directly behind an east-facing guard

    let detected_after_turning = |sight: GuardSight| {
        let mut guard = Guard::stationary(Cell::new(5, 5));
        guard.facing = Direction::East;
        assert_eq!(guard.state, GuardState::Calm, "a patrol, not a hunt");
        guard.look(&facility, sight);
        guard.sense(tail, false, sight);
        assert!(
            !guard.detected_player(),
            "{sight:?}: precondition — directly behind is blind in both arms (§155)",
        );

        // The corner: a quarter turn, position unchanged. The tail is now at the
        // guard's flank.
        guard.turn_in_place(Direction::North, &facility, sight);
        guard.sense(tail, false, sight);
        guard.detected_player()
    };

    assert!(
        detected_after_turning(GuardSight::REAR_CARVE),
        "control: the turn brings the tail to tier 3, which detects (§6.1/§7.2)",
    );
    assert!(
        !detected_after_turning(GuardSight::BASELINE),
        "experiment: a calm guard detects exactly its cone, so the tail survives",
    );
}

/// #410 — **the narrowing, and the whole reason the experiment is conditional.**
/// A guard that is *not* Calm watches its sides again, in both arms. Chasing,
/// investigating, searching or answering a call, tier 3 detects exactly as it
/// always did.
///
/// This is what prices the gift. The unconditional form handed avoidance-first
/// play a win rate rise with no new decision attached — un-priced safety, which is
/// what §7.2 means when it says the takedown's constraints *are* the cost. With
/// the condition, the flank is somewhere to **work from** against a patrol you
/// have read, and never somewhere to **hide** from a guard that is hunting you.
#[test]
fn an_alerted_guard_watches_its_flanks_in_both_arms() {
    let facility = Facility::walled_box(11, 11);
    let flanker = Cell::new(5, 4); // due north of a guard facing east — its flank

    for state in [
        GuardState::Chasing,
        GuardState::Investigating,
        GuardState::Alerted,
        GuardState::Responding,
    ] {
        for sight in [GuardSight::REAR_CARVE, GuardSight::BASELINE] {
            let mut guard = Guard::stationary(Cell::new(5, 5)).with_state(state);
            guard.facing = Direction::East;
            guard.look(&facility, sight);
            guard.sense(flanker, false, sight);
            assert!(
                guard.detected_player(),
                "{state:?} under {sight:?}: a guard that is not calm watches its sides",
            );
        }
    }

    // ...and the same guard, Calm, does not — so the difference is the mood and
    // nothing else about the scene.
    let mut calm = Guard::stationary(Cell::new(5, 5));
    calm.facing = Direction::East;
    calm.look(&facility, GuardSight::BASELINE);
    calm.sense(flanker, false, GuardSight::BASELINE);
    assert!(
        !calm.detected_player(),
        "calm, same cell, same facing, same arm: the flank is blind",
    );
}

/// #410: a **180°** turn still catches you, in both arms. The experiment widens
/// the blind spot from three cells to five; it does not make a guard blind to what
/// it turns to face. Following directly behind a guard that reverses leaves you at
/// tier 1 — dead ahead — which no arm forgives.
#[test]
fn an_about_face_still_catches_a_tail_in_both_arms() {
    let facility = Facility::walled_box(11, 11);
    let tail = Cell::new(4, 5);
    for sight in [GuardSight::REAR_CARVE, GuardSight::BASELINE] {
        let mut guard = Guard::stationary(Cell::new(5, 5));
        guard.facing = Direction::East;
        guard.look(&facility, sight);
        guard.sense(tail, false, sight);
        assert!(!guard.detected_player(), "{sight:?}: behind is blind");

        guard.turn_in_place(Direction::West, &facility, sight);
        guard.sense(tail, false, sight);
        assert!(
            guard.detected_player(),
            "{sight:?}: turning to face the tail detects it — tier 1, dead ahead",
        );
    }
}

/// §7.3/#409 — **the fix**: a dispatch to a cell far beyond [`ALERT_DURATION`]
/// steps still *arrives* and opens its §7.6 search there. The lead bounds the
/// investigation, not the commute; before this, the further from the patrols you
/// struck, the less the radio cost you, because the responder's lead ran out on the
/// road and it stood down having looked at nothing.
#[test]
fn a_responder_arrives_however_far_the_call_is() {
    let facility = Facility::walled_box(40, 40);
    let called_to = Cell::new(38, 38);
    let mut guard = Guard::patrolling(Cell::new(1, 1));
    guard.look(&facility, GuardSight::REAR_CARVE);
    guard.respond_to(called_to);

    let journey = guard.pos.manhattan_distance(called_to);
    assert!(
        journey > ALERT_DURATION,
        "the fixture must out-walk the lead, or it tests nothing: \
             {journey} steps against a lead of {ALERT_DURATION}",
    );

    // Generous cap: the walk plus the turns spent rotating onto each heading.
    for _ in 0..journey * 2 {
        if guard.pos == called_to {
            break;
        }
        take_turn(&mut guard, &facility, &[]);
        assert_eq!(
            guard.state,
            GuardState::Responding,
            "it must not stand down on the road (§7.3)",
        );
    }
    assert_eq!(guard.pos, called_to, "it walked the whole way");

    take_turn(&mut guard, &facility, &[]);
    assert_eq!(
        guard.state,
        GuardState::Alerted,
        "arrival opens the §7.6 search the call was always for",
    );
    assert_eq!(guard.focus, Some(called_to), "centred on the called cell");
    assert_eq!(guard.search, SEARCH_DURATION);
}

/// §7.6/§7.8: the backstops survive the freeze. A responder that cannot get there
/// — the destination is sealed off, or a colleague holds the only way through —
/// **does** burn its lead and stands down cleanly. The freeze is conditioned on a
/// route existing this turn, not on the state, so nothing paces forever.
#[test]
fn a_responder_that_cannot_travel_still_cools_out() {
    // A room split by a solid wall, with the call on the far side of it.
    let mut facility = Facility::walled_box(12, 12);
    for y in 0..12 {
        facility.set_terrain(6, y, Terrain::Wall);
    }
    let mut stranded = Guard::patrolling(Cell::new(2, 2));
    stranded.look(&facility, GuardSight::REAR_CARVE);
    stranded.respond_to(Cell::new(9, 9));
    for _ in 0..ALERT_DURATION {
        assert_eq!(stranded.state, GuardState::Responding, "still trying");
        take_turn(&mut stranded, &facility, &[]);
    }
    assert_eq!(
        stranded.state,
        GuardState::Calm,
        "an unreachable call is given up, not paced forever (§7.6)",
    );

    // A one-cell corridor with a colleague standing in it: the route exists on the
    // board but is blocked every turn (§7.8), so the lead cools just the same.
    let mut facility = Facility::walled_box(5, 12);
    for x in 1..4 {
        facility.set_terrain(x, 6, Terrain::Wall);
    }
    facility.set_terrain(2, 6, Terrain::Floor); // the one gap
    let colleague = Cell::new(2, 6);
    let mut held = Guard::patrolling(Cell::new(2, 4));
    held.look(&facility, GuardSight::REAR_CARVE);
    held.respond_to(Cell::new(2, 9));
    for _ in 0..ALERT_DURATION {
        let step = take_turn(&mut held, &facility, &[colleague]);
        assert_eq!(step, None, "the gap is sealed, so it holds (§7.8)");
    }
    assert_eq!(
        held.state,
        GuardState::Calm,
        "a permanently blocked responder cools out rather than holding forever",
    );
}

/// §7.6 — the **anti-tracking-turret backstop is untouched**. A chase's
/// destination follows the player, so freezing *its* lead would rebuild the
/// un-outrunnable pursuer the design exists to avoid. Only the `Responding` arm
/// changed: a chasing guard's lead cools every turn it steps, exactly as before,
/// and runs out on schedule.
#[test]
fn a_chase_still_burns_its_lead_while_it_steps() {
    let facility = Facility::walled_box(40, 40);
    let mut guard = Guard::patrolling(Cell::new(1, 1));
    guard.look(&facility, GuardSight::REAR_CARVE);
    guard.state = GuardState::Chasing;
    guard.destination = Some(Cell::new(38, 38));
    guard.alert = ALERT_DURATION;
    guard.last_seen = Some(Cell::new(38, 38));

    let mut stepped = 0;
    for _ in 0..ALERT_DURATION {
        assert_eq!(guard.state, GuardState::Chasing, "still on the lead");
        if take_turn(&mut guard, &facility, &[]).is_some() {
            stepped += 1;
        }
    }
    assert!(stepped > 0, "the chaser really was walking, not stuck");
    assert_eq!(
        guard.state,
        GuardState::Calm,
        "the lead went cold on the road and the chase was given up (§7.6)",
    );
    assert!(
        guard.pos.manhattan_distance(Cell::new(38, 38)) > 0,
        "it never got there — which is the point of the backstop",
    );
}

/// §7.7: a call carries its own cell and inherits nobody's memory. A guard that
/// glimpsed the player earlier must not drag that stale sighting into the search
/// it opens on arrival — it searches where it was *sent*, not where it once
/// thought you were.
#[test]
fn a_call_drops_the_responders_stale_sighting() {
    let facility = Facility::walled_box(12, 12);
    let stale = Cell::new(2, 2);
    let called_to = Cell::new(5, 9);

    let mut guard = Guard::patrolling(Cell::new(5, 5));
    guard.look(&facility, GuardSight::REAR_CARVE);
    guard.last_seen = Some(stale); // it saw the player over there, a while ago
    guard.respond_to(called_to);
    assert_eq!(guard.last_seen, None, "the call clears the old sighting");

    for _ in 0..8 {
        if guard.pos == called_to {
            break;
        }
        let step = guard
            .decide(
                &facility,
                &[],
                &mut Rng::new(0),
                Dwell::NEVER,
                PatrolStyle::Beat,
                GuardSight::REAR_CARVE,
            )
            .expect("walking");
        let next = guard.pos.step(step).expect("in bounds");
        guard.advance_to(next, step, &facility, GuardSight::REAR_CARVE);
    }
    guard.decide(
        &facility,
        &[],
        &mut Rng::new(0),
        Dwell::NEVER,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    ); // arrive → search
    assert_eq!(
        guard.focus,
        Some(called_to),
        "the search centres on the call, not the stale sighting at {stale:?}",
    );
}

/// §7.2 (#229): **no guard, in any state, flips 180° in one move** — a reversal
/// passes through an intermediate quarter, always the clockwise one (§12.4), and
/// takes ≥2 turns to face fully about. This is the load-bearing half: a guard
/// cannot spin to face — and detect — a player lined up directly behind it.
#[test]
fn no_guard_flips_180_in_one_move() {
    let facility = Facility::walled_box(12, 12);

    // Reactive reversal: south-facing Responder sent due north. It never returns
    // North on the first move; it rotates a clockwise quarter (west), then — now
    // 90° off — turns fast and steps north. Two turns, through the quarter.
    let mut reactive = Guard::patrolling(Cell::new(5, 5));
    reactive.look(&facility, GuardSight::REAR_CARVE);
    reactive.respond_to(Cell::new(5, 1)); // due north — a 180° reversal
    assert_eq!(
        reactive.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE
        ),
        None,
        "a reactive guard cannot half-turn in one move",
    );
    assert_eq!(
        reactive.facing(),
        Direction::West,
        "it rotated through the fixed clockwise quarter, not straight about",
    );
    assert_eq!(
        reactive.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE
        ),
        Some(Direction::North),
        "now a quarter off, the fast turn steps north",
    );

    // Calm reversal: same about-face costs more — a rotation to the clockwise
    // quarter (west), another to north, then the step. It faces north only on the
    // second rotation, never in one move.
    let mut calm = Guard::patrolling_to(Cell::new(5, 5), Cell::new(5, 1));
    calm.look(&facility, GuardSight::REAR_CARVE);
    assert_eq!(
        calm.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE
        ),
        None
    );
    assert_eq!(
        calm.facing(),
        Direction::West,
        "first the clockwise quarter"
    );
    assert_eq!(
        calm.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE
        ),
        None
    );
    assert_eq!(calm.facing(), Direction::North, "then the second quarter");
    assert_eq!(
        calm.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE
        ),
        Some(Direction::North),
        "aligned at last, it steps",
    );
}

/// §7.5 (#229), the dwell mirror: the moment a Calm guard turns reactive its
/// slow-turn tax is simply gone — a detection never waits on a pending rotation.
/// A guard mid-corner (already rotated a quarter east, still Calm) is dispatched
/// on a fresh 90° heading; a Calm guard would rotate again, but the reactive one
/// steps at once.
#[test]
fn going_reactive_drops_a_pending_slow_turn() {
    let facility = Facility::walled_box(12, 12);
    // Rotate in place once: south spawn facing, target due east — the Calm quarter.
    let mut guard = Guard::patrolling_to(Cell::new(5, 5), Cell::new(8, 5));
    guard.look(&facility, GuardSight::REAR_CARVE);
    assert_eq!(
        guard.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE
        ),
        None
    );
    assert_eq!(guard.facing(), Direction::East, "mid-corner, facing east");

    // Now dispatched due north — 90° off the current east facing. A Calm guard
    // would spend another turn rotating; going reactive drops the tax and steps.
    guard.respond_to(Cell::new(5, 1));
    assert_eq!(
        guard.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE
        ),
        Some(Direction::North),
        "the reactive dispatch steps at once — no pending rotation to wait out",
    );
}

/// §7.5 (#229) liveness: the turn tax delays a corner by exactly one turn, it
/// never freezes the sweep. A Calm guard rounding a 90° corner reaches its target
/// after a single rotation plus the walk — one held turn, then steady progress.
#[test]
fn the_slow_turn_delays_a_corner_it_does_not_freeze_it() {
    let facility = Facility::walled_box(12, 12);
    let target = Cell::new(9, 2);
    let mut guard = Guard::patrolling_to(Cell::new(2, 2), target); // due east, a turn
    guard.look(&facility, GuardSight::REAR_CARVE);

    let mut rotations = 0;
    let mut steps = 0;
    for _ in 0..20 {
        if guard.pos() == target {
            break;
        }
        match guard.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE,
        ) {
            Some(dir) => {
                let dest = guard.pos().step(dir).expect("interior step");
                guard.advance_to(dest, dir, &facility, GuardSight::REAR_CARVE);
                steps += 1;
            }
            None => rotations += 1,
        }
    }
    assert_eq!(guard.pos(), target, "the guard reaches its target");
    assert_eq!(
        rotations, 1,
        "exactly one turn is spent rotating at the corner"
    );
    assert_eq!(steps, 7, "then it walks the seven cells east");
}

/// §7.6 fix 2 (Lost → Hunted → Released): a reactive guard that reaches its
/// last-known cell and finds nothing does **not** snap back to patrol — it enters a
/// bounded [`Alerted`](GuardState::Alerted) search, sweeps for exactly
/// [`SEARCH_DURATION`] turns, and only then releases to Calm. Driven by sight (§9
/// **[SETTLED]**): a glimpse sends the guard Investigating, and once standing on the
/// lead with nothing more seen the search begins.
#[test]
fn a_lost_lead_searches_then_releases_to_patrol() {
    let facility = Facility::walled_box(15, 15);
    let mut guard = Guard::patrolling(Cell::new(7, 2)); // faces south (§7.1)
    guard.look(&facility, GuardSight::REAR_CARVE);
    let glimpse = Cell::new(7, 9); // down the cone: the glimpse zone
    assert!(guard.fov().contains(glimpse), "precondition: in the cone");

    guard.sense(glimpse, false, GuardSight::BASELINE);
    assert_eq!(guard.state(), GuardState::Investigating);

    // Arrive at the lead with nothing more seen: the search begins, not patrol.
    guard.advance_to(glimpse, Direction::South, &facility, GuardSight::REAR_CARVE);
    guard.decide(
        &facility,
        &[],
        &mut Rng::new(0),
        Dwell::NEVER,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    );
    assert_eq!(
        guard.state(),
        GuardState::Alerted,
        "arrival begins a bounded search, not an instant give-up",
    );

    // Wait the search out (player concealed nearby — nothing seen). It stays
    // Alerted for SEARCH_DURATION turns, then releases to Calm.
    let mut alerted_turns = 0u32;
    for _ in 0..SEARCH_DURATION + 2 {
        guard.sense(glimpse, true, GuardSight::BASELINE);
        if guard.state() == GuardState::Alerted {
            alerted_turns += 1;
        }
        guard.decide(
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::REAR_CARVE,
        );
    }
    assert_eq!(
        alerted_turns, SEARCH_DURATION,
        "the search lasts exactly SEARCH_DURATION turns",
    );
    assert_eq!(
        guard.state(),
        GuardState::Calm,
        "the search releases back to patrol",
    );
}

/// §7.6 search **[START]** pins: the search duration and its radii, and the
/// released-watch window, are named constants a later tune must move deliberately.
#[test]
fn the_search_constants_are_pinned() {
    assert_eq!(SEARCH_DURATION, 12, "the [START] search duration");
    assert_eq!(SEARCH_RADIUS, 4, "the [START] search radius");
    assert_eq!(WATCH_DURATION, 20, "the [START] released-watch window");
    assert_eq!(WATCH_RADIUS, 8, "the [START] watch radius");
    assert_eq!(
        GUARD_CLOSE_CHANCE_PERCENT, 25,
        "the [START] guard close-behind chance",
    );
}

/// §10.4/§7.6: only a Calm guard closes a door behind itself — a hunting guard
/// (chasing, investigating, searching, responding) never pauses to tidy up, so
/// the door it opened stays a lasting sightline.
#[test]
fn only_calm_guards_close_doors() {
    let calm = Guard::patrolling(Cell::new(1, 1));
    assert!(calm.closes_doors(), "a Calm guard closes behind itself");
    for hunting in [
        GuardState::Alerted,
        GuardState::Chasing,
        GuardState::Investigating,
        GuardState::Responding,
    ] {
        assert!(
            !Guard::patrolling(Cell::new(1, 1))
                .with_state(hunting)
                .closes_doors(),
            "a {hunting:?} guard never closes doors",
        );
    }
}

/// §7.6 two-zone detection **[START]**: the boundaries and the alert duration are
/// pinned so a later change is a visible edit, and the glimpse edge is exactly the
/// cone's own range — past it there is no cone to be seen in.
#[test]
fn the_detection_zones_and_alert_are_pinned() {
    let baseline = GuardSight::BASELINE;
    assert_eq!(baseline.certain_range(), 5, "the [START] certain zone");
    assert_eq!(
        baseline.glimpse_range(),
        10,
        "the [START] glimpse-zone edge"
    );
    assert_eq!(ALERT_DURATION, 30, "the [START] alert duration");
    assert_eq!(
        baseline.glimpse_range(),
        GUARD_SIGHT_RANGE,
        "the glimpse edge is the cone's own range",
    );
}

/// §7.6 certain zone: a player seen inside it flips the guard to
/// Chasing its **live** cell and refreshes the alert timer. The last-known-precise
/// cell is recorded for a later glimpse to fall back on.
#[test]
fn a_player_in_the_certain_zone_is_chased_at_its_live_cell() {
    let facility = Facility::walled_box(11, 11);
    let mut guard = Guard::stationary(Cell::new(5, 3)); // faces south (§7.1)
    guard.look(&facility, GuardSight::REAR_CARVE);
    let player = Cell::new(5, 7); // 4 cells down the cone: certain
    assert!(guard.fov.contains(player), "precondition: in the cone");

    guard.see(player, false, GuardSight::BASELINE);
    assert_eq!(guard.state(), GuardState::Chasing);
    assert_eq!(guard.destination, Some(player), "tracks the live cell");
    assert_eq!(guard.last_seen, Some(player), "records the certain cell");
    assert_eq!(guard.alert, ALERT_DURATION);
}

/// §7.6 glimpse zone: past the certain zone but within the cone's range the guard
/// only catches imprecise movement, so it Investigates toward where it *last knew*
/// the player — the certain cell — not the imprecise glimpse itself.
#[test]
fn a_glimpse_investigates_toward_the_last_certain_cell() {
    let facility = Facility::walled_box(11, 13);
    let mut guard = Guard::stationary(Cell::new(5, 2)); // faces south
    guard.look(&facility, GuardSight::REAR_CARVE);
    let certain = Cell::new(5, 6); // 4 down: certain — sets the precise memory
    let glimpse = Cell::new(5, 10); // 8 down: glimpse
    assert!(guard.fov.contains(glimpse), "precondition: in the cone");

    guard.see(certain, false, GuardSight::BASELINE);
    assert_eq!(guard.last_seen, Some(certain));

    guard.see(glimpse, false, GuardSight::BASELINE);
    assert_eq!(guard.state(), GuardState::Investigating);
    assert_eq!(
        guard.destination,
        Some(certain),
        "heads for where it last knew you, not the glimpse",
    );
    assert_eq!(guard.alert, ALERT_DURATION);
}

/// §10.3/§7.6: a concealed player — in a cupboard, or ducked behind the right
/// table — is not detected by sight even standing in the cone. This is the AND-in
/// the danger overlay already honours (§11.5), carried into the guard's mind.
#[test]
fn a_concealed_player_in_the_cone_is_not_seen() {
    let facility = Facility::walled_box(11, 11);
    let mut guard = Guard::stationary(Cell::new(5, 3));
    guard.look(&facility, GuardSight::REAR_CARVE);
    let player = Cell::new(5, 7);
    assert!(guard.fov.contains(player), "precondition: in the cone");

    guard.see(player, true, GuardSight::BASELINE); // concealed from this guard
    assert_eq!(
        guard.state(),
        GuardState::Calm,
        "concealment blocks detection"
    );
    assert_eq!(guard.destination, None);
    assert_eq!(guard.alert, 0);
}

/// §7.6 "gone" zone: beyond the cone's range there is no cone to be seen in, so a
/// player past the guard's range is simply not in its FOV and detection does
/// nothing this turn.
#[test]
fn a_player_beyond_the_glimpse_range_is_not_seen() {
    let facility = Facility::walled_box(11, 20);
    let mut guard = Guard::stationary(Cell::new(5, 2));
    guard.look(&facility, GuardSight::REAR_CARVE);
    let far = Cell::new(5, 2 + GuardSight::BASELINE.glimpse_range() + 1); // one past the cone
    assert!(!guard.fov.contains(far), "precondition: out of the cone");

    guard.see(far, false, GuardSight::BASELINE);
    assert_eq!(guard.state(), GuardState::Calm, "> 10 detects nothing");
}

/// §7.6/§12.6/#495: **the two zones survive a shortened cone**, which is the whole of
/// the zone decision the modifier had to make. The zones are the cone's own halves, so
/// a shorter cone keeps a certain zone, a glimpse ring outside it and a gone zone
/// beyond — all three rungs, on a cone barely over half the length.
///
/// The failure it guards against is the one a fixed `CERTAIN_RANGE` of 5 would have
/// produced against a range of 6: a single glimpse ring, and every other sighting
/// certain. That would leave an **easier** modifier biting harder on contact than the
/// baseline, which is the one thing a −N draw must never hand back.
#[test]
fn a_narrowed_cone_keeps_both_detection_zones() {
    let sight = GuardSight::NARROWED;
    assert!(sight.range < GuardSight::BASELINE.range, "shorter");
    assert_eq!(
        sight.arc,
        GuardSight::BASELINE.arc,
        "…and the wedge is §7.1's own: this modifier moves the reach, not the arc",
    );

    let facility = Facility::walled_box(11, 20);
    // Straight down the cone's axis, so nothing but the range and the zones can be
    // what excludes a cell.
    let at = |depth: u32| Cell::new(5, 2 + depth);
    let look = || {
        let mut guard = Guard::stationary(Cell::new(5, 2)); // faces south (§7.1)
        guard.look(&facility, sight);
        guard
    };

    let mut certain = look();
    certain.see(at(sight.certain_range()), false, sight);
    assert_eq!(
        certain.state(),
        GuardState::Chasing,
        "the inner half is still a certain track",
    );

    let mut glimpse = look();
    let edge = at(sight.glimpse_range());
    assert!(glimpse.fov.contains(edge), "precondition: in the cone");
    glimpse.see(edge, false, sight);
    assert_eq!(
        glimpse.state(),
        GuardState::Investigating,
        "…and the outer half is still only a glimpse",
    );

    let mut gone = look();
    let far = at(sight.glimpse_range() + 1);
    assert!(!gone.fov.contains(far), "precondition: out of the cone");
    gone.see(far, false, sight);
    assert_eq!(gone.state(), GuardState::Calm, "past the cone, nothing");

    // And the baseline is untouched by the derivation: §7.6's own [START] numbers.
    assert_eq!(
        (
            GuardSight::BASELINE.certain_range(),
            GuardSight::BASELINE.glimpse_range(),
        ),
        (5, 10),
        "deriving the zones from the cone must not move the baseline's",
    );
}

/// §6.1/§12.6/#495: a shortened cone is a **subset** of the baseline's from the same
/// cell and facing — shorter, never merely different — and it keeps both §6.1's
/// touching ring and §7.1's own ~90° silhouette, neither of which is the modifier's to
/// bend.
///
/// Stated on open floor so the claim is about the range rather than about which wall
/// happened to be in the way.
#[test]
fn a_narrowed_cone_is_a_strict_subset_of_the_baseline_cone() {
    let facility = Facility::walled_box(31, 31);
    let cone = |sight| {
        let mut guard = Guard::stationary(Cell::new(15, 15));
        guard.look(&facility, sight);
        guard.fov.cells().collect::<HashSet<Cell>>()
    };
    let baseline = cone(GuardSight::BASELINE);
    let narrowed = cone(GuardSight::NARROWED);

    assert!(
        narrowed.is_subset(&baseline),
        "a narrowed guard must see less, not elsewhere",
    );
    assert!(
        narrowed.len() < baseline.len(),
        "…and strictly less: {} cells against {}",
        narrowed.len(),
        baseline.len(),
    );
    // Shorter: the far half of the axis is gone…
    assert!(
        baseline.contains(&Cell::new(15, 23)) && !narrowed.contains(&Cell::new(15, 23)),
        "shorter — 8 down the axis is inside the baseline cone alone",
    );
    // …and no thinner: the ~90° wedge's own 45° diagonal is still covered at every
    // depth the shortened cone reaches. This is the assertion that would fail if the
    // arc were ever quietly folded back into this modifier.
    assert!(
        narrowed.contains(&Cell::new(19, 19)),
        "the wedge is §7.1's own — the 45° diagonal at depth 4 is still seen",
    );
    for ring in [Cell::new(15, 16), Cell::new(14, 16), Cell::new(16, 16)] {
        assert!(
            narrowed.contains(&ring),
            "§6.1's touching ring is not the modifier's to bend: {ring:?}",
        );
    }
}

/// §7.2's takedown gate is **per-turn fact, not mood**: a guard whose latest
/// look detected the player is aware; one whose latest look missed them —
/// concealment here — is not, even while its Chasing state lingers. That gap
/// is the puzzle: arrange to be adjacent while the *current* look misses.
#[test]
fn detection_is_per_turn_not_state() {
    let facility = Facility::walled_box(11, 11);
    let mut guard = Guard::stationary(Cell::new(5, 3)); // faces south (§7.1)
    guard.look(&facility, GuardSight::REAR_CARVE);
    let player = Cell::new(5, 5);
    assert!(!guard.detected_player(), "nothing sensed yet");

    guard.sense(player, false, GuardSight::BASELINE);
    assert!(guard.detected_player());
    assert_eq!(guard.state(), GuardState::Chasing);

    guard.sense(player, true, GuardSight::BASELINE); // concealed: this turn's look misses
    assert!(!guard.detected_player(), "awareness is per-turn");
    assert_eq!(guard.state(), GuardState::Chasing, "the mood lingers");
}

/// §7.2: finding a body is the loudest event in the game — the lead it grants
/// is pinned **stronger than a sighting's**, and the finder drops into the
/// §7.6 search centred on the body (a lead whose trail is already cold).
#[test]
fn finding_a_body_out_alerts_a_sighting_and_begins_the_search() {
    assert_eq!(BODY_ALERT_DURATION, 60, "the [START] body-found alert");
    // (That it out-alerts a sighting is a compile-time assert by the const.)

    let facility = Facility::walled_box(15, 15);
    let mut guard = Guard::patrolling(Cell::new(7, 2));
    guard.look(&facility, GuardSight::REAR_CARVE);
    let body = Cell::new(7, 5);
    guard.find_body(body);
    assert_eq!(guard.state(), GuardState::Alerted);
    assert_eq!(guard.alert, BODY_ALERT_DURATION);
    assert_eq!(guard.search, SEARCH_DURATION);
    assert_eq!(guard.focus, Some(body), "the search centres on the body");
}

/// §7.2: the live player outranks the dead — a guard that detected the player
/// this turn keeps its chase when it also sees a body; only the harder alert
/// sticks.
#[test]
fn a_detecting_guard_keeps_its_chase_over_a_found_body() {
    let facility = Facility::walled_box(15, 15);
    let mut guard = Guard::patrolling(Cell::new(7, 2));
    guard.look(&facility, GuardSight::REAR_CARVE);
    let player = Cell::new(7, 5);
    guard.sense(player, false, GuardSight::BASELINE);
    assert!(guard.detected_player());

    guard.find_body(Cell::new(8, 5));
    assert_eq!(guard.state(), GuardState::Chasing, "the chase holds");
    assert_eq!(guard.destination, Some(player), "still after the live cell");
    assert_eq!(guard.alert, BODY_ALERT_DURATION, "the alert still hardens");
}

/// §7.1/§7.6: a lead cools by one each turn nothing is sensed, and a reactive guard
/// whose alert reaches zero gives it up and stands back down to patrol — the honest
/// end of a chase whose sight was broken, and the outer bound on the §7.6 fix-2
/// search arc. This is the anti-tracking-turret backstop: the guard cannot pursue a
/// stale lead forever.
#[test]
fn a_cold_lead_stands_the_guard_down() {
    let facility = Facility::walled_box(11, 11);
    let mut guard = Guard::patrolling(Cell::new(5, 3));
    guard.look(&facility, GuardSight::REAR_CARVE);
    guard.see(Cell::new(5, 7), false, GuardSight::BASELINE);
    assert_eq!(guard.state(), GuardState::Chasing);
    assert_eq!(guard.alert, ALERT_DURATION);

    // The player vanishes (concealed each turn): the lead cools turn by turn.
    for remaining in (0..ALERT_DURATION).rev() {
        guard.sense(Cell::new(5, 7), true, GuardSight::BASELINE);
        assert_eq!(guard.alert, remaining, "the lead cools by one a turn");
    }

    // With the lead cold, deciding stands the guard down to patrol.
    guard.decide(
        &facility,
        &[],
        &mut Rng::new(0),
        Dwell::NEVER,
        PatrolStyle::Beat,
        GuardSight::REAR_CARVE,
    );
    assert_eq!(guard.state(), GuardState::Calm, "a cold lead is given up");
}

/// §15 Q5 (the found-a-body-nearby half) + §2.2 fairness: a **body** search checks
/// the cupboards inside the disc it sweeps and nothing beyond — a hidden player *in
/// range* of the found body is flushed, one *out of range* is left the safe cupboard
/// it is. A **lost-chase** search checks nothing at all, so hiding still works
/// against a guard that only lost sight of you (§10.3), and a spent search that
/// stands down stops checking. The distinction is only ever the result of hiding
/// within the search a body you left triggered.
#[test]
fn a_body_search_checks_only_hideouts_within_its_disc() {
    let body = Cell::new(20, 20);
    let mut finder = Guard::patrolling(body);
    finder.find_body(body);
    assert_eq!(
        finder.state(),
        GuardState::Alerted,
        "a found body opens a search"
    );

    assert_eq!(
        SEARCH_RADIUS, 4,
        "the [START] search radius the check rides on"
    );
    // In range (the §6.1 sight metric): a cupboard inside the swept disc is checked.
    assert!(
        finder.checks_hideout_at(Cell::new(20, 24), false),
        "a cupboard exactly SEARCH_RADIUS south is in the disc",
    );
    assert!(
        finder.checks_hideout_at(Cell::new(23, 23), false),
        "a diagonal cupboard within range is checked",
    );
    // Out of range: one step past the disc is never reached — the body was too far.
    assert!(
        !finder.checks_hideout_at(Cell::new(20, 25), false),
        "one step past the disc is left safe",
    );
    assert!(
        !finder.checks_hideout_at(Cell::new(25, 25), false),
        "a far diagonal cupboard is never checked",
    );

    // A lost-chase search (not a body) checks nothing — the cupboard stays the safe
    // wait-out it is against a guard that merely lost sight of you.
    let mut chaser = Guard::patrolling(body);
    chaser.begin_search();
    assert_eq!(
        chaser.state(),
        GuardState::Alerted,
        "a lost chase also opens a search"
    );
    assert!(
        !chaser.checks_hideout_at(body, false),
        "a lost-chase search never checks a hideout",
    );

    // A body search whose lead runs cold stands the guard down and stops checking.
    finder.stand_down();
    assert!(
        !finder.checks_hideout_at(Cell::new(20, 20), false),
        "a spent search checks nothing",
    );
}

/// The `guards_always_search_hideouts` level modifier (§12.6), directional: the
/// harder setting must flush at least as much as baseline. With `always_search`
/// on, a **lost-chase** search — the one that baseline leaves the safe wait-out
/// (§10.3) — now checks the cupboards inside its disc, and nothing beyond it, so
/// the modifier strictly *adds* pressure without widening the swept area. A Calm
/// guard that has released to a watch keeps its `focus` but is no longer
/// searching, so the modifier never makes it flush.
#[test]
fn the_always_search_hideouts_modifier_flushes_a_lost_chase() {
    let lead = Cell::new(20, 20);
    let mut chaser = Guard::patrolling(lead);
    chaser.begin_search();
    assert_eq!(chaser.state(), GuardState::Alerted, "a lost chase searches");

    // Baseline (modifier off): the lost chase checks nothing — the wait-out holds.
    assert!(
        !chaser.checks_hideout_at(lead, false),
        "baseline: a lost-chase search leaves the cupboard safe",
    );
    // Modifier on: the same search now flushes a cupboard within its disc …
    assert!(
        chaser.checks_hideout_at(lead, true),
        "modifier: a lost-chase search flushes a cupboard at its focus",
    );
    assert!(
        chaser.checks_hideout_at(Cell::new(20, 24), true),
        "modifier: … and one exactly SEARCH_RADIUS away, inside the disc",
    );
    // … but never one beyond the disc: the modifier widens *what* is searched,
    // not *where* — the swept area is unchanged.
    assert!(
        !chaser.checks_hideout_at(Cell::new(20, 25), true),
        "modifier: a cupboard one step past the disc is still left safe",
    );

    // A guard released from its search to a Calm watch keeps `focus` but is no
    // longer Alerted, so the modifier does not turn its stale focus into a flush.
    chaser.release_from_search();
    assert_eq!(chaser.state(), GuardState::Calm, "released to a watch");
    assert!(
        !chaser.checks_hideout_at(lead, true),
        "modifier: a Calm watcher with a stale focus never flushes",
    );
}

/// #430 must not quietly undo #410's pricing. A guard whose first spot is
/// deferred spends the turn it had planned — and if that turn is §7.5's slow
/// quarter-turn, the rotation re-aims its cone. The mood that cone is cast
/// against must be the guard's **live** one, not the Calm it was executing:
/// [`GuardSight::BASELINE`] gives blind flanks to a *patrol*, and a guard
/// that has just spotted you is not patrolling. Otherwise the deferred turn would
/// hand a hunting guard a patrol's blind spot — the "somewhere to hide from a
/// guard that is hunting you" the experiment is conditioned to prevent — and
/// paint it on the danger overlay (§11.5) while the `g` glyph reads Danger.
#[test]
fn a_deferred_first_spot_rotation_still_watches_its_flanks() {
    let facility = Facility::walled_box(11, 11);

    // The guard's pre-look plan: Calm, walking north — a 90° turn from its east
    // facing, so the planned turn is spent rotating.
    let plan = Plan {
        state: GuardState::Calm,
        destination: Some(Cell::new(5, 2)),
    };
    let rotate_then_sense = |state: GuardState, flanker: Cell| {
        let mut guard = Guard::patrolling(Cell::new(5, 5));
        guard.facing = Direction::East;
        // This turn's look left the guard in `state`; the plan above is what it
        // had already decided.
        guard.state = state;
        guard.destination = Some(Cell::new(5, 4));
        let step = guard.decide_planned(
            plan,
            &facility,
            &[],
            &mut Rng::new(0),
            Dwell::NEVER,
            PatrolStyle::Beat,
            GuardSight::BASELINE,
        );
        assert_eq!(step, None, "the planned turn is spent rotating");
        assert_eq!(guard.facing, Direction::North, "…and it rotated");
        assert_eq!(guard.state, state, "the live mood survives the swap");
        guard.sense(flanker, false, GuardSight::BASELINE);
        guard.detected_player()
    };

    // Facing north after the rotation, so (4,5) and (6,5) are the flanks.
    for flanker in [Cell::new(4, 5), Cell::new(6, 5)] {
        assert!(
            rotate_then_sense(GuardState::Chasing, flanker),
            "{flanker:?}: a guard that has just spotted you watches its sides (#410)",
        );
        assert!(
            rotate_then_sense(GuardState::Investigating, flanker),
            "{flanker:?}: a glimpse is a mood too — its flanks are live",
        );
        // The control: a guard that stayed Calm keeps the patrol's blind flanks,
        // so the difference is the mood and nothing else about the turn.
        assert!(
            !rotate_then_sense(GuardState::Calm, flanker),
            "{flanker:?}: a patrol that stayed a patrol is still flank-blind",
        );
    }
}

/// The **watched consoles** picker (§7.5/§12.6/#319) — the modifier's whole mechanism,
/// exercised where it lives: [`Guard::repick_patrol_target`] under
/// [`PatrolStyle::WatchedConsoles`], one leg at a time.
///
/// The scenes below all use one shape — a room, a console or two stamped into it, and
/// the whole interior as the guard's beat — and walk the guard by teleporting it onto
/// each destination it picks, which is what an arrival *is* as far as the picker is
/// concerned. The loop-level facts (the silenced net, the coverage bound, the
/// directional assertion) live in `state::tests::watched_consoles`.
mod watched_consoles {
    use super::*;

    /// The room every scene here is set in, and the guard's whole beat.
    fn room(consoles: &[Cell]) -> (Facility, Vec<Cell>) {
        let mut facility = Facility::walled_box(13, 7);
        for console in consoles {
            facility.set_terrain(console.x, console.y, Terrain::Console);
        }
        (facility, open_beat(13, 7))
    }

    /// Whether `cell` is a cell to watch `console` from — orthogonally adjacent, the
    /// bump range a console is used at (§4.3/§10.3).
    fn beside(cell: Cell, console: Cell) -> bool {
        cell.manhattan_distance(console) == 1
    }

    /// The next `count` patrol destinations this guard picks, arriving at each — one
    /// entry per leg, in order.
    fn legs(guard: &mut Guard, facility: &Facility, style: PatrolStyle, count: usize) -> Vec<Cell> {
        let mut rng = Rng::new(0);
        (0..count)
            .filter_map(|_| {
                guard.repick_patrol_target(facility, style, &mut rng);
                let destination = guard.destination?;
                guard.place_at(destination); // arrived
                Some(destination)
            })
            .collect()
    }

    /// §7.5/#319: the first leg of a watched beat stands the guard **beside** the
    /// console — never on it, which is solid and bump-interacted (§10.3/§4.3) — while
    /// the same guard at baseline takes the farthest uninspected cell and leaves the
    /// console to luck. That contrast is the ticket in one assertion.
    #[test]
    fn a_watched_beat_sends_the_first_leg_to_stand_beside_the_console() {
        let console = Cell::new(6, 3);
        let (facility, beat) = room(&[console]);

        let mut watching = Guard::patrolling(Cell::new(1, 1)).with_beat(beat.clone());
        watching.repick_patrol_target(&facility, PatrolStyle::WatchedConsoles, &mut Rng::new(0));
        let destination = watching
            .destination
            .expect("a patrol picks somewhere to go");
        assert!(beside(destination, console), "{destination:?}");
        assert_ne!(destination, console, "a guard never stands on a console");

        let mut sweeping = Guard::patrolling(Cell::new(1, 1)).with_beat(beat);
        sweeping.repick_patrol_target(&facility, PatrolStyle::Beat, &mut Rng::new(0));
        let plain = sweeping.destination.expect("a patrol picks");
        assert!(
            !beside(plain, console),
            "baseline takes the farthest uninspected cell (§7.5): {plain:?}",
        );
    }

    /// §7.5/§2.3/#319: **the ordinary sweep is not starved.** Console legs and
    /// farthest-uninspected legs strictly alternate, so at most every second leg is
    /// diverted — a guard that only shuttled between consoles would have turned the
    /// level into two watched rooms and a free corridor network, which is easier, not
    /// harder.
    #[test]
    fn console_legs_alternate_with_the_farthest_uninspected_sweep() {
        let console = Cell::new(6, 3);
        let (facility, beat) = room(&[console]);
        let mut guard = Guard::patrolling(Cell::new(1, 1)).with_beat(beat);

        let walked = legs(&mut guard, &facility, PatrolStyle::WatchedConsoles, 8);
        let watched: Vec<bool> = walked.iter().map(|&cell| beside(cell, console)).collect();
        assert_eq!(
            watched,
            [true, false, true, false, true, false, true, false],
            "legs: {walked:?}",
        );
    }

    /// §7.5/#319: the cycle **takes every console before it returns to one**, then wipes
    /// and starts over — §7.5's own inspected-memory wipe, over the watched set rather
    /// than over the ground. This is what makes coverage bounded rather than lucky.
    ///
    /// Nearest-unvisited-first and deterministic (§12.4), so the order is the same on
    /// every run of the same board: the guard starts in the west corner and takes the
    /// western console first.
    #[test]
    fn the_cycle_takes_every_console_before_returning_to_one() {
        let (near, far) = (Cell::new(4, 3), Cell::new(10, 3));
        let (facility, beat) = room(&[near, far]);
        let mut guard = Guard::patrolling(Cell::new(1, 1)).with_beat(beat);

        let visits: Vec<Cell> = legs(&mut guard, &facility, PatrolStyle::WatchedConsoles, 8)
            .into_iter()
            .filter_map(|cell| [near, far].into_iter().find(|&c| beside(cell, c)))
            .collect();
        assert_eq!(
            visits,
            [near, far, near, far],
            "each cycle takes both consoles, and the second cycle starts over",
        );
    }

    /// §10.3/§7.5/#319: a console **sealed off** from the guard is not a destination it
    /// walks at. The preference is drawn from the same candidate set the ordinary sweep
    /// uses — the guard's own territory, filtered to ground it can actually walk to
    /// (#477) — so an unreachable console is simply not in the cycle, and the leg falls
    /// back to the ordinary sweep rather than freezing the guard on a target it can
    /// never arrive at.
    ///
    /// ```text
    ///   0        9 11
    ///   #############   row 0
    ///   #@........$.#   row 1 — the console at (10,1) seals the alcove behind it
    ///   #############   row 2
    /// ```
    #[test]
    fn a_console_the_guard_cannot_reach_is_not_in_its_cycle() {
        let mut facility = Facility::walled_box(13, 3);
        facility.set_terrain(10, 1, Terrain::Console);
        let beat: Vec<Cell> = (1..12).map(|x| Cell::new(x, 1)).collect();
        let mut guard = Guard::patrolling(Cell::new(1, 1)).with_beat(beat);

        guard.repick_patrol_target(&facility, PatrolStyle::WatchedConsoles, &mut Rng::new(0));

        // (11,1) is beside the console and is the farthest cell in the beat — and it is
        // behind the console, so no route reaches it. The only other cell beside the
        // console is (9,1), which the guard *can* reach: that is the watch position.
        assert_eq!(guard.destination, Some(Cell::new(9, 1)));
    }

    /// §7.4/§7.6/#319: **Calm only.** A guard that is chasing, investigating or
    /// responding walks the lead its transition set, exactly as at baseline — this
    /// modifier must not survive into a hunt in any form, and the seam it is read at is
    /// the Calm repick that a reactive `decide` never reaches.
    #[test]
    fn only_a_calm_guard_prefers_a_console() {
        let console = Cell::new(6, 3);
        let (facility, beat) = room(&[console]);
        let quarry = Cell::new(1, 5);

        for state in [
            GuardState::Chasing,
            GuardState::Investigating,
            GuardState::Responding,
        ] {
            let mut guard = Guard::patrolling_to(Cell::new(1, 1), quarry)
                .with_beat(beat.clone())
                .with_state(state);
            // A warm lead: a reactive guard whose alert has run out stands down and
            // patrols, which is §7.6's backstop rather than anything to do with #319.
            guard.alert = ALERT_DURATION;
            guard.decide(
                &facility,
                &[],
                &mut Rng::new(0),
                Dwell::NEVER,
                PatrolStyle::WatchedConsoles,
                GuardSight::BASELINE,
            );
            assert_eq!(
                guard.destination,
                Some(quarry),
                "{state:?} walks its own lead, not a console",
            );
        }
    }

    /// §7.5/§10.5/#319: a **recut** beat (§7.3/#374 — a reinforcement's errand ends and
    /// the level is divided again) hands the guard new ground, so the consoles it
    /// watches and the cycle it tracks them by are both re-read. A guard must never keep
    /// cycling consoles that are now somebody else's.
    #[test]
    fn a_recut_beat_re_reads_the_consoles_it_watches() {
        let (west, east) = (Cell::new(3, 3), Cell::new(10, 3));
        let (facility, _) = room(&[west, east]);
        let western: Vec<Cell> = open_beat(13, 7)
            .into_iter()
            .filter(|cell| cell.x <= 6)
            .collect();
        let eastern: Vec<Cell> = open_beat(13, 7)
            .into_iter()
            .filter(|cell| cell.x >= 7)
            .collect();

        let mut guard = Guard::patrolling(Cell::new(1, 1)).with_beat(western);
        guard.repick_patrol_target(&facility, PatrolStyle::WatchedConsoles, &mut Rng::new(0));
        assert!(
            beside(guard.destination.expect("a pick"), west),
            "the western beat watches the western console",
        );

        guard.set_beat(eastern);
        guard.place_at(Cell::new(12, 1));
        guard.destination = None;
        guard.repick_patrol_target(&facility, PatrolStyle::WatchedConsoles, &mut Rng::new(0));
        assert!(
            beside(guard.destination.expect("a pick"), east),
            "after the recut it watches the console on its new ground",
        );
    }
}

/// **The archive takes the flank back** (§6.1/§6.2/§14 v3/#217): under
/// [`GuardSight::VIGILANT`] a **Calm** patrol watches its sides like every other mood, so
/// the two things a campaign spends six facilities teaching — the flank takedown and the
/// tail through a corner — are both off at the terminus.
///
/// It is the same scene as [`a_deferred_first_spot_rotation_still_watches_its_flanks`]
/// with the mood held at Calm and the **sight** changed instead, which is what makes the
/// assertion about the rule rather than about the guard: same guard, same cone, and only
/// the ring carve moved. §2.3's direction is the pair — baseline blind, archive detecting,
/// on the same cell.
#[test]
fn the_archives_guards_watch_their_sides_even_while_calm() {
    let facility = Facility::walled_box(11, 11);
    let calm_sees = |sight: GuardSight, other: Cell| {
        let mut guard = Guard::patrolling(Cell::new(5, 5));
        guard.facing = Direction::North;
        // The carve happens at **look** time against the guard's own mood, so the look has
        // to run under the level's sight for the ring rule to be the one under test.
        guard.look(&facility, sight);
        guard.sense(other, false, sight);
        guard.detected_player()
    };

    // Facing north, so (4,5) and (6,5) are the flanks.
    for flanker in [Cell::new(4, 5), Cell::new(6, 5)] {
        assert!(
            !calm_sees(GuardSight::BASELINE, flanker),
            "{flanker:?}: an ordinary facility's patrol is flank-blind (#442)",
        );
        assert!(
            calm_sees(GuardSight::VIGILANT, flanker),
            "{flanker:?}: the archive's patrol watches its sides",
        );
        // The shortened cone composes with the carve rather than replacing it (#495): a
        // flank cell is a *touching* neighbour, so it is inside either reach.
        assert!(
            calm_sees(GuardSight::VIGILANT_NARROWED, flanker),
            "{flanker:?}: a short-sighted archive guard still watches its sides",
        );
    }

    // What the archive does **not** take is the back: the takedown stays available from
    // directly behind and rear-diagonal (§7.2), which is what keeps the locked room's key
    // obtainable at all.
    for behind in [Cell::new(4, 6), Cell::new(5, 6), Cell::new(6, 6)] {
        assert!(
            !calm_sees(GuardSight::VIGILANT, behind),
            "{behind:?}: the three cells at a guard's back stay blind (§155)",
        );
    }
    // And the whole of it is the ring carve — the wedge is untouched.
    assert_eq!(GuardSight::VIGILANT.arc, GuardSight::BASELINE.arc);
    assert_eq!(GuardSight::VIGILANT.range, GuardSight::BASELINE.range);
}
