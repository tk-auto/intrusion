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
//! - *Options* land as a fourth tab (§14 v2 "options"; #189 light mode, #237
//!   difficulty).
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
    blank_grid, draw, Grid, BODY_GLYPH, FLOOR_DOT, GUARD_GLYPH, PLAYER_GLYPH, SCHEMATIC_GROUND,
    SCHEMATIC_WALL,
};
use crate::ability::Loadout;
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

/// The key that flips the colour theme (§11.2/#189) — `n`, for *night* mode. It is
/// the panel's own option for now (§14 v2 lists options next to this help screen),
/// so it is drawn as a footer button here as well as listed in the controls, and it
/// is matched in [`ui_command_for_key`](crate::input::ui_command_for_key) (on the
/// board) and [`help_nav_for_key`](crate::help_nav_for_key) (with the panel up).
/// The tests below pin the character against both tables, so the card and the
/// binding can never drift apart.
pub(crate) const THEME_KEY: char = 'n';

/// The tabs the help panel pages between (§14 v2/#248). The panel opens on the
/// leftmost ([`Default`]) and the tab bar switches between them; a shell keeps the
/// current tab on [`ScreenUi`](super::ScreenUi) and hands it to [`render_help`].
///
/// Ordered as the player reads them left to right — *this run* first, the standing
/// reference last — and cycled by [`next`](Self::next)/[`prev`](Self::prev) so
/// the tab bar wraps at either end. A fourth *Options* tab slots in here.
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
    /// Every tab, in reading (left-to-right) order — the tab bar's layout and the
    /// cycle order. A new tab is one entry here.
    ///
    /// Ordered outward from *this run*: the run's rules, then the run's abilities,
    /// then the standing reference that never changes.
    pub const ALL: [HelpTab; 3] = [HelpTab::LevelInfo, HelpTab::Abilities, HelpTab::Help];

    /// The label shown on the tab bar and used to size its hit region.
    fn label(self) -> &'static str {
        match self {
            HelpTab::LevelInfo => "Level info",
            HelpTab::Abilities => "Abilities",
            HelpTab::Help => "Help",
        }
    }

    /// The next tab, wrapping past the last back to the first — the panel's
    /// "advance" motion (Tab / rightward keys, §14 v2/#248).
    #[must_use]
    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&t| t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    /// The previous tab, wrapping past the first back to the last.
    #[must_use]
    pub fn prev(self) -> Self {
        let i = Self::ALL.iter().position(|&t| t == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
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
    /// The footer's `[n]` theme button — flip the colour table (§11.2/#189), the
    /// touch half of the key. Without it the theme would be a keyboard-only option
    /// on a game that is played on phones (§11.6: every control reachable by key
    /// *and* touch).
    ToggleTheme,
    /// The Level info tab's `copy [c]` control — put this run's level-seed token on
    /// the system clipboard (§13.1/#353). The **core neither performs nor knows
    /// about** the write: it owns the geometry and the token, and the shell owns the
    /// clipboard (§12.1). Only ever produced when a token is actually drawn, so a
    /// hand-built state offers no control rather than one that copies nothing.
    CopySeed,
}

/// Whether the player's last attempt to copy this run's level-seed token reached the
/// clipboard (§13.1/#353) — the acknowledgement the Level info tab prints under the
/// token, and the panel's answer to "did that work?".
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
    fn acknowledgement(self) -> Option<(&'static str, Category)> {
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
/// other `[?]`/`[▾]` buttons, so the escape reads as a button.
const CLOSE_BUTTON: &str = "[x]";
const CLOSE_BUTTON_LEN: u32 = 3;

/// The theme control on the panel's **footer** row (#189): the word it does,
/// followed by the key it answers to — `theme [n]`.
///
/// **The label is part of the control, not prose beside it.** A bare `[n]` is only a
/// target if you already know what `n` means, and the word is the larger and more
/// obvious thing to reach for — so the whole `theme [n]` run is drawn in the button
/// colour and the whole run hit-tests ([`help_hit`]). The bracketed key still teaches
/// the shortcut, the way `[?]` and `[x]` do.
///
/// It sits on the footer rather than the tab bar because the tab bar is full at the
/// v1 width (§10.2) and a fourth tab is already planned for it; the footer is the
/// row that already teaches the panel's controls, and it is drawn on every tab, so
/// [`help_hit`] needs no notion of which tab is showing.
pub(super) const THEME_LABEL: &str = "theme";

pub(super) fn theme_control() -> String {
    format!("{THEME_LABEL} [{THEME_KEY}]")
}

pub(super) fn theme_control_len() -> u32 {
    theme_control().chars().count() as u32
}

/// The key that copies the run's **level-seed token** to the clipboard while the
/// panel is open (§13.1/#353) — `c`, for *copy*. It is drawn as `copy [c]` beside the
/// token, the same label-and-key shape [`THEME_LABEL`] uses, and matched in
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
/// target, label included, for the reason [`THEME_LABEL`] gives: the word is the
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
/// the footer's `theme [n]`, the token row's `copy [c]` — and each is *drawn* and
/// *hit-tested* from its own wrapper below, so a tap can only ever land on the cells
/// the frame actually drew.
const fn right_aligned_start(width: u32, len: u32) -> u32 {
    width.saturating_sub(1 + len)
}

/// The column the close control starts at: right-aligned with a one-cell margin,
/// like the ability line's deploy button. Shared by the drawing and [`help_hit`]
/// so a tap lands on exactly the `[x]` drawn.
const fn close_button_start(width: u32) -> u32 {
    right_aligned_start(width, CLOSE_BUTTON_LEN)
}

/// The column the footer's theme control starts at — right-aligned with the same
/// one-cell margin the `[x]` keeps, so the two controls line up at the screen's right
/// edge. Shared by [`draw_footer`] and [`help_hit`] so a tap lands on exactly the
/// `theme [n]` drawn, label included — **and by the title screen** ([`super::menu`]),
/// which puts the control in the very same corner of its own footer row, so the one
/// option the game has so far is in one place wherever you meet it.
pub(super) fn theme_control_start(width: u32) -> u32 {
    right_aligned_start(width, theme_control_len())
}

/// The column the token row's copy control starts at — the same right edge the `[x]`
/// and the theme control keep, so the panel's three controls stack in one column
/// rather than each finding its own place. Shared by [`draw_level_info`] and
/// [`help_hit`].
fn copy_control_start(width: u32) -> u32 {
    right_aligned_start(width, copy_control_len())
}

/// Lay the tab bar out: each tab as `(tab, start col, width)`, drawn `[Label]`
/// from a one-cell margin with a one-cell gap between. The width is independent of
/// which tab is active (the brackets are always there), so switching tabs never
/// shifts a hit region. Shared by [`draw_tab_bar`] and [`help_hit`] so a tap lands
/// on exactly the tab drawn.
fn tab_layout() -> Vec<(HelpTab, u32, u32)> {
    let mut out = Vec::new();
    let mut x = 1u32; // the one-cell left margin
    for tab in HelpTab::ALL {
        let len = tab.label().chars().count() as u32 + 2; // the enclosing `[` `]`
        out.push((tab, x, len));
        x += len + 1; // one cell of gap between tabs
    }
    out
}

/// The **level-seed token the Level info tab draws** for `level`, or `None` when
/// there is none to draw (§13.1/#333): a hand-built state was assembled cell by cell
/// and no token reproduces it, and a config no run can hold is one
/// [`LevelSeed::encode`] cannot express. Both answer `None`, and both mean the same
/// thing — there is nothing here worth taking away.
///
/// Shared by [`draw_level_info`] and [`help_hit`], so the copy control exists on
/// exactly the frames that print something for it to copy: the affordance and the
/// token can never disagree about whether this run has one.
fn seed_token(level: Option<LevelSeed>) -> Option<String> {
    level.and_then(|level| level.encode())
}

/// The pointer→control hit-test for the open panel (§11.6/#248): which [`HelpHit`]
/// screen cell `(x, y)` lands on, or `None` for the body (a press the modal panel
/// swallows without acting). Three rows can carry controls — the tab bar (row 0), the
/// footer's `theme [n]` control on the last row (#189), label and key alike, and the
/// Level info tab's token row with its `copy [c]` (#353) — and on the tab bar the
/// close `[x]` is tested first so it wins even if a layout ever abutted it.
///
/// It takes the panel's `height` for the footer row's sake: the footer is drawn from
/// the bottom up, so the hit-test has to measure from the same edge the drawing does
/// or a tap would land a row off on a shorter screen. The footer is also tested
/// *first*, which is the order [`render_help`] draws in — on a screen short enough for
/// the two to collide, the row belongs to whichever control is actually painted there.
///
/// It takes `tab` and `level` for the copy control's sake, and reads the token through
/// the same [`seed_token`] the drawing does: the control is offered on the Level info
/// tab, on the token's own row, and only when there is a token — never on a run whose
/// panel shows no seed section at all.
#[must_use]
pub fn help_hit(
    width: u32,
    height: u32,
    tab: HelpTab,
    level: Option<LevelSeed>,
    x: u32,
    y: u32,
) -> Option<HelpHit> {
    if height > 0 && y == height - 1 {
        let theme = theme_control_start(width);
        if x >= theme && x < theme + theme_control_len() {
            return Some(HelpHit::ToggleTheme);
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
    if tab == HelpTab::LevelInfo && y == SEED_TOKEN_ROW && seed_token(level).is_some() {
        let copy = copy_control_start(width);
        if x >= copy && x < copy + copy_control_len() {
            return Some(HelpHit::CopySeed);
        }
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
pub(super) fn render_help(
    width: u32,
    height: u32,
    tab: HelpTab,
    level: Option<LevelSeed>,
    modifiers: LevelModifiers,
    alert: &AlertReadout,
    loadout: Loadout,
    copy: SeedCopy,
) -> Grid {
    let mut grid = blank_grid(width, height);

    draw_tab_bar(&mut grid, tab);
    // Content begins two rows down, leaving the tab bar and a blank rule above it.
    match tab {
        HelpTab::LevelInfo => {
            draw_level_info(&mut grid, CONTENT_TOP, level, modifiers, alert, copy)
        }
        HelpTab::Abilities => abilities::draw_abilities(&mut grid, CONTENT_TOP, loadout),
        HelpTab::Help => draw_help_card(&mut grid, CONTENT_TOP),
    }
    draw_footer(&mut grid);
    grid
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
fn draw_level_info(
    grid: &mut Grid,
    mut y: u32,
    level: Option<LevelSeed>,
    modifiers: LevelModifiers,
    alert: &AlertReadout,
    copy: SeedCopy,
) {
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
        // token's own row in System — the HUD-control colour the `[x]` and the theme
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
        // modifier list stays exactly where it was drawn a frame earlier.
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

/// Draw the footer hint on the last row: how to switch tabs and close, so a player
/// who opened the modal panel always sees the way out (§11.6's no-trap rule, made
/// explicit now the header `[?]` is covered).
/// The footer row's left indent — where the hint prose starts on the panel and on
/// the title screen alike (§11.4), leaving the row's right edge to the theme control.
pub(super) const FOOTER_INDENT: u32 = 2;

fn draw_footer(grid: &mut Grid) {
    if grid.height == 0 {
        return;
    }
    let row = grid.height - 1;
    draw(grid, FOOTER_INDENT, row, FOOTER_HINT, Category::Ground);
    // The theme control, right-aligned on the same row. **Label and key together** in
    // System — the HUD-control colour the `[x]` and the near line's `[?]` share — so
    // the word reads as part of the button rather than as more footer prose, which is
    // what makes it obvious the word is the thing to press (#189).
    draw(
        grid,
        theme_control_start(grid.width),
        row,
        &theme_control(),
        Category::System,
    );
}

/// The footer hint — the keys the panel answers that have no on-screen control of
/// their own. Named so the layout test can measure it against
/// [`theme_control_start`] rather than trusting that a longer sentence would have
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
        terrain(Terrain::Exit, "the exit"),
        (FLOOR_DOT, Category::Ground, "floor"),
        // The schematic (§11.5a/#307): what the plans give you before you have been
        // there. Two rows, because the two marks are the whole vocabulary — walking
        // in resolves either one into what is really there.
        (SCHEMATIC_WALL, Category::Neutral, "building — not yet seen"),
        (SCHEMATIC_GROUND, Category::Ground, "floor — not yet seen"),
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
mod tests {
    use super::*;
    use crate::ability::AbilityId;
    use crate::alert::{AlertEffect, AlertTrigger, AlertTuning};
    use crate::modifiers::{ActiveModifier, IntelGate};

    /// A full-screen frame the size of the v1 board's screen (§10.2) — wide enough
    /// that no row truncates, so a test can read the panel's content whole.
    pub(super) const W: u32 = 40;
    pub(super) const H: u32 = 43; // TOP_ROWS + 40 + BOTTOM_ROWS

    pub(super) fn text_of(grid: &Grid) -> String {
        grid.to_text().join("\n")
    }

    /// A facility that has not noticed you (§7.3) — the readout most of these tests
    /// want, since every section but the alert one is the same at any rung. The tests
    /// that *are* about the rung build their own.
    pub(super) fn quiet_alert() -> AlertReadout {
        AlertReadout {
            rung: 0,
            effects: Vec::new(),
        }
    }

    /// One tab of a baseline run's panel, at the v1 screen size — the shape most
    /// tests want, including the [`abilities`] tab's, which is why it is
    /// `pub(super)` rather than local.
    pub(super) fn render_tab(tab: HelpTab, loadout: Loadout) -> Grid {
        render_help(
            W,
            H,
            tab,
            None,
            LevelModifiers::default(),
            &quiet_alert(),
            loadout,
            SeedCopy::default(),
        )
    }

    /// The glyph legend is **derived**, not hand-copied (§11.3): every terrain row's
    /// glyph and category equal the source table's, so an edit to [`Terrain::glyph`]
    /// or [`Terrain::category`] moves the legend with it. The entity rows show the
    /// same constants the world render draws.
    #[test]
    fn the_glyph_legend_matches_the_render_source() {
        let rows = glyph_rows();
        // Entity rows use the render constants.
        assert!(rows
            .iter()
            .any(|&(g, c, _)| g == PLAYER_GLYPH && c == Category::Owned));
        assert!(rows.iter().any(|&(g, _, _)| g == GUARD_GLYPH));
        assert!(rows.iter().any(|&(g, _, _)| g == BODY_GLYPH));
        assert!(rows
            .iter()
            .any(|&(g, c, _)| g == FLOOR_DOT && c == Category::Ground));
        // Terrain rows equal the §10.3 source exactly.
        for t in [
            Terrain::Wall,
            Terrain::DoorPanelClosed,
            Terrain::DoorHinge,
            Terrain::Hideout,
            Terrain::PartialCover,
            Terrain::DuctEntry,
            Terrain::Console,
            Terrain::CommsConsole,
            Terrain::Exit,
        ] {
            assert!(
                rows.iter()
                    .any(|&(g, c, _)| g == t.glyph() && c == t.category()),
                "the legend must carry {t:?} exactly as the terrain table draws it",
            );
        }
    }

    /// **No row of the card clips.** Every column of the Help tab is measured in
    /// *cells* against the v1 board's right margin — the glyph meanings, the colour
    /// names and meanings, and both control columns.
    ///
    /// The `const` guard above covers the colour key in bytes, which is conservative
    /// but blind to the em-dashes the glyph rows carry ("cupboard — bump to hide"),
    /// so this is the exact check over everything the card draws. It is the guard
    /// `Sensed` and `Effect` did not have: they shipped clipped mid-word as
    /// `guard or door, felt throug` and `what your gadget did, and `, and nothing
    /// failed — [`draw`] truncates in silence, which is why a bound has to exist
    /// somewhere that does not.
    #[test]
    fn no_row_of_the_help_card_is_clipped() {
        let fits = |text: &str, start: u32, what: &str| {
            let room = (LevelConfig::V1.width - start - 1) as usize;
            assert!(
                text.chars().count() <= room,
                "{what} {text:?} is {} cells and its column has {room}",
                text.chars().count(),
            );
        };
        for (_, _, meaning) in glyph_rows() {
            fits(meaning, GLYPH_MEANING_X, "glyph meaning");
        }
        for category in CATEGORIES {
            fits(
                category_meaning(category),
                COLOUR_MEANING_X,
                "colour meaning",
            );
            // A name has to clear the meaning column beside it, not just the margin.
            fits(category_name(category), COLOUR_MEANING_X, "colour name");
            let name_end = CONTENT_INDENT + category_name(category).chars().count() as u32;
            assert!(
                name_end < COLOUR_MEANING_X,
                "{category:?}'s name runs into the meaning beside it",
            );
        }
        for (keys, action) in control_rows() {
            fits(&keys, CONTROL_KEYS_X, "control keys");
            fits(&action, CONTROL_ACTION_X, "control action");
        }
    }

    /// Every colour category has a meaning *and* a name in the key — an exhaustive
    /// match guarantees the meaning, and the name list must stay complete too.
    #[test]
    fn every_category_is_documented() {
        assert_eq!(CATEGORIES.len(), 10, "all ten §11.2 categories are keyed");
        for &c in &CATEGORIES {
            assert!(!category_meaning(c).is_empty(), "{c:?} has a meaning");
            assert!(!category_name(c).is_empty(), "{c:?} has a name");
        }
    }

    /// The controls card keeps only the **standing** shortcuts (#296): the six rows
    /// that are true of every run, and no *named* ability. The ability row earns its
    /// place by naming the keys rather than what they fire (#359) — the pairing is the
    /// Abilities tab's job, since it changes with the loadout. It documents its own
    /// keys, too.
    #[test]
    fn the_control_rows_are_the_standing_shortcuts_only() {
        let rows = control_rows();
        for action in [
            "move",
            "wait & sense",
            "abilities",
            "messages",
            "this help",
            "colour theme",
        ] {
            assert!(
                rows.iter().any(|(_, a)| a == action),
                "the controls list {action:?}",
            );
        }
        assert_eq!(rows.len(), 6, "and nothing else — no per-ability rows");
        // The panel's own two keys document themselves.
        assert!(rows.iter().any(|(k, _)| *k == HELP_KEY.to_string()));
        assert!(rows.iter().any(|(k, _)| *k == THEME_KEY.to_string()));
    }

    /// **Nothing on the Legend varies with the run** (#296) — no ability name, no bar
    /// name, no ability key pairing anywhere on the tab, whatever the loadout. That is
    /// what makes it a legend rather than a per-run card, and it is what keeps the
    /// Abilities tab (#343) the single place a loadout-derived ability list is drawn.
    #[test]
    fn no_ability_reaches_the_help_tab() {
        for loadout in [Loadout::full(), Loadout::innate(), Loadout::empty()] {
            let text = text_of(&render_tab(HelpTab::Help, loadout));
            for id in AbilityId::ALL {
                assert!(
                    !text.contains(id.name()),
                    "{} is on the Abilities tab, not the Legend",
                    id.name(),
                );
                for slot in 0..AbilityId::MAX_HELD {
                    assert!(
                        !text.contains(&format!("{} / {}", slot + 1, id.bar_name())),
                        "{}'s key pairing is on the Abilities tab, not the Legend",
                        id.name(),
                    );
                }
            }
        }
    }

    /// The keys column has a width budget like everything else on a 40-wide board
    /// (§11.4): it runs until the action column starts, and every row the card draws
    /// has to leave a gutter rather than run into the action beside it.
    #[test]
    fn the_widest_control_row_clears_the_action_column() {
        let column = (CONTROL_ACTION_X - CONTROL_KEYS_X) as usize;
        for (keys, action) in control_rows() {
            assert!(
                keys.chars().count() < column,
                "{keys:?} is {} cells and the keys column has {column}, gutter included",
                keys.chars().count(),
            );
            assert!(
                CONTROL_ACTION_X as usize + action.chars().count() < LevelConfig::V1.width as usize,
                "{action:?} runs past the board's right margin",
            );
        }
    }

    /// The **Legend** tab still carries the whole reference card — the three
    /// sections and a glyph derived from the real terrain table (the duct `=`, §10.7).
    #[test]
    fn the_help_tab_carries_the_glyphs_colours_and_controls() {
        let text = text_of(&render_tab(HelpTab::Help, Loadout::innate()));
        assert!(text.contains("GLYPHS") && text.contains("COLOURS") && text.contains("CONTROLS"));
        for glyph in [Terrain::DuctEntry.glyph(), Terrain::Exit.glyph(), '}', '$'] {
            assert!(text.contains(glyph), "the legend shows {glyph:?}");
        }
        for keys in ["arrows / num", "w / num 5 / .", "1234"] {
            assert!(text.contains(keys), "the controls show {keys:?}");
        }
        // The Legend tab is not the Level-info tab: its modifier section is elsewhere.
        assert!(
            !text.contains("MODIFIERS"),
            "MODIFIERS lives on the other tab"
        );
    }

    /// The **Level info** tab lists the run's active modifiers by name, and a
    /// baseline run reads clearly as "none active" (#248). The rows are derived from
    /// [`LevelModifiers::active`], so the tab cannot drift from the resolved set.
    #[test]
    fn the_level_info_tab_lists_active_modifiers_or_none() {
        // Baseline: "none active", not blank.
        let baseline = render_help(
            W,
            H,
            HelpTab::LevelInfo,
            None,
            LevelModifiers::default(),
            &quiet_alert(),
            Loadout::innate(),
            SeedCopy::default(),
        );
        let text = text_of(&baseline);
        assert!(text.contains("THIS RUN") && text.contains("MODIFIERS"));
        assert!(
            text.contains("none active"),
            "baseline reads none active: still legible"
        );

        // A harder toggle and the harder knob both surface, by name — and, for the
        // knob, its value.
        let modified = LevelModifiers {
            guards_always_search_hideouts: true,
            intel_to_exit: IntelGate::All,
            ..LevelModifiers::default()
        };
        let g = render_help(
            W,
            H,
            HelpTab::LevelInfo,
            None,
            modified,
            &quiet_alert(),
            Loadout::innate(),
            SeedCopy::default(),
        );
        let text = text_of(&g);
        assert!(
            !text.contains("none active"),
            "an active run does not read none"
        );
        assert!(text.contains("Guards search hideouts"));
        assert!(
            text.contains("Intel to exit: all of it"),
            "a bounded knob renders its value: {text:?}"
        );
    }

    /// #248: **every** caption the card can draw fits the board it is drawn on,
    /// with every modifier at once. The compile-time bound (`CAPTION_MAX`) already
    /// makes an over-long caption a build failure; this is its runtime companion —
    /// it renders the real panel and checks no row was clipped, so the bound is
    /// tied to an actual frame rather than to arithmetic that could drift from the
    /// layout. (Regression: "Sightings called in: one guard converges" was drawn
    /// as `…one guard conver` on the v1 board.)
    #[test]
    fn no_modifier_caption_is_clipped_on_the_board() {
        // Every toggle on, and the knob at each of its non-baseline values, so all
        // five captions in `CAPTIONS` are exercised across the two renders.
        for gate in [IntelGate::All, IntelGate::None] {
            let all_on = LevelModifiers {
                guards_always_search_hideouts: true,
                sighting_lost_calls_a_guard: true,
                body_found_calls_two_guards: true,
                always_show_vision_cones: true,
                full_layout_known: true,
                calm_guards_detect_only_their_cone: true,
                intel_to_exit: gate,
            };
            let g = render_help(
                W,
                H,
                HelpTab::LevelInfo,
                None,
                all_on,
                &quiet_alert(),
                Loadout::innate(),
                SeedCopy::default(),
            );
            let text = text_of(&g);
            for m in all_on.active() {
                let caption = match m.detail {
                    Some(d) => format!("{}: {}", m.name, d),
                    None => m.name.to_string(),
                };
                assert!(
                    text.contains(&caption),
                    "caption {caption:?} was clipped on a {W}-wide board",
                );
            }
        }
        // And the bound itself is not vacuously large: a caption may not run past
        // the board's last column once indented.
        for m in CAPTIONS {
            assert!(
                CONTENT_INDENT as usize + m.caption_len() < W as usize,
                "{:?} does not fit the board with its indent",
                m.name,
            );
        }
    }

    /// The **level-seed token** on the Level info tab (§13.1/#245/#272): the run's
    /// own token, drawn under its heading — and it **decodes back to the very run
    /// showing it**, config and all, so the panel can never hand out a string that
    /// boots a different game. There is one form now (#333), so what the player
    /// reads here is character-for-character what a shared link carries: the panel
    /// and the address bar can no longer disagree about what the run is.
    #[test]
    fn the_level_info_tab_shows_a_token_that_decodes_to_this_run() {
        for level in [
            // The default preset.
            LevelSeed::quick_play(8371),
            // A run carrying a chosen modifier set and loadout.
            LevelSeed {
                seed: 8371,
                modifiers: LevelModifiers {
                    always_show_vision_cones: true,
                    ..LevelModifiers::default()
                },
                abilities: Loadout::innate(),
            },
        ] {
            let g = render_help(
                W,
                H,
                HelpTab::LevelInfo,
                Some(level),
                level.modifiers,
                &quiet_alert(),
                Loadout::innate(),
                SeedCopy::default(),
            );
            let text = text_of(&g);
            let token = level.encode().expect("a config a run can hold");
            assert!(text.contains("LEVEL SEED"), "the section is labelled");
            assert!(text.contains(&token), "the token is shown: {text:?}");
            // The round trip: what a player reads off the panel boots this run.
            assert_eq!(
                LevelSeed::decode(&token),
                Some(level),
                "the displayed token reproduces the run exactly"
            );
        }

        // The default preset is spelled out like any other config — it no longer
        // collapses to a bare seed, which named the preset rather than the run
        // (#333, superseding #328). This assertion is the reverse of the pin that
        // recorded the old decision, and is here to record the new one.
        let quick = LevelSeed::quick_play(8371);
        let token = quick.encode().expect("a config a run can hold");
        assert_ne!(token, "8371", "the link form is no longer a bare seed");
        assert_eq!(
            token.len(),
            crate::level_seed::TOKEN_LEN,
            "one fixed-width form"
        );
        let g = render_help(
            W,
            H,
            HelpTab::LevelInfo,
            Some(quick),
            quick.modifiers,
            &quiet_alert(),
            Loadout::innate(),
            SeedCopy::default(),
        );
        assert!(
            text_of(&g).contains(&token),
            "quick play shows the same token it shares",
        );

        // A hand-built state has no reproducible token, so the section is absent
        // rather than showing a string that boots something else.
        let none = render_help(
            W,
            H,
            HelpTab::LevelInfo,
            None,
            LevelModifiers::default(),
            &quiet_alert(),
            Loadout::innate(),
            SeedCopy::default(),
        );
        assert!(!text_of(&none).contains("LEVEL SEED"));
    }

    /// #375/§2.2: the Level info tab carries the **facility alert** — the rung, and the
    /// retaliation it has in force. Without it the ladder is perceptible for exactly one
    /// turn (the near line's step message, overwritten by anything louder, §11.7) and
    /// inert after that.
    ///
    /// The section is drawn at **every** rung, rung 0 included: a heading that appeared
    /// out of nowhere the turn you were first seen would teach the ladder exists at the
    /// moment that knowledge stopped being useful, and a row that vanishes reads as a
    /// bug rather than as a fact.
    #[test]
    fn the_level_info_tab_shows_the_alert_rung_and_what_it_is_doing() {
        let panel = |alert: &AlertReadout| {
            text_of(&render_help(
                W,
                H,
                HelpTab::LevelInfo,
                None,
                LevelModifiers::default(),
                alert,
                Loadout::innate(),
                SeedCopy::default(),
            ))
        };

        let quiet = panel(&quiet_alert());
        assert!(quiet.contains("ALERT"), "the section is always there");
        assert!(
            quiet.contains(crate::alert::NO_ALERT),
            "a quiet facility says so rather than showing a blank: {quiet}",
        );
        assert!(
            !quiet.contains("Condition"),
            "…and claims no condition it has not reached",
        );

        // A raised rung names itself and lists the effects the ladder actually runs —
        // the numbers included, so "never calm" is a rule the player can plan against
        // rather than a mood.
        let raised = panel(&AlertReadout {
            rung: 2,
            effects: vec![AlertEffect {
                rung: 1,
                name: crate::alert::NEVER_CALM,
                detail: Some("pause 1–3 turns".to_string()),
            }],
        });
        assert!(raised.contains("Condition 2 of 3"), "{raised}");
        assert!(
            raised.contains("Guards never calm: pause 1–3 turns"),
            "{raised}"
        );
        assert!(
            !raised.contains(crate::alert::NO_ALERT),
            "and never both at once",
        );
    }

    /// §11.4's row-fits rule (#248's `CAPTION_MAX`, applied to the alert rows): every
    /// row the ALERT section can draw fits the v1 board's content column. [`draw`] clips
    /// in silence, and the effect rows carry **runtime** numbers off the live
    /// [`AlertTuning`] — so unlike the modifier captions they cannot be bounded at
    /// compile time, and this walks the real ladder instead of trusting them.
    ///
    /// Walked over a **deliberately wide** tuning as well as the shipped one: two-digit
    /// dwell numbers are legal (`validate` allows them) and are exactly what a §13.2
    /// sweep would produce, so the row has to fit at the widest the ladder permits, not
    /// only at its [START].
    #[test]
    fn no_row_of_the_alert_section_is_clipped() {
        let room = column_width(CONTENT_INDENT);
        assert!(
            crate::alert::NO_ALERT.chars().count() <= room,
            "the rung-0 row is too wide for the Level info column",
        );
        for tuning in [
            AlertTuning::default(),
            AlertTuning {
                dwell_turns_min: 98,
                dwell_turns_max: 99,
                rung_two_reinforcements: 98,
                rung_three_reinforcements: 99,
                ..AlertTuning::default()
            },
        ] {
            let mut alert = crate::alert::Alert::new();
            alert.set_tuning(tuning);
            for trigger in AlertTrigger::ALL {
                alert.raise(trigger);
                let readout = alert.readout();
                // Read from the drawing's own helper, so the row measured here is the
                // row the panel draws.
                let condition = super::super::alert::condition_line(readout.rung);
                assert!(condition.chars().count() <= room, "{condition:?}");
                for effect in &readout.effects {
                    let text = match &effect.detail {
                        Some(detail) => format!("{}{CAPTION_SEPARATOR}{detail}", effect.name),
                        None => effect.name.to_string(),
                    };
                    assert!(
                        text.chars().count() <= room,
                        "the alert row {text:?} is {} cells and its column has {room}",
                        text.chars().count(),
                    );
                }
            }
        }
    }

    /// The seed section does not disturb the modifier list it heads (#272): with a
    /// token shown, the run's modifiers still render by name in their cue colour,
    /// just two rows lower.
    #[test]
    fn the_seed_section_shifts_the_modifier_list_without_changing_it() {
        let level = LevelSeed {
            seed: 8371,
            modifiers: LevelModifiers {
                guards_always_search_hideouts: true,
                ..LevelModifiers::default()
            },
            abilities: Loadout::innate(),
        };
        let g = render_help(
            W,
            H,
            HelpTab::LevelInfo,
            Some(level),
            level.modifiers,
            &quiet_alert(),
            Loadout::innate(),
            SeedCopy::default(),
        );
        let text = text_of(&g);
        assert!(text.contains("Guards search hideouts"));
        assert!(!text.contains("none active"));
        // THIS RUN@2, LEVEL SEED@4, the token@5, MODIFIERS@7, the first row@8.
        let token = level.encode().expect("a config a run can hold");
        assert_eq!(
            g.get(3, 5).glyph,
            token.chars().next().expect("a token has letters"),
            "the token sits under its heading",
        );
        assert_eq!(g.get(3, 5).fg, Category::Interest);
        assert_eq!(g.get(3, 8).glyph, 'G');
        assert_eq!(
            g.get(3, 8).fg,
            Category::Warning,
            "the caption keeps its direction cue"
        );
    }

    /// The modifier's **caption** is drawn in its direction's cue colour (§11.2/#248):
    /// Warning for a harder rule, Owned for an easier one — so the direction reads at
    /// a glance, and the colours come from the standing categories, not ad-hoc styling.
    #[test]
    fn the_caption_reads_in_its_direction_cue_colour() {
        // A harder toggle: its caption `Guards search hideouts` is drawn in Warning.
        let harder = LevelModifiers {
            guards_always_search_hideouts: true,
            ..LevelModifiers::default()
        };
        let g = render_help(
            W,
            H,
            HelpTab::LevelInfo,
            None,
            harder,
            &quiet_alert(),
            Loadout::innate(),
            SeedCopy::default(),
        );
        // The MODIFIERS heading is at row 4 (THIS RUN@2, blank, heading@4), the first
        // modifier row at row 5; its caption starts at column 3.
        assert_eq!(g.get(3, 5).glyph, 'G');
        assert_eq!(
            g.get(3, 5).fg,
            Category::Warning,
            "a harder caption cues in Warning orange"
        );

        // An easier toggle's caption `All vision cones shown` cues in Owned.
        let easier = LevelModifiers {
            always_show_vision_cones: true,
            ..LevelModifiers::default()
        };
        let g = render_help(
            W,
            H,
            HelpTab::LevelInfo,
            None,
            easier,
            &quiet_alert(),
            Loadout::innate(),
            SeedCopy::default(),
        );
        assert_eq!(g.get(3, 5).glyph, 'A');
        assert_eq!(
            g.get(3, 5).fg,
            Category::Owned,
            "an easier caption cues in Owned blue"
        );
    }

    /// The tab bar shows both tabs, and the active one reads in Interest while the
    /// rest are dim Ground — the at-a-glance "you are here" (#248), asserted on the
    /// cell colour since a text render loses it.
    #[test]
    fn the_tab_bar_highlights_the_active_tab() {
        let layout = tab_layout();
        for &active in &HelpTab::ALL {
            let g = render_help(
                W,
                H,
                active,
                None,
                LevelModifiers::default(),
                &quiet_alert(),
                Loadout::innate(),
                SeedCopy::default(),
            );
            for &(tab, start, _len) in &layout {
                let expected = if tab == active {
                    Category::Interest
                } else {
                    Category::Ground
                };
                // The `[` at the tab's start carries its colour.
                assert_eq!(g.get(start, 0).glyph, '[', "{tab:?} draws its bracket");
                assert_eq!(
                    g.get(start, 0).fg,
                    expected,
                    "with {active:?} active, {tab:?} reads {expected:?}"
                );
            }
        }
    }

    /// A hit-test on a full-height panel showing the [`Default`] tab of a run with
    /// no level-seed token — the shape the tab-bar and footer tests want, where the
    /// only thing that varies is where the finger landed.
    fn hit(x: u32, y: u32) -> Option<HelpHit> {
        hit_on(H, x, y)
    }

    /// The same, on a panel `height` rows tall — for the footer, which is drawn from
    /// the bottom edge and so must be hit-tested from it too.
    fn hit_on(height: u32, x: u32, y: u32) -> Option<HelpHit> {
        help_hit(W, height, HelpTab::default(), None, x, y)
    }

    /// The panel is escapable and switchable **by touch** (§11.6/#248): the `[x]`
    /// close control hit-tests to [`HelpHit::Close`], each tab's cells to
    /// [`HelpHit::Tab`], and the body to nothing (a press the modal panel swallows).
    #[test]
    fn the_panel_is_escapable_and_switchable_by_touch() {
        // The close control at the right edge → Close, and nothing just left of it.
        let close = close_button_start(W);
        assert_eq!(hit(close, 0), Some(HelpHit::Close));
        assert_eq!(hit(close + 1, 0), Some(HelpHit::Close));
        assert_ne!(hit(close - 1, 0), Some(HelpHit::Close));

        // Each tab's whole `[Label]` region resolves to that tab, by identity.
        for (tab, start, len) in tab_layout() {
            for x in start..start + len {
                assert_eq!(hit(x, 0), Some(HelpHit::Tab(tab)), "tab cell {x}");
            }
        }
        // The body (below the tab bar) and the gap left of the first tab are inert.
        assert_eq!(hit(5, 3), None, "the body swallows presses");
        assert_eq!(hit(0, 0), None, "the left margin is not a tab");
    }

    /// §11.6/#189: the theme is reachable **by touch**, not just by key — the
    /// footer's `theme [n]` hit-tests to [`HelpHit::ToggleTheme`] over exactly the
    /// cells it is drawn on, and the rest of the footer row is inert like the body. A
    /// phone has no `n` key, so without this the light theme would be a desktop-only
    /// option on a game that fits its whole board to a phone screen.
    ///
    /// **The word is a target too, not only the bracketed key.** `theme` is the
    /// larger and more obvious thing to reach for, and a bare `[n]` is only a target
    /// if you already know what `n` means — so every cell of the label presses the
    /// control, which is what this walks.
    #[test]
    fn the_theme_control_is_reachable_by_touch() {
        let start = theme_control_start(W);
        for x in start..start + theme_control_len() {
            assert_eq!(hit(x, H - 1), Some(HelpHit::ToggleTheme), "footer cell {x}");
        }
        // The label's own first cell — the regression this guards is a control that
        // only answered on its last three cells.
        let label_end = start + THEME_LABEL.chars().count() as u32;
        assert_eq!(hit(start, H - 1), Some(HelpHit::ToggleTheme));
        assert_eq!(hit(label_end - 1, H - 1), Some(HelpHit::ToggleTheme));

        assert_eq!(hit(start - 1, H - 1), None, "the hint is inert");
        assert_eq!(hit(start, H - 2), None, "only the footer row");
        // Measured from the *bottom* edge, so a shorter screen moves it with the
        // drawing rather than leaving the hit region a row adrift.
        assert_eq!(hit_on(20, start, 19), Some(HelpHit::ToggleTheme));
        assert_eq!(hit_on(20, start, H - 1), None);
    }

    /// A run whose panel really does draw a token — quick play, spelled out in full
    /// (#333) — for the copy-control tests below.
    fn run_with_a_token() -> LevelSeed {
        LevelSeed::quick_play(8371)
    }

    /// The Level info panel for `level`, with the seed-copy acknowledgement in
    /// `copy` — the frame the copy control is drawn on.
    fn level_info(level: Option<LevelSeed>, copy: SeedCopy) -> Grid {
        render_help(
            W,
            H,
            HelpTab::LevelInfo,
            level,
            LevelModifiers::default(),
            &quiet_alert(),
            Loadout::innate(),
            copy,
        )
    }

    /// **The token is takeable** (§13.1/#353): the Level info tab draws a `copy [c]`
    /// control on the token's own row, and every cell of it — the word as much as the
    /// bracketed key, for the reason `theme [n]` gives — hit-tests to
    /// [`HelpHit::CopySeed`].
    ///
    /// The rows around it stay inert, which is the point of testing the neighbours as
    /// well as the control: the token row is in the middle of the panel body, where
    /// every other press is swallowed, so a copy control with a sloppy region would
    /// start eating presses that used to mean nothing.
    #[test]
    fn the_token_row_carries_a_copy_control_and_its_neighbours_do_not() {
        let level = Some(run_with_a_token());
        let hit = |x, y| help_hit(W, H, HelpTab::LevelInfo, level, x, y);

        let start = copy_control_start(W);
        for x in start..start + copy_control_len() {
            assert_eq!(hit(x, SEED_TOKEN_ROW), Some(HelpHit::CopySeed), "cell {x}");
        }
        // The label's own first cell, and the last cell of the word before the key —
        // the whole `copy` run presses, not just the `[c]`.
        let label_end = start + COPY_LABEL.chars().count() as u32;
        assert_eq!(hit(start, SEED_TOKEN_ROW), Some(HelpHit::CopySeed));
        assert_eq!(hit(label_end - 1, SEED_TOKEN_ROW), Some(HelpHit::CopySeed));

        // Just left of it, and the rows above and below: the heading, the token's own
        // letters, and the acknowledgement line all stay body.
        assert_eq!(hit(start - 1, SEED_TOKEN_ROW), None, "the gap is inert");
        assert_eq!(
            hit(CONTENT_INDENT, SEED_TOKEN_ROW),
            None,
            "the token itself"
        );
        assert_eq!(
            hit(start, SEED_TOKEN_ROW - 1),
            None,
            "the LEVEL SEED heading"
        );
        assert_eq!(hit(start, SEED_TOKEN_ROW + 1), None, "the row beneath");
    }

    /// **No token, no control** (#353): a hand-built state has nothing that
    /// reproduces it, so the panel shows no seed section — and the row where the
    /// control would have been resolves to nothing at all rather than to a button
    /// that copies an empty string.
    #[test]
    fn a_run_with_no_token_offers_nothing_to_copy() {
        let start = copy_control_start(W);
        for x in start..start + copy_control_len() {
            assert_eq!(
                help_hit(W, H, HelpTab::LevelInfo, None, x, SEED_TOKEN_ROW),
                None,
                "cell {x} of a panel with no token",
            );
        }
        assert!(
            !text_of(&level_info(None, SeedCopy::Idle)).contains(&copy_control()),
            "and nothing is drawn there either",
        );
    }

    /// The control belongs to the **Level info** tab, because the token does: the very
    /// same cells on the other tabs are body, whatever run is playing. Otherwise a tap
    /// meant for a line of the Abilities card would copy a seed.
    #[test]
    fn the_copy_control_is_the_level_info_tabs_alone() {
        let level = Some(run_with_a_token());
        let start = copy_control_start(W);
        for tab in HelpTab::ALL {
            if tab == HelpTab::LevelInfo {
                continue;
            }
            assert_eq!(
                help_hit(W, H, tab, level, start, SEED_TOKEN_ROW),
                None,
                "{tab:?} has no token on it",
            );
        }
    }

    /// The control is drawn where it is hit-tested, in the HUD-control colour the
    /// `[x]` and the theme button wear (#353) — and it clears the token beside it, a
    /// thing [`draw`] would otherwise resolve by silently overwriting the end of the
    /// one string on this panel that has to be read character for character.
    #[test]
    fn the_copy_control_is_drawn_clear_of_the_token() {
        let level = run_with_a_token();
        let token = level.encode().expect("a config a run can hold");
        let g = level_info(Some(level), SeedCopy::Idle);

        let start = copy_control_start(W);
        let drawn: String = (start..start + copy_control_len())
            .map(|x| g.get(x, SEED_TOKEN_ROW).glyph)
            .collect();
        assert_eq!(
            drawn,
            copy_control(),
            "the control is drawn on the token row"
        );
        assert_eq!(g.get(start, SEED_TOKEN_ROW).fg, Category::System);

        // The token still reads whole, and the two do not touch.
        let token_end = CONTENT_INDENT + token.chars().count() as u32;
        assert!(
            token_end < start,
            "the token ({token_end} cells in) runs into the copy control at {start}",
        );
        assert_eq!(g.get(token_end - 1, SEED_TOKEN_ROW).fg, Category::Interest);
    }

    /// The acknowledgement (#353) is **honest and quiet**: nothing before a press,
    /// a plain "copied" after one, and a failure that says the clipboard did *not*
    /// take it rather than claiming it did. It lands in the blank spacer the token
    /// already had beneath it, so saying so shifts no row of the modifier list below.
    #[test]
    fn the_copy_acknowledgement_says_only_what_happened() {
        let level = Some(run_with_a_token());
        let ack_row = SEED_TOKEN_ROW + 1;
        let row_text = |g: &Grid, y: u32| -> String { g.to_text()[y as usize].clone() };

        let idle = level_info(level, SeedCopy::Idle);
        assert!(
            row_text(&idle, ack_row).trim().is_empty(),
            "nothing is claimed before anything is pressed",
        );

        let copied = level_info(level, SeedCopy::Copied);
        assert!(row_text(&copied, ack_row).contains(COPIED_ACK));
        assert_eq!(copied.get(CONTENT_INDENT, ack_row).fg, Category::System);

        let failed = level_info(level, SeedCopy::Unavailable);
        let text = row_text(&failed, ack_row);
        assert!(text.contains(UNAVAILABLE_ACK));
        assert!(
            !text.contains(COPIED_ACK) && !text.contains("copied"),
            "a failure never reads as a copy: {text:?}",
        );
        assert_eq!(failed.get(CONTENT_INDENT, ack_row).fg, Category::Warning);

        // Whatever it says, the token is still printed and the list below it has not
        // moved — the failure path degrades to exactly the panel that existed before
        // this control did.
        let token = run_with_a_token().encode().expect("a token");
        for copy in [SeedCopy::Idle, SeedCopy::Copied, SeedCopy::Unavailable] {
            let g = level_info(level, copy);
            assert!(text_of(&g).contains(&token), "the token stays readable");
            assert!(row_text(&g, ack_row + 1).contains("MODIFIERS"));
        }
    }

    /// The panel's key and its control name the same character (#353), the way `n`
    /// and `theme [n]` do — so the `[c]` the player reads is the `c` they press. It is
    /// deliberately **not** a board key: there is no token drawn outside this panel
    /// for it to copy, which is also what leaves the letter free for an ability
    /// mnemonic (#360).
    #[test]
    fn the_copy_key_is_the_same_on_the_control_and_in_the_table() {
        let key = COPY_KEY.to_string();
        assert_eq!(
            crate::help_nav_for_key(&key),
            Some(crate::input::HelpNav::CopySeed),
        );
        assert!(copy_control().ends_with(&format!("[{key}]")));
        assert_eq!(
            crate::input::ui_command_for_key(&key),
            None,
            "panel-only: the board has no token to copy",
        );
        assert_eq!(
            crate::input_for_key(&key),
            None,
            "and it shadows no movement"
        );
    }

    /// The footer's hint and its theme control share one row, so the prose must stop
    /// before the control starts — [`draw`] would clip the control in silence, and a
    /// half-drawn control is one the player cannot see they can press.
    #[test]
    fn the_footer_hint_stops_short_of_the_theme_control() {
        let end = FOOTER_INDENT + FOOTER_HINT.chars().count() as u32;
        assert!(
            end < theme_control_start(W),
            "the footer hint runs into the theme control ({end} vs {})",
            theme_control_start(W),
        );
        // And the control itself clears the board's right margin.
        assert_eq!(theme_control(), format!("{THEME_LABEL} [{THEME_KEY}]"));
        assert!(theme_control_start(W) + theme_control_len() <= W);
    }

    /// The card and the key tables cannot drift (#189): the controls row, the footer
    /// button and both bindings all name the same character. The theme is the one
    /// key the open panel *forwards* rather than swallows, because the panel is
    /// where the option lives — so pressing it on the card actually does something.
    #[test]
    fn the_theme_key_is_the_same_on_the_card_and_in_both_tables() {
        let key = THEME_KEY.to_string();
        assert_eq!(
            crate::input::ui_command_for_key(&key),
            Some(crate::input::UiCommand::ToggleTheme),
        );
        assert_eq!(
            crate::help_nav_for_key(&key),
            Some(crate::input::HelpNav::ToggleTheme),
            "the modal panel forwards its own option's key",
        );
        assert!(theme_control().ends_with(&format!("[{key}]")));
        // It shadows nothing: not a movement key, and not an ability key — those are
        // the bar's four digits now (§11.6/#359), so a letter cannot collide with one.
        assert_eq!(crate::input_for_key(&key), None);
        assert_eq!(
            crate::ability_slot_for_code(&format!("Key{}", key.to_uppercase())),
            None
        );
    }

    /// The tabs cycle, wrapping at both ends (§14 v2/#248) — the Tab / arrow motion.
    /// Written over [`HelpTab::ALL`] rather than naming pairs, so adding a tab (as
    /// #343 did) extends the cycle instead of breaking the test.
    #[test]
    fn the_tabs_cycle_both_ways() {
        assert_eq!(HelpTab::LevelInfo.next(), HelpTab::Abilities);
        for (i, tab) in HelpTab::ALL.into_iter().enumerate() {
            let after = HelpTab::ALL[(i + 1) % HelpTab::ALL.len()];
            assert_eq!(tab.next(), after, "{tab:?} advances, wrapping at the end");
            assert_eq!(after.prev(), tab, "…and steps back the same way");
        }
    }

    /// `ActiveModifier` is re-exported for shells and tests to read the descriptor
    /// directly, not only through the rendered card — a light guard that the type
    /// stays public and constructible.
    #[test]
    fn the_active_modifier_descriptor_is_readable() {
        let m = ActiveModifier {
            name: "x",
            direction: ModifierDirection::Harder,
            detail: None,
        };
        assert_eq!(m.direction, ModifierDirection::Harder);
    }
}
