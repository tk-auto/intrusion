//! Corridor-first binary partition — the primary structure of a facility (§10.1).
//!
//! Most roguelikes place rooms and then connect them. This does the opposite, and
//! it is *right for this game*: corridors are where stealth happens, so generating
//! them first makes them deliberate spaces rather than plumbing (§10.1). We start
//! with the whole interior as one region, then repeatedly carve a corridor through
//! the **largest** remaining region — splitting it in two — until no region can be
//! split. Whatever regions survive become rooms; the corridors are the leftovers'
//! shared connective tissue.
//!
//! This ticket is steps 1–3 of §10.1: the partition itself. Doorways (§10.1.4),
//! room features (§10.1.5), hideouts (§10.1.6) and placement (§10.1.7–9) each land
//! in their own ticket and write into the same [`RegionGraph`] this produces.
//!
//! # Connectivity is by construction, not by luck
//!
//! §10.6 guarantees the corridor network is connected: "each corridor punches into
//! its parent → the network is a tree." The mechanism is the **punch-through** —
//! after stamping a corridor, we punch one cell past each end, opening the wall
//! into whatever lies beyond. For that to actually join the network (and never
//! breach the enclosing border), a carve must reach an *existing* corridor.
//!
//! So every region carries which of its four sides face the corridor network — its
//! **open sides**. A fresh leftover always gains the side facing the corridor that
//! just split it, so it always has at least one. A carve is only allowed on an axis
//! whose two ends face an open side (the first carve of all is the exception — it
//! seeds the tree and connects to nothing). The punch then fires only toward open
//! sides, which are interior walls backed by corridor — never the border. The
//! result: a connected network, and an enclosure that stays intact, both without a
//! reject-and-retry loop. This refines §10.1's raw "50/50 axis" so the §10.6 tree
//! guarantee holds by construction; the property tests below assert it over many
//! seeds.

use crate::cell::Cell;
use crate::facility::{Facility, Terrain};
use crate::region::{RegionGraph, RegionId, RegionKind};
use crate::rng::Rng;

/// Corridor width is random 2–4, **never single-file** (§10.1). **[SETTLED]** — a
/// single-file corridor is a death trap with no counterplay.
const CORRIDOR_MIN_WIDTH: u32 = 2;
const CORRIDOR_MAX_WIDTH: u32 = 4;
/// Each side of a split keeps at least this much depth, so every room is ≥ 6 in
/// every dimension (§10.1).
const MIN_LEFTOVER: u32 = 6;
/// The shortest axis a corridor can split: `6 + 1 + 2 + 1 + 6 = 16` — two ≥6
/// leftovers, two walls, and a minimum-width corridor between them (§10.1).
const MIN_SPLIT_AXIS: u32 = MIN_LEFTOVER * 2 + 2 + CORRIDOR_MIN_WIDTH;
/// The partition loop budget. Room count emerges from the geometry but is capped
/// here so a large map can't subdivide without bound — §10.2 puts it at ~12.
/// **[START]**.
const MAX_ROOMS: usize = 12;

/// The shortest wall run that gets a doorway (§10.1.4). Below this, cutting a door
/// would leave no frame.
const MIN_DOOR_RUN: u32 = 3;
/// The longest a single doorway spans (§10.4): two hinges and up to four panels.
const MAX_DOOR_LEN: u32 = 6;
/// New doors close behind their user by default (§10.4 auto-close **[START]**).
/// Kept as a constant, and per-door mutable via
/// [`Door::set_auto_close`](crate::Door::set_auto_close), so playtest can compare
/// the facility with auto-close on and off.
const AUTO_CLOSE: bool = true;
/// The most doorways any one room gets **[START]**. A room with a door on every
/// wall is a thoroughfare, not a room — most rooms want one or two ways in, and a
/// three-door hub should be the exception. Every room still keeps at least one
/// door, so none is ever sealed off (§10.6). The per-room count is drawn by
/// [`room_door_budget`].
const MAX_DOORS_PER_ROOM: u32 = 3;

/// Why a facility could not be generated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GenError {
    /// The facility is too small to partition: no corridor fits, so the interior
    /// would be a single room — which cannot host the entry, objectives and guards
    /// that must live in *different* rooms (§10.2). Below 18×18 in both axes this
    /// is unavoidable. Guard it; do not ship an unplaceable level.
    TooSmall { width: u32, height: u32 },
}

/// A generated facility: its terrain grid and the spatial region graph that names
/// every room and corridor in it (§10.5). The two are kept in lockstep — every
/// interior floor cell belongs to exactly one region, every wall to none.
#[derive(Clone, Debug)]
pub struct Layout {
    facility: Facility,
    regions: RegionGraph,
}

impl Layout {
    /// The terrain grid.
    pub fn facility(&self) -> &Facility {
        &self.facility
    }

    /// The region graph over that grid.
    pub fn regions(&self) -> &RegionGraph {
        &self.regions
    }

    /// Mutable access to both halves at once — the grid and its graph. Crate-internal
    /// and returned together because operating a door (§10.4) must move the graph's
    /// open/closed state and the panels' terrain in one step; the door runtime in
    /// [`crate::door`] is the only caller.
    pub(crate) fn parts_mut(&mut self) -> (&mut Facility, &mut RegionGraph) {
        (&mut self.facility, &mut self.regions)
    }

    /// Stamp a single terrain cell into the finished grid — the placement write
    /// (§10.1.7–9). Crate-internal: only generation and the turn loop's state
    /// construction place tiles onto a level. Region membership is placement's own
    /// bookkeeping (#12); this touches terrain only.
    pub(crate) fn place(&mut self, cell: Cell, terrain: Terrain) {
        self.facility.set_terrain(cell.x, cell.y, terrain);
    }
}

/// A bare layout over `facility` with no regions or doors — for tests and tools
/// that need a hand-made world without running the full generator. Real levels come
/// from [`generate`]; this just wraps a grid so the turn loop and the door seam can
/// operate on it.
#[cfg(test)]
impl Layout {
    pub(crate) fn from_facility(facility: Facility) -> Self {
        let (w, h) = (facility.width(), facility.height());
        Self {
            facility,
            regions: RegionGraph::new(w, h),
        }
    }
}

/// Generate a facility by corridor-first binary partition (§10.1 steps 1–3).
///
/// All randomness is drawn from `rng` (§12.4): same seed, same facility, forever.
/// Returns [`GenError::TooSmall`] for a footprint that cannot be partitioned into
/// at least two rooms, rather than silently producing an unplaceable level.
pub fn generate(width: u32, height: u32, rng: &mut Rng) -> Result<Layout, GenError> {
    // Step 1: one region covering the interior `(W-2) x (H-2)`. Below the minimum,
    // no corridor fits or a room could not reach 6×6 — reject rather than partition
    // into something unplaceable (§10.2).
    let (iw, ih) = (width.saturating_sub(2), height.saturating_sub(2));
    if iw < MIN_LEFTOVER || ih < MIN_LEFTOVER || iw.max(ih) < MIN_SPLIT_AXIS {
        return Err(GenError::TooSmall { width, height });
    }

    // The interior starts as solid floor inside the unconditional border ring.
    let mut facility = Facility::walled_box(width, height);
    let mut regions = RegionGraph::new(width, height);

    let interior = Rect::new(1, 1, width - 2, height - 2);
    let mut queue = vec![Pending::new(interior)];
    let mut rooms: Vec<Rect> = Vec::new();
    let mut corridors = 0usize;

    // Step 2: repeatedly carve through the largest region.
    while let Some(idx) = pick_largest(&queue) {
        let pending = queue.swap_remove(idx);

        // The budget counts every region still in flight: those already fixed as
        // rooms, those still queued, and this one. Once that reaches the cap, stop
        // carving and let the rest settle into rooms (§10.2).
        let in_flight = rooms.len() + queue.len() + 1;
        let first_carve = corridors == 0;
        let axis = if in_flight < MAX_ROOMS {
            choose_axis(&pending, first_carve, rng)
        } else {
            None
        };

        match axis {
            Some(axis) => {
                let (left, right) = carve(&mut facility, &mut regions, &pending, axis, rng);
                corridors += 1;
                queue.push(left);
                queue.push(right);
            }
            // Step 3: a region that cannot be split becomes a room.
            None => rooms.push(pending.rect),
        }
    }

    for room in &rooms {
        regions.add_region(RegionKind::Room, room.cells());
    }

    // §10.6: the border is enclosed unconditionally. Punch-throughs only fire
    // toward open sides (interior walls), never the border, so this re-stamp is a
    // guarantee, not a repair — but it makes the enclosure true by assertion, not
    // by argument (the §10.6 lesson).
    seal_border(&mut facility);

    // Step 4: cut doorways where a room meets a corridor, now that every region is
    // named (§10.1.4). Runs on the finished grid so it sees the true walls.
    place_doorways(&mut facility, &mut regions, rng);

    debug_assert!(corridors > 0, "guarded footprint yielded no corridor");
    Ok(Layout { facility, regions })
}

/// The index of the largest-area region in the queue, with a deterministic
/// tie-break so the whole partition is reproducible from the seed.
fn pick_largest(queue: &[Pending]) -> Option<usize> {
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
fn choose_axis(pending: &Pending, first_carve: bool, rng: &mut Rng) -> Option<Axis> {
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
fn carve(
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
fn carve_vertical(
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
fn carve_horizontal(
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
fn corridor_width(axis: u32, rng: &mut Rng) -> u32 {
    let max = CORRIDOR_MAX_WIDTH.min(axis - (MIN_LEFTOVER * 2 + 2));
    rng.range_inclusive(CORRIDOR_MIN_WIDTH as i32, max as i32) as u32
}

/// Open a horizontal run of wall (`xs` at row `y`) into corridor floor, recording
/// the opened cells as part of the corridor.
fn punch(
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
fn punch_column(
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
fn seal_border(facility: &mut Facility) {
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

/// Cut doorways where rooms meet corridors (§10.1.4), capping how many each room
/// gets so a room boxed in by corridors is not riddled with doors.
///
/// Two passes. **Collect:** scan every row, then every column, for **maximal runs**
/// of wall with interior on both flanks — the door candidates. "Maximal" is read
/// against the two regions a run separates: it breaks not only where a flank stops
/// being interior but also where the pair of flanking regions changes, so every
/// candidate joins one room to one corridor and never spans two rooms. **Choose:**
/// a room with a door on every wall is a thoroughfare, not a room, so each room
/// keeps only [`room_door_budget`]-many of its candidates (one or two usually,
/// three rarely) — but always at least one, so no room is sealed (§10.6). Because
/// rooms are always separated from each other by corridor-plus-two-walls, **every
/// door connects a room to a corridor** (§10.1.4, §10.6).
fn place_doorways(facility: &mut Facility, regions: &mut RegionGraph, rng: &mut Rng) {
    let (w, h) = (facility.width(), facility.height());
    let mut candidates: Vec<Candidate> = Vec::new();
    // Rows first: a horizontal wall with floor north and south is a run in x, and
    // its doorway joins the region above to the region below.
    for y in 1..h - 1 {
        collect_runs(regions, Line::Row(y), w, &mut candidates);
    }
    // Then columns: a vertical wall with floor west and east is a run in y.
    for x in 1..w - 1 {
        collect_runs(regions, Line::Col(x), h, &mut candidates);
    }

    for idx in choose_doors(&candidates, rng) {
        cut_door(facility, regions, rng, &candidates[idx]);
    }
}

/// A maximal wall run that could become a doorway: where it sits, how long it is,
/// and the room and corridor it would join.
#[derive(Clone, Copy)]
struct Candidate {
    line: Line,
    start: u32,
    len: u32,
    room: RegionId,
    corridor: RegionId,
}

/// One scan line: a fixed row (varying `x`) or column (varying `y`).
#[derive(Clone, Copy)]
enum Line {
    Row(u32),
    Col(u32),
}

impl Line {
    /// The cell at position `i` along this line.
    fn cell(self, i: u32) -> Cell {
        match self {
            Line::Row(y) => Cell::new(i, y),
            Line::Col(x) => Cell::new(x, i),
        }
    }

    /// The two flank cells perpendicular to the line at position `i` — the cells a
    /// doorway here would connect (north/south for a row, west/east for a column).
    fn flanks(self, i: u32) -> (Cell, Cell) {
        match self {
            Line::Row(y) => (Cell::new(i, y - 1), Cell::new(i, y + 1)),
            Line::Col(x) => (Cell::new(x - 1, i), Cell::new(x + 1, i)),
        }
    }
}

/// Walk one scan line, breaking it into maximal runs of wall separating a constant
/// pair of distinct regions, and push each run of length ≥ 3 as a [`Candidate`].
fn collect_runs(regions: &RegionGraph, line: Line, extent: u32, out: &mut Vec<Candidate>) {
    // The run in flight: start position along the line, length, and the two regions
    // it separates.
    let mut run: Option<(u32, u32, RegionId, RegionId)> = None;
    let flush = |run: Option<(u32, u32, RegionId, RegionId)>, out: &mut Vec<Candidate>| {
        if let Some((start, len, a, b)) = run {
            if len >= MIN_DOOR_RUN {
                // Exactly one endpoint is a room (rooms never touch across a single
                // wall), so name the room and corridor for the per-room budget.
                let (room, corridor) = if regions.kind(a) == RegionKind::Room {
                    (a, b)
                } else {
                    (b, a)
                };
                out.push(Candidate {
                    line,
                    start,
                    len,
                    room,
                    corridor,
                });
            }
        }
    };
    for i in 1..extent - 1 {
        let candidate = door_candidate(regions, line, i);
        match (run, candidate) {
            (Some((start, len, a, b)), Some((ca, cb))) if (ca, cb) == (a, b) => {
                run = Some((start, len + 1, a, b));
            }
            _ => {
                flush(run.take(), out);
                run = candidate.map(|(a, b)| (i, 1, a, b));
            }
        }
    }
    flush(run.take(), out);
}

/// Whether the cell at `i` on `line` is a doorway candidate: a wall cell with a
/// distinct interior region on each flank. Returns that region pair, or `None`.
fn door_candidate(regions: &RegionGraph, line: Line, i: u32) -> Option<(RegionId, RegionId)> {
    let (near, far) = line.flanks(i);
    // A wall separating two regions owns neither flank itself, so it is unclaimed.
    if regions.region_at(line.cell(i)).is_some() {
        return None;
    }
    let a = regions.region_at(near)?;
    let b = regions.region_at(far)?;
    if a == b {
        return None;
    }
    Some((a, b))
}

/// Pick which candidates become doors, capping each room to [`room_door_budget`]
/// of its own — but never below one, so no room is sealed (§10.6). Returns the
/// chosen indices into `candidates`, in ascending (scan) order so the cut pass is
/// deterministic. Rooms are budgeted in first-appearance order, itself fixed by the
/// scan, for the same reason.
fn choose_doors(candidates: &[Candidate], rng: &mut Rng) -> Vec<usize> {
    // Group candidate indices by the room they belong to, keeping rooms in the
    // deterministic order they first appear in the scan.
    let mut by_room: Vec<(RegionId, Vec<usize>)> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        match by_room.iter_mut().find(|(r, _)| *r == c.room) {
            Some((_, idxs)) => idxs.push(i),
            None => by_room.push((c.room, vec![i])),
        }
    }

    let mut chosen: Vec<usize> = Vec::new();
    for (_, mut idxs) in by_room {
        let budget = room_door_budget(rng) as usize;
        let keep = budget.clamp(1, idxs.len());
        // Partial Fisher–Yates: shuffle `keep` random candidates to the front.
        for i in 0..keep {
            let j = i + rng.below((idxs.len() - i) as u32) as usize;
            idxs.swap(i, j);
        }
        idxs.truncate(keep);
        chosen.extend(idxs);
    }
    chosen.sort_unstable();
    chosen
}

/// A room's doorway budget: **one or two usually, three rarely, never more**
/// (`MAX_DOORS_PER_ROOM`) **[START]**. Clamped afterwards to the candidates a room
/// actually has, and never below one.
fn room_door_budget(rng: &mut Rng) -> u32 {
    // 40% one, 50% two, 10% three — most rooms have one or two ways in.
    match rng.below(10) {
        0..=3 => 1,
        4..=8 => 2,
        _ => MAX_DOORS_PER_ROOM,
    }
}

/// Cut the doorway for one chosen `candidate`. Length is random `3..=min(len, 6)`
/// at a random offset (§10.1.4); the two ends become hinges, the cells between
/// become closed panels (§10.4).
fn cut_door(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    rng: &mut Rng,
    candidate: &Candidate,
) {
    let Candidate {
        line,
        start,
        len,
        room,
        corridor,
    } = *candidate;
    let door_len = rng.range_inclusive(MIN_DOOR_RUN as i32, len.min(MAX_DOOR_LEN) as i32) as u32;
    let offset = rng.range_inclusive(0, (len - door_len) as i32) as u32;
    let first = start + offset;
    let last = first + door_len - 1;

    let hinges = [line.cell(first), line.cell(last)];
    let panels: Vec<Cell> = (first + 1..last).map(|i| line.cell(i)).collect();

    for hinge in hinges {
        facility.set_terrain(hinge.x, hinge.y, Terrain::DoorHinge);
    }
    for &panel in &panels {
        facility.set_terrain(panel.x, panel.y, Terrain::DoorPanelClosed);
    }
    regions.add_door(room, corridor, hinges, panels, AUTO_CLOSE);
}

/// An inclusive rectangle of interior cells, `[x0, x1] × [y0, y1]`.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl Rect {
    fn new(x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    fn width(&self) -> u32 {
        self.x1 - self.x0 + 1
    }

    fn height(&self) -> u32 {
        self.y1 - self.y0 + 1
    }

    fn area(&self) -> u32 {
        self.width() * self.height()
    }

    fn cells(&self) -> impl Iterator<Item = Cell> {
        let (x0, x1, y0, y1) = (self.x0, self.x1, self.y0, self.y1);
        (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| Cell::new(x, y)))
    }
}

/// Which of a region's four sides face the corridor network. A carve toward an open
/// side reaches an existing corridor; a fresh leftover always gains the side facing
/// the corridor that split it, so it is never fully closed.
#[derive(Clone, Copy, Debug, Default)]
struct Open {
    n: bool,
    s: bool,
    e: bool,
    w: bool,
}

/// A region waiting to be split or settled into a room.
#[derive(Clone, Copy, Debug)]
struct Pending {
    rect: Rect,
    open: Open,
}

impl Pending {
    /// A region open on no side — the root interior, walled in on all four sides.
    fn new(rect: Rect) -> Self {
        Self {
            rect,
            open: Open::default(),
        }
    }
}

/// The orientation of a carved corridor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Axis {
    /// Runs north–south, splitting the region into east and west leftovers.
    Vertical,
    /// Runs east–west, splitting the region into north and south leftovers.
    Horizontal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::RegionKind;
    use std::collections::HashSet;

    /// The bounding box `(width, height)` of a set of cells.
    fn bbox(cells: &[Cell]) -> (u32, u32) {
        let x0 = cells.iter().map(|c| c.x).min().unwrap();
        let x1 = cells.iter().map(|c| c.x).max().unwrap();
        let y0 = cells.iter().map(|c| c.y).min().unwrap();
        let y1 = cells.iter().map(|c| c.y).max().unwrap();
        (x1 - x0 + 1, y1 - y0 + 1)
    }

    fn regions_of_kind(layout: &Layout, kind: RegionKind) -> usize {
        layout
            .regions()
            .regions()
            .filter(|(_, r)| r.kind() == kind)
            .count()
    }

    #[test]
    fn partitions_the_v1_config() {
        let mut rng = Rng::new(7);
        let layout = generate(40, 40, &mut rng).expect("40x40 partitions");
        assert!(
            regions_of_kind(&layout, RegionKind::Corridor) >= 1,
            "expected at least one corridor"
        );
        assert!(
            regions_of_kind(&layout, RegionKind::Room) >= 2,
            "objectives and guards need at least two rooms"
        );
    }

    #[test]
    fn room_count_stays_within_the_budget() {
        for seed in 0..64 {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let rooms = regions_of_kind(&layout, RegionKind::Room);
            assert!(
                rooms <= MAX_ROOMS,
                "seed {seed}: {rooms} rooms exceeds budget"
            );
        }
    }

    /// **[SETTLED]** — corridor width is always 2–4, never single-file. A corridor's
    /// narrow bounding-box dimension is its width (throats only extend its length).
    #[test]
    fn corridor_width_is_always_2_to_4() {
        for seed in 0..200 {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            for (_, region) in layout.regions().regions() {
                if region.kind() == RegionKind::Corridor {
                    let (w, h) = bbox(region.cells());
                    let narrow = w.min(h);
                    assert!(
                        (CORRIDOR_MIN_WIDTH..=CORRIDOR_MAX_WIDTH).contains(&narrow),
                        "seed {seed}: corridor narrow dim {narrow} outside 2..=4"
                    );
                }
            }
        }
    }

    /// Every room is a rectangle, always ≥ 6×6 (§10.1).
    #[test]
    fn rooms_are_at_least_6x6() {
        for seed in 0..200 {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            for (_, region) in layout.regions().regions() {
                if region.kind() == RegionKind::Room {
                    let (w, h) = bbox(region.cells());
                    assert!(w >= 6 && h >= 6, "seed {seed}: room {w}x{h} below 6x6");
                }
            }
        }
    }

    /// The §10.6 guarantee, and the reason the graph exists (§10.5): every interior
    /// floor cell belongs to exactly one region, every wall to none. Nothing is
    /// "painted and forgotten".
    #[test]
    fn every_floor_cell_belongs_to_exactly_one_region() {
        let layout = generate(40, 40, &mut Rng::new(11)).unwrap();
        let facility = layout.facility();
        for y in 0..facility.height() {
            for x in 0..facility.width() {
                let is_floor = facility.terrain_at(x, y) == Some(Terrain::Floor);
                let has_region = layout.regions().region_at(Cell::new(x, y)).is_some();
                assert_eq!(
                    is_floor, has_region,
                    "({x},{y}): floor={is_floor} but region={has_region}"
                );
            }
        }
    }

    /// The border is enclosed unconditionally (§10.6): every border cell is wall.
    #[test]
    fn the_border_stays_sealed() {
        for seed in 0..200 {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let f = layout.facility();
            for x in 0..f.width() {
                assert_eq!(f.terrain_at(x, 0), Some(Terrain::Wall));
                assert_eq!(f.terrain_at(x, f.height() - 1), Some(Terrain::Wall));
            }
            for y in 0..f.height() {
                assert_eq!(f.terrain_at(0, y), Some(Terrain::Wall));
                assert_eq!(f.terrain_at(f.width() - 1, y), Some(Terrain::Wall));
            }
        }
    }

    /// The headline §10.6 property: the corridor network is connected. Every
    /// corridor punches into its parent, so the union of all corridor cells is a
    /// single 4-connected component. Asserted over many seeds.
    #[test]
    fn the_corridor_network_is_always_connected() {
        for seed in 0..200 {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            assert_corridors_connected(&layout, seed);
        }
    }

    /// Connectivity holds across a range of footprints, not just the v1 square.
    #[test]
    fn connectivity_holds_across_sizes() {
        for &(w, h) in &[(18, 18), (24, 40), (40, 24), (33, 51), (60, 60)] {
            for seed in 0..40 {
                let layout = generate(w, h, &mut Rng::new(seed)).unwrap();
                assert_corridors_connected(&layout, seed);
            }
        }
    }

    fn assert_corridors_connected(layout: &Layout, seed: u64) {
        let corridor: HashSet<Cell> = layout
            .regions()
            .regions()
            .filter(|(_, r)| r.kind() == RegionKind::Corridor)
            .flat_map(|(_, r)| r.cells().iter().copied())
            .collect();
        assert!(!corridor.is_empty(), "seed {seed}: no corridors");

        let start = *corridor.iter().next().unwrap();
        let mut seen = HashSet::new();
        let mut stack = vec![start];
        while let Some(c) = stack.pop() {
            if !seen.insert(c) {
                continue;
            }
            for (dx, dy) in [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)] {
                let (nx, ny) = (c.x as i32 + dx, c.y as i32 + dy);
                if nx >= 0 && ny >= 0 {
                    let n = Cell::new(nx as u32, ny as u32);
                    if corridor.contains(&n) && !seen.contains(&n) {
                        stack.push(n);
                    }
                }
            }
        }
        assert_eq!(
            seen.len(),
            corridor.len(),
            "seed {seed}: corridor network split into disconnected pieces"
        );
    }

    /// §12.4: all randomness comes from the run `Rng`, so a seed reproduces a
    /// facility exactly — same grid, same regions.
    #[test]
    fn generation_is_deterministic() {
        let a = generate(40, 40, &mut Rng::new(2026)).unwrap();
        let b = generate(40, 40, &mut Rng::new(2026)).unwrap();
        assert_eq!(
            crate::ascii_grid(a.facility()),
            crate::ascii_grid(b.facility())
        );
        assert_eq!(a.regions().region_count(), b.regions().region_count());
    }

    #[test]
    fn different_seeds_give_different_facilities() {
        let a = generate(40, 40, &mut Rng::new(1)).unwrap();
        let b = generate(40, 40, &mut Rng::new(2)).unwrap();
        assert_ne!(
            crate::ascii_grid(a.facility()),
            crate::ascii_grid(b.facility())
        );
    }

    /// A footprint too small to partition is rejected, not silently shipped as an
    /// unplaceable single-room level (§10.2).
    #[test]
    fn footprints_too_small_are_rejected() {
        // Interior 8×8: no axis reaches 16.
        assert_eq!(
            generate(10, 10, &mut Rng::new(0)).unwrap_err(),
            GenError::TooSmall {
                width: 10,
                height: 10
            }
        );
        // Interior 36×5: one axis fits, but a room could never be 6 deep.
        assert_eq!(
            generate(38, 7, &mut Rng::new(0)).unwrap_err(),
            GenError::TooSmall {
                width: 38,
                height: 7
            }
        );
    }

    /// The smallest footprint that *does* partition: interior 16×16, exactly one
    /// corridor, two rooms.
    #[test]
    fn the_minimum_footprint_partitions() {
        let layout = generate(18, 18, &mut Rng::new(5)).expect("18x18 partitions");
        assert!(regions_of_kind(&layout, RegionKind::Corridor) >= 1);
        assert!(regions_of_kind(&layout, RegionKind::Room) >= 2);
    }

    /// §10.6: every room reaches a corridor. The doorway pass (§10.1.4) must cut at
    /// least one door from every room into the corridor network — a room with no
    /// door is sealed, taking its future objectives and guards with it.
    #[test]
    fn every_room_reaches_a_corridor_through_a_door() {
        for &(w, h) in &[(18, 18), (40, 40), (24, 40), (60, 60)] {
            for seed in 0..40 {
                let layout = generate(w, h, &mut Rng::new(seed)).unwrap();
                let regions = layout.regions();
                for (id, region) in regions.regions() {
                    if region.kind() != RegionKind::Room {
                        continue;
                    }
                    let reaches_corridor = regions
                        .neighbours(id)
                        .any(|(_, other)| regions.kind(other) == RegionKind::Corridor);
                    assert!(
                        reaches_corridor,
                        "{w}x{h} seed {seed}: a room has no door to a corridor"
                    );
                }
            }
        }
    }

    /// A room gets one to three doors, never more (`MAX_DOORS_PER_ROOM`) — a room
    /// boxed in by corridors is not riddled with a door on every wall — and never
    /// fewer than one, so no room is sealed off (§10.6 **[START]**).
    #[test]
    fn rooms_have_one_to_three_doors() {
        for &(w, h) in &[(18, 18), (40, 40), (24, 40), (60, 60)] {
            for seed in 0..64 {
                let layout = generate(w, h, &mut Rng::new(seed)).unwrap();
                let regions = layout.regions();
                for (id, region) in regions.regions() {
                    if region.kind() != RegionKind::Room {
                        continue;
                    }
                    let doors = region.doors().len() as u32;
                    assert!(
                        (1..=MAX_DOORS_PER_ROOM).contains(&doors),
                        "{w}x{h} seed {seed}: room {id:?} has {doors} doors, want 1..={MAX_DOORS_PER_ROOM}"
                    );
                }
            }
        }
    }

    /// Most rooms are calm — one or two doors — with three-door hubs the exception,
    /// per the [`room_door_budget`] weighting **[START]**. Asserted in aggregate over
    /// many seeds so the distribution, not any single room, is what's pinned.
    #[test]
    fn most_rooms_have_one_or_two_doors() {
        let (mut calm, mut total) = (0u32, 0u32);
        for seed in 0..200 {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let regions = layout.regions();
            for (_, region) in regions.regions() {
                if region.kind() != RegionKind::Room {
                    continue;
                }
                total += 1;
                if region.doors().len() <= 2 {
                    calm += 1;
                }
            }
        }
        // Three-door rooms are rare; the overwhelming majority have one or two.
        assert!(
            calm * 100 >= total * 90,
            "only {calm}/{total} rooms have <= 2 doors; expected the vast majority"
        );
    }

    /// Every doorway is a valid §10.4 span: 2 hinges around 1–4 panels, 3–6 cells
    /// total, all lying on a single straight wall line.
    #[test]
    fn doorways_are_well_formed_spans() {
        for seed in 0..64 {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            for (_, door) in layout.regions().doors() {
                assert_eq!(door.hinges().len(), 2, "seed {seed}: a hinge at each end");
                let panels = door.panels().len();
                assert!(
                    (1..=4).contains(&panels),
                    "seed {seed}: {panels} panels, want 1..=4"
                );
                let cells: Vec<Cell> = door.cells().collect();
                let straight = cells.iter().all(|c| c.x == cells[0].x)
                    || cells.iter().all(|c| c.y == cells[0].y);
                assert!(
                    straight,
                    "seed {seed}: a door must lie on one straight line"
                );
            }
        }
    }
}
