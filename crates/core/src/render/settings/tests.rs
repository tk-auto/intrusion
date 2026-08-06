//! The options screen's own tests (§14 v2/#513) — the golden grid with and without
//! the debug section, the gate, the marker walk, and the touch targets.

use super::*;
use crate::render::{MenuUi, SeedCopy};

/// The v1 board's screen (§10.2): 40 wide, `TOP_ROWS + 40 + BOTTOM_ROWS` tall.
const W: u32 = 40;
const H: u32 = 43;

fn text_of(grid: &Grid) -> String {
    grid.to_text().join("\n")
}

/// The view state a shell hands the screen: the options screen up, at the row the
/// marker rests on.
fn ui(selected: SettingsRow) -> ScreenUi {
    ScreenUi {
        settings: Some(SettingsUi { selected }),
        ..ScreenUi::default()
    }
}

/// The same, in a **debug session** — the one flag that opens the gate.
fn debug_ui(selected: SettingsRow) -> ScreenUi {
    ScreenUi {
        debug_mode: true,
        ..ui(selected)
    }
}

/// A level-seed token for the replay row to have something to hand over. Quick play's
/// own preset (#244), which encodes.
fn level() -> Option<LevelSeed> {
    let seed = LevelSeed::quick_play(7);
    assert!(seed.encode().is_some(), "the fixture level has a token");
    Some(seed)
}

/// The **ordinary session's screen**: the heading, the `[x]`, the display section and
/// its two rows, the footer — and *nothing* of the debug section.
#[test]
fn the_options_screen_lists_the_display_settings() {
    let text = text_of(&render_settings(
        W,
        H,
        ui(SettingsRow::Theme),
        DebugModifiers::default(),
        level(),
    ));
    assert!(text.contains(HEADING), "the screen names itself:\n{text}");
    assert!(
        text.contains(CLOSE_BUTTON),
        "and carries its escape:\n{text}"
    );
    assert!(text.contains(DISPLAY_HEADING));
    assert!(text.contains(FOOTER));
    for row in [SettingsRow::Theme, SettingsRow::Renderer] {
        assert!(text.contains(row.label()), "{row:?} is missing:\n{text}");
    }
    // The default view state: dark theme, text renderer, marker on the first row.
    assert!(text.contains("theme        dark"), "\n{text}");
    assert!(text.contains("renderer     text"), "\n{text}");
    assert!(
        text.contains("> theme"),
        "the marker opens on the theme row:\n{text}"
    );
}

/// **The gate** (§12.6/#459/#513): with no debug session there is no heading and no
/// switch — not a dimmed one, not one a stale marker could reach. With one, both are
/// there.
#[test]
fn the_debug_section_is_drawn_only_in_a_debug_session() {
    let plain = text_of(&render_settings(
        W,
        H,
        ui(SettingsRow::Theme),
        DebugModifiers::default(),
        level(),
    ));
    assert!(!plain.contains(DEBUG_HEADING), "no heading:\n{plain}");
    for row in [SettingsRow::Reveal, SettingsRow::Replay] {
        assert!(
            !plain.contains(row.label()),
            "{row:?} is drawn without a debug session:\n{plain}",
        );
        assert_eq!(
            settings_hit(W, ui(SettingsRow::Theme), level(), 0, row_y(row)),
            None,
            "{row:?} is tappable without a debug session",
        );
    }

    let debug = text_of(&render_settings(
        W,
        H,
        debug_ui(SettingsRow::Theme),
        DebugModifiers::default(),
        level(),
    ));
    assert!(debug.contains(DEBUG_HEADING), "\n{debug}");
    for row in [SettingsRow::Reveal, SettingsRow::Replay] {
        assert!(debug.contains(row.label()), "{row:?} is missing:\n{debug}");
    }
    // …and the preferences did not move when the gate opened.
    assert_eq!(row_y(SettingsRow::Theme), FIRST_DISPLAY_ROW);
}

/// Every value column is read from the **live** values, never from a flag the screen
/// keeps: flipping the theme, the renderer or the debug switch changes what is drawn.
#[test]
fn every_row_says_what_the_live_value_is() {
    let light = ScreenUi {
        theme: Theme::Light,
        renderer: Renderer::Tiles,
        ..debug_ui(SettingsRow::Theme)
    };
    let text = text_of(&render_settings(
        W,
        H,
        light,
        DebugModifiers {
            reveal_whole_level: true,
        },
        level(),
    ));
    assert!(text.contains("theme        light"), "\n{text}");
    assert!(text.contains("renderer     tiles"), "\n{text}");
    assert!(text.contains("omni-vision  on"), "\n{text}");

    let off = text_of(&render_settings(
        W,
        H,
        debug_ui(SettingsRow::Theme),
        DebugModifiers::default(),
        level(),
    ));
    assert!(off.contains("omni-vision  off"), "\n{off}");
}

/// **Every row reads alike** — Interest when marked, Neutral otherwise, debug rows
/// included. They were drawn in Ground while unmarked, which said *inert* (§11.2's
/// receding scenery) about the one section where a press does the most; the heading
/// over them carries the gate on its own, and the two sections are laid out
/// identically otherwise.
#[test]
fn a_debug_row_is_inked_like_any_other() {
    let column = block_column(W) + MARKER.chars().count() as u32;
    let grid = render_settings(
        W,
        H,
        debug_ui(SettingsRow::Theme),
        DebugModifiers::default(),
        level(),
    );
    for row in shown_rows(true, true) {
        assert_eq!(
            grid.get(column, row_y(row)).fg,
            if row == SettingsRow::Theme {
                Category::Interest
            } else {
                Category::Neutral
            },
            "{row:?} reads like every other row",
        );
    }
    // The gate is the heading's alone, and it keeps its own cue.
    assert_eq!(
        grid.get(column, DEBUG_HEADING_ROW).fg,
        Category::Warning,
        "the DEBUG heading still says what it is",
    );
    // Both sections are laid out the same way: heading, a blank, then the first row.
    assert_eq!(
        FIRST_DEBUG_ROW - DEBUG_HEADING_ROW,
        FIRST_DISPLAY_ROW - DISPLAY_HEADING_ROW,
    );
}

/// The **replay row exists only for a run with a token** (#333) — the drawn control
/// and the thing it would hand over can never disagree.
#[test]
fn the_replay_row_needs_a_token_to_hand_over() {
    let without = text_of(&render_settings(
        W,
        H,
        debug_ui(SettingsRow::Theme),
        DebugModifiers::default(),
        None,
    ));
    assert!(
        !without.contains(SettingsRow::Replay.label()),
        "no token, no replay row:\n{without}",
    );
    assert!(
        without.contains(SettingsRow::Reveal.label()),
        "the omni switch does not need one:\n{without}",
    );
    assert_eq!(
        settings_hit(
            W,
            debug_ui(SettingsRow::Theme),
            None,
            0,
            row_y(SettingsRow::Replay)
        ),
        None,
        "and nothing is tappable where it would have been",
    );
}

/// The copy acknowledgement (#353) prints under the replay row that produced it, and
/// only where that row is.
#[test]
fn the_copy_acknowledgement_answers_the_replay_row() {
    let acked = ScreenUi {
        seed_copy: SeedCopy::Copied,
        ..debug_ui(SettingsRow::Replay)
    };
    let rows = render_settings(W, H, acked, DebugModifiers::default(), level()).to_text();
    assert!(
        rows[REPLAY_ACK_ROW as usize].contains("copied"),
        "the acknowledgement is under the control: {:?}",
        rows[REPLAY_ACK_ROW as usize],
    );

    // No replay row, no line — the answer belongs to the control that raised it.
    let elsewhere = ScreenUi {
        seed_copy: SeedCopy::Copied,
        ..ui(SettingsRow::Theme)
    };
    let text = text_of(&render_settings(
        W,
        H,
        elsewhere,
        DebugModifiers::default(),
        level(),
    ));
    assert!(!text.contains("copied"), "\n{text}");
}

/// **The whole row is the target** (§11.6), at any column, and the blank row between
/// settings is the buffer that keeps a low tap off the next one.
#[test]
fn every_row_is_a_full_width_target_with_air_between() {
    for row in shown_rows(true, true) {
        for x in [0, W / 2, W - 1] {
            assert_eq!(
                settings_hit(W, debug_ui(SettingsRow::Theme), level(), x, row_y(row)),
                Some(SettingsHit::Row(row)),
                "{row:?} at column {x}",
            );
        }
        assert_eq!(
            settings_hit(
                W,
                debug_ui(SettingsRow::Theme),
                level(),
                W / 2,
                row_y(row) + 1
            ),
            None,
            "the row under {row:?} is air",
        );
    }
}

/// The `[x]` is a target in its own right and nothing else on its row is — the
/// always-reachable escape §11.6 asks for, the exact thing the old options dialog
/// never had.
#[test]
fn the_close_control_is_the_touch_way_out() {
    let start = close_button_start(W);
    for x in start..start + CLOSE_BUTTON_LEN {
        assert_eq!(
            settings_hit(W, ui(SettingsRow::Theme), level(), x, HEADING_ROW),
            Some(SettingsHit::Close),
        );
    }
    assert_eq!(
        settings_hit(W, ui(SettingsRow::Theme), level(), 0, HEADING_ROW),
        None,
        "the heading itself is not a control",
    );
}

/// The marker walks the **shown** rows and wraps at either end — and in an ordinary
/// session it never lands on a debug row, because there is not one to land on.
#[test]
fn the_marker_walks_the_shown_rows_and_wraps() {
    let plain = SettingsUi {
        selected: SettingsRow::Renderer,
    };
    assert_eq!(plain.next_row(false, false), SettingsRow::Theme, "wraps");
    assert_eq!(plain.prev_row(false, false), SettingsRow::Theme);
    for row in shown_rows(false, false) {
        let ui = SettingsUi { selected: row };
        assert!(!ui.next_row(false, false).debug_only());
        assert!(!ui.prev_row(false, false).debug_only());
    }

    let debug = SettingsUi {
        selected: SettingsRow::Renderer,
    };
    assert_eq!(debug.next_row(true, true), SettingsRow::Reveal);
    assert_eq!(
        SettingsUi {
            selected: SettingsRow::Replay
        }
        .next_row(true, true),
        SettingsRow::Theme,
        "past the last debug row it wraps to the first setting",
    );
}

/// A marker left on a row this session does not have falls back rather than marking
/// nothing — the state a shell is in the moment a debug session's screen is drawn for
/// a run whose token has gone.
#[test]
fn a_selection_that_is_not_shown_falls_back() {
    let stale = SettingsUi {
        selected: SettingsRow::Reveal,
    };
    assert_eq!(stale.selection(false, false), SettingsRow::default());
    let rows = render_settings(
        W,
        H,
        ScreenUi {
            settings: Some(stale),
            ..ScreenUi::default()
        },
        DebugModifiers::default(),
        level(),
    )
    .to_text();
    assert!(
        rows[row_y(SettingsRow::Theme) as usize].contains(MARKER.trim_end()),
        "the marker falls back to the first row: {:?}",
        rows[row_y(SettingsRow::Theme) as usize],
    );
}

/// The screen **writes no state and reads no world** beyond the two values it is
/// handed: two renders of the same inputs are the same grid, and the screen drawn over
/// a menu is the screen drawn over a run.
#[test]
fn the_screen_is_a_pure_function_of_what_it_is_handed() {
    let over_menu = ScreenUi {
        menu: Some(MenuUi::default()),
        ..ui(SettingsRow::Renderer)
    };
    let over_help = ScreenUi {
        help_open: true,
        ..ui(SettingsRow::Renderer)
    };
    let plain = render_settings(
        W,
        H,
        ui(SettingsRow::Renderer),
        DebugModifiers::default(),
        level(),
    );
    for other in [over_menu, over_help] {
        assert_eq!(
            text_of(&render_settings(
                W,
                H,
                other,
                DebugModifiers::default(),
                level()
            )),
            text_of(&plain),
            "what the screen was opened over changes nothing about it",
        );
    }
}

/// A board too small for the screen clips rather than panicking — the help card's rule
/// (only hand-built states get this small).
#[test]
fn a_tiny_board_clips_rather_than_panicking() {
    for (w, h) in [(0, 0), (1, 1), (8, 4)] {
        let grid = render_settings(
            w,
            h,
            debug_ui(SettingsRow::Theme),
            DebugModifiers::default(),
            level(),
        );
        assert_eq!((grid.width(), grid.height), (w, h));
    }
}
