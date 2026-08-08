//! **Cover** through the turn loop (§8.3/§10.3, #562).
//!
//! The ability is a §10.3 table you put down and push, so what has to be true divides
//! into four, one section apiece:
//!
//! - **It is a table.** Deployed cover and a generated one are the same thing to every
//!   §10.3 consumer — terrain, glyph, blocking, sight, pathing, the drone, and the
//!   crouch's own run geometry. Nothing in the game may be able to tell them apart.
//! - **The bump is the ability.** Push, advance and crouch in one turn; and *crouch
//!   always, push when it can* — a shove with nowhere to go falls back to the plain
//!   §10.3 duck rather than refusing.
//! - **Where it may go.** Plain, empty floor and nothing else, refused for free (§4.4)
//!   — which is also what makes "nothing is ever inside a piece of cover" true.
//! - **The window is the whole safety model.** Expiry and the free toggle-off both take
//!   it away entirely, pose included, leaving plain floor and no residue.

use crate::crouch;
use crate::guard::{routable, GuardState};
use crate::state::*;
use crate::test_support::{open_beat, open_room};

/// A player holding Cover in a big open room, facing east, with `guards` posted as
/// given. Open floor on purpose: the ability is for the crossing §10.1a left bare, so a
/// fixture with furniture already in it would be testing the wrong ground.
fn coverer(player: Cell, guards: Vec<Guard>) -> State {
    State::new(
        open_room(30, 30),
        player,
        Direction::East,
        guards,
        Vec::new(),
        Cell::new(28, 28),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Cover))
}

/// Deploy, spending the turn (§4.4), and report the cell the table went into.
fn deploy(state: &mut State) -> Cell {
    state.step(Input::Activate(AbilityId::Cover));
    state.deployed_cover().expect("the cover went down")
}

/// Cover's window, read off its own catalogue row rather than restated here.
fn window() -> u32 {
    AbilityId::Cover
        .def()
        .economy()
        .expect("Cover is an activated ability")
        .duration()
}

/// Spend `turns` waiting — the way a window is run down without moving the player.
fn wait_out(state: &mut State, turns: u32) {
    for _ in 0..turns {
        state.step(Input::Wait);
    }
}

// ---------------------------------------------------------------------------
// It is a table
// ---------------------------------------------------------------------------

/// **The ability, working**: the press spends the turn (§4.4) and puts a table in the
/// cell the player faces — and the player is *not* behind it yet. Getting behind it is
/// the bump on the turn after, which is the ability's entry price (§2.3).
#[test]
fn deploying_puts_a_table_in_the_faced_cell_and_leaves_you_standing() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    let events = state.step(Input::Activate(AbilityId::Cover));
    assert!(
        events.iter().any(
            |e| matches!(e, Event::AbilityActivated { ability, .. } if *ability == AbilityId::Cover)
        ),
        "the press fired: {events:?}",
    );
    let at = state.deployed_cover().expect("a piece is out");
    assert_eq!(at, Cell::new(11, 10), "the cell faced (§8.4)");
    assert_eq!(
        state.layout().facility().terrain(at),
        Some(Terrain::PartialCover),
    );
    assert_eq!(
        state.player(),
        Cell::new(10, 10),
        "the deploy does not move you"
    );
    assert_eq!(state.crouched_behind(), None, "nor duck you behind it");
    assert!(matches!(
        state.ability_state(AbilityId::Cover),
        AbilityState::Active { .. }
    ));
}

/// **Deployed cover and a generated table are one thing** (§10.3/§10.5) — the ticket's
/// central claim, asserted against every §10.3 consumer there is rather than against the
/// terrain enum alone, because "same terrain kind" is only interesting if nothing
/// downstream has quietly special-cased it.
#[test]
fn deployed_cover_is_a_table_in_every_model() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    let deployed = deploy(&mut state);
    // A generated bench, stamped the way §10.1a stamps one, two rooms away.
    let stamped = Cell::new(20, 20);
    state.layout.place(stamped, Terrain::PartialCover);

    let facility = state.layout().facility();
    for cell in [deployed, stamped] {
        let terrain = facility.terrain(cell).expect("in bounds");
        assert_eq!(terrain, Terrain::PartialCover);
        assert_eq!(terrain.glyph(), 'π', "no new glyph (§11.3)");
        assert!(terrain.blocks_movement(), "solid like a wall (§10.3)");
        assert!(!terrain.blocks_sight(), "a guard sees straight over it");
        assert!(terrain.blocks_pathing(), "patrols route around it");
        assert!(!terrain.routes_player(), "and so does a player route");
        assert!(terrain.admits_drone(), "the drone flies over it (§8.3)");
        assert!(terrain.provides_cover(), "it is what the crouch is for");
        assert_eq!(terrain.category(), crate::Category::System);
        // And the crouch's own geometry gathers each into a run of its own.
        assert_eq!(crouch::cover_run(facility, cell), vec![cell]);
    }
}

/// **Cover placed against a bench extends that run** (§10.3) — arms included, because
/// the flood fill has no way to tell the two apart. The joined piece is concealed by the
/// bench's own half-plane, which a lone piece could not have granted.
#[test]
fn cover_touching_a_bench_conceals_as_the_bench_does() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    // A north–south bench whose southern end is one cell north of where the deploy
    // will land, so the piece joins it and the run becomes four cells of one arm.
    for y in 7..10 {
        state.layout.place(Cell::new(11, y), Terrain::PartialCover);
    }
    let at = deploy(&mut state);
    assert_eq!(at, Cell::new(11, 10));
    let mut run = crouch::cover_run(state.layout().facility(), at);
    run.sort_by_key(|c| (c.y, c.x));
    assert_eq!(
        run,
        (7..11).map(|y| Cell::new(11, y)).collect::<Vec<_>>(),
        "one run, not a piece beside a bench",
    );
    // The arm's half-plane: a viewer far to the east, well past the piece's own row,
    // is across the *bench* from the player — which the lone piece's perpendicular
    // could not have said, since it only ever splits east from west here.
    assert!(state.crouch_would_conceal(at, Cell::new(10, 10), Cell::new(20, 7)));
}

/// **The §7.5 partition is not recut by either event** (§7.3/#374): beats are cut when
/// the guard set changes and at no other time, so putting a table down and taking it
/// away leaves every guard's territory exactly as it was — while the *sweep* still
/// declines to walk onto the cell, because a beat's candidates are filtered through
/// walkable ground (#477).
#[test]
fn deploying_and_releasing_leave_the_beats_alone() {
    let guard = Guard::patrolling(Cell::new(14, 10)).with_beat(open_beat(30, 30));
    let mut state = coverer(Cell::new(10, 10), vec![guard]);
    let before = state.guards()[0].beat().to_vec();
    assert!(
        before.contains(&Cell::new(11, 10)),
        "the cell is in the beat"
    );

    let at = deploy(&mut state);
    assert_eq!(
        state.guards()[0].beat(),
        before,
        "a table is not a change to the guard set",
    );
    // The region graph is untouched too: the cell still belongs to whatever claimed it,
    // and it is the routability filter — not the partition — that keeps guards off it.
    assert!(!routable(state.layout().facility(), at));

    state.step(Input::Deactivate(AbilityId::Cover));
    assert_eq!(
        state.guards()[0].beat(),
        before,
        "and nor is taking it away"
    );
    assert!(routable(state.layout().facility(), at));
}

/// **Guards route around it while it stands, and re-path when it goes** (§7.6/§10.3).
///
/// The fixture is a one-cell throat: a wall across the room with a single gap, the guard
/// on one side and its destination on the other. Plugging the gap is the tactic the
/// missing §10.6 severance check exists to allow, so the guard holds while the plug
/// stands and walks through the moment it goes.
#[test]
fn a_plugged_throat_stops_a_route_and_the_release_restores_it() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    // A wall down column 11 with one gap, at the cell the player faces.
    for y in 1..29 {
        if y != 10 {
            state.layout.place(Cell::new(11, y), Terrain::Wall);
        }
    }
    let throat = Cell::new(11, 10);
    assert!(
        routable(state.layout().facility(), throat),
        "the gap is open to begin with",
    );
    let reaches = |state: &State| {
        crate::path::first_step_toward(Cell::new(5, 10), Cell::new(20, 10), |c| {
            routable(state.layout().facility(), c)
        })
        .is_some()
    };
    assert!(reaches(&state), "the far side is reachable through the gap");

    deploy(&mut state);
    assert!(!reaches(&state), "the plug is a wall to a guard's route");

    state.step(Input::Deactivate(AbilityId::Cover));
    assert!(reaches(&state), "and the release hands the throat back");
}

// ---------------------------------------------------------------------------
// The bump is the ability
// ---------------------------------------------------------------------------

/// **Push, advance, crouch — one turn, one verb** (§8.3/#562), and the usable line said
/// so before the press (§11.7).
#[test]
fn bumping_the_cover_pushes_it_and_steps_you_in_behind() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    let at = deploy(&mut state);
    assert_eq!(
        state.affordances(),
        vec![(Some(Direction::East), Affordance::PushCover)],
        "the row names the shove, which is the only thing telling it from a table",
    );

    let events = state.step(Input::Step(Direction::East));
    assert_eq!(state.deployed_cover(), Some(Cell::new(12, 10)), "shoved on");
    assert_eq!(state.player(), at, "and you are standing where it was");
    assert_eq!(state.crouched_behind(), Some(Cell::new(12, 10)));
    assert_eq!(
        state.layout().facility().terrain(at),
        Some(Terrain::Floor),
        "the cell it left is plain floor again",
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::Crouched { .. })),
        "the pose reports itself once (§10.3/§11.7): {events:?}",
    );
}

/// **The crossing**: pushed repeatedly, the cover walks across the room a cell a turn
/// and the pose is held the whole way — the ability's whole play, and the reason the
/// window is measured against a room rather than a corridor.
#[test]
fn the_cover_can_be_walked_across_a_room() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    deploy(&mut state);
    for step in 1..=6u32 {
        state.step(Input::Step(Direction::East));
        assert_eq!(state.player(), Cell::new(10 + step, 10));
        assert_eq!(state.deployed_cover(), Some(Cell::new(11 + step, 10)));
        assert_eq!(state.crouched_behind(), state.deployed_cover());
        // Concealed from the far side the whole way — the one-cell run's perpendicular.
        assert!(state.concealed_from(Cell::new(25, 10)));
        assert!(!state.concealed_from(Cell::new(2, 10)), "not from behind");
    }
}

/// **Crouch always; push when it can** (§8.3/#562), swept over the four ways a shove can
/// be refused: a wall, other furniture, a guard, and the board's edge. Every one falls
/// back to the plain §10.3 crouch — the turn is spent, the player does not move, the
/// cover does not move, and the pose is taken.
#[test]
fn a_blocked_push_falls_back_to_the_plain_crouch() {
    // Each case puts something in the cell the shove would need, two east of the player.
    type Blocker = (&'static str, fn(&mut State));
    let blockers: [Blocker; 4] = [
        ("a wall", |s: &mut State| {
            s.layout.place(Cell::new(12, 10), Terrain::Wall)
        }),
        ("furniture", |s: &mut State| {
            s.layout.place(Cell::new(12, 10), Terrain::PartialCover)
        }),
        ("a guard", |s: &mut State| {
            s.guards.push(Guard::stationary(Cell::new(12, 10)))
        }),
        ("a body", |s: &mut State| {
            s.bodies.push(crate::body::Body::new(
                Cell::new(12, 10),
                crate::radio::RadioClock::DEFAULT,
                0,
            ))
        }),
    ];
    for (what, block) in blockers {
        let mut state = coverer(Cell::new(10, 10), Vec::new());
        let at = deploy(&mut state);
        block(&mut state);
        assert_eq!(
            state.affordances(),
            vec![(Some(Direction::East), Affordance::Crouch)],
            "{what}: the row promises the duck, not a shove it cannot deliver",
        );
        state.step(Input::Step(Direction::East));
        assert_eq!(state.player(), Cell::new(10, 10), "{what}: nothing moved");
        assert_eq!(
            state.deployed_cover(),
            Some(at),
            "{what}: nor did the cover"
        );
        assert_eq!(state.crouched_behind(), Some(at), "{what}: but you ducked");
    }

    // And **the board's edge**, which the shell normally hides: a real facility is
    // wrapped in wall (§4.1), so the only way to reach the grid's own boundary is to open
    // one — and the point is that a shove with no cell beyond falls back like any other,
    // rather than trusting terrain to have caught it first.
    let mut state = coverer(Cell::new(1, 10), Vec::new());
    state.layout.place(Cell::new(0, 10), Terrain::Floor);
    state.facing = Direction::West;
    let at = deploy(&mut state);
    assert_eq!(at, Cell::new(0, 10), "on the grid's own boundary column");
    assert!(
        state.cover_push(Direction::West).is_none(),
        "nothing beyond"
    );
    assert_eq!(
        state.affordances(),
        vec![(Some(Direction::West), Affordance::Crouch)],
    );
    let events = state.step(Input::Step(Direction::West));
    assert_eq!(state.player(), Cell::new(1, 10), "no step off the board");
    assert_eq!(state.deployed_cover(), Some(at), "nor did the cover move");
    assert_eq!(state.crouched_behind(), Some(at), "but you ducked");
    assert!(events.iter().any(|e| matches!(e, Event::Crouched { .. })));
}

/// **Only the run's own piece is pushable** (§8.3/#562): a generated table is bumped
/// exactly as it always was, even while a window is running, so the ability adds a verb
/// to one cell rather than changing what §10.3 furniture means.
#[test]
fn a_generated_table_is_still_just_a_table() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    deploy(&mut state);
    // A bench to the player's north, nothing to do with the ability.
    state.layout.place(Cell::new(10, 9), Terrain::PartialCover);
    state.layout.place(Cell::new(9, 9), Terrain::PartialCover);
    assert!(state
        .affordances()
        .contains(&(Some(Direction::North), Affordance::Crouch)));
    state.step(Input::Step(Direction::North));
    assert_eq!(state.player(), Cell::new(10, 10), "a table does not move");
    assert_eq!(
        state.layout().facility().terrain(Cell::new(10, 9)),
        Some(Terrain::PartialCover),
    );
    assert_eq!(state.crouched_behind(), Some(Cell::new(10, 9)));
}

// ---------------------------------------------------------------------------
// Where it may go
// ---------------------------------------------------------------------------

/// **Deployment refuses for free on anything that is not plain, empty floor**
/// (§4.4/#562) — swept over the ticket's whole list. Free means free: no turn, no world
/// change, and no ability window opened.
#[test]
fn deployment_refuses_for_free_on_anything_but_plain_floor() {
    let terrains = [
        Terrain::Wall,
        Terrain::DoorHinge,
        Terrain::DoorPanelClosed,
        Terrain::DoorPanelOpen,
        Terrain::Hideout,
        Terrain::PartialCover,
        Terrain::DuctEntry,
        Terrain::Console,
        Terrain::CommsConsole,
        Terrain::EquipmentCache,
        Terrain::Exit,
    ];
    for terrain in terrains {
        let mut state = coverer(Cell::new(10, 10), Vec::new());
        state.layout.place(Cell::new(11, 10), terrain);
        let events = state.step(Input::Activate(AbilityId::Cover));
        assert!(
            events.is_empty(),
            "{terrain:?}: a refused press is silent and free (§4.4/§11.7): {events:?}",
        );
        assert_eq!(state.deployed_cover(), None, "{terrain:?}: nothing placed");
        assert_eq!(state.turn(), 0, "{terrain:?}: no turn spent");
        assert_eq!(
            state.ability_state(AbilityId::Cover),
            AbilityState::Unusable
        );
    }
    // And the two occupants that are not terrain at all.
    let mut state = coverer(
        Cell::new(10, 10),
        vec![Guard::stationary(Cell::new(11, 10))],
    );
    assert_eq!(
        state.ability_state(AbilityId::Cover),
        AbilityState::Unusable
    );
    assert!(state.step(Input::Activate(AbilityId::Cover)).is_empty());
    assert_eq!(state.deployed_cover(), None, "never over a guard");

    let mut state = coverer(Cell::new(10, 10), Vec::new());
    state.bodies.push(crate::body::Body::new(
        Cell::new(11, 10),
        crate::radio::RadioClock::DEFAULT,
        0,
    ));
    assert!(state.step(Input::Activate(AbilityId::Cover)).is_empty());
    assert_eq!(state.deployed_cover(), None, "nor over a body");
}

/// **Not from inside a crawlspace** (§10.7/#562), on the drone's *needs you on your
/// feet* precedent: a body folded into a duct is contact-safe, so a deploy from in there
/// would be a world change with none of the §2.3 exposure that prices it. Refused for
/// free, like every other cell the ability will not take.
#[test]
fn a_crawling_player_cannot_put_furniture_down() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    // A one-cell duct whose entry the player is standing on, mouth to the east — the
    // geometry a §10.7 shortcut has, minus the crawl nobody here needs.
    state.layout.place(Cell::new(10, 10), Terrain::DuctEntry);
    state.in_duct = Some(0);
    assert_eq!(state.cover_deploy_cell(), None, "no aim from in the walls");
    assert_eq!(
        state.ability_state(AbilityId::Cover),
        AbilityState::Unusable
    );
    assert!(state.step(Input::Activate(AbilityId::Cover)).is_empty());
    assert_eq!(state.deployed_cover(), None);
    assert_eq!(state.turn(), 0, "and it cost nothing (§4.4)");
}

/// **Nothing can ever be inside a piece of cover**, which is why the release needs no
/// eject rule (#562 — the Dephase safety-eject problem is not reinvented here).
///
/// The guarantee is structural rather than checked at the end: a piece is only ever
/// written over an empty cell, and while it stands it is solid, so no actor can enter it
/// by any ordinary means. Swept over the window: the guard next door never gets in.
#[test]
fn nothing_ever_ends_up_inside_the_cover() {
    let guard = Guard::patrolling(Cell::new(14, 10)).with_beat(open_beat(30, 30));
    let mut state = coverer(Cell::new(10, 10), vec![guard]);
    let at = deploy(&mut state);
    for _ in 0..window() {
        state.step(Input::Wait);
        assert!(
            state.guards().iter().all(|g| g.pos() != at),
            "a guard is standing in the cover",
        );
        assert_ne!(state.player(), at, "and neither is the player");
    }
}

// ---------------------------------------------------------------------------
// The window is the whole safety model
// ---------------------------------------------------------------------------

/// **Expiry takes it away entirely — pose included** (§8.2/#562). A player crouched
/// behind the piece when the window ends is simply standing in the open, which is the
/// ability's tension rather than a rough edge.
#[test]
fn expiry_removes_the_cover_and_the_pose_with_it() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    deploy(&mut state);
    state.step(Input::Step(Direction::East)); // push and duck
    let behind = state.deployed_cover().expect("out");
    assert_eq!(state.crouched_behind(), Some(behind));
    assert!(state.concealed_from(Cell::new(25, 10)));

    // Run the window down to its last turn, reading the clock rather than doing the
    // arithmetic: the deploy and the push have each already spent one of the twelve
    // (§8.2 — the activation turn counts).
    while let AbilityState::Active { remaining } = state.ability_state(AbilityId::Cover) {
        if remaining == 1 {
            break;
        }
        state.step(Input::Wait);
    }
    assert!(
        state.deployed_cover().is_some(),
        "still up on the last turn"
    );
    let events = state.step(Input::Wait);

    assert!(
        events.iter().any(
            |e| matches!(e, Event::AbilityExpired { ability } if *ability == AbilityId::Cover)
        ),
        "the window closed: {events:?}",
    );
    assert_eq!(state.deployed_cover(), None);
    assert_eq!(
        state.layout().facility().terrain(behind),
        Some(Terrain::Floor),
        "plain floor, no residue in any model",
    );
    assert_eq!(state.crouched_behind(), None, "simply standing");
    assert!(!state.crouched());
    assert!(
        !state.concealed_from(Cell::new(25, 10)),
        "and a standing figure in the open is seen",
    );
}

/// **The free toggle-off is the same teardown** (§4.4/§8.2/#562): it costs no turn, it
/// refunds nothing — the full lockout still runs — and it leaves exactly as little
/// behind as expiry does. This is what keeps the ability from ever boxing its owner in.
#[test]
fn the_toggle_off_is_free_and_leaves_nothing_behind() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    let at = deploy(&mut state);
    state.step(Input::Step(Direction::East));
    let behind = state.deployed_cover().expect("out");
    let turn = state.turn();

    state.step(Input::Deactivate(AbilityId::Cover));
    assert_eq!(state.turn(), turn, "free (§4.4)");
    assert_eq!(state.deployed_cover(), None);
    assert_eq!(state.crouched_behind(), None, "the pose goes with it");
    for cell in [at, behind] {
        assert_eq!(
            state.layout().facility().terrain(cell),
            Some(Terrain::Floor),
            "{cell:?} is plain floor",
        );
    }
    // Nothing is refunded: the lockout runs in full (§8.2).
    assert!(matches!(
        state.ability_state(AbilityId::Cover),
        AbilityState::Cooling { .. }
    ));
}

/// **A pose anchored on a bench the cover merely joined survives the window** (§10.3):
/// the furniture the player is actually behind is still there, so standing them up would
/// be the teardown reaching past its own cell.
#[test]
fn a_pose_on_the_joined_bench_outlives_the_window() {
    let mut state = coverer(Cell::new(10, 10), Vec::new());
    for y in 7..10 {
        state.layout.place(Cell::new(11, y), Terrain::PartialCover);
    }
    deploy(&mut state);
    // Duck on the *bench*, to the player's north-east — reached by facing it and
    // bumping, which anchors the pose on a generated table rather than on the piece.
    state.step(Input::Step(Direction::North)); // walk to (10, 9), beside the bench
    state.step(Input::Step(Direction::East)); // bump (11, 9): a plain crouch
    assert_eq!(state.crouched_behind(), Some(Cell::new(11, 9)));

    wait_out(&mut state, window());
    assert_eq!(state.deployed_cover(), None, "the window closed");
    assert_eq!(
        state.crouched_behind(),
        Some(Cell::new(11, 9)),
        "the bench is still there and so is the pose",
    );
}

/// **Determinism** (§12.4): the same seed and the same inputs reproduce every deployment,
/// push and removal — no RNG anywhere in the ability.
#[test]
fn a_replay_reproduces_every_deployment_push_and_removal() {
    let script = [
        Input::Activate(AbilityId::Cover),
        Input::Step(Direction::East),
        Input::Step(Direction::East),
        Input::Step(Direction::East),
        Input::Deactivate(AbilityId::Cover),
        Input::Activate(AbilityId::Cover),
        Input::Step(Direction::East),
    ];
    let play = || {
        let mut state = coverer(
            Cell::new(10, 10),
            vec![Guard::patrolling(Cell::new(20, 20)).with_beat(open_beat(30, 30))],
        );
        let mut trace = Vec::new();
        for input in script {
            let events = state.step(input);
            trace.push((
                state.player(),
                state.deployed_cover(),
                state.crouched_behind(),
                format!("{events:?}"),
            ));
        }
        (trace, state.layout().facility().clone())
    };
    let (a, grid_a) = play();
    let (b, grid_b) = play();
    assert_eq!(a, b, "the same script plays the same way");
    for y in 0..30 {
        for x in 0..30 {
            let cell = Cell::new(x, y);
            assert_eq!(grid_a.terrain(cell), grid_b.terrain(cell), "{cell:?}");
        }
    }
}

/// **No guard reacts to furniture appearing or vanishing** (§9/#562) — the honest §2.3
/// call, pinned so it stays a decision. Guards detect on vision and nothing in §7 notices
/// that a room changed shape, so a Calm patrol stays Calm through both events.
#[test]
fn a_guard_never_notices_the_furniture_change() {
    // Posted well out of sight range and never moving, so what is being measured is the
    // furniture and not a sighting: the ability changes the shape of a room two thirds of
    // a board away, and nothing in §7 has a channel for that.
    let mut state = coverer(Cell::new(10, 10), vec![Guard::stationary(Cell::new(28, 1))]);
    assert_eq!(state.guards()[0].state(), GuardState::Calm);
    deploy(&mut state);
    wait_out(&mut state, 3);
    assert_eq!(state.guards()[0].state(), GuardState::Calm);
    assert_eq!(state.alert(), 0, "the ladder does not step (§7.3)");
    state.step(Input::Deactivate(AbilityId::Cover));
    wait_out(&mut state, 3);
    assert_eq!(state.guards()[0].state(), GuardState::Calm);
    assert_eq!(state.alert(), 0);
}
