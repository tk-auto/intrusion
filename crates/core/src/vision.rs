//! Field of view: the facing-dependent forward cone (§6).
//!
//! Sight is a **symmetric shadowcast over the square box** (§6.1/§6.2): range *R*
//! means a `(2R+1)²` box around the viewer, no distance falloff, blocked by whatever
//! [`Terrain::blocks_sight`](crate::Terrain::blocks_sight) says — walls, closed door
//! panels, hinges. An opaque cell is itself seen (you see the wall face) but shadows
//! everything behind it.
//!
//! The cone comes from one trick (§6.2): **the out-of-arc cells of the viewer's own
//! 8-neighbour ring are treated as if they were walls.** Shadowcasting propagates
//! outward, so those artificial walls cast the shadows that carve the cone — and
//! because artificial walls are marked seen exactly like real ones, **the 8 cells
//! around a viewer are always seen, in every direction, including directly behind**
//! **[SETTLED]**. That touching ring is load-bearing for the *player*: you can never
//! stand adjacent to the player undetected. Guards keep the ring's forward and side
//! cells but carve out the three at their back — the **rear blind spot** (§6.1/§7.2,
//! [`field_of_view_with_rear_blind_spot`]) — so a takedown from directly behind an
//! unaware guard can be set up. Beside or in front of a guard is still never free.
//!
//! Which ring cells count as walls is the arc-width ↔ tier rule (§6.2): neighbours
//! rank 1–5 by angular deviation from facing (ahead, forward diagonal, side, rear
//! diagonal, behind), and a neighbour is transparent iff `arc_width >= tier`. Arc 2
//! is the guard's ~90° wedge, 3 the player's ~180° half-disc, 5 the full 360° of a
//! turn spent waiting. The arcs are approximate by construction — a transparent side
//! neighbour lets sight graze a little past the square angle — which is exactly the
//! behaviour the design kept: "this is elegant and it works."
//!
//! The algorithm is the symmetric variant of shadowcasting, in integer arithmetic
//! throughout (slopes are rationals), so it is exactly deterministic (§12.4) and has
//! the fairness property the name promises: between transparent cells, if A can see
//! B then B — looking that way with the same arc — can see A.
//!
//! On top of the plain cast sits one deliberate, player-only exception: the
//! **auto-peek** ([`field_of_view_with_peek`], #121) — the union of the view from
//! where the player stands and the view from the cell their head would occupy if
//! they leaned one step forward. The union steps outside the symmetry property on
//! purpose; see the function for the rule and the rationale.

use crate::cell::{Cell, Direction};
use crate::facility::Facility;

/// The player's sight range (§5) — a 31×31 box. **[START]**
pub const PLAYER_SIGHT_RANGE: u32 = 15;
/// The player's sight arc (§5/§6.2): width 3, the ~180° forward half-disc. **[START]**
pub const PLAYER_SIGHT_ARC: u8 = 3;
/// The **full 360°** arc (§6.2: width 5 — every ring neighbour transparent, so no
/// artificial wall carves a cone at all).
///
/// Two things reach it, and they are the same fact seen twice: a turn spent
/// **waiting** (§8.3/§9.1 — the innate way to see behind you, bought one turn at a
/// time) and the **Vision** passive (§8.3/#265 — the same arc bought once, with a
/// loadout slot). One constant, so the two can never drift into two different
/// "360°"s.
pub const FULL_SIGHT_ARC: u8 = 5;
/// The player's sight range while the **Vision** passive is held (§5/§8.3, #265) —
/// a 41×41 box, up from §5's 31×31. **[START]**
///
/// On the current 40×40 facility (§10.2) the lift is mostly nominal — walls, not
/// the box, are what bound sight indoors — so the passive's real gift is the arc.
/// The range is raised anyway so the ability is honest about what it claims, and
/// so it still reads as an upgrade on a larger board.
pub const ENHANCED_SIGHT_RANGE: u32 = 20;
/// A guard's sight range (§7.1) — a 21×21 box. **[START]**
pub const GUARD_SIGHT_RANGE: u32 = 10;
/// A guard's sight arc (§7.1/§6.2): width 2, the ~90° forward wedge. **[START]**
pub const GUARD_SIGHT_ARC: u8 = 2;
/// How much of a guard's **touching ring** (§6.1) is blind — the lowest §6.2 ring
/// tier dropped from its detection set, and everything above it with it.
///
/// The §6.2 tier ladder, by angular deviation from the guard's facing:
///
/// | tier | cells | |
/// |---|---|---|
/// | 1 | directly ahead | always detects |
/// | 2 | the two forward diagonals | always detects |
/// | 3 | the two **sides** | the experiment's question |
/// | 4 | the two rear diagonals | blind since §155 |
/// | 5 | directly behind | blind since §155 |
///
/// It is deliberately **not** called "rear" any more (#410): the name has to keep
/// describing what it carves whichever way the flank experiment goes, and "rear"
/// stopped being true the moment tier 3 became a candidate.
///
/// Whatever it carves, it carves **after the cast has run**, so the cells stay §6.2
/// artificial cone-carving walls and the ~90° silhouette is untouched — this changes
/// what a guard *notices*, never what it can see past. The player's ring is not
/// negotiable either way (§6.1 **[SETTLED]**).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct BlindTier(u8);

impl BlindTier {
    /// **What an alerted guard carves** (§155, §6.1/§7.2): the three cells at a
    /// guard's back — the two rear diagonals (tier 4) and directly behind (tier 5).
    /// Its *sides* still detect, so against a guard that is hunting you a takedown
    /// must come from directly behind or rear-diagonal, and you can never stand
    /// beside or in front of it undetected.
    ///
    /// This was every guard's carve in every mood until #442; it is now what a guard
    /// falls back to the moment it stops being Calm.
    pub const REAR: Self = Self(4);

    /// **What a Calm guard carves** (#410, adopted by #442): tiers 3–5, so a patrol
    /// detects exactly what its ~90° cone covers and the free touching ring becomes
    /// player-only. Two things follow — a **flank** takedown opens up, and a **tail**
    /// survives a corner: walk in a patrol's blind spot and its 90° turn no longer
    /// catches you (a 180° turn still does, since that lands you at tier 1, dead
    /// ahead).
    ///
    /// Reached only through [`BlindPolicy::FlankWhileCalm`], and so only ever by a
    /// guard that is **Calm** — which is the whole of the rule's pricing.
    pub const FLANK: Self = Self(3);

    /// Whether a ring neighbour at `tier` is dropped from detection.
    fn carves(self, tier: u8) -> bool {
        tier >= self.0
    }
}

/// How much of a guard's ring goes blind (§6.1/§6.2/#410/#442) — the **policy**,
/// resolved to a [`BlindTier`] per guard by the guard's own
/// [`GuardState`](crate::GuardState).
///
/// It is a policy rather than a bare tier because the rule is **conditional on the
/// guard's mood**, and the mood lives on the guard. Passing the policy down and
/// resolving it there keeps one reading of one fact: nothing has to remember to ask
/// "is this guard calm?" alongside "which carve applies?", and the two can never be
/// answered inconsistently.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BlindPolicy {
    /// **The rule this replaced** (§155): every guard, in every mood, blind only at
    /// [`BlindTier::REAR`] — the three cells at its back, its sides always live.
    ///
    /// No longer reachable in a shipped run (#442): it is kept as the named control
    /// arm the A/B tests still compare against, because "what a flank used to do" is
    /// the contrast that makes the tier ladder legible. Do not wire it back to a
    /// level's config — restoring the harder rule would be a **new** modifier slot,
    /// never a revival of the retired one.
    Rear,

    /// **The rule** (#410, adopted by #442): a **Calm** guard is blind at its flanks
    /// ([`BlindTier::FLANK`]) — it detects exactly its ~90° cone — and any guard that
    /// is *not* Calm falls back to [`BlindTier::REAR`] and watches its sides again.
    ///
    /// The point of the condition is that it prices the gift. Reading a patrol is
    /// rewarded: you can tail a calm guard through a corner and take it from the
    /// flank. Being *hunted* is not: the moment a guard is chasing, investigating,
    /// searching or answering a call, its sides are live, so the flank is a place you
    /// can work from and never a place you can hide in. The unconditional form gave
    /// avoidance-first play a win-rate rise with no new decision attached, which is
    /// the un-priced safety §7.2 exists to prevent (appendix 28).
    #[default]
    FlankWhileCalm,
}

impl BlindPolicy {
    /// The tier this policy carves for a guard in `state`.
    pub(crate) fn tier(self, state: crate::GuardState) -> BlindTier {
        match self {
            BlindPolicy::Rear => BlindTier::REAR,
            BlindPolicy::FlankWhileCalm if state == crate::GuardState::Calm => BlindTier::FLANK,
            BlindPolicy::FlankWhileCalm => BlindTier::REAR,
        }
    }
}

/// **How a guard sees on this level** (§6.1/§7.1/§12.6) — the cone's shape and what
/// its ring detects, as one value resolved from the level's modifiers and handed down
/// to every guard that looks.
///
/// It exists because the arc and the range are about to stop being constants (#495).
/// They are read by the cast, by §7.6's two-zone detection and by the danger overlay
/// drawn from both, so a level modifier that moves them is either one value threaded
/// where [`BlindPolicy`] already was, or a second parameter beside it at every call.
/// **One value**, because a second sight-affecting modifier should then be a change to
/// this struct's contents and not another thread through the same six signatures.
///
/// [`BlindPolicy`] is a **field** rather than a neighbour for the same reason: it is
/// already the answer to *"how does a guard look this level?"*, asked about the ring
/// instead of the wedge. Nothing may carry a copy on the guard — the value is derived
/// from [`State`](crate::State) and passed per call, so a guard's cone and the §11.5
/// overlay drawn from it read one truth (§12.3, the #199/#200 shape).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GuardSight {
    /// The §6.2 arc width the cone is carved with.
    pub arc: u8,
    /// How far the cone reaches — a square box (§6.1), range *R* being `(2R+1)²`.
    pub range: u32,
    /// How much of the touching ring is dropped from detection (§6.1/#442).
    pub blind: BlindPolicy,
}

impl GuardSight {
    /// **§7.1's guard, unmodified** — the ~90° wedge out to 10, with the shipped
    /// [`BlindPolicy::FlankWhileCalm`] carve. Every run that does not draw the §12.6
    /// narrowed-cones modifier, and every hand-built guard in a test.
    pub const BASELINE: Self = Self {
        arc: GUARD_SIGHT_ARC,
        range: GUARD_SIGHT_RANGE,
        blind: BlindPolicy::FlankWhileCalm,
    };

    /// **The §155 control arm** ([`BlindPolicy::Rear`]) over §7.1's own cone: every
    /// guard blind at its back alone, whatever its mood. Not reachable in a shipped
    /// run — it is the contrast the flank rule's tests are stated against, which is why
    /// it is a test-only constant rather than a fourth thing a level could ask for.
    #[cfg(test)]
    pub(crate) const REAR_CARVE: Self = Self {
        arc: GUARD_SIGHT_ARC,
        range: GUARD_SIGHT_RANGE,
        blind: BlindPolicy::Rear,
    };

    /// The tier this level's policy carves for a guard in `state` — see
    /// [`BlindPolicy::tier`]. Resolved against the guard's own mood, which is why the
    /// whole value comes down rather than a bare [`BlindTier`].
    pub(crate) fn tier(self, state: crate::GuardState) -> BlindTier {
        self.blind.tier(state)
    }
}

impl Default for GuardSight {
    fn default() -> Self {
        Self::BASELINE
    }
}

/// The set of cells a viewer can currently see — one viewer's field of view,
/// recomputed every sight phase (§4.2) and stored on the viewer.
///
/// A default-constructed set is empty and contains nothing; it is the placeholder a
/// viewer carries before its first sight phase runs (§4.2 runs one full turn at
/// level start, so no live [`State`](crate::State) ever exposes one).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct VisibleSet {
    width: u32,
    height: u32,
    seen: Vec<bool>,
}

impl VisibleSet {
    /// An all-unseen set covering a `width × height` grid.
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            seen: vec![false; (width * height) as usize],
        }
    }

    /// An all-**seen** set covering a `width × height` grid — every cell of the
    /// facility at once.
    ///
    /// No cast produces this: it is the debug reveal (§12.6,
    /// [`DebugModifiers::reveal_whole_level`](crate::DebugModifiers::reveal_whole_level))
    /// standing in for the player's sight in a playtest build, so that *"I can see the
    /// whole level"* is expressed as the one thing it means — a field of view that
    /// covers everything — rather than as a special case in every view that reads one.
    pub(crate) fn everything(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            seen: vec![true; (width * height) as usize],
        }
    }

    /// Mark `cell` seen. Off-grid coordinates are ignored — the caster probes the
    /// box edge freely and the grid boundary simply absorbs it.
    fn mark(&mut self, cell: Cell) {
        if cell.x < self.width && cell.y < self.height {
            self.seen[(cell.y * self.width + cell.x) as usize] = true;
        }
    }

    /// Clear `cell` back to unseen — the corner-solidity pass retracting a floor
    /// tile the raw cast leaked sight into (§6.1). Off-grid is ignored.
    fn unmark(&mut self, cell: Cell) {
        if cell.x < self.width && cell.y < self.height {
            self.seen[(cell.y * self.width + cell.x) as usize] = false;
        }
    }

    /// Fold another set's seen cells into this one — the accumulation step of the
    /// player's tile memory (§11.5a): memory is the running union of every FOV the
    /// sight phase has produced, so it only ever grows. A default (empty)
    /// accumulator adopts the other set's grid; after that both must cover the
    /// same grid, which they do by construction — every set comes from the one
    /// facility.
    pub(crate) fn absorb(&mut self, other: &VisibleSet) {
        if self.seen.is_empty() {
            *self = other.clone();
            return;
        }
        debug_assert_eq!((self.width, self.height), (other.width, other.height));
        for (mine, theirs) in self.seen.iter_mut().zip(&other.seen) {
            *mine |= *theirs;
        }
    }

    /// [`absorb`](Self::absorb), holding one cell back — the crawl step of tile
    /// memory (§11.5a/§10.7). A duct's interior path is never remembered, so the
    /// interior cell the player currently occupies is the one cell of their view
    /// that must not accumulate; everything else the crawl view offers (the mouth
    /// peek out of an entry) is ordinary sight and accumulates as usual.
    ///
    /// The exclusion is applied to the *incoming* set, never to the accumulator, so
    /// memory stays monotonic (§11.5a): a wall cell already remembered from the room
    /// side stays remembered when a duct is later crawled through it.
    pub(crate) fn absorb_except(&mut self, other: &VisibleSet, skip: Option<Cell>) {
        match skip {
            None => self.absorb(other),
            Some(cell) => {
                let mut held_back = other.clone();
                held_back.unmark(cell);
                self.absorb(&held_back);
            }
        }
    }

    /// Whether the viewer sees `cell`. Anything off the grid is unseen.
    pub fn contains(&self, cell: Cell) -> bool {
        cell.x < self.width
            && cell.y < self.height
            && self.seen[(cell.y * self.width + cell.x) as usize]
    }

    /// Every seen cell, in row-major order — for the renderer's lighting pass
    /// (§11.5) and for tests.
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (0..self.height)
            .flat_map(move |y| (0..self.width).map(move |x| Cell::new(x, y)))
            .filter(|&c| self.contains(c))
    }
}

/// Compute the field of view of a viewer standing at `origin` in `facility`,
/// looking `facing`, with the given §6.2 arc width, out to `range` (a square box —
/// range *R* sees at most the `(2R+1)²` cells around the viewer, §6.1).
///
/// The origin itself and the full 8-neighbour ring are always in the result; the
/// arc and the terrain carve everything beyond (§6.2).
pub fn field_of_view(
    facility: &Facility,
    origin: Cell,
    facing: Direction,
    arc_width: u8,
    range: u32,
) -> VisibleSet {
    let mut fov = VisibleSet::new(facility.width(), facility.height());
    fov.mark(origin);
    let caster = Caster {
        facility,
        origin,
        facing,
        arc_width,
        range,
    };
    for quadrant in Quadrant::ALL {
        caster.scan(quadrant, &mut fov, 1, Slope::new(-1, 1), Slope::new(1, 1));
    }

    // Corner-solidity (§6.1): the raw shadowcast leaks sight through the pinch
    // where two walls meet at a diagonal — a viewer looking along the join sees
    // cells whose line of sight is in fact a straight run through a wall's body,
    // or exactly through the vertex the two walls jointly seal. Transparent
    // tiles keep the symmetric criterion — retract them when the centre-to-
    // centre segment is blocked. An opaque cell is seen when any part of its
    // face is, so it keeps the generous side of "you see the wall face":
    // retract it only when the segments to its centre *and* all four corners
    // are blocked — otherwise the cast's fan through a gap paints wall faces
    // deep in rooms no actual ray reaches (the leak seen through a cupboard
    // across a corridor). Grazing a bare corner (a vertex with at most one
    // opaque flank) is still allowed throughout, so the arc silhouette and the
    // always-seen touching ring (§6.1 **[SETTLED]**) are untouched.
    let leaked: Vec<Cell> = fov
        .cells()
        .filter(|&c| {
            if c.sight_distance(origin) <= 1 {
                return false;
            }
            let cx2 = 2 * i64::from(c.x);
            let cy2 = 2 * i64::from(c.y);
            if facility.terrain(c).is_some_and(|t| !t.blocks_sight()) {
                segment_is_blocked(facility, origin, cx2 + 1, cy2 + 1, c)
            } else {
                // Centre first, then the four corners of the cell's square.
                [(1, 1), (0, 0), (2, 0), (0, 2), (2, 2)]
                    .iter()
                    .all(|&(ox, oy)| segment_is_blocked(facility, origin, cx2 + ox, cy2 + oy, c))
            }
        })
        .collect();
    for c in leaked {
        fov.unmark(c);
    }

    fov
}

/// The player's sight: the §6 cone with the **auto-peek** union (#121). What the
/// player sees is [`field_of_view`] from `origin` *unioned with* a second cast
/// from the **head-lean origin** — the cell one step ahead along `facing` — with
/// the same facing, arc and range, clipped to `origin`'s own range box so the
/// advertised range never grows. Leaning is how you look past a corner you are
/// standing against without stepping into the open, and it is not a corner rule:
/// wherever a corner, a doorway edge or a cupboard mouth (§10.3) happens to be
/// adjacent, the second viewpoint sees around it naturally. On open floor the
/// clip means the union adds nothing — the peek only ever re-reveals what
/// geometry hid, never extends reach.
///
/// The lean contributes nothing when the forward cell blocks sight (a wall, a
/// hinge, a closed panel — you cannot lean into those) or lies off the grid. A
/// player facing out of a cupboard (the §7.6 auto-face, #89) leans through the
/// mouth, which is what widens the corridor from the mouth's ~90° wedge to the
/// full ~180° along its axis.
///
/// **This is the player's sight alone, one-sided by design.** Guards keep the
/// plain cast — a guard that saw around corners could not be *broken* by
/// corners, and corners are the player's main flight tool (§7.6). The union
/// therefore deliberately steps outside the module's symmetry property: the
/// peek can show you a guard that cannot see you. That is an information
/// channel in the §9 spirit, not a detection change — detection stays with the
/// guards' own plain cones, so the §11.5 danger overlay (painted from those
/// cones) never claims a peeked guard sees you.
pub fn field_of_view_with_peek(
    facility: &Facility,
    origin: Cell,
    facing: Direction,
    arc_width: u8,
    range: u32,
) -> VisibleSet {
    let mut fov = field_of_view(facility, origin, facing, arc_width, range);
    let Some(lean) = origin.step(facing) else {
        return fov;
    };
    if facility.terrain(lean).is_none_or(|t| t.blocks_sight()) {
        return fov;
    }
    let leaned = field_of_view(facility, lean, facing, arc_width, range);
    for cell in leaned.cells() {
        if cell.sight_distance(origin) <= range {
            fov.mark(cell);
        }
    }
    fov
}

/// The player's field of view while **inside a duct** (§10.7): the occupied crawl
/// cell, plus — when it is an **entry** — the live **mouth peek**. `mouth_out` is the
/// direction out through the mouth (`Some` only on an entry cell, whose recessed
/// geometry has exactly one floor neighbour); mid-duct it is `None` and the result is
/// the lone occupied cell — memory only, no live window (§10.7's information cost).
///
/// The peek is a [`field_of_view_with_peek`] cast **from the mouth** rather than from
/// the crawl cell: a duct entry is opaque (wall-like, §10.7), so the live window is
/// anchored one step out, on the floor, exactly as a cupboard's ~180° mouth peek reads
/// the corridor (§6.1). It stays one-sided — a guard's own plain cone cannot see the
/// concealed crawler back.
pub(crate) fn duct_field_of_view(
    facility: &Facility,
    occupied: Cell,
    mouth_out: Option<Direction>,
    arc_width: u8,
    range: u32,
) -> VisibleSet {
    let mut fov = VisibleSet::new(facility.width(), facility.height());
    fov.mark(occupied);
    if let Some(out) = mouth_out {
        if let Some(mouth) = occupied.step(out) {
            let window = field_of_view_with_peek(facility, mouth, out, arc_width, range);
            for cell in window.cells() {
                fov.mark(cell);
            }
        }
    }
    fov
}

/// A guard's sight for **detection**: the §6 cone with `blind` carved out of its
/// touching ring (§155/#410). At [`BlindTier::REAR`] that is the three cells at the
/// guard's back, so a player standing directly behind or rear-diagonal to an unaware
/// guard is undetected and a behind-the-back Takedown (§7.2) can be lined up; at
/// [`BlindTier::FLANK`] the two side cells go too, and the guard detects exactly its
/// cone.
///
/// This narrows the **[SETTLED]** 360° touching ring (§6.1) for guards only — the
/// player keeps the full ring, unqualified. The carved cells **remain the artificial
/// cone-carving walls** of §6.2: they are removed from the *visible* set only *after*
/// the cast has run, so every cell beyond the ring is untouched and the ~90°
/// silhouette is exactly [`field_of_view`]'s **whichever tier is carved**. That is
/// what makes the flank experiment a change to what a guard notices rather than to
/// what walls shadow. Carving the ring is a property of a guard's attention, not of
/// sight itself, so it lives here beside the player's one-sided peek rather than
/// inside the shared cast.
pub fn field_of_view_with_blind_spot(
    facility: &Facility,
    origin: Cell,
    facing: Direction,
    arc_width: u8,
    range: u32,
    blind: BlindTier,
) -> VisibleSet {
    let mut fov = field_of_view(facility, origin, facing, arc_width, range);
    // Unmarking ring neighbours leaves the cast that used them as walls — and
    // therefore the whole silhouette beyond the ring — intact.
    for dy in -1..=1 {
        for dx in -1..=1 {
            if (dx, dy) != (0, 0) && blind.carves(ring_tier(facing, dx, dy)) {
                let (x, y) = (i64::from(origin.x) + dx, i64::from(origin.y) + dy);
                if let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) {
                    fov.unmark(Cell::new(x, y));
                }
            }
        }
    }
    fov
}

/// Whether the cell at possibly-off-grid coordinates blocks sight — real
/// terrain opacity only (§10.3), never the §6.2 artificial ring. Off-grid
/// counts as opaque, matching the caster.
fn blocks_sight_at(facility: &Facility, x: i64, y: i64) -> bool {
    if x < 0 || y < 0 {
        return true;
    }
    facility
        .terrain(Cell::new(x as u32, y as u32))
        .is_none_or(|t| t.blocks_sight())
}

/// Whether the straight sight segment from `origin`'s centre to the point
/// `(bx2, by2)` — **doubled** coordinates, so cell centres are odd and cell
/// corners even — is blocked by real terrain. Two ways to be blocked, both
/// §6.1 corner-solidity:
///
/// - **Body:** the segment crosses the interior of a sight-blocking cell
///   (other than `target` itself) — seeing through a wall's body, not around
///   it.
/// - **Pinch:** the segment passes exactly through a grid vertex whose two
///   *flanking* cells — the pair it brushes past without entering — are both
///   sight-blocking. That vertex is two walls meeting at a diagonal, and they
///   jointly occlude it: every ray but the measure-zero corner line runs
///   through one wall body or the other.
///
/// A vertex with at most one opaque flank is grazed freely — the permissive
/// behaviour the cone silhouette and the touching ring depend on (§6.2): a
/// lone corner never hides what is beside it.
///
/// All integer arithmetic (centres, corners and boundaries are exact in
/// doubled coordinates), so it is deterministic (§12.4), and symmetric in its
/// endpoints. Real terrain opacity only — the §6.2 artificial ring walls are
/// not consulted, so this never reshapes the arc.
fn segment_is_blocked(facility: &Facility, origin: Cell, bx2: i64, by2: i64, target: Cell) -> bool {
    // Doubled cell-centre coordinates: every centre is odd, every boundary even.
    let ax = 2 * i64::from(origin.x) + 1;
    let ay = 2 * i64::from(origin.y) + 1;
    let vx = bx2 - ax;
    let vy = by2 - ay;

    // The pinch check: every vertex pass shows up as an x-boundary crossing
    // whose y lands on a boundary too (a vertical segment runs through cell
    // interiors and never meets a vertex).
    if vx != 0 && vy != 0 {
        let (sx, sy) = (vx.signum(), vy.signum());
        let (lo, hi) = (ax.min(bx2), ax.max(bx2));
        // Even (boundary) doubled-x values strictly between the endpoints; an
        // endpoint sitting on a boundary (a corner sample) is t = 1, excluded.
        let mut xd = if lo % 2 == 0 { lo + 2 } else { lo + 1 };
        while xd < hi {
            // The segment crosses x-boundary xd/2 at t = (xd − ax) / vx; the
            // doubled y there, scaled by vx to stay integer, is:
            let ynum = ay * vx + vy * (xd - ax);
            if ynum % vx == 0 {
                let y2 = ynum / vx;
                if y2 % 2 == 0 {
                    let (xb, yb) = (xd / 2, y2 / 2);
                    // The two cells framing the vertex diagonally across the
                    // segment's path — sides (sx, −sy) and (−sx, sy).
                    let f1x = if sx > 0 { xb } else { xb - 1 };
                    let f1y = if sy < 0 { yb } else { yb - 1 };
                    let f2x = if sx < 0 { xb } else { xb - 1 };
                    let f2y = if sy > 0 { yb } else { yb - 1 };
                    if blocks_sight_at(facility, f1x, f1y) && blocks_sight_at(facility, f2x, f2y) {
                        return true;
                    }
                }
            }
            xd += 2;
        }
    }

    // The body check. The parameters t in (0, 1) where the segment crosses a
    // cell boundary, as fractions num/den (den > 0). Between consecutive
    // crossings the segment lies wholly inside one cell.
    let mut crossings: Vec<(i64, i64)> = Vec::new();
    let mut push = |num: i64, den: i64| {
        let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
        if num > 0 && num < den {
            crossings.push((num, den));
        }
    };
    for (v, a, b2) in [(vx, ax, bx2), (vy, ay, by2)] {
        if v != 0 {
            let (lo, hi) = (a.min(b2), a.max(b2));
            let mut bd = if lo % 2 == 0 { lo + 2 } else { lo + 1 };
            while bd < hi {
                push(bd - a, v);
                bd += 2;
            }
        }
    }
    // Sort by value and fold coincident crossings — a corner is one point, not a
    // sliver of a third cell — so the midpoints below only ever land in cells
    // the segment truly traverses.
    crossings.sort_by(|&(an, ad), &(bn, bd)| (an * bd).cmp(&(bn * ad)));
    crossings.dedup_by(|&mut (an, ad), &mut (bn, bd)| an * bd == bn * ad);

    // Walk each interval's midpoint; the cell it lands in is one the segment
    // passes through. The origin and target cells are the endpoints, not a
    // crossing.
    let mut bounds = Vec::with_capacity(crossings.len() + 2);
    bounds.push((0, 1));
    bounds.extend(crossings);
    bounds.push((1, 1));
    for pair in bounds.windows(2) {
        let ((pn, pd), (cn, cd)) = (pair[0], pair[1]);
        // Midpoint tm = (prev + cur) / 2 = (pn·cd + cn·pd) / (2·pd·cd).
        let tn = pn * cd + cn * pd;
        let td = 2 * pd * cd;
        let cx = (ax * td + vx * tn).div_euclid(2 * td);
        let cy = (ay * td + vy * tn).div_euclid(2 * td);
        if cx < 0 || cy < 0 {
            continue;
        }
        let cell = Cell::new(cx as u32, cy as u32);
        if cell != origin
            && cell != target
            && facility.terrain(cell).is_none_or(|t| t.blocks_sight())
        {
            return true;
        }
    }
    false
}

/// The §6.2 tier of a viewer's ring neighbour at offset `(dx, dy)`, ranked by
/// angular deviation from `facing`: 1 directly ahead, 2 a forward diagonal, 3 a
/// side, 4 a rear diagonal, 5 directly behind. The neighbour is transparent iff
/// `arc_width >= tier`; otherwise it is one of the artificial walls that carve
/// the cone.
fn ring_tier(facing: Direction, dx: i64, dy: i64) -> u8 {
    let (fx, fy) = match facing {
        Direction::North => (0, -1),
        Direction::South => (0, 1),
        Direction::East => (1, 0),
        Direction::West => (-1, 0),
    };
    // The offset's component along facing: 1 leaning forward, 0 square-on, -1 back.
    let forward = dx * fx + dy * fy;
    let diagonal = dx != 0 && dy != 0;
    match (forward, diagonal) {
        (1, false) => 1,
        (1, true) => 2,
        (0, _) => 3, // only the two side cardinals can be square-on
        (-1, true) => 4,
        _ => 5, // (-1, false): directly behind
    }
}

/// One quarter of the box, opening north, east, south or west of the origin. Each
/// quadrant addresses its cells as `(depth, col)`: `depth` rows out from the viewer
/// along the cardinal, `col` sweeping `-depth..=depth` across the row — together
/// they tile the whole box, meeting on the diagonals.
#[derive(Clone, Copy)]
enum Quadrant {
    North,
    East,
    South,
    West,
}

impl Quadrant {
    const ALL: [Quadrant; 4] = [
        Quadrant::North,
        Quadrant::East,
        Quadrant::South,
        Quadrant::West,
    ];

    /// The grid cell at `(depth, col)` of this quadrant around `origin`, or `None`
    /// where that lands off the low edge of the grid (the high edge is caught by
    /// bounds checks later — coordinates are unsigned).
    fn transform(self, origin: Cell, depth: u32, col: i64) -> Option<Cell> {
        let (ox, oy) = (i64::from(origin.x), i64::from(origin.y));
        let d = i64::from(depth);
        let (x, y) = match self {
            Quadrant::North => (ox + col, oy - d),
            Quadrant::South => (ox + col, oy + d),
            Quadrant::East => (ox + d, oy + col),
            Quadrant::West => (ox - d, oy + col),
        };
        Some(Cell::new(u32::try_from(x).ok()?, u32::try_from(y).ok()?))
    }
}

/// A rational slope `num/den` with `den > 0` — the tangent of a sight ray within a
/// quadrant, kept exact so the cast is integer arithmetic end to end (§12.4).
#[derive(Clone, Copy)]
struct Slope {
    num: i64,
    den: i64,
}

impl Slope {
    fn new(num: i64, den: i64) -> Self {
        Self { num, den }
    }

    /// The slope of the ray grazing the near edge of the tile at `(depth, col)`:
    /// `(2·col − 1) / (2·depth)`. This is the boundary a wall tile hands to the
    /// rows behind it.
    fn of_tile(depth: u32, col: i64) -> Self {
        Self::new(2 * col - 1, 2 * i64::from(depth))
    }
}

/// The first column of a row at `depth` bounded below by `start`: `depth · start`,
/// rounded half up.
fn min_col(depth: u32, start: Slope) -> i64 {
    (2 * i64::from(depth) * start.num + start.den).div_euclid(2 * start.den)
}

/// The last column of a row at `depth` bounded above by `end`: `depth · end`,
/// rounded half down.
fn max_col(depth: u32, end: Slope) -> i64 {
    let doubled = 2 * i64::from(depth) * end.num - end.den;
    // Ceiling division by the (positive) doubled denominator.
    -((-doubled).div_euclid(2 * end.den))
}

/// Whether the tile centre at `(depth, col)` lies within `[start, end]` — the
/// symmetric-visibility test: a transparent tile is seen iff its centre is inside
/// the sector, which is exactly the condition under which it could see the origin
/// back. Walls are exempt (they are revealed whenever scanned — the "you see the
/// wall face" rule, §6.1).
fn is_symmetric(depth: u32, col: i64, start: Slope, end: Slope) -> bool {
    let d = i64::from(depth);
    col * start.den >= d * start.num && col * end.den <= d * end.num
}

/// The shadowcast context: one viewer, one facility, one arc.
struct Caster<'a> {
    facility: &'a Facility,
    origin: Cell,
    facing: Direction,
    arc_width: u8,
    range: u32,
}

impl Caster<'_> {
    /// Whether `cell` blocks sight from this viewer: real opacity from the terrain
    /// table (§10.3), the §6.2 artificial wall — an out-of-arc member of the
    /// viewer's own touching ring — or a pinched ring diagonal. Off-grid is opaque.
    fn opaque(&self, cell: Cell) -> bool {
        let dx = i64::from(cell.x) - i64::from(self.origin.x);
        let dy = i64::from(cell.y) - i64::from(self.origin.y);
        if dx.abs().max(dy.abs()) == 1 {
            if ring_tier(self.facing, dx, dy) > self.arc_width {
                return true;
            }
            // Corner-solidity at the viewer's own ring (§6.1): a diagonal
            // neighbour whose two shared cardinal neighbours are both real
            // walls sits behind a pinch. Treating it as opaque keeps it seen —
            // opaque cells are marked wherever scanned, so the **[SETTLED]**
            // touching ring holds — while the cast stops at it instead of
            // spilling floors *and wall faces* into the space beyond, which
            // only the measure-zero corner line actually reaches. This is the
            // alcove-cupboard geometry: backing and wall line meet diagonally
            // at the mouth's corners.
            if dx != 0
                && dy != 0
                && blocks_sight_at(
                    self.facility,
                    i64::from(self.origin.x) + dx,
                    i64::from(self.origin.y),
                )
                && blocks_sight_at(
                    self.facility,
                    i64::from(self.origin.x),
                    i64::from(self.origin.y) + dy,
                )
            {
                return true;
            }
        }
        self.facility.terrain(cell).is_none_or(|t| t.blocks_sight())
    }

    /// Scan one row of `quadrant` at `depth`, seeing floors inside `[start, end]`
    /// and walls wherever scanned, and recurse behind every gap between walls. The
    /// recursion is bounded by `range`, so the whole cast touches at most the
    /// square box (§6.1).
    fn scan(&self, quadrant: Quadrant, fov: &mut VisibleSet, depth: u32, start: Slope, end: Slope) {
        if depth > self.range {
            return;
        }
        // The row's lower bound tightens as walls interrupt it; the upper bound only
        // ever spawns narrower child rows.
        let mut start = start;
        let mut prev_opaque: Option<bool> = None;
        for col in min_col(depth, start)..=max_col(depth, end) {
            let cell = quadrant.transform(self.origin, depth, col);
            let opaque = cell.is_none_or(|c| self.opaque(c));
            if opaque || is_symmetric(depth, col, start, end) {
                if let Some(c) = cell {
                    fov.mark(c);
                }
            }
            if prev_opaque == Some(true) && !opaque {
                // A gap opens after a wall: sight resumes at this tile's near edge.
                start = Slope::of_tile(depth, col);
            }
            if prev_opaque == Some(false) && opaque {
                // A wall closes a gap: cast on behind the open span just finished.
                self.scan(quadrant, fov, depth + 1, start, Slope::of_tile(depth, col));
            }
            prev_opaque = Some(opaque);
        }
        if prev_opaque == Some(false) {
            // The row ended open: the remaining sector carries straight on.
            self.scan(quadrant, fov, depth + 1, start, end);
        }
    }
}

#[cfg(test)]
mod tests;
