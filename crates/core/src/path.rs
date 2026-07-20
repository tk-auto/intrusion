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

/// The cardinal direction from `from` to an orthogonally adjacent `to`, or `None`
/// if they are not neighbours.
pub(crate) fn direction_between(from: Cell, to: Cell) -> Option<Direction> {
    Direction::ALL
        .into_iter()
        .find(|&dir| from.step(dir) == Some(to))
}

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
            return direction_between(from, step);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A passability predicate for a `w × h` open box (cells `[0,w) × [0,h)`) with a
    /// set of blocked cells punched out — enough to exercise the search without a
    /// [`Facility`](crate::Facility).
    fn open_box(w: u32, h: u32, walls: &[Cell]) -> impl Fn(Cell) -> bool {
        let blocked: HashSet<Cell> = walls.iter().copied().collect();
        move |c: Cell| c.x < w && c.y < h && !blocked.contains(&c)
    }

    #[test]
    fn direction_between_names_the_adjacent_step() {
        let c = Cell::new(3, 3);
        assert_eq!(
            direction_between(c, Cell::new(3, 2)),
            Some(Direction::North)
        );
        assert_eq!(direction_between(c, Cell::new(4, 3)), Some(Direction::East));
        assert_eq!(direction_between(c, Cell::new(3, 5)), None, "not adjacent");
    }

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

    #[test]
    fn an_impassable_origin_reaches_nothing() {
        let passable = open_box(6, 6, &[Cell::new(2, 2)]);
        assert!(reachable_within(Cell::new(2, 2), 5, &passable).is_empty());
    }
}
