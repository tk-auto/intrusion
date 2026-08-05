//! The locked prize room and the key that opens it, through the turn loop
//! (§10.4/§7.2/#236).
//!
//! Four things have to hold at once for this modifier to be the rule the design asked
//! for rather than a wall with a caption on it, and each has a section here:
//!
//! - **The lock refuses the player**, freely, and the usable line says *locked* rather
//!   than going quiet on a doorway that is plainly a doorway (§11.4/§2.3).
//! - **A takedown is the price, and any takedown pays it** (§7.2): every guard carries
//!   a key, it goes straight to hand, and from then on every keyed door in the building
//!   opens on the ordinary bump (§4.3).
//! - **Guards walk through it**, because they hold keys too — which is what leaves the
//!   frameless door standing open for a few turns and gives the player the one way in
//!   that costs no takedown (§10.4/#147).
//! - **It never seals anyone in.** From inside the room the door always opens, key or
//!   no key, so the slip-in is a gamble and not a trap (§2.2/§7.2's soft-lock class).

use crate::region::DoorKind;
use crate::state::*;
use crate::test_support::region_strip;
use crate::{render, DoorId};

/// The [`region_strip`] fixture with **room B locked** (§10.4/#236): its two doorways —
/// wall columns 7 and 11 — are key-gated and frameless, so the only ways into the room
/// holding the strip's prize refuse a bump until a guard has been taken down.
///
/// The player starts wherever `player` says and the guards are posted as given, exactly
/// as the lockdown fixture next door does. The modifier is switched on so the takedown
/// actually yields a key: the lock is a fact about the doors, but *whose belt the key is
/// on* is a fact about the run (§12.6).
fn locked_room(player: Cell, guards: Vec<Guard>) -> State {
    locked_room_on(locked_strip(), player, guards)
}

/// [`region_strip`] with room B's two doorways key-gated (§10.4/#236) — the fixture's
/// layout on its own, for the one test that has to stamp a cupboard on it first.
fn locked_strip() -> Layout {
    let mut layout = region_strip();
    for x in [7, 11] {
        let door = layout
            .regions()
            .door_at(Cell::new(x, 2))
            .expect("the strip has a door in this column");
        layout.key_gate_door(door, 5);
    }
    layout
}

/// [`locked_room`] over a layout the caller has already prepared.
fn locked_room_on(layout: Layout, player: Cell, guards: Vec<Guard>) -> State {
    State::new(
        layout,
        player,
        Direction::East,
        guards,
        Vec::new(),
        Cell::new(2, 4),
    )
    .with_modifiers(LevelModifiers {
        prize_room_locked: true,
        ..LevelModifiers::default()
    })
}

/// The door whose panels fill wall column `x`.
fn door_in_column(s: &State, x: u32) -> DoorId {
    s.layout()
        .regions()
        .door_at(Cell::new(x, 2))
        .expect("the strip has a door in this column")
}

/// Whether the door in column `x` stands open.
fn open(s: &State, x: u32) -> bool {
    s.layout().regions().door(door_in_column(s, x)).is_open()
}

/// What the usable line offers for the bump in `dir` (§11.4), if anything.
fn offered(s: &State, dir: Direction) -> Option<Affordance> {
    s.affordances()
        .into_iter()
        .find_map(|(d, a)| (d == Some(dir)).then_some(a))
}

// ---------------------------------------------------------------------------
// The lock
// ---------------------------------------------------------------------------

/// §10.4/#236: the key gate makes a doorway that will not open. The bump is **free** —
/// nothing moved, so nothing is charged (§4.4) — and the usable line says so *before*
/// the player presses, which is the whole reason the refusal is a kind of its own
/// rather than the silence a wall gets (§11.4/§2.3).
#[test]
fn a_keyed_door_refuses_the_bump_and_the_line_says_locked() {
    let mut s = locked_room(Cell::new(6, 2), Vec::new());
    assert!(!s.holds_key(), "a raid does not start holding the keys");
    assert_eq!(
        offered(&s, Direction::East),
        Some(Affordance::LockedDoor),
        "the line names the lock rather than promising an open",
    );

    let turn = s.turn();
    let events = s.step(Input::Step(Direction::East));
    assert!(events.contains(&Event::Bumped {
        into: Cell::new(7, 2)
    }));
    assert!(!open(&s, 7), "the lock held");
    assert_eq!(s.player(), Cell::new(6, 2), "and nobody moved");
    assert_eq!(s.turn(), turn, "a refused bump is free (§4.4)");
}

/// §10.4/#236: the gated doorway is **frameless and self-closing**. There is no hinge
/// to work — the whole span is panels — which is what stops the lock lasting exactly
/// until the first guard walks through and never again.
#[test]
fn a_gated_doorway_is_frameless_and_automatic() {
    let s = locked_room(Cell::new(6, 2), Vec::new());
    for x in [7, 11] {
        let door = s.layout().regions().door(door_in_column(&s, x));
        assert!(door.is_keyed(), "column {x}: the key gate");
        assert!(
            door.hinges().is_empty(),
            "column {x}: no handle to shut it by"
        );
        assert!(
            matches!(door.kind(), DoorKind::Automatic { .. }),
            "column {x}: it shuts itself",
        );
        assert_eq!(door.panels().len(), 3, "column {x}: the folded-in span");
        assert!(!door.is_open(), "column {x}: a locked room starts shut");
        for &p in door.panels() {
            assert_eq!(
                s.layout().facility().terrain(p),
                Some(Terrain::DoorPanelClosed),
                "column {x}: the folded hinge is a panel now",
            );
        }
    }
}

/// §11.2/#236: a door you cannot open is not working furniture, so it draws in the same
/// Neutral white a spent console wears — and goes back to the System tan every other
/// door wears the moment a key is in hand. The recolour is the price the player paid,
/// made visible on the board rather than only in a message that has scrolled away.
#[test]
fn a_locked_door_draws_neutral_until_the_key_is_in_hand() {
    // A guard with its back turned in the corridor, so the key is one bump away.
    let mut s = locked_room(Cell::new(6, 2), vec![Guard::stationary(Cell::new(6, 3))]);
    let panel = Cell::new(7, 2);
    assert_eq!(
        render::render(&s).get(panel.x, panel.y).fg,
        Category::Neutral,
        "locked: a door-shaped wall",
    );

    s.step(Input::Step(Direction::South)); // the takedown, and the key with it
    assert!(s.holds_key());
    assert_eq!(
        render::render(&s).get(panel.x, panel.y).fg,
        Terrain::DoorPanelClosed.category(),
        "with the key it is an ordinary door again",
    );
}

// ---------------------------------------------------------------------------
// The price
// ---------------------------------------------------------------------------

/// §7.2/§10.4/#236: **any** takedown buys the key. It goes straight to hand — the body
/// is already the price — and from that turn on the keyed doors open on the ordinary
/// bump, from either side of the building.
#[test]
fn a_takedown_hands_over_the_key_and_the_room_opens() {
    // A guard parked in the corridor with its back to the player: unaware, so the bump
    // is the takedown (§7.2).
    let mut s = locked_room(Cell::new(6, 2), vec![Guard::stationary(Cell::new(6, 3))]);
    assert!(
        !s.guards()[0].detected_player(),
        "precondition: the rear blind spot hides the player behind the guard",
    );
    assert_eq!(offered(&s, Direction::South), Some(Affordance::Takedown));

    let events = s.step(Input::Step(Direction::South));
    assert!(events.contains(&Event::TakenDown {
        at: Cell::new(6, 3)
    }));
    assert!(
        events.contains(&Event::KeyTaken {
            at: Cell::new(6, 3)
        }),
        "every guard carries a key (§10.4/#236)",
    );
    assert!(s.holds_key());

    // The lock is now a door: the line offers the open, and the bump delivers it.
    assert_eq!(offered(&s, Direction::East), Some(Affordance::OpenDoor));
    s.step(Input::Step(Direction::East));
    assert!(open(&s, 7), "the key opens the doorway it was taken for");
}

/// #236: the key is taken **once**, and only where there is a lock to open. A second
/// takedown is silent about it, and a facility playing §10.4's baseline never mentions
/// a key at all — a message about a mechanic the run does not have is noise on the one
/// row the near line owns (§11.7).
#[test]
fn the_key_is_taken_once_and_only_where_there_is_a_lock() {
    // Concealed in a cupboard, so both neighbours are unaware and both bumps are
    // takedowns (§7.2) — the shortest way to two of them in a row.
    let mut cupboard = locked_strip();
    cupboard.place(Cell::new(6, 2), Terrain::Hideout);
    let mut s = locked_room_on(
        cupboard,
        Cell::new(6, 2),
        vec![
            Guard::stationary(Cell::new(6, 3)),
            Guard::stationary(Cell::new(6, 1)),
        ],
    );
    assert!(s.hidden(), "precondition: concealed");
    let first = s.step(Input::Step(Direction::South));
    assert!(first.iter().any(|e| matches!(e, Event::KeyTaken { .. })));
    let second = s.step(Input::Step(Direction::North));
    assert!(second.contains(&Event::TakenDown {
        at: Cell::new(6, 1)
    }));
    assert!(
        !second.iter().any(|e| matches!(e, Event::KeyTaken { .. })),
        "a key is a key — having taken one you do not go on needing another",
    );

    // The same board with the modifier off: a takedown is only ever a takedown.
    let mut baseline = locked_room(Cell::new(6, 2), vec![Guard::stationary(Cell::new(6, 3))])
        .with_modifiers(LevelModifiers::default());
    let events = baseline.step(Input::Step(Direction::South));
    assert!(events.contains(&Event::TakenDown {
        at: Cell::new(6, 3)
    }));
    assert!(
        !events.iter().any(|e| matches!(e, Event::KeyTaken { .. })),
        "no lock in the building, nothing to announce",
    );
    assert!(!baseline.holds_key());
}

// ---------------------------------------------------------------------------
// The way in without a key
// ---------------------------------------------------------------------------

/// §10.4/#236: **guards carry keys**, so a key-gated door is one a patrol opens by
/// walking into it exactly as it opens any other. That is not a concession — it is what
/// keeps the locked room on somebody's beat, and it is the only reason the door is ever
/// found standing open by a player with nothing in their pockets.
#[test]
fn a_guard_walks_through_the_lock_it_holds_the_key_to() {
    // A guard patrolling out of room B and west along the strip: its route runs through
    // the gated doorway in column 7. The player waits two closed doors away in room A,
    // well out of its path.
    let mut s = locked_room(
        Cell::new(2, 4),
        vec![Guard::patrolling_to(Cell::new(9, 2), Cell::new(5, 2))],
    );

    let mut opened = false;
    for _ in 0..10 {
        let events = s.step(Input::Wait);
        if events.iter().any(|e| {
            matches!(
                e,
                Event::DoorOpened {
                    by_player: false,
                    ..
                }
            )
        }) {
            opened = true;
            break;
        }
    }
    assert!(
        opened,
        "a guard's key opens the door its route runs through"
    );
    assert!(open(&s, 7));
    assert!(!s.holds_key(), "watching a guard is not frisking one");
}

/// §10.4/#236: what the lock refuses is the **handle**, never the doorway — so the few
/// turns a gated door stands open are walk-through for anybody, key or no key. This is
/// the modifier's one bypass, and the reason it is thin is that somebody had to open the
/// door first and is still standing near it.
#[test]
fn an_open_gated_doorway_is_walked_through_without_a_key() {
    // Standing inside the room, the player opens the door outwards (the lock is on the
    // way in) and steps out through the throat into the corridor.
    let mut s = locked_room(Cell::new(8, 2), Vec::new());
    s.step(Input::Step(Direction::West)); // opens it from the room side
    assert!(open(&s, 7));
    s.step(Input::Step(Direction::West)); // onto the open panel
    assert_eq!(s.player(), Cell::new(7, 2));
    s.step(Input::Step(Direction::West)); // out into the corridor
    assert_eq!(s.player(), Cell::new(6, 2));

    // Back in through the still-open doorway, with nothing in hand: the open panel is
    // movement, not an interaction, and the usable line offers nothing on it.
    assert!(!s.holds_key());
    assert!(open(&s, 7), "still inside its close delay");
    assert_eq!(offered(&s, Direction::East), None);
    s.step(Input::Step(Direction::East));
    assert_eq!(
        s.player(),
        Cell::new(7, 2),
        "slipped back in, no key needed"
    );
}

// ---------------------------------------------------------------------------
// It never seals anyone in
// ---------------------------------------------------------------------------

/// §2.2/§7.2's soft-lock class, discharged: the lock is on the way **in**. A player who
/// slipped in behind a guard and watched the door shut behind them opens it from the
/// inside with no key, so the gamble the modifier invites can cost a run its stealth
/// but never the run itself.
#[test]
fn the_lock_refuses_entry_and_never_exit() {
    // Standing inside room B, beside the gated doorway in column 7.
    let mut s = locked_room(Cell::new(8, 2), Vec::new());
    assert!(!s.holds_key());
    assert!(!open(&s, 7), "shut behind them");

    assert_eq!(
        offered(&s, Direction::West),
        Some(Affordance::OpenDoor),
        "from inside, a locked door is a door",
    );
    s.step(Input::Step(Direction::West));
    assert!(open(&s, 7), "the way out is never locked");

    // …and the room's *other* doorway is the same: it is the side you stand on that
    // decides, not which door you picked.
    let mut s = locked_room(Cell::new(10, 2), Vec::new());
    assert_eq!(offered(&s, Direction::East), Some(Affordance::OpenDoor));
    s.step(Input::Step(Direction::East));
    assert!(open(&s, 11));
}

/// The mirror of the test above, on the same board: from the **corridor** side both of
/// the room's doorways refuse. Together they are the whole rule — the room is shut from
/// outside and open from within — stated as the two halves it actually has.
#[test]
fn both_doorways_refuse_from_the_corridor() {
    let mut s = locked_room(Cell::new(6, 2), Vec::new());
    assert_eq!(offered(&s, Direction::East), Some(Affordance::LockedDoor));
    s.step(Input::Step(Direction::East));
    assert!(!open(&s, 7));

    let mut s = locked_room(Cell::new(12, 2), Vec::new());
    assert_eq!(offered(&s, Direction::West), Some(Affordance::LockedDoor));
    s.step(Input::Step(Direction::West));
    assert!(!open(&s, 11));
}

// ---------------------------------------------------------------------------
// The two lock sources compose
// ---------------------------------------------------------------------------

/// §8.3/§10.4/#242/#236: Lockdown may seal a **keyed** door, and releasing that window
/// leaves the key gate exactly where it was. This is what [`DoorLock`](crate::DoorLock)
/// being a set of flags rather than one value buys: an ability whose whole promise is
/// that it is temporary (§2.2) cannot destroy a lock it never placed.
#[test]
fn a_lockdown_window_over_a_keyed_door_leaves_the_key_gate_standing() {
    let mut s = locked_room(Cell::new(6, 2), Vec::new())
        .with_loadout(Loadout::innate().with(AbilityId::Lockdown));
    let door = door_in_column(&s, 7);

    s.step(Input::Activate(AbilityId::Lockdown));
    let lock = s.layout().regions().door(door).lock();
    assert!(lock.is_sealed(), "the window seals what is in reach");
    assert!(lock.is_keyed(), "…and the room's own lock is untouched");

    s.step(Input::Deactivate(AbilityId::Lockdown)); // the free toggle-off (§4.4)
    let lock = s.layout().regions().door(door).lock();
    assert!(!lock.is_sealed(), "the seal is released with the window");
    assert!(
        lock.is_keyed(),
        "the prize room stays locked — the ability never held that key",
    );
    assert_eq!(
        offered(&s, Direction::East),
        Some(Affordance::LockedDoor),
        "and the board still says so",
    );
}
