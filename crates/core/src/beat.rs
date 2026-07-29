//! Region beats: the territory a guard's Calm patrol claims (§7.5, §10.5).
//!
//! The old territory was a box around the spawn cell — §7.5's named weakness: it
//! straddled walls, spilled into rooms the guard could not walk to, and had no
//! relationship to the building. The §10.5 region graph is the fix: a beat is a
//! **connected set of regions**, so every cell of it is genuinely walkable, and the
//! corridors joining a guard's rooms are first-class parts of its ground rather than
//! space crossed incidentally. The farthest-uninspected sweep (§7.5 — keep it) then
//! drives the guard room → corridor → room through them.
//!
//! # A partition of the level, decided by the building alone
//!
//! Beats used to be **grown outward from each guard**, and that had two faults. It was
//! only a *cover*: growth preferred a less-covered neighbour but never refused a
//! claimed one, so two guards overlapped whenever the scan order brought them
//! together, while a fixed per-beat ceiling meant most of the level belonged to
//! nobody. And it was anchored on the guards, so where a guard happened to be standing
//! decided which rooms existed as a unit — the same building split differently
//! depending on who was where.
//!
//! Both are gone. The level is **partitioned first, from the region graph alone**, into
//! as many connected parts as there are guards ([`partition`]); the guards are then
//! matched to the parts ([`assign`]). So:
//!
//! - **Every region belongs to exactly one part.** No wing goes uncovered — §7.5's
//!   stated weakness, answered by construction rather than by luck — and no two guards
//!   grind the same ground.
//! - **Territory size falls as headcount rises.** The same building divided more ways
//!   is less ground each. A reinforcement (§7.3/#374) now raises coverage **density** —
//!   the same building, watched by more people, each with less of it — which is the
//!   half of the escalation that was missing. The split is evened out as far as
//!   connectivity allows, which on a hub-shaped facility is not all the way: see
//!   [`split`].
//! - **Positions decide who patrols which part, never what the parts are.** The
//!   partition is a pure function of the graph (§12.4), so it is the same however the
//!   guards are arranged; only the matching reads where they stand, and only so that
//!   nobody is handed a wing on the far side of the facility.
//!
//! There is no per-beat size knob left to tune, and in particular no ceiling: the old
//! `BEAT_REGIONS = 4` capped a beat at four regions on levels that have seventeen to
//! twenty-three, which is why most of the building belonged to nobody. A beat is now
//! "the level, split *N* ways" — a statement about the building rather than a number
//! someone chose.
//!
//! **Call [`coordinated_beat_cells`] rarely.** The matching reads guards' *live*
//! positions, so it is a function of a value that changes every turn — fine only
//! because the callers are placement ([`Placement::guards`](crate::Placement)) and the
//! recut a reinforcement's errand ends in ([`State::recut_beats`](crate::State)).
//! Called from the per-turn path it would reshuffle every territory each turn and
//! patrols would visibly churn.

use crate::cell::{Cell, Direction};
use crate::facility::Facility;
use crate::region::{RegionGraph, RegionId};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

/// How the partition decides two regions are **joined**: anywhere a guard can walk
/// from one to the other without crossing a third.
///
/// [`RegionGraph::neighbours`] records **door edges only**, and that is not the same
/// question. Two corridors that flow into each other have no door and so no edge, and
/// two rooms joined by a doorway are separated by the doorway's own cells, which belong
/// to no region at all. Measured over v1 seeds, that fragments a 17-region level into
/// components of 7/6/2/2 — while every one of those regions is walk-reachable from
/// every guard. Partitioning on door edges alone therefore hands whole wings to nobody
/// by construction, which is the very thing this exists to fix.
///
/// So adjacency is the union of three things: a **door** between two regions, their
/// cells **touching** directly, or a run of walkable cells belonging to **no region**
/// that both of them touch — a doorway, a threshold, a scrap of corridor the region
/// carve left out. Cardinal steps only, since a diagonal is not a step (§4.1), and
/// symmetric by construction.
/// How many unowned walkable cells a **bridge** between two regions may span
/// (**[START] = 2**): enough for a doorway panel or a one-cell threshold, not enough
/// for a corridor.
const BRIDGE_SPAN: usize = 2;

fn adjacency(regions: &RegionGraph, facility: &Facility) -> HashMap<RegionId, BTreeSet<RegionId>> {
    let mut owner: HashMap<Cell, RegionId> = HashMap::new();
    for (id, region) in regions.regions() {
        for &cell in region.cells() {
            owner.insert(cell, id);
        }
    }
    let walkable = |cell: Cell| {
        facility
            .terrain(cell)
            .is_some_and(|terrain| !terrain.blocks_pathing())
    };
    let steps = |cell: Cell| Direction::ALL.into_iter().filter_map(move |d| cell.step(d));

    let mut joined: HashMap<RegionId, BTreeSet<RegionId>> = HashMap::new();
    let link = |a: RegionId, b: RegionId, joined: &mut HashMap<_, BTreeSet<_>>| {
        if a != b {
            joined.entry(a).or_default().insert(b);
            joined.entry(b).or_default().insert(a);
        }
    };

    for (id, region) in regions.regions() {
        joined.entry(id).or_default();
        for (_, neighbour) in regions.neighbours(id) {
            link(id, neighbour, &mut joined);
        }
        for &cell in region.cells() {
            for touching in steps(cell) {
                if let Some(&other) = owner.get(&touching) {
                    link(id, other, &mut joined);
                }
            }
        }
    }

    // Short runs of walkable cells nobody owns — a doorway's panel, a threshold, a
    // scrap of floor the region carve left between two rooms. Regions that both touch
    // one are a step apart, so they are joined.
    //
    // The span is bounded on purpose. Flooding an unowned run to its full extent and
    // joining everything it touches makes the graph nearly complete: one long corridor
    // of unclaimed floor brushes a dozen regions and cliques them all together, and a
    // partition drawn on that has no spatial structure left to work with. A bridge is a
    // threshold, not a thoroughfare.
    for y in 0..facility.height() {
        for x in 0..facility.width() {
            let start = Cell::new(x, y);
            if owner.contains_key(&start) || !walkable(start) {
                continue;
            }
            let mut span: HashSet<Cell> = HashSet::from([start]);
            let mut frontier = vec![start];
            for _ in 1..BRIDGE_SPAN {
                frontier = frontier
                    .into_iter()
                    .flat_map(steps)
                    .filter(|c| !owner.contains_key(c) && walkable(*c) && span.insert(*c))
                    .collect();
            }
            let touching: BTreeSet<RegionId> = span
                .iter()
                .flat_map(|&c| steps(c))
                .filter_map(|c| owner.get(&c).copied())
                .collect();
            for &a in &touching {
                for &b in &touching {
                    link(a, b, &mut joined);
                }
            }
        }
    }
    joined
}

/// Partition `regions` into at most `parts` connected groups covering **every**
/// region.
///
/// A pure function of the graph and the walls — no guard positions, no randomness
/// (§12.4).
///
/// The level is split by **component first, share second**. [`adjacency`] can still
/// leave the graph in several pieces, and a piece is where a guard's whole patrol has
/// to live, so each component is allocated a share of the parts in proportion to its
/// size — at least one part each while parts remain, since a component nobody patrols
/// is a wing uncovered. Only then is each component divided among the parts it was
/// given ([`split`]).
fn partition(regions: &RegionGraph, facility: &Facility, parts: usize) -> Vec<Vec<RegionId>> {
    let all: Vec<RegionId> = regions.regions().map(|(id, _)| id).collect();
    if parts == 0 || all.is_empty() {
        return Vec::new();
    }
    let joined = adjacency(regions, facility);
    let components = components(&all, &joined);

    // Parts per component, proportional to size: one each first — a component nobody
    // patrols is a wing uncovered, which is the whole point — then the rest by largest
    // remainder, so the biggest piece of the building gets the most people.
    let mut allocation = vec![0usize; components.len()];
    let mut spare = parts;
    for slot in allocation.iter_mut() {
        if spare == 0 {
            break;
        }
        *slot = 1;
        spare -= 1;
    }
    while spare > 0 {
        // The component whose parts would each be carrying the most ground.
        let Some(index) = (0..components.len())
            .filter(|&i| allocation[i] > 0 && allocation[i] < components[i].len())
            .max_by_key(|&i| (components[i].len() / allocation[i], std::cmp::Reverse(i)))
        else {
            break;
        };
        allocation[index] += 1;
        spare -= 1;
    }

    let mut beats: Vec<Vec<RegionId>> = Vec::with_capacity(parts);
    for (component, &share_of_parts) in components.iter().zip(&allocation) {
        if share_of_parts == 0 {
            continue;
        }
        beats.extend(split(component, &joined, share_of_parts));
    }
    beats
}

/// The connected components of `all` under `joined`, each in region order and the
/// components themselves ordered by their lowest region — the deterministic frame
/// [`partition`] allocates over (§12.4).
fn components(
    all: &[RegionId],
    joined: &HashMap<RegionId, BTreeSet<RegionId>>,
) -> Vec<Vec<RegionId>> {
    let mut seen: HashSet<RegionId> = HashSet::new();
    let mut components = Vec::new();
    for &start in all {
        if !seen.insert(start) {
            continue;
        }
        let mut component = vec![start];
        let mut frontier = vec![start];
        while let Some(region) = frontier.pop() {
            for &neighbour in joined.get(&region).into_iter().flatten() {
                if seen.insert(neighbour) {
                    component.push(neighbour);
                    frontier.push(neighbour);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    components
}

/// Whether `group` is one piece under `neighbours` — the check that keeps a
/// rebalancing move from cutting a guard's territory in two (§10.5).
fn spans_one_piece<I: Iterator<Item = RegionId>>(
    group: &[RegionId],
    neighbours: &impl Fn(&RegionId) -> I,
) -> bool {
    let Some(&first) = group.first() else {
        return true;
    };
    let held: HashSet<RegionId> = group.iter().copied().collect();
    let mut seen = HashSet::from([first]);
    let mut frontier = vec![first];
    while let Some(region) = frontier.pop() {
        for neighbour in neighbours(&region) {
            if held.contains(&neighbour) && seen.insert(neighbour) {
                frontier.push(neighbour);
            }
        }
    }
    seen.len() == group.len()
}

/// Divide one connected `component` into `parts` connected groups covering **all** of
/// it.
///
/// Three stages, and each exists because the one before it is not enough on a real
/// facility. Seeds are placed **farthest-first** — the lowest-numbered region, then
/// repeatedly the region furthest in steps from every seed so far — to put them at
/// opposite ends rather than clustered in one wing. The component is then claimed
/// **ring by ring outward from every seed at once**, each region going to whichever
/// eligible group is smallest so far; a region is only ever offered to a group already
/// holding a neighbour of it, so every group stays connected however the ties fall.
/// Finally the split is **refined**: a boundary region moves from an oversized group to
/// a smaller neighbour whenever the group giving it up stays in one piece.
///
/// **Balance is best-effort, and the building is the limit.** A facility is only a few
/// rooms across and hub-shaped — one corridor can touch eight rooms — so most regions
/// are equidistant from several seeds and the hub itself is an articulation point that
/// its group cannot give up without splitting in two. Measured over v1 seeds the three
/// stages take a 17-region level from 10/4/2/1 to 8/4/3/2: no guard is starved, nothing
/// is uncovered, and the largest wing is large because the building has one. An exact
/// quota was tried first and is not achievable under the connectivity constraint —
/// filling groups one at a time to a fixed size closes rings around ground the later
/// seeds cannot reach, which is worse on every measure.
fn split(
    component: &[RegionId],
    joined: &HashMap<RegionId, BTreeSet<RegionId>>,
    parts: usize,
) -> Vec<Vec<RegionId>> {
    let held: HashSet<RegionId> = component.iter().copied().collect();
    let neighbours = |region: &RegionId| {
        joined
            .get(region)
            .into_iter()
            .flatten()
            .copied()
            .filter(|n| held.contains(n))
    };

    // Distance from the nearest of `from` to every region of the component.
    let sweep = |from: &[RegionId]| -> HashMap<RegionId, (u32, usize)> {
        let mut reached: HashMap<RegionId, (u32, usize)> = HashMap::new();
        let mut frontier: VecDeque<RegionId> = VecDeque::new();
        for (index, &seed) in from.iter().enumerate() {
            reached.insert(seed, (0, index));
            frontier.push_back(seed);
        }
        while let Some(region) = frontier.pop_front() {
            let (distance, owner) = reached[&region];
            let mut ring: Vec<RegionId> = neighbours(&region).collect();
            ring.sort_unstable();
            for neighbour in ring {
                if let std::collections::hash_map::Entry::Vacant(slot) = reached.entry(neighbour) {
                    slot.insert((distance + 1, owner));
                    frontier.push_back(neighbour);
                }
            }
        }
        reached
    };

    let mut seeds: Vec<RegionId> = vec![component[0]];
    while seeds.len() < parts.min(component.len()) {
        let reached = sweep(&seeds);
        // `max_by_key` keeps the later of equal keys, so invert the id to hold the
        // lowest one on a tie (§12.4). A region the sweep never reached cannot happen
        // in a connected component, but sorts first if it ever did.
        let Some(&next) = component
            .iter()
            .filter(|id| !seeds.contains(id))
            .max_by_key(|&&id| {
                let distance = reached.get(&id).map_or(u32::MAX, |&(d, _)| d);
                (distance, std::cmp::Reverse(id))
            })
        else {
            break;
        };
        seeds.push(next);
    }

    // Claim the component ring by ring outward from every seed at once. Within a ring
    // the region goes to whichever eligible group is **smallest so far** — a plain
    // nearest-seed Voronoi hands every tie to the same seed, and a facility is
    // hub-shaped enough (one corridor touching eight rooms) that almost everything is a
    // tie: measured, that produced 10/4/2/1 where this produces 5/4/4/4.
    //
    // A region is only ever offered to a group that already holds a neighbour of it, so
    // every group stays connected however the balancing falls.
    let mut owner: HashMap<RegionId, usize> = HashMap::new();
    let mut sizes: Vec<usize> = vec![1; seeds.len()];
    for (index, &seed) in seeds.iter().enumerate() {
        owner.insert(seed, index);
    }
    let mut frontier: Vec<RegionId> = seeds.clone();
    while !frontier.is_empty() {
        let mut ring: BTreeMap<RegionId, BTreeSet<usize>> = BTreeMap::new();
        for &region in &frontier {
            let holder = owner[&region];
            for neighbour in neighbours(&region) {
                if !owner.contains_key(&neighbour) {
                    ring.entry(neighbour).or_default().insert(holder);
                }
            }
        }
        frontier = ring.keys().copied().collect();
        for (region, candidates) in ring {
            let Some(&taker) = candidates.iter().min_by_key(|&&c| (sizes[c], c)) else {
                continue;
            };
            owner.insert(region, taker);
            sizes[taker] += 1;
        }
    }

    // Even out what the seeds could not. A facility is hub-shaped and only a few rooms
    // across, so no choice of seeds spreads evenly — measured, the sweep alone leaves
    // splits like 9/4/2/2. Repeatedly hand a **boundary** region from the largest group
    // to a smaller neighbour, as long as the group giving it up stays in one piece; a
    // few passes of that reach 5/4/4/4, and it converges because every accepted move
    // strictly shrinks the largest group.
    for _ in 0..component.len() * seeds.len() {
        let mut sizes = vec![0usize; seeds.len()];
        for &holder in owner.values() {
            sizes[holder] += 1;
        }
        // Any group two or more ahead of some neighbour may donate — not just the
        // largest. A facility's hub region is an articulation point, so the biggest
        // group is often the one that *cannot* give anything up without splitting in
        // two; restricting donors to it stalls the refinement early.
        let moved = component
            .iter()
            .filter_map(|&region| {
                let donor = *owner.get(&region)?;
                let taker = neighbours(&region)
                    .filter_map(|n| owner.get(&n).copied())
                    .filter(|&g| g != donor && sizes[g] + 1 < sizes[donor])
                    .min_by_key(|&g| (sizes[g], g))?;
                let rest: Vec<RegionId> = component
                    .iter()
                    .copied()
                    .filter(|r| *r != region && owner.get(r) == Some(&donor))
                    .collect();
                spans_one_piece(&rest, &neighbours).then_some((
                    std::cmp::Reverse(sizes[donor]),
                    sizes[taker],
                    region,
                    taker,
                ))
            })
            .min()
            .map(|(_, _, region, taker)| (region, taker));
        let Some((region, taker)) = moved else { break };
        owner.insert(region, taker);
    }

    let mut beats: Vec<Vec<RegionId>> = vec![Vec::new(); seeds.len()];
    for &region in component {
        // A region no ring reached would mean a disconnected component, which
        // [`components`] rules out; fall to the first group rather than dropping it.
        beats[owner.get(&region).copied().unwrap_or(0)].push(region);
    }
    beats.retain(|beat| !beat.is_empty());
    beats
}

/// Match each guard in `anchors` to one of `parts` — the index of the beat it patrols.
///
/// Greedy nearest-first over every (guard, part) pair: the closest pairing is taken,
/// both drop out, and the rest are matched the same way. Distance is the shortest
/// Manhattan reach from the guard to any cell of the part, which is enough to keep
/// people near their own ground without pretending to be a route. Ties break on
/// (guard, part) index order, so the matching is deterministic (§12.4).
///
/// **This is the only place a guard's position is read.** It decides *who patrols
/// which wing*, never what the wings are — so a recut moves people between parts
/// rather than redrawing the building around them.
///
/// With **more guards than parts** the surplus double up on their own nearest part:
/// a level with fewer regions than guards cannot give everyone distinct ground, and a
/// guard sharing a wing is better than a guard with none (§7.5's graceful degradation).
fn assign(regions: &RegionGraph, parts: &[Vec<RegionId>], anchors: &[Cell]) -> Vec<usize> {
    let reach = |anchor: Cell, part: &Vec<RegionId>| -> u32 {
        part.iter()
            .flat_map(|&id| regions.region(id).cells())
            .map(|&cell| anchor.manhattan_distance(cell))
            .min()
            .unwrap_or(u32::MAX)
    };

    let mut taken = vec![false; parts.len()];
    let mut matched: Vec<Option<usize>> = vec![None; anchors.len()];
    for _ in 0..anchors.len().min(parts.len()) {
        let best = anchors
            .iter()
            .enumerate()
            .filter(|(guard, _)| matched[*guard].is_none())
            .flat_map(|(guard, &anchor)| {
                parts
                    .iter()
                    .enumerate()
                    .filter(|(part, _)| !taken[*part])
                    .map(move |(part, cells)| (reach(anchor, cells), guard, part))
            })
            .min();
        let Some((_, guard, part)) = best else { break };
        matched[guard] = Some(part);
        taken[part] = true;
    }

    // Whoever is left had no part of their own to take — the more-guards-than-regions
    // case. They share the part they are nearest to.
    matched
        .into_iter()
        .enumerate()
        .map(|(guard, part)| {
            part.unwrap_or_else(|| {
                (0..parts.len())
                    .min_by_key(|&part| (reach(anchors[guard], &parts[part]), part))
                    .unwrap_or(0)
            })
        })
        .collect()
}

/// The region beat each guard in `anchors` patrols: the level partitioned between
/// them (§7.5/§10.5), one beat per anchor in `anchors` order.
///
/// See the module header — the partition comes from the graph and the anchors only
/// decide who gets which part. Every region of the level is in exactly one part, so
/// between them the guards cover the whole building; with more guards than parts the
/// surplus share.
pub(crate) fn coordinated_beats(
    regions: &RegionGraph,
    facility: &Facility,
    anchors: &[Cell],
) -> Vec<Vec<RegionId>> {
    let parts = partition(regions, facility, anchors.len());
    if parts.is_empty() {
        return vec![Vec::new(); anchors.len()];
    }
    assign(regions, &parts, anchors)
        .into_iter()
        .map(|part| parts[part].clone())
        .collect()
}

/// The cells of [`coordinated_beats`], each beat's regions flattened — the territory
/// each guard carries (§7.5), one list per anchor in `anchors` order.
pub(crate) fn coordinated_beat_cells(
    regions: &RegionGraph,
    facility: &Facility,
    anchors: &[Cell],
) -> Vec<Vec<Cell>> {
    coordinated_beats(regions, facility, anchors)
        .into_iter()
        .map(|beat| {
            beat.into_iter()
                .flat_map(|id| regions.region(id).cells().iter().copied())
                .collect()
        })
        .collect()
}

/// Whether `beat`'s regions form one component connected across door edges — the
/// §10.5 invariant every beat must keep, so a guard can walk its whole territory.
#[cfg(test)]
pub(crate) fn is_connected(regions: &RegionGraph, facility: &Facility, beat: &[RegionId]) -> bool {
    if beat.is_empty() {
        return true;
    }
    let joined = adjacency(regions, facility);
    let held: HashSet<RegionId> = beat.iter().copied().collect();
    let mut seen = HashSet::from([beat[0]]);
    let mut frontier = vec![beat[0]];
    while let Some(region) = frontier.pop() {
        for &neighbour in joined.get(&region).into_iter().flatten() {
            if held.contains(&neighbour) && seen.insert(neighbour) {
                frontier.push(neighbour);
            }
        }
    }
    seen.len() == beat.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facility::Terrain;
    use crate::region::{DoorKind, RegionKind};

    /// A rectangle of cells `[x0, x1) × [y0, y1)`, for building fixtures.
    fn rect(x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<Cell> {
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| Cell::new(x, y)))
            .collect()
    }

    /// A vertical hinge/panel/hinge doorway in wall column `x`.
    fn door_span(x: u32) -> ([Cell; 2], [Cell; 1]) {
        ([Cell::new(x, 1), Cell::new(x, 3)], [Cell::new(x, 2)])
    }

    /// A row of `count` rooms, each joined to the next by a door — the simplest graph
    /// with a clear "opposite ends" to partition, and the one where a badly spread pair
    /// of seeds would be obvious.
    fn strip(count: u32) -> (RegionGraph, Facility, Vec<RegionId>) {
        let width = count * 4 + 1;
        let mut g = RegionGraph::new(width, 6);
        let mut f = Facility::walled_box(width, 6);
        let ids: Vec<RegionId> = (0..count)
            .map(|i| g.add_region(RegionKind::Room, rect(i * 4 + 1, i * 4 + 4, 1, 5)))
            .collect();
        for (i, pair) in ids.windows(2).enumerate() {
            let x = i as u32 * 4 + 4;
            for y in 1..5 {
                f.set_terrain(x, y, Terrain::Wall);
            }
            f.set_terrain(x, 1, Terrain::DoorHinge);
            f.set_terrain(x, 2, Terrain::DoorPanelClosed);
            f.set_terrain(x, 3, Terrain::DoorHinge);
            let (hinges, panels) = door_span(x);
            g.add_door(pair[0], pair[1], hinges, panels, DoorKind::Manual);
        }
        (g, f, ids)
    }

    /// Every region belongs to exactly one part — §7.5's "a wing goes uncovered"
    /// weakness stated as a property. This is the claim the old grower could not make:
    /// it capped each beat at a fixed size, so most of a level belonged to nobody.
    #[test]
    fn a_partition_covers_every_region_exactly_once() {
        let (g, f, ids) = strip(9);
        for parts in 1..=4 {
            let beats = partition(&g, &f, parts);
            let claimed: Vec<RegionId> = beats.iter().flatten().copied().collect();
            let distinct: HashSet<RegionId> = claimed.iter().copied().collect();
            assert_eq!(
                claimed.len(),
                distinct.len(),
                "{parts}: a region claimed twice"
            );
            assert_eq!(
                distinct.len(),
                ids.len(),
                "{parts}: a region claimed by nobody"
            );
            for beat in &beats {
                assert!(!beat.is_empty(), "{parts}: an empty part");
                assert!(
                    is_connected(&g, &f, beat),
                    "{parts}: a part straddles a wall"
                );
            }
        }
    }

    /// Territory size falls as headcount rises, and nobody is starved. On a plain row
    /// of rooms — no hub, so connectivity does not fight the split — the division is
    /// exact; the guarantee that survives on a real facility is the weaker one asserted
    /// here for every shape: every group non-empty, and the largest no more than twice
    /// the smallest.
    #[test]
    fn parts_shrink_as_headcount_rises() {
        let (g, f, _) = strip(9);
        let sizes = |parts: usize| -> Vec<usize> {
            let mut s: Vec<usize> = partition(&g, &f, parts).iter().map(Vec::len).collect();
            s.sort_unstable();
            s
        };
        assert_eq!(sizes(1), vec![9]);
        assert_eq!(sizes(2), vec![4, 5]);
        assert_eq!(sizes(3), vec![3, 3, 3]);
        for parts in 1..=4 {
            let s = sizes(parts);
            assert!(s[0] >= 1, "{parts}: a guard got nothing: {s:?}");
            assert!(s[s.len() - 1] <= 2 * s[0], "{parts}: lopsided: {s:?}");
        }
    }

    /// The partition is a pure function of the **graph** — the guards' positions do
    /// not enter into it (§12.4). Whatever the anchors, the same building splits the
    /// same way; positions only decide who patrols which part.
    #[test]
    fn the_partition_does_not_depend_on_where_the_guards_stand() {
        let (g, f, _) = strip(6);
        let shape = |anchors: &[Cell]| -> HashSet<Vec<RegionId>> {
            coordinated_beats(&g, &f, anchors).into_iter().collect()
        };
        // Two guards huddled at one end, and the same two spread to opposite ends.
        let huddled = shape(&[Cell::new(2, 2), Cell::new(3, 2)]);
        let spread = shape(&[Cell::new(2, 2), Cell::new(22, 2)]);
        assert_eq!(
            huddled, spread,
            "the building was split differently depending on where the guards were",
        );
    }

    /// …but positions do decide **who gets which part**: each guard is matched to the
    /// part it is nearest, so nobody is handed a wing on the far side of the facility.
    #[test]
    fn guards_are_matched_to_the_part_they_stand_in() {
        let (g, f, ids) = strip(6);
        let west = Cell::new(2, 2); // room 0
        let east = Cell::new(22, 2); // room 5
        let beats = coordinated_beats(&g, &f, &[west, east]);
        assert!(beats[0].contains(&ids[0]), "the western guard got the west");
        assert!(beats[1].contains(&ids[5]), "the eastern guard got the east");
        assert!(!beats[0].contains(&ids[5]));
        assert!(!beats[1].contains(&ids[0]));

        // Swapping the guards swaps the parts, and nothing else moves.
        let swapped = coordinated_beats(&g, &f, &[east, west]);
        assert_eq!(swapped[0], beats[1]);
        assert_eq!(swapped[1], beats[0]);
    }

    /// Graceful degradation (§7.5): with more guards than regions to divide, the
    /// surplus share the part they are nearest rather than being starved of ground —
    /// every guard still has somewhere to patrol, and the level is still fully covered.
    #[test]
    fn more_guards_than_regions_still_gives_each_a_beat() {
        let (g, f, ids) = strip(2);
        let anchors = [
            Cell::new(2, 2),
            Cell::new(3, 2),
            Cell::new(6, 2),
            Cell::new(7, 2),
        ];
        let beats = coordinated_beats(&g, &f, &anchors);
        assert_eq!(beats.len(), anchors.len());
        for beat in &beats {
            assert!(!beat.is_empty(), "every guard gets a beat");
            assert!(is_connected(&g, &f, beat));
        }
        let covered: HashSet<RegionId> = beats.iter().flatten().copied().collect();
        assert_eq!(
            covered,
            ids.iter().copied().collect(),
            "still fully covered"
        );
    }

    /// An anchor in no region — a wall or doorway cell — still gets a part, because
    /// the partition does not depend on it. (The old grower seeded from the anchor's
    /// own region and so handed such a guard nothing at all.)
    #[test]
    fn an_anchor_outside_any_region_still_gets_a_part() {
        let (g, f, _) = strip(3);
        let beats = coordinated_beats(&g, &f, &[Cell::new(0, 0)]);
        assert!(
            !beats[0].is_empty(),
            "a wall-standing guard still has ground"
        );
    }

    /// A graph with no regions at all — a hand-built fixture — yields an empty beat
    /// per guard rather than panicking. Such a guard has no territory and holds (§7.5).
    #[test]
    fn a_graph_with_no_regions_yields_empty_beats() {
        let g = RegionGraph::new(8, 8);
        let f = Facility::walled_box(8, 8);
        assert_eq!(
            coordinated_beats(&g, &f, &[Cell::new(1, 1)]),
            vec![Vec::new()]
        );
        assert!(coordinated_beat_cells(&g, &f, &[Cell::new(1, 1)])[0].is_empty());
    }

    /// A disconnected graph gets a seed into each component before either component
    /// gets a second, so no guard is assigned a part it cannot walk (§10.5).
    #[test]
    fn disconnected_components_are_seeded_before_they_are_subdivided() {
        // Two separate two-room strips, no door between the pairs.
        let mut g = RegionGraph::new(20, 6);
        // Solid wall throughout: the only walkable ground is the regions' own cells and
        // the two doorways, so the pairs are genuinely separate components.
        let mut f = Facility::walled_box(20, 6);
        for y in 0..6 {
            for x in 0..20 {
                f.set_terrain(x, y, Terrain::Wall);
            }
        }
        for (x0, x1) in [(1, 4), (5, 8), (11, 14), (15, 18)] {
            for y in 1..5 {
                for x in x0..x1 {
                    f.set_terrain(x, y, Terrain::Floor);
                }
            }
        }
        let a0 = g.add_region(RegionKind::Room, rect(1, 4, 1, 5));
        let a1 = g.add_region(RegionKind::Room, rect(5, 8, 1, 5));
        let b0 = g.add_region(RegionKind::Room, rect(11, 14, 1, 5));
        let b1 = g.add_region(RegionKind::Room, rect(15, 18, 1, 5));
        for (x, near, far) in [(4, a0, a1), (14, b0, b1)] {
            f.set_terrain(x, 1, Terrain::DoorHinge);
            f.set_terrain(x, 2, Terrain::DoorPanelClosed);
            f.set_terrain(x, 3, Terrain::DoorHinge);
            let (hinges, panels) = door_span(x);
            g.add_door(near, far, hinges, panels, DoorKind::Manual);
        }

        let beats = partition(&g, &f, 2);
        assert_eq!(beats.len(), 2);
        for beat in &beats {
            assert!(is_connected(&g, &f, beat), "a part spans two components");
        }
        let covered: HashSet<RegionId> = beats.iter().flatten().copied().collect();
        assert_eq!(covered, [a0, a1, b0, b1].into_iter().collect());
    }

    /// §12.4: the whole assignment is deterministic — same graph, same anchors, same
    /// beats, every time, and the cell view agrees with the region view.
    #[test]
    fn coordinated_beats_are_deterministic() {
        let (g, f, _) = strip(7);
        let anchors = [Cell::new(2, 2), Cell::new(10, 2), Cell::new(26, 2)];
        assert_eq!(
            coordinated_beats(&g, &f, &anchors),
            coordinated_beats(&g, &f, &anchors)
        );
        for (cells, regions) in coordinated_beat_cells(&g, &f, &anchors)
            .iter()
            .zip(coordinated_beats(&g, &f, &anchors))
        {
            let expected: Vec<Cell> = regions
                .iter()
                .flat_map(|&id| g.region(id).cells().iter().copied())
                .collect();
            assert_eq!(cells, &expected);
        }
    }
}
