use super::*;
use crate::ability::AbilityId;
use crate::cell::Direction;
use crate::place::LevelConfig;
use crate::state::{Event, Input, State};
use crate::status::{message_for, near_line, POP_IN_PRIORITY};
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
    assert_eq!(near_line(&state).priority, POP_IN_PRIORITY, "a loud action");
    state
}

/// The view state that ordinary play would be in a moment after that take.
fn showing_the_pop_in(state: &State) -> ScreenUi {
    ScreenUi {
        pop_in: crate::pop_in(state),
        ..ScreenUi::default()
    }
}

/// Every cell the pop-in changed about the frame — the box and nothing else, found by
/// difference rather than by hunting for glyphs, so a box drawn in the wrong place is
/// still found.
fn box_cells(state: &State, ui: ScreenUi) -> Vec<(u32, u32)> {
    let plain = render_screen(state, ScreenUi::default());
    let shown = render_screen(state, ui);
    (0..plain.height())
        .flat_map(|y| (0..plain.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| plain.get(x, y) != shown.get(x, y))
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
            let (x, y) = placement(board, player, WIDEST, TALLEST, |_, _| false)
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
    let quiet = placement(board, player, WIDEST, TALLEST, |_, _| false).expect("a placement");
    assert!(quiet.1 < player.1, "a quiet board puts the box above");

    let watched = placement(board, player, WIDEST, TALLEST, |_, y| y < player.1)
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
        Event::ExitRefused { still_needed: 9 },
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
        let lines = wrap(&message.text, TEXT_MAX);
        assert!(
            lines.len() <= MAX_LINES,
            "{:?} needs {} lines — widen the box or shorten the message",
            message.text,
            lines.len()
        );
        assert_eq!(
            lines.join(" "),
            message
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            "the wrap dropped words from {:?}",
            message.text
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
