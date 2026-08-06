//! The **options screen** (§14 v2/#513) — the game's global settings, drawn in the
//! character grid like the title screen and the help panel, and reachable from both.
//!
//! §14 v2 promises "saves, options, a help screen and a legend". The other three
//! shipped first, and the settings they wanted went wherever there was room: the
//! colour theme lived on the help panel's footer "until v2 grows an options screen"
//! (§11.2/#189), and the tile renderer hid behind a `?tiles=1` URL flag (§11.1/#460).
//! This is that screen, and it is where both of them now live.
//!
//! # What is on it, and what is deliberately not
//!
//! Two **preferences** — the colour theme and the renderer — which are facts about the
//! person at the screen rather than about the run, so they outlive it and are written
//! to the shell's own settings record (#513's split: ending a run must never reset a
//! preference). And, **only in a debug session** (§12.6/#459), the switches that used
//! to be the help panel's fourth tab: omni-vision and the replay export.
//!
//! It is **not** the pre-run level-options dialog (#298). That one sets the
//! *difficulty of the run you are about to start* and is asked before there is a run;
//! this one sets how the game looks and is answered at any time. Keeping the two apart
//! is why the menu's *Quick play* opens one and its *Options* entry opens the other.
//!
//! # The debug section is gated, and gated visibly
//!
//! A preference and a playtest switch must never be confusable (§12.6 — "a debug
//! modifier changes only what you get to see"), so the debug rows sit under their own
//! heading, in their own colour, behind a line that states the promise the gate rests
//! on. With no debug session there is no heading, no note and no row: [`shown_rows`]
//! is what the drawing, the hit-test and the selection walk all read, so a switch that
//! is not drawn cannot be reached by key, by tap, or by a stale
//! [`SettingsUi::selected`].
//!
//! Everything the gate promised on the Debug tab is unchanged — perception only, never
//! persisted, never in a level-seed token. Only the surface moved.
//!
//! # Input
//!
//! One vertical list, walked like the title screen's: `↑`/`↓` (and the vertical
//! swipes) move the marker, `Enter` fires the marked row, `Escape` — or the drawn
//! `[x]` — leaves. **A press is unbound** (§11.6/appendix 21), so a stray tap on empty
//! screen flips nothing; a row is fired by pressing *the row*.

use super::help::{close_button_start, CLOSE_BUTTON, CLOSE_BUTTON_LEN, FOOTER_INDENT};
use super::menu::{centre, ENTRY_SPACING, MARKER, NO_MARKER};
use super::{blank_grid, draw, Grid, ScreenUi};
use crate::category::{Category, Theme};
use crate::level_seed::LevelSeed;
use crate::modifiers::DebugModifiers;
use crate::place::LevelConfig;

/// Which cell primitive the shell paints the board with (§11.1/#460/#513) — the
/// *only* thing the core knows about a renderer, and the exact counterpart of
/// [`Theme`] one module over.
///
/// §11.1 **[SETTLED]** says the renderer is "a separate concern behind one interface…
/// The core must not know which is in use". This enum keeps that true rather than
/// breaking it: the core carries the **name** of the choice so the options screen can
/// draw a row for it, and knows nothing else — not a sprite, not a sheet, not a
/// `drawImage`. A shell reads the flag and answers a cell with whichever primitive it
/// names.
///
/// [`Text`](Self::Text) is the default, so every test, the sim and a shell that has
/// never heard of a spritesheet draw the game they always drew.
///
/// **Two variants, not three.** The animation step (#462) is a property of the tile
/// renderer rather than a third way of drawing a cell, so it lands as its own row on
/// this screen when it lands — not as a variant here.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Renderer {
    /// The character grid, drawn as text — the game as §11.1 describes it.
    #[default]
    Text,
    /// The tile renderer (#460/#461): a sprite per glyph, tinted by the colour the
    /// glyph would have been.
    Tiles,
}

impl Renderer {
    /// The other renderer — what the screen's row flips to. There are two, so this is
    /// the whole cycle, exactly as [`Theme::toggled`] is.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Renderer::Text => Renderer::Tiles,
            Renderer::Tiles => Renderer::Text,
        }
    }

    /// The value as the screen draws it.
    fn label(self) -> &'static str {
        match self {
            Renderer::Text => "text",
            Renderer::Tiles => "tiles",
        }
    }
}

/// The rows of the options screen, top to bottom — the drawing order, the hit-test
/// order, and the order the marker walks.
///
/// [`ALL`](Self::ALL) is the whole vocabulary; [`shown_rows`] is the list a given
/// session actually has, which is what every rule here is written over. The two debug
/// rows are **last**, so the preferences a player always has keep the positions they
/// have always had.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SettingsRow {
    /// The colour theme (§11.2/#189) — dark or light. The [`Default`], and so the row
    /// the screen opens on.
    #[default]
    Theme,
    /// The renderer (§11.1/#460) — the character grid, or tiles.
    Renderer,
    /// **Omni-vision** (§12.6/#459) — the player's sight becomes the whole facility.
    /// Debug sessions only, never persisted, and perception-only by construction: the
    /// core applies it in the sight phase and nowhere else.
    Reveal,
    /// The **replay export** (§12.4/§13.1/#411) — the whole run as a
    /// `…#seed=<token>&inputs=<script>` link on the clipboard. Debug sessions only,
    /// and only for a run with a token for the link to name (#333).
    ///
    /// The one row that is not a setting: firing it *does* something rather than
    /// flipping something. It is here because exporting a run is a debugging
    /// affordance and this is where the debugging affordances now live.
    Replay,
}

impl SettingsRow {
    /// Every row this screen knows, in reading order. A new setting is one entry here,
    /// one arm in [`label`](Self::label), one arm in [`value`], and — if it is
    /// conditional — one arm in [`shown_rows`].
    pub const ALL: [SettingsRow; 4] = [
        SettingsRow::Theme,
        SettingsRow::Renderer,
        SettingsRow::Reveal,
        SettingsRow::Replay,
    ];

    /// The row's label, as drawn in the left column.
    ///
    /// `renderer`, not `tiles`: the row names the axis rather than one end of it, so
    /// the value column can say `text` or `tiles` and the pair reads as a setting
    /// rather than as a switch whose off position has no name.
    pub const fn label(self) -> &'static str {
        match self {
            SettingsRow::Theme => "theme",
            SettingsRow::Renderer => "renderer",
            SettingsRow::Reveal => "omni-vision",
            SettingsRow::Replay => "replay",
        }
    }

    /// Whether the row belongs to the **debug** section — the gate that decides which
    /// heading it is drawn under and whether it is drawn at all.
    fn debug_only(self) -> bool {
        matches!(self, SettingsRow::Reveal | SettingsRow::Replay)
    }
}

// The conditional rows are the **last** entries of [`SettingsRow::ALL`], which is what
// lets the drawing lay the screen out as two contiguous sections rather than filtering
// one list into two. Checked at compile time, so a preference appended after them
// fails the build instead of quietly landing under the DEBUG heading.
const _: () = assert!(
    matches!(SettingsRow::ALL[0], SettingsRow::Theme)
        && matches!(SettingsRow::ALL[1], SettingsRow::Renderer)
        && matches!(SettingsRow::ALL[2], SettingsRow::Reveal)
        && matches!(SettingsRow::ALL[3], SettingsRow::Replay),
    "the debug rows must stay last in SettingsRow::ALL — the screen draws two \
     contiguous sections from it",
);

/// **The rows this screen actually has**, given whether this is a debug session
/// (§12.6/#459) and whether the run has a level-seed token for the replay link to
/// carry (#333).
///
/// The one place that decides what is on the screen: the drawing, the row
/// measurements, the hit test and the selection walk all read it, so a row can never
/// be drawn where a tap does not land, and a switch this session does not have cannot
/// be reached at all.
#[must_use]
pub fn shown_rows(debug: bool, replay: bool) -> Vec<SettingsRow> {
    SettingsRow::ALL
        .into_iter()
        .filter(|row| match row {
            SettingsRow::Reveal => debug,
            SettingsRow::Replay => debug && replay,
            _ => true,
        })
        .collect()
}

/// The options screen's **view state**, owned by the shell exactly like
/// [`MenuUi`](super::MenuUi) — it changes no world and costs no turn (§12.1). A shell
/// keeps `Some(SettingsUi)` on [`ScreenUi::settings`] while the screen is up and
/// clears it when the screen is left, whatever it was opened over.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SettingsUi {
    /// Which row the marker rests on. The [`Default`] is the theme row, the first
    /// setting on the screen.
    pub selected: SettingsRow,
}

impl SettingsUi {
    /// The row the marker actually rests on — [`selected`](Self::selected), or the
    /// first shown row when that row is not on this screen.
    ///
    /// The fallback matters for the same reason the menu's does: a shell that left the
    /// marker on a debug row would otherwise draw it on a row that is not there the
    /// moment the run loses its token.
    #[must_use]
    pub fn selection(self, debug: bool, replay: bool) -> SettingsRow {
        let rows = shown_rows(debug, replay);
        if rows.contains(&self.selected) {
            self.selected
        } else {
            SettingsRow::default()
        }
    }

    /// The next row below the marker, wrapping past the last.
    #[must_use]
    pub fn next_row(self, debug: bool, replay: bool) -> SettingsRow {
        self.seek(debug, replay, 1)
    }

    /// The previous row above the marker, wrapping past the first.
    #[must_use]
    pub fn prev_row(self, debug: bool, replay: bool) -> SettingsRow {
        let rows = shown_rows(debug, replay);
        self.seek(debug, replay, rows.len() - 1) // one step back, modulo the ring
    }

    /// Walk `step` positions round the shown ring. Every row on this screen does
    /// something, so unlike the menu's walk there is nothing to step over.
    fn seek(self, debug: bool, replay: bool, step: usize) -> SettingsRow {
        let rows = shown_rows(debug, replay);
        let here = self.selection(debug, replay);
        let n = rows.len();
        let start = rows.iter().position(|&r| r == here).unwrap_or(0);
        rows[(start + step) % n]
    }
}

/// The screen's own heading, on row 0 beside the `[x]` — the help panel's tab-bar row,
/// with a title where the tabs would be.
const HEADING: &str = "OPTIONS";

/// The two section headings. `DISPLAY` is what a player came here for; `DEBUG` names
/// the gate rather than hiding it (§12.6).
const DISPLAY_HEADING: &str = "DISPLAY";
const DEBUG_HEADING: &str = "DEBUG";

/// The line under the DEBUG heading: the §12.6 promise, printed where the switches
/// are, so the one thing that makes a control here safe is stated beside the control.
const DEBUG_NOTE: &str = "sight only, never the facility";

/// The footer. It names the keys the screen answers that have no drawn control of
/// their own, and the way back out — §11.6's no-trap rule in prose, beside the `[x]`
/// that is its touch half.
const FOOTER: &str = "↑↓ choose · Enter sets · Esc back";

/// The row the `[x]` and the heading share.
const HEADING_ROW: u32 = 0;

/// The `DISPLAY` heading's row, and the first setting under it.
const DISPLAY_HEADING_ROW: u32 = 2;
const FIRST_DISPLAY_ROW: u32 = DISPLAY_HEADING_ROW + 2;

/// How many rows the display section occupies, from its first row to the blank after
/// its last — two settings at [`ENTRY_SPACING`].
const DISPLAY_ROWS: u32 = 2 * ENTRY_SPACING;

/// The `DEBUG` heading, its promise line, and the first switch under them. The gap
/// above the heading is deliberate and is the widest on the screen: the sections are
/// different kinds of thing, and the space is the first thing that says so.
const DEBUG_HEADING_ROW: u32 = FIRST_DISPLAY_ROW + DISPLAY_ROWS + 1;
const DEBUG_NOTE_ROW: u32 = DEBUG_HEADING_ROW + 1;
const FIRST_DEBUG_ROW: u32 = DEBUG_NOTE_ROW + 2;

/// The row the copy acknowledgement is printed on — directly under the replay row it
/// answers, exactly as the help panel prints its own (#353).
const REPLAY_ACK_ROW: u32 = FIRST_DEBUG_ROW + ENTRY_SPACING + 1;

/// The width the label column is padded to, so every value on the screen starts in the
/// same column and the list reads as a table rather than as a ragged pile of phrases.
/// Two cells clear of the longest label, checked below.
const LABEL_WIDTH: usize = 13;

const _: () = {
    let mut i = 0;
    while i < SettingsRow::ALL.len() {
        assert!(
            SettingsRow::ALL[i].label().len() + 2 <= LABEL_WIDTH,
            "a settings row's label is too long for the value column — shorten it or \
             widen LABEL_WIDTH (see render::settings)",
        );
        i += 1;
    }
};

/// The value column's own bound: the widest value any row can draw. `copy as link` is
/// the longest, and the block is centred from the sum of the two, so this is what
/// keeps the widest row inside the v1 board (§10.2).
const VALUE_MAX: usize = 12;

/// The widest row the screen can draw, in cells — the marker, the padded label, and
/// the longest value.
const ROW_WIDTH: u32 = (MARKER.len() + LABEL_WIDTH + VALUE_MAX) as u32;

// The block is centred, and its **left** column is where the section headings and the
// promise line start too — so the longest of those must fit from that column to the
// board's one-cell right margin. [`draw`] clips in silence, and here it would clip the
// one line that states why a debug control is safe to have at all (§2.3).
const _: () = {
    let label_column = (LevelConfig::V1.width - ROW_WIDTH) / 2 + MARKER.len() as u32;
    assert!(
        label_column as usize + DEBUG_NOTE.len() < LevelConfig::V1.width as usize,
        "the debug section's promise line does not fit the v1 board — shorten it \
         (see render::settings)",
    );
};

/// The screen row `row` is drawn on — the counterpart of the menu's `entry_row`, and
/// shared by the drawing and [`settings_hit`] so a tap lands on exactly the row drawn.
///
/// The rows sit at fixed offsets rather than being packed from the shown list, because
/// the two sections are laid out independently: a session without the debug section
/// simply stops after the display one, and the preferences do not move when the gate
/// opens.
fn row_y(row: SettingsRow) -> u32 {
    match row {
        SettingsRow::Theme => FIRST_DISPLAY_ROW,
        SettingsRow::Renderer => FIRST_DISPLAY_ROW + ENTRY_SPACING,
        SettingsRow::Reveal => FIRST_DEBUG_ROW,
        SettingsRow::Replay => FIRST_DEBUG_ROW + ENTRY_SPACING,
    }
}

/// The column the block of rows starts at: the widest possible row centred, with every
/// row left-aligned inside it — the menu's rule, so the two list screens are read the
/// same way and the labels never jitter as the marker moves.
fn block_column(width: u32) -> u32 {
    centre(width, ROW_WIDTH)
}

/// The column the labels, the section headings and the promise line all start at — the
/// block's column past the marker, so a heading sits directly over the rows it names.
fn label_column(width: u32) -> u32 {
    block_column(width) + MARKER.chars().count() as u32
}

/// What a row's **value** column says, read from the live values every time — never
/// from a flag this screen keeps of its own, so a row and the thing it names cannot
/// disagree (the Debug tab's rule, kept).
fn value(row: SettingsRow, ui: ScreenUi, debug: DebugModifiers) -> &'static str {
    match row {
        SettingsRow::Theme => match ui.theme {
            Theme::Dark => "dark",
            Theme::Light => "light",
        },
        SettingsRow::Renderer => ui.renderer.label(),
        SettingsRow::Reveal => {
            if debug.reveal_whole_level {
                "on"
            } else {
                "off"
            }
        }
        SettingsRow::Replay => "copy as link",
    }
}

/// A row as drawn, marker and all — one function so the drawing and the width the
/// block is centred by can never disagree.
fn row_text(row: SettingsRow, value: &str, selected: bool) -> String {
    let marker = if selected { MARKER } else { NO_MARKER };
    format!("{marker}{:<LABEL_WIDTH$}{value}", row.label())
}

/// What a press on the options screen lands on (§11.6/#513) — the screen's
/// counterpart of [`MenuHit`](super::MenuHit) and [`HelpHit`](super::HelpHit).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsHit {
    /// A setting's row — fire it, exactly as `Enter` on the marker does.
    Row(SettingsRow),
    /// The `[x]` close control — leave the screen, back to whatever it was opened
    /// over. The always-reachable escape §11.6 asks for, and the exact thing the old
    /// options dialog never had.
    Close,
}

/// Which [`SettingsHit`] screen cell `(x, y)` lands on, or `None` for a press that hit
/// nothing and is swallowed (§11.6/#513).
///
/// **The whole row is the target**, at any column, as it is on the title screen:
/// nothing else is drawn on a row, and a generous target is the difference between a
/// screen that works on a phone and one that does not. The blank row between settings
/// ([`ENTRY_SPACING`]) is the buffer that keeps a low tap off the next one.
///
/// It takes the same `debug` and `level` the drawing does, so a switch this session
/// does not have has no hit region where it would have been.
#[must_use]
pub fn settings_hit(
    width: u32,
    ui: ScreenUi,
    level: Option<LevelSeed>,
    x: u32,
    y: u32,
) -> Option<SettingsHit> {
    if y == HEADING_ROW {
        let close = close_button_start(width);
        return (x >= close && x < close + CLOSE_BUTTON_LEN).then_some(SettingsHit::Close);
    }
    shown_rows(ui.debug_mode, level.is_some())
        .into_iter()
        .find(|&row| row_y(row) == y)
        .map(SettingsHit::Row)
}

/// Render the options screen (§14 v2/#513) — the whole `width × height` screen, not an
/// overlay, so the shell paints it through the one path it paints a frame with and
/// nothing of the game or the menu shows behind it.
///
/// `ui` is the shell's whole view state: the screen reads the values it draws rows for
/// ([`ScreenUi::theme`], [`ScreenUi::renderer`]), the gate
/// ([`ScreenUi::debug_mode`]), the marker ([`ScreenUi::settings`]) and the copy
/// acknowledgement ([`ScreenUi::seed_copy`]). `debug` is the run's **live** switches,
/// so the omni-vision row says what the sight phase is actually doing, and `level` is
/// the run's token — the replay row exists on exactly the frames there is something
/// for it to hand over.
///
/// Bounds are clamped, never asserted, like the help card: on a board too small for a
/// row, that row shows what fits and stops.
pub(super) fn render_settings(
    width: u32,
    height: u32,
    ui: ScreenUi,
    debug: DebugModifiers,
    level: Option<LevelSeed>,
) -> Grid {
    let mut grid = blank_grid(width, height);
    let settings = ui.settings.unwrap_or_default();
    let replay = level.is_some();
    let here = settings.selection(ui.debug_mode, replay);
    let labels = label_column(width);

    draw(&mut grid, labels, HEADING_ROW, HEADING, Category::Interest);
    draw(
        &mut grid,
        close_button_start(width),
        HEADING_ROW,
        CLOSE_BUTTON,
        Category::System,
    );
    draw(
        &mut grid,
        labels,
        DISPLAY_HEADING_ROW,
        DISPLAY_HEADING,
        Category::System,
    );

    // The debug section's heading in **Warning** — the standing "this is not the
    // ordinary thing" cue (§11.2), never new ad-hoc styling — so the boundary between a
    // preference and a playtest switch is visible before either is read (§12.6).
    if ui.debug_mode {
        draw(
            &mut grid,
            labels,
            DEBUG_HEADING_ROW,
            DEBUG_HEADING,
            Category::Warning,
        );
        draw(
            &mut grid,
            labels,
            DEBUG_NOTE_ROW,
            DEBUG_NOTE,
            Category::Ground,
        );
    }

    let column = block_column(width);
    for row in shown_rows(ui.debug_mode, replay) {
        let selected = row == here;
        // The marked row reads in Interest — the goal colour, the thing worth reaching
        // for — against Neutral for the rest, exactly as the title screen's list does.
        // A debug row recedes to Ground when unmarked instead, so the two sections stay
        // told apart by ink as well as by heading.
        let category = match (selected, row.debug_only()) {
            (true, _) => Category::Interest,
            (false, false) => Category::Neutral,
            (false, true) => Category::Ground,
        };
        draw(
            &mut grid,
            column,
            row_y(row),
            &row_text(row, value(row, ui, debug), selected),
            category,
        );
    }

    // The copy acknowledgement (#353), on its own row directly under the control that
    // produced it — the same "did that work?" answer the help panel prints, since the
    // control moved here and its reply came with it.
    if shown_rows(ui.debug_mode, replay).contains(&SettingsRow::Replay) {
        if let Some((text, category)) = ui.seed_copy.acknowledgement() {
            draw(&mut grid, labels, REPLAY_ACK_ROW, text, category);
        }
    }

    draw(
        &mut grid,
        FOOTER_INDENT,
        height.saturating_sub(1),
        FOOTER,
        Category::Ground,
    );
    grid
}

#[cfg(test)]
mod tests;
