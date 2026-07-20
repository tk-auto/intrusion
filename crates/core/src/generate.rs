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
//! This module carries §10.1 steps 1–6: the partition (steps 1–3), doorways
//! (step 4), room features (step 5) and the hideout board (step 6), plus the
//! §10.1a sightline-cover pass between doorways and hideouts. Entity
//! placement — entry/exit, objectives, guards (steps 7–9) — lives in
//! [`crate::place`] and reads the [`RegionGraph`] this produces;
//! [`generate_level`] runs both under one seed-retry loop.
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
//! result: a connected network, and an enclosure that stays intact, by
//! construction. This refines §10.1's raw "50/50 axis" so the §10.6 tree
//! guarantee holds by construction; the property tests below assert it over many
//! seeds.
//!
//! Construction is not trusted, though — that was the old generator's mistake
//! (§10.6: a room whose bounding wall runs all came out < 3 sealed shut,
//! objectives inside, and nothing noticed). So [`generate`] gates every carve
//! through the §10.6 assertions — border enclosed, every pathable cell reaching
//! every other — and rejects the carve and redraws if one fails, up to a hard cap.
//! Downstream code only ever sees a layout that passed.

use crate::cell::{Cell, Direction};
use crate::facility::{Facility, Terrain};
use crate::path;
use crate::place::{place, LevelConfig, Placement};
use crate::region::{RegionGraph, RegionId, RegionKind};
use crate::rng::Rng;
use std::collections::HashSet;

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

/// How many feature attempts each room gets (§10.1.5). Each attempt proposes a
/// partition wall and a pillar; the viable proposals pool and one is placed.
const FEATURE_ATTEMPTS: u32 = 4;
/// The shortest room that can host a partition wall (§10.1.5) — it needs an
/// interior stub with a floor lane surviving on both flanks.
const MIN_ROOM_FOR_PARTITION: u32 = 3;
/// The shortest room that can host a pillar (§10.1.5). A pillar is freestanding, so
/// it needs a 2-cell block plus a 1-cell margin on every side — which a 6-wide room
/// affords exactly. This is also the geometry hideouts need, so pillars run before
/// them (§10.1.6).
const MIN_ROOM_FOR_PILLAR: u32 = 6;
/// A partition stub is at least this long (§10.1.5); its max is `axis − 1`, so a
/// stub never spans the room and an alcove always survives past its tip.
const PARTITION_MIN_LEN: u32 = 2;
/// A pillar's side length range (§10.1.5): a freestanding 2–4 by 2–4 block.
const PILLAR_MIN_SIDE: u32 = 2;
const PILLAR_MAX_SIDE: u32 = 4;

/// Hideouts sit at least this far apart (Manhattan) **[START]**, so the board is
/// spread *along* a flight path rather than clumped into a bank of cupboards
/// (§10.1a). Big enough that the facility still reads as a building rather than a
/// honeycomb; small enough that a fleeing player is rarely more than a few steps from
/// cover. Density is the open tuning knob here (§10.1a, §15.2) — a single named value.
const HIDEOUT_MIN_SPACING: u32 = 7;

/// No unbroken straight sightline may exceed this many cells — §10.1a, the
/// generator's most important job after connectivity. The *rule* is **[SETTLED]**;
/// the value is **[START]**: the design band is 10–12, "roughly a guard's sight
/// range" (§7.1's `GUARD_SIGHT_RANGE` is 10), and 11 splits it. This is the single
/// named knob for the §15.2 how-much-cover experiments — longer than this and
/// there is no geometry between the player and being seen.
pub const SIGHTLINE_MAX_RUN: u32 = 11;

/// How many carve attempts [`generate`] makes before giving up on the footprint
/// (§10.6: reject the seed, retry, but never loop forever). Rejection is rare —
/// the partition is connected by construction and the property tests below have
/// never caught a violation — so hitting this cap means the *config* is bad, not
/// the luck, and the caller gets [`GenError::RetriesExhausted`] instead of a hang.
const MAX_GEN_ATTEMPTS: u32 = 64;

/// Why a facility could not be generated.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GenError {
    /// The facility is too small to partition: no corridor fits, so the interior
    /// would be a single room — which cannot host the entry, objectives and guards
    /// that must live in *different* rooms (§10.2). Below 18×18 in both axes this
    /// is unavoidable. Guard it; do not ship an unplaceable level.
    TooSmall { width: u32, height: u32 },
    /// Every one of [`MAX_GEN_ATTEMPTS`] carves failed the §10.6 guarantees. The
    /// loud failure §10.6 demands: a parameter set that cannot produce a valid
    /// level errors out immediately rather than silently shipping a broken one or
    /// spinning forever.
    RetriesExhausted { attempts: u32 },
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

/// Generate a facility that passes every §10.6 guarantee, or fail loudly.
///
/// This is the single generation entry point: a layout it returns has been
/// *asserted* enclosed and reachable, not merely argued so — the §10.6 lesson. A
/// carve that fails [`passes_guarantees`] is rejected and redrawn from the same
/// `rng` stream (so a run is still one seed, §12.4: same seed, same facility,
/// forever), up to [`MAX_GEN_ATTEMPTS`] before [`GenError::RetriesExhausted`].
/// Returns [`GenError::TooSmall`] immediately for a footprint that cannot be
/// partitioned at all — no amount of redrawing fixes geometry.
pub fn generate(width: u32, height: u32, rng: &mut Rng) -> Result<Layout, GenError> {
    generate_where(width, height, rng, passes_guarantees)
}

/// Generate a *placed* level: a carve passing every §10.6 guarantee **and** a
/// [`Placement`] honouring §10.1 steps 7–9 with the spacing guarantees — exact
/// piece counts, a safe starting area, spread intel, and post-placement
/// solvability (start → every objective → exit).
///
/// This is the entry point real levels come from. Carve rejection (#13) and
/// placement rejection (#12) share this one seed-retry loop, as §10.6 asks: a
/// carve whose geometry cannot seat the pieces is redrawn exactly like a carve
/// that sealed a room, from the same `rng` stream (§12.4 — same seed, same level,
/// forever), and a config that can never place errors out loudly with
/// [`GenError::RetriesExhausted`] instead of shipping a silent shortfall.
pub fn generate_level(
    config: &LevelConfig,
    rng: &mut Rng,
) -> Result<(Layout, Placement), GenError> {
    for _ in 0..MAX_GEN_ATTEMPTS {
        let layout = generate_once(config.width, config.height, rng)?;
        if !passes_guarantees(&layout) {
            continue;
        }
        if let Some(placement) = place(&layout, config, rng) {
            return Ok((layout, placement));
        }
    }
    Err(GenError::RetriesExhausted {
        attempts: MAX_GEN_ATTEMPTS,
    })
}

/// The reject-and-redraw loop behind [`generate`], with the guarantee check as a
/// parameter so tests can exercise the loop itself (a real carve essentially never
/// fails validation, which is the point — but the cap must still be provably a cap).
fn generate_where(
    width: u32,
    height: u32,
    rng: &mut Rng,
    valid: impl Fn(&Layout) -> bool,
) -> Result<Layout, GenError> {
    for _ in 0..MAX_GEN_ATTEMPTS {
        let layout = generate_once(width, height, rng)?;
        if valid(&layout) {
            return Ok(layout);
        }
    }
    Err(GenError::RetriesExhausted {
        attempts: MAX_GEN_ATTEMPTS,
    })
}

/// One carve of the corridor-first binary partition (§10.1 steps 1–6), unvalidated.
///
/// All randomness is drawn from `rng` (§12.4). Only [`generate_where`] calls this;
/// everything downstream receives layouts that have passed the §10.6 gate.
fn generate_once(width: u32, height: u32, rng: &mut Rng) -> Result<Layout, GenError> {
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

    // Step 5: break up room interiors with a partition wall or a pillar (§10.1.5),
    // *before* each room becomes a region, so the graph records the room's true
    // (non-rectangular) footprint rather than its bounding box (§10.5). Pillars are
    // the 2-cell-thick geometry hideouts need, so this must precede them (§10.1.6).
    for room in &rooms {
        let feature = carve_room_features(&mut facility, room, rng);
        regions.add_region(
            RegionKind::Room,
            room.cells().filter(|c| !feature.contains(c)),
        );
    }

    // §10.6: the border is enclosed unconditionally. Punch-throughs only fire
    // toward open sides (interior walls), never the border, so this re-stamp is a
    // guarantee, not a repair — but it makes the enclosure true by assertion, not
    // by argument (the §10.6 lesson).
    seal_border(&mut facility);

    // Step 4: cut doorways where a room meets a corridor, now that every region is
    // named (§10.1.4). Runs on the finished grid so it sees the true walls.
    place_doorways(&mut facility, &mut regions, rng);

    // §10.1a: break every straight sightline longer than SIGHTLINE_MAX_RUN with
    // stamped partial cover (tables) — after doorways so the tables keep clear
    // of door throats, and before hideouts so the cupboard pass sees the final
    // floor. (A table does not *back* a cupboard — only walls and pillars do; a
    // cupboard against see-through furniture would read as standing in the open.)
    break_sightlines(&mut facility, &mut regions, rng);

    // Step 6: the hiding-game board — concealment cupboards set against walls and
    // pillar faces, spread along the flight paths (§10.1.6, §10.1a). After doorways
    // so a cupboard can steer clear of a door throat.
    place_hideouts(&mut facility, &regions, rng);

    debug_assert!(corridors > 0, "guarded footprint yielded no corridor");
    Ok(Layout { facility, regions })
}

/// The §10.6 gate: every guarantee that must be *asserted* on a finished carve,
/// not believed from the construction. Three checks:
///
/// - **Fully enclosed** — the border ring is unbroken wall. The punch-through
///   design never fires at the border, but "never" is exactly the kind of claim
///   §10.6 says to verify.
/// - **One pathable component** — every cell that admits pathing (§10.3: floor,
///   door panels open *or* closed, consoles, exits; not walls, hinges or hideouts)
///   reaches every other. This is the reachability guarantee in its strongest
///   form: the old generator could seal a room (with its objectives and guards)
///   behind sub-3-cell wall runs that earned no door, and nothing noticed. With
///   the whole walkable interior one component, *any* placement of start,
///   objectives and exit (§10.1 steps 7–9, #12) is start → every objective → exit
///   solvable — the property placement will rely on rather than re-prove.
/// - **Bounded sightlines** — no straight run longer than [`SIGHTLINE_MAX_RUN`]
///   without counterplay in it: an obstruction or a partial-cover table (§10.1a —
///   a table does not block a guard's sight, but it plants the §10.3 crouch,
///   which is the geometry-between-you-and-being-seen the rule demands).
///   [`break_sightlines`] repairs the carve, but the rule is *measured* here on
///   the finished grid — a run the repair could not break rejects the carve,
///   exactly like a reachability failure.
///
/// Room size and count are not re-checked here: they are fixed by the partition
/// constants before any wall is stamped, and the property tests below pin them.
///
/// **One usable per cell is a preference, not a guarantee** (§11.4): the
/// stamping passes ([`place_blocker`], [`place_hideouts`]) and placement
/// (`crate::place`) *avoid* crowding a floor cell with a second adjacent usable
/// wherever a free alternative exists, but two of the guarantees here —
/// connectivity and the sightline rule — outrank it, and structural doors can
/// cluster no carve can undo. So it is not asserted: a rare doubled cell stays
/// legible because the usable line points each bump with its own arrow (§11.4).
fn passes_guarantees(layout: &Layout) -> bool {
    fully_enclosed(layout.facility())
        && pathable_connected(layout.facility())
        && sightlines_bounded(layout.facility())
}

/// Whether `terrain` is a **usable** a player bumps (§11.4): a door cell (a
/// hinge or either panel pose — whatever its pose, a door is one usable), a
/// table, or a cupboard. Consoles and the exit are still plain floor during
/// generation (§10.1.7–8), so callers pass those cells in via `extra`.
///
/// This is deliberately *not* the runtime bump ladder [`State::bump_kind`](crate::State):
/// generation has only terrain — no objectives, no live door state, no player — so it
/// cannot ask "what would a bump do here". It asks the coarser, pose-independent
/// question "could this cell be a usable at all", which is all the §11.4 one-usable
/// *placement preference* needs. The two must agree on the terrain set (door cell,
/// table, cupboard); they answer different questions, so they stay separate lists.
fn is_usable_terrain(terrain: Terrain) -> bool {
    matches!(
        terrain,
        Terrain::DoorHinge
            | Terrain::DoorPanelClosed
            | Terrain::DoorPanelOpen
            | Terrain::PartialCover
            | Terrain::Hideout
    )
}

/// Whether `cell` already has at least one usable orthogonally adjacent — a
/// door cell, a table, a cupboard, or one of `extra` (placement's consoles and
/// exit). Terrain-only and four lookups: the §11.4 one-usable checks only ever
/// ask this yes/no question, never a deduped count, so this never touches the
/// door list (which turned the check quadratic).
pub(crate) fn has_adjacent_usable(facility: &Facility, cell: Cell, extra: &[Cell]) -> bool {
    facility
        .neighbors(cell)
        .any(|n| extra.contains(&n) || facility.terrain(n).is_some_and(is_usable_terrain))
}

/// Whether stamping a usable at `cell` would give some floor neighbour a
/// **second** adjacent usable — the §11.4 one-usable *preference*. The stamping
/// passes consult this to prefer a cleaner site; unlike a guarantee it may be
/// overridden (a sightline that only one crowded cell can break, a structural
/// door cluster), so nothing asserts its absence — the arrow disambiguates a
/// doubled cell instead.
fn creates_usable_conflict(facility: &Facility, cell: Cell) -> bool {
    facility.neighbors(cell).any(|f| {
        facility.terrain(f) == Some(Terrain::Floor) && has_adjacent_usable(facility, f, &[])
    })
}

/// Whether the border ring is solid wall — §10.6 "fully enclosed".
fn fully_enclosed(facility: &Facility) -> bool {
    let (w, h) = (facility.width(), facility.height());
    let mut border = (0..w)
        .flat_map(|x| [Cell::new(x, 0), Cell::new(x, h - 1)])
        .chain((0..h).flat_map(|y| [Cell::new(0, y), Cell::new(w - 1, y)]));
    border.all(|c| facility.terrain(c) == Some(Terrain::Wall))
}

/// Whether the pathable cells form a single 4-connected component — the §10.6
/// reachability flood fill. "It is a flood fill. It costs nothing." Runs on every
/// carve inside the retry loop, so it leans on [`path::flood_from`]'s bit-grid
/// sweep rather than a set.
fn pathable_connected(facility: &Facility) -> bool {
    let (w, h) = (facility.width(), facility.height());
    let pathable = |c: Cell| facility.terrain(c).is_some_and(|t| !t.blocks_pathing());
    let all: Vec<Cell> = (0..h)
        .flat_map(|y| (0..w).map(move |x| Cell::new(x, y)))
        .filter(|&c| pathable(c))
        .collect();
    let Some(&start) = all.first() else {
        return false; // a level with nowhere to stand is not a level
    };
    // One component iff the flood from any pathable cell reaches them all.
    path::flood_from(start, w, h, pathable).len() == all.len()
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

/// Carve one room feature — a partition wall or a pillar — into `room` (§10.1.5).
///
/// Runs up to [`FEATURE_ATTEMPTS`] attempts; each proposes a partition wall and a
/// pillar, and every viable proposal joins a pool. One is then chosen and stamped
/// as wall, and its cells are returned so the caller can withhold them from the
/// room's region — that withholding is what gives the region its true, non-rectangular
/// shape (§10.5). A room with an empty pool (nothing fit) is left as a plain
/// rectangle. Every proposal is validated against `facility`'s current floor, so a
/// feature can never seal a room off or run into a wall.
fn carve_room_features(facility: &mut Facility, room: &Rect, rng: &mut Rng) -> HashSet<Cell> {
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
fn propose_partition(room: &Rect, facility: &Facility, rng: &mut Rng) -> Option<Vec<Cell>> {
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
fn propose_pillar(room: &Rect, facility: &Facility, rng: &mut Rng) -> Option<Vec<Cell>> {
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
fn is_clear(room: &Rect, facility: &Facility, cell: Cell) -> bool {
    room.contains(cell) && facility.terrain(cell) == Some(Terrain::Floor)
}

/// `cell` shifted by `(dx, dy)`. The room interior sits well inside the border, so
/// feature offsets never underflow the grid.
fn offset(cell: Cell, (dx, dy): (i32, i32)) -> Cell {
    Cell::new((cell.x as i32 + dx) as u32, (cell.y as i32 + dy) as u32)
}

/// Carve the hiding-game board: concealment **cupboards** set against walls and
/// pillar faces, spread along the flight paths (§10.1.6, §10.1a).
///
/// A hideout is a floor cell backed by structure — never floating in open ground —
/// so it reads as a cupboard the player ducks into. Empty it is walk-through yet
/// blocks pathing, so a guard patrol routes *around* it while the player slips *in*;
/// occupied it is solid and conceals its occupant (the vision ticket reads that
/// concealment, §11.5a). The old generator harvested rare three-walled wall-pockets,
/// one attempt per room, stopping at the first failure — so during a chase there was
/// nowhere to hide and the hiding game had no board (§10.1a). This places them
/// deliberately: **corridors and junctions first** (where the player flees), rooms
/// after, spaced out, and it never stops at the first failure. Placement keeps guard
/// pathing connected — a candidate that would wall a patrol route off is skipped
/// ([`severs_pathing`]).
fn place_hideouts(facility: &mut Facility, regions: &RegionGraph, rng: &mut Rng) {
    // Candidate cupboard sites, split so the flight paths are served first: a
    // structure-backed floor cell in a corridor outranks one in a room.
    let mut corridor: Vec<Cell> = Vec::new();
    let mut room: Vec<Cell> = Vec::new();
    for (id, region) in regions.regions() {
        let bucket = match regions.kind(id) {
            RegionKind::Corridor => &mut corridor,
            RegionKind::Room => &mut room,
        };
        for &cell in region.cells() {
            if is_cupboard_site(facility, cell) {
                bucket.push(cell);
            }
        }
    }
    // Shuffle within each bucket so the board varies by seed, then take corridors
    // before rooms. Both are deterministic from `rng` (§12.4).
    shuffle(&mut corridor, rng);
    shuffle(&mut room, rng);

    let mut placed: Vec<Cell> = Vec::new();
    for cell in corridor.into_iter().chain(room) {
        // Spacing: keep cupboards spread along a path, and never block both lanes of
        // a 2-wide corridor at one cross-section.
        if placed
            .iter()
            .any(|&p| p.manhattan_distance(cell) < HIDEOUT_MIN_SPACING)
        {
            continue;
        }
        // A cupboard blocks pathing (§10.3); never let one sever a patrol route.
        if severs_pathing(facility, cell) {
            continue;
        }
        // Prefer not to crowd a floor cell's usable line (§11.4): a cupboard
        // beside a cell that already borders a door, a table or another cupboard
        // is skipped. Cupboards are best-effort furniture with plentiful sites
        // (§10.1.6), so skipping here only improves the spread — nothing depends
        // on this particular cell taking one.
        if creates_usable_conflict(facility, cell) {
            continue;
        }
        facility.set_terrain(cell.x, cell.y, Terrain::Hideout);
        placed.push(cell);
    }
}

/// Whether `cell` can hold a cupboard: a floor cell backed by a wall or pillar face
/// on at least one side — so it never floats in open ground — and clear of any door
/// cell, so a cupboard never clogs a doorway (§10.1a, §10.4). Pillars are wall
/// terrain, so a floor cell against a pillar qualifies, which is what gives a pillared
/// room its hiding spots.
fn is_cupboard_site(facility: &Facility, cell: Cell) -> bool {
    if facility.terrain(cell) != Some(Terrain::Floor) {
        return false;
    }
    let mut backed = false;
    for n in facility.neighbors(cell) {
        match facility.terrain(n) {
            Some(Terrain::Wall) => backed = true,
            // A door throat on any flank disqualifies the cell — keep doorways clear.
            Some(Terrain::DoorHinge)
            | Some(Terrain::DoorPanelClosed)
            | Some(Terrain::DoorPanelOpen) => return false,
            _ => {}
        }
    }
    backed
}

/// Whether turning `cell` into a hideout would sever guard pathing (§10.3) — the
/// cupboard must never wall a patrol route off, nor strand an objective (#13).
///
/// Cutting a single cell can only disconnect the graph *at that cell*: the pieces it
/// would split each hold one of `cell`'s pathable neighbours. So the question is
/// purely local — can those neighbours still reach each other **without** stepping on
/// `cell`? If a detour exists through the 3×3 ring around `cell`, removing it cannot
/// disconnect anything (the ring cells are real and stay pathable). This is *sound*:
/// a local detour is a global one. It is also conservative — a candidate whose
/// neighbours only reconnect via a long way round is skipped rather than risked — which
/// costs at most a few cupboard sites and keeps the check O(1) instead of a per-candidate
/// flood fill over the whole level.
fn severs_pathing(facility: &Facility, cell: Cell) -> bool {
    let pathable = |c: Cell| facility.terrain(c).is_some_and(|t| !t.blocks_pathing());
    // The pathable orthogonal neighbours — the cells that must stay mutually reachable.
    let targets: Vec<Cell> = facility.neighbors(cell).filter(|&n| pathable(n)).collect();
    if targets.len() <= 1 {
        return false; // nothing to keep connected
    }
    // Flood the pathable ring (Chebyshev ≤ 1 of `cell`, excluding `cell` itself) from
    // one target; if it reaches every other target, a detour exists and `cell` is safe.
    // This is deliberately the O(ring) local flood, *not* `path::flood_from` over the
    // whole level — folding it into the full-grid primitive would defeat the O(1) point.
    let in_ring = |c: Cell| c != cell && chebyshev(cell, c) <= 1;
    let mut seen = vec![targets[0]];
    let mut stack = vec![targets[0]];
    while let Some(c) = stack.pop() {
        for n in facility.neighbors(c) {
            if in_ring(n) && pathable(n) && !seen.contains(&n) {
                seen.push(n);
                stack.push(n);
            }
        }
    }
    !targets.iter().all(|t| seen.contains(t))
}

/// Chebyshev (chessboard) distance between two cells — the number of king moves.
fn chebyshev(a: Cell, b: Cell) -> u32 {
    let dx = a.x.abs_diff(b.x);
    let dy = a.y.abs_diff(b.y);
    dx.max(dy)
}

/// A deterministic in-place Fisher–Yates shuffle driven by the run `Rng` (§12.4).
/// Shared with placement (`crate::place`), which shuffles rooms and candidate
/// cells from the same stream.
pub(crate) fn shuffle<T>(items: &mut [T], rng: &mut Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.below((i + 1) as u32) as usize;
        items.swap(i, j);
    }
}

/// Break every over-long straight sightline with stamped cover (§10.1a).
///
/// Corridor-first partition has a severe emergent flaw that only shows up in play
/// (§7.6): it produces long, dead-straight, full-span corridors — and the
/// corridors are where the player flees. The rooms get features; the corridors got
/// nothing. So this pass scans the whole grid for straight runs longer than
/// [`SIGHTLINE_MAX_RUN`] with no counterplay in them and stamps a 1-cell **table**
/// ([`Terrain::PartialCover`]) near the middle of each. The table is furniture,
/// not a wall stub: it blocks movement and pathing but a guard sees straight over
/// it — the counterplay it plants is the *crouch* (§10.3), not a shadow — so the
/// facility keeps reading as a building instead of sprouting orphan wall cells.
/// Rooms are treated exactly like corridors: the §10.1a rule is per-cell over the
/// level, and a long room gallery is as much a sightline as a corridor. (Jogging
/// the corridors mid-carve is the other §10.1a technique; if the §15.2
/// experiments want it, it slots in beside this pass and the assertion below
/// judges both the same way.)
///
/// A candidate that would sever guard pathing (§10.3) or split its own region
/// into pieces (§10.5) is skipped — which in a 2-wide corridor forces the second
/// lane's blocker to land offset from the first, an S-squeeze: the §10.1a "jog"
/// emerging from the pathing constraint rather than a separate mechanism. A
/// blocker *may* land beside a multi-panel door span — that is §10.1a's "cover
/// near doors", something to duck behind on the far side — while a single-panel
/// door can never be sealed, because walling its only approach leaves the panel
/// no local detour and [`severs_pathing`] refuses.
///
/// The pass is a *repair*, not a proof: [`passes_guarantees`] re-measures the
/// finished grid, so a run this pass could not break (every candidate disqualified)
/// rejects the carve and redraws, exactly like a reachability failure (§10.6).
fn break_sightlines(facility: &mut Facility, regions: &mut RegionGraph, rng: &mut Rng) {
    // Take the first still-over-long run in scan order and split it. Every placed
    // table turns floor into cover, so the loop strictly shrinks the floor and
    // always terminates.
    loop {
        let Some(run) = sight_runs(facility)
            .into_iter()
            .find(|r| r.len > SIGHTLINE_MAX_RUN)
        else {
            return;
        };
        if !place_blocker(facility, regions, &run, rng) {
            return; // unbreakable — the §10.6 gate rejects this carve
        }
    }
}

/// A maximal straight run of counterplay-free cells along one scan line — the
/// unit the §10.1a rule is measured in.
#[derive(Clone, Copy)]
struct SightRun {
    line: Line,
    start: u32,
    len: u32,
}

/// Every maximal counterplay-free run in the grid, rows then columns, in scan
/// order. A run is bounded by sight-blocking terrain (§10.3) — wall, hinge,
/// closed panel — **or by partial cover**: a table does not stop a guard's sight,
/// but it plants the crouch (§10.3) in the middle of the straight, which is the
/// §10.1a rule's real demand — geometry between the player and being seen, not
/// darkness.
fn sight_runs(facility: &Facility) -> Vec<SightRun> {
    let (w, h) = (facility.width(), facility.height());
    let mut runs = Vec::new();
    for y in 0..h {
        collect_sight_runs(facility, Line::Row(y), w, &mut runs);
    }
    for x in 0..w {
        collect_sight_runs(facility, Line::Col(x), h, &mut runs);
    }
    runs
}

/// Walk one scan line and push each maximal run of cells that neither block
/// sight nor provide cover.
fn collect_sight_runs(facility: &Facility, line: Line, extent: u32, out: &mut Vec<SightRun>) {
    let mut start: Option<u32> = None;
    for i in 0..extent {
        let clear = facility
            .terrain(line.cell(i))
            .is_some_and(|t| !t.blocks_sight() && !t.provides_cover());
        match (start, clear) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                out.push(SightRun {
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
        out.push(SightRun {
            line,
            start: s,
            len: extent - s,
        });
    }
}

/// The §10.1a sightline rule as a measured property of a finished grid: no
/// straight run without counterplay — an obstruction *or* a partial-cover cell —
/// exceeds [`SIGHTLINE_MAX_RUN`]. Covering every maximal row and column run
/// covers every cell in each of the 4 cardinal directions.
fn sightlines_bounded(facility: &Facility) -> bool {
    sight_runs(facility)
        .iter()
        .all(|r| r.len <= SIGHTLINE_MAX_RUN)
}

/// Stamp one table into `run` and release its cell from the region graph,
/// keeping grid and graph in lockstep. Returns whether a table landed.
///
/// Candidates are tried centre-out — the middle splits the run most evenly — from
/// a seeded jittered aim point, so cover varies by seed instead of forming a
/// metronomic grid. A candidate is skipped if it is no longer plain floor, would
/// sever guard pathing, or would split its region; a table near an end merely
/// shortens the run, and the [`break_sightlines`] loop comes back for the rest.
fn place_blocker(
    facility: &mut Facility,
    regions: &mut RegionGraph,
    run: &SightRun,
    rng: &mut Rng,
) -> bool {
    // Aim at the middle, jittered by up to a sixth of the run either way.
    let jitter = (run.len / 6).max(1) as i32;
    let aim = (run.start + run.len / 2) as i32 + rng.range_inclusive(-jitter, jitter);
    let aim = aim.clamp(run.start as i32, (run.start + run.len - 1) as i32) as u32;

    let mut order: Vec<u32> = (run.start..run.start + run.len).collect();
    order.sort_by_key(|&i| (i.abs_diff(aim), i));
    // The one-usable preference (§11.4) is *not* applied to sightline cover: the
    // §10.1a rule puts tables squarely in corridors, which are door-rich by
    // construction, so a table doubling with a nearby door is often unavoidable
    // — and steering the blocker off the centre to dodge it only shortens the
    // run instead of splitting it, forcing many more break passes (each a full
    // grid re-scan). The blocker takes the best split; the arrow on the usable
    // line keeps the rare doubled cell unambiguous. Cupboards and consoles/exit,
    // which have plentiful sites, do honour the preference (see their passes).
    for i in order {
        let cell = run.line.cell(i);
        if facility.terrain(cell) != Some(Terrain::Floor)
            || severs_pathing(facility, cell)
            || splits_region(regions, cell)
        {
            continue;
        }
        facility.set_terrain(cell.x, cell.y, Terrain::PartialCover);
        regions.remove_cell(cell);
        return true;
    }
    false
}

/// Whether removing `cell` would split the region that owns it into disconnected
/// pieces. A region is a coherent space (§10.5) — a blocker may narrow a room or
/// corridor, never partition it. This is the region-local complement of
/// [`severs_pathing`]: *pathing* can survive a split (guards detour through a
/// door), but the space itself must stay whole, or "which room am I in" stops
/// meaning anything.
fn splits_region(regions: &RegionGraph, cell: Cell) -> bool {
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

    /// Whether `cell` lies within this inclusive rectangle.
    fn contains(&self, cell: Cell) -> bool {
        (self.x0..=self.x1).contains(&cell.x) && (self.y0..=self.y1).contains(&cell.y)
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
    use crate::test_support::{open_room, seed_sweep};
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
        for seed in seed_sweep(64) {
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
        for seed in seed_sweep(200) {
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
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            for (_, region) in layout.regions().regions() {
                if region.kind() == RegionKind::Room {
                    let (w, h) = bbox(region.cells());
                    assert!(w >= 6 && h >= 6, "seed {seed}: room {w}x{h} below 6x6");
                }
            }
        }
    }

    /// The §10.6 guarantee, and the reason the graph exists (§10.5): every walkable
    /// interior cell belongs to exactly one region, every wall to none. A hideout is
    /// a former floor cell that stays owned by its region — it is a spot *in* the
    /// room or corridor, so cell → region still answers for someone standing in it —
    /// so the walkable interior is floor-or-hideout. Nothing is "painted and forgotten".
    #[test]
    fn every_walkable_cell_belongs_to_exactly_one_region() {
        // Many seeds, because the sightline pass now *removes* cells from regions
        // (a table turns claimed floor into solid cover) — lockstep must survive
        // that.
        for seed in seed_sweep(64) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let facility = layout.facility();
            for y in 0..facility.height() {
                for x in 0..facility.width() {
                    let terrain = facility.terrain_at(x, y);
                    let walkable =
                        terrain == Some(Terrain::Floor) || terrain == Some(Terrain::Hideout);
                    let has_region = layout.regions().region_at(Cell::new(x, y)).is_some();
                    assert_eq!(
                        walkable, has_region,
                        "seed {seed} ({x},{y}): walkable={walkable} but region={has_region}"
                    );
                }
            }
        }
    }

    /// The border is enclosed unconditionally (§10.6): every border cell is wall.
    #[test]
    fn the_border_stays_sealed() {
        for seed in seed_sweep(200) {
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
        // Deliberately `generate_once`: this asserts the *construction* is sound,
        // so the §10.6 gate in `generate` must not get the chance to mask a break
        // by silently rejecting and redrawing.
        for seed in seed_sweep(200) {
            let layout = generate_once(40, 40, &mut Rng::new(seed)).unwrap();
            assert_corridors_connected(&layout, seed);
        }
    }

    /// Connectivity holds across a range of footprints, not just the v1 square.
    #[test]
    fn connectivity_holds_across_sizes() {
        for &(w, h) in &[(18, 18), (24, 40), (40, 24), (33, 51), (60, 60)] {
            for seed in seed_sweep(40) {
                let layout = generate_once(w, h, &mut Rng::new(seed)).unwrap();
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

        let (w, h) = (layout.facility().width(), layout.facility().height());
        let start = *corridor.iter().next().unwrap();
        let reached = path::flood_from(start, w, h, |c| corridor.contains(&c)).len();
        assert_eq!(
            reached,
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
            for seed in seed_sweep(40) {
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
            for seed in seed_sweep(64) {
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
        for seed in seed_sweep(200) {
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
        for seed in seed_sweep(64) {
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

    /// Whether a set of cells is a single 4-connected component.
    fn is_4_connected(cells: &HashSet<Cell>) -> bool {
        let start = match cells.iter().next() {
            Some(&c) => c,
            None => return true,
        };
        // Bound the flood grid to just past the set's extent; membership *is* the
        // passability predicate.
        let w = cells.iter().map(|c| c.x).max().unwrap() + 1;
        let h = cells.iter().map(|c| c.y).max().unwrap() + 1;
        path::flood_from(start, w, h, |c| cells.contains(&c)).len() == cells.len()
    }

    /// A feature never seals a room: every room region's floor stays a single
    /// 4-connected component. This is the operational form of the §10.1.5 "footprint
    /// grown by 1 is clear" rule — a partition wall or pillar keeps its 1-cell moat,
    /// so no pocket of floor is ever cut off (which would also break reachability, #13).
    #[test]
    fn room_floor_stays_connected_after_features() {
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            for (id, region) in layout.regions().regions() {
                if region.kind() != RegionKind::Room {
                    continue;
                }
                let cells: HashSet<Cell> = region.cells().iter().copied().collect();
                assert!(
                    is_4_connected(&cells),
                    "seed {seed}: room {id:?} floor split into pieces by a feature"
                );
            }
        }
    }

    /// The §10.5 payoff: a featured room records its *true* footprint, not its
    /// bounding box. Over many seeds the vast majority of rooms end up with fewer
    /// floor cells than their bounding-box area — a genuine non-rectangular shape
    /// carved by a feature — proving the region graph reflects it (§10.1.5, §10.5).
    #[test]
    fn features_make_rooms_non_rectangular() {
        let (mut carved, mut total) = (0u32, 0u32);
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            for (_, region) in layout.regions().regions() {
                if region.kind() != RegionKind::Room {
                    continue;
                }
                total += 1;
                let (w, h) = bbox(region.cells());
                if (region.cells().len() as u32) < w * h {
                    carved += 1;
                }
            }
        }
        // Every room is ≥6×6, so a partition wall always fits and a pillar usually
        // does — the overwhelming majority should carry a feature.
        assert!(
            carved * 100 >= total * 80,
            "only {carved}/{total} rooms are non-rectangular; features are barely landing"
        );
    }

    /// Feature walls stay strictly interior — a feature is stamped inside a room and
    /// never onto the enclosing border (§10.6), which the border-seal test also
    /// guards from the other direction.
    #[test]
    fn features_never_touch_the_border() {
        for seed in seed_sweep(200) {
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

    /// A pillar needs a 6×6 room (§10.1.5): below that there is no space for a
    /// 2-cell block plus the freestanding 1-cell margin. A 5×5 room never yields
    /// one; a 6×6 room can.
    #[test]
    fn pillars_need_a_six_by_six_room() {
        // A 5×5 interior: propose_pillar rejects every draw.
        let small = Facility::walled_box(7, 7);
        let small_room = Rect::new(1, 1, 5, 5);
        for seed in 0..64 {
            assert!(
                propose_pillar(&small_room, &small, &mut Rng::new(seed)).is_none(),
                "a 5x5 room must never host a pillar"
            );
        }
        // A 6×6 interior: at least one draw fits.
        let big = Facility::walled_box(8, 8);
        let big_room = Rect::new(1, 1, 6, 6);
        assert!(
            (0..64).any(|seed| propose_pillar(&big_room, &big, &mut Rng::new(seed)).is_some()),
            "a 6x6 room should be able to host a pillar"
        );
    }

    /// A proposed feature always respects its clearance: every stub/pillar cell and
    /// its checked halo lies on floor inside the room. Asserted directly on the
    /// proposals for a fresh room so a regression in the "grown by 1 is clear" rule
    /// (§10.1.5) shows up close to the code that enforces it.
    #[test]
    fn proposed_features_stay_on_interior_floor() {
        let facility = Facility::walled_box(20, 20);
        let room = Rect::new(1, 1, 18, 18);
        for seed in 0..128 {
            let mut rng = Rng::new(seed);
            for _ in 0..FEATURE_ATTEMPTS {
                for proposal in [
                    propose_partition(&room, &facility, &mut rng),
                    propose_pillar(&room, &facility, &mut rng),
                ]
                .into_iter()
                .flatten()
                {
                    for cell in proposal {
                        assert!(
                            room.contains(cell) && facility.terrain(cell) == Some(Terrain::Floor),
                            "seed {seed}: feature cell {cell:?} is off the room floor"
                        );
                    }
                }
            }
        }
    }

    /// Every cell that renders as a hideout in a layout.
    fn hideout_cells(layout: &Layout) -> Vec<Cell> {
        let f = layout.facility();
        (0..f.height())
            .flat_map(|y| (0..f.width()).map(move |x| Cell::new(x, y)))
            .filter(|&c| f.terrain(c) == Some(Terrain::Hideout))
            .collect()
    }

    /// The hiding game needs a *board* (§10.1a): the v1 config gets a healthy spread
    /// of hideouts every seed, not the one-or-none the old harvester produced.
    #[test]
    fn the_v1_config_gets_a_board_of_hideouts() {
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let count = hideout_cells(&layout).len();
            assert!(
                count >= 6,
                "seed {seed}: only {count} hideouts — not a board"
            );
        }
    }

    /// The headline §10.1a fix: hideouts land on the corridor network, not only in
    /// rooms — the flight path is exactly where the player needs cover. Asserted per
    /// seed, since a chase can happen in any corridor.
    #[test]
    fn hideouts_land_on_corridors() {
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let regions = layout.regions();
            let on_corridor = hideout_cells(&layout).into_iter().any(|c| {
                regions
                    .region_at(c)
                    .is_some_and(|id| regions.kind(id) == RegionKind::Corridor)
            });
            assert!(on_corridor, "seed {seed}: no hideout on any corridor");
        }
    }

    /// Every hideout is a cupboard: backed by a wall or pillar face, never floating,
    /// never clogging a door throat, and always enterable from open floor.
    #[test]
    fn every_hideout_is_a_wall_backed_cupboard() {
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let f = layout.facility();
            for c in hideout_cells(&layout) {
                let mut wall_backed = false;
                let mut enterable = false;
                for n in f.neighbors(c) {
                    match f.terrain(n) {
                        Some(Terrain::Wall) => wall_backed = true,
                        Some(Terrain::DoorHinge)
                        | Some(Terrain::DoorPanelClosed)
                        | Some(Terrain::DoorPanelOpen) => {
                            panic!("seed {seed}: hideout {c:?} clogs a door throat")
                        }
                        Some(Terrain::Floor) => enterable = true,
                        _ => {}
                    }
                }
                assert!(wall_backed, "seed {seed}: hideout {c:?} is not wall-backed");
                assert!(
                    enterable,
                    "seed {seed}: hideout {c:?} cannot be stepped into"
                );
            }
        }
    }

    /// Cupboards are spread, not banked: no two hideouts sit within the spacing
    /// (§10.1a **[START]**), so a 2-wide corridor never has both walls blocked at one
    /// cross-section.
    #[test]
    fn hideouts_keep_their_spacing() {
        for seed in seed_sweep(200) {
            let cells = hideout_cells(&generate(40, 40, &mut Rng::new(seed)).unwrap());
            for (i, &a) in cells.iter().enumerate() {
                for &b in &cells[i + 1..] {
                    assert!(
                        a.manhattan_distance(b) >= HIDEOUT_MIN_SPACING,
                        "seed {seed}: hideouts {a:?} and {b:?} are too close"
                    );
                }
            }
        }
    }

    /// A hideout blocks pathing (§10.3), so the board must never wall a patrol route
    /// off: the pathable space (everything a guard can route through, hideouts
    /// excluded) stays a single connected component. This is what [`severs_pathing`]
    /// guarantees, and it protects the reachability the placement ticket asserts (#13).
    #[test]
    fn hideouts_keep_guard_pathing_connected() {
        // `generate_once`, not `generate`: the §10.6 gate checks this exact
        // property, so going through the entry point would mask a regression in
        // `severs_pathing` as silent rejections instead of a red test.
        for seed in seed_sweep(200) {
            let layout = generate_once(40, 40, &mut Rng::new(seed)).unwrap();
            let f = layout.facility();
            let pathable: HashSet<Cell> = (0..f.height())
                .flat_map(|y| (0..f.width()).map(move |x| Cell::new(x, y)))
                .filter(|&c| f.terrain(c).is_some_and(|t| !t.blocks_pathing()))
                .collect();
            assert!(
                is_4_connected(&pathable),
                "seed {seed}: hideouts split guard pathing"
            );
        }
    }

    /// The longest counterplay-free straight run in the grid, measured
    /// independently of the generator's own scanner: walk every row and column
    /// counting consecutive cells that neither block sight nor provide cover —
    /// the §10.1a measure (a table is see-through, but it is the counterplay
    /// the rule demands).
    fn longest_straight_run(f: &Facility) -> u32 {
        let (w, h) = (f.width(), f.height());
        let clear = |x: u32, y: u32| {
            f.terrain_at(x, y)
                .is_some_and(|t| !t.blocks_sight() && !t.provides_cover())
        };
        let mut longest = 0u32;
        for y in 0..h {
            let mut run = 0;
            for x in 0..w {
                run = if clear(x, y) { run + 1 } else { 0 };
                longest = longest.max(run);
            }
        }
        for x in 0..w {
            let mut run = 0;
            for y in 0..h {
                run = if clear(x, y) { run + 1 } else { 0 };
                longest = longest.max(run);
            }
        }
        longest
    }

    /// The headline §10.1a property: **no unbroken straight sightline longer than
    /// L**, for every cell in each of the 4 cardinal directions — equivalently, no
    /// maximal row or column run exceeds [`SIGHTLINE_MAX_RUN`]. Asserted on
    /// [`generate`]'s accepted layouts across footprints: like reachability, the
    /// rule is "repaired or the seed rejected" (§10.1a), so acceptance is where it
    /// is unconditional.
    #[test]
    fn no_sightline_exceeds_the_cap() {
        for &(w, h) in &[(18, 18), (40, 40), (24, 40), (60, 60)] {
            for seed in seed_sweep(64) {
                let layout = generate(w, h, &mut Rng::new(seed)).unwrap();
                let run = longest_straight_run(layout.facility());
                assert!(
                    run <= SIGHTLINE_MAX_RUN,
                    "{w}x{h} seed {seed}: a {run}-cell sightline on an accepted level"
                );
            }
        }
    }

    /// The §10.1a stamped cover is **furniture, not wall** (#52): the cover pass
    /// stamps tables and only tables, so a corridor blocker never reads as a
    /// floating wall cell. Driven on a bare gallery where every stamp must come
    /// from the cover pass — and the crouch trade is visible in the terrain: the
    /// gallery satisfies the counterplay measure while staying *optically* open
    /// end to end (a guard still sees straight over every table).
    #[test]
    fn the_cover_pass_stamps_tables_not_walls() {
        let mut f = Facility::walled_box(30, 8);
        // Claim the interior as one corridor region, as the real partition would:
        // the pass releases each stamped cell, and only owned cells release.
        let mut regions = RegionGraph::new(30, 8);
        regions.add_region(RegionKind::Corridor, Rect::new(1, 1, 28, 6).cells());
        break_sightlines(&mut f, &mut regions, &mut Rng::new(7));

        assert!(sightlines_bounded(&f), "the gallery must be repaired");
        let mut tables = 0;
        for y in 1..f.height() - 1 {
            for x in 1..f.width() - 1 {
                match f.terrain_at(x, y) {
                    Some(Terrain::Floor) => {}
                    Some(Terrain::PartialCover) => tables += 1,
                    t => panic!("({x},{y}): the cover pass stamped {t:?}"),
                }
            }
        }
        assert!(tables > 0, "a 28-cell gallery cannot pass uncovered");

        // Optically the interior is still one open span per row: no stamped cell
        // blocks sight, so the pure-opacity run down row 1 spans the full 28.
        let opacity_run = (1..f.width() - 1)
            .take_while(|&x| f.terrain_at(x, 1).is_some_and(|t| !t.blocks_sight()))
            .count() as u32;
        assert_eq!(opacity_run, f.width() - 2, "tables must not cast shadows");
    }

    /// The repair must stay a repair: [`break_sightlines`] satisfies §10.1a on
    /// nearly every *raw* carve, with the §10.6 rejection reserved for genuinely
    /// cornered geometry. Without this pin, the pass could silently rot into
    /// "reject and redraw until lucky" and nothing above would notice — measured
    /// at 1-in-1000 on the v1 config when written, budgeted at 2% here.
    #[test]
    fn the_cover_pass_repairs_almost_every_carve() {
        // Budget is the 2% rate scaled to the sweep width, floored at 1 so a single
        // unlucky sampled seed never flakes; the full CI sweep restores the 4/200 pin.
        let seeds = seed_sweep(200);
        let budget = (4 * seeds.len() / 200).max(1);
        let unrepaired = seeds
            .iter()
            .filter(|&&seed| {
                let layout = generate_once(40, 40, &mut Rng::new(seed)).unwrap();
                !sightlines_bounded(layout.facility())
            })
            .count();
        assert!(
            unrepaired <= budget,
            "{unrepaired}/{} carves left unrepaired (budget {budget}) — the cover pass has degraded",
            seeds.len()
        );
    }

    /// §10.1a **[START]** pins: *L* stays in the settled 10–12 band, "roughly a
    /// guard's sight range". A tune that moves the knob out of the band — or lets
    /// a guard's sight outgrow the cover that answers it — must move this pin
    /// deliberately.
    #[test]
    fn the_sightline_cap_sits_in_the_settled_band() {
        assert!(
            (10..=12).contains(&SIGHTLINE_MAX_RUN),
            "SIGHTLINE_MAX_RUN {SIGHTLINE_MAX_RUN} left the §10.1a 10–12 band"
        );
        let range = crate::vision::GUARD_SIGHT_RANGE;
        assert!(
            SIGHTLINE_MAX_RUN.abs_diff(range) <= 2,
            "L {SIGHTLINE_MAX_RUN} drifted from GUARD_SIGHT_RANGE {range}"
        );
    }

    /// An empty straight gallery — enclosed, connected, and one naked sightline —
    /// fails the gate on §10.1a alone; a hall shorter than the cap passes. The
    /// sightline rule is a first-class §10.6 guarantee, not a style preference.
    #[test]
    fn a_long_gallery_fails_the_gate() {
        // 30×8 box: a 28-cell unbroken run down every interior row.
        let long = open_room(30, 8);
        assert!(fully_enclosed(long.facility()) && pathable_connected(long.facility()));
        assert!(!sightlines_bounded(long.facility()));
        assert!(!passes_guarantees(&long));

        // 13×8 box: interior runs of 11 = SIGHTLINE_MAX_RUN, exactly at the cap.
        let short = open_room(13, 8);
        assert!(sightlines_bounded(short.facility()));
        assert!(passes_guarantees(&short));
    }

    /// The §10.6 flood fill rejects the exact failure the old generator shipped:
    /// a room sealed shut, its contents unreachable, nothing noticing. Built by
    /// hand, since the real carve (correctly) never produces one.
    #[test]
    fn a_sealed_pocket_fails_the_gate() {
        let mut f = Facility::walled_box(12, 12);
        for y in 1..=10 {
            f.set_terrain(6, y, Terrain::Wall);
        }
        let layout = Layout::from_facility(f);
        assert!(fully_enclosed(layout.facility()), "border is intact");
        assert!(
            !pathable_connected(layout.facility()),
            "the east half is sealed off"
        );
        assert!(!passes_guarantees(&layout));
    }

    /// A closed door panel is transparent to pathing (§10.3/§10.4), so a room
    /// whose only way out is a closed door is reachable — not a sealed pocket.
    #[test]
    fn a_closed_door_counts_as_reachable() {
        let mut f = Facility::walled_box(12, 12);
        for y in 1..=10 {
            f.set_terrain(6, y, Terrain::Wall);
        }
        f.set_terrain(6, 5, Terrain::DoorPanelClosed);
        assert!(passes_guarantees(&Layout::from_facility(f)));
    }

    /// §10.6 "fully enclosed" is asserted, not assumed: a breached border ring
    /// fails the gate even though the interior stays connected.
    #[test]
    fn a_breached_border_fails_the_gate() {
        let mut f = Facility::walled_box(12, 12);
        f.set_terrain(0, 5, Terrain::Floor);
        assert!(!passes_guarantees(&Layout::from_facility(f)));
    }

    /// The entry point's contract (#13): every layout [`generate`] accepts passes
    /// every §10.6 assertion — no caller ever receives an unsolvable level.
    #[test]
    fn accepted_seeds_always_pass_the_gate() {
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            assert!(passes_guarantees(&layout), "seed {seed}: gate breached");
        }
    }

    /// The retry cap is a real cap: a config that can never validate fails loudly
    /// with [`GenError::RetriesExhausted`] instead of spinning forever (§10.6
    /// "fail loudly or retry the seed" — this is both, in order).
    #[test]
    fn an_unsatisfiable_config_fails_loudly() {
        let err = generate_where(40, 40, &mut Rng::new(0), |_| false).unwrap_err();
        assert_eq!(
            err,
            GenError::RetriesExhausted {
                attempts: MAX_GEN_ATTEMPTS
            }
        );
    }
}
