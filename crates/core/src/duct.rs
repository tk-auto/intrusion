//! Ducts: player-only crawlspace shortcuts that span the facility (§10.7).
//!
//! A duct is a **path of cells** with a mouth-bearing **entry at each end**, drawn
//! `=` ([`Terrain::DuctEntry`](crate::Terrain::DuctEntry)). The player bumps an
//! entry from its single floor mouth to climb in (§4.3), crawls the path one cell
//! per turn, and climbs out at the far entry's mouth. Guards never enter or path
//! through a duct — the entries are wall-like, and the crawl route itself is the
//! player's alone (§10.7).
//!
//! The two *ends* of the path are stamped [`DuctEntry`](crate::Terrain::DuctEntry)
//! terrain; the **interior cells keep whatever terrain they already had**. The path
//! may cross room and corridor floor to connect two far-apart regions (§10.7
//! cross-room routing), so no terrain change marks the interior and this type is the
//! *only* record that those cells are also a crawlspace: it carries the ordered path
//! so the turn loop can resolve a crawl and the renderer (#134) can light the
//! occupied run. Because the interior may overlie ordinary floor, "the player is in a
//! duct" cannot be derived from position — it is stored explicitly on the
//! [`State`](crate::State), set by climbing in and cleared by climbing out.

use crate::cell::Cell;

/// One crawlspace run (§10.7): the ordered path of cells from one entry to the
/// other. `cells[0]` and `cells[len - 1]` are the two **entries** (the mouth-bearing
/// [`DuctEntry`](crate::Terrain::DuctEntry) ends); every cell between them is an
/// **interior** cell, which stays [`Wall`](crate::Terrain::Wall) terrain. Consecutive
/// cells are orthogonally adjacent by construction — the generator lays the path one
/// cardinal step at a time (§10.7 generation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duct {
    cells: Vec<Cell>,
}

impl Duct {
    /// Build a duct from its ordered path. The path must have at least two cells
    /// (the two entries) and every consecutive pair must be orthogonally adjacent;
    /// the generator guarantees both, and this asserts them so a malformed path can
    /// never reach the turn loop.
    pub(crate) fn new(cells: Vec<Cell>) -> Self {
        assert!(
            cells.len() >= 2,
            "a duct needs at least two cells (its two entries)"
        );
        debug_assert!(
            cells.windows(2).all(|w| w[0].manhattan_distance(w[1]) == 1),
            "a duct path must be orthogonally contiguous"
        );
        Self { cells }
    }

    /// The ordered path, entry to entry (§10.7). `cells()[0]` and the last cell are
    /// the entries; the rest are interior wall cells.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// The two mouth-bearing **entry** cells (§10.7): the path's ends.
    pub fn entries(&self) -> [Cell; 2] {
        [self.cells[0], self.cells[self.cells.len() - 1]]
    }

    /// Whether `cell` is any cell of this duct — an entry or an interior cell.
    pub fn contains(&self, cell: Cell) -> bool {
        self.cells.contains(&cell)
    }

    /// Whether `cell` is one of this duct's **entries** (§10.7) — the mouth-bearing
    /// ends the player can climb in and out at.
    pub fn is_entry(&self, cell: Cell) -> bool {
        self.entries().contains(&cell)
    }

    /// Whether `from` and `to` are consecutive cells of this duct — a single legal
    /// **crawl** step (§10.7). Order-independent: crawling runs both ways along the
    /// path.
    pub fn is_crawl_step(&self, from: Cell, to: Cell) -> bool {
        self.cells
            .windows(2)
            .any(|w| (w[0] == from && w[1] == to) || (w[0] == to && w[1] == from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight_duct() -> Duct {
        // A 4-cell horizontal run: entries at the ends, two interior cells.
        Duct::new(vec![
            Cell::new(2, 5),
            Cell::new(3, 5),
            Cell::new(4, 5),
            Cell::new(5, 5),
        ])
    }

    #[test]
    fn entries_are_the_path_ends() {
        let d = straight_duct();
        assert_eq!(d.entries(), [Cell::new(2, 5), Cell::new(5, 5)]);
        assert!(d.is_entry(Cell::new(2, 5)));
        assert!(d.is_entry(Cell::new(5, 5)));
        assert!(
            !d.is_entry(Cell::new(3, 5)),
            "an interior cell is not an entry"
        );
    }

    #[test]
    fn contains_every_path_cell() {
        let d = straight_duct();
        for c in [
            Cell::new(2, 5),
            Cell::new(3, 5),
            Cell::new(4, 5),
            Cell::new(5, 5),
        ] {
            assert!(d.contains(c));
        }
        assert!(!d.contains(Cell::new(6, 5)));
        assert!(!d.contains(Cell::new(2, 6)));
    }

    #[test]
    fn a_crawl_step_is_one_cell_along_the_path_either_way() {
        let d = straight_duct();
        // Adjacent along the path, both directions.
        assert!(d.is_crawl_step(Cell::new(2, 5), Cell::new(3, 5)));
        assert!(d.is_crawl_step(Cell::new(4, 5), Cell::new(3, 5)));
        // Not a crawl: two cells apart, or off the path entirely.
        assert!(!d.is_crawl_step(Cell::new(2, 5), Cell::new(4, 5)));
        assert!(!d.is_crawl_step(Cell::new(2, 5), Cell::new(2, 6)));
    }

    #[test]
    #[should_panic]
    fn a_single_cell_path_is_rejected() {
        Duct::new(vec![Cell::new(1, 1)]);
    }
}
