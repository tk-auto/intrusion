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
//! Doors (§10.4) are the first terrain whose §10.3 properties actually diverge —
//! a closed panel is opaque and solid yet *transparent to pathfinding* — so the
//! door ticket is where the rest of the §10.3 table earns its keep. The columns
//! filled in here (fill, sight, pathing, sound) are the ones doors demonstrate;
//! the remaining terrain kinds and any partial-cover axis land in their own
//! tickets, extending these same exhaustive matches.

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
        }
    }

    /// This terrain's contribution to a cell's occupancy fill (§4.3). Fill 1.0 is
    /// solid and exclusive; 0.0 is walk-through. A move into a cell succeeds when
    /// the fills already there plus the mover's stay ≤ 1.0.
    pub fn fill(self) -> f32 {
        match self {
            Terrain::Floor | Terrain::DoorPanelOpen => 0.0,
            Terrain::Wall | Terrain::DoorHinge | Terrain::DoorPanelClosed => 1.0,
        }
    }

    /// Whether terrain alone blocks a mover from entering the cell (§4.3, §10.3).
    /// A blocked move is an *interaction*, not a failure — bumping a closed door
    /// opens it rather than stopping you dead (§4.3).
    pub fn blocks_movement(self) -> bool {
        self.fill() >= 1.0
    }

    /// Whether this terrain blocks sight (§10.3). Opacity is all-or-nothing —
    /// there is no partial cover in v1 **[START]**.
    pub fn blocks_sight(self) -> bool {
        match self {
            Terrain::Floor | Terrain::DoorPanelOpen => false,
            Terrain::Wall | Terrain::DoorHinge | Terrain::DoorPanelClosed => true,
        }
    }

    /// Whether this terrain blocks **pathfinding** (§10.3). The one deliberate
    /// surprise: a **closed door panel does not**. Guards route through closed
    /// doors and open them by walking in, so guard traffic monotonically opens the
    /// facility up over a level (§10.4).
    pub fn blocks_pathing(self) -> bool {
        match self {
            Terrain::Wall | Terrain::DoorHinge => true,
            Terrain::Floor | Terrain::DoorPanelClosed | Terrain::DoorPanelOpen => false,
        }
    }

    /// How this terrain affects sound crossing it (§9.1, §10.3). Sound flows around
    /// walls, not through them; a closed door only **attenuates**, which is what
    /// gives "close the door behind you" a point.
    pub fn sound(self) -> SoundBlocking {
        match self {
            Terrain::Floor | Terrain::DoorPanelOpen => SoundBlocking::Passes,
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

    /// The terrain at `(x, y)`, or `None` if the coordinate is off the grid.
    pub fn terrain_at(&self, x: u32, y: u32) -> Option<Terrain> {
        if x < self.width && y < self.height {
            Some(self.cells[(y * self.width + x) as usize])
        } else {
            None
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
}
