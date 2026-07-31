//! Ducts: the player-only crawlspace (§10.7).
//!
//! Climbing in from a mouth, being confined to the recorded path, the degraded sense
//! and the mouth peek that are the crawl's whole cost, and the guard-facing
//! invariant: a duct changes *nothing* for a guard — it neither captures the crawler
//! under it nor is blocked by them.

use crate::facility::Facility;
use crate::state::*;
use crate::test_support::open_room;

/// A hand-built duct fixture: a wall band under the top border with a 4-cell duct
/// threaded through it — entries at `(2,1)` and `(5,1)`, interior `(3,1)`/`(4,1)` —
/// opening (mouths `(2,2)`/`(5,2)`) into an open room below (rows 2..7). The player
/// starts on the near mouth `(2,2)`, facing the entry.
///
/// ```text
///   #########   row 0 (border)
///   #.=..=..#   row 1: wall band, = are entries at x=2,5; interior wall at x=3,4
///   #.......#   rows 2..7: open room; mouths at (2,2) and (5,2)
///   ...
/// ```
pub(super) fn duct_world() -> State {
    duct_world_with(Vec::new())
}

/// [`duct_world`] with `guards` posted in the room below — the fixture the tests that
/// need something for the crawler to perceive (or fail to) build on.
pub(super) fn duct_world_with(guards: Vec<crate::Guard>) -> State {
    let mut f = Facility::walled_box(9, 9);
    for x in 1..=7 {
        f.set_terrain(x, 1, Terrain::Wall); // the wall band the duct hides in
    }
    f.set_terrain(2, 1, Terrain::DuctEntry);
    f.set_terrain(5, 1, Terrain::DuctEntry);
    let duct = crate::Duct::new(vec![
        Cell::new(2, 1),
        Cell::new(3, 1),
        Cell::new(4, 1),
        Cell::new(5, 1),
    ]);
    let layout = crate::Layout::from_facility(f).with_ducts(vec![duct]);
    State::new(
        layout,
        Cell::new(2, 2),
        Direction::North,
        guards,
        Vec::new(),
        Cell::new(7, 7),
    )
}

/// §10.7: bump the mouth to climb in, crawl the path one cell per turn, climb out the
/// far mouth — every step a spent turn (§4.4), concealment on the whole time inside.
#[test]
fn enter_crawl_and_climb_out_of_a_duct() {
    let mut s = duct_world();
    assert!(!s.in_duct());
    let t0 = s.turn();

    // Bump the entry from the mouth: a decision, the turn spent, now concealed inside.
    let e = s.step(Input::Step(Direction::North));
    assert!(e.contains(&Event::EnteredDuct {
        at: Cell::new(2, 1)
    }));
    assert!(s.in_duct());
    assert_eq!(s.player(), Cell::new(2, 1));
    assert_eq!(s.turn(), t0 + 1, "entering spends the turn");

    // Crawl the path east, one cell per spent turn.
    for x in [3, 4, 5] {
        let before = s.turn();
        let e = s.step(Input::Step(Direction::East));
        assert!(e.contains(&Event::DuctCrawled {
            to: Cell::new(x, 1)
        }));
        assert_eq!(s.player(), Cell::new(x, 1));
        assert!(s.in_duct());
        assert_eq!(s.turn(), before + 1, "each crawl spends the turn");
    }

    // At the far entry, step out through its mouth: an ordinary move onto the floor.
    let e = s.step(Input::Step(Direction::South));
    assert!(e.contains(&Event::Moved {
        to: Cell::new(5, 2)
    }));
    assert!(!s.in_duct(), "climbing out the far mouth leaves the duct");
    assert_eq!(s.player(), Cell::new(5, 2));
}

/// §10.7 confinement: inside a duct the only way off the path is the mouth of the
/// entry you stand on. A step into the surrounding wall — or off an interior cell
/// toward the floor it happens to touch — is a solid bump that changes nothing.
#[test]
fn a_duct_confines_the_player_to_its_path() {
    let mut s = duct_world();
    s.step(Input::Step(Direction::North)); // enter at (2,1)
    s.step(Input::Step(Direction::East)); // crawl to interior (3,1)
    assert_eq!(s.player(), Cell::new(3, 1));

    // (3,1) touches floor at (3,2), but it is not an entry — stepping there is walled.
    let e = s.step(Input::Step(Direction::South));
    assert!(e.contains(&Event::Bumped {
        into: Cell::new(3, 2)
    }));
    assert_eq!(s.player(), Cell::new(3, 1), "no exit from a mid-duct cell");
    assert!(s.in_duct());

    // Into the top wall: also a solid bump.
    let e = s.step(Input::Step(Direction::North));
    assert!(e.contains(&Event::Bumped {
        into: Cell::new(3, 0)
    }));
    assert_eq!(s.player(), Cell::new(3, 1));
}

/// §6.1/§10.7 perception: an **entry** cell casts the live mouth peek out into the
/// room, while a **mid-duct** cell has no live vision at all — memory only.
#[test]
fn an_entry_peeks_but_a_mid_duct_cell_is_blind() {
    let mut s = duct_world();
    s.step(Input::Step(Direction::North)); // enter at (2,1)

    // The entry peeks down its mouth into the room below.
    assert!(
        s.player_fov().contains(Cell::new(2, 4)),
        "the entry-cell peek reads down the room"
    );
    assert!(
        !s.player_fov().contains(Cell::new(2, 0)),
        "no live vision back through the top wall"
    );

    // Crawl to an interior cell: FOV collapses to the occupied cell — no live window,
    // not even the floor the cell touches (§10.7's information cost).
    s.step(Input::Step(Direction::East)); // (3,1)
    assert!(s.player_fov().contains(Cell::new(3, 1)));
    assert!(
        !s.player_fov().contains(Cell::new(3, 2)),
        "a mid-duct cell sees no live world"
    );
}

/// §10.7: the guard sense shrinks inside a duct and **Wait does not widen it** —
/// unlike the open floor, where waiting buys the larger box (§9.1).
#[test]
fn the_in_duct_sense_is_reduced_and_wait_does_not_widen_it() {
    let mut s = duct_world();
    s.step(Input::Step(Direction::North)); // enter the duct

    assert!(s.in_duct());
    assert_eq!(s.sense_range(), DUCT_SENSE_RANGE, "reduced inside a duct");
    s.step(Input::Wait);
    assert_eq!(
        s.sense_range(),
        DUCT_SENSE_RANGE,
        "waiting inside a duct does not widen the sense",
    );

    // Climb out onto the floor (crawl to the far entry, step out): the sense is normal
    // again, and there waiting *does* widen it (§9.1).
    for _ in 0..3 {
        s.step(Input::Step(Direction::East));
    }
    s.step(Input::Step(Direction::South)); // out at (5,2)
    assert!(!s.in_duct());
    assert_eq!(s.sense_range(), PLAYER_SENSE_RANGE, "normal on the floor");
    s.step(Input::Wait);
    assert_eq!(
        s.sense_range(),
        PLAYER_SENSE_RANGE_WAITING,
        "waiting on the floor widens the sense as usual",
    );
}

/// §4.5/§10.7: a guard on the mouth can never follow the player into a duct —
/// contact is refused like an occupied cupboard, and the concealed crawler is never
/// detected. The guard is sent to the player's duct cell and holds at the mouth.
#[test]
fn a_guard_cannot_capture_or_enter_a_duct() {
    let mut f = Facility::walled_box(9, 9);
    for x in 1..=7 {
        f.set_terrain(x, 1, Terrain::Wall);
    }
    f.set_terrain(2, 1, Terrain::DuctEntry);
    f.set_terrain(5, 1, Terrain::DuctEntry);
    let duct_cells = [
        Cell::new(2, 1),
        Cell::new(3, 1),
        Cell::new(4, 1),
        Cell::new(5, 1),
    ];
    let layout =
        crate::Layout::from_facility(f).with_ducts(vec![crate::Duct::new(duct_cells.to_vec())]);
    // Player at the near mouth; a guard patrolling up to it, its cone falling on the
    // entry cell every time it stands there. The player climbs into the duct on the
    // first step — "in a duct" is entered state now (§10.7), not a placement.
    let mut s = State::new(
        layout,
        Cell::new(2, 2),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(2, 6), Cell::new(2, 2))],
        Vec::new(),
        Cell::new(7, 7),
    );
    s.step(Input::Step(Direction::North)); // bump the mouth to climb in at (2,1)
    assert!(s.in_duct());

    // Over the patrol the guard reaches the mouth (2,2), adjacent to the entry with
    // its cone on it — yet the concealed crawler is never seen, the guard never steps
    // onto a duct cell (they are wall to it), and there is no capture (§10.7).
    let mut reached_mouth = false;
    for _ in 0..12 {
        let e = s.step(Input::Wait);
        assert!(
            !e.iter().any(|e| matches!(e, Event::Captured { .. })),
            "a player in a duct is never captured",
        );
        assert_eq!(s.outcome(), Outcome::Playing);
        let g = s.guards()[0].pos();
        assert!(
            !duct_cells.contains(&g),
            "a guard never enters a duct cell (it is wall to guards)",
        );
        assert!(
            !s.guards()[0].detected_player(),
            "a concealed crawler is never detected, even from the mouth",
        );
        reached_mouth |= g == Cell::new(2, 2);
    }
    assert!(
        reached_mouth,
        "the guard did come adjacent to the entry, so concealment was tested at contact range",
    );
}

/// §10.7 cross-room routing: a duct interior may now overlie **room floor** the guards
/// walk. A guard crossing that floor must behave as if the duct were not there (nothing
/// guard-facing changes): it walks straight over the concealed crawler's cell — neither
/// blocked by them nor capturing them — and never detects the crawler.
///
/// The fixture: rows 1–2 are a solid wall band, rows 4–6 another, so **row 3 is a
/// one-wide floor corridor** the guard is confined to and must sweep end to end. A
/// duct runs entry `(2,2)` → `(3,2)` → `(3,3)` → `(4,3)` → `(5,3)` → `(5,2)` → entry
/// `(6,2)`, so its middle three cells `(3,3)`/`(4,3)`/`(5,3)` cross the corridor floor.
/// The player crawls to `(3,3)`; the guard patrols row 3 straight through it.
#[test]
fn a_guard_walks_over_a_crawler_on_a_floor_duct_cell() {
    let mut f = Facility::walled_box(9, 9);
    for x in 1..=7 {
        f.set_terrain(x, 1, Terrain::Wall);
        f.set_terrain(x, 2, Terrain::Wall); // the wall band the entries recess in
        f.set_terrain(x, 4, Terrain::Wall); // seal the corridor to one row so the
        f.set_terrain(x, 5, Terrain::Wall); // guard is confined to row 3 and must
        f.set_terrain(x, 6, Terrain::Wall); // sweep across the crawler's cell
    }
    f.set_terrain(2, 2, Terrain::DuctEntry); // mouth (2,3)
    f.set_terrain(6, 2, Terrain::DuctEntry); // mouth (6,3)
    let path = vec![
        Cell::new(2, 2),
        Cell::new(3, 2), // wall backing
        Cell::new(3, 3), // floor — crosses the room
        Cell::new(4, 3), // floor
        Cell::new(5, 3), // floor
        Cell::new(5, 2), // wall backing
        Cell::new(6, 2),
    ];
    let layout = crate::Layout::from_facility(f).with_ducts(vec![crate::Duct::new(path)]);
    // Player at the near mouth; a guard patrolling row 3, straight across the duct's
    // floor cells (its start (5,3) is itself one of them — floor to the guard).
    let mut s = State::new(
        layout,
        Cell::new(2, 3),
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(5, 3), Cell::new(1, 3))
            .with_beat((1..8).map(|x| Cell::new(x, 3)).collect())],
        Vec::new(),
        Cell::new(7, 7),
    );
    s.step(Input::Step(Direction::North)); // climb in at (2,2)
    s.step(Input::Step(Direction::East)); // crawl to backing (3,2)
    s.step(Input::Step(Direction::South)); // crawl onto the floor cell (3,3)
    assert!(s.in_duct());
    assert_eq!(s.player(), Cell::new(3, 3));

    // The guard oscillates along row 3 and passes through the crawler's floor cell.
    let mut guard_stood_on_crawler = false;
    for _ in 0..16 {
        let e = s.step(Input::Wait);
        assert!(
            !e.iter().any(|e| matches!(e, Event::Captured { .. })),
            "a crawler on a floor duct cell is never captured",
        );
        assert_eq!(s.outcome(), Outcome::Playing);
        assert!(s.in_duct(), "the crawler stays in the duct throughout");
        assert_eq!(
            s.player(),
            Cell::new(3, 3),
            "and stays put on the floor cell"
        );
        assert!(
            !s.guards()[0].detected_player(),
            "the concealed crawler is never detected, even underfoot",
        );
        guard_stood_on_crawler |= s.guards()[0].pos() == Cell::new(3, 3);
    }
    assert!(
        guard_stood_on_crawler,
        "the guard walked straight over the crawler's floor cell — not blocked by it",
    );
}

/// §10.7: a body cannot follow the player into the walls, so a **dragging** player is
/// refused entry — the entry reads solid until the body is let go.
#[test]
fn you_cannot_enter_a_duct_while_dragging_a_body() {
    let mut layout = open_room(10, 10);
    layout.place(Cell::new(5, 5), Terrain::Hideout);
    layout.place(Cell::new(7, 4), Terrain::DuctEntry);
    layout.place(Cell::new(8, 4), Terrain::Wall);
    let layout = layout.with_ducts(vec![crate::Duct::new(vec![
        Cell::new(7, 4),
        Cell::new(8, 4),
    ])]);
    let mut s = State::new(
        layout,
        Cell::new(5, 5),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 4))],
        Vec::new(),
        Cell::new(8, 8),
    );
    // Take the guard down from concealment, then pick up the body and carry it east.
    s.step(Input::Step(Direction::North)); // takedown at (5,4)
    s.step(Input::Step(Direction::North)); // climb out onto the body at (5,4)
    s.step(Input::Wait); // stand on the body: take hold (§8.3/#451)
    s.step(Input::Step(Direction::East)); // step off, hauling; player (6,4)
    assert!(s.dragging().is_some());
    assert_eq!(s.player(), Cell::new(6, 4));

    // Bump the duct entry to the east: refused while a body is in hand.
    let e = s.step(Input::Step(Direction::East));
    assert!(!s.in_duct(), "a dragging player cannot climb into a duct");
    assert_eq!(s.player(), Cell::new(6, 4), "the bump changed nothing");
    assert!(e.contains(&Event::Bumped {
        into: Cell::new(7, 4)
    }));
    assert!(s.dragging().is_some(), "still holding the body");
}

/// §11.4: the usable line offers "duct: enter" at a mouth, and nothing once inside
/// (crawling is movement, not an offered interaction).
#[test]
fn the_usable_line_offers_duct_enter_at_a_mouth() {
    let mut s = duct_world();
    assert!(
        s.affordances()
            .iter()
            .any(|&(dir, a)| dir == Some(Direction::North) && a == Affordance::EnterDuct),
        "the mouth offers duct: enter",
    );
    s.step(Input::Step(Direction::North)); // climb in
    assert!(
        !s.affordances()
            .iter()
            .any(|(_, a)| *a == Affordance::EnterDuct),
        "inside, there is nothing to enter",
    );
}

/// A duct **buried in a thick wall**: a 3-cell-deep band (rows 3..5) between an
/// upper and a lower room, with the crawl threaded along its hidden middle row.
///
/// Unlike [`duct_world`]'s single-cell band — whose interior is plainly in view from
/// the room below — every interior cell here sits behind a wall face from both
/// rooms, so it can only ever enter tile memory by being *crawled*. That is what
/// makes it the fixture for the §11.5a no-trace rule.
///
/// ```text
///   #########   row 0 (border)
///   #.......#   rows 1..2: upper room; the player starts at (2,2)
///   #.......#
///   ##=######   row 3: wall band — entry at (2,3), mouth (2,2)
///   ##.....##   row 4: wall band — the interior run, invisible from either room
///   ######=##   row 5: wall band — entry at (6,5), mouth (6,6)
///   #.......#   rows 6..7: lower room
/// ```
fn buried_duct_world() -> State {
    let mut f = Facility::walled_box(9, 9);
    for y in 3..=5 {
        for x in 1..=7 {
            f.set_terrain(x, y, Terrain::Wall);
        }
    }
    f.set_terrain(2, 3, Terrain::DuctEntry);
    f.set_terrain(6, 5, Terrain::DuctEntry);
    let duct = crate::Duct::new(vec![
        Cell::new(2, 3),
        Cell::new(2, 4),
        Cell::new(3, 4),
        Cell::new(4, 4),
        Cell::new(5, 4),
        Cell::new(6, 4),
        Cell::new(6, 5),
    ]);
    let layout = crate::Layout::from_facility(f).with_ducts(vec![duct]);
    State::new(
        layout,
        Cell::new(2, 2),
        Direction::South,
        Vec::new(),
        Vec::new(),
        Cell::new(7, 7),
    )
}

/// §11.5a/§10.7: a duct's **interior** never enters tile memory, so crawling a
/// shortcut leaves no trace of its route on the base map. Entries are geometry and
/// do accumulate — they are the mouths you plan around either way.
///
/// This matters because memory is what tells explored geometry from unexplored
/// (#307): were an interior cell remembered, the crawled path would read as known
/// wall threading the map, giving away a route the design keeps in its own layer.
#[test]
fn crawling_a_duct_never_remembers_its_interior() {
    let mut s = buried_duct_world();
    let interior = [
        Cell::new(2, 4),
        Cell::new(3, 4),
        Cell::new(4, 4),
        Cell::new(5, 4),
        Cell::new(6, 4),
    ];
    let entries = [Cell::new(2, 3), Cell::new(6, 5)];
    assert!(
        !interior.iter().any(|&c| s.memory().contains(c)),
        "the fixture must start with its interior unseen, or it proves nothing",
    );

    s.step(Input::Step(Direction::South)); // bump the mouth: climb in at (2,3)
    assert!(s.in_duct());
    for step in [
        Direction::South,
        Direction::East,
        Direction::East,
        Direction::East,
        Direction::East,
        Direction::South,
    ] {
        s.step(Input::Step(step));
        let leaked: Vec<_> = interior
            .iter()
            .filter(|&&c| s.memory().contains(c))
            .collect();
        assert!(
            leaked.is_empty(),
            "crawling remembered interior cells {leaked:?} at {:?}",
            s.player(),
        );
    }
    assert_eq!(s.player(), Cell::new(6, 5), "reached the far entry");
    s.step(Input::Step(Direction::South)); // climb out into the lower room

    for c in interior {
        assert!(
            !s.memory().contains(c),
            "{c:?} is interior — the crawled path must leave no trace",
        );
    }
    for c in entries {
        assert!(
            s.memory().contains(c),
            "{c:?} is an entry: geometry, remembered like any cell stood on",
        );
    }
}

/// Memory is **monotonic** (§11.5a), and holding a crawled interior cell back must
/// not dent that: a wall cell already remembered from the room side stays remembered
/// after a duct is crawled through it. The exclusion applies to the incoming view,
/// never to the accumulator.
#[test]
fn crawling_does_not_erase_a_wall_already_remembered() {
    let mut s = duct_world();
    let seen_from_below = Cell::new(3, 1);
    assert!(
        s.memory().contains(seen_from_below),
        "the wall band is in view from the starting mouth",
    );

    s.step(Input::Step(Direction::North)); // climb in
    s.step(Input::Step(Direction::East)); // crawl onto (3,1) itself
    assert_eq!(s.player(), seen_from_below);
    assert!(
        s.memory().contains(seen_from_below),
        "crawling through a wall you had already looked at must not un-remember it",
    );
}

// ---------------------------------------------------------------------------
// Auto lateral-shift past an obstacle (#57) — the traversal experiment.
// ---------------------------------------------------------------------------
