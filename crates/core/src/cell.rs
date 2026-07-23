//! A cell coordinate: the address of one square on the facility grid (§4.1).
//!
//! The grid is integer cells with the origin at the top-left — `(0, 0)` is the
//! north-west corner, `x` grows east, `y` grows south — the same convention the
//! [`Facility`](crate::Facility) already uses. This is deliberately *just* a
//! coordinate: no terrain, no occupancy, no distance metric. Those belong to the
//! grid/occupancy model (a separate ticket); everything that only needs to name
//! a square can lean on this without pulling that weight in.

/// The address of one grid square, `(x, y)` from the top-left origin (§4.1).
///
/// A plain value type — copy it freely. It carries no notion of grid bounds, so
/// a `Cell` can name a coordinate that no particular facility contains; whoever
/// holds the grid is responsible for bounds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Cell {
    /// Column, growing east from the west wall.
    pub x: u32,
    /// Row, growing south from the north wall.
    pub y: u32,
}

impl Cell {
    /// The cell at column `x`, row `y`.
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    /// The Manhattan distance — the number of 4-directional steps — to `other`
    /// (§4.1). This is the game's distance metric everywhere except sight range,
    /// which uses a square box instead (§6.1).
    pub fn manhattan_distance(self, other: Cell) -> u32 {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }

    /// The Chebyshev (square-box) distance — the larger of the two axis gaps — to
    /// `other`. This is the sight metric, not the movement one (§6.1): a viewer of
    /// range *R* sees the `(2R+1)²` box, so this is the distance the §7.6 detection
    /// zones (certain ≤ 5, glimpse ≤ 10) are measured in, matching how the cone is
    /// cast. Use [`manhattan_distance`](Self::manhattan_distance) for anything about
    /// *walking* — the two differ on diagonals.
    pub fn sight_distance(self, other: Cell) -> u32 {
        self.x.abs_diff(other.x).max(self.y.abs_diff(other.y))
    }

    /// The neighbouring cell one step in `dir`, or `None` when that step would
    /// leave the grid's north or west edge — coordinates are unsigned, so there
    /// is no cell there to name. Stepping east or south always yields a `Cell`;
    /// whether *that* cell is in bounds is the grid's call, so callers that need
    /// bounds should go through [`Facility::neighbours`](crate::Facility::neighbours).
    pub fn step(self, dir: Direction) -> Option<Cell> {
        let cell = match dir {
            Direction::North => Cell::new(self.x, self.y.checked_sub(1)?),
            Direction::South => Cell::new(self.x, self.y + 1),
            Direction::East => Cell::new(self.x + 1, self.y),
            Direction::West => Cell::new(self.x.checked_sub(1)?, self.y),
        };
        Some(cell)
    }
}

/// One of the four cardinal directions movement is allowed in (§4.1).
///
/// There is deliberately no diagonal variant: 4-directional movement is
/// **[SETTLED]**, and the *absence* of a diagonal here is what makes "no diagonal
/// path anywhere" structural — nothing built on [`Cell::step`] or
/// [`Facility::neighbours`](crate::Facility::neighbours) can travel diagonally,
/// because there is no diagonal to travel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    /// Decreasing `y` — toward the north wall.
    North,
    /// Increasing `x` — toward the east wall.
    East,
    /// Increasing `y` — toward the south wall.
    South,
    /// Decreasing `x` — toward the west wall.
    West,
}

impl Direction {
    /// The four directions, clockwise from north. Iterating this is the canonical
    /// way to visit a cell's cardinal neighbours.
    pub const ALL: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];

    /// The direction facing the opposite way — north↔south, east↔west. Turning
    /// around, in other words; useful for reasoning about the far side of a panel
    /// or where a step came from.
    pub fn opposite(self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
        }
    }

    /// The two directions at right angles to this one — the axis this direction
    /// does *not* lie on. Returned east-then-west for a vertical direction and
    /// north-then-south for a horizontal one, so the order is fixed and the answer
    /// reproducible.
    pub fn perpendicular(self) -> [Direction; 2] {
        match self {
            Direction::North | Direction::South => [Direction::East, Direction::West],
            Direction::East | Direction::West => [Direction::North, Direction::South],
        }
    }

    /// The cardinal direction stepping `from` to an orthogonally adjacent `to`, or
    /// `None` when they are not neighbours. Because [`Cell::step`] and this share the
    /// same [`Direction::ALL`] ordering, `from.step(dir) == Some(to)` holds exactly
    /// for the returned `dir`.
    pub fn between(from: Cell, to: Cell) -> Option<Direction> {
        Direction::ALL
            .into_iter()
            .find(|&dir| from.step(dir) == Some(to))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manhattan_distance_counts_steps_not_diagonals() {
        let a = Cell::new(1, 1);
        let b = Cell::new(4, 5);
        // 3 east + 4 south = 7 steps; symmetric.
        assert_eq!(a.manhattan_distance(b), 7);
        assert_eq!(b.manhattan_distance(a), 7);
        assert_eq!(a.manhattan_distance(a), 0);
    }

    #[test]
    fn step_moves_one_cell_in_each_cardinal_direction() {
        let c = Cell::new(3, 3);
        assert_eq!(c.step(Direction::North), Some(Cell::new(3, 2)));
        assert_eq!(c.step(Direction::South), Some(Cell::new(3, 4)));
        assert_eq!(c.step(Direction::East), Some(Cell::new(4, 3)));
        assert_eq!(c.step(Direction::West), Some(Cell::new(2, 3)));
    }

    /// Every cardinal step lands exactly one Manhattan unit away — the structural
    /// guarantee that there is no diagonal move.
    #[test]
    fn every_step_is_one_manhattan_unit() {
        let c = Cell::new(5, 5);
        for dir in Direction::ALL {
            let n = c.step(dir).expect("interior cell steps in every direction");
            assert_eq!(c.manhattan_distance(n), 1, "{dir:?} is not a unit step");
        }
    }

    /// Stepping off the north/west edge is `None` rather than an underflow panic;
    /// the low edges are the only ones `Cell` alone can see (it holds no bounds).
    #[test]
    fn stepping_off_the_low_edge_is_none() {
        assert_eq!(Cell::new(0, 0).step(Direction::North), None);
        assert_eq!(Cell::new(0, 0).step(Direction::West), None);
        // East/south always yield a cell; bounds are the grid's concern.
        assert_eq!(Cell::new(0, 0).step(Direction::East), Some(Cell::new(1, 0)));
        assert_eq!(
            Cell::new(0, 0).step(Direction::South),
            Some(Cell::new(0, 1))
        );
    }

    #[test]
    fn opposite_flips_each_direction() {
        assert_eq!(Direction::North.opposite(), Direction::South);
        assert_eq!(Direction::South.opposite(), Direction::North);
        assert_eq!(Direction::East.opposite(), Direction::West);
        assert_eq!(Direction::West.opposite(), Direction::East);
        // Turning around twice returns you to where you faced.
        for dir in Direction::ALL {
            assert_eq!(dir.opposite().opposite(), dir);
        }
    }

    #[test]
    fn perpendicular_gives_the_other_axis() {
        assert_eq!(
            Direction::North.perpendicular(),
            [Direction::East, Direction::West]
        );
        assert_eq!(
            Direction::South.perpendicular(),
            [Direction::East, Direction::West]
        );
        assert_eq!(
            Direction::East.perpendicular(),
            [Direction::North, Direction::South]
        );
        assert_eq!(
            Direction::West.perpendicular(),
            [Direction::North, Direction::South]
        );
        // A perpendicular is never the direction itself nor its opposite.
        for dir in Direction::ALL {
            for perp in dir.perpendicular() {
                assert_ne!(perp, dir);
                assert_ne!(perp, dir.opposite());
            }
        }
    }

    #[test]
    fn between_names_the_adjacent_step() {
        let c = Cell::new(3, 3);
        assert_eq!(
            Direction::between(c, Cell::new(3, 2)),
            Some(Direction::North)
        );
        assert_eq!(
            Direction::between(c, Cell::new(3, 4)),
            Some(Direction::South)
        );
        assert_eq!(
            Direction::between(c, Cell::new(4, 3)),
            Some(Direction::East)
        );
        assert_eq!(
            Direction::between(c, Cell::new(2, 3)),
            Some(Direction::West)
        );
        // Every step's inverse is named by `between`.
        for dir in Direction::ALL {
            let n = c.step(dir).expect("interior cell steps everywhere");
            assert_eq!(Direction::between(c, n), Some(dir));
        }
    }

    #[test]
    fn between_is_none_for_non_neighbours() {
        let c = Cell::new(3, 3);
        assert_eq!(
            Direction::between(c, c),
            None,
            "a cell is not its own neighbour"
        );
        assert_eq!(Direction::between(c, Cell::new(3, 5)), None, "two away");
        assert_eq!(Direction::between(c, Cell::new(4, 4)), None, "diagonal");
    }
}
