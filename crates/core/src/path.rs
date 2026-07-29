//! Deterministic grid pathing over the cell lattice (§7.5, §12.4).
//!
//! These are the movement primitives the guard AI walks on, kept free of any guard
//! concept: each takes a `passable` predicate and answers "which way is the target"
//! or "what can I reach", nothing more. Neighbours are always visited in
//! [`Direction::ALL`] order, so every answer is reproducible for a given board —
//! the determinism the replay tests depend on. Bounds are the predicate's job: an
//! off-grid cell is simply one that does not pass.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::cell::{Cell, Direction};

/// The first step of the shortest path from `from` to `to` across cells where
/// `passable` holds, or `None` when they coincide or nothing connects them. A plain
/// breadth-first search expanding neighbours in [`Direction::ALL`] order, so the
/// path — and any patrol built on it — is deterministic (§12.4).
///
/// The *goal* `to` is reachable even when it is not itself `passable`: a guard can
/// be sent onto a cell it will be refused entry to (a cupboard holding a hidden
/// player, or — later — a chase target), so only the cells walked *through* must
/// pass.
pub(crate) fn first_step_toward(
    from: Cell,
    to: Cell,
    passable: impl Fn(Cell) -> bool,
) -> Option<Direction> {
    if from == to {
        return None;
    }
    let mut came_from: HashMap<Cell, Cell> = HashMap::new();
    came_from.insert(from, from);
    let mut frontier = VecDeque::new();
    frontier.push_back(from);
    while let Some(cell) = frontier.pop_front() {
        if cell == to {
            // Walk the parent chain back to the cell one step out of `from`.
            let mut step = to;
            while came_from[&step] != from {
                step = came_from[&step];
            }
            return Direction::between(from, step);
        }
        for dir in Direction::ALL {
            let Some(next) = cell.step(dir) else {
                continue;
            };
            if next != to && !passable(next) {
                continue;
            }
            // Only the *first* time a cell is reached fixes its parent — overwriting
            // would corrupt the search tree (a later visit could point it back at a
            // descendant, cycling the reconstruction above).
            if let Entry::Vacant(slot) = came_from.entry(next) {
                slot.insert(cell);
                frontier.push_back(next);
            }
        }
    }
    None
}

/// The cells reachable from `origin` across `passable` cells without leaving the
/// `radius` Manhattan disc — a bounded flood fill, returned in breadth-first order.
/// `origin` is included when it is itself passable; an impassable origin yields an
/// empty set.
pub(crate) fn reachable_within(
    origin: Cell,
    radius: u32,
    passable: impl Fn(Cell) -> bool,
) -> Vec<Cell> {
    let mut cells = Vec::new();
    if !passable(origin) {
        return cells;
    }
    let mut seen = HashSet::new();
    let mut frontier = VecDeque::new();
    seen.insert(origin);
    frontier.push_back(origin);
    while let Some(cell) = frontier.pop_front() {
        cells.push(cell);
        for dir in Direction::ALL {
            let Some(next) = cell.step(dir) else {
                continue;
            };
            if origin.manhattan_distance(next) <= radius && passable(next) && seen.insert(next) {
                frontier.push_back(next);
            }
        }
    }
    cells
}

/// How many steps each cell reachable from `origin` is away across `passable`
/// cells — the same 4-connected flood as [`flood_from`], keeping the depth it
/// reached each cell at rather than only that it did.
///
/// One pass answers "how far is every guard from here" for a whole roster
/// ([`nearest_respondable`](crate::radio::nearest_respondable)), which straight-line
/// distance cannot: a cell two rooms down the corridor is nearer than one just
/// across a wall. Cells the flood never reaches are simply absent — the caller
/// decides what an unreachable actor means, since "no route" is a different fact
/// from "a long route".
///
/// `origin` seeds the flood unconditionally and maps to `0`, like [`flood_from`]:
/// a dispatch site may be a cell nobody could stand on, and it is still the place
/// everyone is measured from. Only the cells stepped *into* are gated on `passable`.
pub(crate) fn route_lengths_from(
    origin: Cell,
    passable: impl Fn(Cell) -> bool,
) -> HashMap<Cell, u32> {
    let mut lengths = HashMap::new();
    lengths.insert(origin, 0);
    let mut frontier = VecDeque::new();
    frontier.push_back(origin);
    while let Some(cell) = frontier.pop_front() {
        let next_length = lengths[&cell] + 1;
        for dir in Direction::ALL {
            let Some(next) = cell.step(dir) else {
                continue;
            };
            if !passable(next) {
                continue;
            }
            if let Entry::Vacant(slot) = lengths.entry(next) {
                slot.insert(next_length);
                frontier.push_back(next);
            }
        }
    }
    lengths
}

/// Every cell reachable from `start` across `passable` cells on a `width × height`
/// grid — the full-grid 4-connected flood fill. A sibling to [`reachable_within`]
/// with no distance bound: only the `passable` predicate stops it.
///
/// Neighbours are visited in [`Direction::ALL`] order and de-duplicated through a
/// `width × height` bit grid — the same cheap index sweep the §10.6 reachability
/// and solvability checks each used to hand-roll — so the flood stays fast enough
/// to run inside the generation retry loop and the reachable set is reproducible
/// (§12.4). The return order is breadth-first, but callers should treat the result
/// as a set; only membership and count are meaningful.
///
/// `start` seeds the flood unconditionally — its own passability is the caller's
/// concern (a caller may flood outward from a cell masked solid in play), matching
/// how [`first_step_toward`] lets a guard be sent onto a cell it cannot enter. Only
/// the cells stepped *into* are gated on `passable`, and any neighbour off the
/// `width × height` grid is skipped.
pub(crate) fn flood_from(
    start: Cell,
    width: u32,
    height: u32,
    passable: impl Fn(Cell) -> bool,
) -> Vec<Cell> {
    let mut reached = Vec::new();
    let mut seen = vec![false; (width * height) as usize];
    let idx = |c: Cell| (c.y * width + c.x) as usize;
    seen[idx(start)] = true;
    let mut frontier = VecDeque::new();
    frontier.push_back(start);
    while let Some(cell) = frontier.pop_front() {
        reached.push(cell);
        for dir in Direction::ALL {
            let Some(next) = cell.step(dir) else {
                continue;
            };
            if next.x < width && next.y < height && passable(next) && !seen[idx(next)] {
                seen[idx(next)] = true;
                frontier.push_back(next);
            }
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::open_box;

    #[test]
    fn first_step_takes_the_shortest_route_around_a_wall() {
        // A wall at (2,2) forces the path off the straight line east.
        let passable = open_box(6, 6, &[Cell::new(2, 2)]);
        let dir = first_step_toward(Cell::new(2, 1), Cell::new(2, 3), &passable);
        // Direct south is blocked at (2,2); the deterministic BFS steps aside first.
        assert!(matches!(dir, Some(Direction::East | Direction::West)));
        // Coincident endpoints and unreachable goals yield nothing.
        assert_eq!(
            first_step_toward(Cell::new(1, 1), Cell::new(1, 1), &passable),
            None
        );
        let boxed_in = open_box(6, 6, &[Cell::new(0, 1), Cell::new(1, 0)]);
        assert_eq!(
            first_step_toward(Cell::new(0, 0), Cell::new(5, 5), &boxed_in),
            None,
            "no path exists",
        );
    }

    #[test]
    fn first_step_reaches_an_impassable_goal_cell() {
        // The goal itself is blocked, but a guard must still be routable onto it.
        let passable = open_box(6, 6, &[Cell::new(3, 1)]);
        assert_eq!(
            first_step_toward(Cell::new(1, 1), Cell::new(3, 1), &passable),
            Some(Direction::East),
            "the goal is reachable even when not passable; only the path through is",
        );
    }

    #[test]
    fn reachable_within_is_bounded_and_flood_stops_at_walls() {
        let passable = open_box(20, 20, &[]);
        let cells = reachable_within(Cell::new(10, 10), 3, &passable);
        assert!(cells.contains(&Cell::new(10, 10)), "origin is included");
        assert!(cells
            .iter()
            .all(|&c| Cell::new(10, 10).manhattan_distance(c) <= 3));
        assert!(cells.contains(&Cell::new(13, 10)), "a cell at the radius");
        assert!(
            !cells.contains(&Cell::new(14, 10)),
            "past the radius is out"
        );

        // A wall column just west of the origin seals the whole west side within the
        // radius: the flood cannot round it (it extends past the disc), so cells
        // behind it stay unreached even though they sit inside the radius.
        let wall: Vec<Cell> = (7..=13).map(|y| Cell::new(9, y)).collect();
        let sealed = open_box(20, 20, &wall);
        let cells = reachable_within(Cell::new(10, 10), 3, &sealed);
        assert!(cells.contains(&Cell::new(10, 7)), "the open side is swept");
        assert!(
            !cells.contains(&Cell::new(8, 10)),
            "the walled-off side is not reached, though it is within the radius",
        );
    }

    /// The route length is the walk, not the straight line: a cell just the other
    /// side of a wall is far, and one further away down an open lane is near. That
    /// inversion is the whole reason dispatch selection uses this (§7.3/#409).
    #[test]
    fn route_lengths_measure_the_walk_not_the_line() {
        // A wall column at x=1 spanning the box, with the one gap at its foot.
        let wall: Vec<Cell> = (0..5).map(|y| Cell::new(1, y)).collect();
        let passable = open_box(6, 6, &wall);
        let lengths = route_lengths_from(Cell::new(0, 0), &passable);

        assert_eq!(lengths[&Cell::new(0, 0)], 0, "the origin is zero");
        assert_eq!(lengths[&Cell::new(0, 3)], 3, "straight down the open lane");
        // (2,0) is 2 cells away in a straight line but the wall forces the walk
        // down to the gap at y=5 and back up: 5 + 2 + 5 = 12.
        assert_eq!(lengths[&Cell::new(2, 0)], 12, "around the wall");
        assert!(
            lengths[&Cell::new(2, 0)] > lengths[&Cell::new(0, 3)],
            "nearer by line is farther by route",
        );
    }

    /// A cell with no route is **absent**, not far away — the caller distinguishes
    /// "a long walk" from "no walk at all". The origin still maps to zero even when
    /// it is itself impassable, so a dispatch site nobody can stand on still
    /// measures everyone (§7.3).
    #[test]
    fn route_lengths_omit_the_unreachable_and_seed_the_origin() {
        let sealed = open_box(6, 6, &[Cell::new(0, 1), Cell::new(1, 0), Cell::new(1, 1)]);
        let lengths = route_lengths_from(Cell::new(0, 0), &sealed);
        assert_eq!(lengths.len(), 1, "boxed into its own corner");
        assert_eq!(lengths[&Cell::new(0, 0)], 0);
        assert!(
            !lengths.contains_key(&Cell::new(5, 5)),
            "no route, no entry"
        );

        let blocked_origin = open_box(6, 6, &[Cell::new(2, 2)]);
        let lengths = route_lengths_from(Cell::new(2, 2), &blocked_origin);
        assert_eq!(lengths[&Cell::new(2, 2)], 0, "the origin seeds regardless");
        assert_eq!(lengths[&Cell::new(2, 4)], 2, "and the flood leaves it");
    }

    #[test]
    fn an_impassable_origin_reaches_nothing() {
        let passable = open_box(6, 6, &[Cell::new(2, 2)]);
        assert!(reachable_within(Cell::new(2, 2), 5, &passable).is_empty());
    }
}
