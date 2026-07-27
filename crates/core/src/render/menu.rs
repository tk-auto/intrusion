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

use super::{blank_grid, draw, Grid};
use crate::category::Category;

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

/// The menu's **view state**, owned by the shell exactly like
/// [`ScreenUi`](super::ScreenUi) — it changes no world and costs no turn (§12.1).
/// A shell keeps `Some(MenuUi)` on [`ScreenUi::menu`](super::ScreenUi::menu) while
/// the menu is up and clears it the moment a run starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MenuUi {
    /// Which entry the selection marker rests on. The [`Default`] is
    /// [`MenuEntry::QuickPlay`], so a load is one Enter (or one tap) from playing.
    pub selected: MenuEntry,
    /// Whether the **seed prompt** is showing instead of the entry list — the
    /// sub-screen [`MenuEntry::SeedPlay`] opens, where the DOM text box takes a
    /// level-seed token. Escape (or the box's own *back* button) clears it.
    pub seed_entry: bool,
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
const MENU_FOOTER: &str = "↑↓ choose · Enter or tap plays";

/// The footer of the seed prompt — the way back out, spelled out beside the box's
/// own *back* button, so the sub-screen is never a dead end (§11.6's no-trap rule).
const SEED_FOOTER: &str = "Esc or [back] returns to the menu";

/// The heading and the two instruction lines of the seed prompt
/// (§13.1/#110/#245/#333). The second line says what a token *looks* like, so a
/// player who has one in hand can tell at a glance whether they have the whole thing
/// — it is a fixed twelve letters, and a truncated paste is the likely mistake. It
/// replaces the bare-seed promise that used to stand here: a number named a preset
/// rather than a run, and no longer decodes at all (#333).
const SEED_HEADING: &str = "SEED PLAY";
const SEED_LINES: [&str; 2] = [
    "type or paste a level-seed token",
    "twelve letters, like bcwdrhliqsmm",
];

/// The tag drawn after an entry that is not built yet (§14 v2/v3) — short, so the
/// row reads as an entry with a note rather than a sentence.
const LATER_TAG: &str = " — later";

/// The marker on the selected row, and the blank that holds its place on every
/// other row so the labels never shift as the selection moves (the two are the
/// same width — asserted below).
const MARKER: &str = "> ";
const NO_MARKER: &str = "  ";

/// Rows between one entry and the next: one drawn row, one blank. The gap is what
/// makes a full-width row a comfortable tap target — a mis-aimed tap lands on the
/// blank between entries and does nothing, never on the neighbour (§11.6).
const ENTRY_SPACING: u32 = 2;

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
fn centre(width: u32, len: u32) -> u32 {
    width.saturating_sub(len) / 2
}

/// Draw `text` centred on row `y`.
fn draw_centred(grid: &mut Grid, y: u32, text: &str, category: Category) {
    let len = text.chars().count() as u32;
    draw(grid, centre(grid.width(), len), y, text, category);
}

/// What a press on the menu lands on (§11.6) — the touch counterpart of
/// [`menu_nav_for_key`](crate::menu_nav_for_key). A shell maps the tap to a screen
/// cell and asks this; `None` means the press hit nothing and is swallowed.
///
/// **The whole row is the target**, at any column: nothing else is drawn on an
/// entry's row, and a generous target is the difference between a menu that works
/// on a phone and one that does not. The blank row between entries
/// ([`ENTRY_SPACING`]) is the buffer that keeps a low tap off the next entry.
///
/// The seed prompt answers `None` everywhere — its controls are the DOM box's own
/// *play* and *back* buttons, which handle their taps before the board sees them.
#[must_use]
pub fn menu_hit(height: u32, ui: MenuUi, y: u32) -> Option<MenuEntry> {
    if ui.seed_entry {
        return None;
    }
    MenuEntry::ALL
        .iter()
        .enumerate()
        .find(|&(i, _)| entry_row(height, i) == y)
        .map(|(_, &entry)| entry)
}

/// Render the title screen (§11.4/§14, #268) — the whole `width × height` screen,
/// not an overlay, so the shell paints it through the one path it paints a frame
/// with and nothing of the game shows behind it.
///
/// Two screens, one for each state of [`MenuUi::seed_entry`]:
///
/// - the **entry list** — the title block centred, the four entries with the
///   selection marker, and the footer that names both ways to choose;
/// - the **seed prompt** — the same title, moved up the screen, over the
///   instructions for a level-seed token, with **the middle band left deliberately
///   blank**. That band is where the shell's DOM text box floats (a canvas cannot
///   raise a phone's keyboard, so the box has to be real markup); leaving the space
///   empty rather than aligning glyphs to it means the two never fight over a row,
///   at any fit.
///
/// Bounds are clamped, never asserted (like the help card): on a board too small
/// for a row, that row shows what fits and stops.
pub(super) fn render_menu(width: u32, height: u32, ui: MenuUi) -> Grid {
    let mut grid = blank_grid(width, height);
    let (title_row, tagline_row, heading_row) = if ui.seed_entry {
        seed_rows(height)
    } else {
        rows(height)
    };

    draw_centred(&mut grid, title_row, TITLE, Category::Interest);
    draw_centred(&mut grid, tagline_row, TAGLINE, Category::Ground);

    if ui.seed_entry {
        draw_seed_prompt(&mut grid, heading_row);
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

    let footer = if ui.seed_entry {
        SEED_FOOTER
    } else {
        MENU_FOOTER
    };
    draw_centred(
        &mut grid,
        height.saturating_sub(1),
        footer,
        Category::Ground,
    );
    grid
}

/// Draw the seed prompt's heading and instructions from `heading` down, high enough
/// on the screen that the band around the **middle** stays clear for the DOM text box
/// that floats there (see [`render_menu`]). The box is centred in the viewport and the
/// canvas is centred in the viewport too, so the middle of the grid is where it lands
/// at every fit — the one row-level coupling between the two, kept as slack rather
/// than arithmetic, and asserted below.
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
            seed_entry: false,
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
                let _ = x; // the row is the whole target; the column is not read
                assert_eq!(menu_hit(H, ui, row), Some(entry));
            }
            assert_eq!(
                menu_hit(H, ui, row + 1),
                None,
                "the gap under {entry:?} is not a target",
            );
        }
        assert_eq!(menu_hit(H, ui, 0), None, "the title row is not a target");
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
        let ui = MenuUi {
            seed_entry: true,
            ..MenuUi::default()
        };
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
        let prompt = text_of(&render_menu(
            W,
            H,
            MenuUi {
                seed_entry: true,
                ..MenuUi::default()
            },
        ));
        assert!(prompt.contains(SEED_FOOTER), "{prompt}");
    }

    /// Bounds are clamped, never asserted: a board far too small for the block still
    /// renders something rather than panicking (the help card's rule).
    #[test]
    fn a_tiny_screen_clamps_instead_of_panicking() {
        for (w, h) in [(1, 1), (8, 4), (12, 7)] {
            let grid = render_menu(w, h, MenuUi::default());
            assert_eq!((grid.width(), grid.height()), (w, h));
            let seeded = render_menu(
                w,
                h,
                MenuUi {
                    seed_entry: true,
                    ..MenuUi::default()
                },
            );
            assert_eq!((seeded.width(), seeded.height()), (w, h));
        }
    }
}
