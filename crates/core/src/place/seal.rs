//! Whether stamping a **solid usable** would seal walkable ground off (§10.3/§10.6).
//!
//! An intel console, the comms console and the exit all stamp in solid (§10.3): they
//! block movement, and — the part §10.3 spells out and the code has to act on —
//! nothing a bump does to one lets anyone past. A closed door panel is pathable
//! because walking into it *opens the way* (§10.4); a console is not, because using it
//! opens nothing. So a solid usable dropped into a one-cell throat is a wall, and the
//! ground behind it belongs to nobody: guards cannot route to it, the player cannot
//! walk to it, and both can still see in, because a usable does not block sight. A
//! visible alcove nobody can ever enter (#477, #481).
//!
//! Nothing caught it. [`solvable`](super::solvable) is the §10.6 assert and it proves
//! the player's *objective* route survives the stamping — orphaned ground holds no
//! objective, so the flood never has a reason to visit it. The §10.5 region graph is
//! blind for a different reason: beats are cut before the usables are stamped (that
//! happens in [`State::new`](crate::State::new), not in the generator), so the graph
//! the partition is computed on still shows plain floor where the console will land.
//!
//! Two functions, and the split is the ticket's own risk note. [`seals_ground`] is the
//! **candidate filter**: placement skips a cell whose stamping would disconnect the
//! walkable graph, which costs no redraws — an assert on the finished board would cost
//! one per bad seed, and appendix 17 records exactly that failure mode stalling
//! generation when the one-usable rule was a hard guarantee. [`nothing_orphaned`] is
//! the finished-board assert anyway, because §10.6 is explicit that a generator must
//! never *believe* a reachability property: the filter makes it true, this makes it
//! checked, and it is a flood fill that costs nothing.
//!
//! **Both movement rules, not one** (§10.3). A guard refuses a cupboard and partial
//! cover where the player refuses neither, so neither rule implies the other: a
//! detour through a cupboard is the player's alone, and a cupboard's single mouth is
//! ground only the player loses. A pocket orphaned for guards while the player still
//! reaches it is a coverage hole worth refusing just the same.

use crate::cell::Cell;
use crate::facility::{Facility, Terrain};
use crate::guard::routable;
use crate::path;
use std::collections::HashSet;

/// Whether stamping a solid usable at `cell` would cut walkable ground off — given
/// `stamped`, the usable cells placement has already chosen (recorded but, at this
/// point in generation, **not yet stamped**, so the grid still shows plain floor
/// under every one of them).
///
/// The check is the O(ring) local one `generate::severs_pathing` makes of a table
/// before stamping it, not a full-grid flood: if every walkable neighbour of `cell` can
/// still reach every other within the 3×3 ring around it, then any route that ran
/// through `cell` has a local detour and connectivity survives. That is sound in the
/// direction that matters — it
/// never *passes* a cell that seals — and conservative in the other, refusing the odd
/// safe cell whose detour is longer than a ring. Placement can always pick another
/// cell, so paying for the exact answer would buy nothing.
///
/// Threading `stamped` through is what makes a *pair* of usables that jointly seal a
/// throat the responsibility of whichever lands second: each candidate is judged on
/// the graph the previous stamps left behind. With the §10.6 gate proving the bare
/// carve is one component, that induction is the whole guarantee.
pub(super) fn seals_ground(facility: &Facility, cell: Cell, stamped: &[Cell]) -> bool {
    severs(facility, cell, stamped, guard_walkable) || severs(facility, cell, stamped, walkable)
}

/// Whether every walkable cell of the finished board is still reachable from every
/// other — the §10.6 assert [`seals_ground`] exists to keep quiet.
///
/// Stated as one component per movement rule rather than as a flood from the player's
/// entry, because that is the property both halves of the acceptance need and neither
/// direction implies the other: [`solvable`](super::solvable) separately proves the
/// player's entry is *in* the player's component, and a guard placed anywhere is in
/// the guard's.
pub(super) fn nothing_orphaned(facility: &Facility, stamped: &[Cell]) -> bool {
    one_component(facility, stamped, guard_walkable) && one_component(facility, stamped, walkable)
}

/// Whether a **player** may come to occupy `cell` (§10.3): floor, either door panel
/// pose (a bump opens a closed one, §10.4) and a cupboard (bump-to-enter). The single
/// source of truth is [`Terrain::routes_player`].
fn walkable(facility: &Facility, cell: Cell) -> bool {
    facility.terrain(cell).is_some_and(Terrain::routes_player)
}

/// Whether a **guard's** walk may cross `cell` — [`routable`], which already treats a
/// stamped usable as solid (it pairs the pathing rule with the move check, §10.3).
/// The cells of `stamped` are masked separately by the callers below, because at
/// placement time they are still plain floor on the grid.
fn guard_walkable(facility: &Facility, cell: Cell) -> bool {
    routable(facility, cell)
}

/// Whether removing `cell` would disconnect its walkable neighbours under `rule`.
fn severs(
    facility: &Facility,
    cell: Cell,
    stamped: &[Cell],
    rule: fn(&Facility, Cell) -> bool,
) -> bool {
    let open = |c: Cell| c != cell && !stamped.contains(&c) && rule(facility, c);
    // The walkable orthogonal neighbours — the cells that must stay mutually
    // reachable. One or none of them, and there is nothing to keep connected: a
    // usable at the end of a dead end orphans only itself, and it is solid anyway.
    let targets: Vec<Cell> = facility.neighbours(cell).filter(|&n| open(n)).collect();
    if targets.len() <= 1 {
        return false;
    }
    // Flood the walkable ring (Chebyshev ≤ 1 of `cell`, excluding `cell` itself) from
    // one target; reaching every other proves a detour exists. Deliberately the
    // O(ring) local flood rather than `path::flood_from` over the whole level — this
    // runs per *candidate cell*, inside the generation retry loop.
    let in_ring = |c: Cell| c != cell && cell.sight_distance(c) <= 1;
    let mut seen = vec![targets[0]];
    let mut stack = vec![targets[0]];
    while let Some(c) = stack.pop() {
        for n in facility.neighbours(c) {
            if in_ring(n) && open(n) && !seen.contains(&n) {
                seen.push(n);
                stack.push(n);
            }
        }
    }
    !targets.iter().all(|t| seen.contains(t))
}

/// Whether the cells `rule` admits — `stamped` masked solid — form a single
/// 4-connected component. The §10.6 flood fill, leaning on
/// [`path::flood_from`]'s bit grid like the carve gate does.
fn one_component(facility: &Facility, stamped: &[Cell], rule: fn(&Facility, Cell) -> bool) -> bool {
    let (w, h) = (facility.width(), facility.height());
    let solid: HashSet<Cell> = stamped.iter().copied().collect();
    let open = |c: Cell| !solid.contains(&c) && rule(facility, c);
    let all: Vec<Cell> = (0..h)
        .flat_map(|y| (0..w).map(move |x| Cell::new(x, y)))
        .filter(|&c| open(c))
        .collect();
    let Some(&start) = all.first() else {
        return false; // a level with nowhere to stand is not a level
    };
    path::flood_from(start, w, h, open).len() == all.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A room with a one-cell throat into a two-cell alcove:
    ///
    /// ```text
    ///  #######
    ///  #..TPP#     T = the throat, P = the alcove behind it
    ///  #..####
    ///  #.....#
    ///  #.....#
    ///  #.....#
    ///  #######
    /// ```
    fn throat() -> (Facility, Cell) {
        let mut f = Facility::walled_box(7, 7);
        for x in 3..=5 {
            f.set_terrain(x, 2, Terrain::Wall);
        }
        (f, Cell::new(3, 1))
    }

    /// The whole point: a usable stamped in the throat seals the alcove, so the
    /// candidate is refused — and a cell out in the open beside it is not.
    #[test]
    fn a_one_cell_throat_is_refused_and_open_floor_is_not() {
        let (f, throat) = throat();
        assert!(seals_ground(&f, throat, &[]), "the throat seals the alcove");
        assert!(!seals_ground(&f, Cell::new(1, 1), &[]), "a corner is free");
        assert!(
            !seals_ground(&f, Cell::new(3, 4), &[]),
            "open floor is free"
        );
        // A dead end orphans only itself, and it is solid anyway: the far cell of
        // the alcove has one walkable neighbour, so there is nothing to disconnect.
        assert!(!seals_ground(&f, Cell::new(5, 1), &[]));
    }

    /// The assert's side of the same board: with nothing stamped every cell is
    /// reachable, and with the throat stamped the alcove is not.
    #[test]
    fn the_finished_board_assert_sees_the_alcove_the_filter_would_have_refused() {
        let (f, throat) = throat();
        assert!(nothing_orphaned(&f, &[]));
        assert!(!nothing_orphaned(&f, &[throat]));
        // The dead end really is one: stamping it costs the board nothing.
        assert!(nothing_orphaned(&f, &[Cell::new(5, 1)]));
    }

    /// A **two-cell** throat takes one usable and no more: the parallel cell keeps
    /// the ground beyond connected, so the first stamp is allowed — and the second,
    /// judged against the graph the first left behind, is refused. This is the pair
    /// that jointly seals, and why `stamped` is threaded through the filter.
    #[test]
    fn the_second_usable_of_a_pair_is_judged_on_what_the_first_left() {
        // A two-row corridor; the column x = 3 is its two-cell throat.
        //  #######
        //  #..u..#     u = the upper throat cell, l = the lower
        //  #..l..#
        //  #######
        let f = Facility::walled_box(7, 4);
        let (upper, lower) = (Cell::new(3, 1), Cell::new(3, 2));
        assert!(!seals_ground(&f, upper, &[]), "the other row still routes");
        assert!(!seals_ground(&f, lower, &[]));
        assert!(nothing_orphaned(&f, &[upper]));

        // …but the second one, with the first already claimed, closes the throat.
        assert!(seals_ground(&f, lower, &[upper]));
        assert!(seals_ground(&f, upper, &[lower]));
        assert!(!nothing_orphaned(&f, &[upper, lower]));
    }

    /// **Both movement rules.** A cupboard has exactly one floor neighbour (§10.1.6),
    /// so a usable stamped at its mouth strands it — ground only the *player* could
    /// ever have used, which a guard-only check would wave through.
    #[test]
    fn a_cupboard_mouth_is_refused_though_no_guard_wanted_the_cupboard() {
        let mut f = Facility::walled_box(7, 5);
        // A recess in the north wall, its only floor neighbour the mouth below it.
        f.set_terrain(3, 0, Terrain::Hideout);
        let mouth = Cell::new(3, 1);
        assert!(
            !severs(&f, mouth, &[], guard_walkable),
            "a guard routes around a cupboard, so it loses nothing"
        );
        assert!(
            severs(&f, mouth, &[], walkable),
            "the player loses the only way in"
        );
        assert!(seals_ground(&f, mouth, &[]));
    }

    /// …and the mirror: ground a *guard* loses while the player keeps it. A cupboard
    /// is the player's detour and nobody else's, so a throat whose only bypass runs
    /// through one severs the patrol graph alone.
    #[test]
    fn a_detour_through_a_cupboard_saves_the_player_and_not_the_guard() {
        // A 1×3 corridor east–west, with a cupboard providing a parallel bypass:
        //  #####
        //  #.}.#      } = the cupboard, at (2,1)
        //  #.T.#      T = the candidate, at (2,2)
        //  #####
        let mut f = Facility::walled_box(5, 4);
        f.set_terrain(2, 1, Terrain::Hideout);
        let throat = Cell::new(2, 2);
        assert!(
            severs(&f, throat, &[], guard_walkable),
            "a patrol has no way round"
        );
        assert!(
            !severs(&f, throat, &[], walkable),
            "the player slips through the cupboard"
        );
        assert!(
            seals_ground(&f, throat, &[]),
            "a guard-only loss still counts"
        );
    }

    /// A closed door panel is walkable to both (§10.4 — the bump opens it), so a
    /// usable beside one is judged against a route that runs *through* the door.
    #[test]
    fn a_closed_panel_is_a_route_and_not_a_wall() {
        //  #####
        //  #.+.#      + = a closed panel at (2,1)
        //  #.T.#      T = the candidate at (2,2)
        //  #####
        let mut f = Facility::walled_box(5, 4);
        f.set_terrain(2, 1, Terrain::DoorPanelClosed);
        assert!(!seals_ground(&f, Cell::new(2, 2), &[]));
        // Close that route with a hinge and the candidate becomes the only throat.
        f.set_terrain(2, 1, Terrain::DoorHinge);
        assert!(seals_ground(&f, Cell::new(2, 2), &[]));
    }
}
