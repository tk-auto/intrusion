//! The help panel: a full-screen, input-capturing reference card (§14 v2,
//! #139/#248).
//!
//! The old game never had a legend — "nothing ever explained what `$`, `E`, `}`
//! or `z` meant" (§14 v2) — so this is the reference the player can call up on
//! demand. It began as a single stacked page overlaid on the map; #248 splits it
//! into **tabs** so a fourth kind of content (the run's active level modifiers)
//! has somewhere to live without the page overflowing the board:
//!
//! - **Level info** ([`HelpTab::LevelInfo`]) — what's bending the rules *this run*:
//!   the run's own **level-seed token**, with a `copy [c]` control that puts it on the
//!   clipboard (§13.1/#353), the active [`LevelModifiers`] by name and direction
//!   (§12.6), and the **facility alert** — the rung reached and the retaliation it has
//!   in force (§7.3/#375, drawn by [`super::alert`]). The first two are fixed at boot;
//!   the third is the one thing on the card that moves while you play, which is why the
//!   near line alone could not carry it (§11.7: it is overwritten by anything louder).
//! - **Abilities** ([`HelpTab::Abilities`]) — what each of the run's abilities
//!   actually *does*, and what it costs (§8.2/§8.3; #343, and see [`abilities`]).
//! - **Help** ([`HelpTab::Help`]) — the glyph legend, the colour key, and the
//!   **standing** controls, the original reference card (#139/#296).
//!
//! **Settings are not a tab** (§14 v2/#513). They were going to be — and the debug
//! session's switches were a fourth tab, [`HelpTab::Debug`], from #459 until #513 —
//! but the bar is one row of a 40-cell board (§10.2) and a fourth `[Label]` clear of
//! the `[x]` is all it holds. The options screen ([`super::settings`]) is a screen of
//! its own instead, and the panel keeps a **door** to it: the footer's `options [o]`
//! control, which is also how the debug switches stay reachable mid-run now that
//! their tab is gone.
//!
//! **Every row derives from the real source**, never a hand-copied table that
//! could drift from the game it documents (§11.2/§11.3/§11.6): terrain glyphs and
//! their categories come from [`Terrain::glyph`]/[`Terrain::category`], the entity
//! glyphs from the [`super`] render constants the world draws with, the colour
//! meanings from an exhaustive match over [`Category`], the ability entries from
//! the run's own [`Loadout`] and the §11.6 keys its bar slots answer to, and the
//! modifier rows from [`LevelModifiers::active`] — so a newly added modifier
//! appears here on its own. The tests assert each derivation.
//!
//! **What varies with the run, and what does not.** The Level info and Abilities
//! tabs are drawn *per run*; **Help** is the same card for every run. That split is
//! why the ability rows left its controls block (#296): it listed all eight of the
//! catalogue when a run holds at most four (§8.3), so half its rows named a key that
//! did nothing this run.
//!
//! It was called *Legend* until the abilities left it. The name fitted a card that
//! was only the glyph key, but the tab now answers "how do I play this?" — glyphs,
//! colours and the standing keys — while the glyph *legend* is one section inside
//! it. The tab is what it is for, not what its first section is.
//!
//! Opening and closing the panel is a pure **view** action owned by the shell
//! ([`ScreenUi::help_open`](super::ScreenUi)): it changes no world and costs no
//! turn (§4.4), so no guard moves while it is up. Unlike the old map-only overlay,
//! the panel is **modal and full-screen** (#248): while it is up it takes the whole
//! screen and the shell routes input to it — keys through
//! [`help_nav_for_key`](crate::help_nav_for_key), taps through [`help_hit`] — so
//! the game never steps underneath. It stays escapable (§11.6's no-trap rule): `?`
//! or `Escape` closes it, and the tab bar carries a touchable `[x]`.

mod abilities;

use super::{
    blank_grid, draw, Grid, ScreenUi, BODY_GLYPH, FLOOR_DOT, GUARD_GLYPH, PLAYER_GLYPH,
    SCHEMATIC_WALL,
};
use crate::ability::AbilityId;
use crate::alert::AlertReadout;
use crate::category::Category;
use crate::facility::Terrain;
use crate::level_seed::LevelSeed;
use crate::modifiers::{LevelModifiers, ModifierDirection, CAPTIONS, CAPTION_SEPARATOR};
use crate::place::LevelConfig;

/// The key that toggles the help panel (§11.6). A free letter — not a movement
/// key, an ability key, or another UI control — and the conventional roguelike
/// help key. Shown in the controls list and matched in
/// [`ui_command_for_key`](crate::input::ui_command_for_key) (to open) and
/// [`help_nav_for_key`](crate::help_nav_for_key) (to close).
pub(crate) const HELP_KEY: char = '?';

/// The key that flips the colour theme (§11.2/#189) — `n`, for *night* mode. A
/// **standing** shortcut: it is listed in this card's controls block, and matched in
/// [`ui_command_for_key`](crate::input::ui_command_for_key) (on the board) and in
/// every modal screen's own table, the panel's included. The tests below pin the
/// character against those tables, so the card and the binding cannot drift apart.
///
/// Its home is the options screen ([`super::settings`], #513) — the row that draws the
/// live value and persists it. The key stays because a one-press legibility toggle is
/// worth having wherever you are when the screen is hard to read; what it no longer
/// has here is a *drawn control*, which moved with the setting.
pub(crate) const THEME_KEY: char = 'n';

/// The key that opens the **options screen** from the panel (§14 v2/#513) — `o`, for
/// *options*. Drawn as `options [o]` on the footer and matched in
/// [`help_nav_for_key`](crate::help_nav_for_key).
///
/// **Panel-only**, like [`COPY_KEY`]: it is absent from
/// [`ui_command_for_key`](crate::input::ui_command_for_key), so it claims no board
/// letter and shadows no ability mnemonic (§11.6/#368) — the open panel swallows the
/// mnemonics anyway. The board's own door to the screen is this panel, one `?` away.
pub(crate) const SETTINGS_KEY: char = 'o';

/// The tabs the help panel pages between (§14 v2/#248). The panel opens on the
/// leftmost ([`Default`]) and the tab bar switches between them; a shell keeps the
/// current tab on [`ScreenUi`](super::ScreenUi) and hands it to [`render_help`].
///
/// Ordered as the player reads them left to right — *this run* first, then the
/// standing reference — and cycled by [`next`](Self::next)/[`prev`](Self::prev) so the
/// tab bar wraps at either end.
///
/// **Every tab is always there.** It took a `shown` list while the debug session had a
/// fourth tab of its own (#459); the switches live on the options screen since #513, so
/// the bar is the same three tabs in every session and the layout, the cycle and the
/// hit-test are all written over [`ALL`](Self::ALL) again.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HelpTab {
    /// This run's active level modifiers (§12.6/#248) — what is bending the rules.
    #[default]
    LevelInfo,
    /// What each of the run's held abilities does, and what it costs (§8.2/#343).
    Abilities,
    /// The glyph legend, colour key, and standing controls (#139) — the reference
    /// card, the same one for every run (#296).
    Help,
}

impl HelpTab {
    /// Every tab this panel knows, in reading (left-to-right) order. A new tab is one
    /// entry here.
    ///
    /// Ordered outward from *this run*: the run's rules, then the run's abilities,
    /// then the standing reference that never changes.
    pub const ALL: [HelpTab; 3] = [HelpTab::LevelInfo, HelpTab::Abilities, HelpTab::Help];

    /// The label shown on the tab bar and used to size its hit region. `const` so the
    /// bar's width can be checked at compile time ([`TAB_BAR_FITS`]).
    const fn label(self) -> &'static str {
        match self {
            // *Level*, not *Level info*: the bar is one row of a 40-cell board and has
            // to hold its `[Label]`s clear of the `[x]` (§10.2/#459). The tab's own
            // heading says `THIS RUN`, so the extra word on the bar was the most
            // expendable five cells on the panel.
            HelpTab::LevelInfo => "Level",
            HelpTab::Abilities => "Abilities",
            HelpTab::Help => "Help",
        }
    }

    /// The next tab, wrapping past the last back to the first — the panel's
    /// "advance" motion (Tab / rightward keys, §14 v2/#248).
    #[must_use]
    pub fn next(self) -> Self {
        self.step(1)
    }

    /// The previous tab, wrapping past the first back to the last.
    #[must_use]
    pub fn prev(self) -> Self {
        self.step(Self::ALL.len() - 1)
    }

    /// Walk `by` places along the bar, wrapping.
    fn step(self, by: usize) -> Self {
        let i = Self::ALL.iter().position(|&t| t == self).unwrap_or(0);
        Self::ALL[(i + by) % Self::ALL.len()]
    }
}

/// What a pointer press inside the open help panel lands on (§11.6/#248) — the
/// touch counterpart of [`help_nav_for_key`](crate::help_nav_for_key). A shell
/// maps a tap to a screen cell, asks [`help_hit`], and applies the result; a press
/// anywhere else is swallowed by the modal panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HelpHit {
    /// The `[x]` close control — dismiss the panel (the always-reachable escape).
    Close,
    /// A tab in the tab bar — switch the panel to it.
    Tab(HelpTab),
    /// The footer's `options [o]` control — open the options screen (§14 v2/#513),
    /// the touch half of its key. It is the panel's door to the settings that used to
    /// camp on it: the colour theme, whose `theme [n]` button stood in this very
    /// corner (#189), and the debug session's switches, which had a tab of their own
    /// (#459). Without it the screen would be keyboard-only mid-run, on a game that is
    /// played on phones (§11.6: every control reachable by key *and* touch).
    OpenSettings,
    /// The Level info tab's `copy [c]` control — put this run's level-seed token on
    /// the system clipboard (§13.1/#353). The **core neither performs nor knows
    /// about** the write: it owns the geometry and the token, and the shell owns the
    /// clipboard (§12.1). Only ever produced when a token is actually drawn, so a
    /// hand-built state offers no control rather than one that copies nothing.
    CopySeed,
}

/// Whether the player's last attempt to copy from the panel reached the clipboard
/// (§13.1/#353) — the acknowledgement the panel prints under the control, and its
/// answer to "did that work?". One line for both copy controls: the Level info tab's
/// `copy [c]` under its token, and the Debug tab's `replay [r]` under its own row
/// (#411/#459) — "copied to clipboard" is the honest answer either way, and a tab
/// switch drops it, so the line always belongs to the tab it is drawn on.
///
/// It lives on [`ScreenUi`](super::ScreenUi), **not** on [`State`](crate::State): the
/// panel writes no state ([`render_help`]'s standing promise), the copy costs no turn
/// (§4.4), and whether a browser has a clipboard is a fact about the shell rather than
/// about the run. The core only says which of the three things to print.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SeedCopy {
    /// Nothing has been copied since the panel opened: the control offers itself and
    /// the row under the token stays the blank spacer it always was.
    #[default]
    Idle,
    /// The token reached the system clipboard.
    Copied,
    /// The browser had no clipboard to write to, or refused the write — an insecure
    /// context, or a frame without clipboard permission. **Not silent, and not a
    /// claim**: the token is still printed one row above, ready to be read off by
    /// eye as it was before this control existed.
    Unavailable,
}

impl SeedCopy {
    /// What to print under the token, and in which category — `None` while nothing
    /// has been attempted. Success takes [`Category::System`], the HUD-control colour
    /// the `[x]` and the control itself already use, because the line belongs to the
    /// control that produced it; a failure takes Warning, the standing "this is not
    /// what you wanted" cue (§11.2), rather than any new ad-hoc styling.
    pub(super) fn acknowledgement(self) -> Option<(&'static str, Category)> {
        match self {
            SeedCopy::Idle => None,
            SeedCopy::Copied => Some((COPIED_ACK, Category::System)),
            SeedCopy::Unavailable => Some((UNAVAILABLE_ACK, Category::Warning)),
        }
    }
}

/// The two acknowledgements, worded so neither can be misread as the other: the
/// failure says what did *not* happen and never names the clipboard as holding
/// anything.
const COPIED_ACK: &str = "copied to clipboard";
const UNAVAILABLE_ACK: &str = "clipboard unavailable";

// They share the Level info tab's content column, and [`draw`] clips in silence — so
// they are bounded at **compile time** like every other fixed column of the panel
// (§2.3), rather than being discovered half-drawn on a player's screen.
const _: () = {
    assert!(
        COPIED_ACK.len() <= column_width(CONTENT_INDENT)
            && UNAVAILABLE_ACK.len() <= column_width(CONTENT_INDENT),
        "a seed-copy acknowledgement is too long for the Level info tab — shorten it \
         (see column_width in render::help)",
    );
};

/// The `[x]` close control on the tab bar — three cells wide, like the header's
/// other `[?]`/`[▾]` buttons, so the escape reads as a button. Shared with the
/// options screen ([`super::settings`]), which puts the same escape in the same
/// corner of its own first row.
pub(super) const CLOSE_BUTTON: &str = "[x]";
pub(super) const CLOSE_BUTTON_LEN: u32 = 3;

/// The **options control** on the panel's footer row (§14 v2/#513): the word it does,
/// followed by the key it answers to — `options [o]`.
///
/// **The label is part of the control, not prose beside it.** A bare `[o]` is only a
/// target if you already know what `o` means, and the word is the larger and more
/// obvious thing to reach for — so the whole run is drawn in the button colour and the
/// whole run hit-tests ([`help_hit`]). The bracketed key still teaches the shortcut,
/// the way `[?]` and `[x]` do.
///
/// It stands where the `theme [n]` control used to (#189). That control was the
/// theme's temporary home "until v2 grows an options screen" (§11.2), and this is the
/// door to that screen: from here the theme is one press further away and the renderer
/// is reachable at all. It is also the panel's answer to *how do the debug switches
/// stay flippable mid-run* once their tab is gone (#459/#513) — the help panel is the
/// one modal surface a running game can always raise, so it is where the way in sits.
pub(super) const OPTIONS_LABEL: &str = "options";

pub(super) fn options_control() -> String {
    format!("{OPTIONS_LABEL} [{SETTINGS_KEY}]")
}

pub(super) fn options_control_len() -> u32 {
    options_control().chars().count() as u32
}

/// The key that copies the run's **level-seed token** to the clipboard while the
/// panel is open (§13.1/#353) — `c`, for *copy*. It is drawn as `copy [c]` beside the
/// token, the same label-and-key shape [`OPTIONS_LABEL`] uses, and matched in
/// [`help_nav_for_key`](crate::help_nav_for_key).
///
/// Unlike `?` and `n` it is a **panel-only** key: it is absent from
/// [`ui_command_for_key`](crate::input::ui_command_for_key), because there is nothing
/// for it to copy on the board — the token is drawn here and nowhere else. That is
/// also why it is not on the Help tab's standing controls list, which holds only the
/// shortcuts true of every frame of every run; the drawn `[c]` teaches it where it
/// works, the way the tab bar's `[x]` teaches itself.
pub(crate) const COPY_KEY: char = 'c';

/// The copy control's word, drawn with its key as `copy [c]`. The whole run is the
/// target, label included, for the reason [`OPTIONS_LABEL`] gives: the word is the
/// larger and more obvious thing to reach for, and a bare `[c]` is only a target if
/// you already know what `c` means.
const COPY_LABEL: &str = "copy";

fn copy_control() -> String {
    format!("{COPY_LABEL} [{COPY_KEY}]")
}

fn copy_control_len() -> u32 {
    copy_control().chars().count() as u32
}

/// The column every Level info row is drawn from — one in from the section
/// headings, the panel's standing content indent.
const CONTENT_INDENT: u32 = 3;

/// Where the panel's content starts, below the tab bar and the blank rule under it —
/// the `y` [`render_help`] hands each tab, named so the Level info rows below can be
/// derived from it rather than counted by hand.
const CONTENT_TOP: u32 = 2;

/// The row the level-seed token is drawn on, and so the row its `copy [c]` control
/// shares with it: `THIS RUN` at [`CONTENT_TOP`], a blank, the `LEVEL SEED` heading,
/// then the token. Shared by [`draw_level_info`] and [`help_hit`] so a tap lands on
/// exactly the control drawn.
const SEED_TOKEN_ROW: u32 = CONTENT_TOP + 3;

/// The row under the token: the seed-copy acknowledgement's own line (#353). The
/// `replay [r]` control lodged on its right until #459 moved it to the Debug tab, and
/// the tab itself gave way to the options screen in #513 — the row keeps the spacer it
/// always was, so the modifier list below it never moved for either.
const SEED_ACK_ROW: u32 = SEED_TOKEN_ROW + 1;

/// **The modifier caption width bound** (#248). The panel fills the board, so the
/// narrowest screen a real run ever renders on is the v1 board
/// ([`LevelConfig::V1`], 40 wide — §10.2); a caption starts at
/// [`CONTENT_INDENT`] and leaves one column of right margin, the same margin the
/// `[x]` control keeps. Anything longer is silently clipped by [`draw`], which is
/// how "Sightings called in: one guard converges" reached a screenshot as
/// `…one guard conver`.
///
/// So it is checked **at compile time** against [`CAPTIONS`] — the whole set of
/// captions the card can draw — and a caption that would not fit fails the build
/// instead of the eye. Derived from the board width rather than written as a
/// number, so retuning §10.2 moves the bound with it.
const CAPTION_MAX: usize = (LevelConfig::V1.width - CONTENT_INDENT - 1) as usize;

// The bound bites here, over the complete caption set (§2.3 — a check that cannot
// be bypassed by adding a modifier, because `active` may only draw from `CAPTIONS`).
const _: () = {
    let mut i = 0;
    while i < CAPTIONS.len() {
        assert!(
            CAPTIONS[i].caption_len() <= CAPTION_MAX,
            "a level-modifier caption is too long for the Level info panel — \
             shorten its name or detail (see CAPTION_MAX in render::help)",
        );
        i += 1;
    }
};

/// **The panel's one right-alignment rule**: where a control `len` cells wide starts
/// on a screen `width` wide, with the one-cell right margin every column of the card
/// keeps. Every control on the panel is laid out through this — the tab bar's `[x]`,
/// the footer's `options [o]`, the token row's `copy [c]` — and each is *drawn* and
/// *hit-tested* from its own wrapper below, so a tap can only ever land on the cells
/// the frame actually drew.
pub(super) const fn right_aligned_start(width: u32, len: u32) -> u32 {
    width.saturating_sub(1 + len)
}

/// The column the close control starts at: right-aligned with a one-cell margin,
/// like the ability line's deploy button. Shared by the drawing and [`help_hit`]
/// so a tap lands on exactly the `[x]` drawn.
pub(super) const fn close_button_start(width: u32) -> u32 {
    right_aligned_start(width, CLOSE_BUTTON_LEN)
}

/// The column the footer's options control starts at — right-aligned with the same
/// one-cell margin the `[x]` keeps, so the two controls line up at the screen's right
/// edge. Shared by [`draw_footer`] and [`help_hit`] so a tap lands on exactly the
/// `options [o]` drawn, label included.
fn options_control_start(width: u32) -> u32 {
    right_aligned_start(width, options_control_len())
}

/// The column the token row's copy control starts at — the same right edge the `[x]`
/// and the options control keep, so the panel's controls stack in one column rather
/// than each finding its own place. Shared by [`draw_level_info`] and [`help_hit`].
fn copy_control_start(width: u32) -> u32 {
    right_aligned_start(width, copy_control_len())
}

/// The tab bar's one-cell left margin, and the one-cell gap between tabs — named so
/// the compile-time width check below measures the bar [`tab_layout`] actually draws.
const TAB_BAR_MARGIN: u32 = 1;
const TAB_GAP: u32 = 1;

/// Lay the tab bar out: each tab as `(tab, start col, width)`, drawn `[Label]` from a
/// one-cell margin with a one-cell gap between. The width is independent of which tab
/// is active (the brackets are always there), so switching tabs never shifts a hit
/// region. Shared by [`draw_tab_bar`] and [`help_hit`] so a tap lands on exactly the
/// tab drawn.
fn tab_layout() -> Vec<(HelpTab, u32, u32)> {
    let mut out = Vec::new();
    let mut x = TAB_BAR_MARGIN;
    for &tab in HelpTab::ALL.iter() {
        let len = tab.label().chars().count() as u32 + 2; // the enclosing `[` `]`
        out.push((tab, x, len));
        x += len + TAB_GAP;
    }
    out
}

/// The column past the **last** tab of the full bar — [`tab_layout`]'s arithmetic in
/// `const` form, over ASCII labels whose bytes are their cells.
const fn tab_bar_end() -> u32 {
    let mut x = TAB_BAR_MARGIN;
    let mut i = 0;
    while i < HelpTab::ALL.len() {
        x += HelpTab::ALL[i].label().len() as u32 + 2 + TAB_GAP;
        i += 1;
    }
    x - TAB_GAP // the trailing gap belongs to no tab
}

// **The tab bar has to fit its row** (§10.2/#459). The panel fills the board, so the
// narrowest screen a real run draws on is the v1 board's 40 columns, and the bar
// shares row 0 with the right-aligned `[x]`. [`draw`] clips in silence, so a tab that
// overran would simply arrive half-drawn — and, worse, still hit-test over the cells
// the `[x]` is painted on. This is also why the options screen is a screen and not a
// fourth tab (#513): `[Options]` does not fit here, and the check below is what said
// so rather than a player's eye.
const _: () = assert!(
    tab_bar_end() < close_button_start(LevelConfig::V1.width),
    "the help panel's tab bar runs into its [x] close control — shorten a tab label \
     (see tab_layout in render::help)",
);

/// The **level-seed token the Level info tab draws** for `level`, or `None` when
/// there is none to draw (§13.1/#333): a hand-built state was assembled cell by cell
/// and no token reproduces it, and a config no run can hold is one
/// [`LevelSeed::encode`] cannot express. Both answer `None`, and both mean the same
/// thing — there is nothing here worth taking away.
///
/// Shared by [`draw_level_info`] and [`help_hit`], so the copy control exists on
/// exactly the frames that print something for it to copy: the affordance and the
/// token can never disagree about whether this run has one.
pub(super) fn seed_token(level: Option<LevelSeed>) -> Option<String> {
    level.and_then(|level| level.encode())
}

/// The pointer→control hit-test for the open panel (§11.6/#248): which [`HelpHit`]
/// screen cell `(x, y)` lands on, or `None` for the body (a press the modal panel
/// swallows without acting). Three rows can carry controls — the tab bar (row 0), the
/// footer's `options [o]` control on the last row (#513), label and key alike, and the
/// Level info tab's token row with its `copy [c]` (#353) — and on the tab bar the
/// close `[x]` is tested first so it wins even if a layout ever abutted it.
///
/// It takes the panel's `height` for the footer row's sake: the footer is drawn from
/// the bottom up, so the hit-test has to measure from the same edge the drawing does
/// or a tap would land a row off on a shorter screen. The footer is also tested
/// *first*, which is the order [`render_help`] draws in — on a screen short enough for
/// the two to collide, the row belongs to whichever control is actually painted there.
///
/// It takes the shell's whole `ui` and the run's `level` because every control here is
/// conditional on one of them, and reading the same values [`render_help`] draws from
/// is what keeps a tap landing on exactly the cells the frame painted: the tab up
/// ([`ScreenUi::help_tab`](super::ScreenUi)), whether this session has a Debug tab at
/// all ([`ScreenUi::debug_mode`](super::ScreenUi), #459) — and the token itself, through the same [`seed_token`] the drawing uses, so a run
/// whose panel shows no seed section offers nothing to copy.
#[must_use]
pub fn help_hit(
    width: u32,
    height: u32,
    ui: ScreenUi,
    level: Option<LevelSeed>,
    x: u32,
    y: u32,
) -> Option<HelpHit> {
    if height > 0 && y == height - 1 {
        let options = options_control_start(width);
        if x >= options && x < options + options_control_len() {
            return Some(HelpHit::OpenSettings);
        }
        return None;
    }
    if y == 0 {
        let close = close_button_start(width);
        if x >= close && x < close + CLOSE_BUTTON_LEN {
            return Some(HelpHit::Close);
        }
        for (entry, start, len) in tab_layout() {
            if x >= start && x < start + len {
                return Some(HelpHit::Tab(entry));
            }
        }
        return None;
    }
    // The Level info tab is the only one whose *content* carries a control.
    match ui.help_tab {
        HelpTab::LevelInfo => {
            if seed_token(level).is_some() && y == SEED_TOKEN_ROW {
                let copy = copy_control_start(width);
                if x >= copy && x < copy + copy_control_len() {
                    return Some(HelpHit::CopySeed);
                }
            }
        }
        HelpTab::Abilities | HelpTab::Help => {}
    }
    None
}

/// Render the full-screen help panel (§14 v2/#139/#248): the tab bar, the active
/// tab's content, and a footer hint, filling a `width × height` grid on its own —
/// **the whole screen**, not an overlay on the map. Called by
/// [`render_screen`](super::render_screen) in place of the game frame while
/// [`ScreenUi::help_open`](super::ScreenUi) is set, so the panel is modal: nothing
/// of the game shows and the shell captures input against it.
///
/// It writes no state, so closing restores the exact frame beneath. Bounds are
/// clamped, never asserted: on a board too small for a row (only hand-built test
/// states get that small — the v1 board is 40×40, §10.2) that row shows what fits
/// and stops. The Abilities tab wraps its prose to the v1 width rather than relying
/// on that clamp, because a clipped sentence is a wrong sentence (see
/// [`abilities`]).
///
/// `ui` is the shell's whole view state; the panel reads the two fields that are its
/// own — the tab up ([`ScreenUi::help_tab`]) and the copy acknowledgement
/// ([`ScreenUi::seed_copy`], #353) — and ignores the rest, which
/// [`render_screen`](super::render_screen) has already adjudicated. `run` is what the
/// panel draws *about*: the facts of the run itself, bundled so the tabs' one entry
/// point stays readable as they multiply.
pub(super) fn render_help(width: u32, height: u32, ui: ScreenUi, run: PanelRun<'_>) -> Grid {
    let mut grid = blank_grid(width, height);

    draw_tab_bar(&mut grid, ui.help_tab);
    // Content begins two rows down, leaving the tab bar and a blank rule above it.
    match ui.help_tab {
        HelpTab::LevelInfo => draw_level_info(&mut grid, CONTENT_TOP, &run, ui.seed_copy),
        HelpTab::Abilities => abilities::draw_abilities(&mut grid, CONTENT_TOP, &run.bar),
        HelpTab::Help => draw_help_card(&mut grid, CONTENT_TOP),
    }
    draw_footer(&mut grid);
    grid
}

/// **What the panel draws about** — the run's own facts, as against the shell's view
/// state in [`ScreenUi`](super::ScreenUi). Bundled rather than passed one by one
/// because the panel's tabs each want a different subset and the list only grows:
/// [`render_screen`](super::render_screen) fills it from the live
/// [`State`](crate::State), and a test fills it by hand without needing one.
#[derive(Clone)]
pub(super) struct PanelRun<'a> {
    /// The run's reproducible config (§13.1/#245), whose token the Level info tab
    /// draws and both copy controls hand over. `None` for a hand-built state that no
    /// token reproduces.
    pub(super) level: Option<LevelSeed>,
    /// The level modifiers in force (§12.6) — the Level info tab's list.
    pub(super) modifiers: LevelModifiers,
    /// The facility alert as it stands (§7.3/#375) — the one section that moves while
    /// the panel is closed.
    pub(super) alert: &'a AlertReadout,
    /// **The abilities the ability bar is drawing**, in its slot order (§8.3/#343) —
    /// the Abilities tab's list, and the source of the `key / bar name` pairing it
    /// prints beside each one.
    ///
    /// The *bar's* row rather than the run's loadout, because the two differ while a
    /// crate is offering (§8.3/#266): the bar becomes the exchange's four candidates,
    /// and the digits follow it. A panel drawn from the held set there would pair every
    /// entry with the wrong digit — and would leave out the one ability the player most
    /// needs to read about, the one they are being offered.
    pub(super) bar: Vec<AbilityId>,
}

/// Draw the tab bar on row 0: each tab as `[Label]` — the active one in Interest
/// (the bright goal colour), the rest in Ground (dim) — and the right-aligned
/// `[x]` close control in System (the HUD-control colour, like the deploy button).
fn draw_tab_bar(grid: &mut Grid, active: HelpTab) {
    for (tab, start, _len) in tab_layout() {
        let category = if tab == active {
            Category::Interest
        } else {
            Category::Ground
        };
        draw(grid, start, 0, &format!("[{}]", tab.label()), category);
    }
    draw(
        grid,
        close_button_start(grid.width),
        0,
        CLOSE_BUTTON,
        Category::System,
    );
}

/// The **Level info** tab (§12.6/#248/#272): the run's **level-seed token** in
/// full — the one token that reproduces this exact run (§13.1/#245), spelling out
/// its modifiers and loadout rather than implying them — then its active level
/// modifiers, each by name and direction, or a clear "none active" when the run is
/// baseline. The modifier list is [`LevelModifiers::active`], so it is derived and
/// cannot drift — a new modifier field surfaces here on its own — and the token is
/// [`LevelSeed::encode`] of the run's own config, so the panel can never show a
/// string that boots a different game.
fn draw_level_info(grid: &mut Grid, mut y: u32, run: &PanelRun<'_>, copy: SeedCopy) {
    let (level, modifiers, alert) = (run.level, run.modifiers, run.alert);
    draw(grid, 2, y, "THIS RUN", Category::Interest);
    y += 2;

    // The level-seed token (§13.1/#245/#333): the handle that hands this run around.
    // Absent for a run with no token at all ([`seed_token`]), so the section — control
    // and all — is simply not there rather than showing an honest-looking string that
    // boots something else.
    if let Some(token) = seed_token(level) {
        draw(grid, 2, y, "LEVEL SEED", Category::System);
        y += 1;
        debug_assert_eq!(y, SEED_TOKEN_ROW, "the token row and its hit-test agree");
        // Interest, the goal/reward colour: this is the thing worth taking away
        // from the panel. One form — the token spells the whole config out, so what
        // the player copies off this panel is exactly what a link carries (#333).
        draw(grid, CONTENT_INDENT, y, &token, Category::Interest);
        // …and the control that actually takes it away (#353), right-aligned on the
        // token's own row in System — the HUD-control colour the `[x]` and the footer
        // button already wear, so it reads as a button rather than as more of the
        // token's ink. The token was the one thing on this panel that existed *to be
        // taken* and the only thing the player could not take.
        draw(
            grid,
            copy_control_start(grid.width),
            y,
            &copy_control(),
            Category::System,
        );
        y += 1;
        // The acknowledgement goes in the blank spacer that already sat under the
        // token, so saying whether the copy worked shifts nothing below it — the
        // modifier list stays exactly where it was drawn a frame earlier. The row is
        // the acknowledgement's alone again since #459 took the `replay [r]` control
        // to the Debug tab; it stays a spacer when there is nothing to say, which is
        // what keeps the list below it still.
        debug_assert_eq!(y, SEED_ACK_ROW, "the acknowledgement row is where it says");
        if let Some((text, category)) = copy.acknowledgement() {
            draw(grid, CONTENT_INDENT, y, text, category);
        }
        y += 1;
    }

    draw(grid, 2, y, "MODIFIERS", Category::System);
    y += 1;

    let active = modifiers.active();
    if active.is_empty() {
        // Baseline quick play: legible as "none active", not blank or absent (#248).
        draw(
            grid,
            CONTENT_INDENT,
            y,
            "none active — baseline rules",
            Category::Ground,
        );
        y += 1;
    }
    for m in active {
        // The modifier's own name carries the direction as a **colour cue** (§11.2):
        // the whole caption is drawn in Warning (orange) for a harder rule, Owned
        // (blue) for an easier one — pulled from the standing categories, not ad-hoc
        // styling. A bounded knob appends its value (`name: value`).
        let text = match m.detail {
            Some(detail) => format!("{}{CAPTION_SEPARATOR}{detail}", m.name),
            None => m.name.to_string(),
        };
        debug_assert!(
            text.chars().count() == m.caption_len(),
            "the drawn caption and the measured one must agree",
        );
        draw(
            grid,
            CONTENT_INDENT,
            y,
            &text,
            direction_category(m.direction),
        );
        y += 1;
    }

    // The facility alert (§7.3/#375), **last**: the modifiers above say what was
    // bending the rules before the raid started, and this says what the raid itself
    // has bent since. It goes below them because it is the only section that changes
    // while the panel is closed, and a growing list is better placed where nothing sits
    // under it to be pushed around.
    y += 1;
    super::alert::draw_alert(grid, y, alert, CONTENT_INDENT);
}

/// The §11.2 category a direction reads in — the colour cue the caption is drawn
/// in: Warning (a hunting threat's orange) for *harder*, Owned (yours, calm blue)
/// for *easier*. Pulled from the standing categories, never new ad-hoc styling
/// (§11.2/#248).
fn direction_category(direction: ModifierDirection) -> Category {
    match direction {
        ModifierDirection::Harder => Category::Warning,
        ModifierDirection::Easier => Category::Owned,
    }
}

/// The **Help** tab (#139/#296): the glyph legend, the colour key, and the
/// **standing** controls — the original reference card, now one tab of the panel.
///
/// Nothing here varies with the run: it takes no loadout, no modifiers and no seed,
/// so the card a player learns is the same card every run. The abilities that used
/// to sit in `CONTROLS` moved to their own tab (#343), where they can say what they
/// do rather than only which key they answer to.
fn draw_help_card(grid: &mut Grid, mut y: u32) {
    draw(grid, 2, y, "GLYPHS", Category::System);
    y += 1;
    for (glyph, category, meaning) in glyph_rows() {
        draw(grid, 3, y, &glyph.to_string(), category);
        draw(grid, GLYPH_MEANING_X, y, meaning, Category::Neutral);
        y += 1;
    }
    y += 1;

    draw(grid, 2, y, "COLOURS", Category::System);
    y += 1;
    for category in CATEGORIES {
        // The name is drawn *in its own colour*, so the player reads the colour and
        // its meaning on one line.
        draw(grid, 3, y, category_name(category), category);
        draw(
            grid,
            COLOUR_MEANING_X,
            y,
            category_meaning(category),
            Category::Neutral,
        );
        y += 1;
    }
    y += 1;

    draw(grid, 2, y, "CONTROLS", Category::System);
    y += 1;
    for (keys, action) in control_rows() {
        draw(grid, CONTROL_KEYS_X, y, &keys, Category::System);
        draw(grid, CONTROL_ACTION_X, y, &action, Category::Neutral);
        y += 1;
    }
}

/// Where the controls card's two columns start: the keys on the left, the action
/// they perform on the right. Named because the gap between them is the keys
/// column's whole width budget — an entry that grows past it runs into the action
/// beside it — and a test pins the widest entry against that (§11.4's row-fits rule,
/// applied to the panel rather than the bar).
const CONTROL_KEYS_X: u32 = 3;
const CONTROL_ACTION_X: u32 = 26;

/// Where the glyph legend's meaning column starts — three cells in from the glyph,
/// which is one cell wide.
const GLYPH_MEANING_X: u32 = 6;

/// Where the colour key's meaning column starts, clear of the widest
/// [`category_name`].
const COLOUR_MEANING_X: u32 = 14;

/// **The right margin every card column is measured against** (#248's `CAPTION_MAX`,
/// generalised). The panel fills the board, so the narrowest screen a real run
/// renders on is the v1 board (40 wide — §10.2), and every text column leaves the
/// same one cell of right margin the `[x]` control keeps.
///
/// This is the bound the colour key was missing. `Sensed` and `Effect` had meanings
/// of 33 and 38 cells in a 25-cell column, so the card shipped reading
/// `guard or door, felt throug` and `what your gadget did, and ` — [`draw`] clips in
/// silence, exactly as it did for the modifier caption that reached a screenshot as
/// `…one guard conver`. A truncated explanation is worse than a short one: it looks
/// like the whole sentence.
const fn column_width(start: u32) -> usize {
    (LevelConfig::V1.width - start - 1) as usize
}

// The bound bites at **compile time**, over every fixed column of the card, so a
// meaning that would not fit fails the build instead of the eye (§2.3 — a check that
// cannot be bypassed, because both lists are exhaustive matches over their enums).
//
// Measured in **bytes**, which is conservative rather than exact: a UTF-8 string is
// never fewer bytes than cells, so passing this guarantees the row fits. A meaning
// written with an em-dash therefore has to be a little shorter than one without —
// a fair price for a check that runs at build time.
const _: () = {
    let mut i = 0;
    while i < CATEGORIES.len() {
        assert!(
            category_name(CATEGORIES[i]).len() <= column_width(3) - column_width(COLOUR_MEANING_X),
            "a colour-key name runs into the meaning beside it (see COLOUR_MEANING_X)",
        );
        assert!(
            category_meaning(CATEGORIES[i]).len() <= column_width(COLOUR_MEANING_X),
            "a colour-key meaning is too long for the Help card — shorten it \
             (see column_width in render::help)",
        );
        i += 1;
    }
};

/// The footer row's left indent — where the hint prose starts on the panel, on the
/// title screen and on the campaign map alike (§11.4), leaving the row's right edge to
/// whichever control that screen carries.
pub(super) const FOOTER_INDENT: u32 = 2;

/// Draw the footer on the last row: how to switch tabs and close — so a player who
/// opened the modal panel always sees the way out (§11.6's no-trap rule, made explicit
/// now the header `[?]` is covered) — and the `options [o]` control beside it.
fn draw_footer(grid: &mut Grid) {
    if grid.height == 0 {
        return;
    }
    let row = grid.height - 1;
    draw(grid, FOOTER_INDENT, row, FOOTER_HINT, Category::Ground);
    // The options control, right-aligned on the same row. **Label and key together** in
    // System — the HUD-control colour the `[x]` and the near line's `[?]` share — so
    // the word reads as part of the button rather than as more footer prose, which is
    // what makes it obvious the word is the thing to press (#189's rule, kept by the
    // control that took the corner over in #513).
    draw(
        grid,
        options_control_start(grid.width),
        row,
        &options_control(),
        Category::System,
    );
}

/// The footer hint — the keys the panel answers that have no on-screen control of
/// their own. Named so the layout test can measure it against
/// [`options_control_start`] rather than trusting that a longer sentence would have
/// been noticed: [`draw`] clips in silence, and here it would clip the control, not
/// the prose.
const FOOTER_HINT: &str = "Tab switches   Esc closes";

/// The glyph legend (§11.3): each `(glyph, category, meaning)`, glyph and category
/// pulled from the real source — [`Terrain`] for the terrain rows, the [`super`]
/// render constants for the entity rows — so the card cannot show a mark the board
/// does not.
fn glyph_rows() -> Vec<(char, Category, &'static str)> {
    // A terrain row derives both its glyph and its colour meaning from the §10.3
    // table itself, so the two can never disagree with what the world draws.
    let terrain = |t: Terrain, meaning: &'static str| (t.glyph(), t.category(), meaning);
    vec![
        (PLAYER_GLYPH, Category::Owned, "you"),
        (
            GUARD_GLYPH,
            Category::Caution,
            "a guard (colour = its state)",
        ),
        (BODY_GLYPH, Category::Caution, "a body you left"),
        terrain(Terrain::Wall, "wall"),
        terrain(Terrain::DoorPanelClosed, "a closed door"),
        terrain(Terrain::DoorHinge, "a door frame"),
        terrain(Terrain::Hideout, "cupboard — bump to hide"),
        terrain(Terrain::PartialCover, "table — bump to crouch"),
        terrain(Terrain::DuctEntry, "duct — bump to crawl in"),
        terrain(Terrain::Console, "intel — bump to take"),
        terrain(Terrain::CommsConsole, "comms — bump to kill the radio"),
        terrain(Terrain::EquipmentCache, "cache — bump to salvage tech"),
        terrain(Terrain::Exit, "the exit"),
        (FLOOR_DOT, Category::Ground, "floor you can see"),
        // The schematic (§11.5a/#307/#470): what the plans give you before you have
        // been there. One row, because the plan is one mark and one absence — the
        // floor space between the fabric draws blank, exactly as floor out of your
        // sight does, and a blank row on a card teaches nothing.
        (SCHEMATIC_WALL, Category::Neutral, "building — not yet seen"),
    ]
}

/// Every information category (§11.2), in reading order. Paired with
/// [`category_meaning`] this is the colour key — the shell draws each name in the
/// colour the category maps to, so the player sees the colour and its meaning
/// together.
const CATEGORIES: [Category; 10] = [
    Category::Owned,
    Category::Caution,
    Category::Warning,
    Category::Danger,
    Category::Sensed,
    Category::Effect,
    Category::Interest,
    Category::System,
    Category::Neutral,
    Category::Ground,
];

/// What each colour category *means* (§11.2), as one line for the legend. An
/// exhaustive match, so adding a [`Category`] will not compile until it is given a
/// meaning here — the card can never silently omit a colour.
const fn category_meaning(category: Category) -> &'static str {
    match category {
        Category::Neutral => "inert scenery",
        Category::Ground => "floor you can cross",
        Category::Owned => "you and your things",
        Category::Caution => "an unaware threat",
        Category::Warning => "a hunting threat",
        Category::Danger => "you're in its cone",
        Category::Interest => "a goal or reward",
        Category::System => "door / cupboard / duct",
        // Both of these used to run off the board and clip mid-word. Shortened to
        // the fact each colour actually carries — that it was *not seen* (§9.2), and
        // that it is *your* gadget's mark (§11.5/#344) — because the neighbouring
        // GLYPHS section already says what the things themselves are.
        Category::Sensed => "guard or door, unseen",
        Category::Effect => "what your gadget did",
    }
}

/// The **standing** controls (§11.6/#296), each `(keys, action)` — the shortcuts that
/// are true of every run: move, wait, the ability bar's four digits, the message log,
/// this panel, and the colour theme (#189).
///
/// It used to list the abilities too, one row per [`AbilityId::ALL`] entry. That was
/// wrong twice over: it named all eight when a run holds at most four (§8.3), so half
/// the rows advertised a key that did nothing this run and nothing distinguished them
/// from the half that worked; and a card that changes with the loadout is not a
/// legend. The abilities now have a tab of their own ([`abilities`], #343), keyed off
/// the run's real loadout and able to say what each one *does* — which is the question
/// a key on its own never answered.
fn control_rows() -> Vec<(String, String)> {
    vec![
        // "num" rather than the bare `8246` the row used to print: the digit path is
        // the **numpad**'s now (#359), bound by physical code so it steps on any
        // layout, while the top row's digits went to the abilities below. The keys
        // column has 22 cells, which is why the digits themselves are left to §11.6
        // and to the numpad's own printing.
        // `hjkl` left the row with the binding (#368): the vi keys stepped for a
        // while, and the alphabet they held was worth more to the ability mnemonics
        // than the comfort was.
        ("arrows / num".to_string(), "move".to_string()),
        ("w / num 5 / .".to_string(), "wait & sense".to_string()),
        // The abilities are a standing control after all (#359): *which* ability a
        // digit fires is the run's business — the Abilities tab pairs each with its
        // slot — but that `1`–`4` fire the bar, left to right, is true of every run,
        // which is precisely what belongs on a legend.
        ("1234".to_string(), "abilities".to_string()),
        ("m".to_string(), "messages".to_string()),
        (HELP_KEY.to_string(), "this help".to_string()),
        // The theme toggle (#189) is a standing shortcut like the rest: it is true of
        // every run, and it is the one row that also works *while this card is up*.
        (THEME_KEY.to_string(), "colour theme".to_string()),
    ]
}

/// Where the Abilities tab's key column starts (§11.4/#343): the full §8.3 name runs
/// from [`CONTENT_INDENT`] up to here, and the `key / bar name` pairing from here
/// to the right margin.
///
/// It lives on this module rather than the tab's, beside [`CONTROL_ACTION_X`], because
/// the two are the same decision — the panel's one two-column measure — and a reader
/// checking whether the layout holds should find both bounds in one place. The
/// widest entry a legal catalogue can produce is measured against it by
/// `the_widest_entry_heading_fits_the_board`.
const fn ability_keys_column_start() -> u32 {
    16
}

/// The category's display name for the colour key — its own identifier, so the key
/// names exactly the [`Category`] the renderer tags cells with.
const fn category_name(category: Category) -> &'static str {
    match category {
        Category::Neutral => "Neutral",
        Category::Ground => "Ground",
        Category::Owned => "Owned",
        Category::Caution => "Caution",
        Category::Warning => "Warning",
        Category::Danger => "Danger",
        Category::Interest => "Interest",
        Category::System => "System",
        Category::Sensed => "Sensed",
        Category::Effect => "Effect",
    }
}

#[cfg(test)]
mod tests;
