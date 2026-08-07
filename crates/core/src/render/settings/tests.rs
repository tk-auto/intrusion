//! The Options tab's own tests (§14 v2/#513) — the golden grid with and without the
//! debug section, the gate, the marker walk, and the touch targets.

use super::*;
use crate::alert::AlertReadout;
use crate::render::help::{render_help, PanelRun};
use crate::render::{HelpTab, MenuUi, SeedCopy};

/// The v1 board's screen (§10.2): 40 wide, `TOP_ROWS + 40 + BOTTOM_ROWS` tall.
const W: u32 = 40;
const H: u32 = 43;

fn text_of(grid: &Grid) -> String {
    grid.to_text().join("\n")
}

/// The view state a shell hands the panel: the Options tab up, at the row the marker
/// rests on.
fn ui(selected: SettingsRow) -> ScreenUi {
    ScreenUi {
        help_tab: HelpTab::Options,
        settings: SettingsUi { selected },
        ..ScreenUi::default()
    }
}

/// The whole panel, drawn on its Options tab — the frame a player actually sees, so
/// these tests read what the tab bar, the body and the footer produce together.
fn render_settings(
    _w: u32,
    _h: u32,
    ui: ScreenUi,
    debug: DebugModifiers,
    level: Option<LevelSeed>,
) -> Grid {
    render_settings_for(ui, debug, level, false)
}

/// The same, for a run whose ghost **latch** is set (§12.6/#507) — the only fact the
/// tab reads that is not on [`DebugModifiers`], so it is the one the fixture takes
/// separately.
fn render_settings_for(
    ui: ScreenUi,
    debug: DebugModifiers,
    level: Option<LevelSeed>,
    ghosted: bool,
) -> Grid {
    let alert = AlertReadout {
        rung: 0,
        effects: Vec::new(),
    };
    render_help(
        W,
        H,
        ui,
        PanelRun {
            level,
            modifiers: crate::modifiers::LevelModifiers::default(),
            alert: &alert,
            bar: Vec::new(),
            debug,
            ghosted,
        },
    )
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

/// The **ordinary session's tab**: the display section and its two rows, the tab's own
/// footer — and *nothing* of the debug section.
#[test]
fn the_options_screen_lists_the_display_settings() {
    let text = text_of(&render_settings(
        W,
        H,
        ui(SettingsRow::Theme),
        DebugModifiers::default(),
        level(),
    ));
    assert!(
        text.contains(HelpTab::Options.label()),
        "the tab names itself on the bar:\n{text}",
    );
    assert!(text.contains(DISPLAY_HEADING));
    assert!(text.contains(FOOTER), "and prints its own footer:\n{text}");
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
            settings_hit(false, true, row_y(row)),
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
            ..DebugModifiers::default()
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
    let column = CONTENT_INDENT;
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
    // **The two headings read alike too** — System, the panel's own heading colour. The
    // gate is the word and the gap, not an alarm colour over the section most sessions
    // never see.
    assert_eq!(
        grid.get(SECTION_INDENT, DEBUG_HEADING_ROW).fg,
        grid.get(SECTION_INDENT, DISPLAY_HEADING_ROW).fg,
        "the DEBUG heading is inked like the DISPLAY one",
    );
    assert_eq!(
        grid.get(SECTION_INDENT, DEBUG_HEADING_ROW).fg,
        Category::System,
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
        settings_hit(true, false, row_y(SettingsRow::Replay)),
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

/// **The ghost row reads its real state off the run** (§12.6/#507), like the switch
/// beside it — and it is drawn only behind the same gate, so an ordinary session has no
/// way to reach the one debug control that bends a rule.
#[test]
fn the_ghost_row_says_whether_the_guards_can_see_you() {
    let on = text_of(&render_settings(
        W,
        H,
        debug_ui(SettingsRow::Theme),
        DebugModifiers {
            ghost: true,
            ..DebugModifiers::default()
        },
        level(),
    ));
    assert!(on.contains("ghost        on"), "\n{on}");

    let off = text_of(&render_settings(
        W,
        H,
        debug_ui(SettingsRow::Theme),
        DebugModifiers::default(),
        level(),
    ));
    assert!(off.contains("ghost        off"), "\n{off}");

    // The gate, same as its neighbour's: no session, no row and no tap target.
    let plain = text_of(&render_settings(
        W,
        H,
        ui(SettingsRow::Theme),
        DebugModifiers {
            ghost: true,
            ..DebugModifiers::default()
        },
        level(),
    ));
    assert!(
        !plain.contains(SettingsRow::Ghost.label()),
        "the switch is not drawn without a debug session:\n{plain}",
    );
    assert_eq!(settings_hit(false, true, row_y(SettingsRow::Ghost)), None);
}

/// **The replay export is refused for a ghosted run** (§12.6/#507), and refused
/// *legibly*: the row is still drawn — a row that vanished would look like a run with
/// no token — but reads as unavailable, and the acknowledgement under it names the
/// switch that did it rather than leaving a dead control to be read as a bug.
///
/// The refusal answers to the run's **latch**, not to the live switch: a run that has
/// had the ghost on and switched it off again is still refused, which is what
/// `ghosted` being a separate argument to the drawing is for.
#[test]
fn a_ghosted_run_is_refused_its_replay_and_told_why() {
    let offered = text_of(&render_settings_for(
        debug_ui(SettingsRow::Replay),
        DebugModifiers::default(),
        level(),
        false,
    ));
    assert!(offered.contains("replay       copy as link"), "\n{offered}");

    // Latched — and the switch itself is *off*, so what the row answers to is the run.
    let refused = render_settings_for(
        debug_ui(SettingsRow::Replay),
        DebugModifiers::default(),
        level(),
        true,
    );
    let text = text_of(&refused);
    assert!(text.contains("replay       unavailable"), "\n{text}");
    assert!(
        !text.contains(REPLAY_VALUE),
        "and it stops offering what it cannot do:\n{text}",
    );
    assert!(
        text.contains(SettingsRow::Replay.label()),
        "…while staying on screen to be pressed:\n{text}",
    );

    // The press's answer names the switch.
    let acked = render_settings_for(
        ScreenUi {
            seed_copy: SeedCopy::Refused,
            ..debug_ui(SettingsRow::Replay)
        },
        DebugModifiers::default(),
        level(),
        true,
    )
    .to_text();
    let line = &acked[REPLAY_ACK_ROW as usize];
    assert!(
        line.contains("ghost"),
        "the refusal names the switch that caused it: {line:?}",
    );
}

/// **The whole row is the target** (§11.6), at any column — which is why the tab's hit
/// test takes no column at all — and the blank row between settings is the buffer that
/// keeps a low tap off the next one. Asserted through the panel's own [`help_hit`], so
/// what is checked is the press a player actually makes.
#[test]
fn every_row_is_a_full_width_target_with_air_between() {
    let hit = |x, y| crate::render::help_hit(W, H, debug_ui(SettingsRow::Theme), level(), x, y);
    for row in shown_rows(true, true) {
        for x in [0, W / 2, W - 1] {
            assert_eq!(
                hit(x, row_y(row)),
                Some(crate::render::HelpHit::Setting(row)),
                "a press on {row:?} at column {x} fires it",
            );
        }
        assert_eq!(hit(W / 2, row_y(row) + 1), None, "the row under it is air");
        for x in [0, W / 2, W - 1] {
            assert_eq!(
                settings_hit(true, true, row_y(row)),
                Some(row),
                "{row:?} at column {x}",
            );
        }
        assert_eq!(
            settings_hit(true, true, row_y(row) + 1),
            None,
            "the row under {row:?} is air",
        );
    }
}

/// The panel's `[x]` is the touch way out of this tab as of any other — the
/// always-reachable escape §11.6 asks for, and the exact thing the old options dialog
/// never had. It belongs to the panel, so this only pins that the tab did not take it
/// away.
#[test]
fn the_panels_close_control_is_the_touch_way_out() {
    let close = crate::render::help_hit(W, H, ui(SettingsRow::Theme), level(), W - 2, 0);
    assert_eq!(close, Some(crate::render::HelpHit::Close));
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
            help_tab: HelpTab::Options,
            settings: stale,
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

/// A board too small for the tab clips rather than panicking — the help card's rule
/// (only hand-built states get this small). Drawn through the panel's own entry point,
/// since that is what sizes the grid.
#[test]
fn a_tiny_board_clips_rather_than_panicking() {
    let alert = AlertReadout {
        rung: 0,
        effects: Vec::new(),
    };
    for (w, h) in [(0, 0), (1, 1), (8, 4)] {
        let grid = render_help(
            w,
            h,
            debug_ui(SettingsRow::Theme),
            PanelRun {
                level: level(),
                modifiers: crate::modifiers::LevelModifiers::default(),
                alert: &alert,
                bar: Vec::new(),
                debug: DebugModifiers::default(),
                ghosted: false,
            },
        );
        assert_eq!((grid.width(), grid.height), (w, h));
    }
}
