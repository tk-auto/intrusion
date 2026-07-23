//! Room-feature phase (§10.2): the interior cover a carved room earns —
//! partitions, pillars, and benches/tables — placed without severing pathing.
//!
//! Part of the [`generate`](super) pipeline; `use super::*` pulls the shared
//! types, tuning constants, and sibling-phase helpers into scope.

use super::*;

/// Carve one room feature — a partition wall or a pillar — into `room` (§10.1.5).
///
/// Runs up to [`FEATURE_ATTEMPTS`] attempts; each proposes a partition wall and a
/// pillar, and every viable proposal joins a pool. One is then chosen and stamped
/// as wall, and its cells are returned so the caller can withhold them from the
/// room's region — that withholding is what gives the region its true, non-rectangular
/// shape (§10.5). A room with an empty pool (nothing fit) is left as a plain
/// rectangle. Every proposal is validated against `facility`'s current floor, so a
/// feature can never seal a room off or run into a wall.
pub(super) fn carve_room_features(
    facility: &mut Facility,
    room: &Rect,
    rng: &mut Rng,
) -> HashSet<Cell> {
    let mut pool: Vec<Vec<Cell>> = Vec::new();
    for _ in 0..FEATURE_ATTEMPTS {
        if let Some(cells) = propose_partition(room, facility, rng) {
            pool.push(cells);
        }
        if let Some(cells) = propose_pillar(room, facility, rng) {
            pool.push(cells);
        }
    }
    if pool.is_empty() {
        return HashSet::new();
    }
    let chosen = pool.swap_remove(rng.below(pool.len() as u32) as usize);
    for &cell in &chosen {
        facility.set_terrain(cell.x, cell.y, Terrain::Wall);
    }
    chosen.into_iter().collect()
}

/// Propose a partition wall for `room`, or `None` if this draw does not fit
/// (§10.1.5). A 1-cell-thick stub juts perpendicularly in from one of the room's
/// four walls; the alcoves it opens make sight-line breaks and dead ends.
/// Orientation is weighted by the room's perpendicular extent — a tall room gets a
/// horizontal stub — and the proposal is rejected unless its footprint grown by 1
/// on its three free sides is clear floor, so a floor lane always survives on both
/// flanks and past the tip.
pub(super) fn propose_partition(
    room: &Rect,
    facility: &Facility,
    rng: &mut Rng,
) -> Option<Vec<Cell>> {
    if room.width() < MIN_ROOM_FOR_PARTITION || room.height() < MIN_ROOM_FOR_PARTITION {
        return None;
    }
    // Orientation weighted by perpendicular extent: a horizontal stub grows along x
    // and its flanking lanes run in y, so a tall room (large height) favours it.
    let horizontal = rng.below(room.width() + room.height()) < room.height();
    // `grow_axis` is the length the stub extends along; `span_axis` the wall it
    // anchors to, measured perpendicular.
    let (grow_axis, span_axis) = if horizontal {
        (room.width(), room.height())
    } else {
        (room.height(), room.width())
    };
    // Length 2..=(axis−1): a stub never spans the room, leaving an alcove past its
    // tip (§10.1.5). The grown-footprint check below still rejects a too-long draw.
    let len = rng.range_inclusive(PARTITION_MIN_LEN as i32, (grow_axis - 1) as i32) as u32;
    let from_low = rng.bool();
    // Offset along the anchoring wall, kept off both corners (1..=span−2) so a floor
    // lane survives on each flank of the base.
    let along = rng.range_inclusive(1, (span_axis - 2) as i32) as u32;

    // The base cell (adjacent to the anchoring wall), the inward growth step, and
    // the lateral step along the wall.
    let (base, grow, lat) = if horizontal {
        let y = room.y0 + along;
        if from_low {
            (Cell::new(room.x0, y), (1i32, 0i32), (0i32, 1i32))
        } else {
            (Cell::new(room.x1, y), (-1, 0), (0, 1))
        }
    } else {
        let x = room.x0 + along;
        if from_low {
            (Cell::new(x, room.y0), (0, 1), (1, 0))
        } else {
            (Cell::new(x, room.y1), (0, -1), (1, 0))
        }
    };

    let stub: Vec<Cell> = (0..len as i32)
        .map(|i| offset(base, (grow.0 * i, grow.1 * i)))
        .collect();
    // Clearance: both lateral flanks of every stub cell, plus the one cell past the
    // tip. The anchoring wall behind the base is deliberately not checked — a stub
    // is *supposed* to touch the wall it juts from.
    let mut clearance: Vec<Cell> = Vec::new();
    for &cell in &stub {
        clearance.push(offset(cell, lat));
        clearance.push(offset(cell, (-lat.0, -lat.1)));
    }
    clearance.push(offset(base, (grow.0 * len as i32, grow.1 * len as i32)));

    if stub
        .iter()
        .chain(&clearance)
        .all(|&c| is_clear(room, facility, c))
    {
        Some(stub)
    } else {
        None
    }
}

/// Propose a freestanding pillar for `room`, or `None` if this draw does not fit
/// (§10.1.5). A 2–4 by 2–4 block placed with a 1-cell floor margin on every side —
/// that margin is what keeps it *freestanding* and is the geometry hideouts later
/// carve into (§10.1.6). Rejected unless its footprint grown by 1 is clear floor.
pub(super) fn propose_pillar(room: &Rect, facility: &Facility, rng: &mut Rng) -> Option<Vec<Cell>> {
    if room.width() < MIN_ROOM_FOR_PILLAR || room.height() < MIN_ROOM_FOR_PILLAR {
        return None;
    }
    // A side up to 4, but capped so a 1-cell margin still fits inside the room.
    let pw = rng.range_inclusive(
        PILLAR_MIN_SIDE as i32,
        PILLAR_MAX_SIDE.min(room.width() - 2) as i32,
    ) as u32;
    let ph = rng.range_inclusive(
        PILLAR_MIN_SIDE as i32,
        PILLAR_MAX_SIDE.min(room.height() - 2) as i32,
    ) as u32;
    // Top-left corner, kept ≥1 off every wall so the grown footprint stays interior.
    let px = rng.range_inclusive((room.x0 + 1) as i32, (room.x1 - pw) as i32) as u32;
    let py = rng.range_inclusive((room.y0 + 1) as i32, (room.y1 - ph) as i32) as u32;

    let block: Vec<Cell> = (py..py + ph)
        .flat_map(|y| (px..px + pw).map(move |x| Cell::new(x, y)))
        .collect();
    // The whole block grown by 1 must be clear floor — a 1-cell moat all around.
    let grown = (py - 1..=py + ph).flat_map(|y| (px - 1..=px + pw).map(move |x| Cell::new(x, y)));
    if grown.into_iter().all(|c| is_clear(room, facility, c)) {
        Some(block)
    } else {
        None
    }
}

/// Whether `cell` lies inside `room`'s floor rectangle and is currently floor — the
/// "clear" test both feature proposals reject against (§10.1.5). Cells outside the
/// room (a boundary wall, another region) are not clear, so a feature can neither
/// touch a wall it doesn't anchor to nor collide with an already-placed feature.
pub(super) fn is_clear(room: &Rect, facility: &Facility, cell: Cell) -> bool {
    room.contains(cell) && facility.terrain(cell) == Some(Terrain::Floor)
}

/// Stamp a 2×2 wall pillar whose corner sits on run position `i`, spanning one
/// step along the run and one lane toward `dir` — the last-resort §10.1a repair
/// for a corridor stretch too open for any recess ([`recess_run_hideout`]).
/// Every cell must be corridor floor, not a cupboard's mouth (walling a mouth
/// seals the cupboard), and each loss must sever no patrol route and split no
/// region; legality is judged incrementally as cells are stamped, and a pillar
/// that cannot complete is rolled back — floor and region membership both.
pub(super) fn place_pillar(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    run: &SightRun,
    i: u32,
    dir: Direction,
) -> bool {
    let corner = run.line.cell(i);
    let along = run.line.cell(i + 1);
    let Some(side_a) = corner.step(dir) else {
        return false;
    };
    let Some(side_b) = along.step(dir) else {
        return false;
    };
    let may_join = |facility: &Facility, regions: &RegionGraph, cell: Cell| {
        facility.terrain(cell) == Some(Terrain::Floor)
            && regions
                .region_at(cell)
                .is_some_and(|id| regions.kind(id) == RegionKind::Corridor)
            && facility
                .neighbours(cell)
                .all(|n| facility.terrain(n) != Some(Terrain::Hideout))
            && !severs_pathing(facility, cell)
            && !splits_region(regions, cell)
    };
    let mut stamped: Vec<(Cell, RegionId)> = Vec::new();
    for cell in [corner, along, side_a, side_b] {
        if !may_join(facility, regions, cell) {
            break;
        }
        let region = regions
            .region_at(cell)
            .expect("a pillar cell is claimed corridor floor");
        regions.remove_cell(cell);
        facility.set_terrain(cell.x, cell.y, Terrain::Wall);
        stamped.push((cell, region));
    }
    if stamped.len() == 4 {
        return true;
    }
    for &(cell, region) in stamped.iter().rev() {
        facility.set_terrain(cell.x, cell.y, Terrain::Floor);
        regions.add_cell(region, cell);
    }
    false
}

/// Whether flank `wall` could recess a cupboard opening onto `mouth` if the floor
/// cell on its far side were first walled up — the alcove fallback of
/// [`recess_run_hideout`], for a run flanked by one-thick walls only. Valid when
/// `wall`'s neighbours are exactly the floor `mouth`, two lateral walls, and one
/// far-side floor cell (`back`) that can go quietly: eating it must sever no
/// patrol route and split no region — the same tests a thickened wall cell passes
/// (§10.1.5). A **room** back must also keep its room at the §10.1 6×6 floor
/// minimum; a **corridor** back is a single-cell dent in the space behind — a
/// §10.1a squeeze, not the lane-eating thicken §10.1.5 forbids, and the
/// sever/split guards keep it a dent. Returns `back`.
pub(super) fn alcove_site(
    facility: &Facility,
    regions: &RegionGraph,
    wall: Cell,
    mouth: Cell,
) -> Option<Cell> {
    if facility.terrain(wall) != Some(Terrain::Wall) {
        return None;
    }
    let mut back = None;
    let mut walls = 0;
    for n in facility.neighbours(wall) {
        match facility.terrain(n) {
            _ if n == mouth => {
                if facility.terrain(n) != Some(Terrain::Floor) {
                    return None;
                }
            }
            Some(Terrain::Wall) => walls += 1,
            Some(Terrain::Floor) if back.is_none() => back = Some(n),
            _ => return None,
        }
    }
    let back = back?;
    if walls != 2 {
        return None;
    }
    // The walled-up cell must also not be another cupboard's mouth (walling it
    // would seal that cupboard) and must not butt against a bench (a wall landing
    // mid-bench would break its furniture pose after the fact).
    let kind = regions.region_at(back).map(|id| regions.kind(id));
    (matches!(kind, Some(RegionKind::Room | RegionKind::Corridor))
        && facility.neighbours(back).all(|n| {
            !matches!(
                facility.terrain(n),
                Some(Terrain::Hideout | Terrain::PartialCover)
            )
        })
        && (kind != Some(RegionKind::Room) || !thinning_underruns_room(facility, regions, back))
        && !severs_pathing(facility, back)
        && !splits_region(regions, back))
    .then_some(back)
}

/// Break `run` with a **bench** — a short straight row of partial-cover tables that
/// reads as one placed piece of furniture (a workbench, a desk) — and release its
/// cells from the region graph, keeping grid and graph in lockstep. Returns whether
/// a bench landed.
///
/// Rooms only: a bench is room furniture (§10.1.6) — a corridor run is repaired by
/// [`recess_run_hideout`] instead, and [`can_take_table`] refuses corridor floor
/// outright. The seed cell is chosen along the run centre-out from a seeded
/// jittered aim point, so cover varies by seed instead of forming a metronomic
/// grid. From the seed the bench grows in a straight line — across the run or
/// along it, tried in a seed-flipped order — toward a drawn target length of
/// [`COVER_BENCH_MIN`]..=[`COVER_BENCH_MAX`] cells ([`try_bench`]). A line that
/// cannot reach [`COVER_BENCH_MIN`] cells or lands in no recognisable furniture
/// pose ([`bench_pose`]) is rolled back and the next candidate tried: every bench
/// that ships is 2+ cells posed like furniture — never the lone-table confetti the
/// first version scattered.
///
/// Two tables flanking one floor cell are avoidable clutter: that cell's usable
/// line shows the *same* `crouch` hint twice, once per arrow (§11.4, #75). So a
/// first pass admits only benches that make no such table+table double, and only
/// the mandatory floor pass — where breaking the run is not negotiable — falls
/// back to a doubling bench. `strict` (the #91 room-preference pass) never falls
/// back: that cover is optional, so a run it could only break by doubling a
/// crouch hint is left alone instead.
pub(super) fn place_bench(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    run: &SightRun,
    rng: &mut Rng,
    strict: bool,
) -> bool {
    // Aim at the middle, jittered by up to a sixth of the run either way.
    let jitter = (run.len / 6).max(1) as i32;
    let aim = (run.start + run.len / 2) as i32 + rng.range_inclusive(-jitter, jitter);
    let aim = aim.clamp(run.start as i32, (run.start + run.len - 1) as i32) as u32;

    let mut order: Vec<u32> = (run.start..run.start + run.len).collect();
    order.sort_by_key(|&i| (i.abs_diff(aim), i));

    // The target length and the growth-axis order are drawn up front, once per
    // placement — not per candidate — so the stream stays one draw pair per bench
    // (§12.4) however many candidates reject.
    let target = COVER_BENCH_MIN + rng.below(COVER_BENCH_MAX - COVER_BENCH_MIN + 1);
    let (across, along) = match run.line {
        Line::Row(_) => (
            [Direction::North, Direction::South],
            [Direction::West, Direction::East],
        ),
        Line::Col(_) => (
            [Direction::East, Direction::West],
            [Direction::North, Direction::South],
        ),
    };
    let axes = if rng.below(2) == 0 {
        [across, along]
    } else {
        [along, across]
    };

    let passes: &[bool] = if strict { &[false] } else { &[false, true] };
    for &allow_double in passes {
        for &i in &order {
            for axis in axes {
                if try_bench(
                    facility,
                    regions,
                    run.line.cell(i),
                    axis,
                    target,
                    allow_double,
                ) {
                    return true;
                }
            }
        }
    }
    false
}

/// Grow and stamp one bench through `seed` along `axis`, rolling the whole line
/// back unless it reaches [`COVER_BENCH_MIN`] cells *and* lands in a furniture
/// pose ([`bench_pose`]). Returns whether the bench was kept.
///
/// Cells are stamped as the line grows because legality is incremental: each next
/// cell is judged (`severs_pathing`, `splits_region`) on the grid with the bench
/// so far already solid. The rollback restores plain floor and re-claims each
/// cell for its region, so an abandoned attempt leaves no trace.
pub(super) fn try_bench(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    seed: Cell,
    axis: [Direction; 2],
    target: u32,
    allow_double: bool,
) -> bool {
    // A cell may join the bench if a table is legal there at all, it doesn't
    // double a neighbour's crouch hint (unless this is the mandatory fallback),
    // and it touches no table other than the bench cell it grows from — two
    // benches must never merge into an L or a T; each is one straight piece.
    let may_join = |facility: &Facility, regions: &RegionGraph, cell: Cell, prev: Option<Cell>| {
        can_take_table(facility, regions, cell)
            && (allow_double || !creates_table_double(facility, cell))
            && facility
                .neighbours(cell)
                .all(|n| Some(n) == prev || facility.terrain(n) != Some(Terrain::PartialCover))
    };

    if !may_join(facility, regions, seed, None) {
        return false;
    }
    let mut stamped: Vec<(Cell, RegionId)> = Vec::new();
    let region = regions
        .region_at(seed)
        .expect("a bench cell is claimed room floor");
    stamp_table(facility, regions, seed);
    stamped.push((seed, region));
    for dir in axis {
        let mut c = seed;
        while (stamped.len() as u32) < target {
            let Some(n) = c.step(dir) else { break };
            if !may_join(facility, regions, n, Some(c)) {
                break;
            }
            let region = regions
                .region_at(n)
                .expect("a bench cell is claimed room floor");
            stamp_table(facility, regions, n);
            stamped.push((n, region));
            c = n;
        }
    }

    let cells: Vec<Cell> = stamped.iter().map(|&(c, _)| c).collect();
    if (cells.len() as u32) >= COVER_BENCH_MIN && bench_pose(facility, &cells).is_some() {
        return true;
    }
    // Not furniture — roll the whole line back, floor and region membership both.
    for &(cell, region) in stamped.iter().rev() {
        facility.set_terrain(cell.x, cell.y, Terrain::Floor);
        regions.add_cell(region, cell);
    }
    false
}

/// Classify the straight line `cells` against the walls around it, or `None` when
/// the contact pattern is one no placed piece has — a wall stub brushing the line
/// mid-bench, wall contact at both ends (that is a partition, not furniture),
/// mixed sides. Callers roll such a line back. Contact means solid structure:
/// [`Terrain::Wall`] or a door hinge.
pub(super) fn bench_pose(facility: &Facility, cells: &[Cell]) -> Option<BenchPose> {
    let mut line: Vec<Cell> = cells.to_vec();
    line.sort_unstable_by_key(|c| (c.x, c.y));
    let (first, last) = (line[0], line[line.len() - 1]);
    let vertical = first.x == last.x;
    let (lateral, out_first, out_last) = if vertical {
        (
            [Direction::West, Direction::East],
            Direction::North,
            Direction::South,
        )
    } else {
        (
            [Direction::North, Direction::South],
            Direction::West,
            Direction::East,
        )
    };
    let walled = |cell: Cell, dir: Direction| {
        cell.step(dir)
            .and_then(|n| facility.terrain(n))
            .is_some_and(|t| matches!(t, Terrain::Wall | Terrain::DoorHinge))
    };

    let end_hits = u32::from(walled(first, out_first)) + u32::from(walled(last, out_last));
    let flush = |side: Direction| line.iter().all(|&c| walled(c, side));
    let any_lateral = line
        .iter()
        .any(|&c| walled(c, lateral[0]) || walled(c, lateral[1]));

    if !any_lateral {
        return match end_hits {
            0 => Some(BenchPose::FreeStanding),
            1 => Some(BenchPose::EndOn),
            _ => None,
        };
    }
    // Lateral contact is only furniture when the whole side hugs one wall; a
    // counter may run into a corner (one walled end) but never wall-to-wall.
    if (flush(lateral[0]) || flush(lateral[1])) && end_hits <= 1 {
        return Some(BenchPose::AlongWall);
    }
    None
}

/// Whether a table may be stamped on `cell`: plain **room** floor — never a
/// corridor's (§10.1.6: corridor counterplay is the recessed cupboard, not
/// furniture) — not the mouth of a cupboard (a table there would seal the only way
/// in), and turning it solid keeps the room at its §10.1 6×6 floor minimum
/// ([`thinning_underruns_room`] — a bench is not floor, so it erodes the bounding
/// box exactly like a thickened wall), severs no patrol route
/// ([`severs_pathing`]) and splits no region ([`splits_region`]).
pub(super) fn can_take_table(facility: &Facility, regions: &RegionGraph, cell: Cell) -> bool {
    facility.terrain(cell) == Some(Terrain::Floor)
        && regions
            .region_at(cell)
            .is_some_and(|id| regions.kind(id) == RegionKind::Room)
        && facility
            .neighbours(cell)
            .all(|n| facility.terrain(n) != Some(Terrain::Hideout))
        && !thinning_underruns_room(facility, regions, cell)
        && !severs_pathing(facility, cell)
        && !splits_region(regions, cell)
}

/// Stamp a table on `cell` and drop it from its region, in lockstep (§10.5).
pub(super) fn stamp_table(facility: &mut Facility, regions: &mut RegionGraph, cell: Cell) {
    facility.set_terrain(cell.x, cell.y, Terrain::PartialCover);
    regions.remove_cell(cell);
}

/// Whether stamping a table at `cell` would give some floor neighbour a **second**
/// adjacent table — the §11.4 doubled-crouch case, where one floor cell shows
/// `→ table: crouch` *and* `↑ table: crouch`. Table-specific on purpose: a table
/// beside a *door* is the doubling §11.4 accepts (corridors are door-rich), so this
/// looks only at partial-cover neighbours, not the whole usable set.
pub(super) fn creates_table_double(facility: &Facility, cell: Cell) -> bool {
    facility.neighbours(cell).any(|f| {
        facility.terrain(f) == Some(Terrain::Floor)
            && facility
                .neighbours(f)
                .any(|n| facility.terrain(n) == Some(Terrain::PartialCover))
    })
}

/// Whether removing `cell` would split the region that owns it into disconnected
/// pieces. A region is a coherent space (§10.5) — a blocker may narrow a room or
/// corridor, never partition it. This is the region-local complement of
/// [`severs_pathing`]: *pathing* can survive a split (guards detour through a
/// door), but the space itself must stay whole, or "which room am I in" stops
/// meaning anything.
pub(super) fn splits_region(regions: &RegionGraph, cell: Cell) -> bool {
    let Some(id) = regions.region_at(cell) else {
        return false; // unclaimed cells partition nothing
    };
    let cells: HashSet<Cell> = regions
        .region(id)
        .cells()
        .iter()
        .copied()
        .filter(|&c| c != cell)
        .collect();
    let Some(&flood_start) = cells.iter().next() else {
        return true; // the region's only cell — removing it deletes the space
    };
    // Region-local: this floods only the region's own cell set, not the full grid, so
    // it stays `path::flood_from`'s O(region) sibling rather than being folded into it.
    let mut seen: HashSet<Cell> = HashSet::new();
    let mut stack = vec![flood_start];
    while let Some(c) = stack.pop() {
        if !seen.insert(c) {
            continue;
        }
        for dir in Direction::ALL {
            if let Some(n) = c.step(dir) {
                if cells.contains(&n) && !seen.contains(&n) {
                    stack.push(n);
                }
            }
        }
    }
    seen.len() != cells.len()
}
