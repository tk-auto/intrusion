//! Partial-cover runs and the crouch's concealment geometry (§10.3).
//!
//! The generator never places a lone table — §10.1a stamps **benches**, straight
//! rows of 2+ partial-cover cells — so cover comes in *runs*, and the crouch
//! treats a run as one piece of furniture: bump any table of it to duck, stay
//! crouched while you keep hugging it ([`run_hugs`]), and be concealed from any
//! viewer whose line of sight to you crosses **any** cell of it
//! ([`run_conceals`]). The old rule — a quarter-plane behind the single bumped
//! cell — let a guard look straight down a bench and see the player through its
//! other tables, which undercut the exact cover §10.1a places.
//!
//! Everything here is integer arithmetic on doubled coordinates, so the answers
//! are exact and deterministic (§12.4): no floats, no epsilon, no tie that two
//! platforms could break differently.

use crate::cell::Cell;
use crate::facility::{Facility, Terrain};

/// The contiguous run of partial-cover cells containing `anchor` — the bench the
/// bumped table belongs to, gathered by 4-connected flood fill. Contiguity is
/// orthogonal, matching how §10.1a grows a bench; an L happens only where two
/// stamped runs touch, and then the whole L is honestly one piece of cover.
///
/// Empty when `anchor` is not partial cover at all (a stale anchor names no run).
pub(crate) fn cover_run(facility: &Facility, anchor: Cell) -> Vec<Cell> {
    if facility.terrain(anchor) != Some(Terrain::PartialCover) {
        return Vec::new();
    }
    let mut run = vec![anchor];
    let mut scan = 0;
    while scan < run.len() {
        let here = run[scan];
        scan += 1;
        for next in facility.neighbors(here) {
            if facility.terrain(next) == Some(Terrain::PartialCover) && !run.contains(&next) {
                run.push(next);
            }
        }
    }
    run
}

/// Whether `pos` is touching the run — within one cell of any of its tables,
/// diagonals included. The diagonal is what lets a crouch-walk round the end of
/// a bench without standing: the corner cell past the last table touches it
/// only diagonally, and the player hugging that corner is still *at* the
/// furniture, just on its turn.
pub(crate) fn run_hugs(run: &[Cell], pos: Cell) -> bool {
    run.iter().any(|&c| pos.sight_distance(c) <= 1)
}

/// Whether a crouched player at `player` is concealed from a viewer at `viewer`
/// by this run (§10.3): true when the straight sight line between the two cell
/// centres crosses any table of the run. Grazing a table's corner counts — the
/// crouch is generous at the exact diagonal, as the single-table rule was.
///
/// This is deliberately *not* the vision system's shadowcast: a table does not
/// block sight (§10.3 — a guard sees straight over it). It is the crouch's own
/// question — "is that table between us?" — answered per-viewer.
pub(crate) fn run_conceals(run: &[Cell], player: Cell, viewer: Cell) -> bool {
    if player == viewer {
        return false;
    }
    let p = doubled(player);
    let v = doubled(viewer);
    run.iter().any(|&c| segment_crosses_cell(p, v, doubled(c)))
}

/// A cell centre in doubled coordinates, where every cell spans ±1 around its
/// centre — integers all the way down, so the segment test below needs no
/// fractions.
fn doubled(cell: Cell) -> (i64, i64) {
    (i64::from(cell.x) * 2, i64::from(cell.y) * 2)
}

/// Whether the segment `p → v` (doubled coordinates) meets the unit square of
/// the cell centred at `c` (its ±1 box), touching included. The standard exact
/// test: reject when the segment's bounding box misses the square, then when
/// every square corner lies strictly on one side of the segment's line;
/// whatever survives meets the square.
fn segment_crosses_cell(p: (i64, i64), v: (i64, i64), c: (i64, i64)) -> bool {
    // Bounding boxes first: a segment wholly past one face cannot cross.
    if p.0.max(v.0) < c.0 - 1 || p.0.min(v.0) > c.0 + 1 {
        return false;
    }
    if p.1.max(v.1) < c.1 - 1 || p.1.min(v.1) > c.1 + 1 {
        return false;
    }
    // Side test: the cross product of the segment direction with each corner
    // offset. All four corners strictly one side → the line misses the square;
    // a zero (a corner exactly on the line) is the graze that counts.
    let (dx, dy) = (v.0 - p.0, v.1 - p.1);
    let mut ahead = false;
    let mut behind = false;
    for (cx, cy) in [
        (c.0 - 1, c.1 - 1),
        (c.0 - 1, c.1 + 1),
        (c.0 + 1, c.1 - 1),
        (c.0 + 1, c.1 + 1),
    ] {
        let side = dx * (cy - p.1) - dy * (cx - p.0);
        if side == 0 {
            return true; // grazing the corner counts as covered
        }
        if side > 0 {
            ahead = true;
        } else {
            behind = true;
        }
    }
    ahead && behind
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A walled box with a run of tables stamped in — enough facility to flood.
    fn boxed_with_tables(cells: &[(u32, u32)]) -> Facility {
        let mut f = Facility::walled_box(12, 12);
        for &(x, y) in cells {
            f.set_terrain(x, y, Terrain::PartialCover);
        }
        f
    }

    /// §10.1a benches are the unit of cover: the flood gathers the whole
    /// orthogonal run from any of its cells, and two touching runs are one.
    #[test]
    fn cover_run_gathers_the_contiguous_bench() {
        // A vertical 3-bench and, touching its south end, a horizontal 2-bench:
        // one L-shaped piece of furniture.
        let f = boxed_with_tables(&[(5, 3), (5, 4), (5, 5), (6, 5), (7, 5)]);
        let mut run = cover_run(&f, Cell::new(5, 4));
        run.sort_by_key(|c| (c.y, c.x));
        assert_eq!(
            run,
            vec![
                Cell::new(5, 3),
                Cell::new(5, 4),
                Cell::new(5, 5),
                Cell::new(6, 5),
                Cell::new(7, 5),
            ]
        );
        // A separate table one gap away is its own run.
        let f = boxed_with_tables(&[(5, 3), (5, 4), (5, 6)]);
        assert_eq!(cover_run(&f, Cell::new(5, 6)), vec![Cell::new(5, 6)]);
        // A stale anchor that names no table names no run.
        assert_eq!(cover_run(&f, Cell::new(2, 2)), Vec::<Cell>::new());
    }

    /// Hugging is the 8-neighbourhood of the run: the diagonal past a bench's
    /// end still hugs (that is the corner turn), two cells out does not.
    #[test]
    fn run_hugs_includes_the_diagonal_corner() {
        let run = vec![Cell::new(5, 3), Cell::new(5, 4), Cell::new(5, 5)];
        assert!(run_hugs(&run, Cell::new(4, 4)), "flush beside the bench");
        assert!(run_hugs(&run, Cell::new(4, 6)), "the corner past its end");
        assert!(run_hugs(&run, Cell::new(5, 6)), "square-on below its end");
        assert!(!run_hugs(&run, Cell::new(4, 7)), "two cells past the end");
        assert!(!run_hugs(&run, Cell::new(3, 4)), "a cell of air between");
    }

    /// The single-table geometry the old quarter-plane rule established still
    /// holds under the segment test: covered across the table out to the exact
    /// 45° graze, open on the flanks and behind.
    #[test]
    fn a_single_table_covers_its_quarter_plane() {
        let run = vec![Cell::new(5, 4)];
        let player = Cell::new(4, 4);
        // Straight across, near and far; leaning to the exact diagonal.
        assert!(run_conceals(&run, player, Cell::new(6, 4)));
        assert!(run_conceals(&run, player, Cell::new(9, 4)));
        assert!(run_conceals(&run, player, Cell::new(6, 3)));
        assert!(run_conceals(&run, player, Cell::new(6, 2)), "45° graze");
        // The flank, the perpendicular, behind: open.
        assert!(!run_conceals(&run, player, Cell::new(5, 2)));
        assert!(!run_conceals(&run, player, Cell::new(4, 2)));
        assert!(!run_conceals(&run, player, Cell::new(2, 4)));
    }

    /// The ticket's regression: a viewer the anchored table alone would not
    /// cover is still blinded when its sight line crosses *another* cell of the
    /// same bench — and a viewer past the bench's end stays uncovered, so the
    /// flanks are still real.
    #[test]
    fn a_bench_covers_across_its_whole_run() {
        let run = vec![Cell::new(5, 3), Cell::new(5, 4), Cell::new(5, 5)];
        let player = Cell::new(4, 4);
        // Oblique to the south-east: outside the anchor's quarter-plane, but
        // the line to the player crosses the bench's southern table.
        assert!(run_conceals(&run, player, Cell::new(6, 7)));
        // And symmetrically to the north-east, across the northern table.
        assert!(run_conceals(&run, player, Cell::new(6, 1)));
        // Due north past the bench's end: no table on the line — seen.
        assert!(!run_conceals(&run, player, Cell::new(4, 0)));
        // Behind the player, away from the bench: seen.
        assert!(!run_conceals(&run, player, Cell::new(2, 4)));
    }

    /// Rounding the corner keeps the cover honest: from below the bench's end
    /// the run blinds a viewer straight up the column, while a viewer level
    /// with the player sees them — cover is where the furniture is, not a
    /// status the crouch grants.
    #[test]
    fn cover_follows_the_player_round_the_corner() {
        let run = vec![Cell::new(5, 3), Cell::new(5, 4), Cell::new(5, 5)];
        let player = Cell::new(5, 6); // square-on below the end table
        assert!(run_conceals(&run, player, Cell::new(5, 1)), "up the column");
        assert!(!run_conceals(&run, player, Cell::new(8, 6)), "level flank");
        assert!(!run_conceals(&run, player, Cell::new(5, 9)), "behind");
    }
}
