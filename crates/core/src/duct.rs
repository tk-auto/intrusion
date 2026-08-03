//! Ducts: crawlspaces only the player can use (§10.7) — the found shortcuts, and
//! the one the player dug themselves (§4.5).
//!
//! A duct is a **path of cells**. A **shortcut** ([`DuctKind::Shortcut`]) has a
//! mouth-bearing **entry at each end**, drawn `=`
//! ([`Terrain::DuctEntry`](crate::Terrain::DuctEntry)): the player bumps an entry from
//! its single floor mouth to climb in (§4.3), crawls the path one cell per turn, and
//! climbs out at the far entry's mouth. Guards never enter or path through a duct —
//! the entries are wall-like, and the crawl route itself is the player's alone (§10.7).
//!
//! The **exit tunnel** ([`DuctKind::Exit`], §4.5/#466) is the same crawlspace with one
//! end in a different world. `cells[0]` is the exit `E` — the tunnel's inner mouth, the
//! one entry, the one the player *made* rather than found — and the last cell is the
//! **way out**: a cell of the level border, where a step off the board ends the run
//! (§4.5's intel gate permitting). It is not an entry: there is no mouth on the border
//! and nothing to climb out onto but the world. The run **begins** on it, so the first
//! inputs of every run are the crawl in.
//!
//! The mouth-bearing *ends* of the path are stamped
//! [`DuctEntry`](crate::Terrain::DuctEntry) terrain (the exit tunnel's is
//! [`Exit`](crate::Terrain::Exit), stamped with the other usables in
//! [`State::new`](crate::State::new)); the **interior cells keep whatever terrain they
//! already had**, and so does the way-out cell, which stays the border wall it was born
//! as — the §10.6 enclosure is untouched. The path may cross room and corridor floor to
//! connect two far-apart regions (§10.7 cross-room routing), so no terrain change marks
//! the interior and this type is the *only* record that those cells are also a
//! crawlspace: it carries the ordered path so the turn loop can resolve a crawl and the
//! renderer (#134) can light the occupied run. Because the interior may overlie ordinary
//! floor, "the player is in a duct" cannot be derived from position — it is stored
//! explicitly on the [`State`](crate::State), set by climbing in and cleared by climbing
//! out.

use crate::cell::Cell;

/// What a duct **is for** (§10.7/§4.5) — the one thing that differs between the two,
/// which is what its far end opens onto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuctKind {
    /// A crawlspace shortcut found in the facility (§10.7): a mouth-bearing entry at
    /// each end, joining two far-apart regions.
    Shortcut,
    /// The tunnel the player dug and came in by (§4.5/#466): `cells[0]` is the exit
    /// `E`, its one entry; the last cell is the **way out** on the level border, where
    /// a step off the board wins the run.
    Exit,
}

/// One crawlspace run (§10.7): the ordered path of cells from one end to the other.
/// `cells[0]` is always a mouth-bearing end — a [`DuctEntry`](crate::Terrain::DuctEntry)
/// on a shortcut, the [`Exit`](crate::Terrain::Exit) on the player's own tunnel — and
/// what the far end is depends on the [`kind`](Duct::kind). Every cell between them is
/// an **interior** cell, which keeps whatever terrain it already had. Consecutive cells
/// are orthogonally adjacent by construction — the generator lays the path one cardinal
/// step at a time (§10.7 generation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Duct {
    cells: Vec<Cell>,
    kind: DuctKind,
}

impl Duct {
    /// Build a **shortcut** duct from its ordered path (§10.7). The path must have at
    /// least two cells (the two entries) and every consecutive pair must be
    /// orthogonally adjacent; the generator guarantees both, and this asserts them so a
    /// malformed path can never reach the turn loop.
    pub(crate) fn new(cells: Vec<Cell>) -> Self {
        Self::of_kind(cells, DuctKind::Shortcut)
    }

    /// Build the player's own **exit tunnel** (§4.5/#466) from its ordered path:
    /// `cells[0]` is the exit `E` and the last cell is the way out on the level border.
    pub(crate) fn exit_tunnel(cells: Vec<Cell>) -> Self {
        Self::of_kind(cells, DuctKind::Exit)
    }

    fn of_kind(cells: Vec<Cell>, kind: DuctKind) -> Self {
        assert!(
            cells.len() >= 2,
            "a duct needs at least two cells (its ends)"
        );
        debug_assert!(
            cells.windows(2).all(|w| w[0].manhattan_distance(w[1]) == 1),
            "a duct path must be orthogonally contiguous"
        );
        Self { cells, kind }
    }

    /// What this duct is for (§10.7/§4.5) — a found shortcut, or the way home.
    pub fn kind(&self) -> DuctKind {
        self.kind
    }

    /// The ordered path, end to end (§10.7). `cells()[0]` is the mouth-bearing end;
    /// the last is the far entry (a shortcut) or the way out (the exit tunnel).
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// The mouth-bearing **entry** cells (§10.7): both ends of a shortcut, and only
    /// `cells[0]` — the exit `E` — on the player's own tunnel, whose far end is the way
    /// out rather than something to climb out of.
    pub fn entries(&self) -> Vec<Cell> {
        match self.kind {
            DuctKind::Shortcut => vec![self.cells[0], self.far_end()],
            DuctKind::Exit => vec![self.cells[0]],
        }
    }

    /// The **way out** (§4.5/#466): the level-border cell where a step off the board
    /// ends the run, and where the run begins. `None` on a shortcut duct — a found
    /// crawlspace leads nowhere but back into the facility.
    pub fn way_out(&self) -> Option<Cell> {
        match self.kind {
            DuctKind::Shortcut => None,
            DuctKind::Exit => Some(self.far_end()),
        }
    }

    /// The last cell of the path.
    fn far_end(&self) -> Cell {
        self.cells[self.cells.len() - 1]
    }

    /// Whether `cell` is any cell of this duct — an end or an interior cell.
    pub fn contains(&self, cell: Cell) -> bool {
        self.cells.contains(&cell)
    }

    /// Whether `cell` is one of this duct's **entries** (§10.7) — the mouth-bearing
    /// ends the player can climb in and out at. The exit tunnel's way-out cell is not
    /// one: it has no mouth, and the only thing to step into from it is the world
    /// (§4.5).
    pub fn is_entry(&self, cell: Cell) -> bool {
        cell == self.cells[0] || (self.kind == DuctKind::Shortcut && cell == self.far_end())
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

    /// The player's own tunnel (§4.5/#466): the same 4-cell run, `E` at one end and
    /// the level border at the other.
    fn exit_tunnel() -> Duct {
        Duct::exit_tunnel(vec![
            Cell::new(3, 5),
            Cell::new(2, 5),
            Cell::new(1, 5),
            Cell::new(0, 5),
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

    /// §4.5/#466: the exit tunnel has **one** entry — the `E` its path starts at. The
    /// far end is the way out, and calling it an entry would promise a mouth that is
    /// not there (the turn loop reads `is_entry` to decide what a step off the path
    /// does).
    #[test]
    fn the_exit_tunnel_has_one_entry_and_a_way_out() {
        let d = exit_tunnel();
        assert_eq!(d.kind(), DuctKind::Exit);
        assert_eq!(d.entries(), [Cell::new(3, 5)]);
        assert!(d.is_entry(Cell::new(3, 5)), "E is the tunnel's mouth");
        assert!(
            !d.is_entry(Cell::new(0, 5)),
            "the way out is not something you climb out of"
        );
        assert_eq!(d.way_out(), Some(Cell::new(0, 5)));
        // A found shortcut has no way out: it leads back into the facility.
        assert_eq!(straight_duct().way_out(), None);
        assert_eq!(straight_duct().kind(), DuctKind::Shortcut);
    }

    /// The crawl is the same either way (§10.7): the exit tunnel is a duct, not a
    /// second mechanism.
    #[test]
    fn the_exit_tunnel_crawls_like_any_duct() {
        let d = exit_tunnel();
        assert!(d.is_crawl_step(Cell::new(3, 5), Cell::new(2, 5)));
        assert!(d.is_crawl_step(Cell::new(0, 5), Cell::new(1, 5)));
        assert!(d.contains(Cell::new(1, 5)));
        assert!(!d.contains(Cell::new(4, 5)));
    }
}
