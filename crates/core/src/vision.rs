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
//! **[SETTLED]**. That touching ring is load-bearing: you can never stand adjacent to
//! a guard undetected, so sneaking up behind someone is never free (§6.1/§7.2).
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
/// The arc for a turn spent waiting (§8.3): width 5, the full 360° — waiting is the
/// only way to see behind you (§5).
pub const WAIT_SIGHT_ARC: u8 = 5;
/// A guard's sight range (§7.1) — a 21×21 box. **[START]**
pub const GUARD_SIGHT_RANGE: u32 = 10;
/// A guard's sight arc (§7.1/§6.2): width 2, the ~90° forward wedge. **[START]**
pub const GUARD_SIGHT_ARC: u8 = 2;

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
mod tests {
    use super::*;
    use crate::facility::Terrain;

    /// An open `w × h` walled box — all interior floor.
    fn open(w: u32, h: u32) -> Facility {
        Facility::walled_box(w, h)
    }

    /// The Chebyshev (square-box) distance between two cells: the §6.1 range metric.
    fn chebyshev(a: Cell, b: Cell) -> u32 {
        a.sight_distance(b)
    }

    /// Render a field of view as text for golden tests: `@` the viewer, `*` a seen
    /// cell, `.` an unseen one. Walls are drawn `#` when seen so shadows read.
    fn picture(facility: &Facility, fov: &VisibleSet, origin: Cell) -> Vec<String> {
        (0..facility.height())
            .map(|y| {
                (0..facility.width())
                    .map(|x| {
                        let cell = Cell::new(x, y);
                        if cell == origin {
                            '@'
                        } else if !fov.contains(cell) {
                            '.'
                        } else if facility.terrain(cell) == Some(Terrain::Wall) {
                            '#'
                        } else {
                            '*'
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// §6.1: range is a square box, no falloff. With the full 360° arc on open
    /// floor, the visible set is *exactly* the Chebyshev-≤-range box.
    #[test]
    fn full_arc_on_open_floor_sees_exactly_the_range_box() {
        let f = open(11, 11);
        let origin = Cell::new(5, 5);
        let fov = field_of_view(&f, origin, Direction::North, WAIT_SIGHT_ARC, 3);
        for y in 0..f.height() {
            for x in 0..f.width() {
                let cell = Cell::new(x, y);
                assert_eq!(
                    fov.contains(cell),
                    chebyshev(origin, cell) <= 3,
                    "({x},{y}) against the range-3 box"
                );
            }
        }
    }

    /// §6.1 **[SETTLED]**: the 8 cells around a viewer are always seen, in every
    /// direction — even at arc width 1, and even the cell directly behind. Two
    /// steps directly behind, though, stays dark at every arc short of 360°.
    #[test]
    fn the_touching_ring_is_always_seen() {
        let f = open(11, 11);
        let origin = Cell::new(5, 5);
        for facing in Direction::ALL {
            for arc in 1..=5u8 {
                let fov = field_of_view(&f, origin, facing, arc, 4);
                for n in f.neighbours(origin) {
                    assert!(fov.contains(n), "{facing:?} arc {arc}: cardinal ring");
                }
                for (dx, dy) in [(-1i64, -1i64), (1, -1), (-1, 1), (1, 1)] {
                    let c = Cell::new(
                        (i64::from(origin.x) + dx) as u32,
                        (i64::from(origin.y) + dy) as u32,
                    );
                    assert!(fov.contains(c), "{facing:?} arc {arc}: diagonal ring");
                }
            }
        }
        // Directly behind at distance 2: dark for every partial arc, lit at 360°.
        let behind = Cell::new(5, 7); // facing north, behind is south
        for arc in 1..=4u8 {
            let fov = field_of_view(&f, origin, Direction::North, arc, 4);
            assert!(!fov.contains(behind), "arc {arc} must not see behind");
        }
        let fov = field_of_view(&f, origin, Direction::North, WAIT_SIGHT_ARC, 4);
        assert!(fov.contains(behind), "the 360° wait arc sees behind");
    }

    /// The §6.2 arc table, pinned as golden pictures: one viewer mid-floor, facing
    /// north, range 4, at every arc width. This is the arc_width ↔ tier rule made
    /// visible — the cone widens tier by tier, and the touching ring is present in
    /// every picture. The edges are grazes past the square angle (the ray slipping
    /// past a transparent side neighbour), which is the trick's real silhouette.
    #[test]
    fn golden_cone_shapes_per_arc_width() {
        let f = open(11, 11);
        let origin = Cell::new(5, 5);
        let shot = |arc: u8| {
            let fov = field_of_view(&f, origin, Direction::North, arc, 4);
            picture(&f, &fov, origin)
        };

        // Arc 1 — ahead only: the beam through the single transparent cell, widening
        // with distance like any 1-cell gap.
        assert_eq!(
            shot(1),
            vec![
                "...........",
                "...*****...",
                "....***....",
                "....***....",
                "....***....",
                "....*@*....",
                "....***....",
                "...........",
                "...........",
                "...........",
                "...........",
            ]
        );
        // Arc 2 — the guard's ~90° forward wedge.
        assert_eq!(
            shot(2),
            vec![
                "...........",
                ".*********.",
                ".*********.",
                ".*********.",
                "...*****...",
                "....*@*....",
                "....***....",
                "...........",
                "...........",
                "...........",
                "...........",
            ]
        );
        // Arc 3 — the player's ~180° half-disc, with the rear skirt the side
        // neighbours let sight graze into.
        assert_eq!(
            shot(3),
            vec![
                "...........",
                ".*********.",
                ".*********.",
                ".*********.",
                ".*********.",
                ".****@****.",
                ".*********.",
                ".*.......*.",
                "...........",
                "...........",
                "...........",
            ]
        );
        // Arc 4 — ~270°: only the shadow of the directly-behind cell stays dark.
        assert_eq!(
            shot(4),
            vec![
                "...........",
                ".*********.",
                ".*********.",
                ".*********.",
                ".*********.",
                ".****@****.",
                ".*********.",
                ".****.****.",
                ".***...***.",
                ".***...***.",
                "...........",
            ]
        );
        // Arc 5 — 360°: the full range box (§6.1).
        assert_eq!(
            shot(5),
            vec![
                "...........",
                ".*********.",
                ".*********.",
                ".*********.",
                ".*********.",
                ".****@****.",
                ".*********.",
                ".*********.",
                ".*********.",
                ".*********.",
                "...........",
            ]
        );
    }

    /// The cone follows the facing: the guard wedge pointed east is the north wedge
    /// rotated, pinned as its own golden so a rotation bug cannot hide.
    #[test]
    fn the_cone_rotates_with_facing() {
        let f = open(11, 11);
        let origin = Cell::new(5, 5);
        let fov = field_of_view(&f, origin, Direction::East, GUARD_SIGHT_ARC, 4);
        assert_eq!(
            picture(&f, &fov, origin),
            vec![
                "...........",
                ".......***.",
                ".......***.",
                "......****.",
                "....******.",
                "....*@****.",
                "....******.",
                "......****.",
                ".......***.",
                ".......***.",
                "...........",
            ]
        );
    }

    /// §6.1: an opaque cell is itself seen — you see the wall face — but shadows
    /// everything behind it. A free-standing wall stub ahead of the viewer.
    #[test]
    fn an_opaque_cell_is_seen_and_shadows_behind_itself() {
        let mut f = open(11, 11);
        f.set_terrain(5, 3, Terrain::Wall); // two ahead of the viewer
        let origin = Cell::new(5, 5);
        let fov = field_of_view(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 4);

        assert!(fov.contains(Cell::new(5, 3)), "the wall face is seen");
        assert!(
            !fov.contains(Cell::new(5, 2)) && !fov.contains(Cell::new(5, 1)),
            "the cells behind the wall are shadowed"
        );
        assert!(
            fov.contains(Cell::new(4, 2)) && fov.contains(Cell::new(6, 2)),
            "the shadow is the wall's, not the whole row's"
        );
    }

    /// §10.3 through the caster's eyes: a closed door panel and a hinge block
    /// sight; opening the panel opens the view. The door terrain carries the
    /// opacity; vision just reads the table.
    #[test]
    fn doors_block_sight_closed_and_pass_it_open() {
        let mut f = open(11, 11);
        for terrain in [Terrain::DoorPanelClosed, Terrain::DoorHinge] {
            f.set_terrain(5, 3, terrain);
            let fov = field_of_view(&f, Cell::new(5, 5), Direction::North, PLAYER_SIGHT_ARC, 4);
            assert!(fov.contains(Cell::new(5, 3)), "{terrain:?} face is seen");
            assert!(!fov.contains(Cell::new(5, 1)), "{terrain:?} blocks sight");
        }
        f.set_terrain(5, 3, Terrain::DoorPanelOpen);
        let fov = field_of_view(&f, Cell::new(5, 5), Direction::North, PLAYER_SIGHT_ARC, 4);
        assert!(fov.contains(Cell::new(5, 1)), "an open panel passes sight");
    }

    /// The symmetry the algorithm is named for: between transparent cells, at the
    /// 360° arc, A sees B iff B sees A. Checked over every floor pair of a room
    /// with a wall stub — the geometry that would catch a one-sided caster.
    #[test]
    fn vision_is_symmetric_between_open_cells() {
        let mut f = open(9, 9);
        for y in 2..=5 {
            f.set_terrain(4, y, Terrain::Wall);
        }
        let floors: Vec<Cell> = (1..8)
            .flat_map(|y| (1..8).map(move |x| Cell::new(x, y)))
            .filter(|&c| f.terrain(c) == Some(Terrain::Floor))
            .collect();
        for &a in &floors {
            let from_a = field_of_view(&f, a, Direction::North, WAIT_SIGHT_ARC, 8);
            for &b in &floors {
                let from_b = field_of_view(&f, b, Direction::North, WAIT_SIGHT_ARC, 8);
                assert_eq!(
                    from_a.contains(b),
                    from_b.contains(a),
                    "asymmetry between {a:?} and {b:?}"
                );
            }
        }
    }

    /// A viewer against the level edge casts without panicking and simply loses
    /// the off-grid part of its box — the border absorbs the cone.
    #[test]
    fn a_viewer_in_a_corner_is_bounded_by_the_grid() {
        let f = open(6, 6);
        let fov = field_of_view(&f, Cell::new(1, 1), Direction::North, WAIT_SIGHT_ARC, 10);
        assert!(
            fov.contains(Cell::new(0, 0)),
            "the corner wall face is seen"
        );
        assert!(fov.contains(Cell::new(4, 4)));
        assert!(!fov.contains(Cell::new(5, 5).step(Direction::East).unwrap()));
    }

    /// §6.1 corner-solidity: two walls meeting only at a diagonal must jointly
    /// occlude the pinch between them. A viewer looking straight along the join
    /// used to see floor cells whose line of sight is a clean run *through* a
    /// wall body — the classic diagonal corner peek. Pinned as a golden picture:
    /// viewer at (1,1) waiting (360°), walls touching at (3,3)+(4,4). The cells
    /// hidden behind the join — (4,3),(3,4),(5,4),(4,5) — must be dark, while the
    /// wall faces and everything genuinely in view stay lit.
    #[test]
    fn diagonal_corner_does_not_leak_sight_through_the_pinch() {
        let mut f = open(11, 11);
        f.set_terrain(3, 3, Terrain::Wall);
        f.set_terrain(4, 4, Terrain::Wall);
        let origin = Cell::new(1, 1);
        let fov = field_of_view(&f, origin, Direction::North, WAIT_SIGHT_ARC, 10);
        assert_eq!(
            picture(&f, &fov, origin),
            vec![
                "###########",
                "#@********#",
                "#*********#",
                "#**#.*****#",
                "#**...****#",
                "#***....**#",
                "#****.....#",
                "#****......",
                "#*****.....",
                "#*****.....",
                "#######....",
            ]
        );
        // The specific cells behind the diagonal join go dark (they were the leak).
        for c in [
            Cell::new(4, 3),
            Cell::new(3, 4),
            Cell::new(5, 4),
            Cell::new(4, 5),
        ] {
            assert!(!fov.contains(c), "{c:?} leaked through the corner");
        }
        // The near wall face is still seen — corner-solidity hides what is behind
        // the pinch, never the wall the viewer looks at (§6.1). The far wall
        // (4,4) is legitimately shadowed by the nearer wall (3,3) directly in
        // front of it on the diagonal.
        assert!(fov.contains(Cell::new(3, 3)), "the near wall face is seen");
        assert!(
            !fov.contains(Cell::new(4, 4)),
            "the far wall is shadowed by the near one"
        );
    }

    /// The corner fix closes the leak *both* ways: no floor cell the cast lights
    /// has its centre-to-centre line of sight buried in a wall body, checked
    /// exhaustively over every two-wall diagonal L-corner and viewer position,
    /// against an independent floating-point ray as the oracle (tests may use
    /// floats; the shipped path stays integer, §12.4).
    #[test]
    fn no_floor_cell_is_seen_through_a_wall_body() {
        // Independent oracle (a different method than the shipped integer walk):
        // clip the centre-to-centre segment against each wall cell's square,
        // slightly shrunk so a grazed corner or edge does not register — a
        // positive-length overlap means the line runs through the wall's body.
        fn body_blocked(walls: &[Cell], a: Cell, c: Cell) -> bool {
            let (px, py) = (f64::from(a.x) + 0.5, f64::from(a.y) + 0.5);
            let (dx, dy) = (f64::from(c.x) + 0.5 - px, f64::from(c.y) + 0.5 - py);
            let e = 0.02;
            walls.iter().any(|&w| {
                // Liang–Barsky clip against [wx+e, wx+1-e] × [wy+e, wy+1-e].
                let edges = [
                    (-dx, px - (f64::from(w.x) + e)),
                    (dx, (f64::from(w.x) + 1.0 - e) - px),
                    (-dy, py - (f64::from(w.y) + e)),
                    (dy, (f64::from(w.y) + 1.0 - e) - py),
                ];
                let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
                for (p, q) in edges {
                    if p.abs() < 1e-12 {
                        if q < 0.0 {
                            return false;
                        }
                    } else {
                        let r = q / p;
                        if p < 0.0 {
                            if r > t1 {
                                return false;
                            }
                            t0 = t0.max(r);
                        } else {
                            if r < t0 {
                                return false;
                            }
                            t1 = t1.min(r);
                        }
                    }
                }
                t1 - t0 > 1e-9
            })
        }
        for wx in 3..8 {
            for wy in 3..8 {
                for &(ddx, ddy) in &[(1i64, 1i64), (1, -1), (-1, 1), (-1, -1)] {
                    let mut f = open(11, 11);
                    f.set_terrain(wx, wy, Terrain::Wall);
                    let (bx, by) = ((wx as i64 + ddx) as u32, (wy as i64 + ddy) as u32);
                    f.set_terrain(bx, by, Terrain::Wall);
                    let walls: Vec<Cell> = (0..f.height())
                        .flat_map(|y| (0..f.width()).map(move |x| Cell::new(x, y)))
                        .filter(|&c| f.terrain(c) == Some(Terrain::Wall))
                        .collect();
                    for oy in 1..10 {
                        for ox in 1..10 {
                            let o = Cell::new(ox, oy);
                            if f.terrain(o) != Some(Terrain::Floor) {
                                continue;
                            }
                            let fov = field_of_view(&f, o, Direction::North, WAIT_SIGHT_ARC, 7);
                            for c in fov.cells() {
                                if f.terrain(c) == Some(Terrain::Floor) {
                                    assert!(
                                        !body_blocked(&walls, o, c),
                                        "viewer {o:?} sees floor {c:?} through walls \
                                         {:?}+{:?}",
                                        Cell::new(wx, wy),
                                        Cell::new(bx, by)
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// The #88 reopen fixture: the PR #119 alcove cupboard. A room above a
    /// 1-thick wall line, a corridor below; one room cell walled up as backing
    /// and the wall cell under it recessed into a cupboard. The backing and the
    /// wall line meet only diagonally at the mouth's corners — the pinch the
    /// leak threaded.
    ///
    /// ```text
    /// #################
    /// #...............#
    /// #.......#.......#   <- walled-up backing (8,2)
    /// ########}########   <- wall line, } = cupboard (8,3)
    /// #...............#
    /// #...............#
    /// #################
    /// #################
    /// ```
    fn alcove_cupboard() -> Facility {
        let mut f = Facility::walled_box(17, 8);
        for x in 1..16 {
            f.set_terrain(x, 3, Terrain::Wall);
            f.set_terrain(x, 6, Terrain::Wall);
        }
        f.set_terrain(8, 3, Terrain::Hideout);
        f.set_terrain(8, 2, Terrain::Wall);
        f
    }

    /// §6.1 corner-solidity at the cupboard mouth: a viewer hidden in the
    /// alcove sees the corridor out the mouth and its own touching ring —
    /// including the two room cells diagonally behind the backing, which the
    /// **[SETTLED]** ring keeps lit — but nothing else of the room. Before the
    /// fix the cast threaded the double-walled corners and lit the room's
    /// deeper floor and far border even at the 360° wait arc.
    #[test]
    fn a_cupboard_alcove_does_not_leak_sight_into_the_room() {
        let f = alcove_cupboard();
        let origin = Cell::new(8, 3);
        let fov = field_of_view(&f, origin, Direction::South, WAIT_SIGHT_ARC, 15);
        assert_eq!(
            picture(&f, &fov, origin),
            vec![
                ".................",
                ".................",
                ".......*#*.......",
                ".......#@#.......",
                ".......***.......",
                "......*****......",
                ".....#######.....",
                ".................",
            ]
        );
        // The exact-diagonal cells beyond the pinch were the floor leak.
        for c in [Cell::new(6, 1), Cell::new(10, 1)] {
            assert!(!fov.contains(c), "{c:?} leaked through the mouth corner");
        }
        // Facing out the mouth (the §7.6 auto-face) shows exactly the same
        // view: the alcove already contains everything the wait arc can add.
        let facing = field_of_view(&f, origin, Direction::South, PLAYER_SIGHT_ARC, 15);
        assert_eq!(picture(&f, &facing, origin), picture(&f, &fov, origin));
    }

    /// The same pinch closed from the outside: a guard in the room diagonally
    /// behind the backing still sees the cupboard interior itself — it is in
    /// the guard's touching ring, and standing next to a guard is never free
    /// (§6.1 **[SETTLED]**) — but no longer sees the corridor beyond it.
    #[test]
    fn a_guard_diagonal_to_the_backing_cannot_see_past_the_cupboard() {
        let f = alcove_cupboard();
        let origin = Cell::new(7, 2);
        let fov = field_of_view(&f, origin, Direction::South, GUARD_SIGHT_ARC, 10);
        assert_eq!(
            picture(&f, &fov, origin),
            vec![
                ".................",
                "......***........",
                "......*@#........",
                "......##*........",
                ".................",
                ".................",
                ".................",
                ".................",
            ]
        );
        assert!(
            fov.contains(Cell::new(8, 3)),
            "the adjacent cupboard interior stays seen — the touching ring"
        );
        for c in [Cell::new(9, 4), Cell::new(10, 5)] {
            assert!(!fov.contains(c), "{c:?}: the corridor leaked to the guard");
        }
    }

    /// The playtest leak that reopened the reopen: two alcove cupboards facing
    /// each other across a corridor. From inside one, the room behind the
    /// *opposite* cupboard must stay dark — the alcove is a dead end: its
    /// backing blocks the straight line and its mouth corners are double-walled
    /// pinches. The raw cast fans through the one-cell gap and paints the far
    /// room's wall faces (floors were already retracted); the corner-sampled
    /// wall retraction now darkens those too. What legitimately remains: the
    /// opposite alcove's interior and its backing's face — you see into the
    /// recess, never through it.
    #[test]
    fn a_cupboard_across_the_corridor_is_a_dead_end_not_a_window() {
        let mut f = Facility::walled_box(17, 10);
        for x in 1..16 {
            f.set_terrain(x, 3, Terrain::Wall);
            f.set_terrain(x, 6, Terrain::Wall);
        }
        // Top cupboard opens south into the corridor; backing walled up above.
        f.set_terrain(8, 3, Terrain::Hideout);
        f.set_terrain(8, 2, Terrain::Wall);
        // Bottom cupboard opens north; backing walled up below. The viewer.
        f.set_terrain(8, 6, Terrain::Hideout);
        f.set_terrain(8, 7, Terrain::Wall);
        let origin = Cell::new(8, 6);
        let fov = field_of_view(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 15);

        // Into the recess: the opposite interior and its backing's face.
        assert!(
            fov.contains(Cell::new(8, 3)),
            "the opposite alcove interior"
        );
        assert!(fov.contains(Cell::new(8, 2)), "the opposite backing's face");
        // Never through it: the room beyond stays dark — floors, the walls of
        // its far border, and the backing's room-side neighbours alike.
        for c in [
            Cell::new(7, 1),
            Cell::new(8, 1),
            Cell::new(9, 1),
            Cell::new(6, 1),
            Cell::new(10, 1),
            Cell::new(7, 2),
            Cell::new(9, 2),
            Cell::new(7, 0),
            Cell::new(8, 0),
            Cell::new(9, 0),
        ] {
            assert!(!fov.contains(c), "{c:?} shows through the opposite alcove");
        }
        // The viewer's own touching ring is intact (§6.1 [SETTLED]), including
        // the room cells diagonally behind their own backing.
        for c in [Cell::new(7, 7), Cell::new(9, 7), Cell::new(7, 5)] {
            assert!(fov.contains(c), "{c:?}: the touching ring must hold");
        }
    }

    /// The strictness choice, pinned: a vertex flanked by **two** walls blocks
    /// the diagonal through it (they jointly occlude the pinch), while a lone
    /// corner still grazes — the permissive behaviour the cone silhouette and
    /// the touching ring depend on. And the closure is symmetric: dark from
    /// one side means dark from the other.
    #[test]
    fn a_double_walled_corner_blocks_the_diagonal_a_lone_corner_grazes() {
        let mut f = open(11, 11);
        f.set_terrain(5, 4, Terrain::Wall);
        f.set_terrain(4, 5, Terrain::Wall);
        let a = Cell::new(2, 2);
        let from_a = field_of_view(&f, a, Direction::North, WAIT_SIGHT_ARC, 8);
        for c in [Cell::new(5, 5), Cell::new(6, 6), Cell::new(7, 7)] {
            assert!(!from_a.contains(c), "{c:?} threaded the double corner");
        }
        assert!(from_a.contains(Cell::new(4, 4)), "this side of the pinch");
        assert!(from_a.contains(Cell::new(5, 4)), "the wall faces are seen");
        assert!(from_a.contains(Cell::new(4, 5)), "the wall faces are seen");
        let b = Cell::new(6, 6);
        let from_b = field_of_view(&f, b, Direction::North, WAIT_SIGHT_ARC, 8);
        assert!(!from_b.contains(a), "the closure holds both ways");

        // Remove one wall: the vertex has a single opaque flank and the
        // diagonal grazes it freely again.
        f.set_terrain(4, 5, Terrain::Floor);
        let grazing = field_of_view(&f, a, Direction::North, WAIT_SIGHT_ARC, 8);
        assert!(
            grazing.contains(Cell::new(5, 5)),
            "a lone corner never hides"
        );
        assert!(
            grazing.contains(Cell::new(6, 6)),
            "a lone corner never hides"
        );
    }

    /// An interior filled with wall, with corridors carved into it — the pinched
    /// geometry the #121 auto-peek exists for.
    fn carved(w: u32, h: u32, floors: &[(u32, u32)]) -> Facility {
        let mut f = Facility::walled_box(w, h);
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                f.set_terrain(x, y, Terrain::Wall);
            }
        }
        for &(x, y) in floors {
            f.set_terrain(x, y, Terrain::Floor);
        }
        f
    }

    /// An L-corner: a vertical corridor meeting a horizontal arm at (3,3).
    fn l_corridor() -> Facility {
        let mut floors = Vec::new();
        floors.extend((3..=8).map(|y| (3, y)));
        floors.extend((3..=8).map(|x| (x, 3)));
        carved(11, 11, &floors)
    }

    /// #121 auto-peek: on open floor the union adds nothing. The lean cast is
    /// clipped to the origin's own range box, so leaning can only re-reveal
    /// what geometry hides — it never extends reach or widens the arc where
    /// nothing occludes.
    #[test]
    fn on_open_floor_the_peek_changes_nothing() {
        let f = open(11, 11);
        let origin = Cell::new(5, 5);
        for arc in [PLAYER_SIGHT_ARC, WAIT_SIGHT_ARC] {
            let plain = field_of_view(&f, origin, Direction::North, arc, 4);
            let peek = field_of_view_with_peek(&f, origin, Direction::North, arc, 4);
            assert_eq!(
                picture(&f, &plain, origin),
                picture(&f, &peek, origin),
                "arc {arc}: open floor must gain nothing from the peek"
            );
        }
    }

    /// #121 auto-peek at an L-corner, pinned as goldens: standing one cell
    /// short of the corner and facing it, the head-lean origin *is* the corner
    /// cell, so the peek reads down the cross arm the corner walls hide from
    /// the body's own cast. The plain cast keeps only the arm cell diagonally
    /// ahead (the lean origin's ring reaches no further than the touching
    /// ring's own reach).
    #[test]
    fn peeking_at_an_l_corner_reads_down_the_cross_arm() {
        let f = l_corridor();
        let origin = Cell::new(3, 4);
        let plain = field_of_view(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 8);
        let peek = field_of_view_with_peek(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 8);
        assert_eq!(
            picture(&f, &plain, origin),
            vec![
                "...........",
                "...........",
                "..####.....",
                "..#**......",
                "..#@#......",
                "..#*#......",
                "...........",
                "...........",
                "...........",
                "...........",
                "...........",
            ]
        );
        assert_eq!(
            picture(&f, &peek, origin),
            vec![
                "...........",
                "...........",
                "..########.",
                "..#******#.",
                "..#@######.",
                "..#*#......",
                "...........",
                "...........",
                "...........",
                "...........",
                "...........",
            ]
        );
        // The cross arm beyond the diagonal is the peek's delta.
        for x in 5..=8 {
            let c = Cell::new(x, 3);
            assert!(!plain.contains(c), "{c:?} hidden from the body's cast");
            assert!(peek.contains(c), "{c:?} revealed by the lean");
        }
    }

    /// #121 auto-peek at a T-junction: one cell short of the junction, facing
    /// the stem's end, the lean origin sits in the junction and reads both
    /// arms at once — the ~180° the ticket promises, from a corridor instead
    /// of a cupboard.
    #[test]
    fn peeking_at_a_t_junction_reads_both_arms() {
        let mut floors = Vec::new();
        floors.extend((1..=9).map(|x| (x, 3)));
        floors.extend((3..=8).map(|y| (5, y)));
        let f = carved(11, 11, &floors);
        let origin = Cell::new(5, 4);
        let plain = field_of_view(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 8);
        let peek = field_of_view_with_peek(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 8);
        for x in [1, 2, 8, 9] {
            let c = Cell::new(x, 3);
            assert!(!plain.contains(c), "{c:?} hidden from the body's cast");
            assert!(peek.contains(c), "{c:?} revealed by the lean");
        }
    }

    /// #121: a sight-blocking forward cell refuses the lean — you cannot put
    /// your head into a wall or a closed door panel — and the peek collapses to
    /// the plain cast exactly.
    #[test]
    fn a_blocked_forward_cell_means_no_lean() {
        for terrain in [Terrain::Wall, Terrain::DoorPanelClosed, Terrain::DoorHinge] {
            let mut f = open(11, 11);
            f.set_terrain(5, 4, terrain);
            let origin = Cell::new(5, 5);
            for arc in [PLAYER_SIGHT_ARC, WAIT_SIGHT_ARC] {
                let plain = field_of_view(&f, origin, Direction::North, arc, 8);
                let peek = field_of_view_with_peek(&f, origin, Direction::North, arc, 8);
                assert_eq!(
                    picture(&f, &plain, origin),
                    picture(&f, &peek, origin),
                    "{terrain:?} arc {arc}: no lean into a blocked cell"
                );
            }
        }
    }

    /// #121: the peek never escapes the origin's range box — the §6.1 promise
    /// "range R sees at most the (2R+1)² box" holds for the union too.
    #[test]
    fn the_peek_stays_inside_the_origin_range_box() {
        let f = l_corridor();
        let origin = Cell::new(3, 4);
        for arc in [PLAYER_SIGHT_ARC, WAIT_SIGHT_ARC] {
            let fov = field_of_view_with_peek(&f, origin, Direction::North, arc, 3);
            for c in fov.cells() {
                assert!(
                    chebyshev(origin, c) <= 3,
                    "arc {arc}: {c:?} escaped the range-3 box"
                );
            }
        }
    }

    /// #121, the flagship case: hidden in the alcove cupboard, facing out (the
    /// §7.6 auto-face), the head leans through the mouth and the corridor reads
    /// at ~180° — both directions to the range box — where the plain cast gets
    /// only the mouth's ~90° wedge (pinned by
    /// `a_cupboard_alcove_does_not_leak_sight_into_the_room` above). The room
    /// behind the backing stays exactly as dark as the plain cast leaves it:
    /// leaning *out* opens nothing *inward*.
    #[test]
    fn hidden_in_a_cupboard_the_peek_reads_the_whole_corridor() {
        let f = alcove_cupboard();
        let origin = Cell::new(8, 3);
        let peek = field_of_view_with_peek(&f, origin, Direction::South, PLAYER_SIGHT_ARC, 15);
        assert_eq!(
            picture(&f, &peek, origin),
            vec![
                ".................",
                ".................",
                ".......*#*.......",
                "########@########",
                "#***************#",
                "#***************#",
                "#################",
                ".................",
            ]
        );
        // Both corridor directions, far past the mouth wedge.
        for c in [
            Cell::new(1, 4),
            Cell::new(15, 4),
            Cell::new(3, 5),
            Cell::new(13, 5),
        ] {
            assert!(peek.contains(c), "{c:?}: the corridor must read both ways");
        }
        // The room stays dark — the peek widens the corridor, not the pinch.
        for c in [
            Cell::new(6, 1),
            Cell::new(10, 1),
            Cell::new(7, 1),
            Cell::new(9, 1),
        ] {
            assert!(
                !peek.contains(c),
                "{c:?}: leaning out must not open the room"
            );
        }
    }

    /// A default set is the empty placeholder: it contains nothing.
    #[test]
    fn a_default_visible_set_is_empty() {
        let set = VisibleSet::default();
        assert!(!set.contains(Cell::new(0, 0)));
        assert_eq!(set.cells().count(), 0);
    }
}
