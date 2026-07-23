//! Resolving an ability's [`TargetingMode`] to a concrete, validated target
//! (§8.4) — **built up front, in the core, before the ability space fills in**.
//!
//! The old version had *no* targeting system: every ability was self- or
//! auto-targeted at the nearest valid thing, and that path of least resistance
//! *is* what produced the free unlimited-range neutralise (§8.4). So the rule
//! this module enforces is the inverse: a target is either the player's own cell,
//! a cardinal the player **chose**, or a cell the player **steered a cursor to** —
//! never the nearest anything. Nothing here scans the board for a candidate.
//!
//! # The two halves
//!
//! - [`Target`] is the *resolved* result an ability effect consumes — a self
//!   cell, a [`Direction`], or a validated in-range [`Cell`]. Producing one is the
//!   whole point; an out-of-range or off-grid pick is **rejected**, never clamped.
//! - [`Targeting`] is a *live session* the shell drives while the player aims: it
//!   opens from a mode ([`Targeting::begin`]), takes cardinal input
//!   ([`Targeting::steer`]), and resolves on [`Targeting::confirm`]. Cancelling is
//!   just dropping the session — it owns no world state, so there is nothing to
//!   undo and no turn is spent (§4.4-style).
//!
//! The session lives in **core** so validity is deterministic and testable
//! without a browser (§12.1): core owns *what is a legal target*; the render/input
//! crate only draws the cursor and forwards key events. "Within range" is the same
//! **square box** sight uses (§6.1, [`Cell::sight_distance`]) — one consistent
//! notion of range, not a second metric that could disagree.
//!
//! What this module deliberately does **not** decide is whether a targeted cell is
//! a *good* target for a particular effect — a decoy needs enterable floor, a
//! future bolt might need a guard. That per-effect check stays with the effect
//! (as [`decoy_spawn_cell`](crate::State) already does its own), exactly so the
//! targeting layer answers one question — *is this cell in range and on the grid* —
//! and answers it the same way for every ability.

use crate::ability::TargetingMode;
use crate::cell::{Cell, Direction};
use crate::facility::Facility;

/// A resolved, validated target an ability effect consumes (§8.4).
///
/// Each arm corresponds to a [`TargetingMode`], carrying the concrete thing the
/// mode resolves to: the self cell, the chosen cardinal, or a cell already
/// checked to be in range and on the grid. An effect that receives a `Target`
/// can act on it without re-validating the range — that check happened when the
/// session confirmed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// [`TargetingMode::Itself`]: the player's own cell (Run, Camouflage, Dephase).
    Itself(Cell),
    /// [`TargetingMode::Direction`]: the cardinal the player faces or chose (Decoy).
    Direction(Direction),
    /// [`TargetingMode::Tile`]: a cell within the §6.1 range box, already validated.
    Tile(Cell),
}

/// Whether `cell` is within a §6.1 **box** of radius `range` around `origin` — the
/// single "within range" notion, the same square shape and metric as sight
/// ([`Cell::sight_distance`]), so range and sight can never disagree (§6.1).
///
/// This is range only. It says nothing about the grid's edges or the cell's
/// terrain: [`TileCursor`] pairs it with an [`in_bounds`](Facility::in_bounds)
/// check, and per-effect terrain rules stay with the effect.
pub fn within_range(origin: Cell, cell: Cell, range: u32) -> bool {
    origin.sight_distance(cell) <= range
}

/// The cursor that resolves [`TargetingMode::Tile`] (§8.4): a movable pick inside
/// the §6.1 range box, kept **always valid** so a confirm can never yield an
/// illegal cell.
///
/// It starts on the player's own cell — the one cell always in range (distance 0)
/// and on the grid — rather than snapping to some nearby candidate: the player
/// steers it out to the target themselves, which is the §8.4 "no auto-target"
/// rule made literal. Both drivers — cardinal [`step`](Self::step) for the
/// keyboard and [`try_place`](Self::try_place) for a pointer — **refuse** a move
/// that would leave the box or the grid rather than clamping to the nearest legal
/// cell; the cursor simply holds, the way a blocked step is a free no-op (§4.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TileCursor {
    origin: Cell,
    range: u32,
    cursor: Cell,
}

impl TileCursor {
    /// Open a cursor centred on `origin` (the player's cell) with a §6.1 box
    /// radius of `range`. The cursor starts *on* the origin — always a legal
    /// pick, and never an auto-aim at something nearby.
    pub fn new(origin: Cell, range: u32) -> Self {
        Self {
            origin,
            range,
            cursor: origin,
        }
    }

    /// The box centre — the player's cell and the range anchor.
    pub fn origin(&self) -> Cell {
        self.origin
    }

    /// The §6.1 box radius this cursor is bounded to.
    pub fn range(&self) -> u32 {
        self.range
    }

    /// Where the cursor currently sits — always in range and on the grid.
    pub fn cursor(&self) -> Cell {
        self.cursor
    }

    /// Whether `cell` is a legal target for this cursor: on the grid **and** within
    /// the §6.1 range box. The one gate both movement drivers pass a candidate
    /// through, and the reason a confirmed [`Target::Tile`] never needs
    /// re-checking.
    pub fn accepts(&self, cell: Cell, facility: &Facility) -> bool {
        facility.in_bounds(cell) && within_range(self.origin, cell, self.range)
    }

    /// Move the cursor one cell in `dir`. Returns whether it moved: a step that
    /// would leave the grid or the range box is **refused** (the cursor holds, no
    /// move), never clamped to the nearest legal cell (§8.4). Cardinal only — the
    /// grid has no diagonal (§4.1).
    pub fn step(&mut self, dir: Direction, facility: &Facility) -> bool {
        let Some(next) = self.cursor.step(dir) else {
            return false;
        };
        if self.accepts(next, facility) {
            self.cursor = next;
            true
        } else {
            false
        }
    }

    /// Jump the cursor straight to `cell` — the pointer driver (a click/tap). Same
    /// gate as [`step`](Self::step): an in-range on-grid cell is accepted and the
    /// cursor moves there; an out-of-range or off-grid cell is **rejected** and the
    /// cursor does not move. Returns whether it moved.
    pub fn try_place(&mut self, cell: Cell, facility: &Facility) -> bool {
        if self.accepts(cell, facility) {
            self.cursor = cell;
            true
        } else {
            false
        }
    }

    /// Resolve the current cursor cell as the target. Always a valid
    /// [`Target::Tile`], because the cursor is kept inside the box.
    pub fn confirm(&self) -> Target {
        Target::Tile(self.cursor)
    }
}

/// A live targeting session the shell drives while the player aims (§8.4).
///
/// Opened from a mode by [`begin`](Self::begin) against the player's cell and
/// facing; steered by cardinal input ([`steer`](Self::steer)); resolved by
/// [`confirm`](Self::confirm). It holds **no world state** — cancelling is
/// dropping the value, which changes nothing and spends no turn (§4.4).
///
/// `Itself` and `Direction` resolve the instant the session opens (there is
/// nothing off-cell to steer for a self-target, and a direction defaults to the
/// player's §5 facing); a cardinal key still *re-aims* a `Direction` session so
/// the player can throw somewhere other than straight ahead. `Tile` carries a
/// [`TileCursor`] and needs the player to steer it out and confirm.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Targeting {
    /// A self-target — resolved on open; nothing to steer.
    Itself(Cell),
    /// A direction-target — starts at the player's facing (§5); a cardinal key
    /// re-aims it, so the chosen cardinal, not only the faced one, is reachable.
    Direction(Direction),
    /// A tile-target — a [`TileCursor`] the player steers inside the range box.
    Tile(TileCursor),
}

impl Targeting {
    /// Open a session for `mode`, anchored at the player's `origin` cell and
    /// `facing` (§5). `Itself` and `Direction` come back already resolvable;
    /// `Tile` comes back with a fresh cursor on the origin.
    pub fn begin(mode: TargetingMode, origin: Cell, facing: Direction) -> Self {
        match mode {
            TargetingMode::Itself => Targeting::Itself(origin),
            TargetingMode::Direction => Targeting::Direction(facing),
            TargetingMode::Tile { range } => Targeting::Tile(TileCursor::new(origin, range)),
        }
    }

    /// Feed a cardinal to the session. Returns whether it changed the aim:
    /// - `Itself` never does — a self-target has nothing to steer.
    /// - `Direction` always does — the cardinal *becomes* the chosen direction.
    /// - `Tile` does when the cursor moved and refuses at the box/grid edge
    ///   ([`TileCursor::step`]).
    pub fn steer(&mut self, dir: Direction, facility: &Facility) -> bool {
        match self {
            Targeting::Itself(_) => false,
            Targeting::Direction(chosen) => {
                *chosen = dir;
                true
            }
            Targeting::Tile(cursor) => cursor.step(dir, facility),
        }
    }

    /// Resolve the session into its [`Target`]. Always succeeds: every arm is kept
    /// in a valid state throughout (the cursor never leaves the box), so there is
    /// no failure to report here — an illegal pick was already refused at the
    /// point it was made, not deferred to confirm.
    pub fn confirm(&self) -> Target {
        match self {
            Targeting::Itself(cell) => Target::Itself(*cell),
            Targeting::Direction(dir) => Target::Direction(*dir),
            Targeting::Tile(cursor) => cursor.confirm(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facility::Facility;

    /// Range is the §6.1 **box**, not a Manhattan diamond: a cell two east and two
    /// south of the origin is `sight_distance` 2, so it is inside a range-2 box even
    /// though its Manhattan distance is 4. This is the shape sight uses, and the
    /// point of sharing one notion of range (§6.1).
    #[test]
    fn within_range_is_the_square_box() {
        let origin = Cell::new(10, 10);
        // The whole 5×5 box around the origin is in range 2 — corners included.
        for dx in 0..=2u32 {
            for dy in 0..=2u32 {
                assert!(within_range(origin, Cell::new(10 + dx, 10 + dy), 2));
            }
        }
        // One cell past any edge of the box is out — the corner diagonal too.
        assert!(!within_range(origin, Cell::new(13, 10), 2));
        assert!(!within_range(origin, Cell::new(10, 13), 2));
        assert!(!within_range(origin, Cell::new(13, 13), 2));
        // A Manhattan-4 cell that is Chebyshev-2 is *in* range — box, not diamond.
        assert!(within_range(origin, Cell::new(12, 12), 2));
    }

    /// Self mode resolves to the player's own cell, with no interaction (§8.4).
    #[test]
    fn self_mode_resolves_to_the_player_cell() {
        let facility = Facility::walled_box(20, 20);
        let mut session =
            Targeting::begin(TargetingMode::Itself, Cell::new(5, 5), Direction::North);
        // Nothing to steer; the aim never changes.
        assert!(!session.steer(Direction::East, &facility));
        assert_eq!(session.confirm(), Target::Itself(Cell::new(5, 5)));
    }

    /// Direction mode resolves the faced cardinal by default, and a cardinal key
    /// re-aims it to the chosen one (§8.4 — "the faced/chosen cardinal").
    #[test]
    fn direction_mode_resolves_the_faced_then_chosen_cardinal() {
        let facility = Facility::walled_box(20, 20);
        let mut session =
            Targeting::begin(TargetingMode::Direction, Cell::new(5, 5), Direction::North);
        // Defaults to facing.
        assert_eq!(session.confirm(), Target::Direction(Direction::North));
        // A cardinal key chooses a different direction.
        assert!(session.steer(Direction::East, &facility));
        assert_eq!(session.confirm(), Target::Direction(Direction::East));
    }

    /// Tile mode: a cursor that starts on the player, steers by cardinals, and
    /// resolves to the cell it sits on (§8.4). The start is the origin — never an
    /// auto-aim at something nearby.
    #[test]
    fn tile_mode_starts_on_the_player_and_steers() {
        let facility = Facility::walled_box(20, 20);
        let origin = Cell::new(10, 10);
        let mut session =
            Targeting::begin(TargetingMode::Tile { range: 3 }, origin, Direction::North);
        assert_eq!(
            session.confirm(),
            Target::Tile(origin),
            "starts on the player"
        );
        assert!(session.steer(Direction::East, &facility));
        assert!(session.steer(Direction::East, &facility));
        assert_eq!(session.confirm(), Target::Tile(Cell::new(12, 10)));
    }

    /// The cursor **accepts** an in-range cell and **rejects** an out-of-range one,
    /// rather than clamping to the nearest legal cell (§8.4). At the box edge a
    /// further step simply holds.
    #[test]
    fn tile_cursor_accepts_in_range_and_rejects_out_of_range() {
        let facility = Facility::walled_box(20, 20);
        let origin = Cell::new(10, 10);
        let mut cursor = TileCursor::new(origin, 2);

        // Steer to the eastern edge of the range-2 box (12,10).
        assert!(cursor.step(Direction::East, &facility));
        assert!(cursor.step(Direction::East, &facility));
        assert_eq!(cursor.cursor(), Cell::new(12, 10));
        // One more east is out of range: refused, cursor holds — not clamped.
        assert!(!cursor.step(Direction::East, &facility));
        assert_eq!(cursor.cursor(), Cell::new(12, 10));

        // The pointer driver enforces the same gate: an in-range jump is taken,
        // an out-of-range jump is rejected and the cursor does not move.
        assert!(cursor.try_place(Cell::new(8, 9), &facility)); // Chebyshev 2 — in.
        assert_eq!(cursor.cursor(), Cell::new(8, 9));
        assert!(!cursor.try_place(Cell::new(13, 13), &facility)); // Chebyshev 3 — out.
        assert_eq!(
            cursor.cursor(),
            Cell::new(8, 9),
            "rejected pick did not move it"
        );
    }

    /// The cursor never leaves the grid either: a range box that overhangs the
    /// north/west edge still cannot be steered off it (§8.4 validity is range *and*
    /// bounds).
    #[test]
    fn tile_cursor_will_not_leave_the_grid() {
        let facility = Facility::walled_box(20, 20);
        // Origin one cell from the west wall, range 3 — the box overhangs west.
        let mut cursor = TileCursor::new(Cell::new(1, 5), 3);
        assert!(cursor.step(Direction::West, &facility)); // to (0,5)
        assert_eq!(cursor.cursor(), Cell::new(0, 5));
        // Off the west edge is out of bounds: refused despite being in range.
        assert!(!cursor.step(Direction::West, &facility));
        assert_eq!(cursor.cursor(), Cell::new(0, 5));
    }

    /// A confirmed tile is always valid, because every path to it was gated: the
    /// resolved cell is in range and on the grid without any confirm-time check.
    #[test]
    fn a_confirmed_tile_is_always_in_range() {
        let facility = Facility::walled_box(20, 20);
        let origin = Cell::new(10, 10);
        let mut session =
            Targeting::begin(TargetingMode::Tile { range: 4 }, origin, Direction::North);
        for dir in [
            Direction::North,
            Direction::East,
            Direction::East,
            Direction::South,
        ] {
            session.steer(dir, &facility);
        }
        let Target::Tile(cell) = session.confirm() else {
            panic!("tile session resolves to a tile");
        };
        assert!(within_range(origin, cell, 4));
        assert!(facility.in_bounds(cell));
    }
}
