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
//!   the active [`LevelModifiers`], by name and direction (§12.6).
//! - **Legend** ([`HelpTab::Legend`]) — the glyph legend, the colour key, and the
//!   controls, the original reference card (#139).
//! - *Options* land as a third tab (§14 v2 "options"; #189 light mode, #237
//!   difficulty).
//!
//! **Every row derives from the real source**, never a hand-copied table that
//! could drift from the game it documents (§11.2/§11.3/§11.6): terrain glyphs and
//! their categories come from [`Terrain::glyph`]/[`Terrain::category`], the entity
//! glyphs from the [`super`] render constants the world draws with, the colour
//! meanings from an exhaustive match over [`Category`], the ability keys from
//! [`AbilityId`]'s settled §11.6 hotkeys, and the modifier rows from
//! [`LevelModifiers::active`] — so a newly added modifier appears here on its own.
//! The tests assert each derivation.
//!
//! Opening and closing the panel is a pure **view** action owned by the shell
//! ([`ScreenUi::help_open`](super::ScreenUi)): it changes no world and costs no
//! turn (§4.4), so no guard moves while it is up. Unlike the old map-only overlay,
//! the panel is **modal and full-screen** (#248): while it is up it takes the whole
//! screen and the shell routes input to it — keys through
//! [`help_nav_for_key`](crate::help_nav_for_key), taps through [`help_hit`] — so
//! the game never steps underneath. It stays escapable (§11.6's no-trap rule): `?`
//! or `Escape` closes it, and the tab bar carries a touchable `[x]`.

use super::{blank_grid, draw, Grid, BODY_GLYPH, FLOOR_DOT, GUARD_GLYPH, PLAYER_GLYPH};
use crate::ability::{AbilityId, PASSIVE_MARKER};
use crate::category::Category;
use crate::facility::Terrain;
use crate::level_seed::LevelSeed;
use crate::modifiers::{LevelModifiers, ModifierDirection, CAPTIONS, CAPTION_SEPARATOR};
use crate::place::LevelConfig;

/// The key that toggles the help panel (§11.6). A free letter — not a movement
/// key, an ability hotkey, or another UI control — and the conventional roguelike
/// help key. Shown in the controls list and matched in
/// [`ui_command_for_key`](crate::input::ui_command_for_key) (to open) and
/// [`help_nav_for_key`](crate::help_nav_for_key) (to close).
pub(crate) const HELP_KEY: char = '?';

/// The tabs the help panel pages between (§14 v2/#248). The panel opens on the
/// leftmost ([`Default`]) and the tab bar switches between them; a shell keeps the
/// current tab on [`ScreenUi`](super::ScreenUi) and hands it to [`render_help`].
///
/// Ordered as the player reads them left to right — *this run* first, the standing
/// reference second — and cycled by [`next`](Self::next)/[`prev`](Self::prev) so
/// the tab bar wraps at either end. A third *Options* tab slots in here.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HelpTab {
    /// This run's active level modifiers (§12.6/#248) — what is bending the rules.
    #[default]
    LevelInfo,
    /// The glyph legend, colour key, and controls (#139) — the reference card.
    Legend,
}

impl HelpTab {
    /// Every tab, in reading (left-to-right) order — the tab bar's layout and the
    /// cycle order. A new tab is one entry here.
    pub const ALL: [HelpTab; 2] = [HelpTab::LevelInfo, HelpTab::Legend];

    /// The label shown on the tab bar and used to size its hit region.
    fn label(self) -> &'static str {
        match self {
            HelpTab::LevelInfo => "Level info",
            HelpTab::Legend => "Legend",
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
}

/// The `[x]` close control on the tab bar — three cells wide, like the header's
/// other `[?]`/`[▾]` buttons, so the escape reads as a button.
const CLOSE_BUTTON: &str = "[x]";
const CLOSE_BUTTON_LEN: u32 = 3;

/// The column every Level info row is drawn from — one in from the section
/// headings, the panel's standing content indent.
const CONTENT_INDENT: u32 = 3;

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

/// The column the close control starts at: right-aligned with a one-cell margin,
/// like the ability line's deploy button. Shared by the drawing and [`help_hit`]
/// so a tap lands on exactly the `[x]` drawn.
fn close_button_start(width: u32) -> u32 {
    width.saturating_sub(1 + CLOSE_BUTTON_LEN)
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

/// The pointer→control hit-test for the open panel (§11.6/#248): which [`HelpHit`]
/// screen cell `(x, y)` lands on, or `None` for the body (a press the modal panel
/// swallows without acting). Only the tab-bar row (row 0) carries controls; the
/// close `[x]` is tested first so it wins even if a layout ever abutted it.
#[must_use]
pub fn help_hit(width: u32, x: u32, y: u32) -> Option<HelpHit> {
    if y != 0 {
        return None;
    }
    let close = close_button_start(width);
    if x >= close && x < close + CLOSE_BUTTON_LEN {
        return Some(HelpHit::Close);
    }
    for (tab, start, len) in tab_layout() {
        if x >= start && x < start + len {
            return Some(HelpHit::Tab(tab));
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
/// and stops.
pub(super) fn render_help(
    width: u32,
    height: u32,
    tab: HelpTab,
    level: Option<LevelSeed>,
    modifiers: LevelModifiers,
) -> Grid {
    let mut grid = blank_grid(width, height);

    draw_tab_bar(&mut grid, tab);
    // Content begins two rows down, leaving the tab bar and a blank rule above it.
    match tab {
        HelpTab::LevelInfo => draw_level_info(&mut grid, 2, level, modifiers),
        HelpTab::Legend => draw_legend(&mut grid, 2),
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

/// The **Level info** tab (§12.6/#248/#272): the run's **level-seed string** in
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
) {
    draw(grid, 2, y, "THIS RUN", Category::Interest);
    y += 2;

    // The level-seed string (§13.1/#245): the handle that hands this run around.
    // A hand-built state has none — it was assembled cell by cell, and there is no
    // token that reproduces it — so the section is simply absent rather than
    // showing an honest-looking string that boots something else.
    if let Some(level) = level {
        draw(grid, 2, y, "LEVEL SEED", Category::System);
        y += 1;
        // Interest, the goal/reward colour: this is the thing worth taking away
        // from the panel. The **full** form ([`LevelSeed::encode_full`]), even for
        // the default preset whose link form is the bare seed: this surface exists
        // to show what the run *is*, and `8371` alone says nothing about the
        // modifiers and loadout it implies. It decodes to the same run either way.
        draw(grid, 3, y, &level.encode_full(), Category::Interest);
        y += 2;
    }

    draw(grid, 2, y, "MODIFIERS", Category::System);
    y += 1;

    let active = modifiers.active();
    if active.is_empty() {
        // Baseline quick play: legible as "none active", not blank or absent (#248).
        draw(grid, 3, y, "none active — baseline rules", Category::Ground);
        return;
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

/// The **Legend** tab (#139): the glyph legend, the colour key, and the controls —
/// the original reference card, now one tab of the panel.
fn draw_legend(grid: &mut Grid, mut y: u32) {
    draw(grid, 2, y, "GLYPHS", Category::System);
    y += 1;
    for (glyph, category, meaning) in glyph_rows() {
        draw(grid, 3, y, &glyph.to_string(), category);
        draw(grid, 6, y, meaning, Category::Neutral);
        y += 1;
    }
    y += 1;

    draw(grid, 2, y, "COLOURS", Category::System);
    y += 1;
    for category in CATEGORIES {
        // The name is drawn *in its own colour*, so the player reads the colour and
        // its meaning on one line.
        draw(grid, 3, y, category_name(category), category);
        draw(grid, 14, y, category_meaning(category), Category::Neutral);
        y += 1;
    }
    y += 1;

    draw(grid, 2, y, "CONTROLS", Category::System);
    y += 1;
    for (keys, action) in control_rows() {
        draw(grid, 3, y, &keys, Category::System);
        draw(grid, 26, y, &action, Category::Neutral);
        y += 1;
    }
}

/// Draw the footer hint on the last row: how to switch tabs and close, so a player
/// who opened the modal panel always sees the way out (§11.6's no-trap rule, made
/// explicit now the header `[?]` is covered).
fn draw_footer(grid: &mut Grid) {
    if grid.height == 0 {
        return;
    }
    draw(
        grid,
        2,
        grid.height - 1,
        "Tab switches   Esc or [?] closes",
        Category::Ground,
    );
}

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
fn category_meaning(category: Category) -> &'static str {
    match category {
        Category::Neutral => "inert scenery",
        Category::Ground => "floor you can cross",
        Category::Owned => "you and your things",
        Category::Caution => "an unaware threat",
        Category::Warning => "a hunting threat",
        Category::Danger => "you're in its cone",
        Category::Interest => "a goal or reward",
        Category::System => "door / cupboard / duct",
        Category::Sensed => "guard or door, felt through a wall",
        Category::Effect => "your gadget's reach, and what it holds",
    }
}

/// The controls (§11.6), each `(keys, action)`. Movement and wait are the fixed
/// rows; the **ability** rows derive their keys from [`AbilityId`]'s settled hotkeys,
/// so an ability's key on this card is exactly the key that activates it; the UI keys
/// close the card and drive the panels.
/// **This card is now the only place hotkeys are written down** (#287): the ability
/// bar spends its width on names instead of letters, so a player who wants to know
/// which key fires what comes here. Which makes it also where the bar's short names
/// are explained — an ability's left column is `<key> / <bar name>`, exactly the two
/// things a player is holding in their head ("the `Camo` down there, what fires it?"),
/// and the right column is the full §8.3 name the near line's messages speak.
///
/// A **passive** (#264) shows its bar entry and no key: it has nothing to press, and
/// advertising its identity letter would promise an action that does nothing — but
/// it *is* an entry on the bar, so a card that omitted it would leave the one entry
/// a player cannot act on as the one entry nothing explains.
fn control_rows() -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = vec![
        ("arrows / hjkl / 8246".to_string(), "move".to_string()),
        ("w / 5 / .".to_string(), "wait & sense".to_string()),
    ];
    for id in AbilityId::ALL {
        rows.push((ability_row_keys(id), id.name().to_string()));
    }
    rows.push(("m".to_string(), "messages".to_string()));
    rows.push((HELP_KEY.to_string(), "this help".to_string()));
    rows
}

/// An ability's left column on the controls card (#287): the key that fires it and
/// the [bar name](AbilityId::bar_name) it appears under, so the two are read as one
/// fact. A **passive** has no key, so it shows its bar entry as the bar draws it —
/// `Sight (on)` — which is the thing on screen the row is there to explain.
fn ability_row_keys(id: AbilityId) -> String {
    if id.is_passive() {
        format!("{} {PASSIVE_MARKER}", id.bar_name())
    } else {
        format!("{} / {}", id.hotkey(), id.bar_name())
    }
}

/// The category's display name for the colour key — its own identifier, so the key
/// names exactly the [`Category`] the renderer tags cells with.
fn category_name(category: Category) -> &'static str {
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
    use crate::ability::Loadout;
    use crate::modifiers::{ActiveModifier, IntelGate};

    /// A full-screen frame the size of the v1 board's screen (§10.2) — wide enough
    /// that no row truncates, so a test can read the panel's content whole.
    const W: u32 = 40;
    const H: u32 = 43; // TOP_ROWS + 40 + BOTTOM_ROWS

    fn text_of(grid: &Grid) -> String {
        grid.to_text().join("\n")
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

    /// The ability control rows carry each **activated** ability's settled §11.6
    /// hotkey, straight from [`AbilityId`] — so the card's keys are the keys that
    /// actually activate them, and cannot drift. Since the bar stopped showing
    /// letters (#287) this card is the whole key reference, so *every* ability is
    /// listed: an activated one under its key, a **passive** under its always-on
    /// marker, because it has no key to press but is still an entry on the bar.
    #[test]
    fn the_control_rows_carry_the_real_ability_hotkeys() {
        let rows = control_rows();
        for id in AbilityId::ALL {
            let keys = ability_row_keys(id);
            assert!(
                rows.iter().any(|(k, a)| *k == keys && a == id.name()),
                "the controls must list {} under {keys:?}",
                id.name(),
            );
        }
        // The help key documents itself.
        assert!(rows.iter().any(|(k, _)| *k == HELP_KEY.to_string()));
    }

    /// The card joins the bar's short name to the key that fires it and to the full
    /// name the messages speak (#287) — the whole reason it is safe for the bar to
    /// show neither the letter nor the long name. A passive shows its bar entry and
    /// no key, because there is none.
    #[test]
    fn the_control_rows_explain_the_bar_names() {
        assert_eq!(ability_row_keys(AbilityId::Camouflage), "c / Camo");
        assert_eq!(ability_row_keys(AbilityId::Run), "r / Run");
        assert_eq!(ability_row_keys(AbilityId::Vision), "Sight (on)");
        let text = text_of(&render_help(
            W,
            H,
            HelpTab::Legend,
            None,
            LevelModifiers::default(),
        ));
        for (keys, name) in [
            ("c / Camo", "Camouflage"),
            ("Sight (on)", "Vision"),
            ("a / Doors", "Autodoors"),
        ] {
            assert!(text.contains(keys), "the card shows {keys:?}");
            assert!(text.contains(name), "…against {name:?}");
        }
    }

    /// The **Legend** tab still carries the whole reference card — the three
    /// sections and a glyph derived from the real terrain table (the duct `=`, §10.7).
    #[test]
    fn the_legend_tab_carries_the_glyphs_colours_and_controls() {
        let g = render_help(W, H, HelpTab::Legend, None, LevelModifiers::default());
        let text = text_of(&g);
        assert!(text.contains("GLYPHS") && text.contains("COLOURS") && text.contains("CONTROLS"));
        for glyph in [Terrain::DuctEntry.glyph(), Terrain::Exit.glyph(), '}', '$'] {
            assert!(text.contains(glyph), "the legend shows {glyph:?}");
        }
        for id in AbilityId::ALL {
            assert!(
                text.contains(&ability_row_keys(id)),
                "the controls show {}",
                id.name()
            );
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
        let baseline = render_help(W, H, HelpTab::LevelInfo, None, LevelModifiers::default());
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
        let g = render_help(W, H, HelpTab::LevelInfo, None, modified);
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
                intel_to_exit: gate,
            };
            let g = render_help(W, H, HelpTab::LevelInfo, None, all_on);
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

    /// The **level-seed string** on the Level info tab (§13.1/#245/#272): the run's
    /// own token, drawn under its heading — and it **decodes back to the very run
    /// showing it**, config and all, so the panel can never hand out a string that
    /// boots a different game. The panel always shows the **full** form, including
    /// for the default preset whose link form is a bare seed: this surface is where
    /// you read what the run *is*, so it spells the initial situation out.
    #[test]
    fn the_level_info_tab_shows_a_token_that_decodes_to_this_run() {
        for level in [
            // The default preset: a bare decimal seed.
            LevelSeed::quick_play(8371),
            // A run carrying a chosen modifier set and loadout: the versioned form.
            LevelSeed {
                seed: 8371,
                modifiers: LevelModifiers {
                    always_show_vision_cones: true,
                    ..LevelModifiers::default()
                },
                abilities: Loadout::innate(),
            },
        ] {
            let g = render_help(W, H, HelpTab::LevelInfo, Some(level), level.modifiers);
            let text = text_of(&g);
            let token = level.encode_full();
            assert!(text.contains("LEVEL SEED"), "the section is labelled");
            assert!(text.contains(&token), "the full token is shown: {text:?}");
            assert!(
                token.starts_with("L1-"),
                "…in the versioned form, whatever the preset"
            );
            // The round trip: what a player reads off the panel boots this run.
            assert_eq!(
                LevelSeed::decode(&token),
                Some(level),
                "the displayed token reproduces the run exactly"
            );
        }

        // The default preset is *not* collapsed to its bare-seed link form here:
        // the panel spells the initial situation out (the loadout letters and all).
        let quick = LevelSeed::quick_play(8371);
        let g = render_help(W, H, HelpTab::LevelInfo, Some(quick), quick.modifiers);
        let text = text_of(&g);
        assert!(
            text.contains(&quick.encode_full()),
            "quick play shows its full token: {text:?}"
        );
        assert_eq!(quick.encode(), "8371", "…while its link form stays bare");

        // A hand-built state has no reproducible token, so the section is absent
        // rather than showing a string that boots something else.
        let none = render_help(W, H, HelpTab::LevelInfo, None, LevelModifiers::default());
        assert!(!text_of(&none).contains("LEVEL SEED"));
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
        let g = render_help(W, H, HelpTab::LevelInfo, Some(level), level.modifiers);
        let text = text_of(&g);
        assert!(text.contains("Guards search hideouts"));
        assert!(!text.contains("none active"));
        // THIS RUN@2, LEVEL SEED@4, the token@5, MODIFIERS@7, the first row@8.
        // A chosen modifier set encodes as the versioned `L1-…` form.
        assert_eq!(g.get(3, 5).glyph, 'L', "the token sits under its heading");
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
        let g = render_help(W, H, HelpTab::LevelInfo, None, harder);
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
        let g = render_help(W, H, HelpTab::LevelInfo, None, easier);
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
            let g = render_help(W, H, active, None, LevelModifiers::default());
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

    /// The panel is escapable and switchable **by touch** (§11.6/#248): the `[x]`
    /// close control hit-tests to [`HelpHit::Close`], each tab's cells to
    /// [`HelpHit::Tab`], and the body to nothing (a press the modal panel swallows).
    #[test]
    fn the_panel_is_escapable_and_switchable_by_touch() {
        // The close control at the right edge → Close, and nothing just left of it.
        let close = close_button_start(W);
        assert_eq!(help_hit(W, close, 0), Some(HelpHit::Close));
        assert_eq!(help_hit(W, close + 1, 0), Some(HelpHit::Close));
        assert_ne!(help_hit(W, close - 1, 0), Some(HelpHit::Close));

        // Each tab's whole `[Label]` region resolves to that tab, by identity.
        for (tab, start, len) in tab_layout() {
            for x in start..start + len {
                assert_eq!(help_hit(W, x, 0), Some(HelpHit::Tab(tab)), "tab cell {x}");
            }
        }
        // The body (below the tab bar) and the gap left of the first tab are inert.
        assert_eq!(help_hit(W, 5, 3), None, "the body swallows presses");
        assert_eq!(help_hit(W, 0, 0), None, "the left margin is not a tab");
    }

    /// The tabs cycle, wrapping at both ends (§14 v2/#248) — the Tab / arrow motion.
    #[test]
    fn the_tabs_cycle_both_ways() {
        assert_eq!(HelpTab::LevelInfo.next(), HelpTab::Legend);
        assert_eq!(HelpTab::Legend.next(), HelpTab::LevelInfo, "next wraps");
        assert_eq!(HelpTab::LevelInfo.prev(), HelpTab::Legend, "prev wraps");
        assert_eq!(HelpTab::Legend.prev(), HelpTab::LevelInfo);
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
