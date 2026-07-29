use super::*;
use crate::facility::Terrain;

/// An open `w × h` walled box — all interior floor.
fn open(w: u32, h: u32) -> Facility {
    Facility::walled_box(w, h)
}

/// The Chebyshev (square-box) distance between two cells: the §6.1 range metric.
fn chebyshev(a: Cell, b: Cell) -> u32 {
    a.sight_distance(b)
}

/// Render a field of view as text for golden tests: `@` the viewer, `*` a seen
/// cell, `.` an unseen one. Walls are drawn `#` when seen so shadows read.
fn picture(facility: &Facility, fov: &VisibleSet, origin: Cell) -> Vec<String> {
    (0..facility.height())
        .map(|y| {
            (0..facility.width())
                .map(|x| {
                    let cell = Cell::new(x, y);
                    if cell == origin {
                        '@'
                    } else if !fov.contains(cell) {
                        '.'
                    } else if facility.terrain(cell) == Some(Terrain::Wall) {
                        '#'
                    } else {
                        '*'
                    }
                })
                .collect()
        })
        .collect()
}

/// §6.1: range is a square box, no falloff. With the full 360° arc on open
/// floor, the visible set is *exactly* the Chebyshev-≤-range box.
#[test]
fn full_arc_on_open_floor_sees_exactly_the_range_box() {
    let f = open(11, 11);
    let origin = Cell::new(5, 5);
    let fov = field_of_view(&f, origin, Direction::North, FULL_SIGHT_ARC, 3);
    for y in 0..f.height() {
        for x in 0..f.width() {
            let cell = Cell::new(x, y);
            assert_eq!(
                fov.contains(cell),
                chebyshev(origin, cell) <= 3,
                "({x},{y}) against the range-3 box"
            );
        }
    }
}

/// §6.1 **[SETTLED]**: the 8 cells around a viewer are always seen, in every
/// direction — even at arc width 1, and even the cell directly behind. Two
/// steps directly behind, though, stays dark at every arc short of 360°.
#[test]
fn the_touching_ring_is_always_seen() {
    let f = open(11, 11);
    let origin = Cell::new(5, 5);
    for facing in Direction::ALL {
        for arc in 1..=5u8 {
            let fov = field_of_view(&f, origin, facing, arc, 4);
            for n in f.neighbours(origin) {
                assert!(fov.contains(n), "{facing:?} arc {arc}: cardinal ring");
            }
            for (dx, dy) in [(-1i64, -1i64), (1, -1), (-1, 1), (1, 1)] {
                let c = Cell::new(
                    (i64::from(origin.x) + dx) as u32,
                    (i64::from(origin.y) + dy) as u32,
                );
                assert!(fov.contains(c), "{facing:?} arc {arc}: diagonal ring");
            }
        }
    }
    // Directly behind at distance 2: dark for every partial arc, lit at 360°.
    let behind = Cell::new(5, 7); // facing north, behind is south
    for arc in 1..=4u8 {
        let fov = field_of_view(&f, origin, Direction::North, arc, 4);
        assert!(!fov.contains(behind), "arc {arc} must not see behind");
    }
    let fov = field_of_view(&f, origin, Direction::North, FULL_SIGHT_ARC, 4);
    assert!(fov.contains(behind), "the 360° wait arc sees behind");
}

/// The §6.2 arc table, pinned as golden pictures: one viewer mid-floor, facing
/// north, range 4, at every arc width. This is the arc_width ↔ tier rule made
/// visible — the cone widens tier by tier, and the touching ring is present in
/// every picture. The edges are grazes past the square angle (the ray slipping
/// past a transparent side neighbour), which is the trick's real silhouette.
#[test]
fn golden_cone_shapes_per_arc_width() {
    let f = open(11, 11);
    let origin = Cell::new(5, 5);
    let shot = |arc: u8| {
        let fov = field_of_view(&f, origin, Direction::North, arc, 4);
        picture(&f, &fov, origin)
    };

    // Arc 1 — ahead only: the beam through the single transparent cell, widening
    // with distance like any 1-cell gap.
    assert_eq!(
        shot(1),
        vec![
            "...........",
            "...*****...",
            "....***....",
            "....***....",
            "....***....",
            "....*@*....",
            "....***....",
            "...........",
            "...........",
            "...........",
            "...........",
        ]
    );
    // Arc 2 — the guard's ~90° forward wedge.
    assert_eq!(
        shot(2),
        vec![
            "...........",
            ".*********.",
            ".*********.",
            ".*********.",
            "...*****...",
            "....*@*....",
            "....***....",
            "...........",
            "...........",
            "...........",
            "...........",
        ]
    );
    // Arc 3 — the player's ~180° half-disc, with the rear skirt the side
    // neighbours let sight graze into.
    assert_eq!(
        shot(3),
        vec![
            "...........",
            ".*********.",
            ".*********.",
            ".*********.",
            ".*********.",
            ".****@****.",
            ".*********.",
            ".*.......*.",
            "...........",
            "...........",
            "...........",
        ]
    );
    // Arc 4 — ~270°: only the shadow of the directly-behind cell stays dark.
    assert_eq!(
        shot(4),
        vec![
            "...........",
            ".*********.",
            ".*********.",
            ".*********.",
            ".*********.",
            ".****@****.",
            ".*********.",
            ".****.****.",
            ".***...***.",
            ".***...***.",
            "...........",
        ]
    );
    // Arc 5 — 360°: the full range box (§6.1).
    assert_eq!(
        shot(5),
        vec![
            "...........",
            ".*********.",
            ".*********.",
            ".*********.",
            ".*********.",
            ".****@****.",
            ".*********.",
            ".*********.",
            ".*********.",
            ".*********.",
            "...........",
        ]
    );
}

/// The cone follows the facing: the guard wedge pointed east is the north wedge
/// rotated, pinned as its own golden so a rotation bug cannot hide.
#[test]
fn the_cone_rotates_with_facing() {
    let f = open(11, 11);
    let origin = Cell::new(5, 5);
    let fov = field_of_view(&f, origin, Direction::East, GUARD_SIGHT_ARC, 4);
    assert_eq!(
        picture(&f, &fov, origin),
        vec![
            "...........",
            ".......***.",
            ".......***.",
            "......****.",
            "....******.",
            "....*@****.",
            "....******.",
            "......****.",
            ".......***.",
            ".......***.",
            "...........",
        ]
    );
}

/// §6.1: an opaque cell is itself seen — you see the wall face — but shadows
/// everything behind it. A free-standing wall stub ahead of the viewer.
#[test]
fn an_opaque_cell_is_seen_and_shadows_behind_itself() {
    let mut f = open(11, 11);
    f.set_terrain(5, 3, Terrain::Wall); // two ahead of the viewer
    let origin = Cell::new(5, 5);
    let fov = field_of_view(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 4);

    assert!(fov.contains(Cell::new(5, 3)), "the wall face is seen");
    assert!(
        !fov.contains(Cell::new(5, 2)) && !fov.contains(Cell::new(5, 1)),
        "the cells behind the wall are shadowed"
    );
    assert!(
        fov.contains(Cell::new(4, 2)) && fov.contains(Cell::new(6, 2)),
        "the shadow is the wall's, not the whole row's"
    );
}

/// §10.3 through the caster's eyes: a closed door panel and a hinge block
/// sight; opening the panel opens the view. The door terrain carries the
/// opacity; vision just reads the table.
#[test]
fn doors_block_sight_closed_and_pass_it_open() {
    let mut f = open(11, 11);
    for terrain in [Terrain::DoorPanelClosed, Terrain::DoorHinge] {
        f.set_terrain(5, 3, terrain);
        let fov = field_of_view(&f, Cell::new(5, 5), Direction::North, PLAYER_SIGHT_ARC, 4);
        assert!(fov.contains(Cell::new(5, 3)), "{terrain:?} face is seen");
        assert!(!fov.contains(Cell::new(5, 1)), "{terrain:?} blocks sight");
    }
    f.set_terrain(5, 3, Terrain::DoorPanelOpen);
    let fov = field_of_view(&f, Cell::new(5, 5), Direction::North, PLAYER_SIGHT_ARC, 4);
    assert!(fov.contains(Cell::new(5, 1)), "an open panel passes sight");
}

/// The symmetry the algorithm is named for: between transparent cells, at the
/// 360° arc, A sees B iff B sees A. Checked over every floor pair of a room
/// with a wall stub — the geometry that would catch a one-sided caster.
#[test]
fn vision_is_symmetric_between_open_cells() {
    let mut f = open(9, 9);
    for y in 2..=5 {
        f.set_terrain(4, y, Terrain::Wall);
    }
    let floors: Vec<Cell> = (1..8)
        .flat_map(|y| (1..8).map(move |x| Cell::new(x, y)))
        .filter(|&c| f.terrain(c) == Some(Terrain::Floor))
        .collect();
    for &a in &floors {
        let from_a = field_of_view(&f, a, Direction::North, FULL_SIGHT_ARC, 8);
        for &b in &floors {
            let from_b = field_of_view(&f, b, Direction::North, FULL_SIGHT_ARC, 8);
            assert_eq!(
                from_a.contains(b),
                from_b.contains(a),
                "asymmetry between {a:?} and {b:?}"
            );
        }
    }
}

/// A viewer against the level edge casts without panicking and simply loses
/// the off-grid part of its box — the border absorbs the cone.
#[test]
fn a_viewer_in_a_corner_is_bounded_by_the_grid() {
    let f = open(6, 6);
    let fov = field_of_view(&f, Cell::new(1, 1), Direction::North, FULL_SIGHT_ARC, 10);
    assert!(
        fov.contains(Cell::new(0, 0)),
        "the corner wall face is seen"
    );
    assert!(fov.contains(Cell::new(4, 4)));
    assert!(!fov.contains(Cell::new(5, 5).step(Direction::East).unwrap()));
}

/// §6.1 corner-solidity: two walls meeting only at a diagonal must jointly
/// occlude the pinch between them. A viewer looking straight along the join
/// used to see floor cells whose line of sight is a clean run *through* a
/// wall body — the classic diagonal corner peek. Pinned as a golden picture:
/// viewer at (1,1) waiting (360°), walls touching at (3,3)+(4,4). The cells
/// hidden behind the join — (4,3),(3,4),(5,4),(4,5) — must be dark, while the
/// wall faces and everything genuinely in view stay lit.
#[test]
fn diagonal_corner_does_not_leak_sight_through_the_pinch() {
    let mut f = open(11, 11);
    f.set_terrain(3, 3, Terrain::Wall);
    f.set_terrain(4, 4, Terrain::Wall);
    let origin = Cell::new(1, 1);
    let fov = field_of_view(&f, origin, Direction::North, FULL_SIGHT_ARC, 10);
    assert_eq!(
        picture(&f, &fov, origin),
        vec![
            "###########",
            "#@********#",
            "#*********#",
            "#**#.*****#",
            "#**...****#",
            "#***....**#",
            "#****.....#",
            "#****......",
            "#*****.....",
            "#*****.....",
            "#######....",
        ]
    );
    // The specific cells behind the diagonal join go dark (they were the leak).
    for c in [
        Cell::new(4, 3),
        Cell::new(3, 4),
        Cell::new(5, 4),
        Cell::new(4, 5),
    ] {
        assert!(!fov.contains(c), "{c:?} leaked through the corner");
    }
    // The near wall face is still seen — corner-solidity hides what is behind
    // the pinch, never the wall the viewer looks at (§6.1). The far wall
    // (4,4) is legitimately shadowed by the nearer wall (3,3) directly in
    // front of it on the diagonal.
    assert!(fov.contains(Cell::new(3, 3)), "the near wall face is seen");
    assert!(
        !fov.contains(Cell::new(4, 4)),
        "the far wall is shadowed by the near one"
    );
}

/// The corner fix closes the leak *both* ways: no floor cell the cast lights
/// has its centre-to-centre line of sight buried in a wall body, checked
/// exhaustively over every two-wall diagonal L-corner and viewer position,
/// against an independent floating-point ray as the oracle (tests may use
/// floats; the shipped path stays integer, §12.4).
#[test]
fn no_floor_cell_is_seen_through_a_wall_body() {
    // Independent oracle (a different method than the shipped integer walk):
    // clip the centre-to-centre segment against each wall cell's square,
    // slightly shrunk so a grazed corner or edge does not register — a
    // positive-length overlap means the line runs through the wall's body.
    fn body_blocked(walls: &[Cell], a: Cell, c: Cell) -> bool {
        let (px, py) = (f64::from(a.x) + 0.5, f64::from(a.y) + 0.5);
        let (dx, dy) = (f64::from(c.x) + 0.5 - px, f64::from(c.y) + 0.5 - py);
        let e = 0.02;
        walls.iter().any(|&w| {
            // Liang–Barsky clip against [wx+e, wx+1-e] × [wy+e, wy+1-e].
            let edges = [
                (-dx, px - (f64::from(w.x) + e)),
                (dx, (f64::from(w.x) + 1.0 - e) - px),
                (-dy, py - (f64::from(w.y) + e)),
                (dy, (f64::from(w.y) + 1.0 - e) - py),
            ];
            let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
            for (p, q) in edges {
                if p.abs() < 1e-12 {
                    if q < 0.0 {
                        return false;
                    }
                } else {
                    let r = q / p;
                    if p < 0.0 {
                        if r > t1 {
                            return false;
                        }
                        t0 = t0.max(r);
                    } else {
                        if r < t0 {
                            return false;
                        }
                        t1 = t1.min(r);
                    }
                }
            }
            t1 - t0 > 1e-9
        })
    }
    for wx in 3..8 {
        for wy in 3..8 {
            for &(ddx, ddy) in &[(1i64, 1i64), (1, -1), (-1, 1), (-1, -1)] {
                let mut f = open(11, 11);
                f.set_terrain(wx, wy, Terrain::Wall);
                let (bx, by) = ((wx as i64 + ddx) as u32, (wy as i64 + ddy) as u32);
                f.set_terrain(bx, by, Terrain::Wall);
                let walls: Vec<Cell> = (0..f.height())
                    .flat_map(|y| (0..f.width()).map(move |x| Cell::new(x, y)))
                    .filter(|&c| f.terrain(c) == Some(Terrain::Wall))
                    .collect();
                for oy in 1..10 {
                    for ox in 1..10 {
                        let o = Cell::new(ox, oy);
                        if f.terrain(o) != Some(Terrain::Floor) {
                            continue;
                        }
                        let fov = field_of_view(&f, o, Direction::North, FULL_SIGHT_ARC, 7);
                        for c in fov.cells() {
                            if f.terrain(c) == Some(Terrain::Floor) {
                                assert!(
                                    !body_blocked(&walls, o, c),
                                    "viewer {o:?} sees floor {c:?} through walls \
                                         {:?}+{:?}",
                                    Cell::new(wx, wy),
                                    Cell::new(bx, by)
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The #88 reopen fixture: the PR #119 alcove cupboard. A room above a
/// 1-thick wall line, a corridor below; one room cell walled up as backing
/// and the wall cell under it recessed into a cupboard. The backing and the
/// wall line meet only diagonally at the mouth's corners — the pinch the
/// leak threaded.
///
/// ```text
/// #################
/// #...............#
/// #.......#.......#   <- walled-up backing (8,2)
/// ########}########   <- wall line, } = cupboard (8,3)
/// #...............#
/// #...............#
/// #################
/// #################
/// ```
fn alcove_cupboard() -> Facility {
    let mut f = Facility::walled_box(17, 8);
    for x in 1..16 {
        f.set_terrain(x, 3, Terrain::Wall);
        f.set_terrain(x, 6, Terrain::Wall);
    }
    f.set_terrain(8, 3, Terrain::Hideout);
    f.set_terrain(8, 2, Terrain::Wall);
    f
}

/// §6.1 corner-solidity at the cupboard mouth: a viewer hidden in the
/// alcove sees the corridor out the mouth and its own touching ring —
/// including the two room cells diagonally behind the backing, which the
/// **[SETTLED]** ring keeps lit — but nothing else of the room. Before the
/// fix the cast threaded the double-walled corners and lit the room's
/// deeper floor and far border even at the 360° wait arc.
#[test]
fn a_cupboard_alcove_does_not_leak_sight_into_the_room() {
    let f = alcove_cupboard();
    let origin = Cell::new(8, 3);
    let fov = field_of_view(&f, origin, Direction::South, FULL_SIGHT_ARC, 15);
    assert_eq!(
        picture(&f, &fov, origin),
        vec![
            ".................",
            ".................",
            ".......*#*.......",
            ".......#@#.......",
            ".......***.......",
            "......*****......",
            ".....#######.....",
            ".................",
        ]
    );
    // The exact-diagonal cells beyond the pinch were the floor leak.
    for c in [Cell::new(6, 1), Cell::new(10, 1)] {
        assert!(!fov.contains(c), "{c:?} leaked through the mouth corner");
    }
    // Facing out the mouth (the §7.6 auto-face) shows exactly the same
    // view: the alcove already contains everything the wait arc can add.
    let facing = field_of_view(&f, origin, Direction::South, PLAYER_SIGHT_ARC, 15);
    assert_eq!(picture(&f, &facing, origin), picture(&f, &fov, origin));
}

/// The same pinch closed from the outside: a guard in the room diagonally
/// behind the backing still sees the cupboard interior itself — it is in
/// the guard's touching ring, and standing next to a guard is never free
/// (§6.1 **[SETTLED]**) — but no longer sees the corridor beyond it.
#[test]
fn a_guard_diagonal_to_the_backing_cannot_see_past_the_cupboard() {
    let f = alcove_cupboard();
    let origin = Cell::new(7, 2);
    let fov = field_of_view(&f, origin, Direction::South, GUARD_SIGHT_ARC, 10);
    assert_eq!(
        picture(&f, &fov, origin),
        vec![
            ".................",
            "......***........",
            "......*@#........",
            "......##*........",
            ".................",
            ".................",
            ".................",
            ".................",
        ]
    );
    assert!(
        fov.contains(Cell::new(8, 3)),
        "the adjacent cupboard interior stays seen — the touching ring"
    );
    for c in [Cell::new(9, 4), Cell::new(10, 5)] {
        assert!(!fov.contains(c), "{c:?}: the corridor leaked to the guard");
    }
}

/// The playtest leak that reopened the reopen: two alcove cupboards facing
/// each other across a corridor. From inside one, the room behind the
/// *opposite* cupboard must stay dark — the alcove is a dead end: its
/// backing blocks the straight line and its mouth corners are double-walled
/// pinches. The raw cast fans through the one-cell gap and paints the far
/// room's wall faces (floors were already retracted); the corner-sampled
/// wall retraction now darkens those too. What legitimately remains: the
/// opposite alcove's interior and its backing's face — you see into the
/// recess, never through it.
#[test]
fn a_cupboard_across_the_corridor_is_a_dead_end_not_a_window() {
    let mut f = Facility::walled_box(17, 10);
    for x in 1..16 {
        f.set_terrain(x, 3, Terrain::Wall);
        f.set_terrain(x, 6, Terrain::Wall);
    }
    // Top cupboard opens south into the corridor; backing walled up above.
    f.set_terrain(8, 3, Terrain::Hideout);
    f.set_terrain(8, 2, Terrain::Wall);
    // Bottom cupboard opens north; backing walled up below. The viewer.
    f.set_terrain(8, 6, Terrain::Hideout);
    f.set_terrain(8, 7, Terrain::Wall);
    let origin = Cell::new(8, 6);
    let fov = field_of_view(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 15);

    // Into the recess: the opposite interior and its backing's face.
    assert!(
        fov.contains(Cell::new(8, 3)),
        "the opposite alcove interior"
    );
    assert!(fov.contains(Cell::new(8, 2)), "the opposite backing's face");
    // Never through it: the room beyond stays dark — floors, the walls of
    // its far border, and the backing's room-side neighbours alike.
    for c in [
        Cell::new(7, 1),
        Cell::new(8, 1),
        Cell::new(9, 1),
        Cell::new(6, 1),
        Cell::new(10, 1),
        Cell::new(7, 2),
        Cell::new(9, 2),
        Cell::new(7, 0),
        Cell::new(8, 0),
        Cell::new(9, 0),
    ] {
        assert!(!fov.contains(c), "{c:?} shows through the opposite alcove");
    }
    // The viewer's own touching ring is intact (§6.1 [SETTLED]), including
    // the room cells diagonally behind their own backing.
    for c in [Cell::new(7, 7), Cell::new(9, 7), Cell::new(7, 5)] {
        assert!(fov.contains(c), "{c:?}: the touching ring must hold");
    }
}

/// The strictness choice, pinned: a vertex flanked by **two** walls blocks
/// the diagonal through it (they jointly occlude the pinch), while a lone
/// corner still grazes — the permissive behaviour the cone silhouette and
/// the touching ring depend on. And the closure is symmetric: dark from
/// one side means dark from the other.
#[test]
fn a_double_walled_corner_blocks_the_diagonal_a_lone_corner_grazes() {
    let mut f = open(11, 11);
    f.set_terrain(5, 4, Terrain::Wall);
    f.set_terrain(4, 5, Terrain::Wall);
    let a = Cell::new(2, 2);
    let from_a = field_of_view(&f, a, Direction::North, FULL_SIGHT_ARC, 8);
    for c in [Cell::new(5, 5), Cell::new(6, 6), Cell::new(7, 7)] {
        assert!(!from_a.contains(c), "{c:?} threaded the double corner");
    }
    assert!(from_a.contains(Cell::new(4, 4)), "this side of the pinch");
    assert!(from_a.contains(Cell::new(5, 4)), "the wall faces are seen");
    assert!(from_a.contains(Cell::new(4, 5)), "the wall faces are seen");
    let b = Cell::new(6, 6);
    let from_b = field_of_view(&f, b, Direction::North, FULL_SIGHT_ARC, 8);
    assert!(!from_b.contains(a), "the closure holds both ways");

    // Remove one wall: the vertex has a single opaque flank and the
    // diagonal grazes it freely again.
    f.set_terrain(4, 5, Terrain::Floor);
    let grazing = field_of_view(&f, a, Direction::North, FULL_SIGHT_ARC, 8);
    assert!(
        grazing.contains(Cell::new(5, 5)),
        "a lone corner never hides"
    );
    assert!(
        grazing.contains(Cell::new(6, 6)),
        "a lone corner never hides"
    );
}

/// An interior filled with wall, with corridors carved into it — the pinched
/// geometry the #121 auto-peek exists for.
fn carved(w: u32, h: u32, floors: &[(u32, u32)]) -> Facility {
    let mut f = Facility::walled_box(w, h);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            f.set_terrain(x, y, Terrain::Wall);
        }
    }
    for &(x, y) in floors {
        f.set_terrain(x, y, Terrain::Floor);
    }
    f
}

/// An L-corner: a vertical corridor meeting a horizontal arm at (3,3).
fn l_corridor() -> Facility {
    let mut floors = Vec::new();
    floors.extend((3..=8).map(|y| (3, y)));
    floors.extend((3..=8).map(|x| (x, 3)));
    carved(11, 11, &floors)
}

/// #121 auto-peek: on open floor the union adds nothing. The lean cast is
/// clipped to the origin's own range box, so leaning can only re-reveal
/// what geometry hides — it never extends reach or widens the arc where
/// nothing occludes.
#[test]
fn on_open_floor_the_peek_changes_nothing() {
    let f = open(11, 11);
    let origin = Cell::new(5, 5);
    for arc in [PLAYER_SIGHT_ARC, FULL_SIGHT_ARC] {
        let plain = field_of_view(&f, origin, Direction::North, arc, 4);
        let peek = field_of_view_with_peek(&f, origin, Direction::North, arc, 4);
        assert_eq!(
            picture(&f, &plain, origin),
            picture(&f, &peek, origin),
            "arc {arc}: open floor must gain nothing from the peek"
        );
    }
}

/// #121 auto-peek at an L-corner, pinned as goldens: standing one cell
/// short of the corner and facing it, the head-lean origin *is* the corner
/// cell, so the peek reads down the cross arm the corner walls hide from
/// the body's own cast. The plain cast keeps only the arm cell diagonally
/// ahead (the lean origin's ring reaches no further than the touching
/// ring's own reach).
#[test]
fn peeking_at_an_l_corner_reads_down_the_cross_arm() {
    let f = l_corridor();
    let origin = Cell::new(3, 4);
    let plain = field_of_view(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 8);
    let peek = field_of_view_with_peek(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 8);
    assert_eq!(
        picture(&f, &plain, origin),
        vec![
            "...........",
            "...........",
            "..####.....",
            "..#**......",
            "..#@#......",
            "..#*#......",
            "...........",
            "...........",
            "...........",
            "...........",
            "...........",
        ]
    );
    assert_eq!(
        picture(&f, &peek, origin),
        vec![
            "...........",
            "...........",
            "..########.",
            "..#******#.",
            "..#@######.",
            "..#*#......",
            "...........",
            "...........",
            "...........",
            "...........",
            "...........",
        ]
    );
    // The cross arm beyond the diagonal is the peek's delta.
    for x in 5..=8 {
        let c = Cell::new(x, 3);
        assert!(!plain.contains(c), "{c:?} hidden from the body's cast");
        assert!(peek.contains(c), "{c:?} revealed by the lean");
    }
}

/// #121 auto-peek at a T-junction: one cell short of the junction, facing
/// the stem's end, the lean origin sits in the junction and reads both
/// arms at once — the ~180° the ticket promises, from a corridor instead
/// of a cupboard.
#[test]
fn peeking_at_a_t_junction_reads_both_arms() {
    let mut floors = Vec::new();
    floors.extend((1..=9).map(|x| (x, 3)));
    floors.extend((3..=8).map(|y| (5, y)));
    let f = carved(11, 11, &floors);
    let origin = Cell::new(5, 4);
    let plain = field_of_view(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 8);
    let peek = field_of_view_with_peek(&f, origin, Direction::North, PLAYER_SIGHT_ARC, 8);
    for x in [1, 2, 8, 9] {
        let c = Cell::new(x, 3);
        assert!(!plain.contains(c), "{c:?} hidden from the body's cast");
        assert!(peek.contains(c), "{c:?} revealed by the lean");
    }
}

/// #121: a sight-blocking forward cell refuses the lean — you cannot put
/// your head into a wall or a closed door panel — and the peek collapses to
/// the plain cast exactly.
#[test]
fn a_blocked_forward_cell_means_no_lean() {
    for terrain in [Terrain::Wall, Terrain::DoorPanelClosed, Terrain::DoorHinge] {
        let mut f = open(11, 11);
        f.set_terrain(5, 4, terrain);
        let origin = Cell::new(5, 5);
        for arc in [PLAYER_SIGHT_ARC, FULL_SIGHT_ARC] {
            let plain = field_of_view(&f, origin, Direction::North, arc, 8);
            let peek = field_of_view_with_peek(&f, origin, Direction::North, arc, 8);
            assert_eq!(
                picture(&f, &plain, origin),
                picture(&f, &peek, origin),
                "{terrain:?} arc {arc}: no lean into a blocked cell"
            );
        }
    }
}

/// #121: the peek never escapes the origin's range box — the §6.1 promise
/// "range R sees at most the (2R+1)² box" holds for the union too.
#[test]
fn the_peek_stays_inside_the_origin_range_box() {
    let f = l_corridor();
    let origin = Cell::new(3, 4);
    for arc in [PLAYER_SIGHT_ARC, FULL_SIGHT_ARC] {
        let fov = field_of_view_with_peek(&f, origin, Direction::North, arc, 3);
        for c in fov.cells() {
            assert!(
                chebyshev(origin, c) <= 3,
                "arc {arc}: {c:?} escaped the range-3 box"
            );
        }
    }
}

/// #121, the flagship case: hidden in the alcove cupboard, facing out (the
/// §7.6 auto-face), the head leans through the mouth and the corridor reads
/// at ~180° — both directions to the range box — where the plain cast gets
/// only the mouth's ~90° wedge (pinned by
/// `a_cupboard_alcove_does_not_leak_sight_into_the_room` above). The room
/// behind the backing stays exactly as dark as the plain cast leaves it:
/// leaning *out* opens nothing *inward*.
#[test]
fn hidden_in_a_cupboard_the_peek_reads_the_whole_corridor() {
    let f = alcove_cupboard();
    let origin = Cell::new(8, 3);
    let peek = field_of_view_with_peek(&f, origin, Direction::South, PLAYER_SIGHT_ARC, 15);
    assert_eq!(
        picture(&f, &peek, origin),
        vec![
            ".................",
            ".................",
            ".......*#*.......",
            "########@########",
            "#***************#",
            "#***************#",
            "#################",
            ".................",
        ]
    );
    // Both corridor directions, far past the mouth wedge.
    for c in [
        Cell::new(1, 4),
        Cell::new(15, 4),
        Cell::new(3, 5),
        Cell::new(13, 5),
    ] {
        assert!(peek.contains(c), "{c:?}: the corridor must read both ways");
    }
    // The room stays dark — the peek widens the corridor, not the pinch.
    for c in [
        Cell::new(6, 1),
        Cell::new(10, 1),
        Cell::new(7, 1),
        Cell::new(9, 1),
    ] {
        assert!(
            !peek.contains(c),
            "{c:?}: leaning out must not open the room"
        );
    }
}

/// §155 the guard rear blind spot: the three cells at a guard's back — the
/// two rear diagonals (tier 4) and directly behind (tier 5) — are removed
/// from the detection set, while the sides (tier 3) and everything forward
/// still detect. Facing north here, so the rear three are the row to the
/// south; a player directly behind or rear-diagonal is undetected, one
/// beside the guard is not.
#[test]
fn a_guard_is_blind_to_the_three_cells_at_its_back() {
    let f = open(11, 11);
    let origin = Cell::new(5, 5);
    let fov = field_of_view_with_blind_spot(
        &f,
        origin,
        Direction::North,
        GUARD_SIGHT_ARC,
        4,
        BlindTier::REAR,
    );

    // The rear three go dark: rear diagonals and directly behind.
    for c in [Cell::new(4, 6), Cell::new(5, 6), Cell::new(6, 6)] {
        assert!(!fov.contains(c), "{c:?}: a rear cell must not detect");
    }
    // The sides still detect — you can never stand beside a guard unseen.
    for c in [Cell::new(4, 5), Cell::new(6, 5)] {
        assert!(fov.contains(c), "{c:?}: a side cell still detects");
    }
    // The forward ring is untouched.
    for c in [Cell::new(4, 4), Cell::new(5, 4), Cell::new(6, 4)] {
        assert!(fov.contains(c), "{c:?}: the forward ring still detects");
    }
}

/// §155 pinned as a golden picture: the guard's ~90° wedge facing north with
/// the three cells directly behind now dark where the plain cast lit them —
/// compare `golden_cone_shapes_per_arc_width`'s arc-2 shot, whose row below
/// the viewer reads `***`. Here it is bare: the blind spot, and nothing else.
#[test]
fn golden_guard_cone_with_the_rear_blind_spot() {
    let f = open(11, 11);
    let origin = Cell::new(5, 5);
    let fov = field_of_view_with_blind_spot(
        &f,
        origin,
        Direction::North,
        GUARD_SIGHT_ARC,
        4,
        BlindTier::REAR,
    );
    assert_eq!(
        picture(&f, &fov, origin),
        vec![
            "...........",
            ".*********.",
            ".*********.",
            ".*********.",
            "...*****...",
            "....*@*....",
            "...........",
            "...........",
            "...........",
            "...........",
            "...........",
        ]
    );
}

/// §155/#410: **the silhouette is the same in both arms.** Whatever tier is
/// carved, the carve touches only ring cells — every cell beyond the touching
/// ring is exactly what the plain cast produces, because the carved cells still
/// act as §6.2 artificial walls *during* the cast and are only unmarked
/// afterwards.
///
/// This is the acceptance criterion the flank experiment turns on: it must change
/// what a guard **notices**, never what walls shadow. Comparing the whole cast's
/// shape rather than only the detection set is what makes that a real assertion —
/// a carve that reshaped the cone would move cells past the ring, and this would
/// catch it. Checked at every facing, for every tier.
#[test]
fn a_blind_spot_leaves_the_cone_silhouette_untouched() {
    let f = open(11, 11);
    let origin = Cell::new(5, 5);
    // Tier 4 carves the three cells at the back; tier 3 those plus the two sides.
    for (blind, expected_removed) in [(BlindTier::REAR, 3), (BlindTier::FLANK, 5)] {
        for facing in Direction::ALL {
            let plain = field_of_view(&f, origin, facing, GUARD_SIGHT_ARC, 4);
            let carved =
                field_of_view_with_blind_spot(&f, origin, facing, GUARD_SIGHT_ARC, 4, blind);
            // Beyond the ring the two sets agree exactly — the silhouette is the
            // plain cast's, in both arms.
            for c in plain.cells() {
                if c.sight_distance(origin) > 1 {
                    assert!(
                        carved.contains(c),
                        "{blind:?} {facing:?} {c:?}: silhouette changed beyond the ring"
                    );
                }
            }
            // The whole difference is exactly the carved ring cells.
            let removed: Vec<Cell> = plain.cells().filter(|&c| !carved.contains(c)).collect();
            assert_eq!(
                removed.len(),
                expected_removed,
                "{blind:?} {facing:?}: exactly the carved ring cells are removed"
            );
            for c in removed {
                assert_eq!(
                    c.sight_distance(origin),
                    1,
                    "{blind:?} {facing:?} {c:?}: a removed cell must be a ring cell"
                );
                let (dx, dy) = (i64::from(c.x) - 5, i64::from(c.y) - 5);
                assert!(
                    blind.carves(ring_tier(facing, dx, dy)),
                    "{blind:?} {facing:?} {c:?}: a removed cell must be one this tier carves"
                );
            }
        }
    }
}

/// #410: the two arms differ by **exactly the two flank cells** — the ring
/// cardinals square-on to the facing (§6.2 tier 3), and nothing else. The
/// experiment is one tier wide, so a change that quietly took a forward cell
/// with it would be a different experiment.
#[test]
fn the_flank_arm_removes_exactly_the_two_side_cells() {
    let f = open(11, 11);
    let origin = Cell::new(5, 5);
    for facing in Direction::ALL {
        let rear =
            field_of_view_with_blind_spot(&f, origin, facing, GUARD_SIGHT_ARC, 4, BlindTier::REAR);
        let flank =
            field_of_view_with_blind_spot(&f, origin, facing, GUARD_SIGHT_ARC, 4, BlindTier::FLANK);
        let extra: Vec<Cell> = rear.cells().filter(|&c| !flank.contains(c)).collect();
        assert_eq!(extra.len(), 2, "{facing:?}: the two flanks, no more");
        for c in extra {
            let (dx, dy) = (i64::from(c.x) - 5, i64::from(c.y) - 5);
            assert_eq!(
                ring_tier(facing, dx, dy),
                3,
                "{facing:?} {c:?}: the extra cells are exactly tier 3"
            );
        }
        // And the flank arm never *adds* anything the rear arm did not have.
        assert!(
            flank.cells().all(|c| rear.contains(c)),
            "{facing:?}: the narrower arm is a strict subset",
        );
    }
}

/// A default set is the empty placeholder: it contains nothing.
#[test]
fn a_default_visible_set_is_empty() {
    let set = VisibleSet::default();
    assert!(!set.contains(Cell::new(0, 0)));
    assert_eq!(set.cells().count(), 0);
}
