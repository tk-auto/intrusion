//! Region beats: the territory a guard's Calm patrol claims (§7.5, §10.5).
//!
//! The old territory was a box around the spawn cell — §7.5's named weakness: it
//! straddled walls, spilled into rooms the guard could not walk to, and had no
//! relationship to the building. The §10.5 region graph is the fix: a beat is a
//! **connected set of regions** — the anchor's own region grown outward across
//! door edges — so every cell of it is genuinely walkable from the anchor, and
//! the corridors joining a guard's rooms are first-class parts of its ground, not
//! space crossed incidentally. The farthest-uninspected sweep (§7.5 — keep it)
//! then drives the guard room → corridor → room through them.
//!
//! **An anchor is a guard's live position, not the cell it spawned at.** That is the
//! other half of §7.5's weakness: a remembered spawn point is as unrelated to where
//! a guard has got to as a box is to the building. It matters most for a
//! reinforcement (§7.3/#374), which walks in at the far end of the map and would
//! otherwise be tethered to the arrival room for the rest of the level. The price is
//! that growth must be *called* rarely — at placement, and when the guard set changes
//! — because an anchor that moves every turn would make patrols churn.
//!
//! Growth prefers the unclaimed neighbour whose connecting door is **nearest the
//! anchor**: the beat hugs the guard's own wing of the building, and — the
//! best-effort spread §7.5 wants — guards anchored apart in the same room grow
//! toward their own nearest doors first, so their beats diverge where the level
//! allows. Everything is a deterministic function of the graph and the anchor
//! cell (§12.4): no randomness, ties broken by scan order.
//!
//! Grown *independently*, that spread is only best-effort: two guards anchored
//! near the same door still claim the same wing, the §7.5 "grind the same ground
//! while a wing goes uncovered" weakness. [`coordinated_beats`] closes it — it
//! grows every guard's beat with knowledge of the others, preferring a region **no
//! other guard already patrols**, so the beats fan out to cover distinct regions
//! where the graph allows. Coordination changes *which* regions a beat claims,
//! never *how many* (each beat still fills its reachable component up to the
//! limit), so it is pure coverage spread, not a bigger or smaller territory.

use crate::cell::Cell;
use crate::region::{DoorId, RegionGraph, RegionId};
use std::collections::HashMap;

/// How many regions a guard's beat claims (§7.5 **[START] = 4**) — the named
/// knob replacing the old `PATROL_RADIUS` box for Calm patrol on generated
/// levels. Four is a wing: typically the anchor's room, the corridor outside
/// it, and the neighbouring room or two — comparable ground to the old
/// 15-step disc, but shaped like the building.
pub(crate) const BEAT_REGIONS: usize = 4;

/// Manhattan distance from `anchor` to the door's nearest panel — how a beat
/// measures "which neighbour is closest to my corner of the level" (§7.5).
fn door_distance(regions: &RegionGraph, door: DoorId, anchor: Cell) -> u32 {
    regions
        .door(door)
        .panels()
        .iter()
        .map(|&panel| anchor.manhattan_distance(panel))
        .min()
        .expect("a door has at least one panel")
}

/// The regions of a *single* guard's beat grown from `anchor`'s region — the
/// independent-growth result, i.e. [`coordinated_beats`] for one guard, which has
/// no other beats to spread away from. Up to `limit` regions, connected across
/// door edges, in claim order; empty when `anchor` lies in no region. This is the
/// baseline the coordinated grower is measured against and the shape the per-guard
/// §7.5 tests pin — production grows all guards' beats together via
/// [`coordinated_beats`], so this is a test-only convenience.
#[cfg(test)]
pub(crate) fn beat_regions(regions: &RegionGraph, anchor: Cell, limit: usize) -> Vec<RegionId> {
    coordinated_beats(regions, &[anchor], limit)
        .pop()
        .unwrap_or_default()
}

/// The cells of a single guard's [`beat_regions`], flattened in claim order — the
/// territory a lone placed guard carries (§7.5). Test-only, like [`beat_regions`].
#[cfg(test)]
pub(crate) fn beat_cells(regions: &RegionGraph, anchor: Cell, limit: usize) -> Vec<Cell> {
    beat_regions(regions, anchor, limit)
        .into_iter()
        .flat_map(|id| regions.region(id).cells().iter().copied())
        .collect()
}

/// The region beats of a whole set of guards, grown **cooperatively** so their
/// territories cover distinct regions instead of doubling up (§7.5/§7.7 coverage,
/// the "fixes itself" step §7.5 promises once §10.5's spatial model exists). One
/// beat per anchor, in `anchors` order.
///
/// Every beat is seeded with its guard's own anchor region, then grown in
/// **round-robin** — one region per guard per round — so each guard's next claim
/// sees the ground every other guard has taken so far. The growth step prefers,
/// among a beat's unclaimed neighbours, the region **covered by the fewest other
/// beats** (an uncovered wing first); ties fall back to the same nearest-door hug
/// as the independent grower, so a beat still anchors to its guard's corner. Cover
/// never *blocks* a claim — a beat always grows into some reachable region if one
/// is free — so each beat fills its reachable component up to `limit` exactly as
/// independent single-guard growth would: coordination reshapes *which* regions,
/// never how many.
///
/// With more guards than regions the beats share (they cannot each own a distinct
/// wing), but every guard still gets a non-empty, anchor-connected beat. Fully
/// deterministic (§12.4): anchors order and the graph's scan order decide every
/// tie; no randomness.
///
/// **Call this rarely.** The anchors are guards' *live positions*, so this is a
/// function of a value that changes every turn — fine only because the callers are
/// placement ([`Placement::guards`](crate::Placement)) and the settle pass a
/// reinforcement's errand ends in
/// ([`State::settle_new_beats`](crate::State)). Called from the per-turn path it
/// would re-cut every territory under every guard each turn, and patrols would
/// visibly churn.
pub(crate) fn coordinated_beats(
    regions: &RegionGraph,
    anchors: &[Cell],
    limit: usize,
) -> Vec<Vec<RegionId>> {
    // Seed each beat with its anchor's region — a placed guard always stands in
    // one, so every beat is non-empty, connected, and walkable from the anchor by
    // construction. An anchor in no region (a wall/doorway cell — no placed guard)
    // yields an empty beat, matching independent single-guard growth.
    let mut beats: Vec<Vec<RegionId>> = anchors
        .iter()
        .map(|&anchor| regions.region_at(anchor).into_iter().collect())
        .collect();

    // How many beats already claim each region — the overlap coordination minimises
    // by steering each next claim toward a less-covered region.
    let mut coverage: HashMap<RegionId, usize> = HashMap::new();
    for beat in &beats {
        for &region in beat {
            *coverage.entry(region).or_default() += 1;
        }
    }

    // Round-robin growth: interleaving the guards (rather than growing each beat to
    // full before the next starts) is what lets a later guard react to an earlier
    // guard's claims and peel off to its own wing.
    loop {
        let mut grew = false;
        for (beat_idx, &anchor) in anchors.iter().enumerate() {
            if beats[beat_idx].len() >= limit {
                continue;
            }
            if let Some(region) =
                least_covered_neighbour(regions, &beats[beat_idx], anchor, &coverage)
            {
                beats[beat_idx].push(region);
                *coverage.entry(region).or_default() += 1;
                grew = true;
            }
        }
        if !grew {
            break; // every beat is full or boxed into its own reachable component
        }
    }
    beats
}

/// The cells of [`coordinated_beats`], each beat's claimed regions flattened in
/// claim order — the territory each placed guard carries (§7.5), one list per
/// anchor in `anchors` order.
pub(crate) fn coordinated_beat_cells(
    regions: &RegionGraph,
    anchors: &[Cell],
    limit: usize,
) -> Vec<Vec<Cell>> {
    coordinated_beats(regions, anchors, limit)
        .into_iter()
        .map(|beat| {
            beat.into_iter()
                .flat_map(|id| regions.region(id).cells().iter().copied())
                .collect()
        })
        .collect()
}

/// The next region to grow `beat` (a guard anchored at `anchor`) into: an
/// unclaimed neighbour of the beat, chosen to first minimise how many other beats
/// already `coverage` it — spreading the guards — then, on a tie, hug the anchor
/// nearest-door first (§7.5). `None` when the beat holds its whole reachable
/// component. Coverage steers but never gates: a free neighbour is always
/// returned if one exists, so the beat grows to the same size the independent
/// grower would reach. Strict `<` on the `(cover, distance)` key keeps the first
/// candidate in scan order on an exact tie, so the choice is deterministic (§12.4).
fn least_covered_neighbour(
    regions: &RegionGraph,
    beat: &[RegionId],
    anchor: Cell,
    coverage: &HashMap<RegionId, usize>,
) -> Option<RegionId> {
    let mut best: Option<(usize, u32, RegionId)> = None;
    for &region in beat {
        for (door_id, neighbour) in regions.neighbours(region) {
            if beat.contains(&neighbour) {
                continue;
            }
            let cover = coverage.get(&neighbour).copied().unwrap_or(0);
            let distance = door_distance(regions, door_id, anchor);
            if best.is_none_or(|(c, d, _)| (cover, distance) < (c, d)) {
                best = Some((cover, distance, neighbour));
            }
        }
    }
    best.map(|(_, _, region)| region)
}

#[cfg(test)]
mod tests {
    use super::*;
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

    /// Room A — corridor C — room B in a row, a door between each pair: the
    /// same shape as the region-graph fixture, seen from the beat's side.
    fn strip() -> (RegionGraph, RegionId, RegionId, RegionId) {
        let mut g = RegionGraph::new(12, 7);
        let a = g.add_region(RegionKind::Room, rect(1, 4, 1, 5));
        let c = g.add_region(RegionKind::Corridor, rect(5, 7, 1, 5));
        let b = g.add_region(RegionKind::Room, rect(8, 11, 1, 5));
        let (hinges, panels) = door_span(4);
        g.add_door(a, c, hinges, panels, DoorKind::Manual);
        let (hinges, panels) = door_span(7);
        g.add_door(c, b, hinges, panels, DoorKind::Manual);
        (g, a, c, b)
    }

    /// §7.5: the beat starts at the anchor's region and grows across door
    /// edges, one region per step of `limit` — and never past what connects.
    #[test]
    fn a_beat_grows_from_the_anchor_region_across_doors() {
        let (g, a, c, b) = strip();
        let anchor = Cell::new(2, 2); // in room A

        assert_eq!(beat_regions(&g, anchor, 1), vec![a]);
        assert_eq!(beat_regions(&g, anchor, 2), vec![a, c]);
        assert_eq!(beat_regions(&g, anchor, 3), vec![a, c, b]);
        // A limit past the level's regions claims what exists and stops.
        assert_eq!(beat_regions(&g, anchor, 10), vec![a, c, b]);

        // The cells are the claimed regions', flattened in claim order.
        let cells = beat_cells(&g, anchor, 2);
        assert_eq!(
            cells.len(),
            g.region(a).cells().len() + g.region(c).cells().len()
        );
        assert!(g.region(c).cells().iter().all(|c| cells.contains(c)));
    }

    /// §7.5 best-effort spread: two guards anchored apart in the same room
    /// grow toward their own nearest doors first, so their beats diverge where
    /// the level allows — deterministically, from the anchor cell alone.
    #[test]
    fn guards_anchored_apart_grow_different_beats() {
        // One room flanked by two corridors, a door to each side.
        let mut g = RegionGraph::new(12, 7);
        let west = g.add_region(RegionKind::Corridor, rect(1, 3, 1, 5));
        let room = g.add_region(RegionKind::Room, rect(4, 8, 1, 5));
        let east = g.add_region(RegionKind::Corridor, rect(9, 11, 1, 5));
        let (hinges, panels) = door_span(3);
        g.add_door(room, west, hinges, panels, DoorKind::Manual);
        let (hinges, panels) = door_span(8);
        g.add_door(room, east, hinges, panels, DoorKind::Manual);

        let by_west_door = beat_regions(&g, Cell::new(4, 2), 2);
        let by_east_door = beat_regions(&g, Cell::new(7, 2), 2);
        assert_eq!(by_west_door, vec![room, west]);
        assert_eq!(by_east_door, vec![room, east]);
        assert_ne!(by_west_door, by_east_door, "the beats diverge");

        // Deterministic: the same anchor always grows the same beat (§12.4).
        assert_eq!(by_west_door, beat_regions(&g, Cell::new(4, 2), 2));
    }

    /// An anchor in no region — a wall or doorway cell — has no beat to grow.
    #[test]
    fn an_anchor_outside_any_region_has_no_beat() {
        let (g, _, _, _) = strip();
        assert!(beat_regions(&g, Cell::new(0, 0), 3).is_empty(), "wall");
        assert!(beat_cells(&g, Cell::new(4, 2), 3).is_empty(), "doorway");
    }

    /// §7.5 **[START]** pin: the beat size is a named constant a later tune
    /// must move deliberately.
    #[test]
    fn the_beat_size_is_pinned() {
        assert_eq!(BEAT_REGIONS, 4, "the [START] beat size");
    }

    /// A hub room flanked by a west and an east corridor, a door to each — the
    /// shape where two guards anchored on the *same* side would grind the same
    /// wing under independent growth.
    fn hub() -> (RegionGraph, RegionId, RegionId, RegionId) {
        let mut g = RegionGraph::new(12, 7);
        let west = g.add_region(RegionKind::Corridor, rect(1, 3, 1, 5));
        let room = g.add_region(RegionKind::Room, rect(4, 8, 1, 5));
        let east = g.add_region(RegionKind::Corridor, rect(9, 11, 1, 5));
        let (hinges, panels) = door_span(3);
        g.add_door(room, west, hinges, panels, DoorKind::Manual);
        let (hinges, panels) = door_span(8);
        g.add_door(room, east, hinges, panels, DoorKind::Manual);
        (g, west, room, east)
    }

    /// §7.5 coverage: two guards anchored in the same room on the same side of it
    /// — where *independent* growth would send both down the nearer corridor —
    /// fan out to different wings when their beats are grown cooperatively.
    #[test]
    fn coordination_spreads_beats_that_independent_growth_would_overlap() {
        let (g, west, room, east) = hub();
        // Both anchors are nearer the west door than the east one, so independent
        // growth grabs the west corridor for each (the §7.5 grind).
        let anchors = [Cell::new(4, 2), Cell::new(5, 2)];
        for &s in &anchors {
            assert_eq!(
                beat_regions(&g, s, 2),
                vec![room, west],
                "independent grind"
            );
        }

        // Coordinated: the first guard takes the west wing, the second — seeing it
        // covered — peels off to the east.
        let beats = coordinated_beats(&g, &anchors, 2);
        assert_eq!(beats[0], vec![room, west]);
        assert_eq!(
            beats[1],
            vec![room, east],
            "the second guard covers the other wing"
        );

        // The union of coordinated beats spans every region; the overlapping
        // independent beats leave the east wing uncovered.
        let union = |beats: &[Vec<RegionId>]| {
            beats
                .iter()
                .flatten()
                .copied()
                .collect::<std::collections::HashSet<_>>()
        };
        let independent: Vec<Vec<RegionId>> =
            anchors.iter().map(|&s| beat_regions(&g, s, 2)).collect();
        assert_eq!(
            union(&beats).len(),
            3,
            "coordinated covers hub + both wings"
        );
        assert_eq!(
            union(&independent).len(),
            2,
            "independent leaves a wing uncovered"
        );
    }

    /// A guard already spread apart is untouched by coordination: two guards each
    /// nearer their own door still grow toward it, exactly as independent growth
    /// would — coordination only breaks *ties*, it doesn't shuffle a good spread.
    #[test]
    fn coordination_leaves_an_already_spread_pair_alone() {
        let (g, west, room, east) = hub();
        let anchors = [Cell::new(4, 2), Cell::new(7, 2)]; // by the west, by the east
        let beats = coordinated_beats(&g, &anchors, 2);
        assert_eq!(beats[0], vec![room, west]);
        assert_eq!(beats[1], vec![room, east]);
    }

    /// Graceful degradation (§7.5): with more guards than regions to divide, every
    /// guard still gets a non-empty beat that starts at its own anchor region and
    /// stays connected across doors — coverage overlaps, but no guard is starved.
    #[test]
    fn more_guards_than_regions_still_gives_each_a_connected_beat() {
        let (g, a, c, b) = strip(); // three regions
                                    // Four guards, two sharing room A.
        let anchors = [
            Cell::new(2, 2),
            Cell::new(3, 4),
            Cell::new(6, 2),
            Cell::new(9, 2),
        ];
        let beats = coordinated_beats(&g, &anchors, BEAT_REGIONS);
        assert_eq!(beats.len(), anchors.len());
        for (beat, &anchor) in beats.iter().zip(&anchors) {
            assert!(!beat.is_empty(), "every guard gets a beat");
            assert_eq!(
                beat[0],
                g.region_at(anchor).unwrap(),
                "a beat starts at its own anchor region"
            );
            assert!(
                beat_is_connected(&g, beat),
                "a beat is connected across doors"
            );
            // Three regions total, so a beat never exceeds them however high the limit.
            assert!(beat.len() <= 3);
        }
        // The whole level (all three regions) is covered.
        let covered: std::collections::HashSet<RegionId> =
            beats.iter().flatten().copied().collect();
        assert_eq!(covered, [a, c, b].into_iter().collect());
    }

    /// Coordination never changes a beat's *size*, only its composition: each
    /// coordinated beat holds exactly as many regions as the independent grower
    /// would give the same anchor (its reachable component, capped at the limit).
    #[test]
    fn coordination_preserves_each_beats_size() {
        let (g, ..) = hub();
        let anchors = [Cell::new(4, 2), Cell::new(5, 2)];
        for limit in 1..=4 {
            let coordinated = coordinated_beats(&g, &anchors, limit);
            for (&anchor, beat) in anchors.iter().zip(&coordinated) {
                assert_eq!(
                    beat.len(),
                    beat_regions(&g, anchor, limit).len(),
                    "limit {limit}: size changed for anchor {anchor:?}"
                );
            }
        }
    }

    /// §12.4: the coordinated assignment is a pure function of the graph and the
    /// anchors — the same inputs always grow the same beats.
    #[test]
    fn coordinated_beats_are_deterministic() {
        let (g, ..) = hub();
        let anchors = [Cell::new(4, 2), Cell::new(5, 2), Cell::new(7, 2)];
        assert_eq!(
            coordinated_beats(&g, &anchors, BEAT_REGIONS),
            coordinated_beats(&g, &anchors, BEAT_REGIONS)
        );
        // And the cell view agrees with the region view.
        let by_cells = coordinated_beat_cells(&g, &anchors, BEAT_REGIONS);
        let by_regions = coordinated_beats(&g, &anchors, BEAT_REGIONS);
        for (cells, regions) in by_cells.iter().zip(&by_regions) {
            let expected: Vec<Cell> = regions
                .iter()
                .flat_map(|&id| g.region(id).cells().iter().copied())
                .collect();
            assert_eq!(cells, &expected);
        }
    }

    /// Whether a beat's regions form one component connected across door edges —
    /// the §10.5 invariant every beat must keep, so a guard can actually walk its
    /// whole territory from its anchor.
    fn beat_is_connected(g: &RegionGraph, beat: &[RegionId]) -> bool {
        if beat.is_empty() {
            return true;
        }
        let held: std::collections::HashSet<RegionId> = beat.iter().copied().collect();
        let mut seen = std::collections::HashSet::from([beat[0]]);
        let mut frontier = vec![beat[0]];
        while let Some(region) = frontier.pop() {
            for (_, neighbour) in g.neighbours(region) {
                if held.contains(&neighbour) && seen.insert(neighbour) {
                    frontier.push(neighbour);
                }
            }
        }
        seen.len() == beat.len()
    }
}
