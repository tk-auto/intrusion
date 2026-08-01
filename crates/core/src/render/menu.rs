//! The title screen and main menu (§11.4/§14 v1, #268) — the surface the page
//! opens on, and the only thing between a load and a run.
//!
//! It is deliberately **thin**. §14 v1 is "quick play, and nothing else", and its
//! warning is about exactly this kind of screen: *"everything outside the loop was
//! scaffolding around an unanswered question. Don't do that again."* So the menu
//! offers the two things that actually start a run today — **Quick play** (a fresh
//! seeded facility off the clock) and **Seed play** (the level-seed token re-entry
//! that used to be the always-on seed bar, §13.1/#110/#245) — and lists **Options**
//! (§14 v2) and **Story mode** (§14 v3) as visibly *later*, inert entries. They are
//! there so the menu has room to grow, and they do nothing at all: the moment one of
//! them acts, it is v2/v3 work, not this screen.
//!
//! **Drawn in the character grid** (§11.1), like the help card ([`super::help`]) and
//! for the same reasons: the whole screen is a pure function of its view state, so
//! it prints as text and every row is pinned by a native test. Only the seed *text
//! box* is DOM — a canvas cannot raise a phone's keyboard — and it floats in the
//! band this screen deliberately leaves blank (see [`render_menu`]).
//!
//! **Nothing here traps a touch user** (§11.6 — the failure the old options dialog
//! shipped: a screen that could be opened but not closed by touch). Every entry row
//! is a full-width tap target ([`menu_hit`]), the seed prompt carries its own DOM
//! *back* button beside the *play* one, and the footer always spells out the way on.

use super::help::{theme_control, theme_control_len, theme_control_start, FOOTER_INDENT};
use super::{blank_grid, draw, Grid};
use crate::category::Category;
use crate::difficulty::Difficulty;

/// The entries on the main menu, top to bottom. Two start a run today; the other
/// two are the §14 v2/v3 surfaces, listed as *later* and inert ([`enabled`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MenuEntry {
    /// A fresh seeded facility, straight in — the §14 v1 loop and the default
    /// selection, so the common case is one keypress (or one tap) from a load.
    #[default]
    QuickPlay,
    /// Enter a level-seed token and play the run it names (§13.1/#110/#245) —
    /// what the always-on seed bar used to do, now behind a menu entry.
    SeedPlay,
    /// Settings (§14 v2 "options"; #189 light mode, #237 difficulty). **Later.**
    Options,
    /// The campaign (§14 v3). **Later.**
    StoryMode,
}

impl MenuEntry {
    /// Every entry in menu order — the drawing order, the hit-test order, and the
    /// cycle order. A new entry is one line here.
    pub const ALL: [MenuEntry; 4] = [
        MenuEntry::QuickPlay,
        MenuEntry::SeedPlay,
        MenuEntry::Options,
        MenuEntry::StoryMode,
    ];

    /// The entry's label as drawn.
    pub fn label(self) -> &'static str {
        match self {
            MenuEntry::QuickPlay => "Quick play",
            MenuEntry::SeedPlay => "Seed play",
            MenuEntry::Options => "Options",
            MenuEntry::StoryMode => "Story mode",
        }
    }

    /// Whether choosing the entry does anything. The §14 v2/v3 entries answer
    /// `false`: they are drawn dim and tagged *later*, selection steps over them
    /// ([`next`](Self::next)/[`prev`](Self::prev)), and activating one — by key or
    /// by tap — is a no-op. **This is the whole of their behaviour** (#268): a menu
    /// with room to grow, with nothing yet growing in it.
    pub fn enabled(self) -> bool {
        matches!(self, MenuEntry::QuickPlay | MenuEntry::SeedPlay)
    }

    /// The next **enabled** entry below this one, wrapping past the last. Disabled
    /// entries are stepped over rather than landed on: the selection marker only
    /// ever rests somewhere that pressing Enter does something.
    #[must_use]
    pub fn next(self) -> Self {
        self.seek(1)
    }

    /// The previous **enabled** entry above this one, wrapping past the first.
    #[must_use]
    pub fn prev(self) -> Self {
        self.seek(Self::ALL.len() - 1) // one step backwards, modulo the ring
    }

    /// Walk `step` positions round the entry ring at a time until an enabled entry
    /// comes up, giving up (and staying put) after a full lap — so a menu whose
    /// entries were *all* disabled would freeze rather than spin.
    fn seek(self, step: usize) -> Self {
        let n = Self::ALL.len();
        let start = Self::ALL.iter().position(|&e| e == self).unwrap_or(0);
        (1..=n)
            .map(|i| Self::ALL[(start + step * i) % n])
            .find(|e| e.enabled())
            .unwrap_or(self)
    }
}

/// Which of the title screen's surfaces is showing (#268).
///
/// The menu is a small stack of full screens rather than one screen with flags: an
/// enum makes "the entry list and the seed prompt at once" unrepresentable, where a
/// second `bool` beside the first would make it merely unlikely. Every surface is
/// drawn by [`render_menu`], hit-tested by [`menu_hit`], and reached and left through
/// the shell's one [`MenuNav`](crate::MenuNav) handler.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MenuScreen {
    /// The **entry list** — the title block and [`MenuEntry::ALL`]. The root: there
    /// is nowhere further back from it.
    #[default]
    Entries,
    /// The **seed prompt** — the sub-screen [`MenuEntry::SeedPlay`] opens, where the
    /// DOM text box takes a level-seed token. Escape (or the box's own *back* button)
    /// returns to the list.
    SeedPrompt,
    /// The **level options** dialog (§14 v2/#298) — the sub-screen
    /// [`MenuEntry::QuickPlay`] opens, carrying the difficulty slider and the play
    /// control. A *pre-run* dialog, deliberately not the (still inert)
    /// [`MenuEntry::Options`] entry, which is §14 v2's global settings screen.
    LevelOptions,
}

/// The controls on the level-options dialog that the selection marker rests on — the
/// slider is set by `←`/`→` (or by tapping a stop) at any time, so only these two are
/// in the up/down ring.
///
/// [`Play`](OptionsControl::Play) is the [`Default`] so the fast path from a load
/// stays **Enter, Enter**: the dialog earns its place by driving a real mechanic
/// (§14 v1's scaffolding warning), not by making the common case longer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OptionsControl {
    /// Start the run at the chosen difficulty.
    #[default]
    Play,
    /// Return to the entry list — the drawn, tappable *back* §11.6 wants beside the
    /// way on, so the dialog is never a screen a touch player can open and not leave.
    Back,
}

impl OptionsControl {
    /// Both controls, in drawing order — also the order the marker walks.
    pub const ALL: [OptionsControl; 2] = [OptionsControl::Play, OptionsControl::Back];

    /// The control's label as drawn.
    pub fn label(self) -> &'static str {
        match self {
            OptionsControl::Play => "Play",
            OptionsControl::Back => "Back",
        }
    }

    /// The other control — the ring is two long, so up and down are the same step.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            OptionsControl::Play => OptionsControl::Back,
            OptionsControl::Back => OptionsControl::Play,
        }
    }
}

/// The menu's **view state**, owned by the shell exactly like
/// [`ScreenUi`](super::ScreenUi) — it changes no world and costs no turn (§12.1).
/// A shell keeps `Some(MenuUi)` on [`ScreenUi::menu`](super::ScreenUi::menu) while
/// the menu is up and clears it the moment a run starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MenuUi {
    /// Which entry the selection marker rests on. The [`Default`] is
    /// [`MenuEntry::QuickPlay`], so a load is one Enter (or one tap) from playing.
    pub selected: MenuEntry,
    /// Which surface is showing. The [`Default`] is
    /// [`MenuScreen::Entries`](MenuScreen::Entries), the screen a load opens on.
    pub screen: MenuScreen,
    /// Where the level-options slider sits (§12.6/#297). The [`Default`] is
    /// [`Difficulty::Standard`] — quick play exactly as it is — so a player who never
    /// touches the slider gets the game they got before it existed.
    pub difficulty: Difficulty,
    /// Which level-options control the marker rests on. Ignored on the other screens.
    pub options_control: OptionsControl,
}

impl MenuUi {
    /// Whether the seed prompt is the surface showing — the one question most of this
    /// module asks of [`screen`](Self::screen), kept as a name rather than a
    /// comparison repeated at a dozen sites.
    fn seed_prompt(self) -> bool {
        self.screen == MenuScreen::SeedPrompt
    }

    /// Whether the level-options dialog is the surface showing.
    fn level_options(self) -> bool {
        self.screen == MenuScreen::LevelOptions
    }
}

/// The title, spaced out — the one piece of ornament on the screen. Spacing the
/// capitals is the whole "logo": it reads as a title at a glance without a block-art
/// asset that a 40-column board could not hold (§10.2).
const TITLE: &str = "I N T R U S I O N";

/// The one line under the title: what the game is, in the §1/§4.5 terms that matter
/// — you dug the way in, and it is also the only way out.
const TAGLINE: &str = "one tunnel in, the same tunnel out";

/// The footer of the entry list. It names **both** input paths on purpose (§11.6):
/// a touch player who cannot see a keyboard still reads that tapping a row plays it.
///
/// Left-aligned at [`FOOTER_INDENT`] rather than centred, and pruned to fit, because
/// the row's right edge now belongs to the theme control (#189) — the footer is one
/// strip with prose on the left and the control on the right, the same on this screen
/// as on the help panel. A test pins that the two never meet.
const MENU_FOOTER: &str = "↑↓ choose · Enter/tap plays";

/// The footer of the seed prompt — the way back out, spelled out beside the box's
/// own *back* button, so the sub-screen is never a dead end (§11.6's no-trap rule).
const SEED_FOOTER: &str = "Esc or [back] returns to the menu";

/// The heading and the two instruction lines of the seed prompt
/// (§13.1/#110/#245/#333). The second line says what a token *looks* like, so a
/// player who has one in hand can tell at a glance whether they have the whole thing
/// — it is a fixed 18 letters, and a truncated paste is the likely mistake. It
/// replaces the bare-seed promise that used to stand here: a number named a preset
/// rather than a run, and no longer decodes at all (#333).
const SEED_HEADING: &str = "SEED PLAY";
const SEED_LINES: [&str; 2] = [
    "type or paste a level-seed token",
    "18 letters, like prbjdokbxcqgjnrnco",
];

/// The footer of the level-options dialog. It names the slider's keys, the way **on**
/// and the way **back** — §11.6's no-trap rule, which is the exact failure the old
/// options dialog shipped: a screen that could be opened and not left. The drawn
/// *Play* and *Back* rows are the touch half of the same promise, so the prose does
/// not have to spell "tap" as the entry list's does.
const OPTIONS_FOOTER: &str = "←→ set · Enter · Esc back";

/// The heading of the level-options dialog, and the caption over the slider. Both are
/// **meta vocabulary** (§11.8) — they name the run's setup rather than anything inside
/// the facility, so they read as plainly as `LEVEL SEED` does on the help panel.
const OPTIONS_HEADING: &str = "LEVEL OPTIONS";
const DIFFICULTY_CAPTION: &str = "DIFFICULTY";

/// The slider's stops: a ring for each position it can rest at, and the filled one it
/// rests at now. Same width, so the track does not reflow as the slider moves — the
/// [`MARKER`]/[`NO_MARKER`] rule, one row over.
///
/// The stops are drawn in Neutral against a Ground [`TRACK_FILL`], so the row reads as
/// *five positions on a rail* at a glance. Drawn as five bare dots with nothing
/// between them it read as a decorative row rather than as a control, and a stop had
/// to be found before it could be aimed at.
const STOP: &str = "○";
const STOP_HERE: &str = "●";

/// What runs **between** the stops — the slider's rail. Faint (Ground), so it joins
/// the stops into one control without competing with them for the eye.
const TRACK_FILL: &str = "·";

/// Cells from one slider stop to the next. Wide enough that a stop's **tap band** is
/// a comfortable target on a phone (§11.6) — six cells each, against the entry list's
/// full-width rows — and narrow enough that the whole five-stop track fits the v1
/// board's 40 columns with room either side.
const STOP_SPACING: u32 = 6;

/// The tag drawn after an entry that is not built yet (§14 v2/v3) — short, so the
/// row reads as an entry with a note rather than a sentence.
const LATER_TAG: &str = " — later";

/// The marker on the selected row, and the blank that holds its place on every
/// other row so the labels never shift as the selection moves (the two are the
/// same width — asserted below).
pub(super) const MARKER: &str = "> ";
pub(super) const NO_MARKER: &str = "  ";

/// Rows between one entry and the next: one drawn row, one blank. The gap is what
/// makes a full-width row a comfortable tap target — a mis-aimed tap lands on the
/// blank between entries and does nothing, never on the neighbour (§11.6).
pub(super) const ENTRY_SPACING: u32 = 2;

/// Rows from the title to the last entry: title, blank, tagline, three blanks, then
/// the four entries at [`ENTRY_SPACING`] apart. Used to centre the block vertically,
/// so the screen looks composed at any board height.
const BLOCK_ROWS: u32 = 6 + (MenuEntry::ALL.len() as u32 - 1) * ENTRY_SPACING + 1;

/// Where the title, the tagline, and the first entry sit on a screen `height` tall:
/// the whole block centred vertically, never above row 1. Shared by the drawing and
/// [`menu_hit`], so a tap lands on exactly the row that was drawn.
fn rows(height: u32) -> (u32, u32, u32) {
    let top = height.saturating_sub(BLOCK_ROWS) / 2;
    let title = top.max(1);
    (title, title + 2, title + 6)
}

/// Where the seed prompt's title, tagline and heading sit — the title **near the
/// top**, not centred as on the list. The prompt's text has to clear the middle of
/// the screen for the DOM box that floats there, and the centred block does not: on
/// the v1 board its title row and the heading would land on each other. Moving the
/// title up is what buys the clear band, so the two are one decision, here.
/// The level-options dialog's rows, as offsets from its title row. Named rather than
/// arithmetic at the draw sites, because the drawing and [`menu_hit`] both walk them
/// and a tap must land on exactly the row that was drawn.
///
/// The slider is three rows — the track, the position's name, and the line saying what
/// the position will actually do — because a bare row of dots says *five of something*
/// and not which one you are on.
const OPTIONS_TAGLINE: u32 = 2;
const OPTIONS_HEADING_ROW: u32 = 6;
const OPTIONS_CAPTION_ROW: u32 = 8;
const OPTIONS_TRACK_ROW: u32 = 10;
const OPTIONS_NAME_ROW: u32 = 12;
const OPTIONS_BLURB_ROW: u32 = 13;
const OPTIONS_FIRST_CONTROL: u32 = 16;

/// Rows from the level-options title to its last control — the block's height, used
/// to centre it exactly as [`BLOCK_ROWS`] centres the entry list.
const OPTIONS_BLOCK_ROWS: u32 =
    OPTIONS_FIRST_CONTROL + (OptionsControl::ALL.len() as u32 - 1) * ENTRY_SPACING + 1;

/// The level-options dialog's title row; every other row is a named offset from it.
/// The block is centred like the entry list's and, unlike the seed prompt's, needs no
/// clear band — the dialog is glyphs all the way down, with no DOM floating over it.
fn options_title_row(height: u32) -> u32 {
    (height.saturating_sub(OPTIONS_BLOCK_ROWS) / 2).max(1)
}

/// The screen row the level-options control at `index` is drawn on — the counterpart
/// of [`entry_row`] for the dialog, at the same [`ENTRY_SPACING`], so a mis-aimed tap
/// lands on the blank between the two and does nothing.
fn options_control_row(height: u32, index: usize) -> u32 {
    options_title_row(height) + OPTIONS_FIRST_CONTROL + index as u32 * ENTRY_SPACING
}

/// The whole slider track's width: [`Difficulty::ALL`] stops, [`STOP_SPACING`] apart.
const TRACK_WIDTH: u32 = (Difficulty::ALL.len() as u32 - 1) * STOP_SPACING + 1;

/// The column the slider stop at `index` is drawn on — the track centred, the stops
/// evenly spread across it. Shared by the drawing and [`menu_hit`].
fn stop_column(width: u32, index: usize) -> u32 {
    centre(width, TRACK_WIDTH) + index as u32 * STOP_SPACING
}

/// Which slider stop a press at column `x` lands on, or `None` for a press past
/// either end of the track.
///
/// **Each stop owns a band, not a cell** (§11.6): a single-cell dot is not a target a
/// finger can hit, so the band runs half the spacing either side and the whole track
/// is live from the first stop's left edge to the last one's right. Between-stop
/// columns resolve to the nearer stop rather than to nothing — on a track there is no
/// gap that could sensibly mean *neither*.
fn stop_hit(width: u32, x: u32) -> Option<Difficulty> {
    let half = STOP_SPACING / 2;
    let first = stop_column(width, 0).saturating_sub(half);
    let last = stop_column(width, Difficulty::ALL.len() - 1) + half;
    if x < first || x > last {
        return None;
    }
    let index = ((x - first) / STOP_SPACING) as usize;
    Difficulty::ALL
        .get(index.min(Difficulty::ALL.len() - 1))
        .copied()
}

fn seed_rows(height: u32) -> (u32, u32, u32) {
    let title = (height / 12).max(1);
    (title, title + 2, title + 5)
}

/// The screen row entry `index` is drawn on — the counterpart of [`rows`] for the
/// list itself.
fn entry_row(height: u32, index: usize) -> u32 {
    rows(height).2 + index as u32 * ENTRY_SPACING
}

/// The entry text as drawn, marker and all: `> Quick play`, or `  Options — later`
/// for one of the §14 v2/v3 entries. One function so the drawing and the block's
/// width measurement can never disagree.
fn entry_text(entry: MenuEntry, selected: bool) -> String {
    let marker = if selected { MARKER } else { NO_MARKER };
    let tag = if entry.enabled() { "" } else { LATER_TAG };
    format!("{marker}{}{tag}", entry.label())
}

/// The column the entry block starts at: the widest entry line centred, with every
/// row left-aligned inside it — a ragged-right list reads as a list, where centring
/// each row individually would make the labels jitter as the selection moves.
fn entry_column(width: u32) -> u32 {
    let widest = MenuEntry::ALL
        .iter()
        .map(|&e| entry_text(e, true).chars().count() as u32)
        .max()
        .unwrap_or(0);
    centre(width, widest)
}

/// The column a `len`-wide run of text starts at to sit centred on a `width` screen,
/// clamped to 0 on a board too narrow to hold it.
pub(super) fn centre(width: u32, len: u32) -> u32 {
    width.saturating_sub(len) / 2
}

/// Draw `text` centred on row `y`.
fn draw_centred(grid: &mut Grid, y: u32, text: &str, category: Category) {
    let len = text.chars().count() as u32;
    draw(grid, centre(grid.width(), len), y, text, category);
}

/// What a press on the title screen lands on (§11.6/#268) — the menu's counterpart
/// of [`HelpHit`](super::HelpHit), and the touch half of
/// [`menu_nav_for_key`](crate::menu_nav_for_key).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuHit {
    /// An entry row — choose it (start the run, or open the seed prompt).
    Entry(MenuEntry),
    /// The footer's `theme [n]` control — flip the colour table (§11.2/#189). The
    /// same control the help panel carries, in the same corner: the title screen is
    /// the first thing a player sees, so it is where a theme they cannot read is
    /// most worth being able to change.
    ToggleTheme,
    /// A **slider stop** on the level-options dialog (#298) — set the difficulty to
    /// that position. A stop is set by tapping it directly rather than by tapping a
    /// nudge control: five stops is few enough to aim at, and it is the one gesture
    /// that cannot leave a finger holding a repeat.
    Difficulty(Difficulty),
    /// A **control row** on the level-options dialog — play at the chosen difficulty,
    /// or go back to the entry list.
    OptionsControl(OptionsControl),
}

/// Which [`MenuHit`] screen cell `(x, y)` lands on, or `None` for a press that hit
/// nothing and is swallowed. A shell maps the tap to a screen cell and asks this.
///
/// **The whole row is the target** for an entry, at any column: nothing else is
/// drawn on an entry's row, and a generous target is the difference between a menu
/// that works on a phone and one that does not. The blank row between entries
/// ([`ENTRY_SPACING`]) is the buffer that keeps a low tap off the next entry. The
/// theme control is the one thing tested by column as well, because it shares its
/// row with the footer prose — and, like the panel's, its **label** is inside the
/// target, not only the bracketed key.
///
/// The **level options** dialog is targeted the same way (#298): its two control rows
/// are full-width, and the slider's five stops each own a band of [`STOP_SPACING`]
/// cells on the track row ([`stop_hit`]) rather than the single cell their dot is
/// drawn on. Every control it has is reachable by finger, which is the half of §11.6
/// the old options dialog never shipped.
///
/// The seed prompt answers `None` everywhere — its controls are the DOM box's own
/// *play* and *back* buttons, which handle their taps before the board sees them,
/// and a theme control under a floating text box is a control half hidden.
#[must_use]
pub fn menu_hit(width: u32, height: u32, ui: MenuUi, x: u32, y: u32) -> Option<MenuHit> {
    if ui.seed_prompt() {
        return None;
    }
    if ui.level_options() {
        if height > 0 && y == height - 1 {
            let theme = theme_control_start(width);
            return (x >= theme && x < theme + theme_control_len()).then_some(MenuHit::ToggleTheme);
        }
        if y == options_title_row(height) + OPTIONS_TRACK_ROW {
            return stop_hit(width, x).map(MenuHit::Difficulty);
        }
        return OptionsControl::ALL
            .iter()
            .enumerate()
            .find(|&(i, _)| options_control_row(height, i) == y)
            .map(|(_, &control)| MenuHit::OptionsControl(control));
    }
    if height > 0 && y == height - 1 {
        let theme = theme_control_start(width);
        return (x >= theme && x < theme + theme_control_len()).then_some(MenuHit::ToggleTheme);
    }
    MenuEntry::ALL
        .iter()
        .enumerate()
        .find(|&(i, _)| entry_row(height, i) == y)
        .map(|(_, &entry)| MenuHit::Entry(entry))
}

/// Render the title screen (§11.4/§14, #268) — the whole `width × height` screen,
/// not an overlay, so the shell paints it through the one path it paints a frame
/// with and nothing of the game shows behind it.
///
/// One screen per [`MenuScreen`]:
///
/// - the **entry list** — the title block centred, the four entries with the
///   selection marker, and the footer that names both ways to choose;
/// - the **seed prompt** — the same title, moved up the screen, over the
///   instructions for a level-seed token, with **the middle band left deliberately
///   blank**. That band is where the shell's DOM text box floats (a canvas cannot
///   raise a phone's keyboard, so the box has to be real markup); leaving the space
///   empty rather than aligning glyphs to it means the two never fight over a row,
///   at any fit.
/// - the **level options** dialog (#298) — the same title block centred, the
///   difficulty slider, and the *Play* and *Back* controls. All glyphs: nothing of it
///   is DOM, so unlike the seed prompt it needs no clear band.
///
/// Bounds are clamped, never asserted (like the help card): on a board too small
/// for a row, that row shows what fits and stops.
pub(super) fn render_menu(width: u32, height: u32, ui: MenuUi) -> Grid {
    let mut grid = blank_grid(width, height);
    let (title_row, tagline_row, heading_row) = match ui.screen {
        MenuScreen::SeedPrompt => seed_rows(height),
        MenuScreen::LevelOptions => {
            let title = options_title_row(height);
            (title, title + OPTIONS_TAGLINE, title + OPTIONS_HEADING_ROW)
        }
        MenuScreen::Entries => rows(height),
    };

    draw_centred(&mut grid, title_row, TITLE, Category::Interest);
    draw_centred(&mut grid, tagline_row, TAGLINE, Category::Ground);

    if ui.seed_prompt() {
        draw_seed_prompt(&mut grid, heading_row);
    } else if ui.level_options() {
        draw_level_options(&mut grid, height, ui);
    } else {
        let column = entry_column(width);
        for (i, &entry) in MenuEntry::ALL.iter().enumerate() {
            let selected = entry == ui.selected;
            let category = match (entry.enabled(), selected) {
                // The selection reads in Interest — the goal colour, the thing worth
                // reaching for — against Neutral for the rest of the live entries and
                // Ground for the ones that do nothing yet (§11.2).
                (true, true) => Category::Interest,
                (true, false) => Category::Neutral,
                (false, _) => Category::Ground,
            };
            draw(
                &mut grid,
                column,
                entry_row(height, i),
                &entry_text(entry, selected),
                category,
            );
        }
    }

    let footer_row = height.saturating_sub(1);
    let footer = match ui.screen {
        MenuScreen::SeedPrompt => SEED_FOOTER,
        MenuScreen::LevelOptions => OPTIONS_FOOTER,
        MenuScreen::Entries => MENU_FOOTER,
    };
    draw(
        &mut grid,
        FOOTER_INDENT,
        footer_row,
        footer,
        Category::Ground,
    );
    // The theme control, in the same corner of the same row as the help panel's
    // (#189) — label and key together in System, so the word is visibly part of the
    // button and is a target in its own right. Not on the seed prompt: the DOM text
    // box floats over that screen, and a control it might cover is worse than none.
    if !ui.seed_prompt() {
        draw(
            &mut grid,
            theme_control_start(width),
            footer_row,
            &theme_control(),
            Category::System,
        );
    }
    grid
}

/// Draw the seed prompt's heading and instructions from `heading` down, high enough
/// on the screen that the band around the **middle** stays clear for the DOM text box
/// that floats there (see [`render_menu`]). The box is centred in the viewport and the
/// canvas is centred in the viewport too, so the middle of the grid is where it lands
/// at every fit — the one row-level coupling between the two, kept as slack rather
/// than arithmetic, and asserted below.
/// Draw the level-options dialog (#298): the heading, the difficulty slider, and the
/// two controls.
///
/// The slider is **three rows and a caption**, not a row of dots — the track says
/// where you are on the axis, the name says what that position is called, and the
/// blurb says what it will actually do to the run. That last row is why the dialog
/// earns its place at all (§14 v1's scaffolding warning): a control that named a
/// number would be a screen wrapped around an unanswered question, and one that says
/// *two rules bent against you* is driving a real mechanic.
///
/// What it deliberately does **not** show is *which* rules. The seed is not drawn
/// until the run starts, so naming them here would mean either deciding the seed at
/// dialog time or printing a guess; the resolved set is the help panel's Level info
/// tab to show once there is a run to describe.
fn draw_level_options(grid: &mut Grid, height: u32, ui: MenuUi) {
    let title = options_title_row(height);
    let width = grid.width();
    draw_centred(
        grid,
        title + OPTIONS_HEADING_ROW,
        OPTIONS_HEADING,
        Category::System,
    );
    draw_centred(
        grid,
        title + OPTIONS_CAPTION_ROW,
        DIFFICULTY_CAPTION,
        Category::Neutral,
    );

    // The rail first, then the stops over it: the current one filled and lifted into
    // Interest — the same "the thing worth reaching for" cue the entry list's marker
    // carries — the rest as open rings in Neutral, on a Ground line joining them.
    let track_row = title + OPTIONS_TRACK_ROW;
    draw(
        grid,
        stop_column(width, 0),
        track_row,
        &TRACK_FILL.repeat(TRACK_WIDTH as usize),
        Category::Ground,
    );
    for (i, &position) in Difficulty::ALL.iter().enumerate() {
        let here = position == ui.difficulty;
        draw(
            grid,
            stop_column(width, i),
            track_row,
            if here { STOP_HERE } else { STOP },
            if here {
                Category::Interest
            } else {
                Category::Neutral
            },
        );
    }
    draw_centred(
        grid,
        title + OPTIONS_NAME_ROW,
        ui.difficulty.label(),
        Category::Interest,
    );
    draw_centred(
        grid,
        title + OPTIONS_BLURB_ROW,
        ui.difficulty.blurb(),
        Category::Ground,
    );

    // The controls, marked and columned exactly as the entry list's rows are, so the
    // two screens are read the same way.
    let column = control_column(width);
    for (i, &control) in OptionsControl::ALL.iter().enumerate() {
        let selected = control == ui.options_control;
        draw(
            grid,
            column,
            options_control_row(height, i),
            &control_text(control, selected),
            if selected {
                Category::Interest
            } else {
                Category::Neutral
            },
        );
    }
}

/// A level-options control as drawn, marker and all — the [`entry_text`] of the
/// dialog, and one function for the same reason: the drawing and the width it is
/// centred by cannot disagree.
fn control_text(control: OptionsControl, selected: bool) -> String {
    let marker = if selected { MARKER } else { NO_MARKER };
    format!("{marker}{}", control.label())
}

/// The column a control's drawn text starts at: **the label is centred and the marker
/// hangs into the margin left of it**, so the words sit on the same centre line as the
/// heading and the slider above them.
///
/// The entry list centres marker-and-label together, which is right there — it has
/// four rows of differing width and the block reads as a list. Here there are two
/// short labels and nothing else on the row, so including the marker in the measure
/// pushed both words off the screen's centre by half its width, and the dialog read as
/// leaning right against the rows above it.
fn control_column(width: u32) -> u32 {
    let widest = OptionsControl::ALL
        .iter()
        .map(|&c| c.label().chars().count() as u32)
        .max()
        .unwrap_or(0);
    centre(width, widest).saturating_sub(MARKER.chars().count() as u32)
}

fn draw_seed_prompt(grid: &mut Grid, heading: u32) {
    draw_centred(grid, heading, SEED_HEADING, Category::System);
    for (i, line) in SEED_LINES.iter().enumerate() {
        draw_centred(grid, heading + 2 + i as u32, line, Category::Neutral);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The v1 board's screen (§10.2): 40 wide, `TOP_ROWS + 40 + BOTTOM_ROWS` tall.
    const W: u32 = 40;
    const H: u32 = 43;

    fn text_of(grid: &Grid) -> String {
        grid.to_text().join("\n")
    }

    fn menu(selected: MenuEntry) -> MenuUi {
        MenuUi {
            selected,
            ..MenuUi::default()
        }
    }

    /// The seed prompt, at the default selection.
    fn seed_prompt() -> MenuUi {
        MenuUi {
            screen: MenuScreen::SeedPrompt,
            ..MenuUi::default()
        }
    }

    /// The level-options dialog at a slider position, *Play* marked as it opens.
    fn options(difficulty: Difficulty) -> MenuUi {
        MenuUi {
            screen: MenuScreen::LevelOptions,
            difficulty,
            ..MenuUi::default()
        }
    }

    /// The screen names the game and offers **every** entry — the two that play and
    /// the two that are only listed (#268).
    #[test]
    fn the_title_screen_names_the_game_and_lists_every_entry() {
        let text = text_of(&render_menu(W, H, MenuUi::default()));
        assert!(text.contains(TITLE), "the title is drawn:\n{text}");
        assert!(text.contains(TAGLINE));
        for entry in MenuEntry::ALL {
            assert!(
                text.contains(entry.label()),
                "{entry:?} is missing:\n{text}"
            );
        }
    }

    /// The selection marker rests on exactly one row — the selected entry's — and
    /// moving the selection moves the marker, never the labels (the marker's blank
    /// holds its column on every other row).
    #[test]
    fn the_marker_marks_the_selected_entry_and_only_it() {
        for selected in [MenuEntry::QuickPlay, MenuEntry::SeedPlay] {
            let grid = render_menu(W, H, menu(selected));
            let rows = grid.to_text();
            let marked: Vec<usize> = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.contains(MARKER.trim_end()))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(
                marked,
                vec![entry_row(
                    H,
                    MenuEntry::ALL.iter().position(|&e| e == selected).unwrap()
                ) as usize],
                "exactly the {selected:?} row is marked",
            );
            let index = MenuEntry::ALL.iter().position(|&e| e == selected).unwrap();
            assert!(rows[entry_row(H, index) as usize].contains(selected.label()));
        }
    }

    /// The marker and its blank are the same width, so the labels sit in one column
    /// whatever the selection is — a list that shifted sideways as you moved down it
    /// would read as the labels moving, not the marker.
    #[test]
    fn the_marker_and_its_blank_share_a_width() {
        assert_eq!(MARKER.chars().count(), NO_MARKER.chars().count());
        assert!(NO_MARKER.trim().is_empty());
        let column = |text: String| text.find(char::is_alphabetic);
        assert_eq!(
            column(entry_text(MenuEntry::QuickPlay, true)),
            column(entry_text(MenuEntry::QuickPlay, false)),
        );
    }

    /// §14's scaffolding warning, pinned: Options and Story mode are **visible but
    /// inert** — tagged *later* on screen and answering `false` to
    /// [`MenuEntry::enabled`], while the two that start a run answer `true`. If one
    /// of them ever does something, this test is the reminder that it became v2/v3
    /// work.
    #[test]
    fn the_unbuilt_entries_are_listed_but_do_nothing() {
        assert!(MenuEntry::QuickPlay.enabled());
        assert!(MenuEntry::SeedPlay.enabled());
        assert!(!MenuEntry::Options.enabled());
        assert!(!MenuEntry::StoryMode.enabled());

        let rows = render_menu(W, H, MenuUi::default()).to_text();
        for (i, entry) in MenuEntry::ALL.iter().enumerate() {
            let row = &rows[entry_row(H, i) as usize];
            assert_eq!(
                row.contains(LATER_TAG.trim()),
                !entry.enabled(),
                "{entry:?}'s row must be tagged later iff it is disabled: {row}",
            );
        }
    }

    /// Selection steps **over** the entries that do nothing and wraps at both ends,
    /// so the marker can only ever rest where Enter starts something.
    #[test]
    fn selection_skips_the_disabled_entries_and_wraps() {
        assert_eq!(MenuEntry::QuickPlay.next(), MenuEntry::SeedPlay);
        assert_eq!(
            MenuEntry::SeedPlay.next(),
            MenuEntry::QuickPlay,
            "next past the last enabled entry wraps to the first",
        );
        assert_eq!(MenuEntry::SeedPlay.prev(), MenuEntry::QuickPlay);
        assert_eq!(
            MenuEntry::QuickPlay.prev(),
            MenuEntry::SeedPlay,
            "prev past the first wraps to the last enabled entry",
        );
        // A disabled entry can never be reached from either direction.
        for entry in MenuEntry::ALL {
            assert!(entry.next().enabled(), "next from {entry:?} lands live");
            assert!(entry.prev().enabled(), "prev from {entry:?} lands live");
        }
    }

    /// A tap resolves to exactly the entry drawn on that row, at **any** column —
    /// the full-width target §11.6 wants on a phone — and the blank row between two
    /// entries resolves to nothing, so a low tap never activates the neighbour.
    #[test]
    fn a_tap_lands_on_the_entry_drawn_on_that_row() {
        let ui = MenuUi::default();
        for (i, &entry) in MenuEntry::ALL.iter().enumerate() {
            let row = entry_row(H, i);
            for x in [0, W / 2, W - 1] {
                assert_eq!(
                    menu_hit(W, H, ui, x, row),
                    Some(MenuHit::Entry(entry)),
                    "column {x} of {entry:?}'s row",
                );
            }
            assert_eq!(
                menu_hit(W, H, ui, W / 2, row + 1),
                None,
                "the gap under {entry:?} is not a target",
            );
        }
        assert_eq!(
            menu_hit(W, H, ui, W / 2, 0),
            None,
            "the title row is not a target"
        );
    }

    /// The title screen carries the theme control too (#189), **in the same corner of
    /// the same row** as the help panel's — so the one option the game has is in one
    /// place wherever you meet it, and a player who cannot comfortably read the
    /// current theme can change it before starting a run rather than after.
    ///
    /// Its whole run is the target, the word included, and the footer prose beside it
    /// is inert — the same two facts the panel's control is held to.
    #[test]
    fn the_title_screen_carries_the_theme_control_in_the_panel_s_corner() {
        let ui = MenuUi::default();
        let start = theme_control_start(W);
        for x in start..start + theme_control_len() {
            assert_eq!(
                menu_hit(W, H, ui, x, H - 1),
                Some(MenuHit::ToggleTheme),
                "footer cell {x}",
            );
        }
        assert_eq!(
            menu_hit(W, H, ui, start - 1, H - 1),
            None,
            "the footer prose is inert",
        );
        // It is drawn where it is tested, and the footer prose stops short of it —
        // `draw` clips in silence, and a half-drawn control cannot be seen to be one.
        let screen = text_of(&render_menu(W, H, ui));
        let footer = screen.lines().last().expect("a footer row").to_string();
        assert!(footer.contains(&theme_control()), "footer: {footer:?}");
        let prose_end = FOOTER_INDENT + MENU_FOOTER.chars().count() as u32;
        assert!(
            prose_end < start,
            "the menu footer runs into the theme control ({prose_end} vs {start})",
        );
    }

    /// **Not on the seed prompt.** The DOM text box floats over the middle of that
    /// screen and `n` is an ordinary letter of a level-seed token, so a control there
    /// would be half hidden and its key a trap mid-token — the prompt keeps its own
    /// *back* button as the way out (§11.6's no-trap rule) and nothing else.
    #[test]
    fn the_seed_prompt_carries_no_theme_control() {
        let ui = seed_prompt();
        let start = theme_control_start(W);
        for x in start..start + theme_control_len() {
            assert_eq!(menu_hit(W, H, ui, x, H - 1), None, "footer cell {x}");
        }
        let screen = text_of(&render_menu(W, H, ui));
        assert!(
            !screen.contains(&theme_control()),
            "the seed prompt drew a theme control",
        );
    }

    /// The token the prompt shows as an example is **a real one** — it decodes. A
    /// sample that had drifted out of the format would be worse than no sample: it is
    /// the one token a new player is certain to try, and the shape they will measure
    /// their own paste against. This fails the moment the format moves, which is the
    /// prompt asking to be rewritten (#333).
    #[test]
    fn the_example_token_in_the_prompt_actually_decodes() {
        let example = SEED_LINES[1]
            .rsplit(' ')
            .next()
            .expect("the line ends in the example");
        assert_eq!(example.len(), crate::level_seed::TOKEN_LEN);
        assert!(
            crate::LevelSeed::decode(example).is_some(),
            "the prompt's example token no longer decodes: {example}",
        );
    }

    /// The seed prompt (§13.1/#110/#245/#333) says what to type, shows the shape of a
    /// token so a truncated paste is obvious, and — critically — leaves the **middle
    /// band blank** for the DOM text box that floats there. A glyph drawn into that
    /// band would sit under the box.
    ///
    /// Each row is matched **whole**, not by `contains`: the first cut of this screen
    /// centred its title exactly where the heading went, and the two drew over each
    /// other into `I N SEED PLAY O N` — which a substring check reads as both lines
    /// present and correct.
    #[test]
    fn the_seed_prompt_instructs_and_keeps_the_middle_clear() {
        let ui = seed_prompt();
        let rows = render_menu(W, H, ui).to_text();
        let (title, tagline, heading) = seed_rows(H);
        let row = |y: u32| rows[y as usize].trim().to_string();

        assert_eq!(row(title), TITLE);
        assert_eq!(row(tagline), TAGLINE);
        assert_eq!(row(heading), SEED_HEADING);
        for (i, line) in SEED_LINES.iter().enumerate() {
            assert_eq!(row(heading + 2 + i as u32), *line);
        }
        // Nothing of the entry list survives into the prompt.
        let text = rows.join("\n");
        for entry in MenuEntry::ALL {
            assert!(
                !text.contains(entry.label()),
                "{entry:?} still shows on the seed prompt:\n{text}",
            );
        }
        let middle = H / 2;
        for y in middle - 3..=middle + 3 {
            assert!(
                row(y).is_empty(),
                "row {y} must stay blank for the DOM seed box, found: {:?}",
                row(y),
            );
        }
    }

    /// The list screen's own rows, matched whole for the same reason — the title and
    /// tagline must not collide with each other or with the first entry at the fit
    /// the game actually ships (§10.2's 40×40 board).
    #[test]
    fn the_list_screen_draws_each_row_whole() {
        let (title, tagline, first) = rows(H);
        let drawn = render_menu(W, H, MenuUi::default()).to_text();
        assert_eq!(drawn[title as usize].trim(), TITLE);
        assert_eq!(drawn[tagline as usize].trim(), TAGLINE);
        assert_eq!(
            drawn[first as usize].trim(),
            entry_text(MenuEntry::QuickPlay, true).trim(),
        );
        assert!(first > tagline, "the entries sit below the title block");
    }

    /// §11.6's no-trap rule, on the screen itself: both footers name the way on —
    /// the list says how to choose (by key *and* by tap), the prompt says how to get
    /// back. A player who reaches either screen can always read their way out of it.
    #[test]
    fn every_screen_spells_out_the_way_on() {
        let list = text_of(&render_menu(W, H, MenuUi::default()));
        assert!(list.contains(MENU_FOOTER), "{list}");
        let prompt = text_of(&render_menu(W, H, seed_prompt()));
        assert!(prompt.contains(SEED_FOOTER), "{prompt}");
    }

    /// The level-options dialog's rows, matched **whole** (#298) — the same discipline
    /// the seed prompt's test keeps, and for the same reason: the first cut of that
    /// screen drew its title and heading over each other into `I N SEED PLAY O N`,
    /// which a `contains` check reads as both lines present and correct. This screen
    /// stacks seven drawn rows in one block, so it has more ways to collide, not fewer.
    #[test]
    fn the_level_options_dialog_draws_each_row_whole() {
        let ui = options(Difficulty::Standard);
        let rows = render_menu(W, H, ui).to_text();
        let title = options_title_row(H);
        let row = |y: u32| rows[y as usize].trim().to_string();

        assert_eq!(row(title), TITLE);
        assert_eq!(row(title + OPTIONS_TAGLINE), TAGLINE);
        assert_eq!(row(title + OPTIONS_HEADING_ROW), OPTIONS_HEADING);
        assert_eq!(row(title + OPTIONS_CAPTION_ROW), DIFFICULTY_CAPTION);
        assert_eq!(row(title + OPTIONS_NAME_ROW), Difficulty::Standard.label());
        assert_eq!(row(title + OPTIONS_BLURB_ROW), Difficulty::Standard.blurb());
        for (i, control) in OptionsControl::ALL.iter().enumerate() {
            assert_eq!(
                row(options_control_row(H, i)),
                control_text(*control, *control == ui.options_control).trim(),
            );
        }
        // Every drawn row is a distinct one — a block whose rows landed on each other
        // would still pass each equality above if the collision were a prefix.
        let drawn = [
            title,
            title + OPTIONS_TAGLINE,
            title + OPTIONS_HEADING_ROW,
            title + OPTIONS_CAPTION_ROW,
            title + OPTIONS_TRACK_ROW,
            title + OPTIONS_NAME_ROW,
            title + OPTIONS_BLURB_ROW,
            options_control_row(H, 0),
            options_control_row(H, 1),
        ];
        let distinct: std::collections::BTreeSet<u32> = drawn.into_iter().collect();
        assert_eq!(
            distinct.len(),
            drawn.len(),
            "two rows share a line: {drawn:?}"
        );
        assert!(
            *distinct.last().expect("rows") < H - 1,
            "the block runs into the footer row",
        );
        // Nothing of the entry list survives onto the dialog.
        let text = rows.join("\n");
        for entry in MenuEntry::ALL {
            assert!(!text.contains(entry.label()), "{entry:?} shows:\n{text}");
        }
    }

    /// The slider shows **which** of its five stops it rests on, and says what that
    /// position is called and what it will do — the three rows that make it a control
    /// rather than a row of dots. Moving it moves exactly one filled stop, and never
    /// reflows the track (the [`MARKER`]/[`NO_MARKER`] rule, one row over).
    #[test]
    fn the_slider_marks_one_stop_and_names_what_it_will_do() {
        for (index, &position) in Difficulty::ALL.iter().enumerate() {
            let rows = render_menu(W, H, options(position)).to_text();
            let title = options_title_row(H);
            let track = &rows[(title + OPTIONS_TRACK_ROW) as usize];
            assert_eq!(
                track.matches(STOP_HERE).count(),
                1,
                "exactly one stop is filled: {track}",
            );
            assert_eq!(
                track.matches(STOP).count(),
                Difficulty::ALL.len() - 1,
                "every other stop is drawn open: {track}",
            );
            // The filled stop is the one at the position's own column.
            let filled = track.chars().position(|c| c.to_string() == STOP_HERE);
            assert_eq!(filled, Some(stop_column(W, index) as usize), "{position:?}");
            // The stops sit on a **rail**: every cell between two of them carries the
            // fill, so the row reads as one control rather than as five loose dots.
            let cells: Vec<char> = track.chars().collect();
            for column in stop_column(W, 0)..stop_column(W, Difficulty::ALL.len() - 1) {
                let on_stop = Difficulty::ALL
                    .iter()
                    .enumerate()
                    .any(|(i, _)| stop_column(W, i) == column);
                if !on_stop {
                    assert_eq!(
                        cells[column as usize].to_string(),
                        TRACK_FILL,
                        "column {column} of the rail is bare",
                    );
                }
            }
            // …and the two rows under it name the position and its effect.
            assert_eq!(
                rows[(title + OPTIONS_NAME_ROW) as usize].trim(),
                position.label(),
            );
            assert_eq!(
                rows[(title + OPTIONS_BLURB_ROW) as usize].trim(),
                position.blurb(),
            );
        }
    }

    /// **Fully driveable by touch** (§11.6/#298): every control on the dialog is a
    /// generous tap target. The two control rows are full-width like the entry list's,
    /// and each slider stop owns a band of cells rather than the single cell its dot
    /// is drawn on — a one-cell target is not a target a finger can hit.
    #[test]
    fn every_control_on_the_dialog_is_a_generous_tap_target() {
        let ui = options(Difficulty::Standard);
        let track = options_title_row(H) + OPTIONS_TRACK_ROW;

        // Each stop owns a band [`STOP_SPACING`] cells wide, centred on the dot it is
        // drawn as — the dot's own column plus the cells either side of it, so a
        // finger aimed at a stop hits it without having to find one cell.
        let half = STOP_SPACING / 2;
        for (index, &position) in Difficulty::ALL.iter().enumerate() {
            let column = stop_column(W, index);
            for x in column - half..column + half {
                assert_eq!(
                    menu_hit(W, H, ui, x, track),
                    Some(MenuHit::Difficulty(position)),
                    "column {x} of {position:?}'s band",
                );
            }
        }
        // The bands tile the track without a gap: every column from the first stop's
        // left edge to the last one's right edge sets *something*.
        for x in stop_column(W, 0) - half..=stop_column(W, Difficulty::ALL.len() - 1) + half {
            assert!(
                matches!(menu_hit(W, H, ui, x, track), Some(MenuHit::Difficulty(_))),
                "column {x} of the track sets nothing",
            );
        }
        // Past either end of the track there is nothing to set.
        let first = stop_column(W, 0) - STOP_SPACING / 2;
        assert_eq!(
            menu_hit(W, H, ui, first - 1, track),
            None,
            "left of the track"
        );
        let last = stop_column(W, Difficulty::ALL.len() - 1) + STOP_SPACING / 2;
        assert_eq!(menu_hit(W, H, ui, last + 1, track), None, "right of it");

        // The control rows are the whole row, with the blank between them inert.
        for (i, &control) in OptionsControl::ALL.iter().enumerate() {
            let row = options_control_row(H, i);
            for x in [0, W / 2, W - 1] {
                assert_eq!(
                    menu_hit(W, H, ui, x, row),
                    Some(MenuHit::OptionsControl(control)),
                    "column {x} of {control:?}'s row",
                );
            }
            assert_eq!(
                menu_hit(W, H, ui, W / 2, row + 1),
                None,
                "the gap under {control:?} is not a target",
            );
        }
        // The theme control is still in its corner — the dialog is glyphs all the way
        // down, so unlike the seed prompt nothing floats over it (#189).
        let theme = theme_control_start(W);
        assert_eq!(menu_hit(W, H, ui, theme, H - 1), Some(MenuHit::ToggleTheme),);
    }

    /// §11.6's no-trap rule on the dialog — **the exact failure the old options dialog
    /// shipped**, which is why this is pinned rather than left to review. The footer
    /// names the way on and the way back, a *Back* control is drawn and tappable, and
    /// the prose stops short of the theme control it shares its row with.
    #[test]
    fn the_dialog_spells_out_the_way_on_and_the_way_back() {
        let screen = text_of(&render_menu(W, H, options(Difficulty::Standard)));
        assert!(screen.contains(OPTIONS_FOOTER), "{screen}");
        assert!(screen.contains(OptionsControl::Back.label()), "{screen}");
        assert!(screen.contains(OptionsControl::Play.label()), "{screen}");
        let prose_end = FOOTER_INDENT + OPTIONS_FOOTER.chars().count() as u32;
        assert!(
            prose_end < theme_control_start(W),
            "the options footer runs into the theme control ({prose_end} vs {})",
            theme_control_start(W),
        );
    }

    /// The dialog sits on **one centre line**: the heading, the slider's rail, the
    /// position's name and both control labels all centre on the same column. The
    /// controls get there by centring the *label* and hanging the marker into the
    /// margin — measuring the marker in pushed both words half its width to the right,
    /// and against the rows above it the block read as leaning.
    #[test]
    fn the_dialog_composes_on_one_centre_line() {
        // Twice the midpoint, so a run of even length and one of odd length are both
        // exact — on a character grid they can never share a *cell*, only a line, and
        // the most either may be off it is the half cell that parity costs.
        let midpoint = |start: u32, len: u32| 2 * start + len;
        let heading = midpoint(
            centre(W, OPTIONS_HEADING.chars().count() as u32),
            OPTIONS_HEADING.chars().count() as u32,
        );
        let rail = midpoint(stop_column(W, 0), TRACK_WIDTH);
        assert_eq!(
            rail, heading,
            "the slider's rail is off the heading's centre"
        );
        for control in OptionsControl::ALL {
            let label = control.label().chars().count() as u32;
            let start = control_column(W) + MARKER.chars().count() as u32;
            assert!(
                midpoint(start, label).abs_diff(heading) <= 1,
                "{control:?}'s label is off the centre line by more than parity",
            );
        }
        // The **marker hangs into the margin** — measuring it into the centring is the
        // bug this test exists for, and it would show as both labels sitting a whole
        // cell right of every row above them.
        assert_eq!(
            control_column(W) + MARKER.chars().count() as u32,
            centre(W, OptionsControl::Play.label().chars().count() as u32),
            "the label is not centred in its own right",
        );
        // The two labels still share a column, so the marker moves and they do not.
        let rows = render_menu(W, H, options(Difficulty::Standard)).to_text();
        let label_at =
            |i: usize| rows[options_control_row(H, i) as usize].find(char::is_alphabetic);
        assert_eq!(label_at(0), label_at(1), "the labels sit in one column");
    }

    /// The marker rests on exactly one control, and *Play* is where it opens — the
    /// fast path from a load stays Enter, Enter (#298).
    #[test]
    fn the_dialog_opens_on_play_with_the_baseline_difficulty() {
        let fresh = MenuUi {
            screen: MenuScreen::LevelOptions,
            ..MenuUi::default()
        };
        assert_eq!(fresh.options_control, OptionsControl::Play);
        assert_eq!(fresh.difficulty, Difficulty::Standard);
        assert_eq!(OptionsControl::Play.other(), OptionsControl::Back);
        assert_eq!(OptionsControl::Back.other(), OptionsControl::Play);

        for marked in OptionsControl::ALL {
            let ui = MenuUi {
                options_control: marked,
                ..fresh
            };
            let rows = render_menu(W, H, ui).to_text();
            let marked_rows: Vec<usize> = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.contains(MARKER.trim_end()))
                .map(|(i, _)| i)
                .collect();
            let index = OptionsControl::ALL
                .iter()
                .position(|&c| c == marked)
                .expect("a control");
            assert_eq!(marked_rows, vec![options_control_row(H, index) as usize]);
        }
    }

    /// Bounds are clamped, never asserted: a board far too small for the block still
    /// renders something rather than panicking (the help card's rule).
    #[test]
    fn a_tiny_screen_clamps_instead_of_panicking() {
        for (w, h) in [(1, 1), (8, 4), (12, 7)] {
            let grid = render_menu(w, h, MenuUi::default());
            assert_eq!((grid.width(), grid.height()), (w, h));
            let seeded = render_menu(w, h, seed_prompt());
            assert_eq!((seeded.width(), seeded.height()), (w, h));
            // The dialog's track is wider than any of these boards, so its stops all
            // clamp to the left edge rather than drawing off it.
            for position in Difficulty::ALL {
                let dialog = render_menu(w, h, options(position));
                assert_eq!((dialog.width(), dialog.height()), (w, h));
                // …and a tap anywhere on it answers without panicking.
                for x in 0..w {
                    for y in 0..h {
                        let _ = menu_hit(w, h, options(position), x, y);
                    }
                }
            }
        }
    }
}
