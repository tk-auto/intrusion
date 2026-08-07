use super::*;
use crate::ability::AbilityId;
use crate::cell::Direction;
use crate::guard::Guard;
use crate::place::LevelConfig;
use crate::state::{Event, Input, State};
use crate::status::{
    live_messages, message_for, near_line, near_line_beside, PopIn, POP_IN_PRIORITY,
};
use crate::test_support::open_room;
use crate::{render_screen, ScreenUi};

/// The board a v1 run draws on (§10.2), in the screen coordinates [`placement`] works
/// in: 40 × 40 of facility, with the two status rows above it.
fn v1_board() -> Board {
    Board {
        width: LevelConfig::V1.width,
        top: hud::TOP_ROWS,
        height: LevelConfig::V1.height,
    }
}

/// The widest and tallest box the wrap can produce — what every geometry test measures
/// with, since a box that fits nowhere is only ever the biggest one.
const WIDEST: u32 = TEXT_MAX as u32 + FRAME_CELLS;
const TALLEST: u32 = MAX_LINES as u32 + EDGE_ROWS;

/// The cells one box covers.
fn covered(x: u32, y: u32, width: u32, height: u32) -> impl Iterator<Item = (u32, u32)> {
    (y..y + height).flat_map(move |cy| (x..x + width).map(move |cx| (cx, cy)))
}

/// A run whose last action **took the intel** — the objective feedback that raises a
/// pop-in (§11.7). The room is board-sized so the placement has somewhere real to go.
fn took_the_intel() -> State {
    let mut state = State::new(
        open_room(40, 20),
        Cell::new(5, 6),
        Direction::North,
        Vec::new(),
        [Cell::new(5, 5)],
        Cell::new(30, 15),
    );
    state.step(Input::Step(Direction::North));
    assert!(
        near_line(&state).priority >= POP_IN_PRIORITY,
        "a loud action"
    );
    state
}

/// The view state that ordinary play would be in a moment after that take.
fn showing_the_pop_in(state: &State) -> ScreenUi {
    ScreenUi {
        pop_in: crate::pop_in(state),
        ..ScreenUi::default()
    }
}

/// Every cell of the **board** the pop-in changed, found by difference rather than by
/// hunting for glyphs, so a box drawn in the wrong place is still found.
///
/// The status rows are excluded because the box changes those too, and deliberately: it
/// **takes** its message off the near line rather than copying it there
/// (`the_box_takes_its_message_off_the_near_line` is where that half is asserted).
fn box_cells(state: &State, ui: ScreenUi) -> Vec<(u32, u32)> {
    let plain = render_screen(state, ScreenUi::default());
    let shown = render_screen(state, ui);
    (hud::TOP_ROWS..plain.height())
        .flat_map(|y| (0..plain.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| plain.get(x, y) != shown.get(x, y))
        .collect()
}

/// The near line's words on a frame drawn with `ui`.
fn near_line_row(state: &State, ui: ScreenUi) -> String {
    let grid = render_screen(state, ui);
    (0..grid.width())
        .map(|x| grid.get(x, super::hud::NEAR_ROW).glyph)
        .collect()
}

/// §11.7/#576: the box covers neither the player's cell nor any of its eight
/// neighbours — the ring the console, crate or exit it is reporting is standing in —
/// and it is **clamped to the board**, never clipped off the edge of it.
///
/// Swept over every cell of a v1 board with the biggest box the wrap can make, which
/// also pins the other half of the promise: on a real board there is always somewhere
/// legal to put one, so a loud message is never silently dropped.
#[test]
fn no_placement_covers_the_player_or_a_neighbour() {
    let board = v1_board();
    for py in 0..LevelConfig::V1.height {
        for px in 0..LevelConfig::V1.width {
            let player = (px, board.top + py);
            let (x, y) = placement(board, player, WIDEST, TALLEST, |_, _| (0, 0))
                .unwrap_or_else(|| panic!("a v1 board always has room for a box at {player:?}"));
            assert!(
                fits(board, x, y, WIDEST, TALLEST),
                "the box at {:?} runs off the board from {player:?}",
                (x, y)
            );
            for (cx, cy) in covered(x, y, WIDEST, TALLEST) {
                assert!(
                    cx.abs_diff(player.0) > 1 || cy.abs_diff(player.1) > 1,
                    "the box at {:?} covers {:?}, in the ring around {player:?}",
                    (x, y),
                    (cx, cy)
                );
            }
        }
    }
}

/// §11.5/#576: among the legal placements the box takes the one covering the **fewest**
/// danger-overlay cells — the lose condition stays readable even while it is up.
///
/// The board here is watched everywhere *above* the player and nowhere below, which is
/// the arrangement that inverts the default: above leads on a quiet board, so a box that
/// still goes above is one ignoring the overlay entirely.
#[test]
fn the_box_gives_way_to_the_danger_overlay() {
    let board = v1_board();
    let player = (20, board.top + 20);
    let quiet = placement(board, player, WIDEST, TALLEST, |_, _| (0, 0)).expect("a placement");
    assert!(quiet.1 < player.1, "a quiet board puts the box above");

    let watched = placement(board, player, WIDEST, TALLEST, |_, y| {
        (0, usize::from(y < player.1))
    })
    .expect("a placement below the cones");
    assert!(
        watched.1 > player.1,
        "the box left the watched half of the board: {watched:?}"
    );
    for (x, y) in covered(watched.0, watched.1, WIDEST, TALLEST) {
        assert!(
            y >= player.1,
            "the box still covers a watched cell at {x},{y}"
        );
    }
}

/// §11.7/#576: a message on the top rung draws a box over the board, bordered in its own
/// §11.2 category, with its words in it — and one below the rung draws nothing at all.
#[test]
fn a_loud_message_draws_its_box_and_a_quiet_one_does_not() {
    let state = took_the_intel();
    let ui = showing_the_pop_in(&state);
    let message = near_line(&state);
    let cells = box_cells(&state, ui);
    assert!(!cells.is_empty(), "the take raised a box");

    let shown = render_screen(&state, ui);
    let frame: Vec<char> = cells
        .iter()
        .map(|&(x, y)| shown.get(x, y).glyph)
        .filter(|&glyph| {
            matches!(
                glyph,
                CORNER_TOP_LEFT | CORNER_TOP_RIGHT | CORNER_BOTTOM_LEFT | CORNER_BOTTOM_RIGHT
            )
        })
        .collect();
    assert_eq!(frame.len(), 4, "one box, four corners: {frame:?}");
    for &(x, y) in &cells {
        assert_eq!(
            shown.get(x, y).bg,
            None,
            "the box carries the screen's own fill at {x},{y}"
        );
    }
    let corner = cells
        .iter()
        .find(|&&(x, y)| shown.get(x, y).glyph == CORNER_TOP_LEFT)
        .expect("a top-left corner");
    assert_eq!(
        shown.get(corner.0, corner.1).fg,
        message.category,
        "the border wears the message's category"
    );

    // The words are on the board, wrapped: the first line of the wrap reads back off the
    // row under the top edge.
    let first = wrap(&message.text, TEXT_MAX).remove(0);
    let row: String = (0..shown.width())
        .map(|x| shown.get(x, corner.1 + 1).glyph)
        .collect();
    assert!(row.contains(&first), "the box says the message: {row:?}");

    // **The gate is the ladder and nothing else.** A message that speaks — a real
    // message, on the near line, in its own band — but sits below the rung raises no
    // box: slipping into a cupboard is self-narration at 0.
    let mut layout = open_room(40, 20);
    layout.place(Cell::new(5, 5), crate::Terrain::Hideout);
    let mut quiet = State::new(
        layout,
        Cell::new(5, 6),
        Direction::North,
        Vec::new(),
        [Cell::new(30, 5)],
        Cell::new(30, 15),
    );
    quiet.step(Input::Step(Direction::North));
    let said = near_line(&quiet);
    assert!(
        !said.is_ambient() && said.priority < POP_IN_PRIORITY,
        "a message below the rung: {said:?}"
    );
    assert!(crate::pop_in(&quiet).is_none(), "and it raises no box");
    assert!(
        box_cells(&quiet, showing_the_pop_in(&quiet)).is_empty(),
        "nothing is drawn without a pop-in"
    );
}

/// §11.7/#576: the box **rides out its life**. The near line's copy clears on the
/// player's next action, as §11.7 requires; the box does not — a player who acts inside
/// two seconds is the very case it exists for, so a key-press that erased it would fail
/// its only job. Both halves of the divergence are pinned here.
#[test]
fn the_box_outlives_the_near_lines_copy() {
    let mut state = took_the_intel();
    let held = showing_the_pop_in(&state);
    assert!(held.pop_in.is_some(), "the take raised one");

    state.step(Input::Step(Direction::South));
    assert!(
        near_line(&state).is_ambient(),
        "the near line cleared to the ambient floor"
    );
    assert!(
        crate::pop_in(&state).is_none(),
        "and the next action raised no pop-in of its own"
    );
    assert!(
        !box_cells(&state, held).is_empty(),
        "the box the shell still holds is still drawn"
    );
}

/// §11.7/#576: a second qualifying message **replaces** the first — one box at any time,
/// always the newest fact, and the replaced one leaves no trace. The ladder's own
/// tie-break decides which of two loud events on one turn it is: the later leads, as it
/// does on the near line, so the band and the box can never name different facts.
#[test]
fn a_second_loud_message_replaces_the_first() {
    // Two consoles either side of the player: two takes, two turns, two loud facts.
    let mut state = State::new(
        open_room(40, 20),
        Cell::new(5, 6),
        Direction::North,
        Vec::new(),
        [Cell::new(5, 5), Cell::new(4, 6)],
        Cell::new(30, 15),
    );
    state.step(Input::Step(Direction::North));
    let first = crate::pop_in(&state).expect("the first take");
    state.step(Input::Step(Direction::West));
    let second = crate::pop_in(&state).expect("a second loud message");
    assert_ne!(
        first.message().text,
        second.message().text,
        "the newer fact replaces the older"
    );
    assert_eq!(
        second.message().text,
        near_line(&state).text,
        "and it is the one the near line is speaking"
    );

    let shown = render_screen(
        &state,
        ScreenUi {
            pop_in: Some(second),
            ..ScreenUi::default()
        },
    );
    let corners = (0..shown.height())
        .flat_map(|y| (0..shown.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| shown.get(x, y).glyph == CORNER_TOP_LEFT)
        .count();
    assert_eq!(corners, 1, "one box is drawn, never a stack of them");
}

/// §11.7/#576: **every** message that can pop in fits the box it is drawn in.
///
/// [`wrap`] truncates past [`MAX_LINES`] and the box is laid over the board rather than
/// across it, so an over-long message would arrive silently short — the §2.3 failure the
/// near line's own width test exists to catch, and the reason this one walks the events
/// rather than measuring a `const`: nearly every message on this rung is built with
/// `format!` from an ability's name or a console count.
///
/// The list is hand-maintained, like [`every_near_line_message_fits`]'s: an event added
/// to the rung without a line here is measured by nothing. The ability-carrying ones
/// sweep the whole catalogue, so the longest bar name is always in the sample.
///
/// [`every_near_line_message_fits`]: super::super::hud::tests
#[test]
fn every_pop_in_message_fits_its_box() {
    let at = Cell::new(3, 3);
    let mut events = vec![
        // The escalation rung the threshold sits on, every rung of the §7.3 ladder.
        Event::AlertRaised {
            rung: 1,
            trigger: crate::AlertTrigger::Sighting,
        },
        Event::AlertRaised {
            rung: 3,
            trigger: crate::AlertTrigger::RepeatSightings,
        },
        Event::BodyCalledIn { at },
        // …and the rungs above it, up to the endings.
        Event::Ejected {
            from: at,
            to: at,
            stunned: crate::phase_eject_stun(1),
        },
        Event::CaptureSaved { at },
        Event::Captured {
            guard: 0,
            state: crate::GuardState::Chasing,
            at,
        },
        Event::Entombed { at },
        Event::IntelTaken {
            remaining: 0,
            still_needed: 0,
        },
        Event::IntelTaken {
            remaining: 9,
            still_needed: 0,
        },
        Event::IntelTaken {
            remaining: 9,
            still_needed: 9,
        },
        // Both refusal wordings (#584): a count under `All`, and the gate that asks for
        // any one thing.
        Event::ExitRefused {
            still_needed: 9,
            gate: crate::IntelGate::All,
        },
        Event::ExitRefused {
            still_needed: 1,
            gate: crate::IntelGate::AtLeastOne,
        },
        Event::CommsSilenced { at },
        Event::KeyTaken { at },
        Event::Won,
    ];
    for id in AbilityId::ALL {
        events.extend([
            Event::TechSalvaged { id },
            Event::SalvageRefused { id },
            Event::UsesRecharged { id, uses: 3 },
            Event::ExchangeOffered { id },
            Event::ExchangeDeclined { id },
        ]);
        for dropped in AbilityId::ALL {
            events.push(Event::Traded { taken: id, dropped });
        }
    }
    for event in events {
        let message = message_for(event).expect("a loud event speaks");
        assert!(
            message.priority >= POP_IN_PRIORITY,
            "{event:?} is not on the rung this test measures"
        );
        // The block as the box draws it: the headline, and the §11.7 subordinate under
        // it where the event carries one (#418) — both have to fit, together.
        let mut lines = wrap(&message.text, TEXT_MAX);
        let reason = PopIn::of(event).and_then(PopIn::reason);
        if let Some(reason) = &reason {
            lines.extend(wrap(&reason.text, TEXT_MAX));
        }
        assert!(
            lines.len() <= MAX_LINES,
            "{:?} needs {} lines — widen the box or shorten the message",
            message.text,
            lines.len()
        );
        let whole = match &reason {
            Some(reason) => format!("{} {}", message.text, reason.text),
            None => message.text.clone(),
        };
        assert_eq!(
            lines.join(" "),
            whole.split_whitespace().collect::<Vec<_>>().join(" "),
            "the wrap dropped words from {whole:?}"
        );
        let width = lines
            .iter()
            .map(|l| l.chars().count() as u32)
            .max()
            .unwrap()
            + FRAME_CELLS;
        assert!(
            width <= LevelConfig::V1.width,
            "the box for {:?} is wider than the board",
            message.text
        );
    }
}

/// A word longer than a whole line is **cut**, not dropped: no message on the rung has
/// one — the test above proves it — but a line that silently vanished would be worse
/// than a broken word, and the fallback is what makes that guarantee cheap.
#[test]
fn an_unbreakable_word_is_cut_rather_than_lost() {
    assert_eq!(
        wrap("rematerialization", 8),
        ["remateri", "alizatio", "n"],
        "a word past the line's width is cut across lines, never dropped"
    );
}

/// §11.7/#576: the box **takes** its message rather than copying it. While it is up the
/// near line does not repeat it — the row falls through to the next live message, or to
/// the ambient floor when the box has taken the only one there was — and the message is
/// back on the row the moment the box goes.
#[test]
fn the_box_takes_its_message_off_the_near_line() {
    let state = took_the_intel();
    let spoken = near_line(&state).text;
    assert!(
        near_line_row(&state, ScreenUi::default()).contains(&spoken),
        "with no box the row speaks it as it always did"
    );

    let ui = showing_the_pop_in(&state);
    let row = near_line_row(&state, ui);
    assert!(
        !row.contains(&spoken),
        "the box has it, so the row does not: {row:?}"
    );
    assert!(
        near_line_beside(&state, ui.pop_in).is_ambient(),
        "and with nothing else live the row falls to the ambient floor"
    );

    // Two facts on one turn: the box takes the loudest and hands the row the next. Take
    // the intel under a guard's nose, and the turn says both *intel in hand* and *a guard
    // has seen you*.
    let mut both = State::new(
        open_room(40, 20),
        Cell::new(5, 6),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 3))],
        // West of the player, so the console is not standing between the guard and them.
        [Cell::new(4, 6)],
        Cell::new(30, 15),
    );
    // One turn under the guard's eye for the §7.6 sighting window to close, then take the
    // intel: the turn lands the objective *and* the rung the facility climbed for it.
    both.step(Input::Wait);
    both.step(Input::Step(Direction::West));
    let messages = live_messages(&both);
    assert!(messages.len() > 1, "the turn said more than one thing");
    let beside = near_line_beside(&both, crate::pop_in(&both));
    assert_ne!(
        beside.text, messages[0].text,
        "the row is not repeating what the box has"
    );
    assert_eq!(
        beside.text, messages[1].text,
        "it speaks the next fact down instead"
    );
}

/// A run the **facility has just noticed**: a guard holds the player in its cone until
/// the §7.6 sighting window closes, which climbs the §7.3 ladder a rung. The one event
/// that carries a §11.7 subordinate (#418), and the reason the rung came down to 5.
fn the_facility_climbed() -> State {
    let mut state = State::new(
        open_room(40, 20),
        Cell::new(5, 6),
        Direction::North,
        vec![Guard::stationary(Cell::new(5, 3))],
        [Cell::new(15, 15)],
        Cell::new(30, 18),
    );
    for _ in 0..8 {
        state.step(Input::Wait);
        if crate::pop_in(&state).is_some_and(|popped| popped.reason().is_some()) {
            return state;
        }
    }
    panic!("a guard staring at the player climbs the ladder within a few turns");
}

/// §11.7/#576: **the facility climbing a rung pops in.** It is the fact that most
/// changes what the next ten turns should be, and it was arriving on the row the player
/// was not reading — which is why [`POP_IN_PRIORITY`] sits at the escalation rung rather
/// than at objective feedback.
///
/// And it brings its **reason** with it (#418): the pair is inseparable, so with the
/// headline off the near line the *why* cannot be left behind on its own.
#[test]
fn the_facility_climbing_pops_in_and_brings_its_reason() {
    let state = the_facility_climbed();
    let popped = crate::pop_in(&state).expect("the raise pops in");
    let headline = popped.message();
    assert!(
        headline.text.starts_with("security condition"),
        "{headline:?}"
    );
    let reason = popped.reason().expect("a closed sighting says why");

    let ui = ScreenUi {
        pop_in: Some(popped),
        ..ScreenUi::default()
    };
    let shown = render_screen(&state, ui);
    let board: String = box_cells(&state, ui)
        .iter()
        .map(|&(x, y)| shown.get(x, y).glyph)
        .collect();
    for word in [headline.text.as_str(), reason.text.as_str()] {
        let joined: String = word.split_whitespace().collect::<Vec<_>>().join("");
        let drawn: String = board.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            drawn.contains(&joined),
            "the box draws {word:?}; it drew {board:?}"
        );
    }
}

/// §11.7/#576: what the box takes, it takes from the **deployed log** too — the same
/// words in two places at once is one fact wearing two surfaces. The block goes whole,
/// headline and reason together, and is back at its expected offset the moment the box
/// goes. The history stacked under the rule is never touched.
#[test]
fn the_log_gives_up_the_block_the_box_is_holding() {
    let state = the_facility_climbed();
    let popped = crate::pop_in(&state).expect("the raise pops in");
    let reason = popped.reason().expect("with a reason under it");

    // Asserted on the log's own rows rather than on the painted frame: the box is drawn
    // over the board too, so a frame-wide search would find the block's words in the box
    // and call that the log listing them.
    let listed = |popped| format!("{:?}", super::super::message_log::log_rows(&state, popped));
    assert!(
        listed(None).contains(&reason.text),
        "with no box up, the log lists the reason under the near line's headline: {}",
        listed(None)
    );
    assert!(
        !listed(Some(popped)).contains(&reason.text),
        "with the box holding the block, the log does not list it as well: {}",
        listed(Some(popped))
    );
    // …and the near line is not repeating the headline either: the whole block is the
    // box's for as long as the box is up.
    assert!(
        !near_line_row(
            &state,
            ScreenUi {
                pop_in: Some(popped),
                ..ScreenUi::default()
            }
        )
        .contains(popped.message().text.as_str()),
        "the row has given the headline up as well"
    );
}

/// §11.5/§9.2/#576: the box does not sit on a **guard**. A guard's own cell is never in
/// its own cone, so the danger count alone cannot see this — and covering it is the
/// overlay failure in its worst form: the cone stays perfectly readable while the guard
/// has vanished from the middle of it.
///
/// The staged frame is the one that found it. With a guard three cells north of the
/// player and its cone fanning south, the quietest placement by danger cells alone is
/// *above* — straight on top of the `g` that just raised the alert the box is announcing.
#[test]
fn the_box_does_not_sit_on_a_guard() {
    let state = the_facility_climbed();
    let ui = showing_the_pop_in(&state);
    let plain = render_screen(&state, ScreenUi::default());
    let guards: Vec<(u32, u32)> = (hud::TOP_ROWS..plain.height())
        .flat_map(|y| (0..plain.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| plain.get(x, y).glyph == GUARD_GLYPH)
        .collect();
    assert!(!guards.is_empty(), "the fixture has a guard on screen");

    let covered = box_cells(&state, ui);
    for guard in guards {
        assert!(
            !covered.contains(&guard),
            "the box covers the guard at {guard:?}"
        );
    }
}
