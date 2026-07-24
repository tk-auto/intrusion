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
use crate::duct::Duct;
use crate::facility::{Facility, Terrain};
use crate::path;
use crate::place::{place, LevelConfig, Placement};
use crate::region::{DoorId, DoorKind, RegionGraph, RegionId, RegionKind};
use crate::rng::Rng;
use std::collections::HashSet;

// The generation pipeline is split one file per phase; `generate_once` below stays
// the thin orchestrator that runs them in order. Each phase module is re-globbed
// here so the orchestrator and the tests call its helpers unqualified.
mod carve;
mod doors;
mod ducts;
mod features;
mod hideouts;
mod sightlines;
mod walls;
use carve::*;
use doors::*;
use ducts::*;
use features::*;
use hideouts::*;
use sightlines::*;
use walls::*;

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
/// The share of doorways generated as **automatic** (§10.4/#147) **[START]**:
/// frameless spans that shut themselves, versus manual hinged doors that a hand or a
/// passing Calm guard (#146) closes. Drawn per doorway from the seeded RNG (§12.4),
/// so the same seed makes the same doors automatic. A minority — most doors stay
/// manual, keeping the hinge/bump vocabulary the common case; the automatics are the
/// self-healing seam that stops a busy wing propping every door open.
const AUTO_DOOR_PERCENT: u32 = 30;
/// How long an automatic door stays open after its doorway is last vacated
/// (§10.4/#147, **[START] = 3** turns): short but nonzero, so a guard passing through
/// leaves a real slip-through window before the door shuts (the ticket's stealth knob).
const AUTO_CLOSE_DELAY: u32 = 3;
/// The most doorways any one room gets **[START]**. A room with a door on every
/// wall is a thoroughfare, not a room — most rooms want one or two ways in, and a
/// three-door hub should be the exception. Every room still keeps at least one
/// door, so none is ever sealed off (§10.6). The per-room count is drawn by
/// [`room_door_budget`].
const MAX_DOORS_PER_ROOM: u32 = 3;

/// The percentage of doorways that generate already **open** (#145, §10.4/§11.3)
/// **[START]**. Every door otherwise starts closed; opening ~1-in-5 lets the
/// facility read lived-in and varies the turn-one sightlines run to run. Kept
/// deliberately low: an open door is a permanent sightline until a guard closes it
/// (#146) or an automatic door times out (#147), so until those give the level a
/// way to heal, few open doors is safer than many. Pinned by a test. Drawn from the
/// seeded RNG so the same seed opens the same doors (§12.4).
const OPEN_DOOR_PERCENT: u32 = 20;

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

/// Hideouts sit at least this far apart (Manhattan), so the board is spread *along*
/// a flight path rather than clumped into a bank of cupboards (§10.1a). Big enough
/// that the facility still reads as a building rather than a honeycomb; small enough
/// that a fleeing player is rarely more than a few steps from cover. Density is the
/// open tuning knob here (§10.1a, §15.2).
///
/// The spacing is now **region-aware** (§10.1.6, #91): the flight path is the
/// corridor, so corridors host cupboards **denser** than rooms — the cupboard you
/// vanish into on the run belongs where you run (§7.6). Both are **[START]**.
const HIDEOUT_MIN_SPACING_CORRIDOR: u32 = 5;
/// A room's cupboards are spaced wider than a corridor's — a room is where you crouch
/// behind furniture, not where you dive into a wall to vanish (§10.3). **[START]**
const HIDEOUT_MIN_SPACING_ROOM: u32 = 10;

/// Roughly one interior wall run in this many is thickened to two cells before the
/// cover and hideout passes (§10.1.5, the [`thicken_walls`] pass) **[START]**. A
/// two-thick wall is the backing a **recessed** cupboard needs (§10.1.6) and reads
/// as a pilaster/buttress rather than a bare partition. Not every wall — "a third of
/// them" keeps the facility from turning fortress-thick; the value is the single
/// named knob. `1` here would thicken every eligible run, higher numbers fewer.
const WALL_THICKEN_ONE_IN: u32 = 3;

/// The shortest wall run [`thicken_walls`] will thicken. Below this a thickened
/// stretch has no flush interior cell (one whose lateral neighbours along the wall
/// are both solid), so it would seed no recessed cupboard and only eat room floor.
const WALL_THICKEN_MIN_RUN: u32 = 3;

/// No unbroken straight sightline may exceed this many cells — §10.1a, the
/// generator's most important job after connectivity. The *rule* is **[SETTLED]**;
/// the value is **[START]**: the design band is 10–12, "roughly a guard's sight
/// range" (§7.1's `GUARD_SIGHT_RANGE` is 10), and 11 splits it. This is the single
/// named knob for the §15.2 how-much-cover experiments — longer than this and
/// there is no geometry between the player and being seen.
pub const SIGHTLINE_MAX_RUN: u32 = 11;

/// The §10.1a run limit **inside rooms** — tighter than the corridor floor
/// [`SIGHTLINE_MAX_RUN`], so a room breaks its straights sooner and carries
/// **proportionally more tables** (§10.1.6, #91): the room is where you duck behind
/// furniture and crouch (§10.3). This is a *preference layered on top of* the hard
/// §10.1a floor — it only ever adds cover, never removes it, so the uniform
/// guarantee still holds. **[START]** (must stay ≤ [`SIGHTLINE_MAX_RUN`], and above a
/// room's 6-cell minimum lane so a small room is not needlessly furnished).
pub const SIGHTLINE_MAX_RUN_ROOM: u32 = 7;

/// The placement-density knobs, region-aware (#91). Kept as a value so the bias can
/// be A/B'd against the old uniform numbers (a room and a corridor treated alike) —
/// [`Tuning::UNIFORM`] reproduces pre-#91 behaviour, [`Tuning::BIASED`] is what ships.
#[derive(Clone, Copy)]
struct Tuning {
    /// Minimum Manhattan spacing between cupboards opening onto a corridor.
    hideout_spacing_corridor: u32,
    /// Minimum Manhattan spacing between cupboards opening onto a room.
    hideout_spacing_room: u32,
    /// The §10.1a run limit applied to room-dominated straights.
    room_sightline_max_run: u32,
}

impl Tuning {
    /// The shipped bias: denser cupboards along corridors, more tables in rooms.
    const BIASED: Tuning = Tuning {
        hideout_spacing_corridor: HIDEOUT_MIN_SPACING_CORRIDOR,
        hideout_spacing_room: HIDEOUT_MIN_SPACING_ROOM,
        room_sightline_max_run: SIGHTLINE_MAX_RUN_ROOM,
    };
    /// The pre-#91 numbers: rooms and corridors treated identically. Retained so the
    /// bias test can measure the shift against it, not a brittle absolute count.
    #[cfg(test)]
    const UNIFORM: Tuning = Tuning {
        hideout_spacing_corridor: 7,
        hideout_spacing_room: 7,
        room_sightline_max_run: SIGHTLINE_MAX_RUN,
    };
}

/// The most tables one cover placement clusters into a **bench** (§10.1a) **[START]**.
/// Each placement stamps a short straight row of tables — a workbench, a desk — up
/// to this many cells, stopping at a wall or at a cell that would seal the passage
/// (so a pathing gap always survives). One bench breaks every lane it spans at once,
/// so the whole facility carries **fewer, organized** pieces — benches, not confetti
/// — for the same sightline guarantee. This is a named knob for the §15.2
/// cover-density experiments.
const COVER_BENCH_MAX: u32 = 4;

/// The fewest tables a bench may hold (§10.1a) **[START]**. A single lone table
/// reads as scattered noise, not furniture — the very confetti the bench mechanism
/// was built to kill — so a placement that cannot reach this length is abandoned
/// (rolled back) rather than left as a one-cell stamp. Must stay ≥ 2 and ≤
/// [`COVER_BENCH_MAX`].
const COVER_BENCH_MIN: u32 = 2;

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
    /// The player-only crawlspace shortcuts that span the facility (§10.7). Each is an
    /// ordered path of cells whose ends are [`Terrain::DuctEntry`] and whose interior
    /// cells keep whatever terrain they already had — the path may cross room and
    /// corridor **floor** to connect two far-apart regions (§10.7 cross-room routing),
    /// so this list is the *only* record that those cells are also a crawl route;
    /// nothing on the grid tells. Empty on a level the generator placed none on (ducts
    /// are optional — reachability never depends on one, §10.6/§10.7).
    ducts: Vec<Duct>,
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

    /// The duct crawlspaces on this level (§10.7), for the turn loop and renderer.
    pub fn ducts(&self) -> &[Duct] {
        &self.ducts
    }

    /// The index (into [`ducts`](Self::ducts)) of the duct whose path includes `cell`,
    /// if any (§10.7). The turn loop reads this at the one moment it needs to bind
    /// "the player is now inside a duct" to a concrete duct: bumping a mouth to climb
    /// in ([`State::in_duct`](crate::State)). It is *not* a per-turn "am I in a duct"
    /// query — that state is stored on the [`State`](crate::State), because a duct's
    /// interior may overlie ordinary floor and a cell alone can no longer answer it.
    pub fn duct_index_containing(&self, cell: Cell) -> Option<usize> {
        self.ducts.iter().position(|d| d.contains(cell))
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
            ducts: Vec::new(),
        }
    }

    /// A hand-made layout from both halves — for fixtures that need real regions
    /// and doors (a region beat, a guard-opened door) without running the full
    /// generator. The caller keeps the two in lockstep, as the generator would.
    pub(crate) fn from_parts(facility: Facility, regions: RegionGraph) -> Self {
        Self {
            facility,
            regions,
            ducts: Vec::new(),
        }
    }

    /// Attach hand-built ducts to a fixture layout (§10.7) — for turn-loop tests
    /// that exercise crawl/peek/concealment without running the generator. The
    /// caller stamps the entry cells as [`Terrain::DuctEntry`] to match.
    pub(crate) fn with_ducts(mut self, ducts: Vec<Duct>) -> Self {
        self.ducts = ducts;
        self
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
    generate_where(width, height, rng, passes_guarantees, &Tuning::BIASED)
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
        let mut layout = generate_once(config.width, config.height, rng, &Tuning::BIASED)?;
        if !passes_guarantees(&layout) {
            continue;
        }
        // Layer the #145 initial door state on the *validated* carve, before
        // placement: the §10.6 guarantees describe the closed-door geometry, and
        // door open/closed is live state (§11.3), so this comes after the gate and
        // before `place` — whose §10.1.9 turn-one cone check must see the open
        // doorways a guard's cone now reaches through.
        open_initial_doors(&mut layout, rng);
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
    tuning: &Tuning,
) -> Result<Layout, GenError> {
    for _ in 0..MAX_GEN_ATTEMPTS {
        let layout = generate_once(width, height, rng, tuning)?;
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
fn generate_once(
    width: u32,
    height: u32,
    rng: &mut Rng,
    tuning: &Tuning,
) -> Result<Layout, GenError> {
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

    // Step 5a (§10.1.5): thicken about a third of the interior walls to two cells,
    // giving recessed cupboards their solid backing (§10.1.6) and the facility some
    // buttresses. After doorways so a thickened wall steers clear of a throat, and
    // before the hideout pass so it sees the true, thick walls. Adding wall can only
    // *shorten* sightlines, never lengthen one, so this never fights the §10.1a rule.
    thicken_walls(&mut facility, &mut regions, rng);

    // Step 6: the hiding-game board — concealment cupboards **recessed** into the
    // two-thick walls and pillar faces, spread along the flight paths (§10.1.6,
    // §10.1a). Before the sightline pass, not after: a recessed cupboard is
    // see-through, so it must be on the grid when §10.1a is measured and repaired, or
    // a run lengthened by one open recess could slip past uncovered. (Ordering is now
    // free to put it here — `recess_site` demands three *wall* neighbours, so a table
    // can never back a cupboard whether it is stamped before or after.)
    place_hideouts(&mut facility, &mut regions, rng, tuning);

    // §10.1a: break every straight sightline longer than SIGHTLINE_MAX_RUN — with
    // a bench of tables in a room, one more recessed cupboard in a corridor — last
    // of the sight-affecting passes, so it measures and repairs the final grid,
    // thick walls and open recesses included, and `passes_guarantees` re-asserts
    // the result.
    break_sightlines(&mut facility, &mut regions, rng, tuning);

    // Step 7 (§10.7): thread a small number of player-only duct crawlspaces through
    // the walls, each a shortcut between two regions far apart on the region graph.
    // Last of all, and deliberately after the §10.6/§10.1a gate's inputs are fixed:
    // a duct entry is wall-like in every guard-facing property (opaque, solid,
    // pathing-blocking), so converting a wall to an entry changes neither
    // reachability nor a sightline — the crawl route it opens is the player's alone.
    let ducts = place_ducts(&mut facility, &regions, rng);

    debug_assert!(corridors > 0, "guarded footprint yielded no corridor");
    Ok(Layout {
        facility,
        regions,
        ducts,
    })
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
///   without counterplay in it: an obstruction, a partial-cover table, or a
///   cupboard mouth (§10.1a — neither a table nor a flush recess blocks a guard's
///   sight, but the one plants the §10.3 crouch and the other a cell to vanish
///   from, which is the geometry-between-you-and-being-seen the rule demands).
///   [`break_sightlines`] repairs the carve, but the rule is *measured* here on
///   the finished grid — a run the repair could not break rejects the carve,
///   exactly like a reachability failure.
///
/// Room size and count are not re-checked here: they are fixed by the partition
/// constants before any wall is stamped, and the property tests below pin them.
///
/// **One usable per cell is a preference, not a guarantee** (§11.4): the
/// stamping passes ([`place_bench`], [`place_hideouts`]) and placement
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
            // A duct entry is a bumpable usable too (§11.4 "duct: enter"), so
            // placement avoids crowding its mouth with a second adjacent usable.
            | Terrain::DuctEntry
    )
}

/// Whether `cell` already has at least one usable orthogonally adjacent — a
/// door cell, a table, a cupboard, or one of `extra` (placement's consoles and
/// exit). Terrain-only and four lookups: the §11.4 one-usable checks only ever
/// ask this yes/no question, never a deduped count, so this never touches the
/// door list (which turned the check quadratic).
pub(crate) fn has_adjacent_usable(facility: &Facility, cell: Cell, extra: &[Cell]) -> bool {
    facility
        .neighbours(cell)
        .any(|n| extra.contains(&n) || facility.terrain(n).is_some_and(is_usable_terrain))
}

/// Whether stamping a usable at `cell` would give some floor neighbour a
/// **second** adjacent usable — the §11.4 one-usable *preference*. The stamping
/// passes consult this to prefer a cleaner site; unlike a guarantee it may be
/// overridden (a sightline that only one crowded cell can break, a structural
/// door cluster), so nothing asserts its absence — the arrow disambiguates a
/// doubled cell instead.
fn creates_usable_conflict(facility: &Facility, cell: Cell) -> bool {
    facility.neighbours(cell).any(|f| {
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
#[derive(Clone, Copy, PartialEq, Eq)]
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

/// `cell` shifted by `(dx, dy)`. The room interior sits well inside the border, so
/// feature offsets never underflow the grid.
fn offset(cell: Cell, (dx, dy): (i32, i32)) -> Cell {
    Cell::new((cell.x as i32 + dx) as u32, (cell.y as i32 + dy) as u32)
}

/// A maximal straight run of plain wall cells along one scan line — the unit
/// [`thicken_walls`] decides to thicken or leave.
#[derive(Clone, Copy)]
struct WallRun {
    line: Line,
    start: u32,
    len: u32,
}

/// Chebyshev (chessboard) distance between two cells — the number of king moves.
fn chebyshev(a: Cell, b: Cell) -> u32 {
    a.sight_distance(b)
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

/// A maximal straight run of counterplay-free cells along one scan line — the
/// unit the §10.1a rule is measured in.
#[derive(Clone, Copy)]
struct SightRun {
    line: Line,
    start: u32,
    len: u32,
}

/// The furniture poses a bench may land in (§10.1.6) — how the piece relates to
/// the room's walls, which is what makes a stamped row read as *placed* rather
/// than scattered.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BenchPose {
    /// Touching no wall: the piece sits in the open, crouch cover on every side.
    FreeStanding,
    /// Square against a wall at exactly one end, jutting into the room — a desk
    /// or workbench pushed up to the wall.
    EndOn,
    /// Flush along one wall side, like a counter. Only the ends offer useful
    /// crouch cover: the §10.3 concealment quarter-plane behind the long side is
    /// the wall itself.
    AlongWall,
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

    /// The bounding box of a region's **floor lane** — its cells minus the recessed
    /// cupboards on the wall line. Room-size and corridor-width are guarantees about
    /// the walkable lane (§10.1); a cupboard recessed into a wall joins the region it
    /// opens onto (§10.1.6) but sits *outside* that lane, so it must not count toward
    /// the lane's extent.
    fn floor_bbox(facility: &Facility, cells: &[Cell]) -> (u32, u32) {
        let floor: Vec<Cell> = cells
            .iter()
            .copied()
            .filter(|&c| facility.terrain(c) == Some(Terrain::Floor))
            .collect();
        bbox(&floor)
    }

    fn regions_of_kind(layout: &Layout, kind: RegionKind) -> usize {
        layout
            .regions()
            .regions()
            .filter(|(_, r)| r.kind() == kind)
            .count()
    }

    /// The placement densities over a seed sweep under `tuning`, as
    /// `(corridor_hideout, room_hideout, corridor_table, room_table)` — cupboards
    /// or tables per walkable cell of that region kind. The metric behind the #91
    /// bias, measured against [`Tuning::UNIFORM`] rather than a brittle absolute.
    fn placement_shares(seeds: &[u64], tuning: &Tuning) -> (f64, f64, f64, f64) {
        let (mut cc, mut rc) = (0u32, 0u32); // corridor / room walkable cells
        let (mut ch, mut rh) = (0u32, 0u32); // corridor / room hideouts
        let (mut ct, mut rt) = (0u32, 0u32); // corridor / room tables
        for &seed in seeds {
            let layout =
                generate_where(40, 40, &mut Rng::new(seed), passes_guarantees, tuning).unwrap();
            let (f, g) = (layout.facility(), layout.regions());
            for (_, region) in g.regions() {
                let hideouts = region
                    .cells()
                    .iter()
                    .filter(|&&c| f.terrain(c) == Some(Terrain::Hideout))
                    .count() as u32;
                if region.kind() == RegionKind::Room {
                    rc += region.cells().len() as u32;
                    rh += hideouts;
                } else {
                    cc += region.cells().len() as u32;
                    ch += hideouts;
                }
            }
            for y in 0..f.height() {
                for x in 0..f.width() {
                    let c = Cell::new(x, y);
                    if f.terrain(c) != Some(Terrain::PartialCover) {
                        continue;
                    }
                    // A table is region-less (solid cover); name it by an adjacent
                    // floor cell's region.
                    match f
                        .neighbours(c)
                        .find_map(|n| g.region_at(n).map(|id| g.kind(id)))
                    {
                        Some(RegionKind::Room) => rt += 1,
                        Some(RegionKind::Corridor) => ct += 1,
                        None => {}
                    }
                }
            }
        }
        (
            ch as f64 / cc.max(1) as f64,
            rh as f64 / rc.max(1) as f64,
            ct as f64 / cc.max(1) as f64,
            rt as f64 / rc.max(1) as f64,
        )
    }

    /// #91 sharpened into a rule: hideouts lean into corridors (denser than rooms,
    /// denser than the uniform tuning), and a table is **room furniture only** —
    /// corridors carry none at all, under either tuning, because
    /// [`can_take_table`] refuses corridor floor structurally rather than by
    /// preference. Directions are asserted against [`Tuning::UNIFORM`] over a
    /// seed sweep, not as brittle absolute counts.
    #[test]
    fn placement_is_biased_by_region() {
        let seeds = seed_sweep(48);
        let (u_ch, _u_rh, u_ct, u_rt) = placement_shares(&seeds, &Tuning::UNIFORM);
        let (b_ch, b_rh, b_ct, b_rt) = placement_shares(&seeds, &Tuning::BIASED);

        // Hideouts lean harder into corridors than the uniform tuning, and than
        // rooms do.
        assert!(
            b_ch > u_ch,
            "corridor hideout share should rise vs uniform: {b_ch:.4} vs {u_ch:.4}"
        );
        assert!(
            b_ch > b_rh,
            "hideouts should favour corridors over rooms: {b_ch:.4} vs {b_rh:.4}"
        );
        // Tables are room furniture, full stop — no tuning puts one in a corridor.
        assert!(
            b_ct == 0.0 && u_ct == 0.0,
            "corridors must carry no tables: biased {b_ct:.4}, uniform {u_ct:.4}"
        );
        // The #91 room preference still bites: the tighter room run-limit stamps
        // more room cover than the uniform limit does.
        assert!(
            b_rt > u_rt,
            "room table share should rise vs uniform: {b_rt:.4} vs {u_rt:.4}"
        );
        assert!(b_rt > 0.0, "rooms must still carry crouch cover");
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
                    let (w, h) = floor_bbox(layout.facility(), region.cells());
                    let narrow = w.min(h);
                    assert!(
                        (CORRIDOR_MIN_WIDTH..=CORRIDOR_MAX_WIDTH).contains(&narrow),
                        "seed {seed}: corridor narrow dim {narrow} outside 2..=4"
                    );
                }
            }
        }
    }

    /// Every room's floor lane is always ≥ 6×6 (§10.1) — the thicken pass (§10.1.5)
    /// may erode a wall inward, but never past the minimum ([`thinning_underruns_room`]).
    #[test]
    fn rooms_are_at_least_6x6() {
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            for (_, region) in layout.regions().regions() {
                if region.kind() == RegionKind::Room {
                    let (w, h) = floor_bbox(layout.facility(), region.cells());
                    assert!(w >= 6 && h >= 6, "seed {seed}: room {w}x{h} below 6x6");
                }
            }
        }
    }

    /// The §10.6 guarantee, and the reason the graph exists (§10.5): every walkable
    /// interior cell belongs to exactly one region, every wall to none. A recessed
    /// hideout is a former *wall* cell that the cupboard pass claims for the region it
    /// opens onto (§10.1.6) — it is a spot *in* that room or corridor, so cell → region
    /// still answers for someone ducked inside — so the walkable interior is
    /// floor-or-hideout. Nothing is "painted and forgotten".
    #[test]
    fn every_walkable_cell_belongs_to_exactly_one_region() {
        // Many seeds, because generation both *removes* cells from regions (a table
        // turns claimed floor into solid cover; a thickened wall eats room floor) and
        // *adds* them (a recessed cupboard claims a wall cell) — lockstep must survive
        // both.
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
            let layout = generate_once(40, 40, &mut Rng::new(seed), &Tuning::BIASED).unwrap();
            assert_corridors_connected(&layout, seed);
        }
    }

    /// Connectivity holds across a range of footprints, not just the v1 square.
    #[test]
    fn connectivity_holds_across_sizes() {
        for &(w, h) in &[(18, 18), (24, 40), (40, 24), (33, 51), (60, 60)] {
            for seed in seed_sweep(40) {
                let layout = generate_once(w, h, &mut Rng::new(seed), &Tuning::BIASED).unwrap();
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

    /// Every doorway is a valid §10.4 span of 3–6 cells on one straight wall line,
    /// shaped by its kind (§10.4/#147): a **manual** door is 2 hinges around 1–4
    /// panels, an **automatic** door is 3–6 panels and no hinges (the frameless span).
    #[test]
    fn doorways_are_well_formed_spans() {
        for seed in seed_sweep(64) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            for (_, door) in layout.regions().doors() {
                match door.kind() {
                    DoorKind::Manual => {
                        assert_eq!(door.hinges().len(), 2, "seed {seed}: a hinge at each end");
                        let panels = door.panels().len();
                        assert!(
                            (1..=4).contains(&panels),
                            "seed {seed}: {panels} panels, want 1..=4"
                        );
                    }
                    DoorKind::Automatic { delay } => {
                        assert!(
                            door.hinges().is_empty(),
                            "seed {seed}: automatic: no hinges"
                        );
                        let panels = door.panels().len();
                        assert!(
                            (3..=6).contains(&panels),
                            "seed {seed}: {panels} panels, want 3..=6"
                        );
                        assert_eq!(delay, AUTO_CLOSE_DELAY, "seed {seed}: the [START] delay");
                    }
                }
                let total = door.cells().count();
                assert!(
                    (3..=6).contains(&total),
                    "seed {seed}: {total} cells, want a 3..=6 span"
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

    /// §10.4/#147: generation produces *both* door kinds — most manual, a minority
    /// automatic (the [`AUTO_DOOR_PERCENT`] share) — and the split is deterministic
    /// per seed (§12.4). Asserted in aggregate so the distribution, not one door, is
    /// what's pinned.
    #[test]
    fn generation_produces_both_door_kinds_deterministically() {
        assert_eq!(AUTO_DOOR_PERCENT, 30, "the [START] automatic-door share");
        let (mut manual, mut automatic) = (0u32, 0u32);
        for seed in seed_sweep(200) {
            let a = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let b = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let kinds = |l: &Layout| -> Vec<bool> {
                l.regions().doors().map(|(_, d)| d.is_automatic()).collect()
            };
            assert_eq!(
                kinds(&a),
                kinds(&b),
                "seed {seed}: door kinds are deterministic"
            );
            for (_, door) in a.regions().doors() {
                if door.is_automatic() {
                    automatic += 1;
                } else {
                    manual += 1;
                }
            }
        }
        assert!(automatic > 0, "some doors are automatic");
        assert!(manual > automatic, "but most doors are manual");
    }

    /// #145: in a *placed* level a deterministic share of doorways starts open, and
    /// the graph pose and panel terrain are stamped together — an open door reads
    /// `DoorPanelOpen`, a closed one `DoorPanelClosed`, never a mismatch, and hinges
    /// stay solid whatever the pose (§10.4). Same seed → the same open doors (§12.4).
    #[test]
    fn some_doors_start_open_deterministically_and_stamped_together() {
        for seed in seed_sweep(64) {
            let (a, _) = generate_level(&LevelConfig::V1, &mut Rng::new(seed)).unwrap();
            let (b, _) = generate_level(&LevelConfig::V1, &mut Rng::new(seed)).unwrap();

            // Determinism: the same seed opens exactly the same doors.
            let poses_a: Vec<bool> = a.regions().doors().map(|(_, d)| d.is_open()).collect();
            let poses_b: Vec<bool> = b.regions().doors().map(|(_, d)| d.is_open()).collect();
            assert_eq!(
                poses_a, poses_b,
                "seed {seed}: open set is not deterministic"
            );

            // Graph pose and grid terrain agree, cell for cell.
            for (_, door) in a.regions().doors() {
                let want = if door.is_open() {
                    Terrain::DoorPanelOpen
                } else {
                    Terrain::DoorPanelClosed
                };
                for &p in door.panels() {
                    assert_eq!(
                        a.facility().terrain(p),
                        Some(want),
                        "seed {seed}: door pose and panel terrain disagree at {p:?}",
                    );
                }
                for &h in door.hinges() {
                    assert_eq!(
                        a.facility().terrain(h),
                        Some(Terrain::DoorHinge),
                        "seed {seed}: a hinge is not solid",
                    );
                }
            }
        }
    }

    /// #145: a named [START] fraction (~20%) of doorways starts open — reliably some
    /// open and some closed, in the neighbourhood of [`OPEN_DOOR_PERCENT`]. The knob
    /// itself is pinned so a retune is a visible decision, not a silent drift.
    #[test]
    fn about_a_fifth_of_doors_start_open() {
        assert_eq!(
            OPEN_DOOR_PERCENT, 20,
            "the [START] open-door share is pinned"
        );

        let (mut open, mut total) = (0u32, 0u32);
        for seed in seed_sweep(128) {
            let (layout, _) = generate_level(&LevelConfig::V1, &mut Rng::new(seed)).unwrap();
            for (_, door) in layout.regions().doors() {
                total += 1;
                open += u32::from(door.is_open());
            }
        }
        assert!(total > 0, "the sweep generated no doors");
        assert!(open > 0, "no door ever started open across the sweep");
        assert!(open < total, "every door started open across the sweep");
        let frac = f64::from(open) / f64::from(total);
        assert!(
            (0.08..0.36).contains(&frac),
            "open share {frac:.3} strays far from the ~20% [START] target ({open}/{total})",
        );
    }

    /// The bare (unplaced) carve is still all-closed — opening is a placement-time
    /// state layer (#145), so `generate` stays the canonical closed-door primitive
    /// the door-mechanics tests build on. (The placed path opens doors; see above.)
    #[test]
    fn the_bare_carve_leaves_every_door_closed() {
        for seed in seed_sweep(64) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            for (_, door) in layout.regions().doors() {
                assert!(!door.is_open(), "seed {seed}: a bare carve opened a door");
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

    /// The cover cells of a layout, grouped into 4-connected clusters — returns
    /// `(total tables, cluster count, largest cluster)`. A lone stamp is a
    /// one-cell cluster; a bench is a multi-cell one.
    fn cover_clustering(layout: &Layout) -> (u32, u32, u32) {
        let f = layout.facility();
        let mut seen: HashSet<Cell> = HashSet::new();
        let (mut tables, mut clusters, mut largest) = (0u32, 0u32, 0u32);
        for y in 0..f.height() {
            for x in 0..f.width() {
                let c = Cell::new(x, y);
                if f.terrain(c) != Some(Terrain::PartialCover) {
                    continue;
                }
                tables += 1;
                if !seen.insert(c) {
                    continue;
                }
                clusters += 1;
                let (mut size, mut stack) = (0u32, vec![c]);
                while let Some(p) = stack.pop() {
                    size += 1;
                    for nb in f.neighbours(p) {
                        if f.terrain(nb) == Some(Terrain::PartialCover) && seen.insert(nb) {
                            stack.push(nb);
                        }
                    }
                }
                largest = largest.max(size);
            }
        }
        (tables, clusters, largest)
    }

    /// #74: the §10.1a repair used to drop a lone `π` per over-long run — scattered
    /// confetti. Now each placement extends into a **bench** across the space, so the
    /// same cover reads as far fewer, organized pieces. Asserted in aggregate: distinct
    /// cover clusters are markedly fewer than cover cells (benches formed), and a
    /// multi-cell bench genuinely appears.
    #[test]
    fn cover_clusters_into_benches() {
        let seeds = seed_sweep(200);
        let (mut tables, mut clusters, mut largest) = (0u32, 0u32, 0u32);
        for &seed in &seeds {
            let (t, c, l) = cover_clustering(&generate(40, 40, &mut Rng::new(seed)).unwrap());
            tables += t;
            clusters += c;
            largest = largest.max(l);
        }
        assert!(tables > 0, "no cover placed at all");
        assert!(
            clusters * 10 < tables * 9,
            "cover barely clusters: {clusters} clusters over {tables} cells — benches are not forming"
        );
        assert!(
            largest >= 2,
            "no bench longer than a single cell formed over the sweep"
        );
    }

    /// The bench-length knobs stay sane §10.1a **[START]** values — a bench is at
    /// least two cells (a lone table is the confetti the mechanism exists to
    /// kill), and not so long it walls a wide space (`COVER_BENCH_MAX`).
    #[test]
    fn the_bench_cap_is_a_sane_start_value() {
        assert!(
            (2..=6).contains(&COVER_BENCH_MAX),
            "COVER_BENCH_MAX {COVER_BENCH_MAX} left the sane 2..=6 range"
        );
        assert!(
            (2..=COVER_BENCH_MAX).contains(&COVER_BENCH_MIN),
            "COVER_BENCH_MIN {COVER_BENCH_MIN} left the 2..=COVER_BENCH_MAX range"
        );
    }

    /// The bench rules as a per-level property (§10.1.6): every stamped table
    /// belongs to a **bench** — a straight row of [`COVER_BENCH_MIN`]..=
    /// [`COVER_BENCH_MAX`] cells, never a lone cell — every bench is **room
    /// furniture** (each cell borders room floor and never corridor floor), and
    /// every bench sits in a furniture pose ([`bench_pose`]): free-standing,
    /// end-on to a wall, or flush along one.
    #[test]
    fn benches_are_room_furniture_in_furniture_poses() {
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let (f, g) = (layout.facility(), layout.regions());
            let mut seen: HashSet<Cell> = HashSet::new();
            for y in 0..f.height() {
                for x in 0..f.width() {
                    let c = Cell::new(x, y);
                    if f.terrain(c) != Some(Terrain::PartialCover) || !seen.insert(c) {
                        continue;
                    }
                    let mut bench = vec![c];
                    let mut stack = vec![c];
                    while let Some(p) = stack.pop() {
                        for nb in f.neighbours(p) {
                            if f.terrain(nb) == Some(Terrain::PartialCover) && seen.insert(nb) {
                                bench.push(nb);
                                stack.push(nb);
                            }
                        }
                    }

                    let len = bench.len() as u32;
                    assert!(
                        (COVER_BENCH_MIN..=COVER_BENCH_MAX).contains(&len),
                        "seed {seed}: bench at {c:?} has {len} cells"
                    );
                    assert!(
                        bench.iter().all(|p| p.x == c.x) || bench.iter().all(|p| p.y == c.y),
                        "seed {seed}: bench at {c:?} is not one straight row"
                    );
                    assert!(
                        bench_pose(f, &bench).is_some(),
                        "seed {seed}: bench at {c:?} sits in no furniture pose"
                    );
                    // Room furniture: the bench opens onto room floor somewhere
                    // (an along-wall piece slotted into a niche may have interior
                    // cells touching only walls), and no cell of it ever borders
                    // corridor floor.
                    let kinds: Vec<RegionKind> = bench
                        .iter()
                        .flat_map(|&p| f.neighbours(p))
                        .filter_map(|n| g.region_at(n).map(|id| g.kind(id)))
                        .collect();
                    assert!(
                        kinds.contains(&RegionKind::Room),
                        "seed {seed}: bench at {c:?} borders no room floor"
                    );
                    assert!(
                        !kinds.contains(&RegionKind::Corridor),
                        "seed {seed}: bench at {c:?} borders a corridor — tables are room furniture"
                    );
                }
            }
        }
    }

    /// The number of floor cells flanked by two or more tables in a layout — the
    /// §11.4 doubled-crouch clutter the cover pass tries to avoid (#75).
    fn doubled_crouch_cells(layout: &Layout) -> u32 {
        let f = layout.facility();
        let mut doubles = 0;
        for y in 0..f.height() {
            for x in 0..f.width() {
                let c = Cell::new(x, y);
                if f.terrain(c) == Some(Terrain::Floor)
                    && f.neighbours(c)
                        .filter(|&n| f.terrain(n) == Some(Terrain::PartialCover))
                        .count()
                        >= 2
                {
                    doubles += 1;
                }
            }
        }
        doubles
    }

    /// #75: two tables flanking one floor cell put the *same* `crouch` hint on it
    /// twice (§11.4). Both the bench seed and its extension steer around that, so
    /// doubled-crouch cells stay rare — the residual (~0.13/level over the full sweep)
    /// is forced seeds and the odd spot where a bench meets a crossing one. This pins
    /// the preference is working, not that it is a hard guarantee (§11.4 keeps it a
    /// preference — the arrow disambiguates any survivor); the gross-regression class
    /// (a bench that over-covers into a haze of doubles) ran ~15/level and is caught.
    #[test]
    fn cover_rarely_doubles_the_crouch_hint() {
        let seeds = seed_sweep(1000);
        let doubles: u32 = seeds
            .iter()
            .map(|&seed| doubled_crouch_cells(&generate(40, 40, &mut Rng::new(seed)).unwrap()))
            .sum();
        // Ceiling of 0.3 doubled cells per level — ~2× the measured rate, floored so a
        // thin fast-mode sample never flakes, and far under the ~15/level regression class.
        let budget = (seeds.len() as u32 * 3 / 10).max(3);
        assert!(
            doubles <= budget,
            "{doubles} doubled-crouch cells over {} seeds (budget {budget}) — the §11.4 table preference has degraded",
            seeds.len()
        );
    }

    /// [`creates_table_double`] fires only when a candidate table would share a floor
    /// neighbour with an existing one — the exact table+table adjacency #75 avoids,
    /// and nothing else (a lone table, or one across the room, is fine).
    #[test]
    fn a_table_double_needs_a_shared_floor_neighbour() {
        let mut f = Facility::walled_box(8, 8);
        f.set_terrain(3, 3, Terrain::PartialCover);
        // (3,5)'s north neighbour (3,4) is floor and already borders the (3,3) table.
        assert!(creates_table_double(&f, Cell::new(3, 5)));
        // A candidate sharing no floor neighbour with any table does not double.
        assert!(!creates_table_double(&f, Cell::new(6, 6)));
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

    /// Every hideout is a **flush recess** (§10.1.6): a wall-line cell with **exactly
    /// one floor neighbour** — the mouth the player bumps it from — and **three solid
    /// wall neighbours**. The three walls are the safety guarantee: the cupboard is
    /// backed and flanked, so it can be neither walked nor seen *through* to the far
    /// side, and it never clogs a door throat (a door cell on a flank would break the
    /// exactly-three-wall count). This is the geometry the thicken pass and the pillars
    /// manufacture.
    #[test]
    fn every_hideout_is_a_flush_recess() {
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let f = layout.facility();
            for c in hideout_cells(&layout) {
                let neighbours: Vec<Terrain> =
                    f.neighbours(c).filter_map(|n| f.terrain(n)).collect();
                assert_eq!(
                    neighbours.len(),
                    4,
                    "seed {seed}: hideout {c:?} is on the border, not an interior recess"
                );
                let floors = neighbours.iter().filter(|&&t| t == Terrain::Floor).count();
                let walls = neighbours.iter().filter(|&&t| t == Terrain::Wall).count();
                assert_eq!(
                    (floors, walls),
                    (1, 3),
                    "seed {seed}: hideout {c:?} is not a flush recess (1 floor mouth + 3 wall), \
                     neighbours {neighbours:?}"
                );
            }
        }
    }

    /// A corridor-facing cupboard is proof the thicken pass (§10.1.5) did structural
    /// work: a bare corridor flank is one cell thick, so its wall cell has a corridor
    /// floor on one side *and a room floor on the other* — two floor neighbours, which
    /// [`recess_site`] rejects. A cupboard can open onto a corridor **only** where the
    /// flank was thickened to two, giving the wall cell a solid back. So a corridor
    /// hideout, backed by wall, with room floor two steps in, could not exist without
    /// the pass. Asserted in aggregate over the sweep.
    #[test]
    fn corridor_cupboards_require_a_thickened_wall() {
        let mut found = 0;
        for seed in seed_sweep(200) {
            let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
            let (f, regions) = (layout.facility(), layout.regions());
            for c in hideout_cells(&layout) {
                let opens_on_corridor = regions
                    .region_at(c)
                    .is_some_and(|id| regions.kind(id) == RegionKind::Corridor);
                if !opens_on_corridor {
                    continue;
                }
                // The mouth is the sole floor neighbour; the backing is opposite it.
                let mouth = f
                    .neighbours(c)
                    .find(|&n| f.terrain(n) == Some(Terrain::Floor))
                    .unwrap();
                let (dx, dy) = (c.x as i32 - mouth.x as i32, c.y as i32 - mouth.y as i32);
                let backing = Cell::new((c.x as i32 + dx) as u32, (c.y as i32 + dy) as u32);
                assert_eq!(
                    f.terrain(backing),
                    Some(Terrain::Wall),
                    "seed {seed}: corridor cupboard {c:?} is not solidly backed"
                );
                found += 1;
            }
        }
        assert!(
            found > 0,
            "no corridor cupboards over the sweep — the thicken pass is not producing backing"
        );
    }

    /// Cupboards are spread, not banked. The [`place_hideouts`] pass enforces the
    /// spacing knobs outright; the §10.1a corridor repair ([`recess_run_hideout`])
    /// treats them as a preference — a flight path's run must break even where the
    /// only site is close to an existing cupboard (§10.1a: "a flight path with no
    /// hideout on it is a failed flight path"). Two properties survive that:
    ///
    /// - a **structural floor** — no two hideouts within Manhattan 2 of each
    ///   other, which the [`recess_site`] three-solid-walls geometry makes
    ///   impossible (a hideout flanking the candidate fails the wall count), so a
    ///   cupboard's backing is never itself hollowed out;
    /// - **statistically spread** — pairs closer than the corridor spacing stay a
    ///   small fraction of all pairs (measured ~1.7% when the repair landed;
    ///   budgeted at 4%), so the board never rots into a honeycomb.
    #[test]
    fn hideouts_keep_their_spacing() {
        let (mut pairs, mut close) = (0u64, 0u64);
        for seed in seed_sweep(200) {
            let cells = hideout_cells(&generate(40, 40, &mut Rng::new(seed)).unwrap());
            for (i, &a) in cells.iter().enumerate() {
                for &b in &cells[i + 1..] {
                    let d = a.manhattan_distance(b);
                    assert!(
                        d >= 2,
                        "seed {seed}: hideouts {a:?} and {b:?} share backing"
                    );
                    pairs += 1;
                    close += u64::from(d < HIDEOUT_MIN_SPACING_CORRIDOR);
                }
            }
        }
        assert!(
            close * 25 <= pairs,
            "{close}/{pairs} hideout pairs closer than the corridor spacing — the board is banking up"
        );
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
            let layout = generate_once(40, 40, &mut Rng::new(seed), &Tuning::BIASED).unwrap();
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
    /// counting consecutive cells that neither block sight, nor provide cover,
    /// nor have a cupboard within two moves — the §10.1a measure (a table is
    /// see-through but plants the crouch; a mouth is see-past but a bump from
    /// vanishing — both are the counterplay the rule demands).
    fn longest_straight_run(f: &Facility) -> u32 {
        let (w, h) = (f.width(), f.height());
        let mouth = |c: Cell| {
            f.neighbours(c)
                .any(|n| f.terrain(n) == Some(Terrain::Hideout))
        };
        let clear = |x: u32, y: u32| {
            let c = Cell::new(x, y);
            f.terrain_at(x, y)
                .is_some_and(|t| !t.blocks_sight() && !t.provides_cover())
                && !mouth(c)
                && !f
                    .neighbours(c)
                    .any(|n| f.terrain(n) == Some(Terrain::Floor) && mouth(n))
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

    /// In a **room**, the §10.1a repair is **furniture, not wall** (#52): the
    /// pass stamps tables and only tables, so a room blocker never reads as a
    /// floating wall cell. Driven on a bare gallery where every stamp must come
    /// from the cover pass — and the crouch trade is visible in the terrain: the
    /// gallery satisfies the counterplay measure while staying *optically* open
    /// end to end (a guard still sees straight over every table).
    #[test]
    fn the_cover_pass_stamps_tables_not_walls_in_a_room() {
        let mut f = Facility::walled_box(30, 8);
        // Claim the interior as one room region, as the real partition would:
        // the pass releases each stamped cell, and only owned cells release.
        let mut regions = RegionGraph::new(30, 8);
        regions.add_region(RegionKind::Room, Rect::new(1, 1, 28, 6).cells());
        break_sightlines(&mut f, &mut regions, &mut Rng::new(7), &Tuning::BIASED);

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

    /// In a **corridor**, the §10.1a repair is architecture, never furniture
    /// (§10.1.6): the pass recesses cupboards or raises structural pillars, and
    /// no table ever lands. Driven on the same bare gallery claimed as a corridor
    /// — walled 1-thick all round, so the first repairs must be pillars (no recess
    /// backing exists yet; a later repair may then recess into a pillar it built).
    #[test]
    fn the_cover_pass_never_stamps_a_table_in_a_corridor() {
        let mut f = Facility::walled_box(30, 8);
        let mut regions = RegionGraph::new(30, 8);
        regions.add_region(RegionKind::Corridor, Rect::new(1, 1, 28, 6).cells());
        break_sightlines(&mut f, &mut regions, &mut Rng::new(7), &Tuning::BIASED);

        assert!(sightlines_bounded(&f), "the gallery must be repaired");
        let mut repairs = 0;
        for y in 1..f.height() - 1 {
            for x in 1..f.width() - 1 {
                match f.terrain_at(x, y) {
                    Some(Terrain::Floor) => {}
                    // Pillar wall, or a cupboard recessed into a pillar's backing —
                    // both architecture. What must never appear is a table.
                    Some(Terrain::Wall | Terrain::Hideout) => repairs += 1,
                    t => panic!("({x},{y}): a corridor repair stamped {t:?}"),
                }
            }
        }
        assert!(repairs > 0, "a 28-cell corridor cannot pass unbroken");
    }

    /// The repair must stay a repair: [`break_sightlines`] satisfies §10.1a on
    /// nearly every *raw* carve, with the §10.6 rejection reserved for genuinely
    /// cornered geometry. Without this pin, the pass could silently rot into
    /// "reject and redraw until lucky" and nothing above would notice — measured
    /// at 1-in-1000 on the v1 config when the pass stamped tables anywhere; the
    /// region-dispatched repair (no tables in corridors, benches of 2+ in
    /// furniture poses) is a strictly harder constraint set, re-measured at 2%
    /// (the residue is room lanes boxed in by earlier furniture, where any table
    /// would sever pathing), budgeted at 4% here.
    #[test]
    fn the_cover_pass_repairs_almost_every_carve() {
        // Budget is the 4% rate scaled to the sweep width, floored at 1 so a single
        // unlucky sampled seed never flakes; the full CI sweep restores the 8/200 pin.
        let seeds = seed_sweep(200);
        let budget = (8 * seeds.len() / 200).max(1);
        let unrepaired = seeds
            .iter()
            .filter(|&&seed| {
                let layout = generate_once(40, 40, &mut Rng::new(seed), &Tuning::BIASED).unwrap();
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
        let err = generate_where(40, 40, &mut Rng::new(0), |_| false, &Tuning::BIASED).unwrap_err();
        assert_eq!(
            err,
            GenError::RetriesExhausted {
                attempts: MAX_GEN_ATTEMPTS
            }
        );
    }
}
