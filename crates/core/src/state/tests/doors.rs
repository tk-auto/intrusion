//! Doors, both halves (§10.4 and §9.4).
//!
//! What a door does — bumped open by the player or a guard, timed shut by an
//! automatic door's delay, pulled shut behind a Calm guard, and never, ever closing
//! on an occupant — and what the player *feels* of a door that changes out of sight:
//! the §9.4 cue, its range, and its fade. The **Autodoors** ability (§8.3/§7.6) is
//! here too: it is a door subsystem wearing an ability's clothes.

use super::ducts::duct_world;
use crate::facility::Facility;
use crate::region::{DoorKind, RegionGraph, RegionKind};
use crate::state::*;
use crate::test_support::{region_strip, solo};
use crate::vision::field_of_view;
use crate::{generate, generate_level, DoorId, Rng};

/// §10.4: a closed door does not stop a guard — its route runs straight
/// through, and walking into the panel is the bump that opens it. The door is
/// the guard's whole action that turn; it steps through on the next. Guard traffic
/// opens the facility up over a level; the close-behind (#146) is exercised on its
/// own below, so this test turns it off to isolate the opening and pass-through.
#[test]
fn a_guard_walking_its_route_opens_the_door_and_passes_through() {
    let layout = region_strip();
    let panel = Cell::new(4, 2);
    let door = layout.regions().door_at(panel).unwrap();
    let mut s = State::new(
        layout,
        Cell::new(13, 4), // parked in corridor D, two closed doors away
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(2, 2), Cell::new(6, 2))],
        Vec::new(),
        Cell::new(13, 1),
    );
    s.set_guard_close_chance(0); // isolate opening/pass-through from the close (#146)

    // The startup turn is a quarter-turn in place (§229): heading east off its south
    // spawn facing, the Calm guard rotates east without moving; a Wait then walks it
    // up against the closed panel.
    assert_eq!(s.guards()[0].pos(), Cell::new(2, 2));
    assert_eq!(s.guards()[0].facing(), Direction::East);
    s.step(Input::Wait);
    assert_eq!(s.guards()[0].pos(), Cell::new(3, 2));
    assert!(!s.layout().regions().door(door).is_open());

    // Its next step is *into* the panel: the walk-in opens the door instead.
    let events = s.step(Input::Wait);
    assert!(events.contains(&Event::DoorOpened {
        at: panel,
        by_player: false,
    }));
    assert_eq!(
        s.guards()[0].pos(),
        Cell::new(3, 2),
        "the door was the turn"
    );
    assert!(s.layout().regions().door(door).is_open());

    // Then it walks through the doorway into the corridor, door left open.
    s.step(Input::Wait);
    assert_eq!(s.guards()[0].pos(), panel, "onto the open panel");
    s.step(Input::Wait);
    assert_eq!(s.guards()[0].pos(), Cell::new(5, 2), "into the corridor");
    assert!(
        s.layout().regions().door(door).is_open(),
        "close turned off: the door stays open behind the guard",
    );
    assert_eq!(s.outcome(), Outcome::Playing);
}

/// §10.4/#146: a Calm guard that walks fully through a hinged door closes it
/// behind itself — the counter-pressure to guard traffic's monotonic opening, so
/// an open door stays evidence someone passed. The close-behind chance is forced
/// to 100 to make the *sometimes* certain for the assertion; the probability
/// itself is pinned in guard.rs and swept for determinism elsewhere. The shut
/// surfaces as a [`DoorClosed`](Event::DoorClosed) event the player can read.
#[test]
fn a_calm_guard_closes_the_door_behind_itself() {
    let layout = region_strip();
    let panel = Cell::new(4, 2);
    let door = layout.regions().door_at(panel).unwrap();
    let mut s = State::new(
        layout,
        Cell::new(13, 4), // parked in corridor D, well clear of the door
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(2, 2), Cell::new(6, 2))],
        Vec::new(),
        Cell::new(13, 1),
    );
    s.set_guard_close_chance(100); // make the "sometimes" certain for the test

    // The startup turn is a quarter-turn in place (§229): heading east off its south
    // spawn facing, the Calm guard rotates east without moving; a Wait then walks it
    // up against the closed panel (§10.4).
    assert_eq!(s.guards()[0].pos(), Cell::new(2, 2));
    assert_eq!(s.guards()[0].facing(), Direction::East);
    s.step(Input::Wait);
    assert_eq!(s.guards()[0].pos(), Cell::new(3, 2));

    s.step(Input::Wait); // the walk-in opens the door; the guard holds
    assert!(s.layout().regions().door(door).is_open());
    s.step(Input::Wait); // steps onto the open panel
    assert_eq!(s.guards()[0].pos(), panel);
    assert!(
        s.layout().regions().door(door).is_open(),
        "still in the throat: nothing to close behind yet",
    );

    // Stepping clear of the panel: the Calm guard shuts the door behind itself.
    let events = s.step(Input::Wait);
    assert_eq!(s.guards()[0].pos(), Cell::new(5, 2), "into the corridor");
    assert!(
        !s.layout().regions().door(door).is_open(),
        "the guard closed the door behind itself",
    );
    assert!(
        events.contains(&Event::DoorClosed {
            at: panel,
            by_player: false,
        }),
        "the shut surfaces as an event",
    );
    assert_eq!(s.outcome(), Outcome::Playing);
}

/// §10.4/#146 end-to-end: on real generated geometry, with the close-behind
/// certain, Calm guards patrolling their beats do shut doors behind them — the
/// wiring fires on the corridor-first facility, not just the hand-built strip.
#[test]
fn guard_close_behind_fires_on_generated_levels() {
    use crate::test_support::seed_sweep;
    let mut any_close = false;
    for seed in seed_sweep(32) {
        let mut rng = Rng::new(seed);
        let (layout, placement) = generate_level(
            &crate::LevelConfig::V1,
            &LevelModifiers::default(),
            &mut rng,
        )
        .expect("v1 generates");
        let guards = placement.guards(&layout);
        let mut s = State::new(
            layout,
            placement.player(),
            Direction::North,
            guards,
            placement.intel().iter().copied(),
            placement.exit(),
        )
        .with_rng(rng);
        s.set_guard_close_chance(100);

        for _ in 0..200 {
            if s.outcome() != Outcome::Playing {
                break;
            }
            if s.step(Input::Wait)
                .iter()
                .any(|e| matches!(e, Event::DoorClosed { .. }))
            {
                any_close = true;
                break;
            }
        }
        if any_close {
            break;
        }
    }
    assert!(
        any_close,
        "a Calm patrol closes a door behind itself somewhere in the sweep",
    );
}

/// A hand-built state whose two rooms are joined by one **automatic** door
/// (§10.4/#147) with close `delay` — a frameless 3-panel span down wall column 3,
/// no hinges. The player starts in the left room facing the door; no guards. The
/// fixture for the auto-close timer in the running loop.
fn auto_door_state(delay: u32) -> (State, DoorId) {
    let cells = |xs: std::ops::Range<u32>| {
        xs.flat_map(|x| (1..4).map(move |y| Cell::new(x, y)))
            .collect::<Vec<_>>()
    };
    let mut f = Facility::walled_box(7, 5);
    let mut g = RegionGraph::new(7, 5);
    let left = g.add_region(RegionKind::Room, cells(1..3));
    let right = g.add_region(RegionKind::Room, cells(4..6));
    let panels: Vec<Cell> = (1..4).map(|y| Cell::new(3, y)).collect();
    for &p in &panels {
        f.set_terrain(p.x, p.y, Terrain::DoorPanelClosed);
    }
    let door = g.add_door(left, right, [], panels, DoorKind::Automatic { delay });
    let s = State::new(
        Layout::from_parts(f, g),
        Cell::new(2, 2), // left room, next to the panel at (3,2)
        Direction::East, // facing the door
        Vec::new(),
        Vec::new(),
        Cell::new(4, 3), // exit parked in the right room, unused
    )
    .with_loadout(Loadout::innate().with(AbilityId::Autodoors));
    (s, door)
}

/// §10.4/#147 in the loop: an automatic door the player opens shuts itself a few
/// turns after the doorway is vacated, with no hand needed — and the shut reaches
/// the player as a [`DoorClosed`](Event::DoorClosed) event.
#[test]
fn an_automatic_door_closes_itself_in_the_loop() {
    let (mut s, door) = auto_door_state(3);
    let panel = Cell::new(3, 2);

    // Bump the closed panel: it opens (§4.3), and the countdown is armed.
    let opened = s.step(Input::Step(Direction::East));
    assert!(opened.contains(&Event::DoorOpened {
        at: panel,
        by_player: true,
    }));
    assert!(s.layout().regions().door(door).is_open());
    assert_eq!(s.player(), Cell::new(2, 2), "the bump opened, did not move");

    // Waiting clear of the doorway, it times out and shuts on its own.
    let e1 = s.step(Input::Wait);
    assert!(s.layout().regions().door(door).is_open(), "still open");
    assert!(!e1.iter().any(|e| matches!(e, Event::DoorClosed { .. })));
    let e2 = s.step(Input::Wait);
    assert!(
        !s.layout().regions().door(door).is_open(),
        "the automatic door timed out and shut itself",
    );
    assert!(
        e2.iter().any(|e| matches!(e, Event::DoorClosed { .. })),
        "the shut reaches the player as an event",
    );
}

/// §10.4/#147: an automatic door never crushes — the player standing in the
/// doorway holds it open indefinitely, and it only times out once they step clear.
#[test]
fn an_automatic_door_never_shuts_on_the_player_in_the_doorway() {
    let (mut s, door) = auto_door_state(2);

    s.step(Input::Step(Direction::East)); // open it
    s.step(Input::Step(Direction::East)); // step into the doorway (onto the panel)
    assert_eq!(s.player(), Cell::new(3, 2), "standing on the panel");

    // Wait in the throat far longer than the delay: it will not close on the player.
    for _ in 0..8 {
        s.step(Input::Wait);
        assert!(
            s.layout().regions().door(door).is_open(),
            "the door is held open by the player in it",
        );
    }

    // Step clear into the far room, and it times out. Leaving the panel is itself
    // the first vacant tick (delay 2: 2 → 1), so it shuts on the next turn.
    s.step(Input::Step(Direction::East)); // onto (4,2): vacant tick 1
    assert!(s.layout().regions().door(door).is_open(), "one tick left");
    let shut = s.step(Input::Wait); // vacant tick 2 → closes
    assert!(!s.layout().regions().door(door).is_open());
    assert!(
        shut.iter().any(|e| matches!(e, Event::DoorClosed { .. })),
        "it times out once the doorway is finally clear",
    );
}

/// §10.4/#147: an automatic door offers **open** from the usable line when closed,
/// and *no close affordance* when open (there is no hinge to bump) — you simply
/// walk through it. The whole point of the frameless span.
#[test]
fn an_automatic_door_offers_open_but_never_close() {
    let (mut s, _door) = auto_door_state(3);

    // Closed and faced: the usable line offers "door: open" to the east.
    assert!(
        s.affordances()
            .contains(&(Direction::East, Affordance::OpenDoor)),
        "a closed automatic door offers open",
    );

    s.step(Input::Step(Direction::East)); // open it; the player stays put
                                          // Open now: the east cell is a walk-through, so no door affordance at all —
                                          // and a close is never offered, because an automatic door has no handle.
    let affs = s.affordances();
    assert!(
        !affs.iter().any(|(_, a)| *a == Affordance::CloseDoor),
        "an automatic door never offers close",
    );
    assert!(
        !affs
            .iter()
            .any(|(d, a)| *d == Direction::East && *a == Affordance::OpenDoor),
        "an open automatic door is walked through, not re-opened",
    );
}

/// Bumping a closed door opens it and spends the turn (§4.3, §10.4). Uses a
/// generated facility, which is where real doors live: stand on a floor cell next
/// to a panel and step into it.
#[test]
fn bumping_a_closed_door_opens_it() {
    let layout = generate(40, 40, &mut Rng::new(7)).unwrap();
    let (id, panel) = {
        let (id, door) = layout.regions().doors().next().unwrap();
        (id, door.panels()[0])
    };

    // One of the four orthogonal approaches stands on floor and bumps the panel.
    let opened = Direction::ALL.into_iter().any(|dir| {
        let Some(from) = panel.step(dir.opposite()) else {
            return false;
        };
        if !layout.facility().can_enter(from, ACTOR_FILL) {
            return false;
        }
        let mut s = State::new(
            layout.clone(),
            from,
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(1, 1),
        );
        let opened = s.step(Input::Step(dir))
            == vec![Event::DoorOpened {
                at: panel,
                by_player: true,
            }];
        if opened {
            assert!(s.layout().regions().door(id).is_open());
            assert_eq!(s.turn(), 1, "opening a door spends the turn");
        }
        opened
    });
    assert!(opened, "one approach must bump the panel open");
}

/// §10.4: **a door never closes on an actor** — doors don't crush. Standing on a
/// panel and bumping the hinge to shut the door must be refused, leaving the door
/// open and the panel walk-through. (Regression: the close check once consulted
/// only guards, so a player on a panel got shut in on themselves.)
#[test]
fn a_door_will_not_close_on_the_player() {
    // Find a door across seeds whose panel can be reached from a perpendicular
    // floor cell and has a hinge adjacent along the door line, then try to shut it
    // on ourselves.
    for seed in 0..64 {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let Some((id, from, into, panel, hinge_dir)) = crush_scenario(&layout) else {
            continue;
        };

        // Exit parked on the border corner (always wall, never walked): a valid
        // Cell we never touch, so stamping it can't disturb the door.
        let mut s = State::new(
            layout,
            from,
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(0, 0),
        );

        // Open the door, then step onto the now-open panel.
        assert_eq!(
            s.step(Input::Step(into)),
            vec![Event::DoorOpened {
                at: panel,
                by_player: true,
            }]
        );
        assert_eq!(s.step(Input::Step(into)), vec![Event::Moved { to: panel }]);
        assert_eq!(s.player(), panel);

        // Bump the hinge to close: refused — we're on a panel. Nothing changes.
        let events = s.step(Input::Step(hinge_dir));
        assert!(events.is_empty(), "a refused close is a free no-op");
        assert!(
            s.layout().regions().door(id).is_open(),
            "seed {seed}: the door shut on the player"
        );
        assert_eq!(
            s.layout().facility().terrain(panel),
            Some(Terrain::DoorPanelOpen),
            "seed {seed}: the panel went solid under the player — crushed"
        );
        assert_eq!(s.player(), panel, "the player is unmoved and uncrushed");
        return;
    }
    panic!("no door with a reachable end panel found in 64 seeds");
}

/// A door setup for the crush test: a door id, the floor cell to start on, the
/// direction to step into the panel, the end panel itself, and the direction from
/// that panel to its adjacent hinge (what you bump to close).
fn crush_scenario(layout: &Layout) -> Option<(DoorId, Cell, Direction, Cell, Direction)> {
    for (id, door) in layout.regions().doors() {
        let panel = door.panels()[0];
        // The end panel abuts a hinge; the door line runs panel→hinge.
        let Some(&hinge) = door
            .hinges()
            .iter()
            .find(|&&h| panel.manhattan_distance(h) == 1)
        else {
            continue;
        };
        let Some(hinge_dir) = Direction::between(panel, hinge) else {
            continue;
        };
        // Approach the panel perpendicular to the door line, from floor.
        for perp in hinge_dir.perpendicular() {
            let Some(from) = panel.step(perp) else {
                continue;
            };
            let f = layout.facility();
            if f.terrain(from) == Some(Terrain::Floor) && f.can_enter(from, ACTOR_FILL) {
                return Some((id, from, perp.opposite(), panel, hinge_dir));
            }
        }
    }
    None
}

/// #148: bumping a *closed hinge* from beside the frame opens the door and turns
/// the player to face **along the door line, toward the panels**, so the #121
/// head-lean peek reads through the doorway from cover — you crack the door and
/// see the room beyond without ever stepping into the new sightline.
#[test]
fn a_frame_bump_opens_the_door_and_auto_faces_the_peek() {
    // region_strip: a vertical door in column 4 joins room A (cols 1–3) to
    // corridor C (cols 5–6); hinges at (4,1) and (4,3), panel at (4,2).
    let hinge = Cell::new(4, 1);
    let panel = Cell::new(4, 2);
    let mut s = State::new(
        region_strip(),
        Cell::new(3, 1), // beside the top hinge, in room A
        Direction::East, // arbitrary prior facing — the frame bump overrides it
        Vec::new(),
        Vec::new(),
        Cell::new(0, 0), // exit parked on the border corner, never touched
    );

    // With the door closed, the corridor beyond it is unseen.
    assert!(
        !s.player_fov().contains(Cell::new(5, 2)),
        "the closed door hides the corridor",
    );

    // The usable line predicts the frame open (§11.4): the closed hinge to the
    // east now offers `door: open`, in step with what the bump will do.
    assert!(
        s.affordances()
            .iter()
            .any(|&(dir, a)| dir == Direction::East && a == Affordance::OpenDoor),
        "a closed hinge offers door: open on the usable line",
    );

    // Bump the closed hinge to the east: the door opens, spending the turn.
    let events = s.step(Input::Step(Direction::East));
    assert_eq!(
        events,
        vec![Event::DoorOpened {
            at: hinge,
            by_player: true,
        }]
    );
    assert_eq!(s.turn(), 1, "opening spends the turn (§4.3)");
    assert_eq!(
        s.player(),
        Cell::new(3, 1),
        "the player did not move to open"
    );
    assert_eq!(
        s.layout().facility().terrain(panel),
        Some(Terrain::DoorPanelOpen),
        "every panel swung open",
    );

    // Facing turned along the door line, toward the panel (south).
    assert_eq!(
        s.facing(),
        Direction::South,
        "the frame bump faces the player along the door line, toward the panels",
    );

    // The recomputed FOV + #121 peek now leans through the doorway: the open
    // panel and a corridor cell on the far side are both seen (#121-style).
    assert!(s.player_fov().contains(panel), "the open doorway is seen");
    assert!(
        s.player_fov().contains(Cell::new(5, 2)),
        "the peek reads through the doorway into the corridor",
    );
}

// ---------------------------------------------------------------------------
// The withheld frame (#320): the hinge of the door you just opened slides you
// past it instead of shutting it again.
// ---------------------------------------------------------------------------

/// The #320 worked example as geometry: an east–west wall across the middle of a
/// box, holding one **manual** door — hinge `(3,3)`, panels `(4,3)`/`(5,3)`, hinge
/// `(6,3)` — with a room either side. The player starts in the south room at
/// `player`, facing north into the wall line.
///
/// ```text
///   col 0123456789
///        ..........   row 1   north room
///        ..........   row 2
///       #############
///        ##+++##      row 3   wall line: hinges at x=3,6, panels at x=4,5
///        ..........   row 4   south room — the player starts here
///        ..........   row 5
/// ```
///
/// Returns the state and the door's id.
fn frame_bump_rooms(player: Cell) -> (State, DoorId) {
    let mut f = Facility::walled_box(11, 7);
    let mut g = RegionGraph::new(11, 7);
    let row = |y0: u32, y1: u32| (y0..y1).flat_map(|y| (1..10).map(move |x| Cell::new(x, y)));
    let north = g.add_region(RegionKind::Room, row(1, 3));
    let south = g.add_region(RegionKind::Room, row(4, 6));
    for x in 1..10 {
        f.set_terrain(x, 3, Terrain::Wall);
    }
    for x in [3, 6] {
        f.set_terrain(x, 3, Terrain::DoorHinge);
    }
    for x in [4, 5] {
        f.set_terrain(x, 3, Terrain::DoorPanelClosed);
    }
    let door = g.add_door(
        north,
        south,
        [Cell::new(3, 3), Cell::new(6, 3)],
        [Cell::new(4, 3), Cell::new(5, 3)],
        DoorKind::Manual,
    );
    let s = State::new(
        Layout::from_parts(f, g),
        player,
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(9, 5), // exit parked in the south room's far corner, never touched
    );
    (s, door)
}

/// #320, the worked example: a player one cell off the panel line presses the **same
/// direction three times** and gets through — open, slide, walk — instead of opening
/// and immediately shutting the door they just opened. The middle press is the fix:
/// a frame bump on a door the previous action opened is a *dead bump* (§4.4), so the
/// #57 slide rounds the frame onto the open panel rather than the hinge closing it
/// (§10.4's "bump a hinge to close", with this one documented exception).
#[test]
fn a_frame_bump_on_the_door_you_just_opened_slides_past_it() {
    let hinge = Cell::new(3, 3);
    let panel = Cell::new(4, 3);
    let (mut s, door) = frame_bump_rooms(Cell::new(3, 4)); // south of the west hinge

    // Press 1 — the #148 frame bump: the door opens and the player turns to face
    // along the door line, toward the panels, without stepping into the sightline.
    let opened = s.step(Input::Step(Direction::North));
    assert_eq!(
        opened,
        vec![Event::DoorOpened {
            at: hinge,
            by_player: true,
        }],
    );
    assert!(s.layout().regions().door(door).is_open());
    assert_eq!(
        s.player(),
        Cell::new(3, 4),
        "the open did not move the player"
    );
    assert_eq!(
        s.facing(),
        Direction::East,
        "#148's peek along the door line"
    );

    // The §11.4 usable line goes quiet on the frame while the close is withheld: it
    // must never promise a *close door* the very next bump will not deliver.
    assert!(
        !s.affordances()
            .iter()
            .any(|&(_, a)| a == Affordance::CloseDoor),
        "the withheld frame offers no close",
    );

    // Press 2 — the same direction again. Today this shut the door; now it is a dead
    // bump offered to the #57 slide. Both laterals are open floor, so the
    // forward-diagonal breaks the tie: `(4,3)` is the open panel, `(2,3)` is wall, so
    // the player rounds the frame east onto the panel's column.
    let slid = s.step(Input::Step(Direction::North));
    assert_eq!(
        slid,
        vec![Event::Moved {
            to: Cell::new(4, 4)
        }],
        "the frame bump slid east round the frame instead of closing the door",
    );
    assert!(
        s.layout().regions().door(door).is_open(),
        "the door the player just opened is still open",
    );
    assert_eq!(
        s.turn(),
        2,
        "the slide is a move — it spends the turn (§4.4)"
    );

    // Press 3 — the same direction once more: straight through the open panel.
    let through = s.step(Input::Step(Direction::North));
    assert_eq!(through, vec![Event::Moved { to: panel }]);
    assert_eq!(
        s.player(),
        panel,
        "into the doorway, three presses, one door"
    );
}

/// #320/§10.4: the suppression is exactly **one action** wide. Whatever the player
/// does next — here a Wait — spends the mark, and the frame is the plain handle
/// again: the usable line offers the close and the bump delivers it.
#[test]
fn the_frame_close_returns_the_action_after_the_open() {
    let (mut s, door) = frame_bump_rooms(Cell::new(3, 4));

    s.step(Input::Step(Direction::North)); // the frame bump: opens, marks the door
    assert_eq!(s.door_just_opened, Some(door), "the open marked its door");

    s.step(Input::Wait); // any action at all spends the mark
    assert_eq!(s.door_just_opened, None, "one action wide, no more");
    assert!(
        s.affordances()
            .contains(&(Direction::North, Affordance::CloseDoor)),
        "the frame offers the close again",
    );

    let turn_before = s.turn();
    s.step(Input::Step(Direction::North));
    assert!(
        !s.layout().regions().door(door).is_open(),
        "shutting a door behind you is still a real move (§10.4)",
    );
    assert_eq!(
        s.turn(),
        turn_before + 1,
        "the close spends the turn (§4.3)"
    );
    assert_eq!(
        s.player(),
        Cell::new(3, 4),
        "the close did not move the player"
    );
}

/// #320/§10.4: a door that was **already open** when the player walked up to it —
/// nothing to do with their last action — closes on the first frame bump, exactly as
/// it always has. Only the door you opened *this instant* withholds its close.
#[test]
fn a_door_already_open_closes_on_the_first_frame_bump() {
    let (mut s, door) = frame_bump_rooms(Cell::new(3, 4));
    // Open it out of band — the world's doing, not the player's action.
    s.layout.bump_door(Cell::new(4, 3), |_| false);
    assert!(s.layout().regions().door(door).is_open());
    assert_eq!(s.door_just_opened, None, "no player open, no mark");

    let turn_before = s.turn();
    s.step(Input::Step(Direction::North));
    assert!(
        !s.layout().regions().door(door).is_open(),
        "an open door the player did not just open shuts on the frame bump",
    );
    assert_eq!(s.turn(), turn_before + 1, "the close spends the turn");
}

/// #320: a **refused** slide must not shut the door in the same breath — that is the
/// bug, not a fallback. With both laterals walled off, the #57 slide declines, so the
/// frame bump falls through to the free §4.4 no-op: nothing moves, no turn is spent,
/// and the door stays open. The mark is spent all the same, so the player who
/// genuinely wants it shut simply presses again.
#[test]
fn a_refused_slide_leaves_the_just_opened_door_open() {
    let hinge = Cell::new(3, 3);
    let (mut s, door) = frame_bump_rooms(Cell::new(3, 4));
    s.layout.place(Cell::new(2, 4), Terrain::Wall); // west lateral blocked
    s.layout.place(Cell::new(4, 4), Terrain::Wall); // east lateral blocked

    s.step(Input::Step(Direction::North)); // opens the door, marks it
    let turn_before = s.turn();

    let refused = s.step(Input::Step(Direction::North));
    assert_eq!(
        refused,
        vec![Event::Bumped { into: hinge }],
        "boxed in either side: the slide declines and the bump is free",
    );
    assert_eq!(s.player(), Cell::new(3, 4), "nothing moved");
    assert_eq!(s.turn(), turn_before, "a refused slide stays free (§4.4)");
    assert!(
        s.layout().regions().door(door).is_open(),
        "the refusal did not undo the open",
    );

    // The mark went with the action, so the close is right there on the next press.
    s.step(Input::Step(Direction::North));
    assert!(
        !s.layout().regions().door(door).is_open(),
        "a player who genuinely wants it shut simply presses again",
    );
}

/// #320/§2.2/§4.5: the fairness gate matters most here, because the slide is now
/// reachable from a doorway — a place guards walk through. A guard orthogonally
/// adjacent to the slide's destination refuses it (it could step in and capture next
/// phase), and the refusal is the free no-op, never a close.
#[test]
fn the_doorway_slide_is_refused_next_to_a_guard_that_could_capture() {
    let hinge = Cell::new(3, 3);
    let dest = Cell::new(4, 4);
    let (mut s, door) = frame_bump_rooms(Cell::new(3, 4));
    s.layout.place(Cell::new(2, 4), Terrain::Wall); // west lateral blocked: east or nothing
    s.guards.push(Guard::stationary(Cell::new(4, 5))); // faces south; dest is behind it

    assert_eq!(
        s.guards()[0].pos().manhattan_distance(dest),
        1,
        "precondition: a guard could step into the destination next phase",
    );
    s.step(Input::Step(Direction::North)); // opens the door, marks it
    let turn_before = s.turn();

    let refused = s.step(Input::Step(Direction::North));
    assert_eq!(refused, vec![Event::Bumped { into: hinge }]);
    assert_eq!(s.player(), Cell::new(3, 4), "never slid into reach");
    assert_eq!(s.turn(), turn_before, "a refused slide stays free");
    assert!(s.layout().regions().door(door).is_open());
    assert_eq!(s.outcome(), Outcome::Playing);
}

/// #320: the mark is the **player's** alone. A door a guard walks open (§10.4) is the
/// world's doing, so it sets nothing — the player's next frame bump on it closes it
/// like any other open door.
#[test]
fn a_door_a_guard_opened_never_marks_the_players_frame() {
    let layout = region_strip();
    let panel = Cell::new(4, 2);
    let door = layout.regions().door_at(panel).unwrap();
    let mut s = State::new(
        layout,
        Cell::new(13, 4), // parked in corridor D, two closed doors away
        Direction::North,
        vec![Guard::patrolling_to(Cell::new(2, 2), Cell::new(6, 2))],
        Vec::new(),
        Cell::new(13, 1),
    );
    s.set_guard_close_chance(0); // isolate the guard's open from the close-behind

    // Drive the loop until the patrolling guard walks the closed panel open (#146).
    for _ in 0..8 {
        let e = s.step(Input::Wait);
        assert_eq!(
            s.door_just_opened, None,
            "no player action opened a frame this turn",
        );
        if e.iter().any(|ev| {
            matches!(
                ev,
                Event::DoorOpened {
                    by_player: false,
                    ..
                }
            )
        }) {
            assert!(s.layout().regions().door(door).is_open());
            return;
        }
    }
    panic!("the patrolling guard never opened the door");
}

/// #320/§10.4/#147: a **frameless automatic** door has no hinges, so there is no
/// frame bump on it to withhold — the fix cannot reach it. Opening one marks
/// nothing, and none of its cells is a hinge.
#[test]
fn an_automatic_door_has_no_frame_to_withhold() {
    let (mut s, door) = auto_door_state(3);

    s.step(Input::Step(Direction::East)); // the panel bump opens it
    assert!(s.layout().regions().door(door).is_open());
    assert_eq!(
        s.door_just_opened, None,
        "a panel open marks nothing — there is no frame to catch on",
    );
    let cells: Vec<Cell> = s.layout().regions().door(door).cells().collect();
    for cell in cells {
        assert_eq!(
            s.hinge_door_at(cell),
            None,
            "an automatic door is all panels (#147)",
        );
        assert!(!s.frame_bump_withheld(cell));
    }
}

/// Determinism (§12.4): the mark is derived purely from the input stream, so the same
/// fixture and inputs reproduce identical events and frame — open, slide, walk through.
#[test]
fn the_withheld_frame_flow_is_deterministic() {
    let script = [
        Input::Step(Direction::North),
        Input::Step(Direction::North),
        Input::Step(Direction::North),
        Input::Wait,
    ];
    let run = || {
        let (mut s, _) = frame_bump_rooms(Cell::new(3, 4));
        let events: Vec<Vec<Event>> = script.iter().map(|&i| s.step(i)).collect();
        (events, crate::render(&s))
    };
    let (events_a, frame_a) = run();
    let (events_b, frame_b) = run();
    assert_eq!(events_a, events_b, "same inputs → same events");
    assert_eq!(frame_a, frame_b, "same inputs → same frame");
}

/// Door affordances speak the §10.4 door graph: a closed panel offers the
/// open; an open hinge offers the close — except while an actor stands on a
/// panel, when the close would be refused (doors never crush) and so is
/// never offered.
#[test]
fn door_affordances_track_pose_and_obstruction() {
    for seed in 0..64 {
        let layout = generate(40, 40, &mut Rng::new(seed)).unwrap();
        let Some((_, from, into, panel, hinge_dir)) = crush_scenario(&layout) else {
            continue;
        };
        // The hinge's floor neighbour on the player's side of the wall: the
        // cell to close the door from. `side` steps off the door line back
        // toward `from`'s side.
        let hinge = panel.step(hinge_dir).expect("hinge adjacent to panel");
        let side = Direction::between(panel, from).expect("from is beside the panel");
        let Some(beside_hinge) = hinge.step(side) else {
            continue;
        };
        let f = layout.facility();
        if f.terrain(beside_hinge) != Some(Terrain::Floor) || !f.can_enter(beside_hinge, ACTOR_FILL)
        {
            continue;
        }

        let mut s = State::new(
            layout,
            from,
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(0, 0), // border corner: never walked, never bumped
        );
        let offers = |s: &State, want: Affordance| s.affordances().iter().any(|&(_, a)| a == want);
        assert!(
            offers(&s, Affordance::OpenDoor),
            "seed {seed}: a closed panel offers the open"
        );
        assert!(!offers(&s, Affordance::CloseDoor));

        // Open it, then stand on the panel: the close would be refused, so
        // the hinge offers nothing.
        s.step(Input::Step(into));
        s.step(Input::Step(into));
        assert_eq!(s.player(), panel);
        assert!(
            !offers(&s, Affordance::CloseDoor),
            "seed {seed}: no close offered while standing on the panel"
        );

        // Step back off the panel, then along the wall to sit beside the
        // hinge: now the close is a real offer.
        s.step(Input::Step(side));
        s.step(Input::Step(hinge_dir));
        assert_eq!(s.player(), beside_hinge);
        assert!(
            offers(&s, Affordance::CloseDoor),
            "seed {seed}: an open hinge offers the close"
        );
        assert!(!offers(&s, Affordance::OpenDoor));
        return;
    }
    panic!("no usable door scenario found in 64 seeds");
}

/// A hand-built wide strip: a left room and a right room joined by one **manual**
/// door at column 6 (hinges at `(6,1)`/`(6,3)`, panel at `(6,2)`), with a guard
/// patrolling from the left room through the door — so on its beat it walks the
/// closed panel open (§10.4), a change the player did not cause. The player starts at
/// `player` **facing east**, ahead of the door, and the drive below walks it further
/// east each turn: the guard opens the door *behind* the eastward-facing player, so
/// the changed cell is reliably out of the forward FOV (a Wait's 360° look would
/// otherwise see straight through the open doorway — sight and door-sense share the
/// same range, §9.1/§10.4). The close-behind is disabled so the open is isolated.
/// Returns the state and the door's panel cell.
fn guard_door_strip(width: u32, player: Cell) -> (State, Cell) {
    let mut f = Facility::walled_box(width, 6);
    let mut g = RegionGraph::new(width, 6);
    let column =
        |x0: u32, x1: u32| (1..5).flat_map(move |y| (x0..x1).map(move |x| Cell::new(x, y)));
    let left = g.add_region(RegionKind::Room, column(1, 6));
    let right = g.add_region(RegionKind::Room, column(7, width - 1));
    for y in 1..5 {
        f.set_terrain(6, y, Terrain::Wall);
    }
    f.set_terrain(6, 1, Terrain::DoorHinge);
    f.set_terrain(6, 2, Terrain::DoorPanelClosed);
    f.set_terrain(6, 3, Terrain::DoorHinge);
    g.add_door(
        left,
        right,
        [Cell::new(6, 1), Cell::new(6, 3)],
        [Cell::new(6, 2)],
        DoorKind::Manual,
    );
    let mut s = State::new(
        Layout::from_parts(f, g),
        player,
        Direction::East,
        vec![Guard::patrolling_to(Cell::new(4, 2), Cell::new(8, 2))],
        Vec::new(),
        Cell::new(width - 2, 4),
    );
    s.set_guard_close_chance(0); // isolate the open from the close-behind (#146)
    (s, Cell::new(6, 2))
}

/// Walk the player east until the patrolling guard opens the door behind them (the
/// first `by_player: false` open), returning once it has. Stepping east keeps the
/// player facing *away* from the door so the changed cell stays out of the forward
/// FOV. Panics if the guard never opens it.
fn drive_until_guard_opens(s: &mut State) {
    for _ in 0..8 {
        let e = s.step(Input::Step(Direction::East));
        if e.iter().any(|ev| {
            matches!(
                ev,
                Event::DoorOpened {
                    by_player: false,
                    ..
                }
            )
        }) {
            return;
        }
    }
    panic!("the patrolling guard never opened the door");
}

/// §9.4/§10.4 **[START]**: a door change is a louder, coarser event than a guard's
/// exact position, so it is felt **farther** — `DOOR_SENSE_RANGE > PLAYER_SENSE_RANGE`
/// — and both it and the cue decay are pinned so a later change is a visible edit.
#[test]
fn the_door_sense_range_exceeds_the_guard_sense() {
    assert_eq!(DOOR_SENSE_RANGE, 15, "the [START] door-sense range");
    assert_eq!(DOOR_CUE_DECAY_TURNS, 3, "the [START] cue decay");
    // The `DOOR_SENSE_RANGE > PLAYER_SENSE_RANGE` asymmetry itself is pinned at compile
    // time beside the constant (§9.4/§10.4), so it cannot silently invert.
}

/// §9.4/§10.4: unlike the guard sense, the door sense is **not** widened by a Wait —
/// a door change is already loud enough — but it **shrinks inside a duct** with the
/// rest of the crawlspace's degraded perception (§10.7).
#[test]
fn the_door_sense_is_not_widened_by_wait_but_shrinks_in_a_duct() {
    let mut s = solo(Cell::new(5, 5));
    assert_eq!(
        s.door_sense_range(),
        DOOR_SENSE_RANGE,
        "the base door-sense box"
    );
    s.step(Input::Wait);
    assert_eq!(
        s.door_sense_range(),
        DOOR_SENSE_RANGE,
        "a wait does not widen the door sense",
    );

    let mut d = duct_world();
    d.step(Input::Step(Direction::North)); // climb into the duct
    assert!(d.in_duct());
    assert_eq!(
        d.door_sense_range(),
        DUCT_SENSE_RANGE,
        "the door sense shrinks inside a duct, like the guard sense",
    );
}

/// §9.4/§10.4: a door a **guard** opens out of the player's FOV, within
/// `DOOR_SENSE_RANGE`, lights the door's **whole footprint** as a `Category::Sensed`
/// background — the same "sensed through a wall" channel as a guard, readable around
/// the corner — and the generic "the door opens" near-line self-narration is gone for
/// a door the player did not operate.
#[test]
fn a_guard_door_lights_a_cue_out_of_fov() {
    let (mut s, panel) = guard_door_strip(30, Cell::new(7, 2));
    drive_until_guard_opens(&mut s);

    // A door the player did not operate raises no near-line word — the grid cue is
    // the durable evidence instead (§11.7).
    assert_ne!(
        crate::status::near_line(&s).text,
        "the door opens",
        "a guard-opened door no longer narrates on the near line",
    );

    // The eastward-facing player has the door behind them: out of the forward FOV,
    // yet the cue is on the grid.
    assert!(
        !s.player_fov().contains(panel),
        "precondition: the changed door is out of the player's forward FOV",
    );
    assert!(
        s.door_cues().any(|c| c == panel),
        "a guard-opened door lights a cue read out of FOV",
    );

    // The cue lights the door's *whole* footprint — both hinges and the panel — in
    // the Sensed channel, not just the single panel the event named (§9.4).
    let door_cells: Vec<Cell> = {
        let regions = s.layout().regions();
        let id = regions.door_at(panel).expect("the panel belongs to a door");
        regions.door(id).cells().collect()
    };
    assert!(
        door_cells.len() >= 3,
        "the strip door is two hinges and a panel",
    );
    let g = crate::render::render(&s);
    for cell in door_cells {
        assert_eq!(
            g.get(cell.x, cell.y).bg,
            Some(crate::category::Category::Sensed),
            "the whole door is lit in the sensed channel, cell {cell:?}",
        );
    }
}

/// §9.4/§10.4: the cue is a **fading** mark — a door change is a discrete event, not
/// a standing position — so it stays lit for `DOOR_CUE_DECAY_TURNS` turns and is then
/// gone. Checked on `door_cues` directly, independent of FOV.
#[test]
fn a_door_cue_fades_over_its_decay() {
    let (mut s, panel) = guard_door_strip(30, Cell::new(7, 2));
    drive_until_guard_opens(&mut s);
    assert!(
        s.door_cues().any(|c| c == panel),
        "the cue is lit the turn the door changes",
    );

    // No further door events fire (one door, close disabled), so the single cue decays
    // cleanly: lit for DOOR_CUE_DECAY_TURNS turns, then gone.
    for n in 1..DOOR_CUE_DECAY_TURNS {
        s.step(Input::Step(Direction::East));
        assert!(
            s.door_cues().any(|c| c == panel),
            "the cue is still lit {n} turn(s) on",
        );
    }
    s.step(Input::Step(Direction::East));
    assert!(
        !s.door_cues().any(|c| c == panel),
        "the cue has faded after DOOR_CUE_DECAY_TURNS turns",
    );
}

/// §9.4/§10.4: a door change **beyond** `DOOR_SENSE_RANGE` is felt as nothing — the
/// same guard-opened door, but with the player parked out past the door-sense box.
#[test]
fn a_door_change_beyond_range_lights_no_cue() {
    let (mut s, panel) = guard_door_strip(30, Cell::new(24, 4));
    assert!(
        s.player().sight_distance(panel) > DOOR_SENSE_RANGE,
        "precondition: the player is beyond the door-sense box",
    );
    drive_until_guard_opens(&mut s);
    assert_eq!(
        s.door_cues().count(),
        0,
        "a change beyond DOOR_SENSE_RANGE lights no cue",
    );
}

/// §9.4: the door cue and a guard felt through a wall share **one sense channel** —
/// the same orange `Category::Sensed` background — so they read as one thing, not two
/// colours to tell apart. The guard opens the door, then steps onto the open panel:
/// the panel (guard-sensed) and a hinge (door-cued only) both render `Sensed`.
#[test]
fn the_door_cue_and_the_guard_sense_share_one_channel() {
    let (mut s, panel) = guard_door_strip(30, Cell::new(7, 2));
    drive_until_guard_opens(&mut s);
    let hinge = Cell::new(panel.x, panel.y - 1); // a frame cell of the same door
    s.step(Input::Step(Direction::East)); // the guard steps onto the open panel

    assert_eq!(
        s.guards()[0].pos(),
        panel,
        "the guard steps through onto the open panel",
    );
    assert_eq!(
        s.perceive_guard(&s.guards()[0]),
        Some(GuardPerception::Sensed),
        "the guard on the panel is sensed through the wall, not seen",
    );
    assert!(
        s.door_cues().any(|c| c == hinge),
        "the door cue lights the frame cell too (whole door)",
    );

    let g = crate::render::render(&s);
    let sensed = Some(crate::category::Category::Sensed);
    assert_eq!(
        g.get(panel.x, panel.y).bg,
        sensed,
        "the guard-sensed panel reads Sensed",
    );
    assert_eq!(
        g.get(hinge.x, hinge.y).bg,
        sensed,
        "the door-cued frame reads the *same* Sensed channel — one colour, not two",
    );
}

/// §11.7/§10.4: a door **you** operate keeps its quiet near-line self-narration and
/// lights **no** on-grid cue — the cue is only for doors the player did not operate,
/// which is where the durable "someone passed" evidence belongs.
#[test]
fn a_door_you_open_keeps_its_word_and_lights_no_cue() {
    // Player in room A of the strip, next to the closed door; bump it open.
    let mut s = State::new(
        region_strip(),
        Cell::new(3, 2),
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(13, 1),
    );
    let events = s.step(Input::Step(Direction::East)); // bump the panel at (4,2)
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DoorOpened {
                by_player: true,
                ..
            }
        )),
        "the player's bump opens the door",
    );
    assert_eq!(
        crate::status::near_line(&s).text,
        "the door opens",
        "a door you operate keeps its quiet self-narration (§11.7)",
    );
    assert_eq!(
        s.door_cues().count(),
        0,
        "a door you operate lights no on-grid cue",
    );
}

// --- Ducts (§10.7) ------------------------------------------------------------

/// A hand-built strip: a left room (cols 1–5) and a right room (cols 7…) joined by
/// one **manual** door at column 6 — hinges at `(6,1)`/`(6,3)`, a single panel at
/// `(6,2)` — with the player starting `player`, **facing east**, ahead of the door
/// and holding the full loadout (Autodoors included). No guards, so the flight
/// mechanics read clean. Returns the state and the door's panel cell.
fn autodoor_strip(width: u32, player: Cell) -> (State, Cell) {
    let mut f = Facility::walled_box(width, 6);
    let mut g = RegionGraph::new(width, 6);
    let column =
        |x0: u32, x1: u32| (1..5).flat_map(move |y| (x0..x1).map(move |x| Cell::new(x, y)));
    let left = g.add_region(RegionKind::Room, column(1, 6));
    let right = g.add_region(RegionKind::Room, column(7, width - 1));
    for y in 1..5 {
        f.set_terrain(6, y, Terrain::Wall);
    }
    f.set_terrain(6, 1, Terrain::DoorHinge);
    f.set_terrain(6, 2, Terrain::DoorPanelClosed);
    f.set_terrain(6, 3, Terrain::DoorHinge);
    g.add_door(
        left,
        right,
        [Cell::new(6, 1), Cell::new(6, 3)],
        [Cell::new(6, 2)],
        DoorKind::Manual,
    );
    // These are the Autodoors tests, so the run holds Autodoors — and only it, on
    // top of the innate set (§8.3/#244): a loadout is built up from what the test
    // needs, never inherited wholesale.
    let s = State::new(
        Layout::from_parts(f, g),
        player,
        Direction::East,
        Vec::new(),
        Vec::new(),
        Cell::new(width - 2, 4),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Autodoors));
    (s, Cell::new(6, 2))
}

/// The core edge (§8.3/§7.6, AC #241): while Autodoors is active a closed door in
/// the path **opens as the player steps into it** — no manual bump — and the same
/// step carries them onto the panel in one turn, then the door **shuts behind** them
/// the turn they clear the throat. Both changes are the player's own
/// (`by_player: true`), so they self-narrate and light no sensed cue (§9.4/§11.7).
#[test]
fn autodoors_opens_ahead_and_shuts_behind() {
    let (mut s, panel) = autodoor_strip(12, Cell::new(5, 2));
    let door = s
        .layout()
        .regions()
        .door_at(panel)
        .expect("the strip's door");

    // Turn 1: switch the ability on (a spent turn; the player does not move).
    s.step(Input::Activate(AbilityId::Autodoors));
    assert!(matches!(
        s.ability_state(AbilityId::Autodoors),
        AbilityState::Active { .. }
    ));
    assert!(!s.layout().regions().door(door).is_open(), "still closed");

    // Turn 2: step east into the closed panel. It opens *and* the player walks
    // through in the one turn — the open-turn the manual bump would cost is saved.
    let e = s.step(Input::Step(Direction::East));
    assert_eq!(
        e,
        vec![
            Event::DoorOpened {
                at: panel,
                by_player: true,
            },
            Event::Moved { to: panel },
        ],
        "one step both opens the door and moves onto the panel",
    );
    assert_eq!(s.player(), panel, "the player stands in the throat");
    assert!(s.layout().regions().door(door).is_open(), "opened ahead");
    assert!(
        s.autodoors_pending.contains(&door),
        "the door is armed to shut behind",
    );

    // Turn 3: step clear of the throat. Once vacated, the door swings shut behind —
    // the crush-safe §10.4 close, reported as the player's own change.
    let e = s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(7, 2), "through to the far side");
    assert_eq!(
        e,
        vec![
            Event::Moved {
                to: Cell::new(7, 2)
            },
            Event::DoorClosed {
                at: panel,
                by_player: true,
            },
        ],
        "clearing the throat shuts the door behind",
    );
    assert!(!s.layout().regions().door(door).is_open(), "shut behind");
    assert_eq!(
        s.layout().facility().terrain_at(panel.x, panel.y),
        Some(Terrain::DoorPanelClosed),
        "the panel restamps solid, exactly as any close leaves it",
    );
    assert!(
        s.autodoors_pending.is_empty(),
        "a shut door is no longer owed a close",
    );
}

/// AC #241, the §7.6 payoff: a door shut behind the fleeing player **breaks a
/// pursuer's line of sight** (§10.3). Verified from a west vantage looking east:
/// through the open doorway the far cell is visible; once the door shuts behind the
/// player, the closed panel occludes it.
#[test]
fn autodoors_breaks_a_pursuers_sightline() {
    let (mut s, panel) = autodoor_strip(12, Cell::new(5, 2));
    let vantage = Cell::new(2, 2); // a pursuer back in the left room
    let far = Cell::new(7, 2); // where the player flees to, beyond the door

    s.step(Input::Activate(AbilityId::Autodoors));
    s.step(Input::Step(Direction::East)); // onto the panel; the door is open

    let through_open = field_of_view(
        s.layout().facility(),
        vantage,
        Direction::East,
        crate::vision::FULL_SIGHT_ARC,
        20,
    );
    assert!(
        through_open.contains(far),
        "the open doorway lets the line reach the far cell",
    );

    s.step(Input::Step(Direction::East)); // clear the throat → the door shuts

    let through_shut = field_of_view(
        s.layout().facility(),
        vantage,
        Direction::East,
        crate::vision::FULL_SIGHT_ARC,
        20,
    );
    assert!(
        !through_shut.contains(far),
        "the shut panel breaks the pursuer's line (§10.3)",
    );
    // The vantage still sees its own side of the now-closed door — the break is at
    // the panel, not a blanked view.
    assert!(through_shut.contains(panel.step(Direction::West).unwrap()));
}

/// AC #241 (never crushing, §10.4): an armed door **waits for a dragged body** to
/// clear the throat before it shuts. A body on the panel is an occupant to the
/// crush rule (§7.2), so the auto-close refuses until it is gone — the door does not
/// clip the body the player is hauling through.
#[test]
fn an_armed_autodoor_never_shuts_on_a_body_in_the_throat() {
    let (mut s, panel) = autodoor_strip(12, Cell::new(5, 2));
    let door = s
        .layout()
        .regions()
        .door_at(panel)
        .expect("the strip's door");

    s.step(Input::Activate(AbilityId::Autodoors));
    s.step(Input::Step(Direction::East)); // onto the panel; the door is open + armed
    assert!(s.layout().regions().door(door).is_open());

    // The player is hauling a body: place it under them in the throat and take hold,
    // so the next step hauls it into the panel behind them as they clear it. Set up
    // directly to isolate the crush interaction from the full takedown-and-grab dance
    // (`haul_body_to` threads the haul itself).
    s.bodies.push(crate::body::Body::new(
        panel,
        crate::radio::RadioClock::from_period(4),
        s.turn(),
    ));
    s.dragging = Some(0);
    s.drag_debt = false;

    let e = s.step(Input::Step(Direction::East)); // player clears; the body follows in
    assert_eq!(s.player(), Cell::new(7, 2));
    assert_eq!(
        s.bodies[0].cell(),
        panel,
        "the hauled body is now in the throat"
    );
    assert!(
        !e.iter().any(|ev| matches!(ev, Event::DoorClosed { .. })),
        "the door does not shut on the body",
    );
    assert!(
        s.layout().regions().door(door).is_open(),
        "the body in the throat holds the door open (§10.4 never crushes)",
    );
    assert!(s.autodoors_pending.contains(&door), "still owed a close");

    // The body hauled clear of the throat, the next world turn shuts the door.
    s.bodies[0].move_to(Cell::new(3, 2));
    let e = s.step(Input::Wait);
    assert!(
        e.contains(&Event::DoorClosed {
            at: panel,
            by_player: true,
        }),
        "the throat clear, the armed door finally shuts",
    );
    assert!(!s.layout().regions().door(door).is_open());
}

/// The gate (§8.3): with Autodoors **inactive**, a closed door is the ordinary
/// bump-to-open (§10.4/#148) — a spent turn that opens the door but does *not* move
/// the player through, and arms no close-behind. The edge is the ability's alone.
#[test]
fn without_the_ability_a_closed_door_is_a_plain_bump_open() {
    let (mut s, panel) = autodoor_strip(12, Cell::new(5, 2));
    let door = s
        .layout()
        .regions()
        .door_at(panel)
        .expect("the strip's door");

    let e = s.step(Input::Step(Direction::East));
    assert_eq!(
        e,
        vec![Event::DoorOpened {
            at: panel,
            by_player: true,
        }],
        "a plain bump opens the door and nothing more",
    );
    assert_eq!(
        s.player(),
        Cell::new(5, 2),
        "the bump does not move the player"
    );
    assert!(s.layout().regions().door(door).is_open());
    assert!(
        s.autodoors_pending.is_empty(),
        "no close is armed without the ability",
    );

    // And with the door left open, it never auto-shuts — a Wait passes and it stays.
    s.step(Input::Wait);
    assert!(
        s.layout().regions().door(door).is_open(),
        "a manually opened door does not close itself",
    );
}

/// Autodoors offers only a **panel** (§8.3): a closed **hinge** opens the door too
/// (#148), but the hinge cell stays solid, so there is nothing to step onto — the
/// bump is the ordinary door op (open in place), not a walk-through, and arms no
/// close.
#[test]
fn autodoors_leaves_a_closed_hinge_a_plain_door_op() {
    // Player west of the lower hinge (6,3), facing east into it (the fixture facing).
    let (mut s, _) = autodoor_strip(12, Cell::new(5, 3));
    let hinge = Cell::new(6, 3);
    let door = s
        .layout()
        .regions()
        .door_at(hinge)
        .expect("the strip's door");

    s.step(Input::Activate(AbilityId::Autodoors));
    let e = s.step(Input::Step(Direction::East));
    assert!(
        e.iter().any(|ev| matches!(
            ev,
            Event::DoorOpened {
                at,
                by_player: true,
            } if *at == hinge
        )),
        "the hinge opens the door",
    );
    assert_eq!(
        s.player(),
        Cell::new(5, 3),
        "a hinge is solid — the player does not walk onto it",
    );
    assert!(s.layout().regions().door(door).is_open());
    assert!(
        s.autodoors_pending.is_empty(),
        "the hinge path arms no auto-close — only the walk-through panel does",
    );
}

/// Once armed, a door owes its close-behind regardless of the ability's live state:
/// toggling Autodoors **off** is free (§4.4) and does not cancel the pending close —
/// a door opened while active still shuts as the player steps clear.
#[test]
fn an_armed_autodoor_shuts_even_after_the_ability_is_toggled_off() {
    let (mut s, panel) = autodoor_strip(12, Cell::new(5, 2));
    let door = s
        .layout()
        .regions()
        .door_at(panel)
        .expect("the strip's door");

    s.step(Input::Activate(AbilityId::Autodoors));
    s.step(Input::Step(Direction::East)); // onto the panel; armed
    let turn_before = s.turn();

    // Toggle off — a free action (§4.4): the turn does not advance.
    let e = s.step(Input::Deactivate(AbilityId::Autodoors));
    assert_eq!(
        e,
        vec![Event::AbilityDeactivated {
            ability: AbilityId::Autodoors,
        }],
    );
    assert_eq!(s.turn(), turn_before, "toggling off is free");
    assert!(
        s.autodoors_pending.contains(&door),
        "the close is still owed"
    );

    // Stepping clear still shuts the door the ability opened.
    let e = s.step(Input::Step(Direction::East));
    assert!(
        e.contains(&Event::DoorClosed {
            at: panel,
            by_player: true,
        }),
        "the armed door shuts behind even with the ability off",
    );
    assert!(!s.layout().regions().door(door).is_open());
}

/// Determinism (§12.4, AC #241): the whole open-ahead/shut-behind flow draws no
/// randomness, so the same fixture and inputs reproduce identical events and frame.
#[test]
fn the_autodoors_flow_is_deterministic() {
    let script = [
        Input::Activate(AbilityId::Autodoors),
        Input::Step(Direction::East),
        Input::Step(Direction::East),
        Input::Step(Direction::East),
    ];
    let run = || {
        let (mut s, _) = autodoor_strip(12, Cell::new(5, 2));
        let events: Vec<Vec<Event>> = script.iter().map(|&i| s.step(i)).collect();
        (events, crate::render(&s))
    };
    let (events_a, frame_a) = run();
    let (events_b, frame_b) = run();
    assert_eq!(events_a, events_b, "same inputs → same events");
    assert_eq!(frame_a, frame_b, "same inputs → same frame");
}

/// The flight edge on **automatic** doors (§7.6, #241): a door opened via Autodoors
/// is shut *promptly* behind the player — the turn they clear the throat — rather
/// than lingering open for its full `delay` (§10.4/#147). Here the delay is a long
/// 5, so without the ability the door would idle open for several turns; the ability
/// makes the break immediate, exactly as it does for a manual door.
#[test]
fn autodoors_shuts_an_automatic_door_promptly_behind() {
    let (mut s, door) = auto_door_state(5); // a deliberately slow self-close timer
    let panel = Cell::new(3, 2);

    s.step(Input::Activate(AbilityId::Autodoors));

    // Step through: the automatic door opens ahead and the player walks onto it in
    // the one turn, exactly as a manual door does.
    s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), panel, "walked into the doorway in one turn");
    assert!(s.layout().regions().door(door).is_open());
    assert!(
        s.autodoors_pending.contains(&door),
        "the automatic door is armed too"
    );

    // Clear the throat: the ability shuts it at once — its delay-5 timer has barely
    // begun, so this is far sooner than #147 alone would.
    let e = s.step(Input::Step(Direction::East));
    assert_eq!(s.player(), Cell::new(4, 2), "through to the far room");
    assert!(
        e.iter().any(|ev| matches!(
            ev,
            Event::DoorClosed {
                by_player: true,
                ..
            }
        )),
        "the automatic door shuts behind at once, not on its slow timer",
    );
    assert!(
        !s.layout().regions().door(door).is_open(),
        "shut promptly, not left to idle open",
    );
}

// ---------------------------------------------------------------------------
// Confusion (§8.3/§9/#240): blind and freeze guards in a radius, through walls.
// ---------------------------------------------------------------------------
