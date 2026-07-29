//! Rungs 2 and 3 send guards into the facility (§7.3/#374).
//!
//! [`crate::state::reinforcements`] owns the counts and pins them in isolation; what
//! is pinned here is the retaliation **wired into the game**: the right number of
//! guards for the rungs actually crossed, arriving somewhere the player cannot see,
//! walking to the cell that called them and searching it, and then behaving like every
//! other guard — normal speed (§7.1 **[SETTLED]**), a seeded radio clock, a body when
//! taken down.

use crate::alert::SIGHTING_CONTACT_TURNS;
use crate::guard::GuardState;
use crate::state::reinforcements::{RUNG_THREE_REINFORCEMENTS, RUNG_TWO_REINFORCEMENTS};
use crate::state::*;
use crate::test_support::{open_room, seed_sweep};
use crate::{generate_level, radio, AlertTrigger, Rng};

/// The exhaustive seed range the arrival properties sweep. "Never in view" must hold
/// on *every* accepted seed, not on a lucky one — the ticket asks for a batch, not a
/// fixture. The routine gate samples this via [`seed_sweep`]; CI runs it whole.
const SEEDS: u64 = 64;

/// Every reinforcement the events of one step reported.
fn arrivals(events: &[Event]) -> Vec<Cell> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::ReinforcementArrived { at } => Some(*at),
            _ => None,
        })
        .collect()
}

/// A hall with the player in a cupboard at (5,6) and a watcher at (5,2) staring down
/// the column — the [`super::alert`] scene, reused because it is the one that can drive
/// the ladder by nothing but standing up and ducking back down.
///
/// It is **40 cells long** on purpose. Waiting buys 360° vision (§8.3), and the ladder
/// is driven here by waits, so a room the player could see all of would have nowhere
/// out of sight for anybody to walk in — [`a_facility_the_player_can_see_all_of_admits_nobody`]
/// is that case, deliberately, and this one is the ordinary facility where the far end
/// is past [`PLAYER_SIGHT_RANGE`](crate::PLAYER_SIGHT_RANGE).
fn watched_cupboard() -> State {
    watched_hall(40)
}

/// [`watched_cupboard`] at an arbitrary length, so a scene can be made small enough
/// that the player sees the whole of it.
fn watched_hall(height: u32) -> State {
    let mut layout = open_room(12, height);
    layout.place(Cell::new(5, 6), Terrain::Hideout);
    State::new(
        layout,
        Cell::new(5, 6),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 2))],
        Vec::new(),
        Cell::new(10, height - 2),
    )
}

/// Stand up into the watcher's certain zone long enough to be confirmed, then duck
/// back into the cupboard — one sighting's worth of exposure.
fn show_yourself(state: &mut State) -> Vec<Event> {
    let mut events = state.step(Input::Step(Direction::North));
    for _ in 1..SIGHTING_CONTACT_TURNS {
        events.extend(state.step(Input::Wait));
    }
    events.extend(state.step(Input::Step(Direction::South)));
    events
}

/// §7.3/#374: **rung 1 sends nobody.** Being noticed costs the calm patrol dwell and
/// nothing else — the facility does not call anyone in for one confirmed sighting.
#[test]
fn rung_one_sends_nobody() {
    let mut s = watched_cupboard();
    let guards_before = s.guards().len();

    let events = show_yourself(&mut s);
    assert_eq!(s.alert(), 1, "the scene reached rung 1");
    assert!(arrivals(&events).is_empty(), "rung 1 called nobody in");
    assert_eq!(s.guards().len(), guards_before, "…and nobody arrived");
}

/// §7.3/#374: reaching rung 2 sends **exactly one** guard, and reaching rung 3 from
/// there sends **exactly two more** — cumulatively, so the facility ends up three
/// guards heavier. Neither rung pays twice, however many further triggers fire.
#[test]
fn each_rung_sends_its_guards_exactly_once() {
    let mut s = watched_cupboard();
    let guards_before = s.guards().len();

    // Rung 2's other trigger — three separate sightings — since the scene has no
    // console to tamper with. The window has to fall back to zero between each, or a
    // held look is one sighting rather than three (§7.6).
    show_yourself(&mut s);
    assert_eq!(s.alert(), 1);
    while s.alert() < 2 {
        for _ in 0..crate::alert::SIGHTING_WINDOW_TURNS {
            s.step(Input::Wait);
        }
        let events = show_yourself(&mut s);
        if s.alert() == 2 {
            assert_eq!(
                arrivals(&events).len(),
                RUNG_TWO_REINFORCEMENTS,
                "rung 2 sends exactly one guard",
            );
        } else {
            assert!(
                arrivals(&events).is_empty(),
                "no rung below 2 sends anybody",
            );
        }
    }
    assert_eq!(
        s.guards().len(),
        guards_before + RUNG_TWO_REINFORCEMENTS,
        "one guard heavier at rung 2",
    );

    // More sightings at rung 2 escalate nothing, so they send nobody: the rung is
    // paid for, and the ladder reports escalations rather than occurrences.
    for _ in 0..crate::alert::SIGHTING_WINDOW_TURNS {
        s.step(Input::Wait);
    }
    let again = show_yourself(&mut s);
    assert!(
        arrivals(&again).is_empty(),
        "rung 2 does not send its guard a second time",
    );
    assert_eq!(s.guards().len(), guards_before + RUNG_TWO_REINFORCEMENTS);
}

/// §7.3/#374: a run driven **straight to rung 3** gains all three guards at once —
/// the rungs *crossed* are what is paid for, not the rung landed on. This is the
/// jump the design calls out by name, and the one a "did we already send rung 2's
/// guard?" flag would get wrong.
#[test]
fn a_jump_from_nothing_to_the_top_sends_all_three() {
    // Two guards either side of the player's cupboard, both facing south (§7.1) so
    // neither one's cone covers the other's cell: take one down, and what the ladder
    // hears is the radio alone — a *second* quiet post, which is a rung-3 trigger.
    let mut layout = open_room(5, 30);
    layout.place(Cell::new(1, 10), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(1, 10),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(1, 9)).with_radio_clock(radio::RadioClock::from_period(3)),
            Guard::stationary(Cell::new(1, 11)).with_radio_clock(radio::RadioClock::from_period(3)),
        ],
        Vec::new(),
        Cell::new(1, 28),
    );
    s.step(Input::Step(Direction::North)); // the northern victim
    s.step(Input::Step(Direction::South)); // the southern one
    assert_eq!(s.bodies().len(), 2);
    assert!(s.guards().is_empty(), "nobody left in the building");
    assert_eq!(s.alert(), 0, "…and the facility still has no idea");

    // Wait for the pings to be missed. The first is rung 1 (no guards), the second
    // takes it to rung 3 — which owes rung 2's guard *and* rung 3's two.
    let mut sent = Vec::new();
    let mut climbed = Vec::new();
    for _ in 0..12 {
        let events = s.step(Input::Wait);
        for e in &events {
            if let Event::AlertRaised { rung, .. } = e {
                climbed.push(*rung);
            }
        }
        sent.extend(arrivals(&events));
    }

    assert_eq!(climbed, vec![1, 3], "rung 1, then straight to the top");
    assert_eq!(
        sent.len(),
        RUNG_TWO_REINFORCEMENTS + RUNG_THREE_REINFORCEMENTS,
        "crossing 1 → 3 owes rung 2's guard as well as rung 3's two",
    );
    assert_eq!(
        s.guards().len(),
        3,
        "three guards walked into an empty building"
    );
}

/// §7.3/#374, **the rule the mechanic lives or dies by**: an arrival is never inside
/// the player's field of view and never adjacent to them — diagonals included. A
/// guard the player *watches* appear is materialising out of nothing, which no amount
/// of fiction repairs.
///
/// Swept over real generated levels rather than a fixture, because the failure this
/// guards against is a cramped or wide-open seed, not a hand-placed one — and asked
/// for at **every turn of every run**, not only on the turns a run happens to escalate.
///
/// The call is made directly rather than driven through the ladder on purpose. Waiting
/// for a scripted walk to earn rung 2 would test the walk: most seeds never get there,
/// so the sweep would assert almost nothing while looking thorough (§13.4's own trap,
/// applied to a test). Queuing the arrival is the same seam the ladder uses, so what is
/// swept here is exactly the placement rule, over hundreds of player positions.
#[test]
fn an_arrival_is_never_in_view_and_never_adjacent() {
    let mut landed = 0;
    for seed in seed_sweep(SEEDS) {
        let mut rng = Rng::new(seed);
        let (layout, p) =
            generate_level(&crate::LevelConfig::V1, &mut rng).expect("the v1 config generates");
        let guards = p.guards(&layout);
        let mut s = State::new(
            layout,
            p.player(),
            Direction::North,
            guards,
            p.intel().iter().copied(),
            p.exit(),
        )
        .with_rng(rng);

        for turn in 0..40 {
            if s.outcome() != Outcome::Playing {
                break;
            }
            // Move about — including the occasional wait, which buys 360° vision (§8.3)
            // and so is the hardest turn to find an unseen cell on.
            let input = match turn % 5 {
                0 => Input::Step(Direction::East),
                1 => Input::Step(Direction::South),
                2 => Input::Wait,
                3 => Input::Step(Direction::East),
                _ => Input::Step(Direction::North),
            };
            s.step(input);
            // Call one in, wherever the player happens to be standing, and step a free
            // action so the world phases land it.
            s.queue_reinforcements(1, 2, s.player());
            let mut events = Vec::new();
            s.land_reinforcements(&mut events);
            for at in arrivals(&events) {
                landed += 1;
                assert!(
                    !s.player_fov().contains(at),
                    "seed {seed} turn {turn}: a guard walked in at {at:?}, in plain view",
                );
                let (dx, dy) = (at.x.abs_diff(s.player().x), at.y.abs_diff(s.player().y));
                assert!(
                    dx.max(dy) > 1,
                    "seed {seed} turn {turn}: a guard walked in at {at:?}, next to the player",
                );
                assert_eq!(
                    s.layout().facility().terrain(at),
                    Some(Terrain::Floor),
                    "seed {seed}: an arrival on something other than plain floor",
                );
            }
        }
    }
    assert!(
        landed > 100,
        "only {landed} arrivals across the sweep — the property tested almost nothing",
    );
}

/// §7.3/#374: an arrival comes in **at the far end** — the §10.5 region furthest from
/// the player — rather than merely somewhere legal. "Not in view" alone would allow a
/// guard to appear round the nearest corner, which reads as a spawn; the distance is
/// what lets the fiction carry ("they came in from outside").
///
/// Asserted as a comparison rather than against a fixed distance, because what counts
/// as far depends on the carve: the arrival must be no nearer than the median eligible
/// cell, on every seed.
#[test]
fn an_arrival_comes_in_at_the_far_end() {
    for seed in seed_sweep(SEEDS) {
        let mut rng = Rng::new(seed);
        let (layout, p) =
            generate_level(&crate::LevelConfig::V1, &mut rng).expect("the v1 config generates");
        let guards = p.guards(&layout);
        let mut s = State::new(
            layout,
            p.player(),
            Direction::North,
            guards,
            p.intel().iter().copied(),
            p.exit(),
        )
        .with_rng(rng);
        s.step(Input::Wait);

        s.queue_reinforcements(1, 2, s.player());
        let mut events = Vec::new();
        s.land_reinforcements(&mut events);
        let Some(&at) = arrivals(&events).first() else {
            continue;
        };

        // Every floor cell out of the player's sight, by distance — the pool an
        // arrival could legally have chosen from.
        let facility = s.layout().facility();
        let mut distances: Vec<u32> = (0..facility.height())
            .flat_map(|y| (0..facility.width()).map(move |x| Cell::new(x, y)))
            .filter(|&c| facility.terrain(c) == Some(Terrain::Floor) && !s.player_fov().contains(c))
            .map(|c| c.manhattan_distance(s.player()))
            .collect();
        distances.sort_unstable();
        let median = distances[distances.len() / 2];
        assert!(
            at.manhattan_distance(s.player()) >= median,
            "seed {seed}: a guard came in at {at:?}, nearer than half the cells it could have",
        );
    }
}

/// §7.3/§7.6/#374: a reinforcement **walks to the cell that called it and searches
/// it**, then patrols from where it finished. It does not teleport to the trigger, and
/// its lead lasts the whole journey — §7.1's cold-lead backstop must not strand it
/// halfway across the map having looked at nothing.
///
/// A long empty hall, the player shut in a cupboard well away from the errand: no
/// bodies, so nothing for a §15 Q5 body search to flush them out of, and the guard's
/// whole visible behaviour is the errand.
#[test]
fn a_reinforcement_walks_to_the_trigger_and_searches_it() {
    let mut layout = open_room(5, 40);
    layout.place(Cell::new(1, 5), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(1, 5),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(1, 38),
    )
    .with_rng(Rng::new(3));
    s.step(Input::Wait);

    // The errand: a cell in the middle of the hall, far from the player and far from
    // wherever the newcomer will come in.
    let errand = Cell::new(1, 20);
    s.queue_reinforcements(1, 2, errand);
    let mut events = Vec::new();
    s.land_reinforcements(&mut events);
    let arrived_at = *arrivals(&events).first().expect("the ladder sent somebody");

    let newcomer = &s.guards()[0];
    assert_eq!(
        newcomer.state(),
        GuardState::Responding,
        "a reinforcement arrives on an errand, not on patrol",
    );
    assert_eq!(newcomer.pos(), arrived_at);
    assert!(
        arrived_at.manhattan_distance(errand) > 1,
        "it walked in at the far end, not next to the cell that called it",
    );

    // It closes on the trigger over the turns that follow — one cell per turn, no
    // teleporting — and its errand ends in the §7.6 search: the bounded Alerted sweep
    // a responder drops into on arrival.
    let mut closest = arrived_at.manhattan_distance(errand);
    let mut searched = false;
    let mut previous = arrived_at;
    for _ in 0..200 {
        if s.outcome() != Outcome::Playing || searched && closest == 0 {
            break;
        }
        s.step(Input::Wait);
        let Some(g) = s.guards().first() else { break };
        assert!(
            g.pos().manhattan_distance(previous) <= 1,
            "a reinforcement moved {previous:?} → {:?} in one turn (§7.1)",
            g.pos(),
        );
        previous = g.pos();
        closest = closest.min(g.pos().manhattan_distance(errand));
        searched |= g.state() == GuardState::Alerted;
    }
    assert_eq!(
        closest, 0,
        "it walked all the way to the cell that called it"
    );
    assert!(
        searched,
        "…and searched there (§7.6) rather than standing on it"
    );

    // And once the errand is done it patrols from where it finished, rather than
    // standing on the trigger cell forever (§7.5).
    for _ in 0..80 {
        if s.outcome() != Outcome::Playing {
            break;
        }
        s.step(Input::Wait);
    }
    let settled = &s.guards()[0];
    assert_eq!(
        settled.state(),
        GuardState::Calm,
        "the errand ended and the guard stood back down (§7.4)",
    );
    // This hall is hand-built and so has no region graph to cut a beat out of; that
    // the beat is anchored on where the errand *finished* is pinned on a fixture that
    // does have one, in `a_reinforcement_patrols_where_it_finished_not_where_it_landed`.
    assert!(
        settled.pos().manhattan_distance(errand) < arrived_at.manhattan_distance(errand),
        "it came to rest near the cell it was sent to, not back at {arrived_at:?}",
    );
}

/// A row of `regions` three-cell rooms, each joined to the next by a closed door — the
/// [`crate::test_support::region_strip`] shape, long enough that a four-region beat
/// cannot span the whole level. Long strips are what make "the beat is around where the
/// guard finished" a *falsifiable* claim: on a short one every beat covers everything.
fn long_region_strip(regions: u32) -> Layout {
    use crate::region::{DoorKind, RegionGraph, RegionKind};
    let width = regions * 4 + 1;
    let mut f = Facility::walled_box(width, 6);
    let mut g = RegionGraph::new(width, 6);
    let ids: Vec<_> = (0..regions)
        .map(|i| {
            let x0 = i * 4 + 1;
            g.add_region(
                RegionKind::Room,
                (1..5).flat_map(move |y| (x0..x0 + 3).map(move |x| Cell::new(x, y))),
            )
        })
        .collect();
    for (i, pair) in ids.windows(2).enumerate() {
        let x = i as u32 * 4 + 4;
        for y in 1..5 {
            f.set_terrain(x, y, Terrain::Wall);
        }
        f.set_terrain(x, 1, Terrain::DoorHinge);
        f.set_terrain(x, 2, Terrain::DoorPanelClosed);
        f.set_terrain(x, 3, Terrain::DoorHinge);
        g.add_door(
            pair[0],
            pair[1],
            [Cell::new(x, 1), Cell::new(x, 3)],
            [Cell::new(x, 2)],
            DoorKind::Manual,
        );
    }
    Layout::from_parts(f, g)
}

/// §7.5/§7.3/#374 — the defect #398 removed, still held under #399's partition. A
/// reinforcement walks in at the far end of the facility and used to be tethered there
/// for the rest of the run: its beat was grown around the **arrival** cell, so once the
/// post-errand watch expired it walked the whole building back to the room it entered
/// by. And because the arrival region is the one furthest from the player — an answer
/// that barely moves over a run — *every* reinforcement was anchored to the same room.
///
/// Six rooms in a row with an incumbent guard held in the last of them, the player in
/// the first, the errand in the second. The newcomer comes in at the far end, walks the
/// length of the strip, searches, and settles — and the recut then matches it to the
/// half of the strip it is standing in rather than the half it arrived in.
///
/// **Two guards is the minimum that makes the claim falsifiable.** A lone guard owns
/// the whole level under a partition (§7.5), so "its beat holds the errand and not the
/// arrival" would be true and false at once. The incumbent is held in place rather than
/// patrolling because a patrolling one would own the whole strip until the newcomer
/// arrived and could have wandered anywhere by then — deciding the matching itself,
/// rather than the newcomer's own position deciding it.
#[test]
fn a_reinforcement_patrols_where_it_finished_not_where_it_landed() {
    let mut layout = long_region_strip(6);
    layout.place(Cell::new(1, 2), Terrain::Hideout);
    let errand = Cell::new(6, 2); // room 1, next door to the player
    let incumbent = Cell::new(21, 2); // room 5, the far end
    let mut s = State::new(
        layout,
        Cell::new(1, 2),
        Direction::North,
        vec![Guard::stationary(incumbent)],
        Vec::new(),
        Cell::new(2, 4),
    )
    .with_rng(Rng::new(7));
    s.step(Input::Wait);

    let before = s.guards().len();
    s.queue_reinforcements(1, 2, errand);
    let mut events = Vec::new();
    s.land_reinforcements(&mut events);
    let arrived_at = *arrivals(&events).first().expect("the ladder sent somebody");
    assert_eq!(s.guards().len(), before + 1);
    assert!(
        arrived_at.x > 12,
        "it came in at the far end of the strip, not beside the errand ({arrived_at:?})",
    );

    // Run the errand out: walk the strip, search, release to Calm — and be cut a beat.
    for _ in 0..400 {
        if s.outcome() != Outcome::Playing || s.guards()[before].has_beat() {
            break;
        }
        s.step(Input::Wait);
    }

    let settled = &s.guards()[before];
    assert_eq!(settled.state(), GuardState::Calm, "the errand ended");
    let beat = settled.beat();
    assert!(!beat.is_empty(), "…and it was cut a beat to patrol (§7.5)");
    assert!(
        beat.contains(&errand),
        "the beat covers the area the errand finished in",
    );
    assert!(
        !beat.contains(&arrived_at),
        "…and not the room it walked in by — the defect (§7.3/#374)",
    );
}

/// §7.6/#374: **reinforcements search, they do not hunt.** The errand a newcomer
/// arrives on is the cell control learned about, never the player's live position — the
/// distinction between *the net closing* (what §7.6 asks for) and the un-fun chase
/// (§7.6's trap). Asserted on the turn it lands, before anything it does later can
/// muddy it: the destination it is walking to is the trigger cell, and its state is
/// Responding rather than Chasing.
#[test]
fn a_reinforcement_arrives_on_an_errand_not_on_the_player() {
    for seed in seed_sweep(SEEDS) {
        let mut rng = Rng::new(seed);
        let (layout, p) =
            generate_level(&crate::LevelConfig::V1, &mut rng).expect("the v1 config generates");
        let guards = p.guards(&layout);
        let mut s = State::new(
            layout,
            p.player(),
            Direction::North,
            guards,
            p.intel().iter().copied(),
            p.exit(),
        )
        .with_rng(rng);
        s.step(Input::Wait);

        // An errand to a cell that is emphatically *not* where the player is: the
        // console they tampered with several rooms ago.
        let errand = p.intel()[0];
        let before = s.guards().len();
        s.queue_reinforcements(1, 2, errand);
        let mut events = Vec::new();
        s.land_reinforcements(&mut events);
        if s.guards().len() == before {
            continue;
        }

        let newcomer = s.guards().last().expect("the guard that just arrived");
        assert_eq!(
            newcomer.state(),
            GuardState::Responding,
            "seed {seed}: a reinforcement arrives on an errand",
        );
        assert_eq!(
            newcomer.destination(),
            Some(errand),
            "seed {seed}: it walks to the cell that called it, not to the player",
        );
        assert!(
            newcomer.beat().is_empty(),
            "seed {seed}: the beat is cut when the errand *ends*, around where the \
             guard finished — not around the room it walked in by (§7.5/§7.3)",
        );
    }
}

/// §7.3/#374, the other half of "never in view": when the facility offers **no** cell
/// the player cannot see, **nobody arrives**. Breaking the rule is worse than missing
/// the reinforcement, so the escalation goes unanswered rather than cheating.
///
/// The scene is a small hall and the ladder is driven by waiting, which buys 360°
/// vision (§8.3) — so the player genuinely sees every cell of it, and there is nowhere
/// for anyone to come in from.
#[test]
fn a_facility_the_player_can_see_all_of_admits_nobody() {
    let mut s = watched_hall(12);
    let guards_before = s.guards().len();

    show_yourself(&mut s);
    let mut sent = Vec::new();
    while s.alert() < 2 {
        for _ in 0..crate::alert::SIGHTING_WINDOW_TURNS {
            s.step(Input::Wait);
        }
        sent.extend(arrivals(&show_yourself(&mut s)));
    }

    assert_eq!(s.alert(), 2, "the ladder still climbed");
    assert!(
        sent.is_empty(),
        "a guard walked in where the player could see it: {sent:?}",
    );
    assert_eq!(
        s.guards().len(),
        guards_before,
        "the escalation went unanswered rather than cheating",
    );
}

/// §7.3/#374: a reinforcement is **a guard in every other respect**. It carries a
/// radio clock drawn from the run's own stream, it can be taken down, and the body it
/// leaves runs its own §7.3 clock like any other post — a loop the three-rung ceiling
/// is what caps.
#[test]
fn a_reinforcement_is_an_ordinary_guard_in_every_other_respect() {
    // A long empty hall with the player hidden in a cupboard partway down, and no
    // bodies anywhere — nothing to confuse the scene, and nothing for a §15 Q5 body
    // search to flush them out of.
    let mut layout = open_room(5, 40);
    layout.place(Cell::new(1, 8), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(1, 8),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(1, 38),
    )
    .with_rng(Rng::new(7));
    s.step(Input::Wait);

    // Call one in on an errand to the cell directly north of the cupboard, so it walks
    // to somewhere the concealed player can reach out and touch (§7.2's concealment
    // gate — a strike from a cupboard is legitimate from any angle).
    let errand = Cell::new(1, 7);
    s.queue_reinforcements(1, 2, errand);
    let mut events = Vec::new();
    s.land_reinforcements(&mut events);
    assert_eq!(arrivals(&events).len(), 1, "one guard walked in");

    // A drawn clock, not the un-jittered default: the arrival came off the run's own
    // stream (§12.4), so its whole ping schedule reproduces with the run.
    assert_ne!(
        s.guards()[0].radio_clock(),
        radio::RadioClock::DEFAULT,
        "a reinforcement's radio clock comes off the run seed",
    );

    // Wait for it to walk down the hall, and strike the moment it comes within reach —
    // a §7.6 sweep centres on its focus rather than parking on it, so "adjacent" is
    // what the player actually gets, and a strike from a cupboard is legitimate from
    // any angle (§7.2's concealment gate).
    let mut struck = false;
    for _ in 0..200 {
        if s.outcome() != Outcome::Playing || struck {
            break;
        }
        let player = s.player();
        let reachable = s.guards().iter().find_map(|g| {
            Direction::ALL
                .into_iter()
                .find(|&d| player.step(d) == Some(g.pos()))
        });
        let input = reachable.map_or(Input::Wait, Input::Step);
        for e in s.step(input) {
            if matches!(e, Event::TakenDown { .. }) {
                struck = true;
            }
        }
    }
    assert!(struck, "a reinforcement can be taken down like any guard");
    assert_eq!(
        s.bodies().len(),
        1,
        "…and it leaves an ordinary body behind (§7.2)",
    );
    assert!(s.guards().is_empty(), "the hall is empty again");

    // The body runs its own §7.3 clock like any other post — the loop the ladder's
    // three-rung ceiling is what caps.
    let mut went_quiet = false;
    for _ in 0..60 {
        if s.outcome() != Outcome::Playing {
            break;
        }
        for e in s.step(Input::Wait) {
            if matches!(e, Event::RadioSilence { .. }) {
                went_quiet = true;
            }
        }
    }
    assert!(
        went_quiet,
        "a reinforcement's post stops answering the radio like any other (§7.3)",
    );
}

/// §7.1 **[SETTLED]**: **guards never accelerate** — reinforcements included. The
/// tempting wrong answer to "make the top of the ladder hurt" is to make the newcomers
/// faster, so it gets its own assertion over a real level driven up the ladder.
#[test]
fn a_reinforcement_moves_no_faster_than_anybody_else() {
    let mut rng = Rng::new(11);
    let (layout, p) =
        generate_level(&crate::LevelConfig::V1, &mut rng).expect("the v1 config generates");
    let guards = p.guards(&layout);
    let mut s = State::new(
        layout,
        p.player(),
        Direction::North,
        guards,
        p.intel().iter().copied(),
        p.exit(),
    )
    .with_rng(rng);

    // Keyed by cell rather than by index: the guard vector *grows* when a
    // reinforcement lands, so positional pairing would silently compare the wrong
    // guards on exactly the turns this test is about.
    let mut before: Vec<Cell> = s.guards().iter().map(Guard::pos).collect();
    for turn in 0..300 {
        if s.outcome() != Outcome::Playing {
            break;
        }
        let arrived = arrivals(&s.step(Input::Step(if turn % 2 == 0 {
            Direction::East
        } else {
            Direction::South
        })));
        // A guard that arrived this turn has no previous position to move from.
        before.extend(arrived);
        let after: Vec<Cell> = s.guards().iter().map(Guard::pos).collect();
        for pos in &after {
            assert!(
                before.iter().any(|b| b.manhattan_distance(*pos) <= 1),
                "turn {turn}: a guard at {pos:?} was more than one cell from anywhere \
                 anybody stood last turn — something moved twice at rung {}",
                s.alert(),
            );
        }
        before = after;
    }
}

/// §12.4: the same seed and the same inputs produce the **same arrivals, on the same
/// turns, on the same cells**. The arrival cell and the drawn radio clock come off the
/// run's own stream, never a fresh source, so a reinforced run replays exactly.
#[test]
fn the_same_seed_and_inputs_send_the_same_guards_to_the_same_cells() {
    let run = |seed: u64| -> Vec<(u32, Cell)> {
        let mut rng = Rng::new(seed);
        let (layout, p) =
            generate_level(&crate::LevelConfig::V1, &mut rng).expect("the v1 config generates");
        let guards = p.guards(&layout);
        let mut s = State::new(
            layout,
            p.player(),
            Direction::North,
            guards,
            p.intel().iter().copied(),
            p.exit(),
        )
        .with_rng(rng);
        let mut landed = Vec::new();
        for turn in 0..60 {
            if s.outcome() != Outcome::Playing {
                break;
            }
            let dir = if turn % 3 == 0 {
                Direction::North
            } else {
                Direction::West
            };
            s.step(Input::Step(dir));
            // Called in on a fixed schedule rather than waiting for the ladder to earn
            // it — what is being pinned is that the *arrival* reproduces, and a run
            // that never escalates would compare two empty lists (§13.4's trap, in a
            // test). The draws still come off the run's own stream, so this is the
            // determinism the real thing has.
            if turn % 17 == 0 {
                s.queue_reinforcements(1, 2, s.player());
                let mut events = Vec::new();
                s.land_reinforcements(&mut events);
                for at in arrivals(&events) {
                    landed.push((s.turn(), at));
                }
            }
        }
        landed
    };

    let mut reinforced_somewhere = false;
    for seed in [4, 11, 19] {
        let once = run(seed);
        assert_eq!(once, run(seed), "seed {seed}: a replay replays");
        reinforced_somewhere |= !once.is_empty();
    }
    assert!(
        reinforced_somewhere,
        "no seed reinforced at all — the determinism assertion compared two empty lists",
    );
}

/// §7.3/#374: the ladder's own no-decay rule carries the reinforcements with it. Once
/// sent they are simply guards — nothing withdraws them when the chase goes cold,
/// because the rung that called them never falls.
#[test]
fn reinforcements_are_never_withdrawn() {
    let mut layout = open_room(5, 30);
    layout.place(Cell::new(1, 10), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(1, 10),
        Direction::North,
        vec![
            Guard::stationary(Cell::new(1, 9)).with_radio_clock(radio::RadioClock::from_period(3)),
            Guard::stationary(Cell::new(1, 11)).with_radio_clock(radio::RadioClock::from_period(3)),
        ],
        Vec::new(),
        Cell::new(1, 28),
    );
    s.step(Input::Step(Direction::North));
    s.step(Input::Step(Direction::South));
    for _ in 0..12 {
        s.step(Input::Wait);
    }
    let reinforced = s.guards().len();
    assert_eq!(reinforced, 3);
    assert_eq!(s.alert(), crate::TOP_RUNG);

    // A hundred quiet turns later, hidden in the cupboard the whole time: the rung is
    // still 3 and the guards are still in the building.
    for _ in 0..100 {
        if s.outcome() != Outcome::Playing {
            break;
        }
        s.step(Input::Wait);
    }
    assert_eq!(s.alert(), crate::TOP_RUNG, "a rung reached is a fact");
    assert_eq!(
        s.guards().len(),
        reinforced,
        "…and so are the guards it sent — nothing recalls them",
    );
    assert_eq!(
        s.alert(),
        AlertTrigger::SecondPostSilent.rung(),
        "the top of the ladder, reached by the radio",
    );
}
