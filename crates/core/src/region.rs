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

/// Which part of a door a cell is — the two are operated differently (§10.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DoorCell {
    /// A frame end: permanently solid and opaque, the handle you bump to *close*.
    Hinge,
    /// A movable panel: bump it to *open*; it fills the doorway when closed.
    Panel,
}

/// How a door closes (§10.4) — the axis #147 splits the old `auto_close` boolean into.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DoorKind {
    /// A hinged door: a solid frame end at each side and movable panels between them.
    /// Opened by bumping a panel, closed by bumping a hinge (§10.4) or by a Calm guard
    /// passing through it (#146). It has no will of its own — it stays however it was
    /// last left.
    Manual,
    /// A frameless automatic door: the whole span is panels, no hinges, so there is no
    /// handle to close it by hand. It opens like any door — a bump, or a guard walking
    /// in — and closes *itself* `delay` turns after the doorway is last vacated
    /// (§10.4/#147), which is what stops an opened door decaying the level into an open
    /// plan. It never crushes: an actor on a panel holds it open.
    Automatic { delay: u32 },
}

/// One door: the two regions it joins, its hinge/panel structure, and its runtime
/// open/closed state (§10.4).
///
/// A door is a graph *edge*, not a node — its cells are the threshold between two
/// regions and belong to no region, so [`RegionGraph::region_at`] on a door cell is
/// `None`. Topologically it is a span of **3–6 cells**: a **hinge at each end**
/// (permanently solid+opaque — the frame) and **1–4 panels** between them that open
/// and close as one unit. Bump a panel to open, bump a hinge to close; the actual
/// operation, which also restamps the facility terrain, lives on
/// [`Layout`](crate::Layout) since it touches both the graph and the grid.
#[derive(Clone, Debug)]
pub struct Door {
    between: [RegionId; 2],
    /// The two frame ends, in scan order. Always solid and opaque.
    hinges: Vec<Cell>,
    /// The 1–4 panels between the hinges, opened and closed as a unit.
    panels: Vec<Cell>,
    /// Whether the panels are currently open.
    open: bool,
    /// How this door closes (§10.4/#147): a [`Manual`](DoorKind::Manual) hinged door,
    /// or an [`Automatic`](DoorKind::Automatic) frameless one that shuts itself.
    kind: DoorKind,
    /// Turns until an open automatic door shuts itself (§10.4/#147). Meaningful only
    /// while an [`Automatic`](DoorKind::Automatic) door is open: armed to its `delay`
    /// each time it opens or is re-occupied, counted down each vacant turn, and `0`
    /// otherwise. A manual door ignores it entirely.
    auto_timer: u32,
}

impl Door {
    /// The two regions this door joins. Always two distinct regions.
    pub fn regions(&self) -> [RegionId; 2] {
        self.between
    }

    /// The frame-end cells — permanently solid+opaque, the handles that close the
    /// door (§10.4).
    pub fn hinges(&self) -> &[Cell] {
        &self.hinges
    }

    /// The panel cells that open and close as one unit (§10.4).
    pub fn panels(&self) -> &[Cell] {
        &self.panels
    }

    /// Every cell of the doorway — hinges then panels — its full footprint in the
    /// wall line.
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        self.hinges.iter().chain(self.panels.iter()).copied()
    }

    /// Whether the door's panels are currently open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// How this door closes — manual (hinged) or automatic (frameless) (§10.4/#147).
    pub fn kind(&self) -> DoorKind {
        self.kind
    }

    /// Whether this is a frameless automatic door (§10.4/#147) — the common query,
    /// so callers need not match the whole [`DoorKind`].
    pub fn is_automatic(&self) -> bool {
        matches!(self.kind, DoorKind::Automatic { .. })
    }

    /// Turns until an open automatic door shuts itself (§10.4/#147); `0` for a closed
    /// or manual door. See [`auto_timer`](Door::auto_timer) on the field.
    pub(crate) fn auto_timer(&self) -> u32 {
        self.auto_timer
    }

    /// Set the auto-close countdown (§10.4/#147). Crate-internal: the turn loop arms
    /// it on open, resets it while the doorway is occupied, and counts it down.
    pub(crate) fn set_auto_timer(&mut self, turns: u32) {
        self.auto_timer = turns;
    }

    /// Which part of the door `cell` is, or `None` if the cell is not on this door.
    pub fn role(&self, cell: Cell) -> Option<DoorCell> {
        if self.hinges.contains(&cell) {
            Some(DoorCell::Hinge)
        } else if self.panels.contains(&cell) {
            Some(DoorCell::Panel)
        } else {
            None
        }
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

    /// Set the open state. Crate-internal: the panels' terrain must be restamped in
    /// lockstep, which only [`Layout`](crate::Layout) can do, so it is the sole
    /// caller.
    pub(crate) fn set_open(&mut self, open: bool) {
        self.open = open;
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

    /// Register a door joining regions `a` and `b`, returning its handle. The door
    /// is recorded on both regions' boundaries, starts **closed**, and closes as its
    /// [`DoorKind`] dictates (§10.4/#147).
    ///
    /// `hinges` are the frame ends and `panels` the movable cells between them: a
    /// [`Manual`](DoorKind::Manual) door has two hinges around 1–4 panels, an
    /// [`Automatic`](DoorKind::Automatic) door has *no* hinges and 3–6 panels spanning
    /// the whole doorway. Panics if `a == b` (a door joins two *distinct* regions), if
    /// either handle is unknown, or if there are no panels. Door cells are *not*
    /// claimed as region cells — a doorway belongs to no region.
    pub fn add_door(
        &mut self,
        a: RegionId,
        b: RegionId,
        hinges: impl IntoIterator<Item = Cell>,
        panels: impl IntoIterator<Item = Cell>,
        kind: DoorKind,
    ) -> DoorId {
        assert!(
            a != b,
            "a door must join two distinct regions, got {a:?} twice"
        );
        assert!(
            self.is_region(a) && self.is_region(b),
            "unknown region in door {a:?}..{b:?}"
        );
        let hinges: Vec<Cell> = hinges.into_iter().collect();
        let panels: Vec<Cell> = panels.into_iter().collect();
        assert!(!panels.is_empty(), "a door must have at least one panel");

        let id = DoorId(self.doors.len() as u32);
        self.doors.push(Door {
            between: [a, b],
            hinges,
            panels,
            open: false,
            kind,
            auto_timer: 0,
        });
        self.regions[a.0 as usize].doors.push(id);
        self.regions[b.0 as usize].doors.push(id);
        id
    }

    /// Release `cell` from the region that owns it, leaving it unclaimed.
    ///
    /// This is the write behind stamping structure onto claimed floor after the
    /// partition — a sightline blocker (§10.1a) turns a region's floor cell into
    /// wall, and the graph must move in lockstep with the grid (a wall belongs to
    /// no region). Crate-internal: only generation reshapes regions, and releasing
    /// a cell nothing owns is a generator bug, so that panics.
    pub(crate) fn remove_cell(&mut self, cell: Cell) {
        let slot = self.slot(cell);
        let id = self.index[slot]
            .unwrap_or_else(|| panic!("released cell {cell:?} belongs to no region"));
        self.index[slot] = None;
        let cells = &mut self.regions[id.0 as usize].cells;
        let at = cells
            .iter()
            .position(|&c| c == cell)
            .expect("the index and the region's cell list are in lockstep");
        cells.remove(at);
        assert!(
            !cells.is_empty(),
            "removing {cell:?} emptied {id:?} — a region must own at least one cell"
        );
    }

    /// Claim `cell` for the existing region `id`, the inverse of
    /// [`remove_cell`](Self::remove_cell).
    ///
    /// The write behind recessing a cupboard into a wall (§10.1.6): a hideout is a
    /// former *wall* cell, and a wall belongs to no region — but the recessed hideout
    /// is walkable (the player ducks into it), so it must join the region of the
    /// space it opens onto, or the "every walkable cell has exactly one region"
    /// invariant (§10.5) breaks. Crate-internal, like its inverse: only generation
    /// reshapes regions. Panics if `cell` is off the grid or already claimed by any
    /// region — a double-claim is a generation bug.
    pub(crate) fn add_cell(&mut self, id: RegionId, cell: Cell) {
        let slot = self.slot(cell);
        assert!(
            self.index[slot].is_none(),
            "cell {cell:?} is already claimed by {:?}",
            self.index[slot]
        );
        assert!(self.is_region(id), "unknown region {id:?}");
        self.index[slot] = Some(id);
        self.regions[id.0 as usize].cells.push(cell);
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

    /// The door whose hinges or panels include `cell`, or `None` if `cell` is not a
    /// door cell. Linear over the handful of doors a level has — a level's whole
    /// point is that there are few of them (§10.2).
    pub fn door_at(&self, cell: Cell) -> Option<DoorId> {
        self.doors
            .iter()
            .position(|d| d.role(cell).is_some())
            .map(|i| DoorId(i as u32))
    }

    /// Mutable access to a door — crate-internal, because opening or closing one
    /// must restamp the panels' terrain in the same step, which only
    /// [`Layout`](crate::Layout) can coordinate.
    pub(crate) fn door_mut(&mut self, id: DoorId) -> &mut Door {
        &mut self.doors[id.0 as usize]
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
        // Doorways sit in the wall columns separating the regions: a 3-cell span of
        // hinge / panel / hinge running down the wall.
        let door_a = g.add_door(
            room_a,
            corridor,
            [Cell::new(4, 1), Cell::new(4, 3)],
            [Cell::new(4, 2)],
            DoorKind::Manual,
        );
        let door_b = g.add_door(
            corridor,
            room_b,
            [Cell::new(7, 1), Cell::new(7, 3)],
            [Cell::new(7, 2)],
            DoorKind::Manual,
        );
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
    fn a_door_is_hinges_around_panels_and_classifies_its_cells() {
        let (g, _, _, _, door_a, _) = fixture();
        let door = g.door(door_a);
        assert_eq!(door.hinges(), [Cell::new(4, 1), Cell::new(4, 3)]);
        assert_eq!(door.panels(), [Cell::new(4, 2)]);
        assert_eq!(door.role(Cell::new(4, 1)), Some(DoorCell::Hinge));
        assert_eq!(door.role(Cell::new(4, 2)), Some(DoorCell::Panel));
        assert_eq!(door.role(Cell::new(9, 9)), None);
        // A fresh door is closed, and the fixture's is a manual hinged door.
        assert!(!door.is_open());
        assert_eq!(door.kind(), DoorKind::Manual);
        assert!(!door.is_automatic());
        // door_at finds the door from any of its cells.
        assert_eq!(g.door_at(Cell::new(4, 2)), Some(door_a));
        assert_eq!(g.door_at(Cell::new(4, 1)), Some(door_a));
        assert_eq!(g.door_at(Cell::new(1, 1)), None);
    }

    #[test]
    #[should_panic(expected = "two distinct regions")]
    fn a_door_cannot_join_a_region_to_itself() {
        let mut g = RegionGraph::new(10, 10);
        let r = g.add_region(RegionKind::Room, rect(1, 4, 1, 4));
        g.add_door(
            r,
            r,
            [Cell::new(4, 1), Cell::new(4, 3)],
            [Cell::new(4, 2)],
            DoorKind::Manual,
        );
    }

    #[test]
    #[should_panic(expected = "outside")]
    fn cells_off_the_grid_are_rejected() {
        let mut g = RegionGraph::new(5, 5);
        g.add_region(RegionKind::Room, [Cell::new(9, 9)]);
    }

    /// Releasing a cell (a §10.1a sightline blocker stamped over claimed floor)
    /// unclaims it in the index and drops it from the region's cell list — the
    /// lockstep the generator relies on.
    #[test]
    fn remove_cell_releases_the_cell_and_shrinks_the_region() {
        let mut g = RegionGraph::new(8, 8);
        let id = g.add_region(RegionKind::Corridor, [Cell::new(1, 1), Cell::new(2, 1)]);
        g.remove_cell(Cell::new(1, 1));
        assert_eq!(g.region_at(Cell::new(1, 1)), None);
        assert_eq!(g.region(id).cells(), &[Cell::new(2, 1)]);
        assert_eq!(g.region_at(Cell::new(2, 1)), Some(id));
    }

    #[test]
    #[should_panic(expected = "belongs to no region")]
    fn removing_an_unclaimed_cell_is_a_bug() {
        let mut g = RegionGraph::new(8, 8);
        g.add_region(RegionKind::Room, [Cell::new(1, 1)]);
        g.remove_cell(Cell::new(5, 5));
    }

    /// Claiming a cell for a region (a recessed cupboard joining the space it opens
    /// onto, §10.1.6) is the inverse of `remove_cell`: the index points at the
    /// region and the cell joins its list. A claim then a release round-trips.
    #[test]
    fn add_cell_claims_an_unclaimed_cell_for_a_region() {
        let mut g = RegionGraph::new(8, 8);
        let id = g.add_region(RegionKind::Room, [Cell::new(1, 1)]);
        g.add_cell(id, Cell::new(2, 1));
        assert_eq!(g.region_at(Cell::new(2, 1)), Some(id));
        assert!(g.region(id).cells().contains(&Cell::new(2, 1)));
        // Round-trips with remove_cell.
        g.remove_cell(Cell::new(2, 1));
        assert_eq!(g.region_at(Cell::new(2, 1)), None);
    }

    #[test]
    #[should_panic(expected = "already claimed")]
    fn add_cell_rejects_a_double_claim() {
        let mut g = RegionGraph::new(8, 8);
        // The room owns (1,1); the corridor then tries to claim it — a generation bug.
        g.add_region(RegionKind::Room, [Cell::new(1, 1)]);
        let corridor = g.add_region(RegionKind::Corridor, [Cell::new(3, 1)]);
        g.add_cell(corridor, Cell::new(1, 1));
    }
}
