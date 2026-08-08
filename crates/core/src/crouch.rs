//! Partial-cover runs and the crouch's concealment geometry (§10.3).
//!
//! The generator never places a lone table — §10.1a stamps **benches**, straight
//! rows of 2+ partial-cover cells — so cover comes in *runs*, and the crouch
//! treats a run as one piece of furniture: bump any table of it to duck, stay
//! crouched while you keep hugging it ([`run_hugs`]), and be concealed by the
//! **side of it you are on** ([`run_conceals`]).
//!
//! That last rule has been narrowed twice and widened once, and both moves are
//! worth keeping in view:
//!
//! - It began as the **quarter-plane** behind the single bumped cell, which let a
//!   guard look straight down a bench and see the player through its other
//!   tables — undercutting the exact cover §10.1a places.
//! - It became a **per-ray** test across the whole run, which fixed that but was
//!   too tight the other way (#377): a short bench subtends a narrow wedge, so a
//!   guard only a little off the run's axis had a clear line, and the player
//!   could not compute that wedge at a glance. A turn spent on protection you
//!   cannot predict is a turn not spent.
//! - It is now the **half-plane** taken from each straight arm's own line, with
//!   the ray test kept as a union so the on-axis cases a half-plane cannot
//!   express still work. A viewer across the furniture from you does not see you;
//!   one that has come round to *your* side does.
//!
//! Everything here is integer arithmetic — side-of-line signs, and doubled
//! coordinates for the ray — so the answers are exact and deterministic (§12.4):
//! no floats, no epsilon, no tie that two platforms could break differently.

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
        for next in facility.neighbours(here) {
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
/// by this run (§10.3). Two ways, unioned:
///
/// - **Across the furniture** ([`arm_separates`]) — the viewer is strictly on the
///   far side of one of the run's straight arms from the player. This is the rule
///   the player reads at a glance: *which side of the bench is he on?* It holds
///   however far past the arm's ends the viewer stands, which is the whole point
///   — the old per-ray wedge was the part nobody could predict (#377).
/// - **Across a lone piece** ([`lone_separates`]) — the one-cell run's degenerate
///   arm (§10.3/#562). A single covering cell has no arm at all, so it borrows the
///   line **perpendicular to the direction the player is covering from**.
/// - **Through the furniture** ([`ray_crosses_run`]) — the straight sight line
///   between the two cell centres crosses a table of the run, corner grazes
///   included. Kept because a half-plane has nothing to say when the player
///   stands *in* the arm's own line — rounding the end of a bench, or a lone
///   table caught exactly on the diagonal — and looking down the bench must still
///   be blocked.
///
/// This is deliberately *not* the vision system's shadowcast: a table does not
/// block sight (§10.3 — a guard sees straight over it). It is the crouch's own
/// question — "is that furniture between us?" — answered per-viewer.
pub(crate) fn run_conceals(run: &[Cell], player: Cell, viewer: Cell) -> bool {
    if player == viewer {
        return false;
    }
    arm_separates(run, player, viewer)
        || lone_separates(run, player, viewer)
        || ray_crosses_run(run, player, viewer)
}

/// The **one-cell run's** half-plane (§10.3/#562) — the degenerate case of
/// [`arm_separates`], and the only geometry the deployable Cover adds to §10.3.
///
/// §10.1a stamps benches of two or more cells, so until Cover a single covering cell
/// could not exist and the rule had nothing to say about one: [`arm_separates`] wants
/// two adjacent tables to draw a line through, and a lone piece has no second table.
/// What was left was the ray test alone — the quarter-plane straight across the table
/// — which is the pre-#377 rule the half-plane replaced everywhere else *because a
/// player cannot compute a wedge at a glance*. Leaving a lone piece on it would have
/// made the one cover a player places themselves the one cover they cannot read.
///
/// So the lone piece borrows the arm a bench would have had: the line **perpendicular
/// to the direction the player is covering from**, through the table's own cell. Push
/// a table east and stand behind it, and you are hidden from everything east of it —
/// which is exactly the *"push it ahead of you across the room"* play the ability is
/// for, and it is the same half-plane, read the same way, as the bench beside it.
///
/// **Which perpendicular** is decided by the *dominant* axis of the offset from player
/// to table, so it survives the crouch-walk: flush behind the table the answer is the
/// obvious one, and a step along it keeps the same line while the offset stays
/// dominant on that axis. On the **exact diagonal** — the corner hug, where neither
/// axis dominates — there is no honest answer, so this says nothing and the ray test
/// grants the quarter-plane it always did.
///
/// Integer arithmetic and a strict comparison, like everything else here (§12.4).
fn lone_separates(run: &[Cell], player: Cell, viewer: Cell) -> bool {
    let [table] = run else {
        return false;
    };
    let dx = player.x.abs_diff(table.x);
    let dy = player.y.abs_diff(table.y);
    (dx > dy && opposite_sides(player.x, viewer.x, table.x))
        || (dy > dx && opposite_sides(player.y, viewer.y, table.y))
}

/// Whether any straight arm of the run has the player and the viewer strictly on
/// opposite sides of its line (§10.3).
///
/// An *arm* is a direction in which two of the run's tables sit adjacent, so a
/// §10.1a bench contributes its own line and an L-shaped run (two stamped
/// benches touching) contributes one per arm — the arms' half-planes union,
/// which keeps [`cover_run`]'s flood-fill definition of a run intact rather than
/// asking a bent run for a single axis it does not have.
///
/// A lone table has no arm and so hides nobody *this* way; its degenerate arm is
/// [`lone_separates`]'s (§10.3/#562), and past that it falls back to the ray test.
fn arm_separates(run: &[Cell], player: Cell, viewer: Cell) -> bool {
    run.iter().any(|&table| {
        (has_table_south_of(run, table) && opposite_sides(player.x, viewer.x, table.x))
            || (has_table_east_of(run, table) && opposite_sides(player.y, viewer.y, table.y))
    })
}

/// Whether the run has a second table directly south of `table` — i.e. `table`
/// sits on a north–south arm. Checking one direction per axis is enough because
/// [`arm_separates`] asks it of every table in turn.
fn has_table_south_of(run: &[Cell], table: Cell) -> bool {
    table
        .y
        .checked_add(1)
        .is_some_and(|y| run.contains(&Cell::new(table.x, y)))
}

/// Whether the run has a second table directly east of `table` — i.e. `table`
/// sits on an east–west arm.
fn has_table_east_of(run: &[Cell], table: Cell) -> bool {
    table
        .x
        .checked_add(1)
        .is_some_and(|x| run.contains(&Cell::new(x, table.y)))
}

/// Whether `player` and `viewer` lie strictly on opposite sides of the arm's line
/// at `line`, along one axis.
///
/// Sitting *on* the line is neither side: a viewer standing in the bench's own
/// column is looking **along** the furniture rather than across it, and a player
/// there has rounded its end. Both are the ray test's business, not the
/// half-plane's — which is why the two are unioned.
fn opposite_sides(player: u32, viewer: u32, line: u32) -> bool {
    (player < line && viewer > line) || (player > line && viewer < line)
}

/// Whether the straight line between the two cell centres crosses any table of
/// the run — the pre-#377 rule, now the union's second half. Grazing a table's
/// corner counts, out to the exact 45° diagonal, as it always did.
fn ray_crosses_run(run: &[Cell], player: Cell, viewer: Cell) -> bool {
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

    /// **The one-cell run** (§10.3/#562), which is the geometry the deployable Cover
    /// adds: a lone piece defines the line **perpendicular to the direction you are
    /// covering from**, so it conceals across a half-plane exactly as a bench arm does
    /// — the whole point of pushing one ahead of you across a room.
    #[test]
    fn a_lone_piece_conceals_across_its_perpendicular() {
        let run = vec![Cell::new(5, 4)];
        let player = Cell::new(4, 4); // flush behind it, covering eastward
                                      // Everything east of the line x = 5, however far off the table's own row —
                                      // which is the half-plane the pre-#377 wedge could not reach.
        for viewer in [
            Cell::new(6, 4),
            Cell::new(9, 0),
            Cell::new(6, 11),
            Cell::new(11, 11),
        ] {
            assert!(
                run_conceals(&run, player, viewer),
                "{viewer:?} is across the piece",
            );
            assert!(
                !arm_separates(&run, player, viewer),
                "{viewer:?} is not covered by an *arm* — a lone piece has none",
            );
        }
        // …and nothing on the player's own side of it, however far off.
        for viewer in [Cell::new(2, 4), Cell::new(5, 0), Cell::new(4, 11)] {
            assert!(
                !run_conceals(&run, player, viewer),
                "{viewer:?} is on my side of the piece",
            );
        }
        // The other axis, to prove the line follows the *player*, not the grid: the
        // same piece covered from the north hides what is south of it and nothing east.
        let above = Cell::new(5, 3);
        assert!(run_conceals(&run, above, Cell::new(9, 7)));
        assert!(!run_conceals(&run, above, Cell::new(9, 3)));
    }

    /// The corner hug is the one stance a lone piece has no honest line for
    /// (§10.3/#562): on the **exact diagonal** neither axis dominates, so the
    /// half-plane says nothing and the ray test grants the quarter-plane it always
    /// did. Pinned because the silence is a decision, not a gap.
    #[test]
    fn a_lone_piece_hugged_on_the_diagonal_falls_back_to_the_ray() {
        let run = vec![Cell::new(5, 4)];
        let corner = Cell::new(4, 3); // diagonally off it: dx == dy
        assert!(run_hugs(&run, corner), "the pose is held here");
        // No half-plane at all, either way round the diagonal.
        let across = Cell::new(6, 5);
        assert!(!lone_separates(&run, corner, across));
        // What is left is the ray: straight across the piece is covered…
        assert!(run_conceals(&run, corner, across), "the line crosses it");
        // …and a viewer the quarter-plane never reached is still seen.
        assert!(!run_conceals(&run, corner, Cell::new(9, 0)));
    }

    /// A lone piece **joined to a bench is not a lone piece** (§10.3/#562): the flood
    /// gathers one run, the arm rule owns it, and the degenerate line switches itself
    /// off. This is what "cover placed touching furniture extends that run" means in
    /// the geometry rather than only in the terrain.
    #[test]
    fn a_piece_touching_a_bench_conceals_as_the_bench_does() {
        let joined = vec![Cell::new(5, 3), Cell::new(5, 4), Cell::new(5, 5)];
        let player = Cell::new(4, 4);
        let along = Cell::new(4, 0); // up the player's own column, past the run's end
        assert!(
            !lone_separates(&joined, player, along),
            "a three-cell run has arms, so the degenerate rule is silent",
        );
        // And the arm's own half-plane is what answers, exactly as it did before.
        assert!(run_conceals(&joined, player, Cell::new(8, 9)));
        assert!(!run_conceals(&joined, player, along));
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

    /// #377's repro, and the confirmation the ticket asked for. The reported
    /// geometry: crouched round the end of a bench — hugging its last table on the
    /// diagonal, so the pose is held and the run draws Owned — with a guard a
    /// couple of cells to the south-east, off that end. The per-ray rule
    /// *correctly* left that uncovered (no table lies on the line, and it is not
    /// even a corner graze), so #377 was the rule being too tight, not a bug in
    /// the segment test. The arm's half-plane covers it now; the mirror case on
    /// the player's own side stays seen.
    #[test]
    fn a_guard_off_the_ends_far_side_is_covered_by_the_arm_not_the_ray() {
        let run = vec![Cell::new(5, 3), Cell::new(5, 4), Cell::new(5, 5)];
        let player = Cell::new(4, 6); // round the south end, hugging (5,5) diagonally
        assert!(
            run_hugs(&run, player),
            "the pose is held here (the crouch-walk)"
        );
        let far = Cell::new(6, 8); // a couple of cells south-east, across the bench
        assert!(
            !ray_crosses_run(&run, player, far),
            "the old per-ray rule genuinely missed this — the geometry is faithful, \
             the rule was too tight"
        );
        assert!(
            run_conceals(&run, player, far),
            "the bench's line is between them, so the crouch protects"
        );
        // The mirror image, the same way off the end but on the player's own side.
        let near = Cell::new(2, 8);
        assert!(!run_conceals(&run, player, near), "same side of the bench");
    }

    /// The rule the half-plane has to keep honest (§10.3): a bench is directional,
    /// so a guard that has walked round to the **player's** side sees them, and
    /// the cupboard stays the stronger tool. Distance past the ends buys the guard
    /// nothing — only changing sides does.
    #[test]
    fn coming_round_to_the_players_side_sees_them() {
        let run = vec![Cell::new(5, 3), Cell::new(5, 4), Cell::new(5, 5)];
        let player = Cell::new(4, 4);
        for viewer in [
            Cell::new(4, 1), // north, up the player's own column
            Cell::new(3, 4), // due west, behind the player
            Cell::new(4, 8), // south, well past the bench's end
            Cell::new(2, 7), // south-west and far off
            Cell::new(5, 8), // on the bench's own line, past its end
        ] {
            assert!(
                !run_conceals(&run, player, viewer),
                "{viewer:?} is not across the bench from the player"
            );
        }
    }

    /// The regression the per-ray rule was introduced to fix, which the union
    /// preserves: a viewer looking straight **down** the bench's axis is on the
    /// line, not across it, so the half-plane says nothing — the ray test still
    /// blocks it through the intervening tables.
    #[test]
    fn looking_down_the_bench_axis_is_still_blocked() {
        let run = vec![Cell::new(5, 3), Cell::new(5, 4), Cell::new(5, 5)];
        let player = Cell::new(5, 6); // rounded the end, on the bench's own line
        let down_the_axis = Cell::new(5, 1);
        assert!(
            !arm_separates(&run, player, down_the_axis),
            "both stand on the arm's line, so neither is across it"
        );
        assert!(
            run_conceals(&run, player, down_the_axis),
            "the tables between them still block the look"
        );
    }

    /// An L-shaped run (two §10.1a benches that happen to touch) has no single
    /// axis, so each arm contributes its own half-plane and they union — the whole
    /// L is honestly one piece of cover, per [`cover_run`].
    #[test]
    fn each_arm_of_an_l_contributes_its_half_plane() {
        // A vertical arm at x = 5 and a horizontal arm at y = 5, meeting at (5,5).
        let run = vec![
            Cell::new(5, 3),
            Cell::new(5, 4),
            Cell::new(5, 5),
            Cell::new(6, 5),
            Cell::new(7, 5),
        ];
        // The player stands inside the L's elbow.
        let player = Cell::new(4, 4);
        // Across the vertical arm.
        assert!(run_conceals(&run, player, Cell::new(8, 2)));
        // Across the horizontal arm, on the player's side of the vertical one.
        assert!(run_conceals(&run, player, Cell::new(4, 9)));
        // Across both.
        assert!(run_conceals(&run, player, Cell::new(9, 9)));
        // Inside the elbow with the player: neither arm is between them.
        assert!(!run_conceals(&run, player, Cell::new(2, 2)));
    }

    /// **A bench never conceals you from somebody standing next to you** — and that
    /// is a fact about §7.2, not only about §10.3 (#379).
    ///
    /// The takedown is a *bump*, so it needs the guard orthogonally adjacent, and
    /// §7.2 opens the gate against a guard the player is [concealed] from whatever the
    /// angle. Cupboard, duct and cloak all conceal omnidirectionally, so all three
    /// grant that strike. **The crouch cannot**, and neither branch of
    /// [`run_conceals`] can be coaxed into it:
    ///
    /// - [`arm_separates`] wants the two on *strictly* opposite sides of a table's
    ///   line ([`opposite_sides`]), so they differ by at least 2 on that axis;
    /// - [`ray_crosses_run`] wants a table's own cell to meet a segment which, between
    ///   orthogonal neighbours, spans nothing but those two cells — and neither is a
    ///   table, since the player stands on one and the guard on the other.
    ///
    /// So §7.2's third route to a legal strike does not exist as a *crouch*, and a sim
    /// batch reporting zero bench takedowns is the geometry, not a shy bot. Swept
    /// exhaustively below over every stance and every neighbour of the four run shapes
    /// §10.1a can stamp, so a change to either branch that opened the case would fail
    /// here rather than quietly becoming a new play.
    ///
    /// [concealed]: crate::State::concealed_from
    #[test]
    fn an_adjacent_viewer_is_never_concealed_by_a_bench() {
        let runs = [
            vec![Cell::new(5, 3), Cell::new(5, 4), Cell::new(5, 5)], // a north–south bench
            vec![Cell::new(3, 5), Cell::new(4, 5), Cell::new(5, 5)], // an east–west one
            vec![
                Cell::new(5, 3),
                Cell::new(5, 4),
                Cell::new(5, 5),
                Cell::new(6, 5),
            ], // an L
            vec![Cell::new(5, 4)],                                   // a lone table
        ];
        let mut checked = 0;
        for run in &runs {
            for py in 0..12 {
                for px in 0..12 {
                    let player = Cell::new(px, py);
                    if run.contains(&player) {
                        continue; // the player never stands on the furniture
                    }
                    for dir in [(1, 0), (0, 1)] {
                        let viewer = Cell::new(px + dir.0, py + dir.1);
                        if run.contains(&viewer) {
                            continue; // nor does the guard
                        }
                        checked += 1;
                        assert!(
                            !run_conceals(run, player, viewer),
                            "{run:?} concealed {player:?} from the adjacent {viewer:?}",
                        );
                    }
                }
            }
        }
        assert!(
            checked > 500,
            "only {checked} pairs swept — too thin to mean it"
        );
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
