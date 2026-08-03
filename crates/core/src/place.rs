//! Entity placement — §10.1 steps 7–9, with the §10.6 spacing guarantees.
//!
//! Generation so far carves the board (#7–#11); this module puts the pieces on it:
//! the exit `E` and the tunnel behind it, the intel consoles, the comms console, and
//! the guards. The rules are §10.1's — the exit in the **largest room**, objectives and
//! guards in any room *except* the start room — plus the spacing guarantees the old
//! generator entirely lacked (§10.6): the intel never clumps into one room, the comms
//! console is a real detour away (§7.7), and no guard's turn-one cone covers the mouth
//! the player comes up out of. *"The starting area should be safe" — make it so.*
//!
//! **The player is not placed any more** (§4.5/#466). They start inside their own
//! tunnel, on its way-out cell at the level border, and crawl in — so what placement
//! chooses is the exit, and the spawn falls out of it
//! ([`carve_exit_duct`](crate::generate::carve_exit_duct)). The old
//! `PLAYER_EXIT_MIN_DISTANCE` — eight cells between spawn and exit, so that no run
//! started won — retires with it: the distance is the tunnel's own length now, and it is
//! floored in the carve (`EXIT_DUCT_MIN_CELLS`).
//!
//! Two lessons from the old generator shape the module:
//!
//! - **Placement must not fail silently** (§10.6). Guards were quietly dropped
//!   (asked 5, got 4); objectives threw after 100 tries. Here a draw either places
//!   the **exact** requested counts or returns `None`, and the caller
//!   ([`generate_level`](crate::generate::generate_level)) rejects the carve and
//!   redraws from the same seed stream — the same loop that rejects a sealed or
//!   over-sighted carve, so "reject the seed" is one mechanism, not three.
//! - **Solvability is asserted after the pieces land, not before.** The §10.6 gate
//!   (#13) proves the *empty* carve is one pathable component — but consoles and
//!   the exit stamp in as solid (§10.3), and a console dropped into a 1-cell
//!   squeeze could pinch the player's only route. So placement re-floods the
//!   player's actual movement graph and requires every console and the exit to be
//!   bump-adjacent (§4.3) to it: start → every objective → the comms console → exit,
//!   on the level as it will actually be played.

use crate::beat::coordinated_beat_cells;
use crate::cell::Cell;
use crate::duct::Duct;
use crate::facility::{Facility, Terrain};
use crate::generate::{carve_exit_duct, has_adjacent_usable, shuffle, Layout};
use crate::guard::{Guard, GUARD_INITIAL_FACING};
use crate::modifiers::GuardCount;
use crate::path;
use crate::radio::RadioClock;
use crate::region::{RegionId, RegionKind};
use crate::rng::Rng;
use crate::vision::{field_of_view_with_blind_spot, BlindTier, GUARD_SIGHT_ARC, GUARD_SIGHT_RANGE};
use std::collections::HashSet;

/// The **comms console** spawns at least this far (Manhattan) from the exit `E`
/// **[START]** — the cell the player climbs out of the tunnel into, and so the point
/// their run through the facility starts and ends at (§4.5/#466).
///
/// The console silences the radio net for the whole level (§7.3/§7.7), which is the
/// only cost a permanent takedown carries — so a console found in the first few turns
/// would make every later takedown free. Distance is the balance knob the counterplay
/// hangs on: far enough that reaching it is a deliberate detour costing turns and
/// exposure (§2.3 — cost is load-bearing), not a switch on the way out of the start
/// room. On the 40×40 v1 footprint it still leaves most non-start rooms eligible, and a
/// draw with no far-enough cell is rejected and redrawn like any other §10.6 shortfall.
///
/// It used to be measured from a separate player spawn, and to be asserted larger than
/// the spawn-to-exit floor `PLAYER_EXIT_MIN_DISTANCE` — "reaching the radio must cost
/// more than reaching the way out". Both are gone with #466: the player now *starts* at
/// the way out, inside their own tunnel, so there is no spawn-to-exit distance for
/// placement to choose (the tunnel's own length is it, `EXIT_DUCT_MIN_CELLS`) and
/// nothing left for the relation to compare against.
const PLAYER_COMMS_MIN_DISTANCE: u32 = 16;

// The shipped recipe sits **strictly inside** the guard-count envelope (#232), so
// both ends of the knob always bite on the level the game actually ships. Held at
// compile time rather than in a test: retuning §10.2's [START] count to an edge of
// the envelope would leave one end of a shipped modifier silently doing nothing —
// §2.3's facade, arrived at by moving a number somewhere else entirely.
const _: () = assert!(LevelConfig::GUARDS_MIN < LevelConfig::V1.guards);
const _: () = assert!(LevelConfig::V1.guards < LevelConfig::GUARDS_MAX);

/// A level recipe: the footprint and the piece counts (§10.2). v1 ships exactly
/// one tuned configuration — [`LevelConfig::V1`] — but the knobs are data so the
/// sim (§13) can sweep them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LevelConfig {
    /// Facility width in cells.
    pub width: u32,
    /// Facility height in cells.
    pub height: u32,
    /// How many guards to place — exactly this many, or the seed is rejected.
    pub guards: usize,
    /// How many intel consoles to place — exactly this many, or the seed is
    /// rejected. The v1 exit rule is *all intel required* (§10.2).
    pub intel: usize,
}

impl LevelConfig {
    /// The v1 configuration (§10.2): 40×40, 4 guards, 3 intel **[START]**.
    ///
    /// The guard count is tuned against the **bare** headless baseline (§13.2): the
    /// sim bot holds no salvaged tech, because a level must be winnable with none
    /// (§8.3). Over 300 seeds the `--guards` sweep read 3 → 48%, 4 → 37%, 5 → 29%,
    /// 6 → 21%; 4 is the forgiving-but-real end of that curve, and the only row where
    /// every run resolved (no timeouts). Quick play's partial tech grant (#244)
    /// then lands as upside on top, never as the thing that makes a run possible.
    pub const V1: Self = Self {
        width: 40,
        height: 40,
        guards: 4,
        intel: 3,
    };

    /// The fewest guards the [`GuardCount`] modifier may leave a facility with
    /// (§10.2/§10.6/#232).
    ///
    /// **Three, and the floor is about the game rather than about placement.** A carve
    /// seats fewer guards more easily, not less, so nothing here would *fail* at one or
    /// at none — it would simply hand back a building with nobody in it, which is a
    /// walk rather than a raid. Three is the last row of the `--guards` sweep that is
    /// still a real level: a bare bot wins 48% of them against 37% at the baseline
    /// (appendix 26), so it is one step of relief and not a different game.
    pub const GUARDS_MIN: usize = 3;

    /// The most guards the [`GuardCount`] modifier may add (§10.2/§10.6/#232).
    ///
    /// Five, one over the §10.2 baseline. Two things bound it and both point the same
    /// way: the board is **screen-bound** (§11.4/§10.2), so a 40×40 crowds long before
    /// the placement pool runs dry; and the §7.5 region partition divides the facility
    /// into as many beats as there are guards (§10.5), so each extra guard cuts every
    /// territory smaller — the §7.6 trap the design warns about is reached by adding
    /// guards, not by any one of them being cleverer.
    pub const GUARDS_MAX: usize = 5;

    /// This recipe with the §12.6 **guard-count knob** applied (#232) — the effective
    /// config [`generate_level`](crate::generate_level) places from.
    ///
    /// **A step, not a setter.** The knob moves the recipe's own count by one and
    /// stops at the [`GUARDS_MIN`](Self::GUARDS_MIN)…[`GUARDS_MAX`](Self::GUARDS_MAX)
    /// envelope; a recipe already outside it is **left where it is** rather than
    /// dragged into range, because clamping a sim sweep of seven guards down to five
    /// would have "one more" quietly place *two fewer* — a knob that moved the wrong
    /// way would be worse than one that did nothing. For the one shipped recipe
    /// ([`V1`](Self::V1), four guards) both ends always bite.
    #[must_use]
    pub const fn with_guard_count(self, knob: GuardCount) -> Self {
        let guards = match knob {
            GuardCount::Baseline => self.guards,
            GuardCount::Fewer if self.guards > Self::GUARDS_MIN => self.guards - 1,
            GuardCount::More if self.guards < Self::GUARDS_MAX => self.guards + 1,
            // Already at or past the envelope's edge in the direction asked for.
            GuardCount::Fewer | GuardCount::More => self.guards,
        };
        Self { guards, ..self }
    }
}

/// Where everything starts: the output of §10.1 steps 7–9 — the cell each piece
/// spawns on. The player, exit and intel are cells a caller feeds straight into
/// [`State::new`](crate::State::new); the guards become live actors through
/// [`guards`](Self::guards), which spawns the patrolling §7.5 sweepers a real run
/// uses. Keeping that one constructor here is deliberate: "a placed guard patrols"
/// is a fact about the game, not about any one shell, so the web build and the sim
/// spawn the *same* guard rather than each re-deciding.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Placement {
    player: Cell,
    exit: Cell,
    /// The player's own tunnel (§4.5/§10.7/#466): the crawlspace from the exit `E` out
    /// to the level border. Pure geometry — it stamps nothing — so it is computed here,
    /// where `E` is chosen, and recorded on the layout by
    /// [`generate_level`](crate::generate_level). [`player`](Self::player) is its
    /// way-out cell: the run *begins* inside it.
    exit_duct: Duct,
    intel: Vec<Cell>,
    /// The facility's one comms console (§7.3/§7.7) — the radio terminal a bump
    /// silences. Exactly one per facility: "one interaction shuts the whole net" is
    /// the design, so a second would only be a second switch for a net already dead.
    comms: Cell,
    guards: Vec<Cell>,
    /// Each guard's radio ping cadence (§7.3), parallel to `guards` and drawn from
    /// the run seed in [`place`] so the whole ping schedule is deterministic
    /// (§12.4). Carried here rather than derived in [`guards`](Self::guards) so it
    /// comes off the real seed stream, not a fresh source (§7.3/§12.4 anti-pattern).
    guard_clocks: Vec<RadioClock>,
}

impl Placement {
    /// The player's spawn cell (§4.5/#466): the **way out** of their own tunnel, on the
    /// level border. The run begins inside the crawlspace, facing in — the first inputs
    /// crawl to `E` and climb out into the facility.
    pub fn player(&self) -> Cell {
        self.player
    }

    /// The exit `E` (§4.5: the run ends where it began), in the largest room — the
    /// **inner mouth** of the player's tunnel (§10.7/#466), which they climb out of at
    /// the start and back into at the end.
    pub fn exit(&self) -> Cell {
        self.exit
    }

    /// The player's own tunnel (§4.5/§10.7/#466), `E` first and the way-out cell last.
    pub fn exit_duct(&self) -> &Duct {
        &self.exit_duct
    }

    /// The intel consoles — one per room, never the start room (§10.1.8, §10.6).
    pub fn intel(&self) -> &[Cell] {
        &self.intel
    }

    /// The comms console (§7.3/§7.7) — never the start room, at least
    /// [`PLAYER_COMMS_MIN_DISTANCE`] from the spawn, and reachable by a bump like any
    /// other usable (§10.6). Bumping it kills the radio net for the level.
    pub fn comms(&self) -> Cell {
        self.comms
    }

    /// The guard spawn cells — never the start room, never eyeing the player's
    /// spawn on turn one (§10.1.9, §10.6). These are the geometric facts the
    /// placement guarantees are about; [`guards`](Self::guards) turns them into the
    /// actors a run plays.
    pub fn guard_cells(&self) -> &[Cell] {
        &self.guards
    }

    /// The guards as the live actors a real run spawns — the patrolling §7.5
    /// sweepers (§7.4's reactive states ride on the same seam), each carrying its
    /// region **beat** (§10.5, [`crate::beat`]): the region the guard stands in,
    /// grown across `layout`'s door edges, so a territory is rooms plus the
    /// corridors joining them and never straddles a wall. The beats are grown
    /// **cooperatively** ([`coordinated_beat_cells`]) so guards spawned near each
    /// other fan out to cover distinct wings rather than grinding the same ground
    /// (§7.5). At placement a guard's spawn cell *is* its live position, which is
    /// what [`coordinated_beat_cells`] anchors on. Placement
    /// records guard *cells* because the §10.6 guarantees are about where a guard
    /// *stands*; turning a spawn into a behaving guard is a single decision, and it
    /// lives here so every caller — the web build, the sim — spawns the same
    /// patrolling guard.
    pub fn guards(&self, layout: &Layout) -> Vec<Guard> {
        let beats = coordinated_beat_cells(layout.regions(), layout.facility(), self.guard_cells());
        self.guard_cells()
            .iter()
            .zip(&self.guard_clocks)
            .zip(beats)
            .map(|((&cell, &clock), beat)| {
                Guard::patrolling(cell)
                    .with_beat(beat)
                    .with_radio_clock(clock)
            })
            .collect()
    }
}

/// Place the pieces on a finished carve, or `None` if this layout cannot honour
/// the counts and spacings — in which case the caller rejects the carve entirely
/// (§10.6: fail loudly or retry the seed; never a silent shortfall).
///
/// Deterministic from `rng` (§12.4): the same layout and stream always place the
/// same board.
pub(crate) fn place(layout: &Layout, config: &LevelConfig, rng: &mut Rng) -> Option<Placement> {
    let facility = layout.facility();

    // The rooms, each with its free floor cells (a region's cell set also holds
    // hideouts and door-adjacent floor; only plain floor takes a piece).
    let rooms: Vec<(RegionId, Vec<Cell>)> = layout
        .regions()
        .regions()
        .filter(|(id, _)| layout.regions().kind(*id) == RegionKind::Room)
        .map(|(id, region)| {
            let floor: Vec<Cell> = region
                .cells()
                .iter()
                .copied()
                .filter(|&c| facility.terrain(c) == Some(Terrain::Floor))
                .collect();
            (id, floor)
        })
        .collect();

    // §10.1.7: the largest room hosts entry/exit and player. Largest by *true*
    // floor area (a pillared room is not its bounding box, §10.5); ties break on
    // scan order, so the choice is deterministic.
    let start_idx = rooms
        .iter()
        .enumerate()
        .max_by_key(|(i, (_, floor))| (floor.len(), usize::MAX - i))
        .map(|(i, _)| i)?;
    let mut start_floor = rooms[start_idx].1.clone();

    // The exit `E`, and with it the tunnel behind it (§4.5/§10.7/#466): the first
    // shuffled cell of the start room with a clean straight run to the border. There is
    // no separate player spawn to space out any more — the player starts *in* the
    // tunnel, at its far end, so the distance that used to be placement's business is
    // now the tunnel's own length (`EXIT_DUCT_MIN_CELLS`). A start room whose every
    // cell is walled in from the border (or too deep inside the building) fails the draw
    // and the carve is redrawn, like any other §10.6 shortfall.
    //
    // The exit is a usable (§11.4), so the scan prefers an `E` that keeps every floor
    // cell to one adjacent usable — a preference, not a gate: with no conflict-free
    // candidate that can host a tunnel, any tunnelled cell is taken rather than failing
    // the draw (the arrow keeps a doubled cell unambiguous).
    shuffle(&mut start_floor, rng);
    // Cells the §10.7 shortcuts already claim: the tunnel may not share one, or "which
    // duct am I crawling" would have two answers.
    let ducted: HashSet<Cell> = layout
        .ducts()
        .iter()
        .flat_map(|d| d.cells().iter().copied())
        .collect();
    // A candidate `E` needs a tunnel behind it **and** a facility in front of it: at
    // least one cell to climb out onto that is not itself part of the tunnel. The
    // interior may overlie floor (§10.7 cross-room routing), and a step onto a path cell
    // is a *crawl*, not a climb-out (§10.7's confinement) — so a mouth whose only
    // walkable neighbours are its own path is a mouth that opens onto nothing, and a run
    // that starts sealed inside its own tunnel. Rejected here, where another `E` costs
    // nothing.
    let tunnel_from = |exit: Cell| {
        let duct = carve_exit_duct(facility, exit, &ducted)?;
        let opens_onto_the_building = footholds(facility, exit, duct.cells()).next().is_some();
        opens_onto_the_building.then_some(duct)
    };
    let (exit, exit_duct) = start_floor
        .iter()
        .filter(|&&exit| !placement_conflict(layout, exit, &[]))
        .find_map(|&exit| Some((exit, tunnel_from(exit)?)))
        .or_else(|| {
            start_floor
                .iter()
                .find_map(|&exit| Some((exit, tunnel_from(exit)?)))
        })?;
    // The run begins on the tunnel's way-out cell, inside the crawlspace (§4.5/#466).
    let player = exit_duct
        .way_out()
        .expect("carve_exit_duct lays an exit-kind duct");

    // Nothing else may land on the tunnel's own cells: a console or the exit stamped on
    // one would put an interactable under the crawl, which is exactly what §10.7 forbids
    // a duct's interior to overlie. (Guards may stand on one — a guard walks straight
    // over a concealed crawler, §10.7.) The §10.7 shortcuts are held to the same rule
    // here, which nothing used to do.
    let mut taken: Vec<Cell> = vec![exit, player];
    taken.extend(exit_duct.cells().iter().copied());
    taken.extend(ducted.iter().copied());
    // The usables placed so far — what later usable picks prefer not to crowd
    // (§11.4, one usable per cell — a preference, not a gate).
    let mut usables: Vec<Cell> = vec![exit];

    // §10.1.8 + §10.6: intel in any room except the start room — and *spread*, one
    // room each, so all three can never land in one room. Rooms are drawn in
    // shuffled order; too few distinct rooms fails the draw.
    let mut others: Vec<usize> = (0..rooms.len()).filter(|&i| i != start_idx).collect();
    shuffle(&mut others, rng);
    if others.len() < config.intel {
        return None;
    }
    let mut intel = Vec::with_capacity(config.intel);
    for &i in others.iter().take(config.intel) {
        // A console is a usable (§11.4): prefer a cell that keeps every floor
        // cell to one adjacent usable, but fall back to the whole room rather
        // than fail the draw — the preference never blocks a placement (the
        // arrow keeps a doubled cell unambiguous).
        let clean: Vec<Cell> = rooms[i]
            .1
            .iter()
            .copied()
            .filter(|&c| !placement_conflict(layout, c, &usables))
            .collect();
        let console =
            pick_free(&clean, &taken, rng).or_else(|| pick_free(&rooms[i].1, &taken, rng))?;
        intel.push(console);
        taken.push(console);
        usables.push(console);
    }

    // The comms console (§7.3/§7.7): the facility's radio terminal, treated like an
    // objective for the §10.6 guarantees — a non-start room, bump-reachable (asserted
    // below with the rest), and at least `PLAYER_COMMS_MIN_DISTANCE` from the spawn so
    // the counterplay costs a real detour rather than a switch on the way out of the
    // start room. Unlike intel it does *not* claim a room of its own: it may share one
    // with an objective (a different cell), which keeps a four-room carve placeable.
    //
    // Drawn **before the guards**, and that ordering is load-bearing (#232/#466). The
    // §12.6 guard-count knob is supposed to reach the guard *set* and nothing else — the
    // three settings carve one building and place one board, so a paired A/B can
    // attribute what it measures. Drawing the console from a pool that excluded the
    // guards broke that quietly: a pool one cell smaller shuffles differently, so each
    // setting sat the console somewhere else, and a console that pinched a route failed
    // the solvability check below at one setting and not another — which redraws the
    // *carve*. Ahead of the guards, every draw before `take(n)` is knob-independent and
    // the guard sets stay strictly nested.
    let mut comms_pool: Vec<Cell> = others
        .iter()
        .flat_map(|&i| rooms[i].1.iter().copied())
        .filter(|&c| !taken.contains(&c) && c.manhattan_distance(exit) >= PLAYER_COMMS_MIN_DISTANCE)
        .collect();
    shuffle(&mut comms_pool, rng);
    // A usable like any other (§11.4): prefer a cell that leaves every floor
    // neighbour with one adjacent usable, but fall back rather than fail the draw.
    let comms = comms_pool
        .iter()
        .copied()
        .find(|&c| !placement_conflict(layout, c, &usables))
        .or_else(|| comms_pool.first().copied())?;

    // Claimed, so no guard spawns on top of the console it is standing on.
    taken.push(comms);

    // §10.1.9 + §10.6: guards in any room except the start room, and never where
    // the turn-one detection set — the real §6 field of view from the spawn cell,
    // facing south as every guard does at spawn (§7.1), with the §155 rear blind
    // spot carved out — covers the **exit `E`**. That is where the player comes up
    // (§4.5/#466): the crawl itself is concealed and contact-safe (§10.7), so "the
    // starting area should be safe" is now a claim about the mouth they climb out of
    // rather than about a cell they materialise on. This is the same function the sight
    // phase runs, not a conservative box, so it is exact: a guard
    // that only has the mouth in its rear blind spot is genuinely safe. Candidates
    // pool across all non-start rooms (guards may share a room; the intel and comms
    // cells are already taken), shuffled once; too few safe cells fails the draw —
    // asked-for-5-got-4 is precisely the old bug (§10.6).
    let mut guard_pool: Vec<Cell> = others
        .iter()
        .flat_map(|&i| rooms[i].1.iter().copied())
        .filter(|c| !taken.contains(c))
        .collect();
    shuffle(&mut guard_pool, rng);
    let guards: Vec<Cell> = guard_pool
        .into_iter()
        .filter(|&cell| {
            // **Always the shipped [`BlindTier::REAR`] carve, never a modifier's**
            // (#410). Placement runs before a level's modifiers are resolved
            // ([`start_level_with`](crate::start_level_with)), and pinning it here is
            // the better answer rather than a workaround for that: a narrower carve
            // would pass *more* cells, so an experiment that widens a guard's blind
            // spot would also shift where guards spawn — and a paired A/B whose two
            // arms generate different geometry cannot attribute what it measures.
            // The rear rule is the conservative one (a cell safe under it is safe
            // under any wider blind spot), so pinning it costs nothing but the
            // very-slightly-closer spawns a narrower cone would have allowed.
            let cone = field_of_view_with_blind_spot(
                facility,
                cell,
                GUARD_INITIAL_FACING,
                GUARD_SIGHT_ARC,
                GUARD_SIGHT_RANGE,
                BlindTier::REAR,
            );
            !cone.contains(exit)
        })
        .take(config.guards)
        .collect();
    if guards.len() < config.guards {
        return None;
    }

    // The post-placement solvability assertion: on the grid as it will actually
    // be played (consoles and exit solid), the player still reaches every
    // objective and the way out. §10.6's "assert it, don't argue it", applied
    // once more after the last pieces land. The one-usable preference (§11.4) is
    // best-effort, not asserted — the placements above honour it where they can.
    // Solvability is geometric — it does not read the radio clocks — so check it
    // on a clockless placement first, and only draw the clocks once it passes.
    let mut placement = Placement {
        player,
        exit,
        exit_duct,
        intel,
        comms,
        guards,
        guard_clocks: Vec::new(),
    };
    if !solvable(facility, &placement) {
        return None;
    }
    // Each guard's radio ping cadence (§7.3), drawn once from the run stream —
    // strictly *after* every rejection point, and after all geometry is fixed, so
    // the level a seed produces is byte-identical to before and only an accepted
    // placement consumes these draws (§12.4 determinism). One jittered period per
    // guard: the clock a takedown will later start.
    let guard_count = placement.guards.len();
    placement.guard_clocks = (0..guard_count).map(|_| RadioClock::draw(rng)).collect();
    Some(placement)
}

/// The cells the player can climb out of the mouth `exit` onto and **go somewhere**
/// (§4.5/§10.7/#466): its walkable neighbours that are neither cells of the tunnel itself
/// nor a cupboard.
///
/// Two exclusions, both about dead ends. Stepping onto a path cell is a *crawl* rather
/// than a climb-out (§10.7's confinement), so the tunnel is no way into the facility. And
/// a **cupboard** is recessed with exactly one mouth (§10.1.6) — if that mouth is `E`,
/// the only way out of it is back onto `E`, which is solid and can only be bumped: a
/// mouth opening onto nothing else would strand the player in a wardrobe on turn four.
///
/// This is what "the mouth opens onto the building" means, and the same set the
/// solvability flood starts from — the player picks whichever side they come up on.
fn footholds<'a>(
    facility: &'a Facility,
    exit: Cell,
    tunnel: &'a [Cell],
) -> impl Iterator<Item = Cell> + 'a {
    facility.neighbours(exit).filter(move |&n| {
        !tunnel.contains(&n)
            && facility.terrain(n) != Some(Terrain::Hideout)
            && facility.can_enter(n, crate::state::ACTOR_FILL)
    })
}

/// Whether placing a usable (a console, the exit) at `cell` would give some
/// floor neighbour a **second** adjacent usable — the §11.4 one-usable
/// *preference*, given the usable cells already `placed`. A neighbour that is
/// itself a placed usable does not count (it is not a standing cell).
fn placement_conflict(layout: &Layout, cell: Cell, placed: &[Cell]) -> bool {
    let facility = layout.facility();
    facility.neighbours(cell).any(|f| {
        facility.terrain(f) == Some(Terrain::Floor)
            && !placed.contains(&f)
            && has_adjacent_usable(facility, f, placed)
    })
}

/// A random cell of `floor` not already in `taken`, or `None` if the room is
/// exhausted. Draws by shuffled scan from `rng`, so it is deterministic and does
/// not loop unboundedly on a crowded room.
fn pick_free(floor: &[Cell], taken: &[Cell], rng: &mut Rng) -> Option<Cell> {
    let mut free: Vec<Cell> = floor
        .iter()
        .copied()
        .filter(|c| !taken.contains(c))
        .collect();
    if free.is_empty() {
        return None;
    }
    let i = rng.below(free.len() as u32) as usize;
    Some(free.swap_remove(i))
}

/// Whether the placed level is solvable by the player's actual movement rules:
/// start → every objective → the comms console → exit (§10.6).
///
/// Floods the cells a *player* can come to occupy — floor, open **and closed**
/// panels (a bump opens them, §10.4), and hideouts (bump-to-enter, §10.3) —
/// with the placed console and exit cells masked solid, as they will be in play.
/// Consoles and the exit are bump-interactions, never stood on (§4.3), so each
/// must be **adjacent** to the flooded set rather than inside it. The pre-placement
/// §10.6 gate proved the empty carve connected; this catches the rarer sin of a
/// console stamped into a squeeze cell pinching the route that proof relied on.
///
/// The comms console (§7.3/§7.7) is held to the same standard as an objective, and
/// deliberately so: *a console the player cannot reach is not counterplay.* It is not
/// required to win, so a stricter reading might let it be walled off — but the whole
/// point of the §7.7 answer to the radio net is that it is there to be found, and a
/// seed that seals it away silently deletes the mechanic from that run.
fn solvable(facility: &Facility, placement: &Placement) -> bool {
    let solid: Vec<Cell> = placement
        .intel
        .iter()
        .copied()
        .chain([placement.comms, placement.exit])
        .collect();
    let enterable = |c: Cell| {
        !solid.contains(&c)
            && facility.terrain(c).is_some_and(|t| {
                matches!(
                    t,
                    Terrain::Floor
                        | Terrain::DoorPanelOpen
                        | Terrain::DoorPanelClosed
                        | Terrain::Hideout
                )
            })
    };

    let (w, h) = (facility.width(), facility.height());
    // The run starts inside the tunnel (§4.5/#466), so the flood starts where the
    // player first sets foot in the facility: the cells they can climb out of `E`
    // onto. Every one of them, unioned — the player chooses which side to come up on,
    // so ground reachable from any of them is ground they can reach.
    let reached: HashSet<Cell> = footholds(facility, placement.exit, placement.exit_duct.cells())
        .filter(|&n| enterable(n))
        .flat_map(|foothold| path::flood_from(foothold, w, h, enterable))
        .collect();
    if reached.is_empty() {
        return false; // a mouth with nothing to climb out onto is not a way in
    }

    // Consoles and the exit are bump-interactions, so each must be *adjacent* to the
    // flooded set rather than inside it.
    solid
        .iter()
        .all(|&target| facility.neighbours(target).any(|n| reached.contains(&n)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{generate_level, EXIT_DUCT_MAX_CELLS, EXIT_DUCT_MIN_CELLS};
    use crate::guard::Guard;
    use crate::state::State;
    use crate::test_support::seed_sweep;
    use crate::{Direction, GenError, Outcome};

    /// The exhaustive seed range every property below sweeps. Placement must hold on
    /// *accepted* seeds universally, not on a lucky one (§10.6). The routine gate
    /// samples this via [`seed_sweep`]; CI (`INTRUSION_SLOW_TESTS=1`) runs it whole.
    const SEEDS: u64 = 64;

    fn v1(seed: u64) -> (Layout, Placement) {
        generate_level(
            &LevelConfig::V1,
            &crate::LevelModifiers::default(),
            &mut Rng::new(seed),
        )
        .expect("the v1 config places")
    }

    /// The room region a cell belongs to.
    fn room_of(layout: &Layout, cell: Cell) -> RegionId {
        let id = layout
            .regions()
            .region_at(cell)
            .expect("a placed cell is in a region");
        assert_eq!(
            layout.regions().kind(id),
            RegionKind::Room,
            "pieces go in rooms"
        );
        id
    }

    /// The floor-cell count of a room region — the "largest room" measure.
    fn floor_area(layout: &Layout, id: RegionId) -> usize {
        layout
            .regions()
            .region(id)
            .cells()
            .iter()
            .filter(|&&c| layout.facility().terrain(c) == Some(Terrain::Floor))
            .count()
    }

    /// The §12.6 guard-count knob is a **step within an envelope** (#232): it moves
    /// the recipe's own count by one and stops at
    /// [`GUARDS_MIN`](LevelConfig::GUARDS_MIN)…[`GUARDS_MAX`](LevelConfig::GUARDS_MAX),
    /// and — the part worth pinning — a recipe already outside that envelope is left
    /// where it is rather than dragged into it, so an end can never move the count the
    /// way it does not name.
    #[test]
    fn the_guard_knob_steps_the_recipe_and_stops_at_the_envelope() {
        // The shipped recipe sits inside the envelope (held at compile time beside
        // the constants), so both ends bite: §10.2's four becomes three or five, the
        // ±1 the knob promises.
        assert_eq!(LevelConfig::V1.guards, 4, "the §10.2 [START] baseline");
        let at = |knob| LevelConfig::V1.with_guard_count(knob).guards;
        assert_eq!(at(GuardCount::Fewer), LevelConfig::V1.guards - 1);
        assert_eq!(at(GuardCount::Baseline), LevelConfig::V1.guards);
        assert_eq!(at(GuardCount::More), LevelConfig::V1.guards + 1);
        assert_eq!(at(GuardCount::Fewer), LevelConfig::GUARDS_MIN);
        assert_eq!(at(GuardCount::More), LevelConfig::GUARDS_MAX);

        // The knob touches nothing but the count.
        assert_eq!(
            LevelConfig {
                guards: LevelConfig::V1.guards,
                ..LevelConfig::V1.with_guard_count(GuardCount::More)
            },
            LevelConfig::V1,
        );

        // Over the whole range a recipe could name, including the sim's sweeps: the
        // result is within one of the baseline, never past the envelope in the
        // direction asked for, and **never moved the wrong way**.
        for guards in 0..=12 {
            let recipe = LevelConfig {
                guards,
                ..LevelConfig::V1
            };
            let fewer = recipe.with_guard_count(GuardCount::Fewer).guards;
            let more = recipe.with_guard_count(GuardCount::More).guards;
            assert_eq!(recipe.with_guard_count(GuardCount::Baseline).guards, guards);
            assert!(fewer <= guards && guards <= more, "{guards}: wrong way");
            assert!(
                guards - fewer <= 1 && more - guards <= 1,
                "{guards}: a step"
            );
            // Inside the envelope both ends move; outside it the end pointing further
            // out stays put rather than clamping back across the baseline.
            assert_eq!(fewer < guards, guards > LevelConfig::GUARDS_MIN);
            assert_eq!(more > guards, guards < LevelConfig::GUARDS_MAX);
        }
    }

    /// **The §2.3 directional assertion for the guard-count knob** (#232): from one
    /// seed, more guards means at least as much detection pressure as the baseline and
    /// fewer means at most — and the claim is exact rather than distributional,
    /// because the knob is read by *placement* and not by the carve.
    ///
    /// So the three settings put the same player, exit and intel in the same building,
    /// and — since the guards come off one shuffled pool by `take(n)` — their guard
    /// sets are strictly **nested**. The turn-one detection sets are therefore nested
    /// too, which is the pressure claim stated over the cells a guard can actually
    /// catch you in.
    #[test]
    fn more_guards_watch_a_superset_of_what_fewer_guards_watch() {
        // The turn-one detection set of a placed guard: the same call placement
        // itself makes (the conservative rear carve, §7.1/#410).
        let watched = |layout: &Layout, p: &Placement| -> HashSet<Cell> {
            p.guard_cells()
                .iter()
                .flat_map(|&cell| {
                    field_of_view_with_blind_spot(
                        layout.facility(),
                        cell,
                        GUARD_INITIAL_FACING,
                        GUARD_SIGHT_ARC,
                        GUARD_SIGHT_RANGE,
                        BlindTier::REAR,
                    )
                    .cells()
                    .collect::<Vec<_>>()
                })
                .collect()
        };
        let at = |seed: u64, knob| {
            generate_level(
                &LevelConfig::V1,
                &crate::LevelModifiers {
                    guard_count: knob,
                    ..crate::LevelModifiers::default()
                },
                &mut Rng::new(seed),
            )
            .expect("the v1 config places at every setting of the knob")
        };

        for seed in seed_sweep(SEEDS) {
            let (fewer_layout, fewer) = at(seed, GuardCount::Fewer);
            let (base_layout, base) = at(seed, GuardCount::Baseline);
            let (more_layout, more) = at(seed, GuardCount::More);

            // Exactly the requested counts, at every setting — §10.6's no-silent-drop
            // rule applies to the knob's numbers as much as to the recipe's.
            assert_eq!(fewer.guard_cells().len(), LevelConfig::GUARDS_MIN);
            assert_eq!(base.guard_cells().len(), LevelConfig::V1.guards);
            assert_eq!(more.guard_cells().len(), LevelConfig::GUARDS_MAX);

            // The same building, cell for cell: the knob reaches placement, not the
            // carve, so the comparison below is between two runs of one facility.
            let facility = base_layout.facility();
            for y in 0..facility.height() {
                for x in 0..facility.width() {
                    let want = facility.terrain_at(x, y);
                    assert_eq!(
                        fewer_layout.facility().terrain_at(x, y),
                        want,
                        "seed {seed}"
                    );
                    assert_eq!(more_layout.facility().terrain_at(x, y), want, "seed {seed}");
                }
            }

            // …and the same pieces in it, drawn before the guards from one stream.
            for other in [&fewer, &more] {
                assert_eq!(other.player(), base.player(), "seed {seed}");
                assert_eq!(other.exit(), base.exit(), "seed {seed}");
                assert_eq!(other.intel(), base.intel(), "seed {seed}");
            }

            // **Nested guard sets**: fewer is the baseline's guards minus its last,
            // more is them plus one. `take(n)` off one shuffled pool, in one order.
            assert_eq!(
                fewer.guard_cells(),
                &base.guard_cells()[..LevelConfig::GUARDS_MIN],
                "seed {seed}: dropping a guard re-stationed the others",
            );
            assert_eq!(
                &more.guard_cells()[..LevelConfig::V1.guards],
                base.guard_cells(),
                "seed {seed}: adding a guard re-stationed the others",
            );

            // The directional claim itself, in cells a guard can catch you in.
            let (fewer_seen, base_seen, more_seen) = (
                watched(&fewer_layout, &fewer),
                watched(&base_layout, &base),
                watched(&more_layout, &more),
            );
            assert!(
                base_seen.is_subset(&more_seen),
                "seed {seed}: more guards watched less ground",
            );
            assert!(
                fewer_seen.is_subset(&base_seen),
                "seed {seed}: fewer guards watched more ground",
            );
        }
    }

    /// Determinism (§12.4): the guard count is part of the reproducible config, so the
    /// same seed **and the same knob** place the identical board — and two *different*
    /// settings of the knob do not, or the modifier would be a facade (§2.3).
    #[test]
    fn the_guard_knob_is_part_of_the_reproducible_config() {
        let at = |seed: u64, knob| {
            generate_level(
                &LevelConfig::V1,
                &crate::LevelModifiers {
                    guard_count: knob,
                    ..crate::LevelModifiers::default()
                },
                &mut Rng::new(seed),
            )
            .expect("the v1 config places")
            .1
        };
        for seed in seed_sweep(SEEDS) {
            for knob in [GuardCount::Fewer, GuardCount::Baseline, GuardCount::More] {
                assert_eq!(at(seed, knob), at(seed, knob), "seed {seed}: {knob:?}");
            }
            assert_ne!(at(seed, GuardCount::Baseline), at(seed, GuardCount::More));
            assert_ne!(at(seed, GuardCount::Baseline), at(seed, GuardCount::Fewer));
        }
    }

    /// §10.6: **exactly** the requested counts, on every accepted seed — never the
    /// old asked-for-5-got-4 silent shortfall.
    #[test]
    fn accepted_seeds_place_exact_counts_on_plain_floor() {
        for seed in seed_sweep(SEEDS) {
            let (layout, p) = v1(seed);
            assert_eq!(p.intel().len(), LevelConfig::V1.intel, "seed {seed}");
            assert_eq!(p.guard_cells().len(), LevelConfig::V1.guards, "seed {seed}");

            // Every piece on its own plain floor cell — no stacking, no walls. The
            // player is not among them any more (§4.5/#466): they start inside the
            // tunnel, on the border wall it comes out through.
            let mut all = vec![p.exit(), p.comms()];
            all.extend_from_slice(p.intel());
            all.extend_from_slice(p.guard_cells());
            for &c in &all {
                assert_eq!(
                    layout.facility().terrain(c),
                    Some(Terrain::Floor),
                    "seed {seed}: {c:?} is not plain floor"
                );
            }

            let mut dedup = all.clone();
            dedup.sort_unstable_by_key(|c| (c.x, c.y));
            dedup.dedup();
            assert_eq!(
                dedup.len(),
                all.len(),
                "seed {seed}: two pieces share a cell"
            );
        }
    }

    /// §7.7 + §10.6: the comms console is **exactly one** per facility, outside the
    /// start room, and at least [`PLAYER_COMMS_MIN_DISTANCE`] from the spawn — the
    /// distance being what keeps silencing the radio a deliberate detour rather than a
    /// switch on the way out of the start room (§2.3).
    ///
    /// This pins the **[START]** distance so a later tune is a visible edit, and pins
    /// "exactly one" so no seed ever ships a second switch for a net already dead.
    #[test]
    fn the_comms_console_is_one_real_detour_from_the_way_in() {
        assert_eq!(
            PLAYER_COMMS_MIN_DISTANCE, 16,
            "the [START] comms detour distance"
        );
        for seed in seed_sweep(SEEDS) {
            let (layout, p) = v1(seed);
            // Measured from the exit `E` (§4.5/#466) — where the player comes up, and
            // so where their route through the facility starts.
            let distance = p.comms().manhattan_distance(p.exit());
            assert!(
                distance >= PLAYER_COMMS_MIN_DISTANCE,
                "seed {seed}: the comms console spawned {distance} from the way in"
            );
            assert_ne!(
                room_of(&layout, p.comms()),
                room_of(&layout, p.exit()),
                "seed {seed}: comms console in the start room"
            );

            // The carve handed back is **bare** — the console is recorded, not stamped
            // (§10.5/§10.6 run their floods on this grid) — and the record agrees with
            // the placement.
            assert_eq!(
                layout.comms_console(),
                Some(p.comms()),
                "seed {seed}: the layout's record disagrees with the placement"
            );

            // One switch, one net, in the grid a run actually plays. Counted over the
            // whole board, so a stray stamp anywhere would show up.
            let state = State::new(
                layout,
                p.player(),
                Direction::North,
                Vec::new(),
                p.intel().iter().copied(),
                p.exit(),
            );
            let facility = state.layout().facility();
            let stamped = (0..facility.height())
                .flat_map(|y| (0..facility.width()).map(move |x| Cell::new(x, y)))
                .filter(|&c| facility.terrain(c) == Some(Terrain::CommsConsole))
                .count();
            assert_eq!(stamped, 1, "seed {seed}: {stamped} comms consoles");
            assert_eq!(
                state.comms_console(),
                Some(p.comms()),
                "seed {seed}: the state found the wrong console"
            );
        }
    }

    /// §10.1.7: the exit sits in the **largest room**, and behind it runs the player's
    /// own tunnel (§4.5/#466) — straight, short, and ending on the level border, where
    /// the run begins.
    ///
    /// This is what became of `player_and_exit_share_the_largest_room_well_apart`: there
    /// is no second spawn to space out any more, so the guarantee that no run starts won
    /// is now the tunnel's own length floor.
    #[test]
    fn the_exit_sits_in_the_largest_room_with_its_tunnel_behind_it() {
        assert_eq!(EXIT_DUCT_MIN_CELLS, 4, "the [START] tunnel length floor");
        assert_eq!(EXIT_DUCT_MAX_CELLS, 12, "the [START] tunnel length cap");
        for seed in seed_sweep(SEEDS) {
            let (layout, p) = v1(seed);
            let facility = layout.facility();
            let start = room_of(&layout, p.exit());

            let start_area = floor_area(&layout, start);
            for (id, _) in layout.regions().regions() {
                if layout.regions().kind(id) == RegionKind::Room {
                    assert!(
                        floor_area(&layout, id) <= start_area,
                        "seed {seed}: the start room is not the largest"
                    );
                }
            }

            let duct = p.exit_duct();
            let cells = duct.cells();
            assert_eq!(cells[0], p.exit(), "seed {seed}: the tunnel starts at E");
            assert_eq!(
                duct.way_out(),
                Some(p.player()),
                "seed {seed}: the run starts on the way out"
            );
            assert!(
                (EXIT_DUCT_MIN_CELLS..=EXIT_DUCT_MAX_CELLS).contains(&cells.len()),
                "seed {seed}: a {}-cell tunnel",
                cells.len()
            );
            // Straight: one axis varies, and it varies by one cell at a time.
            let straight =
                cells.iter().all(|c| c.x == cells[0].x) || cells.iter().all(|c| c.y == cells[0].y);
            assert!(straight, "seed {seed}: the tunnel bends");
            for w in cells.windows(2) {
                assert_eq!(w[0].manhattan_distance(w[1]), 1, "seed {seed}");
            }
            // It comes out **through** the border ring, which stays the unbroken wall
            // §10.6 asserts — the tunnel stamps nothing.
            let out = p.player();
            assert!(
                out.x == 0
                    || out.y == 0
                    || out.x == facility.width() - 1
                    || out.y == facility.height() - 1,
                "seed {seed}: the way out is not on the border"
            );
            assert_eq!(
                facility.terrain(out),
                Some(Terrain::Wall),
                "seed {seed}: the way out was stamped"
            );
            // Inert geometry only (§10.7): nothing interactable under the crawl.
            for &interior in &cells[1..cells.len() - 1] {
                assert!(
                    matches!(
                        facility.terrain(interior),
                        Some(Terrain::Floor) | Some(Terrain::Wall)
                    ),
                    "seed {seed}: the tunnel crawls over {:?}",
                    facility.terrain(interior)
                );
            }
            // No cell is shared with a §10.7 shortcut, so "which duct am I in" has one
            // answer — and the layout records the tunnel among the ducts.
            let shortcuts: HashSet<Cell> = layout
                .ducts()
                .iter()
                .filter(|d| d.way_out().is_none())
                .flat_map(|d| d.cells().iter().copied())
                .collect();
            for c in cells {
                assert!(
                    !shortcuts.contains(c),
                    "seed {seed}: ducts overlap at {c:?}"
                );
            }
            assert_eq!(
                layout.exit_duct().map(|d| d.cells()),
                Some(cells),
                "seed {seed}: the layout's tunnel disagrees with the placement"
            );
        }
    }

    /// §10.1.8 + §10.6 spacing: every intel in a room that is neither the start
    /// room nor another intel's room — all three can never clump (the old bug).
    #[test]
    fn intel_spreads_across_distinct_non_start_rooms() {
        for seed in seed_sweep(SEEDS) {
            let (layout, p) = v1(seed);
            let start = room_of(&layout, p.exit());
            let rooms: Vec<RegionId> = p.intel().iter().map(|&c| room_of(&layout, c)).collect();
            assert!(
                !rooms.contains(&start),
                "seed {seed}: intel in the start room"
            );
            for (i, a) in rooms.iter().enumerate() {
                assert!(
                    !rooms[i + 1..].contains(a),
                    "seed {seed}: two intel share a room"
                );
            }
        }
    }

    /// §10.1.9 + §10.6 "the starting area should be safe": guards spawn outside
    /// the start room, and — checked through the **real** turn loop, consoles
    /// stamped and the startup turn run — no guard's turn-one cone covers the **mouth
    /// the player comes up out of** (§4.5/#466). The spawn itself needs no such rule
    /// any more: it is inside the tunnel, which conceals absolutely (§10.7).
    #[test]
    fn no_guard_eyes_the_way_in_on_turn_one() {
        for seed in seed_sweep(SEEDS) {
            let (layout, p) = v1(seed);
            let start = room_of(&layout, p.exit());
            for &g in p.guard_cells() {
                assert_ne!(
                    room_of(&layout, g),
                    start,
                    "seed {seed}: guard in the start room"
                );
            }

            // Stationary fixtures on purpose: this asserts the *turn-one spawn cone*
            // (§10.6), the static geometry placement guarantees — a patrolling guard
            // would have swept off its spawn during the startup turn and we'd be
            // checking a different cone than the one the guarantee is about.
            let guards = p
                .guard_cells()
                .iter()
                .map(|&c| Guard::stationary(c))
                .collect();
            let state = State::new(
                layout,
                p.player(),
                Direction::North,
                guards,
                p.intel().iter().copied(),
                p.exit(),
            );
            assert_eq!(state.outcome(), Outcome::Playing, "seed {seed}");
            for guard in state.guards() {
                assert!(
                    !guard.fov().contains(p.exit()),
                    "seed {seed}: the guard at {:?} watches the mouth on turn one",
                    guard.pos()
                );
            }
            // And the crawl itself is safe by a stronger rule than placement's
            // (§10.7): the player starts *inside* the tunnel, concealed from every
            // guard on the board and beyond any contact.
            assert!(state.in_duct(), "seed {seed}: the run starts in the tunnel");
            for guard in state.guards() {
                assert!(state.concealed_from(guard.pos()), "seed {seed}");
            }
        }
    }

    /// §7.5 coverage on real generated levels: the guards **partition the facility
    /// between them**, which is the "a wing goes uncovered" weakness answered by
    /// construction rather than by luck.
    ///
    /// Three claims, all of which the old grower failed. Every region is claimed by
    /// **exactly one** guard: nothing is doubled up, and — the part that was simply
    /// impossible before, when each beat was capped at four regions on a level with
    /// seventeen to twenty-three — nothing is left to nobody. Every beat is connected
    /// across doors, so a guard can walk its whole territory. And the beats are
    /// **balanced** to within one region, because growth is round-robin.
    #[test]
    fn the_guards_partition_the_facility_between_them() {
        use crate::beat::{coordinated_beats, is_connected};

        for seed in seed_sweep(SEEDS) {
            let (layout, p) = v1(seed);
            let regions = layout.regions();
            let beats = coordinated_beats(regions, layout.facility(), p.guard_cells());

            let claimed: Vec<RegionId> = beats.iter().flatten().copied().collect();
            let distinct: HashSet<RegionId> = claimed.iter().copied().collect();
            assert_eq!(
                claimed.len(),
                distinct.len(),
                "seed {seed}: two guards claim the same region",
            );
            assert_eq!(
                distinct.len(),
                regions.regions().count(),
                "seed {seed}: a region belongs to no guard — a wing goes uncovered",
            );

            for beat in &beats {
                assert!(!beat.is_empty(), "seed {seed}: a guard got no beat");
                assert!(
                    is_connected(regions, layout.facility(), beat),
                    "seed {seed}: a beat straddles a wall into ground it cannot reach",
                );
            }
            let mut sizes: Vec<usize> = beats.iter().map(Vec::len).collect();
            sizes.sort_unstable();
            // Balance is best-effort and the building is the limit: a facility is
            // hub-shaped, and the hub's group cannot give it up without splitting in
            // two (see `beat::split`). Splits run from 5/4/4/4 to 9/4/3/1 across the
            // sweep. What is guaranteed is coverage, connectedness, and that every
            // guard has ground — asserted above.
            assert!(sizes.iter().sum::<usize>() == regions.regions().count());
        }
    }

    /// §12.4: placement is deterministic — the same seed places the same board,
    /// piece for piece.
    #[test]
    fn placement_is_deterministic() {
        for seed in [0, 7, 2026] {
            let (_, a) = v1(seed);
            let (_, b) = v1(seed);
            assert_eq!(a, b, "seed {seed}");
        }
    }

    /// §10.6 "fail loudly or retry the seed": a config that can never be placed —
    /// more intel than rooms can exist — errors out with [`GenError::RetriesExhausted`]
    /// instead of shipping a shortfall or spinning forever.
    #[test]
    fn an_unplaceable_config_fails_loudly() {
        let impossible = LevelConfig {
            intel: 40, // room count is capped at ~12 (§10.2) — never satisfiable
            ..LevelConfig::V1
        };
        assert!(matches!(
            generate_level(
                &impossible,
                &crate::LevelModifiers::default(),
                &mut Rng::new(0)
            ),
            Err(GenError::RetriesExhausted { .. })
        ));
    }

    /// The post-placement solvability flood: a console sealed into a pocket the
    /// player cannot bump from outside fails the check; the same console with its
    /// pocket open passes. Holds for the **comms** console (§7.7) exactly as for an
    /// objective — unreachable counterplay is no counterplay. (On generated levels the
    /// §10.6 gate makes this rare — this pins the assertion itself.)
    #[test]
    fn solvability_requires_every_target_bump_adjacent() {
        // A placement with `pocket` holding the named piece and everything else out
        // in the open, so each target can be sealed in turn.
        let with_intel = |pocket: Cell| Placement {
            player: Cell::new(0, 8),
            exit: Cell::new(8, 8),
            exit_duct: crate::duct::Duct::exit_tunnel(vec![
                Cell::new(8, 8),
                Cell::new(7, 8),
                Cell::new(6, 8),
                Cell::new(5, 8),
                Cell::new(4, 8),
                Cell::new(3, 8),
                Cell::new(2, 8),
                Cell::new(1, 8),
                Cell::new(0, 8),
            ]),
            intel: vec![pocket],
            comms: Cell::new(8, 5),
            guards: Vec::new(),
            guard_clocks: Vec::new(),
        };
        let with_comms = |pocket: Cell| Placement {
            comms: pocket,
            intel: vec![Cell::new(8, 5)],
            ..with_intel(Cell::new(8, 5))
        };

        let mut sealed = Facility::walled_box(10, 10);
        // Wall the corner pocket at (1,1) shut: a console inside has no reachable
        // neighbour to bump it from.
        for (x, y) in [(2, 1), (1, 2), (2, 2)] {
            sealed.set_terrain(x, y, Terrain::Wall);
        }
        let pocket = Cell::new(1, 1);
        assert!(!solvable(&sealed, &with_intel(pocket)), "sealed intel");
        assert!(!solvable(&sealed, &with_comms(pocket)), "sealed comms");

        let open = Facility::walled_box(10, 10);
        assert!(solvable(&open, &with_intel(pocket)));
        assert!(solvable(&open, &with_comms(pocket)));
    }
}
