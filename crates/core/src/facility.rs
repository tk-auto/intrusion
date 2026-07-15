//! The facility's static geometry: a grid of terrain cells (§4.1, §10).
//!
//! This is the substrate every other system will read — vision, sound, guards
//! and generation all query terrain. Right now it holds the smallest honest
//! version of that world: a rectangle of floor wrapped in the indestructible
//! 1-cell border the design guarantees (§4.1, §10.6). The real generator
//! (corridor-first partition, §10.1) grows *inside* this type in its own ticket;
//! nothing here should have to change when it lands.
//!
//! Two terrain kinds exist so far — floor and wall — carrying only their glyph
//! (§10.3). The full six-column terrain table (move/sight/path/sound blocking,
//! fill) lands with the occupancy model; this slice deliberately stops at what
//! "draw a big rectangle of walls" needs.

/// A kind of cell. A thin slice of the §10.3 terrain table — extended, not
/// replaced, when occupancy and the other terrain kinds land.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Terrain {
    /// Walkable empty ground. Renders blank (§10.3).
    Floor,
    /// Solid, sight-blocking structure. Renders `#` (§10.3).
    Wall,
}

impl Terrain {
    /// The character this terrain renders as (§10.3). Floor is blank; the
    /// renderer paints it as background.
    pub fn glyph(self) -> char {
        match self {
            Terrain::Floor => ' ',
            Terrain::Wall => '#',
        }
    }
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
}
