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
}
