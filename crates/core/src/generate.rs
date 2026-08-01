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
use crate::modifiers::LevelModifiers;
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
/// How long an automatic door stays open after its doorway is last vacated
/// (§10.4/#147, **[START] = 5** turns): short but nonzero, so a guard passing through
/// leaves a real slip-through window before the door shuts (the ticket's stealth knob)
/// — a touch longer than the original 3 to widen that window (§10.4).
const AUTO_CLOSE_DELAY: u32 = 5;
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
/// as a pilaster/buttress rather than a bare partition. Not every wall — thickening
/// them all turns the facility fortress-thick; the value is the single named knob.
/// `1` here would thicken every eligible run, higher numbers fewer.
///
/// Raised from a third to a half with #361, which made cupboards demand **fully**
/// backed sites (diagonals included, [`recess_site`]) and so retired a whole class of
/// harvested ones: the T-junction of two one-thick walls, whose back diagonals are the
/// rooms either side — the peephole itself. Manufactured backing is what replaces
/// them (§10.1.6: "the backing is **manufactured** by step 5a"), so the knob has to
/// carry more of the board. Measured over 200 seeds of the 40×40 config: cupboards per
/// level 18.1 → 21.7, and large corridors carrying one 81% → 88% (29.6 and 98% before
/// the fix, on sites that leaked).
const WALL_THICKEN_ONE_IN: u32 = 2;

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
    /// Where the facility's **comms console** goes (§7.3/§7.7), recorded rather than
    /// stamped — like [`ducts`](Self::ducts), a fact about the level the grid does not
    /// carry yet. The cell becomes [`Terrain::CommsConsole`] in
    /// [`State::new`](crate::State::new), alongside the intel consoles and the exit,
    /// which is what keeps the carve this type hands back **bare**: guard beats (§10.5)
    /// and the §10.6 floods are computed on the grid before any solid usable lands on
    /// it, and the comms console must not be the one exception to that. `None` on a
    /// hand-built fixture.
    comms_console: Option<Cell>,
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

    /// Where the comms console goes (§7.3/§7.7), or `None` on a facility without one.
    /// Read once by [`State::new`](crate::State::new), which stamps it.
    pub fn comms_console(&self) -> Option<Cell> {
        self.comms_console
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

    /// Claim a formerly-solid cell for `region` — the graph half of a mid-level
    /// terrain change that makes a wall walkable (§10.5).
    ///
    /// Generation reshapes regions as it carves; this is the one path that does so
    /// *during a run*, for Pierce Wall (§8.3/#303): the bored cell becomes floor, and
    /// a walkable cell must belong to exactly one region or the invariant §10.5 rests
    /// on breaks. It is the same claim a recessed cupboard makes
    /// ([`RegionGraph::add_cell`]), reached through the [`Layout`] so the grid and the
    /// graph are moved by the same owner.
    pub(crate) fn claim_cell(&mut self, region: RegionId, cell: Cell) {
        self.regions.add_cell(region, cell);
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
            comms_console: None,
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
            comms_console: None,
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
    // The **baseline** facility (§12.6): every door manual and hinged. This is the
    // bare-carve entry the §10.6 guarantee tests measure over; a run with modifiers
    // comes through [`generate_level`], which threads them.
    generate_where(
        width,
        height,
        rng,
        passes_guarantees,
        &Tuning::BIASED,
        &LevelModifiers::default(),
    )
}

/// Generate a *placed* level: a carve passing every §10.6 guarantee **and** a
/// [`Placement`] honouring §10.1 steps 7–9 with the spacing guarantees — exact
/// piece counts, a safe starting area, spread intel, and post-placement
/// solvability (start → every objective → the comms console → exit).
///
/// The returned layout **records** where the **comms console** goes
/// ([`Layout::comms_console`], §7.3/§7.7) without stamping it, exactly as it records a
/// duct's path. [`State::new`](crate::State::new) does the stamping, with the intel
/// consoles and the exit, for two reasons: the carve handed back stays **bare**, so
/// guard beats (§10.5) and the §10.6 floods are still computed on a grid with no solid
/// usable on it; and no boot path has to remember the console, since every one of them
/// goes through [`State::new`](crate::State::new).
///
/// This is the entry point real levels come from. Carve rejection (#13) and
/// placement rejection (#12) share this one seed-retry loop, as §10.6 asks: a
/// carve whose geometry cannot seat the pieces is redrawn exactly like a carve
/// that sealed a room, from the same `rng` stream (§12.4 — same seed, same level,
/// forever), and a config that can never place errors out loudly with
/// [`GenError::RetriesExhausted`] instead of shipping a silent shortfall.
pub fn generate_level(
    config: &LevelConfig,
    modifiers: &LevelModifiers,
    rng: &mut Rng,
) -> Result<(Layout, Placement), GenError> {
    // The §12.6 **guard-count knob** (#232) is resolved into the recipe here, once,
    // before a piece is placed — the same discipline `automatic_doors` follows below,
    // and for the same reason: a generation-time modifier is threaded in as a
    // parameter, never consulted from a global, or §12.4's determinism would be a
    // claim nobody could check. It reaches *placement* rather than the carve, so the
    // three settings still carve one building from one seed (see the field's note).
    let config = &config.with_guard_count(modifiers.guard_count);
    for _ in 0..MAX_GEN_ATTEMPTS {
        let mut layout =
            generate_once(config.width, config.height, rng, &Tuning::BIASED, modifiers)?;
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
            // Record where the comms console goes; `State::new` stamps it, with the
            // other solid usables — see the doc comment above.
            layout.comms_console = Some(placement.comms());
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
    modifiers: &LevelModifiers,
) -> Result<Layout, GenError> {
    for _ in 0..MAX_GEN_ATTEMPTS {
        let layout = generate_once(width, height, rng, tuning, modifiers)?;
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
    modifiers: &LevelModifiers,
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
    place_doorways(&mut facility, &mut regions, rng, modifiers.automatic_doors);

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
        // Placement decides where the comms console goes, so it is recorded on the
        // finished carve by `generate_level`, not here (§7.3/§7.7).
        comms_console: None,
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
pub(super) fn creates_usable_conflict(facility: &Facility, cell: Cell) -> bool {
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
mod tests;
