//! The level-start splash, pinned as text (§11.1/§12.1/#497).
//!
//! Every screen in this crate prints as a grid of characters, which is what makes the
//! card assertable without a browser: these tests read the rows the player reads, on the
//! first frame of a run.

use super::*;
use crate::cell::Direction;
use crate::input::{dismisses_splash, gesture_dismisses_splash, Gesture};
use crate::modifiers::{
    CacheCount, Composite, GuardCount, IntelCount, IntelGate, LayoutKnowledge, LevelModifiers,
};
use crate::place::LevelConfig;
use crate::render::help::{HelpTab, CONTENT_INDENT, SECTION_INDENT};
use crate::render::modifier_rows::{modifier_rows, NONE_ACTIVE};
use crate::render::objective::{take_line, EXIT_LINE, NO_INTEL};
use crate::render::{render_screen, ScreenUi, BOTTOM_ROWS, TOP_ROWS};
use crate::state::State;
use crate::test_support::{leave_by_the_tunnel, open_room, room_with_tunnel};
use crate::Cell;

/// **The v1 board itself** (§10.2), because the card's whole claim is that it fits the
/// screen a real run is played on without pushing the map around (§11.4 [SETTLED]) —
/// so the tests are run against that board and not a convenient smaller one.
const BOARD: (u32, u32) = (LevelConfig::V1.width, LevelConfig::V1.height);

/// A run standing in a bare room, with `intel` consoles placed in it — the fixture the
/// card draws about. The consoles are cells, not terrain: what the card reads is the
/// objective list's length, which is what [`State::intel_total`] answers.
fn run_with(intel: usize, modifiers: LevelModifiers) -> State {
    run_holding(intel, 0, modifiers)
}

/// [`run_with`] with `caches` equipment crates stamped in the room as well (§8.3/#209) —
/// the campaign's facility, and the case the minimum haul's line is written for (#574).
fn run_holding(intel: usize, caches: usize, modifiers: LevelModifiers) -> State {
    let objectives: Vec<Cell> = (0..intel as u32).map(|i| Cell::new(10 + i, 10)).collect();
    let mut layout = open_room(BOARD.0, BOARD.1);
    for i in 0..caches as u32 {
        layout.place(
            Cell::new(10 + i, 14),
            crate::facility::Terrain::EquipmentCache,
        );
    }
    State::new(
        layout,
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        objectives,
        Cell::new(8, 8),
    )
    .with_caches((0..caches).map(|_| crate::AbilityId::Confusion))
    .with_modifiers(modifiers)
}

/// Every §12.6 rule in force at once, a composite among them — the longest modifier
/// list a card can be asked to draw (#565's attributed rows plus the departures beyond
/// them).
fn every_modifier() -> LevelModifiers {
    LevelModifiers {
        guards_always_search_hideouts: true,
        sighting_lost_calls_a_guard: true,
        body_found_calls_two_guards: true,
        always_show_vision_cones: true,
        layout_knowledge: LayoutKnowledge::Full,
        automatic_doors: true,
        guards_watch_consoles: true,
        show_search_areas: true,
        guard_count: GuardCount::TwoMore,
        intel_count: IntelCount::Fewer,
        caches: CacheCount::Three,
        prize_room_locked: true,
        narrowed_guard_cones: true,
        scouted: true,
        guards_watch_their_sides: true,
        intel_to_exit: IntelGate::None,
        composite: Composite::Vault,
        ..LevelModifiers::default()
    }
}

/// The view state a fresh facility opens on — the card up, everything else clean.
fn opening() -> ScreenUi {
    ScreenUi::default().for_fresh_run()
}

/// The frame as text, under a view state.
fn screen(state: &State, ui: ScreenUi) -> Vec<String> {
    render_screen(state, ui).to_text()
}

/// Whether any row of the frame contains `text`.
fn shows(rows: &[String], text: &str) -> bool {
    rows.iter().any(|row| row.contains(text))
}

/// The **column** `text` starts at on `row`, counting cells rather than bytes — the
/// card's frame is box-drawing, so a byte offset is not a column.
fn column_of(row: &str, text: &str) -> u32 {
    let byte = row
        .find(text)
        .unwrap_or_else(|| panic!("{text:?} is on {row:?}"));
    row[..byte].chars().count() as u32
}

/// The row index carrying `text`.
fn row_of(rows: &[String], text: &str) -> u32 {
    rows.iter()
        .position(|row| row.contains(text))
        .unwrap_or_else(|| panic!("{text:?} is on the screen: {rows:#?}")) as u32
}

/// **The golden card** (§14 v1's golden grid tests): a quick-play facility — three
/// consoles, all of them wanted (#244) — drawn whole. The block is the two sections and
/// nothing else: no seed token, no `copy [c]`, no facility alert. The board reads above
/// and below it, because the card is an overlay and §11.4 is [SETTLED] that the screen
/// is the board.
#[test]
fn the_level_start_card_renders_golden() {
    let state = run_with(
        3,
        LevelModifiers {
            intel_to_exit: IntelGate::All,
            ..LevelModifiers::default()
        },
    );
    let rows = screen(&state, opening());
    let top = row_of(&rows, HEADING) - 2; // the rule and the blank above the heading
    let card: Vec<&str> = rows[top as usize..top as usize + 14]
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(
        card,
        vec![
            "┌──────────────────────────────────────┐",
            "│                                      │",
            "│ THE JOB                              │",
            "│                                      │",
            "│ OBJECTIVE                            │",
            "│  Take all 3 intel                    │",
            "│  Get back out through your tunnel    │",
            "│                                      │",
            "│ MODIFIERS                            │",
            "│  Intel to exit: all of it            │",
            "│                                      │",
            "│ any key to begin                     │",
            "│                                      │",
            "└──────────────────────────────────────┘",
        ],
        "the card: {rows:#?}",
    );

    // The board is still there behind it: the card is centred in the map area, so the
    // facility the player is about to walk into reads above and below.
    assert!(top > TOP_ROWS, "board above the card: {rows:#?}");
    assert!(
        rows.len() as u32 - (top + 14) > 1,
        "board below the card: {rows:#?}",
    );
}

/// **The objective is stated for every gate, baseline included** (§4.5/§12.6/#497).
///
/// This is the case that cannot be read off [`LevelModifiers::active`]: at
/// [`IntelGate::AtLeastOne`] the modifier list says *nothing* about the gate — it is the
/// §4.5 baseline — so a card derived from that list alone would have a run's whole
/// objective missing. Each gate's line is distinct, and every one of them says how to
/// leave as well as what to take (§1: there is no other way out).
#[test]
fn every_intel_gate_states_its_own_objective() {
    let mut lines: Vec<String> = Vec::new();
    for gate in [IntelGate::All, IntelGate::AtLeastOne, IntelGate::None] {
        let state = run_with(
            3,
            LevelModifiers {
                intel_to_exit: gate,
                ..LevelModifiers::default()
            },
        );
        let rows = screen(&state, opening());
        let line = take_line(gate, 3, 0);
        assert!(shows(&rows, &line), "{gate:?} draws {line:?}: {rows:#?}");
        assert!(shows(&rows, EXIT_LINE), "{gate:?} says how to leave");
        assert!(
            !lines.contains(&line),
            "{gate:?} reuses another gate's line"
        );
        lines.push(line);
    }
    // The baseline gate is the one the modifier list is silent about, which is the whole
    // reason the objective is derived from the run rather than from that list.
    let baseline = LevelModifiers::default();
    assert_eq!(baseline.intel_to_exit, IntelGate::AtLeastOne);
    assert!(
        baseline.active().is_empty(),
        "the baseline run has nothing to say through `active`",
    );
    let rows = screen(&run_with(3, baseline), opening());
    assert!(shows(&rows, &take_line(IntelGate::AtLeastOne, 3, 0)));
    assert!(
        shows(&rows, NONE_ACTIVE),
        "…and reads as baseline: {rows:#?}"
    );
}

/// The console **count** is the facility's own, and a facility with nothing in it says
/// so rather than dropping the row (§2.3 — a card that quietly omitted its objective
/// would read as broken).
#[test]
fn the_objective_counts_this_facilitys_own_consoles() {
    for intel in [1, 2, 5] {
        let state = run_with(
            intel,
            LevelModifiers {
                intel_to_exit: IntelGate::All,
                ..LevelModifiers::default()
            },
        );
        assert_eq!(state.intel_total(), intel);
        let rows = screen(&state, opening());
        assert!(
            shows(&rows, &take_line(IntelGate::All, intel, 0)),
            "{intel} consoles: {rows:#?}",
        );
    }
    // Nothing to take, under every gate: the exit is vacuously open (§4.5) and the row
    // names the emptiness instead of a number.
    for gate in [IntelGate::All, IntelGate::AtLeastOne, IntelGate::None] {
        let rows = screen(
            &run_with(
                0,
                LevelModifiers {
                    intel_to_exit: gate,
                    ..LevelModifiers::default()
                },
            ),
            opening(),
        );
        assert!(shows(&rows, NO_INTEL), "{gate:?} on an empty facility");
    }
}

/// **One source, two views, for the objective too** (§11.2/§11.3/#574): the gate the
/// card states before the first turn is the gate the Level info tab states on turn forty,
/// in the same words — one derivation ([`crate::render::objective`]), so the two surfaces
/// cannot come to describe different exits.
///
/// The Level info tab did not carry the objective at all until #574. It could afford not
/// to while every human-facing mode's gate was a *departure* the modifier list surfaced
/// on its own; the minimum haul put the campaign on §4.5's baseline gate, where the list
/// is silent and the panel would have named no exit rule whatsoever.
#[test]
fn the_cards_objective_agrees_with_the_level_info_tab() {
    for gate in [IntelGate::All, IntelGate::AtLeastOne, IntelGate::None] {
        for (intel, caches) in [(3, 0), (2, 1), (5, 3)] {
            let state = run_holding(
                intel,
                caches,
                LevelModifiers {
                    intel_to_exit: gate,
                    ..LevelModifiers::default()
                },
            );
            assert_eq!(state.cache_total(), caches, "the crates are stamped");
            let card = screen(&state, opening());
            let tab = screen(
                &state,
                ScreenUi {
                    help_open: true,
                    help_tab: HelpTab::LevelInfo,
                    ..ScreenUi::default()
                },
            );
            let line = take_line(gate, intel, caches);
            let what = format!("{gate:?} over {intel} consoles and {caches} crates");
            assert!(shows(&card, &line), "{what}: the card says {line:?}");
            assert!(shows(&tab, &line), "{what}: the tab says {line:?}");
            assert!(shows(&card, EXIT_LINE) && shows(&tab, EXIT_LINE));
        }
    }
}

/// **One source, two views** (§11.2/§11.3/#497): the card's modifier rows are the Level
/// info tab's, for the same run — every active rule on both, in the same words, with the
/// same §11.2 direction colour, and no rule on one that is missing from the other.
#[test]
fn the_cards_modifier_rows_agree_with_the_level_info_tab() {
    for modifiers in [
        LevelModifiers::default(),
        LevelModifiers {
            guards_always_search_hideouts: true,
            intel_to_exit: IntelGate::All,
            guard_count: GuardCount::More,
            ..LevelModifiers::default()
        },
        // The whole set at once, composite and all — the longest list either surface can
        // draw, so the agreement is asserted where it is hardest to hold.
        every_modifier(),
    ] {
        let state = run_with(3, modifiers);
        let card = screen(&state, opening());
        let tab = screen(
            &state,
            ScreenUi {
                help_open: true,
                help_tab: HelpTab::LevelInfo,
                ..ScreenUi::default()
            },
        );
        let expected = modifier_rows(modifiers);
        assert!(!expected.is_empty(), "there is always at least one row");
        for (text, category) in &expected {
            assert!(shows(&card, text), "the card lists {text:?}: {card:#?}");
            assert!(shows(&tab, text), "the tab lists {text:?}: {tab:#?}");
            // …and in the same colour: the row is drawn from the content indent on both,
            // so the cue a player learns on one surface reads the same on the other.
            let grid = render_screen(&state, opening());
            let y = row_of(&card, text);
            assert_eq!(
                grid.get(CONTENT_INDENT, y).fg,
                *category,
                "{text:?} wears its §11.2 direction colour",
            );
        }
        // Neither surface invents a row the other does not have.
        let listed = |rows: &[String]| {
            expected
                .iter()
                .filter(|(text, _)| shows(rows, text))
                .count()
        };
        assert_eq!(listed(&card), expected.len());
        assert_eq!(listed(&tab), expected.len());
    }
}

/// **The card is a *reduced* tab** (#497): the three sections that belong to the panel
/// you can call up are not on it — the level-seed token and its `copy [c]` control
/// (§13.1/#353), and the facility alert (§7.3/#375), which is the one section that moves
/// while you play and has nothing to say before the first turn.
#[test]
fn the_card_carries_no_seed_token_no_copy_control_and_no_alert() {
    let level = crate::level_seed::LevelSeed::quick_play(8371);
    let state = run_with(3, LevelModifiers::default()).with_level(level);
    let token = level.encode().expect("a quick-play run has a token");
    let rows = screen(&state, opening());

    assert!(shows(&rows, HEADING), "the card is up: {rows:#?}");
    for absent in [token.as_str(), "LEVEL SEED", "copy [c]", "ALERT"] {
        assert!(
            !shows(&rows, absent),
            "{absent:?} is not on the card: {rows:#?}"
        );
    }
    // …and the tab still has all three, so what the card dropped it dropped for itself.
    let tab = screen(
        &state,
        ScreenUi {
            help_open: true,
            help_tab: HelpTab::LevelInfo,
            ..ScreenUi::default()
        },
    );
    for present in [token.as_str(), "LEVEL SEED", "copy [c]"] {
        assert!(shows(&tab, present), "the tab keeps {present:?}: {tab:#?}");
    }
}

/// **A fresh facility opens on the card, and nothing else does** (#473/#497). The flag
/// is raised at the one seam every fresh run crosses, and the [`Default`] view state — a
/// hand-built board, a test, the replay viewer — draws the frame it always drew.
#[test]
fn a_fresh_run_opens_on_the_card_and_a_default_view_does_not() {
    let state = run_with(3, LevelModifiers::default());
    assert!(opening().splash_open, "a fresh facility opens on its card");
    assert!(
        !ScreenUi::default().splash_open,
        "the default view state has no card up",
    );
    assert!(shows(&screen(&state, opening()), HEADING));
    assert!(!shows(&screen(&state, ScreenUi::default()), HEADING));
}

/// **The card costs no turn and changes no world** (§4.4/§11.6): it is view state, so
/// the frame under it is the same frame — dismissing it draws the board that was always
/// there, on turn zero, with nothing spent.
#[test]
fn the_card_is_a_view_over_an_untouched_first_frame() {
    let state = run_with(3, LevelModifiers::default());
    let under = screen(&state, ScreenUi::default());
    let over = screen(&state, opening());
    assert_ne!(under, over, "the card is drawn");

    // Dismissing is a flag, not an input: the state is untouched, so the frame beneath
    // is restored exactly — which is also why the dismissal never enters a replay's
    // recorded stream (§12.4).
    let mut ui = opening();
    ui.splash_open = false;
    assert_eq!(screen(&state, ui), under, "the frame beneath is restored");
}

/// **Every input dismisses it, and only the bare modifiers do not** (§11.6/#497).
///
/// The card carries no control, so there is nothing to aim at and nothing to walk: the
/// keys the game owns, the keys it does not, and every gesture all answer the one
/// question it asks. Holding Shift or reaching for Control does not — those are keydowns
/// the player did not mean as an answer.
#[test]
fn every_input_dismisses_the_card_except_a_bare_modifier() {
    for key in [
        "ArrowUp",
        "ArrowDown",
        "w",
        ".",
        "1",
        "2",
        "?",
        "m",
        "n",
        "c",
        "Enter",
        " ",
        "Escape",
        "Tab",
        "F5",
        "q",
    ] {
        assert!(dismisses_splash(key), "{key:?} dismisses the card");
    }
    for key in ["Shift", "Control", "Alt", "Meta", "AltGraph", "CapsLock"] {
        assert!(!dismisses_splash(key), "{key:?} is not an answer");
    }
    // The touch half: a press and every swipe, because a finger cannot miss a card with
    // no control on it (§11.6's no-trap rule, satisfied by everything working).
    assert!(gesture_dismisses_splash(Gesture::Press));
    for direction in Direction::ALL {
        assert!(gesture_dismisses_splash(Gesture::Swipe(direction)));
    }
}

/// The dismissal hint speaks the player's own modality (§11.6/#323) — the same choice
/// the usable line's floor makes, and the only thing on the card that varies with it.
#[test]
fn the_hint_is_worded_for_the_hands_in_use() {
    let state = run_with(3, LevelModifiers::default());
    for (modality, hint, other) in [
        (InputModality::Keys, BEGIN_KEYS, BEGIN_TOUCH),
        (InputModality::Touch, BEGIN_TOUCH, BEGIN_KEYS),
    ] {
        let rows = screen(
            &state,
            ScreenUi {
                modality,
                ..opening()
            },
        );
        assert!(shows(&rows, hint), "{modality:?}: {rows:#?}");
        assert!(!shows(&rows, other), "{modality:?} teaches one vocabulary");
    }
}

/// **The card is a box, walled on all four sides** (#497).
///
/// Bounded by two horizontal rules alone it read as a *cut through the level* — board
/// above, board below, and no way to tell at a glance that the middle was one object
/// laid on top rather than the facility itself. The sides and the corners are what say
/// *dialog*, and the first frame of a run is the worst possible place to be unsure what
/// you are looking at.
///
/// Asserted over the longest card as well as the shortest, because the sides are drawn
/// per row: a row the frame forgot would be a hole in exactly the cue this is for.
#[test]
fn the_card_is_a_box_walled_on_every_row() {
    for modifiers in [LevelModifiers::default(), every_modifier()] {
        let rows = screen(&run_with(3, modifiers), opening());
        let top = row_of(&rows, HEADING) - 2;
        let bottom = rows
            .iter()
            .rposition(|row| row.starts_with(CORNER_BOTTOM_LEFT))
            .expect("the card closes on its bottom edge") as u32;
        assert!(bottom > top, "the card has a top and a bottom: {rows:#?}");

        for y in top..=bottom {
            let row = &rows[y as usize];
            let (first, last) = (
                row.chars().next().expect("a full-width row"),
                row.chars().last().expect("a full-width row"),
            );
            let expected = if y == top {
                (CORNER_TOP_LEFT, CORNER_TOP_RIGHT)
            } else if y == bottom {
                (CORNER_BOTTOM_LEFT, CORNER_BOTTOM_RIGHT)
            } else {
                (SIDE_GLYPH, SIDE_GLYPH)
            };
            assert_eq!(
                (first, last),
                expected,
                "row {y} of the card is walled on both sides: {rows:#?}",
            );
        }
        // …and the walls cost the words nothing: every drawn row still starts at the
        // card's own indent, clear of the side beside it.
        assert_eq!(
            column_of(&rows[row_of(&rows, HEADING) as usize], HEADING),
            SECTION_INDENT
        );
        assert_eq!(
            column_of(&rows[row_of(&rows, EXIT_LINE) as usize], EXIT_LINE),
            CONTENT_INDENT,
        );
    }
}

/// The card's own headings are drawn in the Level info tab's columns — the two cards
/// read as one family, which is what makes the reduced one legible to a player who has
/// seen the full one (and the reverse).
#[test]
fn the_card_is_laid_out_in_the_tabs_own_columns() {
    let state = run_with(3, LevelModifiers::default());
    let grid = render_screen(&state, opening());
    let rows = grid.to_text();
    for heading in [HEADING, OBJECTIVE_HEADING, MODIFIERS_HEADING] {
        let y = row_of(&rows, heading);
        assert_eq!(
            column_of(&rows[y as usize], heading),
            SECTION_INDENT,
            "{heading:?} sits at the section indent",
        );
    }
    let objective = row_of(&rows, EXIT_LINE);
    assert_eq!(
        column_of(&rows[objective as usize], EXIT_LINE),
        CONTENT_INDENT,
        "content sits one column in from the heading that names it",
    );
    // The headings' colours: the card's own title in Interest — the goal colour, since
    // the job is the goal — and the sections in the System tan every card labels with.
    assert_eq!(
        grid.get(SECTION_INDENT, row_of(&rows, HEADING)).fg,
        Category::Interest
    );
    for heading in [OBJECTIVE_HEADING, MODIFIERS_HEADING] {
        assert_eq!(
            grid.get(SECTION_INDENT, row_of(&rows, heading)).fg,
            Category::System,
        );
    }
}

/// **A finished run's verdict still wins** (§14 v2/#138): the card is laid on before it,
/// so nothing from a turn that never happened can sit on top of the one thing a finished
/// run has left to say. It is not a case a live run can reach — the flag is down long
/// before a verdict exists — but the paint order is what guarantees that, rather than an
/// argument about which flags can be set together.
#[test]
fn the_verdict_is_drawn_over_the_card_and_not_under_it() {
    // No consoles, so the gate is vacuously satisfied (§4.5) and the player climbs into
    // the tunnel beside them and crawls out (#466) — the verdict fixture, unchanged.
    let mut state = State::new(
        room_with_tunnel(BOARD.0, BOARD.1, Cell::new(5, 4), Direction::East),
        Cell::new(4, 4),
        Direction::North,
        Vec::new(),
        Vec::new(),
        Cell::new(5, 4),
    );
    leave_by_the_tunnel(&mut state);
    assert!(state.verdict().is_some(), "the fixture ends the run");

    let rows = screen(&state, opening());
    assert!(shows(&rows, "ESCAPED"), "the verdict is on top: {rows:#?}");
    assert!(
        !shows(&rows, HEADING),
        "and the card is under it: {rows:#?}"
    );
}

/// **The card fits the board at its widest and its longest** (§11.4): every §12.6 rule
/// in force at once — a composite's attributed rows *and* the departures beyond it — is
/// the longest list either card can draw, and it still leaves map showing above and
/// below. A card that outgrew the board would drop its own footer in silence.
#[test]
fn the_longest_card_still_fits_the_v1_board() {
    let state = run_with(3, every_modifier());
    let rows = screen(&state, opening());
    let top = row_of(&rows, HEADING) - 2;
    let bottom = rows
        .iter()
        .rposition(|row| row.starts_with(CORNER_BOTTOM_LEFT))
        .expect("the card closes on its bottom edge") as u32;

    assert!(top > TOP_ROWS, "board above the card: {rows:#?}");
    assert!(
        rows.len() as u32 - bottom > BOTTOM_ROWS + 1,
        "board below the card: {rows:#?}",
    );
    // …and nothing was dropped off the end: the footer is the row above the closing
    // rule, wherever the list ended.
    assert!(shows(&rows, BEGIN_KEYS), "the hint survives: {rows:#?}");
}
