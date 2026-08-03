//! The tunnel walk (§1/§4.5, #466) — the route between the mouth of the player's
//! own tunnel and where they stand.
//!
//! §1 says the intruder *dug the tunnel and came up through it*, and §4.5 says the
//! same hole is the only way out again. The board has always shown both ends of that
//! journey and never the journey itself: a run simply begins with `@` somewhere on
//! forty by forty glyphs, with nothing to draw the eye to it.
//!
//! This module answers the one question an opening animation needs — **which cells
//! does that walk cross?** — and answers it as a pure function of state, so the
//! shell that plays it owns nothing but a clock. Nothing here is game state and
//! nothing here spends a turn: the walk is computed *from* turn zero and changes
//! nothing about it, which is what lets an animation exist at all without weakening
//! §11.1 (see [`ScreenUi::walk`](crate::ScreenUi::walk), the seam it is drawn
//! through).
//!
//! It is deliberately **directionless**. The arrival walks the route one way and the
//! departure — the run's last beat, once the intel is taken and the player steps
//! back down the hole (§4.5) — walks the same cells the other way, so a reversed
//! [`tunnel_walk`] is all the exit animation will need.

use crate::cell::Cell;
use crate::path::route_between;
use crate::state::State;

/// The cells of the walk between the tunnel mouth and the player, **both ends
/// included** — `[exit, …, player]`, each cell one step from the last.
///
/// Reverse it for the departure: the same corridor, walked back to the hole.
///
/// The route is the player's own ([`Terrain::routes_player`](crate::Terrain::routes_player)),
/// so it crosses only cells the player could really walk, and it is the *shortest*
/// such route — the same deterministic breadth-first search a guard's next step
/// comes from (§12.4), so the same board always yields the same walk and a replay of
/// it is reproducible.
///
/// **Empty when there is no route.** §10.6 guarantees the exit is reachable from the
/// spawn on every level that ships, so this is belt-and-braces rather than a case
/// that arises: a caller with nothing to walk simply plays no animation. A player
/// standing *on* the exit yields the one-cell walk `[exit]`, which is the honest
/// answer — they are already there.
///
/// Terrain only: the walk is drawn over a frozen turn-zero board and steps through
/// nothing but geometry, so a guard seated on the route is passed over rather than
/// routed around. It costs a frame of an `@` drawn where a `g` is (glyph priority,
/// §11.3) on a board where §10.1.9 has already guaranteed no guard is watching the
/// spawn — and it buys a route that cannot fail because of where the roster happened
/// to sit.
pub fn tunnel_walk(state: &State) -> Vec<Cell> {
    let facility = state.layout().facility();
    route_between(state.exit(), state.player(), |cell| {
        facility
            .terrain(cell)
            .is_some_and(|terrain| terrain.routes_player())
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Direction;
    use crate::facility::Terrain;
    use crate::state::State;
    use crate::test_support::open_room;

    /// A walled box with the exit stamped in a corner and the player across it.
    fn state(w: u32, h: u32, player: Cell, exit: Cell) -> State {
        State::new(
            open_room(w, h),
            player,
            Direction::South,
            Vec::new(),
            Vec::new(),
            exit,
        )
    }

    /// The shape of the walk, which is the whole contract the animation reads: it
    /// starts on the tunnel mouth, ends where the player stands, and every cell is
    /// one step from the last — so a clock can walk it a cell at a time.
    #[test]
    fn the_walk_runs_from_the_tunnel_mouth_to_the_player_a_step_at_a_time() {
        let (player, exit) = (Cell::new(3, 3), Cell::new(10, 8));
        let s = state(14, 12, player, exit);
        let walk = tunnel_walk(&s);

        assert_eq!(walk.first(), Some(&exit), "it begins at the tunnel");
        assert_eq!(walk.last(), Some(&player), "and ends where control begins");
        assert!(
            walk.windows(2).all(|w| w[0].manhattan_distance(w[1]) == 1),
            "every cell is one step from the last: {walk:?}",
        );
        // The shortest route on an open floor is the Manhattan distance in steps,
        // which is one fewer than the count of cells.
        assert_eq!(walk.len() as u32, exit.manhattan_distance(player) + 1);
    }

    /// The departure is the arrival backwards (§4.5) — the reason this function is
    /// directionless. Pinned so a later exit animation cannot need a second route
    /// that could disagree with this one.
    #[test]
    fn the_departure_is_the_arrival_reversed() {
        let (player, exit) = (Cell::new(2, 6), Cell::new(9, 2));
        let s = state(12, 12, player, exit);
        let walk = tunnel_walk(&s);
        let mut back = walk.clone();
        back.reverse();

        assert_eq!(back.first(), Some(&player), "the departure starts on you");
        assert_eq!(back.last(), Some(&exit), "and ends down the hole");
        assert_eq!(back.len(), walk.len(), "the same corridor, the other way");
    }

    /// The walk is the **player's** route: it goes round a wall rather than through
    /// it, and it never crosses the solid furniture the player would have to bump.
    #[test]
    fn the_walk_takes_the_route_the_player_could_actually_walk() {
        // A wall across the middle of the room, with one gap at its south end.
        let mut layout = open_room(12, 12);
        for y in 1..10 {
            layout.place(Cell::new(6, y), Terrain::Wall);
        }
        // A console on the direct line, to prove solid usables are not crossed either.
        layout.place(Cell::new(3, 10), Terrain::Console);
        let (player, exit) = (Cell::new(2, 10), Cell::new(9, 2));
        let s = State::new(
            layout,
            player,
            Direction::South,
            Vec::new(),
            Vec::new(),
            exit,
        );
        let walk = tunnel_walk(&s);

        assert_eq!(walk.first(), Some(&exit));
        assert_eq!(walk.last(), Some(&player));
        assert!(
            !walk.iter().any(|c| c.x == 6 && (1..10).contains(&c.y)),
            "the wall is walked around, not through: {walk:?}",
        );
        assert!(
            !walk.contains(&Cell::new(3, 10)),
            "a console is a goal to bump, never a cell to cross (§4.3)",
        );
    }

    /// Two degenerate boards, so the shell never has to guess what an odd one means:
    /// standing on the exit is the one-cell walk, and a sealed-off exit is no walk at
    /// all rather than a panic.
    #[test]
    fn a_standing_start_is_one_cell_and_a_sealed_exit_is_no_walk() {
        let on_the_hole = state(10, 10, Cell::new(4, 4), Cell::new(4, 4));
        assert_eq!(tunnel_walk(&on_the_hole), vec![Cell::new(4, 4)]);

        // Wall the exit's corner off entirely.
        let mut layout = open_room(10, 10);
        layout.place(Cell::new(1, 2), Terrain::Wall);
        layout.place(Cell::new(2, 1), Terrain::Wall);
        layout.place(Cell::new(2, 2), Terrain::Wall);
        let sealed = State::new(
            layout,
            Cell::new(6, 6),
            Direction::South,
            Vec::new(),
            Vec::new(),
            Cell::new(1, 1),
        );
        assert!(
            tunnel_walk(&sealed).is_empty(),
            "no route is an empty walk, and the shell plays nothing",
        );
    }

    /// The walk is deterministic (§12.4): the same board yields the same cells every
    /// time, so a recorded run and a replay of it animate identically.
    #[test]
    fn the_walk_is_deterministic() {
        let s = state(16, 16, Cell::new(3, 12), Cell::new(12, 3));
        assert_eq!(tunnel_walk(&s), tunnel_walk(&s));
    }

    /// A real generated level walks: the route exists on every seed §10.6 lets
    /// through, and it is at least [`PLACE`-far](crate::place) — the spawn is kept
    /// clear of the exit on purpose, and that distance is exactly what the animation
    /// is covering.
    #[test]
    fn a_generated_level_has_a_walk_worth_playing() {
        for seed in 0..16u64 {
            let level = crate::LevelSeed::quick_play(seed);
            let s = crate::start_level(&level).expect("the v1 footprint always carves");
            let walk = tunnel_walk(&s);
            assert_eq!(walk.first(), Some(&s.exit()), "seed {seed}");
            assert_eq!(walk.last(), Some(&s.player()), "seed {seed}");
            assert!(
                walk.len() > 1,
                "seed {seed}: placement keeps the spawn off the exit, so there is a walk",
            );
            assert!(
                walk.windows(2).all(|w| w[0].manhattan_distance(w[1]) == 1),
                "seed {seed}: {walk:?}",
            );
        }
    }
}
