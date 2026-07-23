//! Wall-thickening phase (§10.1a): fatten thin walls so straight sightlines
//! stay bounded, without underrunning a room below its minimum.
//!
//! Part of the [`generate`](super) pipeline; `use super::*` pulls the shared
//! types, tuning constants, and sibling-phase helpers into scope.

use super::*;

/// Thicken about a third of the interior walls to two cells (§10.1.5).
///
/// A one-cell wall backs nothing: a cupboard cut into it would open straight through
/// to the far side (§10.1.6 forbids that traversal / sight-leak). So before the
/// hideout pass this thickens roughly `1 / WALL_THICKEN_ONE_IN` of the interior wall
/// runs to two cells, giving those cupboards solid backing and the facility a few
/// pilasters. Every thickened run grows **into a room**, never into a corridor — a
/// corridor is 2–4 wide and eating a lane could single-file it (the [SETTLED]
/// no-single-file rule), whereas a room is ≥6 and only loses an edge strip. Each
/// candidate cell is validated exactly like a sightline blocker ([`severs_pathing`],
/// [`splits_region`]) and kept clear of door throats, so thickening never seals a
/// route, splits a space, or clogs a doorway. Adding wall only ever shortens
/// sightlines, so the §10.1a rule below is untouched.
pub(super) fn thicken_walls(facility: &mut Facility, regions: &mut RegionGraph, rng: &mut Rng) {
    let (w, h) = (facility.width(), facility.height());
    let mut runs: Vec<WallRun> = Vec::new();
    for y in 1..h - 1 {
        collect_wall_runs(facility, Line::Row(y), w, &mut runs);
    }
    for x in 1..w - 1 {
        collect_wall_runs(facility, Line::Col(x), h, &mut runs);
    }
    for run in runs {
        if run.len < WALL_THICKEN_MIN_RUN || rng.below(WALL_THICKEN_ONE_IN) != 0 {
            continue;
        }
        thicken_run(facility, regions, &run);
    }
}

/// Walk one scan line and push each maximal run of plain [`Terrain::Wall`] cells,
/// interior only (the caller scans `1..extent-1`, so the border ring is never a run).
pub(super) fn collect_wall_runs(
    facility: &Facility,
    line: Line,
    extent: u32,
    out: &mut Vec<WallRun>,
) {
    let mut start: Option<u32> = None;
    for i in 1..extent - 1 {
        let is_wall = facility.terrain(line.cell(i)) == Some(Terrain::Wall);
        match (start, is_wall) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                out.push(WallRun {
                    line,
                    start: s,
                    len: i - s,
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(WallRun {
            line,
            start: s,
            len: extent - 1 - s,
        });
    }
}

/// Thicken one wall run by one cell on its room-facing side. The side is chosen once
/// for the whole run — whichever flank holds more room floor — so the second course
/// of wall is a clean parallel line, not a ragged one; a run with no room floor on
/// either side (both flanks corridor, or one the border) is left alone.
pub(super) fn thicken_run(facility: &mut Facility, regions: &mut RegionGraph, run: &WallRun) {
    let (mut near_room, mut far_room) = (0u32, 0u32);
    for i in run.start..run.start + run.len {
        let (near, far) = run.line.flanks(i);
        near_room += is_room_floor(facility, regions, near) as u32;
        far_room += is_room_floor(facility, regions, far) as u32;
    }
    if near_room == 0 && far_room == 0 {
        return;
    }
    let use_near = near_room >= far_room;
    for i in run.start..run.start + run.len {
        let (near, far) = run.line.flanks(i);
        thicken_cell(facility, regions, if use_near { near } else { far });
    }
}

/// Turn one room-floor cell into wall — the atomic thickening step — if it is safe:
/// the cell must be room floor, clear of any door throat, and its removal must
/// neither sever a patrol route ([`severs_pathing`]) nor split its room
/// ([`splits_region`]). Keeps the grid and region graph in lockstep.
pub(super) fn thicken_cell(facility: &mut Facility, regions: &mut RegionGraph, cell: Cell) {
    if !is_room_floor(facility, regions, cell)
        || facility
            .neighbours(cell)
            .any(|n| facility.terrain(n).is_some_and(is_door_terrain))
        || thinning_underruns_room(facility, regions, cell)
        || severs_pathing(facility, cell)
        || splits_region(regions, cell)
    {
        return;
    }
    facility.set_terrain(cell.x, cell.y, Terrain::Wall);
    regions.remove_cell(cell);
}

/// Whether eating `cell` would shrink its room's floor below the §10.1 6×6 minimum.
///
/// Thickening erodes a room's edge, and a room is only ever ≥6×6 by construction
/// (`MIN_LEFTOVER`) — so without this guard a run down a just-minimal room's wall
/// would thin it to 5. Measures the room's floor bounding box *without* `cell`; the
/// check is stateful across a run, so thickening eats an edge only while the room
/// keeps its margin and stops the moment it would breach the minimum.
pub(super) fn thinning_underruns_room(
    facility: &Facility,
    regions: &RegionGraph,
    cell: Cell,
) -> bool {
    let Some(id) = regions.region_at(cell) else {
        return false;
    };
    let (mut x0, mut x1, mut y0, mut y1) = (u32::MAX, 0u32, u32::MAX, 0u32);
    let mut any = false;
    for &c in regions.region(id).cells() {
        if c == cell || facility.terrain(c) != Some(Terrain::Floor) {
            continue;
        }
        any = true;
        x0 = x0.min(c.x);
        x1 = x1.max(c.x);
        y0 = y0.min(c.y);
        y1 = y1.max(c.y);
    }
    !any || (x1 - x0 + 1) < MIN_LEFTOVER || (y1 - y0 + 1) < MIN_LEFTOVER
}

/// Whether `cell` is plain floor owned by a **room** (not a corridor). Thickening
/// only ever grows into rooms, so this is the "may I eat this cell" test.
pub(super) fn is_room_floor(facility: &Facility, regions: &RegionGraph, cell: Cell) -> bool {
    facility.terrain(cell) == Some(Terrain::Floor)
        && regions.region_at(cell).map(|id| regions.kind(id)) == Some(RegionKind::Room)
}
