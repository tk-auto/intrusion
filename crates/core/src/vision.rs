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
    fov
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
    /// table (§10.3), or the §6.2 artificial wall — an out-of-arc member of the
    /// viewer's own touching ring. Off-grid is opaque.
    fn opaque(&self, cell: Cell) -> bool {
        let dx = i64::from(cell.x) - i64::from(self.origin.x);
        let dy = i64::from(cell.y) - i64::from(self.origin.y);
        if dx.abs().max(dy.abs()) == 1 && ring_tier(self.facing, dx, dy) > self.arc_width {
            return true;
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
        a.x.abs_diff(b.x).max(a.y.abs_diff(b.y))
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
                for n in f.neighbors(origin) {
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

    /// A default set is the empty placeholder: it contains nothing.
    #[test]
    fn a_default_visible_set_is_empty() {
        let set = VisibleSet::default();
        assert!(!set.contains(Cell::new(0, 0)));
        assert_eq!(set.cells().count(), 0);
    }
}
