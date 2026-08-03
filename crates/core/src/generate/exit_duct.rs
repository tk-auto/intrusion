//! The tunnel the player dug (§4.5/§10.7/#466) — the exit as a place, not a tile.
//!
//! The exit `E` is the **inner mouth** of a linear duct that runs from `E` out to the
//! level border and, through it, to the outside world. The run starts with the player
//! inside that duct on the border cell and crawls them out; leaving is the same thing
//! backwards, ending in a step off the board (§4.5).
//!
//! **Straight**, because a tunnel you dug yourself is straight: it also keeps the shape
//! trivially assertable and the crawl short. The run is laid toward the **nearest**
//! border the geometry allows, so the opening is a few turns rather than a march.
//!
//! # Why this changes no §10.6 guarantee
//!
//! It stamps **nothing**. The only cell whose terrain changes is `E`, which
//! [`State::new`](crate::State::new) has always stamped; every interior cell keeps its
//! own terrain (§10.7), and the way-out cell stays the **border wall** it was born as —
//! so the enclosure ring is still unbroken wall and reachability, sightlines and pathing
//! on the finished grid are byte-identical with the tunnel recorded or not. What the
//! tunnel adds is a route only the player can walk, which is exactly what a duct is.
//!
//! Because it is pure geometry it is computed by [`place`](crate::place) — which is
//! where `E` is chosen — and recorded on the layout beside the §10.7 shortcuts.

use std::collections::HashSet;

use crate::cell::{Cell, Direction};
use crate::duct::Duct;
use crate::facility::{Facility, Terrain};

/// The fewest cells the exit tunnel may span, `E` and the way-out cell included
/// (§4.5/#466 **[START]**).
///
/// **Eight** — seven crawl steps and a climb-out. It began at four and came up on the
/// first playtest: a four-cell tunnel reads as a hole in the skirting rather than
/// something you dug, and the opening was over before it had established where you were.
/// The length is what makes the entrance a *passage*, and the passage is the whole point
/// of the ticket.
///
/// It is also what `PLAYER_EXIT_MIN_DISTANCE` became (§10.6): the old constant kept the
/// *player* away from the exit so a run could not start won, and now that the player
/// starts **in** the tunnel with the way out behind them, the distance placement chooses
/// is the tunnel's own length. Pinned by a test.
pub(crate) const EXIT_DUCT_MIN_CELLS: usize = 8;

/// The most cells the exit tunnel may span (§4.5/#466 **[START]**).
///
/// **Sixteen.** Every cell of it is a **turn** — guards patrol while you crawl, which is
/// the point (§4.4) — so this is the knob that decides how long every run takes to start,
/// and the pair with [`EXIT_DUCT_MIN_CELLS`] is what the opening's whole character hangs
/// on. Wide enough above the floor that a candidate `E` still has somewhere to come out
/// (the two together are a band `E` must sit inside: at least seven cells from the border
/// it tunnels to, at most fifteen), short enough that the crawl never becomes a chore.
/// Pinned by a test, and measured over the seed sweep
/// (`the_exit_sits_in_the_largest_room_with_its_tunnel_behind_it`).
pub(crate) const EXIT_DUCT_MAX_CELLS: usize = 16;

// A tunnel must be able to *be* a tunnel: the range has to admit at least one length.
const _: () = assert!(EXIT_DUCT_MIN_CELLS >= 3 && EXIT_DUCT_MIN_CELLS <= EXIT_DUCT_MAX_CELLS);

/// Lay the player's own tunnel from `exit` to the nearest reachable border (§4.5/#466),
/// or `None` when no straight run out of this cell is usable — in which case the caller
/// tries another `E` and, failing that, rejects the seed like any other §10.6 shortfall.
///
/// A run is usable when every cell between `exit` and the border is **inert** — plain
/// wall or floor, never a cupboard, door, console, table or duct entry, the same rule
/// §10.7 puts on a shortcut's interior, and for the same reason: crawling over an
/// interactable would collide with the terrain it overlies. Cells already claimed by a
/// §10.7 shortcut (`used`) are refused too, so two ducts never share a cell and "which
/// duct am I in" stays unambiguous.
///
/// The **shortest** usable direction wins (ties break in [`Direction::ALL`] order), so
/// the tunnel comes out at the nearest border it can reach and the opening crawl is as
/// short as the geometry allows.
pub(crate) fn carve_exit_duct(
    facility: &Facility,
    exit: Cell,
    used: &HashSet<Cell>,
) -> Option<Duct> {
    Direction::ALL
        .into_iter()
        .filter_map(|dir| run_to_border(facility, exit, dir, used))
        .min_by_key(|run| run.len())
        .map(Duct::exit_tunnel)
}

/// The straight run of cells from `exit` outward along `dir`, ending on the first
/// border cell — or `None` if it runs into something a crawl cannot cross, is claimed by
/// another duct, or falls outside the [`EXIT_DUCT_MIN_CELLS`]..=[`EXIT_DUCT_MAX_CELLS`]
/// range.
fn run_to_border(
    facility: &Facility,
    exit: Cell,
    dir: Direction,
    used: &HashSet<Cell>,
) -> Option<Vec<Cell>> {
    let mut cells = vec![exit];
    let mut cell = exit;
    loop {
        // The border ring is unbroken (§10.6), so a straight run always meets it before
        // it leaves the grid — `step` returning `None` would mean `exit` was itself on
        // the border, which placement never chooses.
        cell = cell.step(dir)?;
        if !facility.in_bounds(cell) || used.contains(&cell) {
            return None;
        }
        cells.push(cell);
        if cells.len() > EXIT_DUCT_MAX_CELLS {
            return None;
        }
        if on_border(facility, cell) {
            // The terminus: the way out. It keeps its border-wall terrain, so the
            // enclosure §10.6 asserts is untouched.
            return (cells.len() >= EXIT_DUCT_MIN_CELLS).then_some(cells);
        }
        // An interior cell of the crawl: inert geometry only (§10.7).
        if !matches!(
            facility.terrain(cell),
            Some(Terrain::Wall) | Some(Terrain::Floor)
        ) {
            return None;
        }
    }
}

/// Whether `cell` is on the facility's border ring — the wall the tunnel comes out
/// through (§10.6: the ring is unconditional, and this pass leaves it that way).
fn on_border(facility: &Facility, cell: Cell) -> bool {
    cell.x == 0 || cell.y == 0 || cell.x == facility.width() - 1 || cell.y == facility.height() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duct::DuctKind;

    /// A 20×20 empty walled box: every interior cell is plain floor, so a run out of any
    /// of them is usable and the *nearest* border is the only thing choosing a direction.
    fn box20() -> Facility {
        Facility::walled_box(20, 20)
    }

    /// The tunnel runs straight out of `E` to the **nearest** border it may reach, ends
    /// on the border ring, and keeps that cell's wall terrain (§4.5/#466/§10.6).
    #[test]
    fn the_tunnel_runs_straight_to_the_nearest_border() {
        let f = box20();
        // From (8, 9) the four runs are 9 west, 12 east, 10 north, 11 south — every one
        // of them a legal length, so the shortest wins.
        let duct = carve_exit_duct(&f, Cell::new(8, 9), &HashSet::new()).expect("a clear run");
        assert_eq!(duct.kind(), DuctKind::Exit);
        assert_eq!(
            duct.cells(),
            [
                Cell::new(8, 9),
                Cell::new(7, 9),
                Cell::new(6, 9),
                Cell::new(5, 9),
                Cell::new(4, 9),
                Cell::new(3, 9),
                Cell::new(2, 9),
                Cell::new(1, 9),
                Cell::new(0, 9),
            ]
        );
        assert_eq!(duct.way_out(), Some(Cell::new(0, 9)));
        assert_eq!(
            f.terrain(Cell::new(0, 9)),
            Some(Terrain::Wall),
            "the way out is the border wall, unstamped"
        );
    }

    /// The **minimum** bites: a cell too close to one border takes a further one
    /// instead, so the opening is always a passage rather than a hole in the skirting.
    #[test]
    fn a_run_shorter_than_the_minimum_is_refused() {
        let f = box20();
        // (2, 9): west is 3 cells — well under the floor — and east is 18, over the cap,
        // so the run goes north (10) rather than out of the wall it stands against.
        let duct = carve_exit_duct(&f, Cell::new(2, 9), &HashSet::new()).expect("a clear run");
        assert!(duct.cells().len() >= EXIT_DUCT_MIN_CELLS);
        assert_eq!(duct.way_out(), Some(Cell::new(2, 0)), "not the near border");
    }

    /// The **maximum** bites: a cell in the middle of a footprint wider than twice the
    /// cap has no usable run at all, and placement must try another `E` (§10.6).
    #[test]
    fn a_cell_too_deep_inside_the_building_has_no_tunnel() {
        let f = Facility::walled_box(60, 60);
        assert!(carve_exit_duct(&f, Cell::new(30, 30), &HashSet::new()).is_none());
    }

    /// A crawl crosses **inert** geometry only (§10.7): a door, a console, a cupboard or
    /// a table in the straight run refuses that direction.
    #[test]
    fn an_interactable_in_the_way_refuses_the_run() {
        for blocker in [
            Terrain::DoorPanelClosed,
            Terrain::Console,
            Terrain::Hideout,
            Terrain::PartialCover,
            Terrain::DuctEntry,
        ] {
            let mut f = box20();
            f.set_terrain(4, 9, blocker);
            let duct = carve_exit_duct(&f, Cell::new(8, 9), &HashSet::new())
                .expect("the other directions are still clear");
            assert_ne!(
                duct.way_out(),
                Some(Cell::new(0, 9)),
                "{blocker:?} in the run must refuse the west tunnel"
            );
        }
    }

    /// Cells a §10.7 shortcut already claims are refused, so two ducts never share one
    /// and "which duct am I crawling" stays unambiguous.
    #[test]
    fn a_cell_claimed_by_another_duct_refuses_the_run() {
        let f = box20();
        let used: HashSet<Cell> = [Cell::new(4, 9)].into_iter().collect();
        let duct = carve_exit_duct(&f, Cell::new(8, 9), &used).expect("other directions are clear");
        assert_ne!(duct.way_out(), Some(Cell::new(0, 9)));
    }
}
