//! The facility's static geometry: a grid of terrain cells (§4.1, §10).
//!
//! This is the substrate every other system will read — vision, sound, guards
//! and generation all query terrain. Right now it holds the smallest honest
//! version of that world: a rectangle of floor wrapped in the indestructible
//! 1-cell border the design guarantees (§4.1, §10.6). The real generator
//! (corridor-first partition, §10.1) grows *inside* this type in its own ticket;
//! nothing here should have to change when it lands.
//!
//! Terrain started as two kinds — floor and wall — carrying only their glyph.
//! Doors (§10.4) were the first terrain whose §10.3 properties actually diverge —
//! a closed panel is opaque and solid yet *transparent to pathfinding*. The rest
//! of the static §10.3 table lands here: hideouts, consoles, the exit. Each new
//! kind extends the same exhaustive property matches, so the compiler makes the
//! table's completeness structural — a missing column will not build.
//!
//! What is *not* here is the entity half of the §10.3 table — player, guards,
//! bodies, decoys. Those are entities the level owns (§12.3), not terrain stamped
//! into the grid; they contribute a *fill* to a cell's occupancy, which is what
//! [`Facility::can_enter`] sums. The "occupied hideout" row is the same story: a
//! [`Terrain::Hideout`] is the empty alcove, and its occupied properties (solid,
//! sound only partially muffled) arise when an occupant's fill sits in it — an
//! occupancy overlay the sound and turn tickets read, not a second terrain kind.

use crate::category::Category;
use crate::cell::{Cell, Direction};

/// A kind of cell. The vocabulary the facility grid stores; the §10.3 terrain
/// table lives on it as [`Terrain`]'s property methods.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Terrain {
    /// Walkable empty ground. Renders blank (§10.3).
    Floor,
    /// Solid, sight-blocking structure. Renders `#` (§10.3).
    Wall,
    /// A door's frame end — permanently solid and opaque, the handle you bump to
    /// close (§10.4). One at each end of a doorway. Renders `×` (§10.3).
    DoorHinge,
    /// A closed door panel: solid, opaque, and it attenuates sound — but it is
    /// deliberately **transparent to pathfinding**, so guards route through a
    /// closed door and open it by walking in (§10.4). Renders `+` (§10.3).
    DoorPanelClosed,
    /// An open door panel: walk-through, like floor (§10.4). Renders blank (§10.3).
    DoorPanelOpen,
    /// A hideout: an alcove carved into the wall the player can duck into (§10.1).
    /// Empty it is walk-through (fill 0.0) yet **blocks pathfinding**, so guard
    /// patrols route *around* it while the player slips *in* — that asymmetry is
    /// the hiding mechanic. Renders `}` (§10.3). Its occupied state is an
    /// occupancy overlay, not a separate kind (see the module note).
    Hideout,
    /// Partial cover — a table (§10.1a, §10.3): the furniture the sightline pass
    /// stamps into over-long straight runs. Solid to movement and pathing like a
    /// wall, but it does **not block sight** — a guard sees straight over it. Its
    /// counterplay is behavioural, not optical: a player who spends a turn
    /// *waiting* beside one auto-crouches and is concealed from any viewer whose
    /// line of sight crosses the table ([`provides_cover`](Self::provides_cover);
    /// the crouch itself lives on the turn loop). Renders `π` (§10.3).
    PartialCover,
    /// A console — the intel terminal you bump to use (§4.3). Solid (you cannot
    /// share its cell) but transparent to sight, pathing and sound. Renders `$`
    /// (§10.3, §11.3).
    Console,
    /// The exit: where a laden player leaves to win (§4.5). Solid but otherwise
    /// transparent, like a console. Renders `E` (§10.3, §11.3).
    Exit,
}

impl Terrain {
    /// The character this terrain renders as (§10.3). Floor and an open panel are
    /// blank; the renderer paints them as background.
    pub fn glyph(self) -> char {
        match self {
            Terrain::Floor | Terrain::DoorPanelOpen => ' ',
            Terrain::Wall => '#',
            Terrain::DoorHinge => '×',
            Terrain::DoorPanelClosed => '+',
            Terrain::Hideout => '}',
            Terrain::PartialCover => 'π',
            Terrain::Console => '$',
            Terrain::Exit => 'E',
        }
    }

    /// The information category this terrain declares (§11.2). The renderer tags
    /// each cell with this; the platform shell owns the category → colour table, so
    /// no concrete colour is named here. Walls are inert **Neutral**; floor (and the
    /// walkable gap of an open panel) is **Ground**, drawn to recede so everything
    /// else pops; doors and hideouts are **System** furniture; a console (intel) and
    /// the exit are **Interest** — a goal or reward.
    pub fn category(self) -> Category {
        match self {
            Terrain::Wall => Category::Neutral,
            Terrain::Floor | Terrain::DoorPanelOpen => Category::Ground,
            Terrain::DoorHinge
            | Terrain::DoorPanelClosed
            | Terrain::Hideout
            | Terrain::PartialCover => Category::System,
            Terrain::Console | Terrain::Exit => Category::Interest,
        }
    }

    /// This terrain's contribution to a cell's occupancy fill (§4.3). Fill 1.0 is
    /// solid and exclusive; 0.0 is walk-through. A move into a cell succeeds when
    /// the fills already there plus the mover's stay ≤ 1.0.
    pub fn fill(self) -> f32 {
        match self {
            Terrain::Floor | Terrain::DoorPanelOpen | Terrain::Hideout => 0.0,
            Terrain::Wall
            | Terrain::DoorHinge
            | Terrain::DoorPanelClosed
            | Terrain::PartialCover
            | Terrain::Console
            | Terrain::Exit => 1.0,
        }
    }

    /// Whether terrain alone blocks a mover from entering the cell (§4.3, §10.3).
    /// A blocked move is an *interaction*, not a failure — bumping a closed door
    /// opens it rather than stopping you dead (§4.3).
    pub fn blocks_movement(self) -> bool {
        self.fill() >= 1.0
    }

    /// Whether this terrain blocks sight (§10.3). Opacity itself is still
    /// all-or-nothing; partial cover is see-through — its concealment is the
    /// crouch behaviour ([`provides_cover`](Self::provides_cover)), not opacity.
    pub fn blocks_sight(self) -> bool {
        match self {
            Terrain::Floor
            | Terrain::DoorPanelOpen
            | Terrain::Hideout
            | Terrain::PartialCover
            | Terrain::Console
            | Terrain::Exit => false,
            Terrain::Wall | Terrain::DoorHinge | Terrain::DoorPanelClosed => true,
        }
    }

    /// Whether this terrain is **partial cover** (§10.1a/§10.3): see-through, but a
    /// player crouched beside it is concealed from any viewer whose line of sight
    /// crosses it. Cover also *terminates* a §10.1a sightline run — the rule
    /// guarantees counterplay on every long straight, which cover provides without
    /// blocking sight.
    pub fn provides_cover(self) -> bool {
        matches!(self, Terrain::PartialCover)
    }

    /// Whether this terrain blocks **pathfinding** (§10.3). The one deliberate
    /// surprise: a **closed door panel does not**. Guards route through closed
    /// doors and open them by walking in, so guard traffic monotonically opens the
    /// facility up over a level (§10.4).
    pub fn blocks_pathing(self) -> bool {
        match self {
            // A hideout blocks pathing too: guard routes flow around it while the
            // player ducks in — the asymmetry that makes it a hiding place (§10.1).
            // A table is solid furniture: patrols route around it like a wall.
            Terrain::Wall | Terrain::DoorHinge | Terrain::Hideout | Terrain::PartialCover => true,
            Terrain::Floor
            | Terrain::DoorPanelClosed
            | Terrain::DoorPanelOpen
            | Terrain::Console
            | Terrain::Exit => false,
        }
    }

    /// How this terrain affects sound crossing it (§9.1, §10.3). Sound flows around
    /// walls, not through them; a closed door only **attenuates**, which is what
    /// gives "close the door behind you" a point.
    pub fn sound(self) -> SoundBlocking {
        match self {
            // Empty-hideout sound; an *occupied* one only partially muffles, which
            // the sound system applies from occupancy, not from terrain (§10.3).
            Terrain::Floor
            | Terrain::DoorPanelOpen
            | Terrain::Hideout
            | Terrain::PartialCover
            | Terrain::Console
            | Terrain::Exit => SoundBlocking::Passes,
            Terrain::DoorPanelClosed => SoundBlocking::Attenuates,
            Terrain::Wall | Terrain::DoorHinge => SoundBlocking::Blocks,
        }
    }
}

/// How a terrain cell affects sound trying to cross it (§9.1, §10.3). Three levels,
/// because a closed door is neither transparent nor a wall: it is the "mostly"
/// column of the terrain table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundBlocking {
    /// Sound crosses freely — floor, an open door.
    Passes,
    /// Sound crosses but loses extra intensity — a closed door panel.
    Attenuates,
    /// Sound does not cross at all — wall, door hinge.
    Blocks,
}

/// The facility grid: `width × height` terrain cells in row-major order.
///
/// Coordinates are integer cells with the origin at the top-left; `(0, 0)` is
/// the north-west corner. Movement, distance and the rest of the model are
/// 4-directional (§4.1), but this type is pure storage plus lookups — it holds
/// no turn logic.
#[derive(Clone, Debug)]
pub struct Facility {
    width: u32,
    height: u32,
    cells: Vec<Terrain>,
}

impl Facility {
    /// A rectangular level: a solid wall border enclosing floor (§4.1, §10.6).
    ///
    /// The smallest level the design admits — no rooms, no corridors, just the
    /// enclosing ring the facility is *always* wrapped in. It is what the
    /// generator will carve into, and enough to prove the render/deploy pipeline
    /// end to end.
    ///
    /// Panics if either dimension is below 3, since a border with any interior
    /// at all needs at least a `3 × 3` footprint.
    pub fn walled_box(width: u32, height: u32) -> Self {
        assert!(
            width >= 3 && height >= 3,
            "a walled box needs at least a 3x3 footprint, got {width}x{height}"
        );
        let mut cells = vec![Terrain::Floor; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                let on_border = x == 0 || y == 0 || x == width - 1 || y == height - 1;
                if on_border {
                    cells[(y * width + x) as usize] = Terrain::Wall;
                }
            }
        }
        Self {
            width,
            height,
            cells,
        }
    }

    /// The grid width in cells.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The grid height in cells.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The capacity of every cell (§4.3). Fills occupying a cell may sum to at
    /// most this; a move is admitted when what's already there plus the mover's
    /// fill stays within it.
    pub const CELL_CAPACITY: f32 = 1.0;

    /// The terrain at `(x, y)`, or `None` if the coordinate is off the grid.
    pub fn terrain_at(&self, x: u32, y: u32) -> Option<Terrain> {
        if x < self.width && y < self.height {
            Some(self.cells[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    /// Whether `cell` names a square on this grid.
    pub fn in_bounds(&self, cell: Cell) -> bool {
        cell.x < self.width && cell.y < self.height
    }

    /// The terrain at `cell`, or `None` off the grid — the [`Cell`]-typed
    /// companion to [`terrain_at`](Self::terrain_at).
    pub fn terrain(&self, cell: Cell) -> Option<Terrain> {
        self.terrain_at(cell.x, cell.y)
    }

    /// The in-bounds cardinal neighbours of `cell` (§4.1): up to four, fewer at an
    /// edge. 4-directional by construction — there is no diagonal in the walk, so
    /// nothing built on it (movement, pathfinding, flood fill) can travel one.
    pub fn neighbors(&self, cell: Cell) -> impl Iterator<Item = Cell> + '_ {
        Direction::ALL
            .into_iter()
            .filter_map(move |dir| cell.step(dir))
            .filter(|&c| self.in_bounds(c))
    }

    /// Whether a mover contributing `fill` may enter `cell` (§4.3): true when the
    /// cell is on the grid and the fill already there plus the mover's stays within
    /// [`CELL_CAPACITY`](Self::CELL_CAPACITY). Off-grid never admits anyone.
    ///
    /// A `false` here is not a dead end — it is the game's interaction verb (§4.3):
    /// the caller turns a refused move into a *bump* (open the door, use the
    /// console, take an unaware guard down). Today the only fill a cell carries is
    /// its terrain's; when guards, the player and bodies come to live in the grid
    /// (§12.3), their fills fold into this same sum without changing the query.
    pub fn can_enter(&self, cell: Cell, fill: f32) -> bool {
        match self.terrain(cell) {
            Some(terrain) => terrain.fill() + fill <= Self::CELL_CAPACITY,
            None => false,
        }
    }

    /// Set the terrain at `(x, y)`. Panics if the coordinate is off the grid.
    ///
    /// The seam the generator carves through: the partition (§10.1) starts from a
    /// [`walled_box`](Self::walled_box) of interior floor and stamps corridor walls
    /// and punch-throughs into it. Kept crate-internal — outside the core, a
    /// facility is read-only; only generation writes cells.
    pub(crate) fn set_terrain(&mut self, x: u32, y: u32, terrain: Terrain) {
        assert!(
            x < self.width && y < self.height,
            "set_terrain ({x},{y}) is outside the {}x{} grid",
            self.width,
            self.height
        );
        self.cells[(y * self.width + x) as usize] = terrain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walled_box_has_the_requested_dimensions() {
        let f = Facility::walled_box(40, 40);
        assert_eq!(f.width(), 40);
        assert_eq!(f.height(), 40);
    }

    /// The §4.1/§10.6 guarantee: the level is always fully enclosed. Every
    /// border cell is wall, on all four sides.
    #[test]
    fn the_border_ring_is_solid_wall() {
        let f = Facility::walled_box(12, 8);
        for x in 0..f.width() {
            assert_eq!(f.terrain_at(x, 0), Some(Terrain::Wall), "top row");
            assert_eq!(
                f.terrain_at(x, f.height() - 1),
                Some(Terrain::Wall),
                "bottom row"
            );
        }
        for y in 0..f.height() {
            assert_eq!(f.terrain_at(0, y), Some(Terrain::Wall), "left column");
            assert_eq!(
                f.terrain_at(f.width() - 1, y),
                Some(Terrain::Wall),
                "right column"
            );
        }
    }

    #[test]
    fn the_interior_is_floor() {
        let f = Facility::walled_box(6, 5);
        for y in 1..f.height() - 1 {
            for x in 1..f.width() - 1 {
                assert_eq!(
                    f.terrain_at(x, y),
                    Some(Terrain::Floor),
                    "interior ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn out_of_bounds_reads_none() {
        let f = Facility::walled_box(4, 4);
        assert_eq!(f.terrain_at(4, 0), None);
        assert_eq!(f.terrain_at(0, 4), None);
    }

    #[test]
    #[should_panic]
    fn too_small_is_rejected() {
        Facility::walled_box(2, 10);
    }

    /// The §10.3 terrain table for doors, asserted directly. A closed panel is the
    /// interesting row: solid and opaque like a wall, yet **transparent to
    /// pathfinding** and only *attenuating* sound.
    #[test]
    fn the_door_terrain_table_matches_10_3() {
        // Closed panel: fill 1.0, opaque, attenuates sound, transparent to pathing.
        let closed = Terrain::DoorPanelClosed;
        assert_eq!(closed.fill(), 1.0);
        assert!(closed.blocks_movement());
        assert!(closed.blocks_sight());
        assert!(
            !closed.blocks_pathing(),
            "closed panel must not block pathing"
        );
        assert_eq!(closed.sound(), SoundBlocking::Attenuates);
        assert_eq!(closed.glyph(), '+');

        // Open panel: fill 0.0, walk-through in every sense.
        let open = Terrain::DoorPanelOpen;
        assert_eq!(open.fill(), 0.0);
        assert!(!open.blocks_movement());
        assert!(!open.blocks_sight());
        assert!(!open.blocks_pathing());
        assert_eq!(open.sound(), SoundBlocking::Passes);
        assert_eq!(open.glyph(), ' ');

        // Hinge: a wall in every column that has a distinct glyph.
        let hinge = Terrain::DoorHinge;
        assert!(hinge.blocks_movement() && hinge.blocks_sight() && hinge.blocks_pathing());
        assert_eq!(hinge.sound(), SoundBlocking::Blocks);
        assert_eq!(hinge.glyph(), '×');
    }

    /// Floor and wall keep their original properties as the table grows around them.
    #[test]
    fn floor_and_wall_baselines_hold() {
        assert_eq!(Terrain::Floor.fill(), 0.0);
        assert!(!Terrain::Floor.blocks_movement());
        assert!(!Terrain::Floor.blocks_sight());
        assert!(!Terrain::Floor.blocks_pathing());
        assert_eq!(Terrain::Floor.sound(), SoundBlocking::Passes);

        assert_eq!(Terrain::Wall.fill(), 1.0);
        assert!(Terrain::Wall.blocks_movement());
        assert!(Terrain::Wall.blocks_sight());
        assert!(Terrain::Wall.blocks_pathing());
        assert_eq!(Terrain::Wall.sound(), SoundBlocking::Blocks);
    }

    /// The rest of the static §10.3 table: hideout, console, exit.
    #[test]
    fn hideout_console_and_exit_match_10_3() {
        // Hideout, empty: walk-through (fill 0.0), sight/sound transparent, yet it
        // *blocks pathing* — guards route around, the player ducks in.
        let h = Terrain::Hideout;
        assert_eq!(h.fill(), 0.0);
        assert!(!h.blocks_movement());
        assert!(!h.blocks_sight());
        assert!(h.blocks_pathing(), "hideout must block pathing");
        assert_eq!(h.sound(), SoundBlocking::Passes);
        assert_eq!(h.glyph(), '}');

        // Console and exit: solid (fill 1.0, blocks movement) but transparent to
        // sight, pathing and sound — you see past them and route across them.
        for (t, glyph) in [(Terrain::Console, '$'), (Terrain::Exit, 'E')] {
            assert_eq!(t.fill(), 1.0);
            assert!(t.blocks_movement());
            assert!(!t.blocks_sight());
            assert!(!t.blocks_pathing());
            assert_eq!(t.sound(), SoundBlocking::Passes);
            assert_eq!(t.glyph(), glyph);
        }
    }

    /// The §10.3 partial-cover row: a table is solid to movement and pathing like
    /// a wall, but **see-through** — the one terrain where "can I walk there" and
    /// "can I see there" split this way round. It passes sound, and it is the only
    /// terrain that provides §10.1a cover.
    #[test]
    fn partial_cover_matches_10_3() {
        let t = Terrain::PartialCover;
        assert_eq!(t.fill(), 1.0);
        assert!(t.blocks_movement());
        assert!(!t.blocks_sight(), "a guard sees straight over a table");
        assert!(t.blocks_pathing(), "patrols route around furniture");
        assert_eq!(t.sound(), SoundBlocking::Passes);
        assert_eq!(t.glyph(), 'π');
        assert_eq!(t.category(), Category::System);

        // Cover is this row's exclusive property: nothing else provides it.
        assert!(t.provides_cover());
        for other in [
            Terrain::Floor,
            Terrain::Wall,
            Terrain::DoorHinge,
            Terrain::DoorPanelClosed,
            Terrain::DoorPanelOpen,
            Terrain::Hideout,
            Terrain::Console,
            Terrain::Exit,
        ] {
            assert!(!other.provides_cover(), "{other:?} must not provide cover");
        }
    }

    /// §4.3 occupancy through the grid: a solid mover is admitted onto floor and an
    /// open door, refused by a wall or a closed door, and never off-grid.
    #[test]
    fn can_enter_sums_fills_against_capacity() {
        let mut f = Facility::walled_box(6, 6);
        f.set_terrain(2, 2, Terrain::DoorPanelClosed);
        f.set_terrain(3, 2, Terrain::DoorPanelOpen);

        // A full-fill mover (guard/player, fill 1.0) onto empty floor: 0.0 + 1.0 ≤ 1.0.
        assert!(f.can_enter(Cell::new(1, 1), 1.0));
        // Onto an open door: still admitted.
        assert!(f.can_enter(Cell::new(3, 2), 1.0));
        // Into a wall or a closed door: 1.0 + 1.0 > 1.0, refused (→ a bump).
        assert!(!f.can_enter(Cell::new(0, 0), 1.0));
        assert!(!f.can_enter(Cell::new(2, 2), 1.0));
        // A weightless mover (a decoy, fill 0.0) rides onto floor freely.
        assert!(f.can_enter(Cell::new(1, 1), 0.0));
        // The capacity check is inclusive (§4.3): two half-fills exactly fill a
        // floor cell, so the second is still admitted at the 1.0 boundary.
        assert!(f.can_enter(Cell::new(1, 1), Facility::CELL_CAPACITY));
        // Off the grid admits no one.
        assert!(!f.can_enter(Cell::new(6, 0), 1.0));
    }

    /// Neighbours are the ≤4 in-bounds cardinal cells, all one Manhattan unit away
    /// — the "no diagonal path anywhere" guarantee, seen from the grid.
    #[test]
    fn neighbors_are_cardinal_and_in_bounds() {
        let f = Facility::walled_box(5, 5);

        let interior: Vec<Cell> = f.neighbors(Cell::new(2, 2)).collect();
        assert_eq!(interior.len(), 4);
        for n in &interior {
            assert_eq!(Cell::new(2, 2).manhattan_distance(*n), 1);
        }

        // A corner sees only its two on-grid neighbours; the off-grid steps drop.
        let corner: Vec<Cell> = f.neighbors(Cell::new(0, 0)).collect();
        assert_eq!(corner.len(), 2);
        assert!(corner.contains(&Cell::new(1, 0)));
        assert!(corner.contains(&Cell::new(0, 1)));
    }

    #[test]
    fn in_bounds_and_cell_terrain_agree_with_coordinates() {
        let f = Facility::walled_box(4, 4);
        assert!(f.in_bounds(Cell::new(3, 3)));
        assert!(!f.in_bounds(Cell::new(4, 3)));
        assert_eq!(f.terrain(Cell::new(0, 0)), Some(Terrain::Wall));
        assert_eq!(f.terrain(Cell::new(1, 1)), Some(Terrain::Floor));
        assert_eq!(f.terrain(Cell::new(4, 0)), None);
    }
}
