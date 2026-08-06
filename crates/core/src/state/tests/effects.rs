use crate::ability::Loadout;
use crate::guard::Guard;
use crate::state::effects::*;
use crate::state::*;
use crate::test_support::open_room;

/// A player at (20, 20) of a 40×40 room facing north, carrying Confusion, with
/// `guards` posted around them. The bare world the footprint tests need: room
/// enough that the whole `CONFUSION_RADIUS` box is in bounds on every side, so a
/// clipped edge never masquerades as a footprint rule.
fn level_with(guards: Vec<Guard>) -> State {
    State::new(
        open_room(40, 40),
        Cell::new(20, 20),
        Direction::North,
        guards,
        Vec::new(),
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Confusion))
}

/// The same world with one guard inside the blast — the firing tests all need one,
/// since a blast that would catch nobody is refused (§8.3/#325).
fn level_with_a_target() -> State {
    level_with(vec![Guard::stationary(Cell::new(22, 20))])
}

/// A player at (10, 10) of a 40×40 room with Camouflage **already running** — the
/// world the #341 tests need. Bare floor and no guards, so the only thing that can
/// move the mark is the player's own stillness.
fn level_with_camouflage_on() -> State {
    let mut s = State::new(
        open_room(40, 40),
        Cell::new(10, 10),
        Direction::South,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Camouflage));
    s.step(Input::Activate(AbilityId::Camouflage));
    assert!(
        s.abilities.effect_active(Effect::ConcealWhileStill),
        "precondition: the camo is on"
    );
    s
}

/// A player at (10, 10) of a 40×40 room facing south with a decoy already out at
/// (10, 11) — the world the #340 tests need, with room to walk away from the fake
/// in every direction and no guard to stomp it by accident.
fn level_with_a_live_decoy() -> State {
    let mut s = State::new(
        open_room(40, 40),
        Cell::new(10, 10),
        Direction::South,
        Vec::new(),
        Vec::new(),
        Cell::new(38, 38),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Decoy));
    s.step(Input::Activate(AbilityId::Decoy));
    assert_eq!(
        s.decoy(),
        Some(Cell::new(10, 11)),
        "precondition: the fake is out, on the faced cell"
    );
    s
}

/// Fire Confusion, spending the turn (§4.4).
fn fire(state: &mut State) {
    let events = state.step(Input::Activate(AbilityId::Confusion));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ConfusionFired { .. })),
        "the blast went off"
    );
}

/// The blast fired by the last `step`, straight off the event — the object the daze
/// was computed from, which is what the mark is asserted against.
fn last_blast(state: &State) -> EffectArea {
    state
        .last_events()
        .iter()
        .find_map(|e| match e {
            Event::ConfusionFired { blast, .. } => Some(*blast),
            _ => None,
        })
        .expect("the blast went off")
}

/// [`area_radius`] holds each effect's **own reach** and nothing else's (§8.3): the
/// two that act on a region around the player, and no row for the rest. The layer's
/// geometry no longer reads the table at all — it is read at each firing seam
/// ([`confusion_blast`](State::confusion_blast),
/// [`lockdown_doors`](State::lockdown_doors)) — so this is pinned to keep a new
/// radius tech a visible edit here rather than a silent one at the seam.
#[test]
fn only_the_area_effects_declare_a_radius() {
    for effect in [
        Effect::Confuse,
        Effect::SealDoors,
        Effect::ExtraStep,
        Effect::ConcealWhileStill,
        Effect::SpawnDecoy,
        Effect::Phase,
        Effect::AutoDoors,
        Effect::EnhancedSight,
        Effect::FakeCall,
    ] {
        assert_eq!(
            area_radius(effect).is_some(),
            matches!(
                effect,
                Effect::Confuse | Effect::SealDoors | Effect::FakeCall
            ),
            "{effect:?}: only an effect that acts on a region has a radius",
        );
    }
}

/// The blast's own numbers, pinned so a later change is a visible edit (§8.3
/// **[START]**): the reach it fires with and how long what it catches stays frozen.
/// The ability itself is **instant** — the time it buys lives on the guards, so
/// there is no player-side window at all.
#[test]
fn the_confusion_numbers_are_pinned() {
    assert_eq!(CONFUSION_RADIUS, 6);
    assert_eq!(CONFUSION_DAZE_TURNS, 6);
    assert_eq!(
        AbilityId::Confusion
            .def()
            .economy()
            .expect("Confusion is an activated ability")
            .duration(),
        0,
        "instant: fired, not carried"
    );
}

/// The wash is the §6.1 **box** of [`CONFUSION_RADIUS`] around the cell it fired
/// from — asserted against the rule, not against a hand-drawn shape: every painted
/// cell is one [`EffectArea::contains`] accepts, and every in-bounds cell it accepts
/// is painted. This is the criterion that stops the picture and the mechanic
/// drifting.
#[test]
fn the_cell_mark_is_exactly_the_rule_s_box() {
    let mut s = level_with_a_target();
    let fired_from = s.player();
    fire(&mut s);
    let area = last_blast(&s);
    let painted: Vec<Cell> = s.effect_cell_marks().collect();

    let facility = s.layout().facility();
    for y in 0..facility.height() {
        for x in 0..facility.width() {
            let cell = Cell::new(x, y);
            assert_eq!(
                painted.contains(&cell),
                area.contains(cell),
                "{cell:?}: painted and in-the-box must agree"
            );
        }
    }
    // …and it is a box, not a disc: the corner of the square is in.
    let corner = Cell::new(
        fired_from.x + CONFUSION_RADIUS,
        fired_from.y + CONFUSION_RADIUS,
    );
    assert!(painted.contains(&corner), "the diagonal corner is inside");
}

/// The wash the renderer paints **is** the object the daze was computed from
/// (#308/#324): the fired area rides the event and is turned into the mark's cell
/// set there and then, so the picture and the rule are one value rather than two
/// derivations that happen to agree.
#[test]
fn the_painted_cells_are_the_fired_blast() {
    let mut s = level_with_a_target();
    fire(&mut s);
    let painted: Vec<Cell> = s.effect_cell_marks().collect();
    assert_eq!(
        painted,
        last_blast(&s).cells(s.layout().facility()),
        "the lit cells are the blast that fired"
    );
}

/// The wash stays where it fired (§8.3/#325): a step west leaves the box behind,
/// because the blast is a thing that happened at a place, not a bubble the player
/// carries.
#[test]
fn the_cell_mark_does_not_follow_the_player() {
    let mut s = level_with_a_target();
    let fired_from = s.player();
    fire(&mut s);
    let painted: Vec<Cell> = s.effect_cell_marks().collect();
    assert!(painted.contains(&Cell::new(fired_from.x + CONFUSION_RADIUS, fired_from.y)));

    // The wash outlives the step only if `EFFECT_FLASH_TURNS` is raised; what is
    // asserted here is that while it *is* lit, it does not move. At the [START]
    // life of one turn the step burns it out, which is itself the check that the
    // box never reappears somewhere new.
    s.step(Input::Step(Direction::West));
    assert_eq!(s.player().x, fired_from.x - 1, "the step landed");
    assert!(
        s.effect_cell_marks()
            .all(|c| c.x <= fired_from.x + CONFUSION_RADIUS),
        "nothing is painted east of where the blast reached"
    );
}

/// A **momentary** mark is a flash (§11.5): it shows for [`EFFECT_FLASH_TURNS`]
/// renders — one, the firing frame — and is gone on the next, while the **standing**
/// mark on the guards it caught is still very much lit. The two lifetimes, on one
/// firing, doing exactly the different jobs they are for.
#[test]
fn the_momentary_mark_fades_while_the_standing_one_holds() {
    assert_eq!(EFFECT_FLASH_TURNS, 1, "the [START] flash life is pinned");
    let mut s = level_with_a_target();
    fire(&mut s);
    // The firing frame counts: it is the first render the player reads, and the
    // fade runs at the head of the *next* turn, so the wash shows for exactly
    // `EFFECT_FLASH_TURNS` renders.
    for turn in 0..EFFECT_FLASH_TURNS {
        assert!(
            s.effect_cell_marks().next().is_some(),
            "the wash is still lit on render {turn}"
        );
        s.step(Input::Wait);
    }
    assert!(
        s.effect_cell_marks().next().is_none(),
        "the wash has burned out"
    );
    assert!(
        s.effect_thing_marks().next().is_some(),
        "…while the daze it dealt out is still marked"
    );
}

/// A **standing** mark ends with the state it reports and leaves no residue: the
/// turn the last daze runs out, the mark is gone from the layer — not merely
/// yielding nothing, but dropped, so a mark can never outlive its effect.
#[test]
fn the_standing_mark_ends_with_the_daze() {
    let mut s = level_with_a_target();
    fire(&mut s);
    for _ in 0..CONFUSION_DAZE_TURNS {
        s.step(Input::Wait);
    }
    assert!(
        s.guards().iter().all(|g| !s.guard_confused(g)),
        "precondition: the daze has run out"
    );
    assert!(
        s.effect_thing_marks().next().is_none(),
        "the mark went with it"
    );
    assert!(s.effect_marks.is_empty(), "…and left nothing behind");
}

/// The thing mark is exactly the daze, for a guard the player can see *and* for one
/// felt only through a wall (§9.2) — the common case, since the blast reaches
/// through walls — and never for one the fog is hiding (§11.5a).
#[test]
fn every_dazed_guard_the_player_perceives_is_marked() {
    let mut s = level_with_a_target();
    fire(&mut s);
    let marked: Vec<Cell> = s.effect_thing_marks().collect();
    for guard in s.guards() {
        assert_eq!(
            marked.contains(&guard.pos()),
            s.guard_confused(guard) && s.perceive_guard(guard).is_some(),
            "the mark is the daze, on a guard that is already drawn"
        );
    }
}

/// Pierce Wall is the first fixed-cell reader (#303/#338): boring a wall lights a
/// **momentary** mark on the cell it opened, and nothing else on the board.
#[test]
fn a_bore_marks_the_cell_it_opened() {
    let mut layout = open_room(20, 20);
    let wall = Cell::new(10, 9);
    layout.place(wall, Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(10, 10),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::PierceWall));

    let events = s.step(Input::Activate(AbilityId::PierceWall));
    assert!(
        events.contains(&Event::WallBored { at: wall }),
        "precondition: the bore went through: {events:?}",
    );
    assert_eq!(
        s.effect_cell_marks().collect::<Vec<_>>(),
        vec![wall],
        "the opened cell, and only it",
    );
    assert!(
        s.effect_thing_marks().next().is_none(),
        "a bore holds nothing",
    );
    // Momentary: gone on the very next turn, like the blast's wash.
    s.step(Input::Wait);
    assert!(
        s.effect_cell_marks().next().is_none(),
        "the bore mark is a moment, not a monument",
    );
}

/// A **refusal** lights nothing (§11.7/#338): it is a message, not an effect, so
/// the wall Pierce Wall declined to open is never washed as though it had been.
#[test]
fn a_refused_bore_marks_nothing() {
    // Standing in the open with no adjacent wall: `BoreRefusal::NothingToBore`.
    let mut s = State::new(
        open_room(20, 20),
        Cell::new(10, 10),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 18),
    )
    .with_loadout(Loadout::innate().with(AbilityId::PierceWall));

    let events = s.step(Input::Activate(AbilityId::PierceWall));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::BoreRefused { .. })),
        "precondition: the bore was refused: {events:?}",
    );
    assert!(
        s.effect_cell_marks().next().is_none(),
        "a refusal paints nothing",
    );
}

/// A 12×12 room with a wall at `(5,4)` and the player one cell west of it holding
/// Dephase, seeded so the landing is reproducible (§12.4). Phasing east and waiting
/// out the duration strands them inside the wall and fires the safety eject.
///
/// Returned **one turn short of the expiry**, so a caller's single `Wait` is the turn
/// the window ends. The waiting is counted off the catalogue's duration rather than
/// written out, so a retune (#449 moved it 3 → 4) leaves every caller reading the
/// same way.
fn phased_into_a_wall() -> State {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase))
    .with_rng(crate::Rng::new(7));
    s.step(Input::Activate(AbilityId::Dephase));
    s.step(Input::Step(Direction::East));
    assert_eq!(
        s.player(),
        Cell::new(5, 4),
        "precondition: inside the solid"
    );
    // Stand in the solid for whatever the window has left beyond the caller's own
    // expiry turn — none at all when the duration is 2, one when it is 4.
    for _ in 2..dephase_duration() - 1 {
        s.step(Input::Wait);
    }
    s
}

/// Dephase's `[START]` window (§8.3), counting the activation turn — read from the
/// catalogue so a tune moves the tests with it rather than stranding them on a number.
fn dephase_duration() -> u32 {
    AbilityId::Dephase
        .def()
        .economy()
        .expect("Dephase is activated")
        .duration()
}

/// The two ends of the throw the last `step` reported, straight off the event — the
/// pair the stun was priced from, which is what the mark is asserted against.
fn last_throw(state: &State) -> (Cell, Cell) {
    state
        .last_events()
        .iter()
        .find_map(|e| match e {
            Event::Ejected { from, to, .. } => Some((*from, *to)),
            _ => None,
        })
        .expect("the eject fired")
}

/// §8.3/#329/#339: the safety eject lights a **momentary** mark on both of its ends
/// — the solid it stranded you in and the cell it threw you onto — and on nothing
/// else. Two marks for one event, because the span between them is the fact the
/// stunned player is being told.
#[test]
fn an_eject_marks_both_ends_of_the_throw() {
    let mut s = phased_into_a_wall();
    s.step(Input::Wait); // the duration ends inside the wall
    let (from, to) = last_throw(&s);
    assert_eq!(
        from,
        Cell::new(5, 4),
        "precondition: thrown out of the wall"
    );
    assert_eq!(s.player(), to, "precondition: standing where it put them");

    let mut painted: Vec<Cell> = s.effect_cell_marks().collect();
    painted.sort_by_key(|c| (c.y, c.x));
    let mut both = vec![from, to];
    both.sort_by_key(|c| (c.y, c.x));
    assert_eq!(painted, both, "both ends, and only them");
    assert!(
        s.effect_thing_marks().next().is_none(),
        "an eject holds nothing",
    );
}

/// The origin end is a **solid** and is washed anyway (§11.5a/#339): the layer paints
/// over whatever geometry it finds, and a cell the player occupied a moment ago is
/// their own knowledge rather than a reveal. Gating the mark on walkability would
/// silently drop the half of the throw that explains it.
#[test]
fn the_origin_mark_draws_even_though_it_is_solid() {
    let mut s = phased_into_a_wall();
    s.step(Input::Wait);
    let (from, _) = last_throw(&s);
    assert!(
        !s.layout().facility().can_enter(from, ACTOR_FILL),
        "precondition: {from:?} is a solid no body can stand in",
    );
    assert!(
        s.effect_cell_marks().any(|c| c == from),
        "the solid end is marked all the same",
    );
}

/// The pair is lit on **exactly the frames the player cannot act from** (#339) —
/// stated as the invariant rather than as a count, since the two ways to get this
/// wrong are opposite and both are bugs: go out too early and the cue expires while
/// its reader is still held down; stay one frame too long and it is still reporting
/// the throw on the frame they are choosing a real move from.
///
/// `stunned() > 0` is precisely "the next press will be eaten", so asserting the
/// mark against it — rather than against a number — leaves nothing for a later
/// repricing of the stun (§8.3 **[START]**) to knock out of step.
#[test]
fn the_eject_marks_are_lit_exactly_while_the_player_cannot_act() {
    let mut s = phased_into_a_wall();
    s.step(Input::Wait);
    let stun = s.stunned();
    assert!(stun > 0, "precondition: the throw cost some helplessness");
    let landed = s.player();

    let mut helpless_frames = 0;
    while s.stunned() > 0 {
        assert_eq!(
            s.effect_cell_marks().count(),
            2,
            "both ends are lit on a frame the player cannot act from",
        );
        assert_eq!(
            s.player(),
            landed,
            "a stunned player cannot move off the mark"
        );
        helpless_frames += 1;
        s.step(Input::Wait);
    }
    assert_eq!(
        helpless_frames, stun,
        "precondition: one wasted press per turn of stun",
    );
    assert!(
        s.effect_cell_marks().next().is_none(),
        "the first frame whose press is answered is already dark",
    );
}

/// §8.3: the entombment — nowhere in the facility to be thrown clear to — marks
/// **one** cell, the one that took you. The run ends on this frame, so saying where
/// is the last thing the board has to do.
#[test]
fn an_entombment_marks_the_one_cell_that_took_you() {
    let mut f = crate::facility::Facility::walled_box(9, 9);
    for y in 0..9 {
        for x in 0..9 {
            f.set_terrain(x, y, Terrain::Wall);
        }
    }
    let entombing = Cell::new(4, 4);
    let mut s = State::new(
        crate::Layout::from_facility(f),
        entombing,
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(7, 7),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase));

    s.step(Input::Activate(AbilityId::Dephase));
    for _ in 0..4 {
        if s.last_events().contains(&Event::Entombed { at: entombing }) {
            break;
        }
        s.step(Input::Wait);
    }
    assert_eq!(
        s.outcome(),
        Outcome::Lost,
        "precondition: the wall took them"
    );
    assert_eq!(
        s.effect_cell_marks().collect::<Vec<_>>(),
        vec![entombing],
        "the entombing cell, and only it",
    );
}

/// §8.3/#340: a live decoy wears the mark on the **thing**, not a wash on the
/// board. The fake is what is running, so the mark rides it and claims no geometry
/// around it.
#[test]
fn a_live_decoy_wears_a_mark_on_the_thing() {
    let s = level_with_a_live_decoy();
    assert_eq!(
        s.effect_thing_marks().collect::<Vec<_>>(),
        vec![s.decoy().expect("the fake is out")],
        "the fake, and nothing else",
    );
    assert!(
        s.effect_cell_marks().next().is_none(),
        "a decoy washes no cells: it is a thing, not a footprint",
    );
}

/// §8.3/#340: the mark is **standing**, so it lasts the whole of the decoy's life
/// rather than flashing — and it ends with the window that placed it, leaving
/// nothing behind. The two halves of "for as long as it lives", asserted against
/// the ability's own duration rather than a hand-copied number.
#[test]
fn the_decoy_mark_lasts_the_window_and_dies_with_it() {
    let mut s = level_with_a_live_decoy();
    let duration = AbilityId::Decoy
        .def()
        .economy()
        .expect("Decoy is an activated ability")
        .duration();
    assert!(
        duration > EFFECT_FLASH_TURNS,
        "precondition: a standing mark is only distinguishable past the flash life",
    );
    // The activation turn is the first of the window (§8.2) and the clock is ticked
    // at the **end** of each turn, so the fake is still out after every wait up to
    // the window's last turn, and gone the moment that one is spent.
    for turn in 2..duration {
        s.step(Input::Wait);
        assert!(
            s.decoy().is_some(),
            "precondition: the fake is still alive on turn {turn}",
        );
        assert_eq!(
            s.effect_thing_marks().count(),
            1,
            "still marked on turn {turn}",
        );
    }
    s.step(Input::Wait);
    assert!(s.decoy().is_none(), "the window ran out and took the fake");
    assert!(
        s.effect_thing_marks().next().is_none(),
        "the mark went with it",
    );
    assert!(s.effect_marks.is_empty(), "…and left nothing behind");
}

/// §11.5a's second exception (#321/#326/#340): the mark follows the glyph it sits
/// under, and the decoy's glyph is drawn out of the FOV. A fake you have walked
/// away from is the whole point of the ability, so a mark that needed line of sight
/// would be a mark the ability cannot use.
#[test]
fn the_decoy_mark_is_drawn_out_of_view() {
    let mut s = level_with_a_live_decoy();
    let decoy = s.decoy().expect("the fake is out");
    while s.player_fov().contains(decoy) {
        s.step(Input::Step(Direction::North));
    }
    assert_eq!(
        s.decoy(),
        Some(decoy),
        "precondition: nothing stepped on it"
    );
    assert!(
        s.effect_thing_marks().any(|cell| cell == decoy),
        "the fake is still marked with the player's back to it",
    );
}

/// §8.3/#340: no mark outlives the decoy. A stomp kills the fake in the middle of a
/// turn — after the layer's own decay has already run — and the mark is gone on
/// that very frame, because the mark reads the decoy itself rather than a cell it
/// once remembered.
#[test]
fn the_mark_dies_on_the_same_turn_the_decoy_does() {
    let mut s = level_with_a_live_decoy();
    let decoy = s.decoy().expect("the fake is out");
    let events = s.step(Input::Step(Direction::South));
    assert!(
        events.contains(&Event::DecoyDied { at: decoy }),
        "precondition: the player stepped on their own fake: {events:?}",
    );
    assert!(
        s.effect_thing_marks().next().is_none(),
        "the mark went on the same turn",
    );
    // …and the record is swept on the next turn's decay, so the layer never carries
    // a mark that can no longer paint.
    s.step(Input::Wait);
    assert!(s.effect_marks.is_empty(), "the record went too");
}

/// §4.4/§8.3 (#340): an early toggle-off is free and takes the fake with it, so the
/// mark is cleared outright rather than merely falling silent.
#[test]
fn the_decoy_mark_goes_with_an_early_toggle_off() {
    let mut s = level_with_a_live_decoy();
    s.step(Input::Deactivate(AbilityId::Decoy));
    assert!(s.decoy().is_none(), "precondition: the fake is gone");
    assert!(
        s.effect_thing_marks().next().is_none(),
        "and so is its mark"
    );
    assert!(s.effect_marks.is_empty(), "cleared, not merely quiet");
}

/// §8.3/#341: the mark **blinks with the rule**, which is the whole of what it adds
/// to the bar. Still → marked, move → unmarked *while the ability is still running
/// and still counting down*, still again → marked. The bar reads `Camo[n]` through
/// every one of these frames; the board does not.
#[test]
fn the_camouflage_mark_follows_the_stillness_and_not_the_window() {
    let mut s = level_with_camouflage_on();
    let marked = |s: &State| s.effect_thing_marks().any(|cell| cell == s.player());

    // The activation turn is itself a still turn (§8.2), so the mark is up on the
    // very first frame the ability protects rather than a turn behind it.
    assert!(
        marked(&s),
        "activating does not move: concealed from turn one"
    );

    s.step(Input::Wait);
    assert!(marked(&s), "a wait is a still turn");

    s.step(Input::Step(Direction::East));
    assert!(
        s.abilities.effect_active(Effect::ConcealWhileStill),
        "precondition: the window is still open — the bar would still read Camo[n]",
    );
    assert!(!marked(&s), "…but the board says the concealment lapsed");

    s.step(Input::Wait);
    assert!(marked(&s), "and it resumes on the next still turn");
}

/// §11.5/#341: the mark is the concealment **rule**, not a second derivation of it.
/// Asserted against [`camouflage_holding`](State::camouflage_holding) — the very
/// predicate [`concealed_from`](State::concealed_from) consumes — over a run of
/// mixed turns, so no future change to the rule can move the picture out of step
/// with the detection set.
#[test]
fn the_camouflage_mark_is_exactly_the_concealment_rule() {
    let mut s = level_with_camouflage_on();
    for (turn, input) in [
        Input::Wait,
        Input::Step(Direction::East),
        Input::Step(Direction::West),
        Input::Wait,
        Input::Wait,
        Input::Step(Direction::East),
    ]
    .into_iter()
    .enumerate()
    {
        s.step(input);
        assert_eq!(
            s.effect_thing_marks().any(|cell| cell == s.player()),
            s.camouflage_holding(),
            "turn {turn}: the mark and the rule are one fact",
        );
    }
}

/// §8.3/#341: a **cupboard** conceals too (§10.3), and it is not this ability. The
/// mark says "the camo is working", so it must not light for a player who is merely
/// concealed — otherwise it reports a protection that ends the moment they step out.
#[test]
fn concealment_that_is_not_the_camouflage_lights_nothing() {
    let mut layout = open_room(20, 20);
    layout.place(Cell::new(10, 11), Terrain::Hideout);
    let mut s = State::new(
        layout,
        Cell::new(10, 10),
        Direction::South,
        Vec::new(),
        Vec::new(),
        Cell::new(18, 18),
    );
    s.step(Input::Step(Direction::South));
    assert!(s.hidden(), "precondition: inside the cupboard");
    assert!(s.concealed_from(Cell::new(10, 5)), "…and concealed by it");
    assert!(
        !s.camouflage_holding(),
        "but not by the ability, which is not even held",
    );
    assert!(
        s.effect_thing_marks().next().is_none(),
        "so the effect layer says nothing",
    );
}

/// §8.2/§4.4 (#341): the mark ends with its window — by expiry and by an early
/// toggle-off alike — and leaves no record behind either way.
#[test]
fn the_camouflage_mark_ends_with_its_window() {
    let duration = AbilityId::Camouflage
        .def()
        .economy()
        .expect("Camouflage is an activated ability")
        .duration();

    // Expiry: the clock is ticked at the end of each spent turn, so the window's
    // last turn is the `duration`-th and the mark is gone the moment it is spent.
    let mut s = level_with_camouflage_on();
    for _ in 2..duration {
        s.step(Input::Wait);
    }
    assert!(
        s.effect_thing_marks().next().is_some(),
        "precondition: still concealed on the window's last turn",
    );
    s.step(Input::Wait);
    assert!(
        !s.abilities.effect_active(Effect::ConcealWhileStill),
        "precondition: the window ran out",
    );
    assert!(s.effect_marks.is_empty(), "the mark went with it");

    // Early toggle-off: free (§4.4), and it clears outright rather than falling
    // silent — a still player would otherwise keep drawing a mark for an ability
    // they switched off.
    let mut s = level_with_camouflage_on();
    s.step(Input::Deactivate(AbilityId::Camouflage));
    assert!(!s.camouflage_holding(), "precondition: switched off");
    assert!(s.effect_marks.is_empty(), "cleared, not merely quiet");
}

/// Refiring replaces a mark rather than stacking one: the layer holds at most one
/// mark per (ability, placement), so a second blast cannot leave the first box on
/// the board beside its own.
#[test]
fn refiring_replaces_the_mark_it_relights() {
    let mut s = level_with(vec![
        Guard::stationary(Cell::new(22, 20)),
        Guard::stationary(Cell::new(18, 20)),
    ]);
    fire(&mut s);
    let first: Vec<Cell> = s.effect_cell_marks().collect();
    assert_eq!(s.effect_marks.len(), 2, "one wash, one standing mark");

    // Walk clear of the first box and fire again — the ability's own cooldown is
    // waived by relighting the mark directly, which is what this test is about.
    let blast = EffectArea {
        centre: Cell::new(30, 30),
        radius: 2,
    };
    let cells = blast.cells(s.layout().facility());
    s.light_mark(
        AbilityId::Confusion,
        MarkPlace::Cells(cells.clone()),
        MarkLife::Momentary(EFFECT_FLASH_TURNS),
    );
    assert_eq!(s.effect_marks.len(), 2, "still one wash, not two");
    assert_eq!(s.effect_cell_marks().collect::<Vec<_>>(), cells);
    assert_ne!(first, cells, "precondition: a genuinely different box");
}

/// §8.3/§11.2 (#416): a running Dephase on **open floor** is exactly the sort of
/// unconditional "the ability is on" the effect layer refuses to restate — the §11.4
/// bar already says the clock is running. Nothing is marked until the player is
/// somewhere the eject would have to fire.
#[test]
fn a_phase_over_open_floor_marks_nothing() {
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase));
    s.step(Input::Activate(AbilityId::Dephase));

    assert!(
        s.abilities.effect_active(Effect::Phase),
        "precondition: the window is open — the bar reads Phase[n]",
    );
    assert!(
        s.can_rematerialize(),
        "precondition: standing where a solid body can stand",
    );
    assert!(
        s.effect_thing_marks().next().is_none(),
        "a phase you could end safely says nothing the bar does not",
    );
}

/// §8.3/§11.2 (#416): the mark **follows the cell**, not the window — the
/// disagreement §11.2 requires of a marked effect. One unchanged window, three frames,
/// the answer changing **both ways**: open floor says nothing, the solid lights up,
/// and the floor beyond puts it out again while the bar entry never moves.
///
/// #416 could assert only the first of those transitions. Dephase's duration was
/// **3 [START]** and the activation spends the first of them, so a window held two
/// readable frames — the activation and one move — and the mark could be watched
/// lighting but never going out. #449 tuned the duration to **4**, which buys the
/// second move and with it the darkening: this test's missing arm.
///
/// **The leading wall frame is still out of reach, and the arithmetic says why.** A
/// window of N holds N − 1 moves, and the last of them is the turn the duration
/// *ends* — so a solid entered on that move is ejected out of before anything can be
/// read. The frames a phase can be observed standing still in are therefore the
/// activation plus N − 2 moves: two at N = 4, and the activation is necessarily on
/// floor, because a phase can only be begun somewhere a solid body already stands.
/// A literal `wall → floor → wall` needs three *post-activation* frames and so a
/// window of **5**. What it would prove beyond the two transitions below is that the
/// mark is not a one-shot latch, which the rule-equality test next door already pins
/// over every frame of a window.
#[test]
fn the_phase_mark_follows_the_cell_and_not_the_window() {
    // A wall with open floor either side, so one straight eastward walk crosses
    // floor → solid → floor without ever leaving the window.
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase))
    .with_rng(crate::Rng::new(7));
    let marked = |s: &State| s.effect_thing_marks().any(|cell| cell == s.player());

    s.step(Input::Activate(AbilityId::Dephase));
    assert!(
        s.abilities.effect_active(Effect::Phase),
        "precondition: the window is open — the bar reads Phase[n]",
    );
    assert!(!marked(&s), "on open floor the mark says nothing");

    // Frame by frame east: into the wall, then out the far side. The bar entry is the
    // *same* window throughout — only the cell changes, and the mark changes with it.
    // That is the disagreement §11.2 asks a marked effect for.
    for (step, cell, lit, note) in [
        (
            1,
            Cell::new(5, 4),
            true,
            "the board says the eject would fire if it ended here",
        ),
        (
            2,
            Cell::new(6, 4),
            false,
            "…and out the far side it goes dark again, window untouched",
        ),
    ] {
        s.step(Input::Step(Direction::East));
        assert_eq!(s.player(), cell, "step {step}: walked to {cell:?}");
        assert!(
            s.abilities.effect_active(Effect::Phase),
            "step {step}: the very same window, unchanged — so is the bar entry",
        );
        assert_eq!(marked(&s), lit, "step {step}: {note}");
    }
}

/// §11.5/#416: the mark is the eject **rule**, not a second derivation of it.
/// Asserted against [`can_rematerialize`](State::can_rematerialize) — the very
/// predicate the expiry consumes — over a run of mixed turns, so no later change to
/// the rule can leave the picture claiming a turn the rule would not.
#[test]
fn the_phase_mark_is_exactly_the_rematerialize_rule() {
    // Both answers are covered: the activation frame stands on floor (the rule says
    // yes, the mark stays dark) and the step lands in the solid (the rule says no,
    // the mark lights). Every frame of the window is checked, whichever way it falls.
    let mut layout = open_room(12, 12);
    layout.place(Cell::new(5, 4), Terrain::Wall);
    let mut s = State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(10, 10),
    )
    .with_loadout(Loadout::innate().with(AbilityId::Dephase))
    .with_rng(crate::Rng::new(7));

    let agrees = |s: &State| {
        assert_eq!(
            s.effect_thing_marks().any(|cell| cell == s.player()),
            !s.can_rematerialize(),
            "the mark and the eject rule are one fact",
        );
    };

    s.step(Input::Activate(AbilityId::Dephase));
    let mut seen_both = (false, false);
    for input in [Input::Step(Direction::East), Input::Step(Direction::West)] {
        if !s.abilities.effect_active(Effect::Phase) {
            break;
        }
        agrees(&s);
        match s.can_rematerialize() {
            true => seen_both.0 = true,
            false => seen_both.1 = true,
        }
        s.step(input);
    }
    assert_eq!(
        seen_both,
        (true, true),
        "the run must exercise the rule both ways, or it pins nothing",
    );
}

/// §8.3/#416/#339: the mark ends with its window — including on the very turn the
/// window ends inside a solid and the safety eject fires. The eject's own momentary
/// mark (#339) is a different thing on different cells and is left undisturbed.
#[test]
fn the_phase_mark_ends_with_its_window_even_when_the_eject_fires() {
    let mut s = phased_into_a_wall();
    assert!(
        s.effect_thing_marks().any(|cell| cell == s.player()),
        "precondition: marked while stranded",
    );

    s.step(Input::Wait); // the duration ends inside the wall — the eject fires
    let (from, to) = last_throw(&s);
    assert!(
        !s.abilities.effect_active(Effect::Phase),
        "precondition: the window ran out",
    );
    assert!(
        s.effect_thing_marks().next().is_none(),
        "the phase mark went with its window",
    );

    // The eject's report is untouched: both ends of the throw, on the cell layer.
    let cells: Vec<_> = s.effect_cell_marks().collect();
    assert!(
        cells.contains(&from) && cells.contains(&to),
        "the eject still lights both ends of its throw",
    );

    // Early toggle-off is the other way a window ends — refused inside a solid
    // (§8.3), so the only place it can be taken is a cell that was never marked.
    let mut s = phased_into_a_wall();
    s.step(Input::Step(Direction::West)); // out onto floor, where the toggle is legal
    s.step(Input::Deactivate(AbilityId::Dephase));
    assert!(
        !s.abilities.effect_active(Effect::Phase),
        "precondition: switched off",
    );
    assert!(s.effect_marks.is_empty(), "cleared, not merely quiet");
}
