//! Region beats: the territory a guard's Calm patrol claims (§7.5, §10.5).
//!
//! The old territory was a box around the spawn cell — §7.5's named weakness: it
//! straddled walls, spilled into rooms the guard could not walk to, and had no
//! relationship to the building. The §10.5 region graph is the fix: a beat is a
//! **connected set of regions** — the station's own region grown outward across
//! door edges — so every cell of it is genuinely walkable from the station, and
//! the corridors joining a guard's rooms are first-class parts of its ground, not
//! space crossed incidentally. The farthest-uninspected sweep (§7.5 — keep it)
//! then drives the guard room → corridor → room through them.
//!
//! Growth prefers the unclaimed neighbour whose connecting door is **nearest the
//! station**: the beat hugs the guard's own wing of the building, and — the
//! best-effort spread §7.5 wants — guards stationed apart in the same room grow
//! toward their own nearest doors first, so their beats diverge where the level
//! allows. Everything is a deterministic function of the graph and the station
//! cell (§12.4): no randomness, ties broken by scan order.

use crate::cell::Cell;
use crate::region::{RegionGraph, RegionId};

/// How many regions a guard's beat claims (§7.5 **[START] = 4**) — the named
/// knob replacing the old `PATROL_RADIUS` box for Calm patrol on generated
/// levels. Four is a wing: typically the station's room, the corridor outside
/// it, and the neighbouring room or two — comparable ground to the old
/// 15-step disc, but shaped like the building.
pub(crate) const BEAT_REGIONS: usize = 4;

/// The regions of a beat grown from `station`'s region: up to `limit` regions,
/// connected across door edges, in claim order. Empty when `station` lies in no
/// region (a wall or doorway cell — no placed guard does).
///
/// Growth is greedy: each step claims the unclaimed neighbour whose connecting
/// door is nearest the station (Manhattan, to the door's nearest panel), so the
/// beat stays anchored to the guard's own corner of the level. Ties break on the
/// claim-then-door scan order, which is fixed — the whole walk is deterministic
/// (§12.4).
pub(crate) fn beat_regions(regions: &RegionGraph, station: Cell, limit: usize) -> Vec<RegionId> {
    let Some(start) = regions.region_at(station) else {
        return Vec::new();
    };
    let mut claimed = vec![start];
    while claimed.len() < limit {
        // The nearest-doored unclaimed neighbour of the claimed set. Strict `<`
        // keeps the first candidate in scan order on a tie.
        let mut best: Option<(u32, RegionId)> = None;
        for &region in &claimed {
            for (door_id, neighbour) in regions.neighbours(region) {
                if claimed.contains(&neighbour) {
                    continue;
                }
                let distance = regions
                    .door(door_id)
                    .panels()
                    .iter()
                    .map(|&panel| station.manhattan_distance(panel))
                    .min()
                    .expect("a door has at least one panel");
                if best.is_none_or(|(d, _)| distance < d) {
                    best = Some((distance, neighbour));
                }
            }
        }
        match best {
            Some((_, region)) => claimed.push(region),
            None => break, // nothing left to grow into — a small, sealed wing
        }
    }
    claimed
}

/// The cells of the beat grown from `station` — the regions'
/// [`beat_regions`] claims, flattened in claim order. This is what a placed
/// guard carries as its territory (§7.5); the patrol filters it to the
/// patrollable cells at sweep time, so a console stamped in later or a
/// furniture cell never becomes a sweep target.
pub(crate) fn beat_cells(regions: &RegionGraph, station: Cell, limit: usize) -> Vec<Cell> {
    beat_regions(regions, station, limit)
        .into_iter()
        .flat_map(|id| regions.region(id).cells().iter().copied())
        .collect()
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

    /// §7.5: the beat starts at the station's region and grows across door
    /// edges, one region per step of `limit` — and never past what connects.
    #[test]
    fn a_beat_grows_from_the_station_region_across_doors() {
        let (g, a, c, b) = strip();
        let station = Cell::new(2, 2); // in room A

        assert_eq!(beat_regions(&g, station, 1), vec![a]);
        assert_eq!(beat_regions(&g, station, 2), vec![a, c]);
        assert_eq!(beat_regions(&g, station, 3), vec![a, c, b]);
        // A limit past the level's regions claims what exists and stops.
        assert_eq!(beat_regions(&g, station, 10), vec![a, c, b]);

        // The cells are the claimed regions', flattened in claim order.
        let cells = beat_cells(&g, station, 2);
        assert_eq!(
            cells.len(),
            g.region(a).cells().len() + g.region(c).cells().len()
        );
        assert!(g.region(c).cells().iter().all(|c| cells.contains(c)));
    }

    /// §7.5 best-effort spread: two guards stationed apart in the same room
    /// grow toward their own nearest doors first, so their beats diverge where
    /// the level allows — deterministically, from the station cell alone.
    #[test]
    fn guards_stationed_apart_grow_different_beats() {
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

        // Deterministic: the same station always grows the same beat (§12.4).
        assert_eq!(by_west_door, beat_regions(&g, Cell::new(4, 2), 2));
    }

    /// A station in no region — a wall or doorway cell — has no beat to grow.
    #[test]
    fn a_station_outside_any_region_has_no_beat() {
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
}
