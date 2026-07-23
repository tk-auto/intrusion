//! Ducts: player-only crawlspace shortcuts threaded through the walls (§10.7).
//!
//! The last carve pass. A duct is a path of wall cells whose two ends are recessed
//! **entries** — the exact one-mouth geometry a cupboard uses ([`recess_site`], the
//! §10.1.6 three-solid-sides rule) — connecting two regions that are **far apart on
//! the region graph**. A duct that shortcuts nothing is noise (§10.7), so a candidate
//! is kept only when crawling it saves at least [`DUCT_MIN_PAYOFF`] steps over walking
//! the floor between its mouths.
//!
//! # Why this changes no §10.6/§10.1a guarantee
//!
//! A [`Terrain::DuctEntry`] is wall-like in every guard-facing property — solid,
//! opaque, pathing-blocking (`facility.rs`) — and the interior stays [`Terrain::Wall`]
//! untouched. So the finished grid's reachability, enclosure and sightlines are
//! byte-identical to the pre-duct grid: the crawl route a duct opens is the player's
//! alone (§10.7), invisible to every check the §10.6 gate runs. That is why this pass
//! runs *after* the gate's inputs are fixed and needs no re-validation of them; it
//! asserts only its own invariant — the per-entry one-mouth geometry.
//!
//! Everything here is deterministic from the seed (§12.4): region pairs are walked in
//! a fixed order and entry combinations tried shortest-first, so the same seed threads
//! the same ducts.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::cell::Cell;
use crate::duct::Duct;
use crate::facility::{Facility, Terrain};
use crate::region::{RegionGraph, RegionId};
use crate::rng::Rng;

use super::recess_site;

/// How many duct crawlspaces a level gets, at most (§10.7 **[START]**). A *small*
/// number: ducts are a spice, not the main route — reachability never depends on one
/// (§10.6), so a level that can seat only fewer (or none) is fine. Enough that a
/// 40×40 facility usually carries one or two shortcuts to discover.
const DUCT_RUNS_PER_LEVEL: usize = 2;

/// The fewest cells a duct may span, entries included (§10.7 **[START]**). Below this
/// there is no crawlspace worth the name — two entries back to back is a hole in a
/// wall, not a shortcut.
const DUCT_MIN_CELLS: usize = 4;

/// The most cells a duct may span, entries included (§10.7 **[START]**). A very long
/// crawl is slow (one cell per turn, §4.4) and rarely a real shortcut; the cap keeps
/// the wall-BFS bounded and the feature legible.
const DUCT_MAX_CELLS: usize = 22;

/// The fewest steps crawling a duct must save over **walking** between its two mouths
/// (§10.7 **[START]**): the shortcut payoff. `floor_walk(mouthA, mouthB) − crawl_len ≥`
/// this, or the candidate is rejected as noise — a duct that barely beats the corridor
/// is not worth the degraded-information cost (§2.3) it charges.
const DUCT_MIN_PAYOFF: u32 = 8;

/// How many entry combinations to try per region pair before giving up on it. Bounds
/// the work; entries are tried shortest-Manhattan first, so the best shortcuts are
/// tried first anyway.
const MAX_ENTRY_TRIES: usize = 8;

/// Thread up to [`DUCT_RUNS_PER_LEVEL`] player-only ducts through the walls (§10.7),
/// stamping each entry as [`Terrain::DuctEntry`] and returning the recorded paths.
///
/// Regions are paired **farthest-first** on the region graph (a duct that shortcuts
/// nothing is noise), and for each pair the closest recessed-entry combinations are
/// tried until one yields a wall path within [`DUCT_MIN_CELLS`]..=[`DUCT_MAX_CELLS`]
/// that saves at least [`DUCT_MIN_PAYOFF`] walking steps. Deterministic from `rng`
/// (§12.4).
pub(super) fn place_ducts(
    facility: &mut Facility,
    regions: &RegionGraph,
    rng: &mut Rng,
) -> Vec<Duct> {
    let mut ducts: Vec<Duct> = Vec::new();
    // Every cell already claimed by a placed duct — entries and interior — so two
    // ducts never share a cell or cross.
    let mut used: HashSet<Cell> = HashSet::new();

    // Recessed entry candidates, grouped by the region their mouth opens onto. Each is
    // a wall cell with exactly one floor mouth and three solid wall sides (§10.1.6).
    let entries_by_region = entry_candidates(facility, regions, rng);
    if entries_by_region.len() < 2 {
        return ducts; // nowhere to connect
    }

    // Region pairs, farthest apart on the region graph first (§10.7). Hop distance is
    // BFS over single-door adjacency; ties break on the region handles for determinism.
    let pairs = region_pairs_farthest_first(regions, &entries_by_region);

    for (a, b) in pairs {
        if ducts.len() >= DUCT_RUNS_PER_LEVEL {
            break;
        }
        let Some(ea) = entries_by_region.get(&a) else {
            continue;
        };
        let Some(eb) = entries_by_region.get(&b) else {
            continue;
        };
        // Try entry combinations closest-first: a shorter wall gap is a shorter crawl,
        // which is the better shortcut and the cheaper BFS.
        let mut combos: Vec<(Cell, Cell)> = Vec::new();
        for &(wall_a, _) in ea {
            for &(wall_b, _) in eb {
                if !used.contains(&wall_a) && !used.contains(&wall_b) {
                    combos.push((wall_a, wall_b));
                }
            }
        }
        combos.sort_by_key(|&(x, y)| (x.manhattan_distance(y), x.x, x.y, y.x, y.y));

        for &(wall_a, wall_b) in combos.iter().take(MAX_ENTRY_TRIES) {
            if let Some(path) = route_duct(facility, wall_a, wall_b, &used) {
                if path.len() < DUCT_MIN_CELLS || path.len() > DUCT_MAX_CELLS {
                    continue;
                }
                let mouth_a = mouth_of(ea, wall_a);
                let mouth_b = mouth_of(eb, wall_b);
                if !worth_it(facility, mouth_a, mouth_b, path.len()) {
                    continue;
                }
                // Commit: the two ends become entries; the interior stays wall.
                facility.set_terrain(wall_a.x, wall_a.y, Terrain::DuctEntry);
                facility.set_terrain(wall_b.x, wall_b.y, Terrain::DuctEntry);
                for &c in &path {
                    used.insert(c);
                }
                ducts.push(Duct::new(path));
                break;
            }
        }
    }

    ducts
}

/// The floor **mouth** of the entry candidate at `wall` — the cached second element of
/// the `(wall, mouth)` candidate pair.
fn mouth_of(entries: &[(Cell, Cell)], wall: Cell) -> Cell {
    entries
        .iter()
        .find(|&&(w, _)| w == wall)
        .map(|&(_, m)| m)
        .expect("wall came from this region's entry list")
}

/// Whether a duct of `crawl_len` cells between `mouth_a` and `mouth_b` saves at least
/// [`DUCT_MIN_PAYOFF`] steps over walking the floor (§10.7). A duct whose mouths cannot
/// be walked between at all (disconnected floor — never on a §10.6-valid level) is not
/// worth it either.
fn worth_it(facility: &Facility, mouth_a: Cell, mouth_b: Cell, crawl_len: usize) -> bool {
    let Some(walk) = floor_distance(facility, mouth_a, mouth_b) else {
        return false;
    };
    // Crawling costs one turn per step *between* the mouths — the entries flank them —
    // so the crawl cost is the interior length plus the two climb steps ≈ path len.
    walk.saturating_sub(crawl_len as u32) >= DUCT_MIN_PAYOFF
}

/// Recessed duct-entry candidates (§10.1.6 geometry), grouped by the region their
/// mouth opens onto. Each entry is a `(wall, mouth)` pair. Candidates are shuffled with
/// `rng` so the choice varies by seed while staying deterministic (§12.4).
fn entry_candidates(
    facility: &Facility,
    regions: &RegionGraph,
    rng: &mut Rng,
) -> HashMap<RegionId, Vec<(Cell, Cell)>> {
    let mut all: Vec<(RegionId, Cell, Cell)> = Vec::new();
    for y in 1..facility.height() - 1 {
        for x in 1..facility.width() - 1 {
            let wall = Cell::new(x, y);
            let Some(mouth) = recess_site(facility, wall) else {
                continue;
            };
            let Some(region) = regions.region_at(mouth) else {
                continue;
            };
            all.push((region, wall, mouth));
        }
    }
    // Deterministic shuffle, then group — so the per-region order is seed-dependent
    // but reproducible (§12.4).
    super::shuffle(&mut all, rng);
    let mut grouped: HashMap<RegionId, Vec<(Cell, Cell)>> = HashMap::new();
    for (region, wall, mouth) in all {
        grouped.entry(region).or_default().push((wall, mouth));
    }
    grouped
}

/// Region pairs that both have entry candidates, ordered by **region-graph hop
/// distance, farthest first** (§10.7) — a duct should connect regions that are far
/// apart, so it is a real shortcut. Ties break on the region handles so the order is
/// deterministic (§12.4).
fn region_pairs_farthest_first(
    regions: &RegionGraph,
    entries_by_region: &HashMap<RegionId, Vec<(Cell, Cell)>>,
) -> Vec<(RegionId, RegionId)> {
    let mut ids: Vec<RegionId> = entries_by_region.keys().copied().collect();
    ids.sort_by_key(|id| region_index(regions, *id));

    let mut pairs: Vec<(u32, RegionId, RegionId)> = Vec::new();
    for (i, &a) in ids.iter().enumerate() {
        let dist = hop_distances(regions, a);
        for &b in &ids[i + 1..] {
            // Unreachable pairs (never on a connected level) sort last via u32::MAX.
            let d = dist.get(&b).copied().unwrap_or(u32::MAX);
            pairs.push((d, a, b));
        }
    }
    // Farthest first; deterministic tiebreak on the handles.
    pairs.sort_by(|l, r| {
        r.0.cmp(&l.0)
            .then(region_index(regions, l.1).cmp(&region_index(regions, r.1)))
            .then(region_index(regions, l.2).cmp(&region_index(regions, r.2)))
    });
    pairs.into_iter().map(|(_, a, b)| (a, b)).collect()
}

/// A stable ordinal for a region handle — its position in the graph's region list.
/// Handles are opaque (§10.5), so this is only for a deterministic sort key.
fn region_index(regions: &RegionGraph, id: RegionId) -> usize {
    regions
        .regions()
        .position(|(rid, _)| rid == id)
        .expect("region handle came from this graph")
}

/// BFS hop distances from `start` to every region across single-door adjacency
/// (§10.5). The region graph is small (~12 regions), so an all-from-`start` flood per
/// source is cheap.
fn hop_distances(regions: &RegionGraph, start: RegionId) -> HashMap<RegionId, u32> {
    let mut dist: HashMap<RegionId, u32> = HashMap::new();
    dist.insert(start, 0);
    let mut frontier = VecDeque::new();
    frontier.push_back(start);
    while let Some(id) = frontier.pop_front() {
        let d = dist[&id];
        for (_, next) in regions.neighbours(id) {
            if let std::collections::hash_map::Entry::Vacant(slot) = dist.entry(next) {
                slot.insert(d + 1);
                frontier.push_back(next);
            }
        }
    }
    dist
}

/// The shortest **wall** path from entry candidate `wall_a` to `wall_b` (§10.7): a
/// BFS across interior [`Terrain::Wall`] cells only, so the whole interior of the duct
/// stays solid wall to guards. Returns the path inclusive of both ends (which the
/// caller stamps as entries), or `None` if no wall corridor connects them within reach.
///
/// The border ring is excluded (a duct is internal), as are cells already claimed by
/// another duct (`used`). Only the two ends may sit next to floor (their mouths); the
/// interior may brush floor too, but the turn loop confines the player to the recorded
/// path, so a wall cell that happens to touch floor is never an exit (§10.7).
fn route_duct(
    facility: &Facility,
    wall_a: Cell,
    wall_b: Cell,
    used: &HashSet<Cell>,
) -> Option<Vec<Cell>> {
    let passable = |c: Cell| {
        c.x >= 1
            && c.y >= 1
            && c.x < facility.width() - 1
            && c.y < facility.height() - 1
            && facility.terrain(c) == Some(Terrain::Wall)
            && !used.contains(&c)
    };
    // wall_a / wall_b are themselves plain Wall cells at this point (they become
    // entries only on commit), so they satisfy `passable`.
    if !passable(wall_a) || !passable(wall_b) {
        return None;
    }
    let mut parent: HashMap<Cell, Cell> = HashMap::new();
    let mut frontier = VecDeque::new();
    frontier.push_back(wall_a);
    parent.insert(wall_a, wall_a);
    while let Some(cell) = frontier.pop_front() {
        if cell == wall_b {
            // Reconstruct the path a → … → b.
            let mut path = vec![wall_b];
            let mut cur = wall_b;
            while cur != wall_a {
                cur = parent[&cur];
                path.push(cur);
            }
            path.reverse();
            return Some(path);
        }
        if path_len_so_far(&parent, cell) + 1 > DUCT_MAX_CELLS {
            continue; // prune: this branch can only exceed the cap
        }
        for n in facility.neighbours(cell) {
            if passable(n) {
                if let std::collections::hash_map::Entry::Vacant(slot) = parent.entry(n) {
                    slot.insert(cell);
                    frontier.push_back(n);
                }
            }
        }
    }
    None
}

/// The number of cells from the BFS root to `cell` along the parent chain — used to
/// prune branches that would exceed [`DUCT_MAX_CELLS`] before they are explored.
fn path_len_so_far(parent: &HashMap<Cell, Cell>, cell: Cell) -> usize {
    let mut n = 1;
    let mut cur = cell;
    while parent[&cur] != cur {
        cur = parent[&cur];
        n += 1;
    }
    n
}

/// The 4-connected **floor** walking distance between two mouths (§10.7 payoff): a BFS
/// over cells that do not block pathing (floor, open/closed panels, consoles, exits —
/// the §10.6 pathable set), or `None` if they are disconnected. This is what a duct's
/// crawl length is measured against.
fn floor_distance(facility: &Facility, from: Cell, to: Cell) -> Option<u32> {
    let passable = |c: Cell| facility.terrain(c).is_some_and(|t| !t.blocks_pathing());
    let mut dist: HashMap<Cell, u32> = HashMap::new();
    dist.insert(from, 0);
    let mut frontier = VecDeque::new();
    frontier.push_back(from);
    while let Some(cell) = frontier.pop_front() {
        if cell == to {
            return Some(dist[&cell]);
        }
        let d = dist[&cell];
        for n in facility.neighbours(cell) {
            if passable(n) {
                if let std::collections::hash_map::Entry::Vacant(slot) = dist.entry(n) {
                    slot.insert(d + 1);
                    frontier.push_back(n);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::generate;

    /// Generate a 40×40 layout from `seed` — the shipped path (`generate`), which now
    /// threads ducts as its last carve pass.
    fn level(seed: u64) -> crate::Layout {
        generate(40, 40, &mut Rng::new(seed)).expect("40x40 always generates")
    }

    /// The floor **mouth** of an entry cell: its single non-solid neighbour.
    fn mouth(facility: &Facility, entry: Cell) -> Cell {
        let floors: Vec<Cell> = facility
            .neighbours(entry)
            .filter(|&n| facility.can_enter(n, 1.0))
            .collect();
        assert_eq!(floors.len(), 1, "a duct entry has exactly one floor mouth");
        floors[0]
    }

    /// §12.4 determinism: the same seed threads the exact same ducts, cell for cell.
    #[test]
    fn ducts_are_deterministic_from_the_seed() {
        for seed in 0..40 {
            assert_eq!(
                level(seed).ducts(),
                level(seed).ducts(),
                "seed {seed} must thread identical ducts"
            );
        }
    }

    /// The pass never overshoots its cap (§10.7 `[START]`).
    #[test]
    fn a_level_never_exceeds_the_duct_cap() {
        for seed in 0..80 {
            assert!(level(seed).ducts().len() <= DUCT_RUNS_PER_LEVEL);
        }
    }

    /// The feature actually fires: across a sweep, some levels carry a duct — the
    /// mechanism is not silently dead (a duct that shortcuts nothing is rejected, so
    /// *zero everywhere* would be a bug worth catching).
    #[test]
    fn ducts_do_appear_across_a_seed_sweep() {
        let total: usize = (0..80).map(|s| level(s).ducts().len()).sum();
        assert!(total > 0, "no seed in the sweep threaded a duct");
    }

    /// The §10.7 geometry, asserted on every placed duct: each **entry** is a
    /// `DuctEntry` with exactly one floor mouth and three solid backing sides
    /// (§10.1.6); the **interior** stays plain `Wall`; and the path is orthogonally
    /// contiguous end to end.
    #[test]
    fn every_duct_has_recessed_entries_and_a_wall_interior() {
        for seed in 0..80 {
            let layout = level(seed);
            let facility = layout.facility();
            for duct in layout.ducts() {
                let cells = duct.cells();
                assert!(cells.len() >= DUCT_MIN_CELLS && cells.len() <= DUCT_MAX_CELLS);
                for &entry in &duct.entries() {
                    assert_eq!(
                        facility.terrain(entry),
                        Some(Terrain::DuctEntry),
                        "seed {seed}: entry {entry:?} must be a DuctEntry"
                    );
                    // Exactly one floor mouth; every other neighbour solid.
                    let floors = facility
                        .neighbours(entry)
                        .filter(|&n| facility.can_enter(n, 1.0))
                        .count();
                    assert_eq!(floors, 1, "seed {seed}: entry {entry:?} needs one mouth");
                }
                for &interior in &cells[1..cells.len() - 1] {
                    assert_eq!(
                        facility.terrain(interior),
                        Some(Terrain::Wall),
                        "seed {seed}: duct interior {interior:?} must stay Wall"
                    );
                }
                for w in cells.windows(2) {
                    assert_eq!(w[0].manhattan_distance(w[1]), 1);
                }
            }
        }
    }

    /// A placed duct is a **real shortcut** (§10.7): crawling it saves at least
    /// [`DUCT_MIN_PAYOFF`] steps over walking the floor between its mouths. A duct that
    /// barely beats the corridor is noise and must have been rejected.
    #[test]
    fn every_placed_duct_is_worth_the_crawl() {
        for seed in 0..80 {
            let layout = level(seed);
            let facility = layout.facility();
            for duct in layout.ducts() {
                let [a, b] = duct.entries();
                let walk = floor_distance(facility, mouth(facility, a), mouth(facility, b))
                    .expect("mouths are reachable on a §10.6-valid level");
                assert!(
                    walk.saturating_sub(duct.cells().len() as u32) >= DUCT_MIN_PAYOFF,
                    "seed {seed}: duct saves too little ({walk} walk vs {} crawl)",
                    duct.cells().len()
                );
            }
        }
    }

    /// Two ducts on one level never share a cell (§10.7): the `used` set keeps them
    /// disjoint, so crawl adjacency is unambiguous.
    #[test]
    fn ducts_on_a_level_do_not_overlap() {
        for seed in 0..80 {
            let layout = level(seed);
            let mut seen = HashSet::new();
            for duct in layout.ducts() {
                for &c in duct.cells() {
                    assert!(seen.insert(c), "seed {seed}: ducts share cell {c:?}");
                }
            }
        }
    }
}
