//! The **Options tab** (§14 v2/#513) — the game's global settings, a page of the help
//! panel ([`super::help`]) and reachable everywhere the panel is: from the board with
//! `?`, and from the title screen's `Options` entry, which raises the panel on this tab.
//!
//! §14 v2 promises "saves, options, a help screen and a legend". The other three
//! shipped first, and the settings they wanted went wherever there was room: the
//! colour theme lived on the panel's footer "until v2 grows an options screen"
//! (§11.2/#189), and the tile renderer hid behind a `?tiles=1` URL flag (§11.1/#460).
//! This is where both of them now live.
//!
//! **A tab rather than a screen of its own**, and the bar had to make room for it: the
//! *Abilities* tab became **Actions** so four `[Label]`s clear the `[x]` on the v1
//! board's 40 columns (§10.2 — [`super::help`] checks it at compile time). What that
//! buys is that settings are one press from the reference card rather than a surface
//! away, and that the panel keeps being the one modal thing a running game raises —
//! which is also how the §12.6 switches stay reachable mid-run now that their own tab
//! is gone (#459).
//!
//! # What is on it, and what is deliberately not
//!
//! Two **preferences** — the colour theme and the renderer — which are facts about the
//! person at the screen rather than about the run, so they outlive it and are written
//! to the shell's own settings record (#513's split: ending a run must never reset a
//! preference). And, **only in a debug session** (§12.6/#459), the instruments that
//! used to be the help panel's fourth tab: omni-vision, the ghost (#507), and the
//! replay export.
//!
//! It is **not** the pre-run level-options dialog (#298). That one sets the
//! *difficulty of the run you are about to start* and is asked before there is a run;
//! this one sets how the game looks and is answered at any time. Keeping the two apart
//! is why the menu's *Quick play* opens one and its *Options* entry opens the other.
//!
//! # The debug section is gated, and gated visibly
//!
//! A preference and a playtest instrument must never be confusable (§12.6), so the
//! debug rows sit under their own heading and in their own colour, after the widest gap
//! on the screen. With no debug session there is no heading and no row: [`shown_rows`]
//! is what the drawing, the hit-test and the selection walk all read, so a switch that
//! is not drawn cannot be reached by key, by tap, or by a stale
//! [`SettingsUi::selected`].
//!
//! The gate's promises are what they were when the Debug tab held them — never
//! persisted, never in a level-seed token — with one changed by #507: the section used
//! to be *perception only*, and [`SettingsRow::Ghost`] is a rule-bend. What contains it
//! is [`DebugModifiers::ghost`]'s to state; what this screen owes it is that it reads as
//! **on** while it is on, and that the row it disables — the replay export — says so
//! rather than going quietly dead.
//!
//! # Input
//!
//! One vertical list, walked like the title screen's: `↑`/`↓` (and the vertical swipes,
//! which no other tab uses) move the marker, `Enter` fires the marked row, and the
//! panel's own `Escape` / `?` / `[x]` leave. **A press is unbound** (§11.6/appendix 21),
//! so a stray tap on empty panel flips nothing; a row is fired by pressing *the row*.

use super::help::{CONTENT_INDENT, CONTENT_TOP, SECTION_INDENT};
use super::menu::{ENTRY_SPACING, MARKER, NO_MARKER};
use super::{draw, Grid, ScreenUi};
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
    /// **Ghost** (§12.6/#507) — no guard ever detects the player. Debug sessions only,
    /// never persisted, and the one row on this tab that **bends a rule**: it is an
    /// instrument for standing in a run that misbehaved, not a way to play, and using
    /// it costs the run its replay export ([`SettingsRow::Replay`]).
    Ghost,
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
    pub const ALL: [SettingsRow; 5] = [
        SettingsRow::Theme,
        SettingsRow::Renderer,
        SettingsRow::Reveal,
        SettingsRow::Ghost,
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
            SettingsRow::Ghost => "ghost",
            SettingsRow::Replay => "replay",
        }
    }

    /// Whether the row belongs to the **debug** section — the gate that decides which
    /// heading it is drawn under and whether it is drawn at all.
    pub fn debug_only(self) -> bool {
        matches!(
            self,
            SettingsRow::Reveal | SettingsRow::Ghost | SettingsRow::Replay
        )
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
        && matches!(SettingsRow::ALL[3], SettingsRow::Ghost)
        && matches!(SettingsRow::ALL[4], SettingsRow::Replay),
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
            SettingsRow::Reveal | SettingsRow::Ghost => debug,
            SettingsRow::Replay => debug && replay,
            _ => true,
        })
        .collect()
}

/// The Options tab's **view state**, owned by the shell exactly like
/// [`ScreenUi::help_tab`](super::ScreenUi) — it changes no world and costs no turn
/// (§12.1). It is a plain field rather than an `Option`, because "is this tab showing"
/// is already [`ScreenUi::help_tab`]'s to answer; this is only *where the marker rests*
/// while it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SettingsUi {
    /// Which row the marker rests on. The [`Default`] is the theme row, the first
    /// setting on the tab.
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

/// The two section headings, drawn alike in the panel's own heading colour. `DISPLAY`
/// is what a player came here for; `DEBUG` names the gate rather than hiding it (§12.6),
/// and naming it is the whole of what it has to do.
const DISPLAY_HEADING: &str = "DISPLAY";
const DEBUG_HEADING: &str = "DEBUG";

/// The footer this tab asks the panel to print (§11.6's no-trap rule in prose, beside
/// the `[x]` that is its touch half). It names the two keys only this tab answers, and
/// the way out — every other tab prints the panel's own hint.
pub(super) const FOOTER: &str = "↑↓ choose · Enter sets · Esc closes";

/// The `DISPLAY` heading's row, and the first setting under it — measured from the
/// panel's own content top, so this tab starts where every other tab's content does.
const DISPLAY_HEADING_ROW: u32 = CONTENT_TOP;
const FIRST_DISPLAY_ROW: u32 = DISPLAY_HEADING_ROW + 2;

/// How many rows the display section occupies, from its first row to the blank after
/// its last — two settings at [`ENTRY_SPACING`].
const DISPLAY_ROWS: u32 = 2 * ENTRY_SPACING;

/// The `DEBUG` heading and the first switch under it, the same two rows apart the
/// display section's heading and first row are. The gap **above** the heading is
/// deliberate and is the widest on the screen: the sections are different kinds of
/// thing, and the space is the first thing that says so.
const DEBUG_HEADING_ROW: u32 = FIRST_DISPLAY_ROW + DISPLAY_ROWS + 1;
const FIRST_DEBUG_ROW: u32 = DEBUG_HEADING_ROW + 2;

/// The row the copy acknowledgement is printed on — directly under the replay row it
/// answers, exactly as the help panel prints its own (#353). The replay row is the
/// **third** debug row since the ghost switch landed beside omni-vision (#507).
const REPLAY_ACK_ROW: u32 = FIRST_DEBUG_ROW + 2 * ENTRY_SPACING + 1;

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

/// The **replay** row's value — the longest the screen can draw, and the only one that
/// is a phrase rather than a state, so it is named for the width check below.
const REPLAY_VALUE: &str = "copy as link";

/// …and what that row says once the run has been **ghosted** (§12.6/#507): the export
/// is refused for the rest of the run, so the row stops offering it and reads as what
/// it now is.
///
/// It stays **drawn** rather than vanishing, and the difference matters. A row that
/// disappeared would look like a run with no token — the other reason this row is
/// absent — and leave the player nothing to press and no way to be told why. Drawn and
/// unavailable, a press answers ([`SeedCopy::Refused`](super::help::SeedCopy)), and the
/// answer names the switch that did it.
const REPLAY_REFUSED: &str = "unavailable";

/// The value column's own bound: the widest value any row can draw. The block is
/// centred from this plus [`LABEL_WIDTH`], so it is what keeps the widest row inside
/// the v1 board (§10.2).
const VALUE_MAX: usize = 12;

/// The widest row this tab can draw, in cells — the marker, the padded label, and the
/// longest value.
const ROW_WIDTH: u32 = (MARKER.len() + LABEL_WIDTH + VALUE_MAX) as u32;

/// The column a row's text starts at: the **marker's** column, one in from the panel's
/// left margin, so the labels land on [`CONTENT_INDENT`] — where every other tab's
/// content sits — and the marker hangs into the margin beside the section headings.
const BLOCK_COLUMN: u32 = CONTENT_INDENT - MARKER.len() as u32;

// The widest row must fit the v1 board with its one-cell right margin (§10.2/§2.3), and
// the longest value must fit the column the row was measured from — [`draw`] clips in
// silence, so a value that outgrew it would arrive half-drawn on a player's screen
// rather than failing the build.
const _: () = {
    assert!(
        REPLAY_VALUE.len() <= VALUE_MAX && REPLAY_REFUSED.len() <= VALUE_MAX,
        "a settings value is too long for the value column — shorten it or widen \
         VALUE_MAX (see render::settings)",
    );
    assert!(
        BLOCK_COLUMN + ROW_WIDTH < LevelConfig::V1.width,
        "the settings rows do not fit the v1 board's width (see render::settings)",
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
        SettingsRow::Ghost => FIRST_DEBUG_ROW + ENTRY_SPACING,
        SettingsRow::Replay => FIRST_DEBUG_ROW + 2 * ENTRY_SPACING,
    }
}

/// What a row's **value** column says, read from the live values every time — never
/// from a flag this screen keeps of its own, so a row and the thing it names cannot
/// disagree (the Debug tab's rule, kept).
///
/// `ghosted` is the run's **latch** (§12.6/#507), not the ghost switch: the replay row
/// answers to *has this run ever been ghosted*, so switching the ghost back off leaves
/// the export refused exactly as the run's history says it should.
fn value(row: SettingsRow, ui: ScreenUi, debug: DebugModifiers, ghosted: bool) -> &'static str {
    match row {
        SettingsRow::Theme => match ui.theme {
            Theme::Dark => "dark",
            Theme::Light => "light",
        },
        SettingsRow::Renderer => ui.renderer.label(),
        SettingsRow::Reveal => on_off(debug.reveal_whole_level),
        SettingsRow::Ghost => on_off(debug.ghost),
        SettingsRow::Replay => {
            if ghosted {
                REPLAY_REFUSED
            } else {
                REPLAY_VALUE
            }
        }
    }
}

/// A switch's value column: the two words the debug section's switches read by, spelled
/// once so the two of them cannot drift apart.
fn on_off(flag: bool) -> &'static str {
    if flag {
        "on"
    } else {
        "off"
    }
}

/// A row as drawn, marker and all — one function so the drawing and the width the
/// block is centred by can never disagree.
fn row_text(row: SettingsRow, value: &str, selected: bool) -> String {
    let marker = if selected { MARKER } else { NO_MARKER };
    format!("{marker}{:<LABEL_WIDTH$}{value}", row.label())
}

/// The row screen row `y` lands on, or `None` for a press this tab does not claim —
/// which the panel then swallows, like a press anywhere else on its body (§11.6/#513).
///
/// **The whole row is the target**, at any column, as it is on the title screen:
/// nothing else is drawn on a row, and a generous target is the difference between a
/// panel that works on a phone and one that does not. The blank row between settings
/// ([`ENTRY_SPACING`]) is the buffer that keeps a low tap off the next one — which is
/// why this takes no `x` at all.
///
/// It takes the same `debug` and `replay` the drawing does, so a switch this session
/// does not have has no hit region where it would have been.
pub(super) fn settings_hit(debug: bool, replay: bool, y: u32) -> Option<SettingsRow> {
    shown_rows(debug, replay)
        .into_iter()
        .find(|&row| row_y(row) == y)
}

/// Draw the **Options** tab (§14 v2/#513) into the panel's body: the two sections, the
/// marked row, and the copy acknowledgement.
///
/// `ui` is the shell's whole view state: the tab reads the values it draws rows for
/// ([`ScreenUi::theme`], [`ScreenUi::renderer`]), the gate ([`ScreenUi::debug_mode`]),
/// the marker ([`ScreenUi::settings`]) and the copy acknowledgement
/// ([`ScreenUi::seed_copy`]). `debug` is the run's **live** switches, so the
/// omni-vision row says what the sight phase is actually doing, and `level` is the run's
/// token — the replay row exists on exactly the frames there is something for it to
/// hand over. `ghosted` is the run's ghost **latch** (§12.6/#507), which is what turns
/// that row from an offer into a refusal.
///
/// Bounds are clamped, never asserted, like the rest of the card: on a board too small
/// for a row, that row shows what fits and stops.
pub(super) fn draw_settings(
    grid: &mut Grid,
    ui: ScreenUi,
    debug: DebugModifiers,
    level: Option<LevelSeed>,
    ghosted: bool,
) {
    let replay = level.is_some();
    let here = ui.settings.selection(ui.debug_mode, replay);

    draw(
        grid,
        SECTION_INDENT,
        DISPLAY_HEADING_ROW,
        DISPLAY_HEADING,
        Category::System,
    );

    // **Both headings read alike** — System, the panel's own heading colour, as
    // `THIS RUN` and `MODIFIERS` do one tab over. The debug section was drawn in Warning
    // to mark the gate, and that made it the brightest thing on a tab whose *preferences*
    // are what a player came for: an alarm colour over two switches most sessions never
    // see. The word `DEBUG` names the gate perfectly well on its own, and the widest gap
    // on the tab sets it apart (§12.6 — the split has to be unmistakable, not loud).
    if ui.debug_mode {
        draw(
            grid,
            SECTION_INDENT,
            DEBUG_HEADING_ROW,
            DEBUG_HEADING,
            Category::System,
        );
    }

    for row in shown_rows(ui.debug_mode, replay) {
        // The marked row reads in Interest — the goal colour, the thing worth reaching
        // for — against Neutral for the rest, exactly as the title screen's list does.
        // **Every row reads alike**, debug rows included: they are live controls, and
        // dimming them said *inert* (§11.2's Ground) about the one section where a press
        // does the most. The heading over them carries the gate on its own.
        let selected = row == here;
        let category = if selected {
            Category::Interest
        } else {
            Category::Neutral
        };
        draw(
            grid,
            BLOCK_COLUMN,
            row_y(row),
            &row_text(row, value(row, ui, debug, ghosted), selected),
            category,
        );
    }

    // The copy acknowledgement (#353), on its own row directly under the row that
    // produced it — the same "did that work?" answer the Level info tab prints under
    // its own control, since the replay control moved here and its reply came with it.
    if shown_rows(ui.debug_mode, replay).contains(&SettingsRow::Replay) {
        if let Some((text, category)) = ui.seed_copy.acknowledgement() {
            draw(grid, CONTENT_INDENT, REPLAY_ACK_ROW, text, category);
        }
    }
}

#[cfg(test)]
mod tests;
