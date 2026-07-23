//! Carve phase (§10.1): recursive binary space partition into rooms and
//! corridors, plus the punch/seal geometry the split relies on.
//!
//! Part of the [`generate`](super) pipeline; `use super::*` pulls the shared
//! types, tuning constants, and sibling-phase helpers into scope.

use super::*;

/// The index of the largest-area region in the queue, with a deterministic
/// tie-break so the whole partition is reproducible from the seed.
pub(super) fn pick_largest(queue: &[Pending]) -> Option<usize> {
    queue
        .iter()
        .enumerate()
        .max_by_key(|(_, p)| (p.rect.area(), p.rect.x0, p.rect.y0))
        .map(|(i, _)| i)
}

/// Choose the split axis for a region, or `None` if it cannot be validly split.
///
/// An axis must **fit** (its dimension ≥ 16) and, unless this is the network's
/// first corridor, must **connect** — its two ends face an open side, so the
/// punch-through reaches an existing corridor. When both axes qualify it is a fair
/// coin (§10.1); when one does, that one; when neither, the region is a room.
pub(super) fn choose_axis(pending: &Pending, first_carve: bool, rng: &mut Rng) -> Option<Axis> {
    let rect = &pending.rect;
    let open = &pending.open;
    // A vertical corridor runs north–south (splitting the region east/west), so its
    // ends face N and S; a horizontal corridor's ends face E and W.
    let vertical = rect.width() >= MIN_SPLIT_AXIS && (first_carve || open.n || open.s);
    let horizontal = rect.height() >= MIN_SPLIT_AXIS && (first_carve || open.e || open.w);
    match (vertical, horizontal) {
        (true, true) => Some(if rng.bool() {
            Axis::Vertical
        } else {
            Axis::Horizontal
        }),
        (true, false) => Some(Axis::Vertical),
        (false, true) => Some(Axis::Horizontal),
        (false, false) => None,
    }
}

/// Carve a corridor through `pending` along `axis`, stamping it into `facility`,
/// recording it in `regions`, and returning the two leftover regions.
pub(super) fn carve(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    pending: &Pending,
    axis: Axis,
    rng: &mut Rng,
) -> (Pending, Pending) {
    match axis {
        Axis::Vertical => carve_vertical(facility, regions, pending, rng),
        Axis::Horizontal => carve_horizontal(facility, regions, pending, rng),
    }
}

/// Carve a north–south corridor, splitting the region into a left and right room.
pub(super) fn carve_vertical(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    pending: &Pending,
    rng: &mut Rng,
) -> (Pending, Pending) {
    let r = pending.rect;
    let width = r.width();

    let cw = corridor_width(width, rng);
    // Left space: at least MIN_LEFTOVER, leaving MIN_LEFTOVER + 2 walls + cw for the
    // corridor and the right space.
    let left =
        rng.range_inclusive(MIN_LEFTOVER as i32, (width - cw - MIN_LEFTOVER - 2) as i32) as u32;

    let wall_l = r.x0 + left;
    let cx0 = wall_l + 1;
    let cx1 = cx0 + cw - 1;
    let wall_r = cx1 + 1;

    // The corridor's two flanking walls run the full span (§10.1).
    for y in r.y0..=r.y1 {
        facility.set_terrain(wall_l, y, Terrain::Wall);
        facility.set_terrain(wall_r, y, Terrain::Wall);
    }

    let mut cells: Vec<Cell> = (r.y0..=r.y1)
        .flat_map(|y| (cx0..=cx1).map(move |x| Cell::new(x, y)))
        .collect();
    // Punch one cell past each end that faces the network, joining the corridor to
    // its parent (§10.1). Open sides are interior walls, so this never touches the
    // border.
    if pending.open.n {
        punch(facility, &mut cells, cx0..=cx1, r.y0 - 1);
    }
    if pending.open.s {
        punch(facility, &mut cells, cx0..=cx1, r.y1 + 1);
    }
    regions.add_region(RegionKind::Corridor, cells);

    // Each leftover gains the side facing the new corridor and keeps the others.
    let left_room = Pending {
        rect: Rect::new(r.x0, r.y0, wall_l - 1, r.y1),
        open: Open {
            e: true,
            ..pending.open
        },
    };
    let right_room = Pending {
        rect: Rect::new(wall_r + 1, r.y0, r.x1, r.y1),
        open: Open {
            w: true,
            ..pending.open
        },
    };
    (left_room, right_room)
}

/// Carve an east–west corridor, splitting the region into a top and bottom room.
pub(super) fn carve_horizontal(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    pending: &Pending,
    rng: &mut Rng,
) -> (Pending, Pending) {
    let r = pending.rect;
    let height = r.height();

    let cw = corridor_width(height, rng);
    let top =
        rng.range_inclusive(MIN_LEFTOVER as i32, (height - cw - MIN_LEFTOVER - 2) as i32) as u32;

    let wall_t = r.y0 + top;
    let cy0 = wall_t + 1;
    let cy1 = cy0 + cw - 1;
    let wall_b = cy1 + 1;

    for x in r.x0..=r.x1 {
        facility.set_terrain(x, wall_t, Terrain::Wall);
        facility.set_terrain(x, wall_b, Terrain::Wall);
    }

    let mut cells: Vec<Cell> = (cy0..=cy1)
        .flat_map(|y| (r.x0..=r.x1).map(move |x| Cell::new(x, y)))
        .collect();
    if pending.open.w {
        punch_column(facility, &mut cells, r.x0 - 1, cy0..=cy1);
    }
    if pending.open.e {
        punch_column(facility, &mut cells, r.x1 + 1, cy0..=cy1);
    }
    regions.add_region(RegionKind::Corridor, cells);

    let top_room = Pending {
        rect: Rect::new(r.x0, r.y0, r.x1, wall_t - 1),
        open: Open {
            s: true,
            ..pending.open
        },
    };
    let bottom_room = Pending {
        rect: Rect::new(r.x0, wall_b + 1, r.x1, r.y1),
        open: Open {
            n: true,
            ..pending.open
        },
    };
    (top_room, bottom_room)
}

/// A random corridor width in `[2, 4]`, capped so the two ≥6 leftovers still fit in
/// `axis` cells (§10.1). `axis ≥ 16` guarantees at least width 2.
pub(super) fn corridor_width(axis: u32, rng: &mut Rng) -> u32 {
    let max = CORRIDOR_MAX_WIDTH.min(axis - (MIN_LEFTOVER * 2 + 2));
    rng.range_inclusive(CORRIDOR_MIN_WIDTH as i32, max as i32) as u32
}

/// Open a horizontal run of wall (`xs` at row `y`) into corridor floor, recording
/// the opened cells as part of the corridor.
pub(super) fn punch(
    facility: &mut Facility,
    cells: &mut Vec<Cell>,
    xs: std::ops::RangeInclusive<u32>,
    y: u32,
) {
    for x in xs {
        facility.set_terrain(x, y, Terrain::Floor);
        cells.push(Cell::new(x, y));
    }
}

/// Open a vertical run of wall (column `x` over rows `ys`) into corridor floor.
pub(super) fn punch_column(
    facility: &mut Facility,
    cells: &mut Vec<Cell>,
    x: u32,
    ys: std::ops::RangeInclusive<u32>,
) {
    for y in ys {
        facility.set_terrain(x, y, Terrain::Floor);
        cells.push(Cell::new(x, y));
    }
}

/// Re-stamp the enclosing border as solid wall (§10.6, unconditional border ring).
pub(super) fn seal_border(facility: &mut Facility) {
    let (w, h) = (facility.width(), facility.height());
    for x in 0..w {
        facility.set_terrain(x, 0, Terrain::Wall);
        facility.set_terrain(x, h - 1, Terrain::Wall);
    }
    for y in 0..h {
        facility.set_terrain(0, y, Terrain::Wall);
        facility.set_terrain(w - 1, y, Terrain::Wall);
    }
}
