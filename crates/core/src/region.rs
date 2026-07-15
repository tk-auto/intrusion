//! The facility's spatial model: named regions, door edges, cell → region lookup.
//!
//! This is the single highest-leverage structural decision in the design (§10.5).
//! The old game had exactly one spatial abstraction — an axis-aligned rectangle —
//! asked to be the level bounds, the partition regions, room identity, guard
//! territory *and* the viewport, and it was up to none of it. Two failures cost
//! the most: **corridors were never regions at all** (painted into the plan and
//! forgotten, so the connective tissue where stealth happens was spatially
//! unaddressable), and **the generation graph was thrown away** (once the level
//! existed it had no concept of rooms). Everything downstream then had to fake
//! it — which is *why* guard cooperation, assigned patrols, keys and circuits all
//! stayed unbuilt. **[SETTLED]: keep the graph.**
//!
//! So a level is: **regions** of arbitrary shape (a pillared room or an L-nook is
//! not a rectangle), each tagged room or corridor and **corridors are first-class**;
//! **door edges**, each joining exactly two regions; and an **O(1) cell → region**
//! lookup. This module is the shape and the queries — the generator (the §10.1
//! partition and the doorway pass) writes into it directly via [`RegionGraph::add_region`]
//! and [`RegionGraph::add_door`]. It is fully usable standalone from a hand-built
//! fixture, which is how it is tested here.

use crate::cell::Cell;

/// A handle to a region in a [`RegionGraph`]. Opaque and stable for the life of
/// the graph; compare handles, don't compute with them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RegionId(u32);

/// A handle to a door edge in a [`RegionGraph`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DoorId(u32);

/// What a region *is*. Rooms and corridors are both first-class regions — the old
/// model could not address corridors at all, and that was the point of failure
/// (§10.5). More kinds may join later; this is the vocabulary generation needs now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegionKind {
    /// A leftover of the partition — always ≥ 6×6, the space rooms live in (§10.1).
    Room,
    /// A carved corridor, 2–4 wide (§10.1). First-class so guards and vision can
    /// name it ("which corridor is this?", "does it reach that room?").
    Corridor,
}

/// One region: its kind and the exact set of cells it occupies (any shape), plus
/// the doors on its boundary. The cell set is the payoff over a bounding rectangle
/// — a room with a pillar carved out records its true footprint here (§10.5).
#[derive(Clone, Debug)]
pub struct Region {
    kind: RegionKind,
    cells: Vec<Cell>,
    doors: Vec<DoorId>,
}

impl Region {
    /// Whether this is a room or a corridor.
    pub fn kind(&self) -> RegionKind {
        self.kind
    }

    /// The cells this region occupies, in the order they were added.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// The doors on this region's boundary.
    pub fn doors(&self) -> &[DoorId] {
        &self.doors
    }
}

/// One door edge: the two regions it joins and the cells of the doorway itself.
///
/// A door is an *edge*, not a node — its cells are the threshold between regions
/// and belong to no region, so [`RegionGraph::region_at`] on a door cell is `None`.
/// The full hinged/panelled door object (§10.4) is a later ticket; here a door is
/// just the topological join plus where it sits.
#[derive(Clone, Debug)]
pub struct Door {
    between: [RegionId; 2],
    cells: Vec<Cell>,
}

impl Door {
    /// The two regions this door joins. Always two distinct regions.
    pub fn regions(&self) -> [RegionId; 2] {
        self.between
    }

    /// The cells forming the doorway.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Given one of the door's two regions, the region on the other side. Panics
    /// if `region` is not an endpoint of this door.
    pub fn other(&self, region: RegionId) -> RegionId {
        let [a, b] = self.between;
        if region == a {
            b
        } else if region == b {
            a
        } else {
            panic!("region {region:?} is not an endpoint of this door");
        }
    }
}

/// The level's spatial model: regions of arbitrary shape, door edges between them,
/// and an O(1) cell → region lookup over a fixed `width × height` grid.
///
/// Built incrementally as the generator carves: [`add_region`](Self::add_region)
/// claims a set of cells for a new region; [`add_door`](Self::add_door) records an
/// edge between two of them. The graph enforces its invariants as it is built — a
/// cell belongs to at most one region, a door joins exactly two distinct regions —
/// panicking on violation, because those are generator bugs, not runtime states.
#[derive(Clone, Debug)]
pub struct RegionGraph {
    width: u32,
    height: u32,
    regions: Vec<Region>,
    doors: Vec<Door>,
    /// Row-major `width × height`, `index[y * width + x]` = the region owning that
    /// cell, or `None` for wall / doorway / unclaimed cells. This is the O(1)
    /// lookup; it is what makes "which region is this cell?" cheap for vision,
    /// guards and pathing to ask every turn.
    index: Vec<Option<RegionId>>,
}

impl RegionGraph {
    /// An empty graph over a `width × height` grid. Every cell starts unclaimed.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            regions: Vec::new(),
            doors: Vec::new(),
            index: vec![None; (width * height) as usize],
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

    /// Register a new region of `kind` owning `cells`, returning its handle.
    ///
    /// Every cell is claimed in the lookup index. Panics if `cells` is empty, if
    /// any cell is off the grid, or if any cell already belongs to another region
    /// — a cell has at most one region, and a double-claim is a generation bug.
    pub fn add_region(
        &mut self,
        kind: RegionKind,
        cells: impl IntoIterator<Item = Cell>,
    ) -> RegionId {
        let cells: Vec<Cell> = cells.into_iter().collect();
        assert!(!cells.is_empty(), "a region must own at least one cell");

        let id = RegionId(self.regions.len() as u32);
        for &cell in &cells {
            let slot = self.slot(cell);
            assert!(
                self.index[slot].is_none(),
                "cell {cell:?} is already claimed by {:?}",
                self.index[slot]
            );
            self.index[slot] = Some(id);
        }
        self.regions.push(Region {
            kind,
            cells,
            doors: Vec::new(),
        });
        id
    }

    /// Register a door joining regions `a` and `b` through `cells`, returning its
    /// handle. The door is recorded on both regions' boundaries.
    ///
    /// Panics if `a == b` (a door joins two *distinct* regions), if either handle
    /// is unknown, or if `cells` is empty. Door cells are *not* claimed as region
    /// cells — a doorway belongs to no region.
    pub fn add_door(
        &mut self,
        a: RegionId,
        b: RegionId,
        cells: impl IntoIterator<Item = Cell>,
    ) -> DoorId {
        assert!(
            a != b,
            "a door must join two distinct regions, got {a:?} twice"
        );
        assert!(
            self.is_region(a) && self.is_region(b),
            "unknown region in door {a:?}..{b:?}"
        );
        let cells: Vec<Cell> = cells.into_iter().collect();
        assert!(!cells.is_empty(), "a door must occupy at least one cell");

        let id = DoorId(self.doors.len() as u32);
        self.doors.push(Door {
            between: [a, b],
            cells,
        });
        self.regions[a.0 as usize].doors.push(id);
        self.regions[b.0 as usize].doors.push(id);
        id
    }

    /// The region owning `cell`, or `None` for a wall, doorway, off-grid, or
    /// otherwise unclaimed cell. This is the O(1) lookup (a single indexed read).
    pub fn region_at(&self, cell: Cell) -> Option<RegionId> {
        if cell.x < self.width && cell.y < self.height {
            self.index[(cell.y * self.width + cell.x) as usize]
        } else {
            None
        }
    }

    /// The region behind a handle.
    pub fn region(&self, id: RegionId) -> &Region {
        &self.regions[id.0 as usize]
    }

    /// The door behind a handle.
    pub fn door(&self, id: DoorId) -> &Door {
        &self.doors[id.0 as usize]
    }

    /// The kind of a region — convenience for the common "room or corridor?" query.
    pub fn kind(&self, id: RegionId) -> RegionKind {
        self.region(id).kind
    }

    /// The number of regions in the graph.
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// The number of doors in the graph.
    pub fn door_count(&self) -> usize {
        self.doors.len()
    }

    /// Every region, paired with its handle.
    pub fn regions(&self) -> impl Iterator<Item = (RegionId, &Region)> {
        self.regions
            .iter()
            .enumerate()
            .map(|(i, r)| (RegionId(i as u32), r))
    }

    /// Every door, paired with its handle.
    pub fn doors(&self) -> impl Iterator<Item = (DoorId, &Door)> {
        self.doors
            .iter()
            .enumerate()
            .map(|(i, d)| (DoorId(i as u32), d))
    }

    /// The regions reachable from `id` across a single door, each paired with the
    /// door crossed. This is the adjacency the graph exists for — "does this room
    /// reach that corridor?" and, later, "cover the east wing" (§7.5). A region
    /// with two doors to the same neighbour yields it twice, once per door.
    pub fn neighbours(&self, id: RegionId) -> impl Iterator<Item = (DoorId, RegionId)> + '_ {
        self.region(id)
            .doors
            .iter()
            .map(move |&door_id| (door_id, self.door(door_id).other(id)))
    }

    /// The flat index of an in-bounds cell, panicking if it is off the grid.
    fn slot(&self, cell: Cell) -> usize {
        assert!(
            cell.x < self.width && cell.y < self.height,
            "cell {cell:?} is outside the {}x{} grid",
            self.width,
            self.height
        );
        (cell.y * self.width + cell.x) as usize
    }

    /// Whether a handle names a region this graph holds.
    fn is_region(&self, id: RegionId) -> bool {
        (id.0 as usize) < self.regions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rectangle of cells `[x0, x1) × [y0, y1)`, for building fixtures.
    fn rect(x0: u32, x1: u32, y0: u32, y1: u32) -> Vec<Cell> {
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| Cell::new(x, y)))
            .collect()
    }

    /// A small hand-built level, exercised by the tests below:
    ///
    /// ```text
    ///   col 0         11
    ///   #############   row 0
    ///   #AAA#CC#BBB #
    ///   #AAA#CC#BBB #
    ///   #AAADoCCoDBB#   doors 'o' at the wall columns (x=4, x=7)
    ///   #AAA#CC#BBB #
    ///   #############   row 6
    /// ```
    ///
    /// Room A (left) and Room B (right) each connect to the central Corridor C by
    /// one door. A and B are *not* adjacent — the only path between them is through
    /// the corridor, which is the whole point of the model.
    fn fixture() -> (RegionGraph, RegionId, RegionId, RegionId, DoorId, DoorId) {
        let mut g = RegionGraph::new(12, 7);
        let room_a = g.add_region(RegionKind::Room, rect(1, 4, 1, 5));
        let corridor = g.add_region(RegionKind::Corridor, rect(5, 7, 1, 5));
        let room_b = g.add_region(RegionKind::Room, rect(8, 11, 1, 5));
        // Doorways sit in the wall columns separating the regions.
        let door_a = g.add_door(room_a, corridor, [Cell::new(4, 3)]);
        let door_b = g.add_door(corridor, room_b, [Cell::new(7, 3)]);
        (g, room_a, corridor, room_b, door_a, door_b)
    }

    #[test]
    fn cell_lookup_maps_cells_to_their_region() {
        let (g, room_a, corridor, room_b, _, _) = fixture();
        assert_eq!(g.region_at(Cell::new(1, 1)), Some(room_a));
        assert_eq!(g.region_at(Cell::new(3, 4)), Some(room_a));
        assert_eq!(g.region_at(Cell::new(6, 2)), Some(corridor));
        assert_eq!(g.region_at(Cell::new(10, 4)), Some(room_b));
    }

    #[test]
    fn wall_doorway_and_offgrid_cells_belong_to_no_region() {
        let (g, _, _, _, _, _) = fixture();
        assert_eq!(g.region_at(Cell::new(0, 0)), None, "border wall");
        assert_eq!(
            g.region_at(Cell::new(4, 3)),
            None,
            "door cell is an edge, not a node"
        );
        assert_eq!(
            g.region_at(Cell::new(7, 3)),
            None,
            "door cell is an edge, not a node"
        );
        assert_eq!(g.region_at(Cell::new(100, 100)), None, "off the grid");
    }

    #[test]
    fn region_kinds_are_recorded() {
        let (g, room_a, corridor, room_b, _, _) = fixture();
        assert_eq!(g.kind(room_a), RegionKind::Room);
        assert_eq!(g.kind(corridor), RegionKind::Corridor);
        assert_eq!(g.kind(room_b), RegionKind::Room);
        assert_eq!(g.region_count(), 3);
    }

    #[test]
    fn a_door_joins_exactly_two_distinct_regions() {
        let (g, room_a, corridor, _, door_a, _) = fixture();
        let joined = g.door(door_a).regions();
        assert_eq!(joined.len(), 2);
        assert_ne!(joined[0], joined[1]);
        assert!(joined.contains(&room_a) && joined.contains(&corridor));
        assert_eq!(g.door(door_a).other(room_a), corridor);
        assert_eq!(g.door(door_a).other(corridor), room_a);
    }

    #[test]
    fn neighbours_are_reached_across_doors() {
        let (g, room_a, corridor, room_b, door_a, door_b) = fixture();

        // The corridor is the hub: it reaches both rooms, each across its own door.
        let mut corridor_neighbours: Vec<(DoorId, RegionId)> = g.neighbours(corridor).collect();
        corridor_neighbours.sort_by_key(|(d, _)| d.0);
        assert_eq!(
            corridor_neighbours,
            vec![(door_a, room_a), (door_b, room_b)]
        );

        // Each room reaches only the corridor — never each other directly.
        assert_eq!(
            g.neighbours(room_a).collect::<Vec<_>>(),
            vec![(door_a, corridor)]
        );
        assert_eq!(
            g.neighbours(room_b).collect::<Vec<_>>(),
            vec![(door_b, corridor)]
        );
    }

    #[test]
    fn every_cell_of_a_region_reports_that_region() {
        let (g, room_a, _, _, _, _) = fixture();
        for &cell in g.region(room_a).cells() {
            assert_eq!(g.region_at(cell), Some(room_a));
        }
    }

    #[test]
    #[should_panic(expected = "already claimed")]
    fn overlapping_regions_are_rejected() {
        let mut g = RegionGraph::new(10, 10);
        g.add_region(RegionKind::Room, rect(1, 4, 1, 4));
        // (2,2) is already Room's; claiming it again is a generation bug.
        g.add_region(RegionKind::Corridor, rect(2, 5, 2, 5));
    }

    #[test]
    #[should_panic(expected = "two distinct regions")]
    fn a_door_cannot_join_a_region_to_itself() {
        let mut g = RegionGraph::new(10, 10);
        let r = g.add_region(RegionKind::Room, rect(1, 4, 1, 4));
        g.add_door(r, r, [Cell::new(4, 2)]);
    }

    #[test]
    #[should_panic(expected = "outside")]
    fn cells_off_the_grid_are_rejected() {
        let mut g = RegionGraph::new(5, 5);
        g.add_region(RegionKind::Room, [Cell::new(9, 9)]);
    }
}
