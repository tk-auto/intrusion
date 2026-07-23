//! The help overlay: a glyph legend, the colour categories, and the controls
//! (§14 v2, #139). The old game never had a legend — "nothing ever explained what
//! `$`, `E`, `}` or `z` meant" — so this is the reference card, toggled on demand.
//!
//! **Every row derives from the real source**, never a hand-copied table that could
//! drift from the game it documents (§11.2/§11.3/§11.6): terrain glyphs and their
//! categories come from [`Terrain::glyph`]/[`Terrain::category`], the entity glyphs
//! from the [`super`] render constants the world draws with, the colour meanings from
//! an exhaustive match over [`Category`] (a new category will not compile until it is
//! documented), and the ability keys from [`AbilityId`]'s settled §11.6 hotkeys. The
//! tests assert each derivation, so an edit to a glyph or a key surfaces here.
//!
//! Opening and closing the overlay is a pure **view** toggle owned by the shell
//! ([`ScreenUi::help_open`](super::ScreenUi)) — it changes no world, costs no turn
//! (§4.4), and so no guard moves while it is up. [`overlay_help`] draws it over the
//! map layer only, leaving the header (with its toggle button) and the status lines
//! in place, so the same button that opened it can close it by touch (§11.6).

use super::{GlyphCell, Grid, Visibility, BODY_GLYPH, FLOOR_DOT, GUARD_GLYPH, PLAYER_GLYPH};
use crate::ability::AbilityId;
use crate::category::Category;
use crate::facility::Terrain;

/// The key that toggles the help overlay (§11.6). A free letter — not a movement
/// key, an ability hotkey, or another UI control — and the conventional roguelike
/// help key. Shown in the controls list and matched in
/// [`ui_command_for_key`](crate::input::ui_command_for_key).
pub(crate) const HELP_KEY: char = '?';

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
        terrain(Terrain::Exit, "the exit"),
        (FLOOR_DOT, Category::Ground, "floor"),
    ]
}

/// Every information category (§11.2), in reading order. Paired with
/// [`category_meaning`] this is the colour key — the shell draws each name in the
/// colour the category maps to, so the player sees the colour and its meaning
/// together.
const CATEGORIES: [Category; 9] = [
    Category::Owned,
    Category::Caution,
    Category::Warning,
    Category::Danger,
    Category::Sensed,
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
        Category::Sensed => "guard felt through a wall",
    }
}

/// The controls (§11.6), each `(keys, action)`. Movement and wait are the fixed
/// rows; the **ability** rows derive their keys from [`AbilityId`]'s settled hotkeys,
/// so an ability's key on this card is exactly the key that activates it; the UI keys
/// close the card and drive the panels.
fn control_rows() -> Vec<(String, &'static str)> {
    let mut rows: Vec<(String, &'static str)> = vec![
        ("arrows / hjkl / 8246".to_string(), "move"),
        ("w / 5 / .".to_string(), "wait & sense"),
    ];
    for id in AbilityId::ALL {
        rows.push((id.hotkey().to_string(), id.name()));
    }
    rows.push(("Tab".to_string(), "ability panel"));
    rows.push(("m".to_string(), "messages"));
    rows.push((HELP_KEY.to_string(), "this help"));
    rows
}

/// Overlay the help card onto the map `grid` (§14 v2/#139): the glyph legend, the
/// colour key, and the controls, laid over a cleared board. Drawn on the map layer
/// only — before the header and status rows are added — so the toggle button and the
/// near/usable lines stay put and closing restores the exact board beneath (the
/// overlay writes no state).
///
/// Bounds are clamped, never asserted: a board too small for every row (only
/// hand-built test states get that small — the v1 board is 40×40, §10.2) shows what
/// fits and stops.
pub(super) fn overlay_help(grid: &mut Grid) {
    // Clear the map to background so the card reads as a solid page over the board.
    let blank = GlyphCell {
        glyph: ' ',
        fg: Category::Neutral,
        bg: None,
        vis: Visibility::Live,
    };
    for cell in grid.cells.iter_mut() {
        *cell = blank;
    }

    let mut y = 1u32;
    draw(grid, 2, y, "HELP", Category::Interest);
    y += 2;

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
        draw(grid, 26, y, action, Category::Neutral);
        y += 1;
    }

    // A closing hint, so touch users who opened it by tapping the button know the
    // way out is the same tap (§11.6's trap: never unreachable, never inescapable).
    if y < grid.height {
        draw(
            grid,
            2,
            grid.height - 1,
            "[?] again to close",
            Category::Ground,
        );
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
    }
}

/// Write `text` onto `grid` from `(x, y)` in `category`, clamping at the right edge
/// and off the bottom — the one drawing primitive the overlay shares, so every row
/// truncates the same way on a small board.
fn draw(grid: &mut Grid, x: u32, y: u32, text: &str, category: Category) {
    if y >= grid.height {
        return;
    }
    for (i, glyph) in text.chars().enumerate() {
        let cx = x + i as u32;
        if cx >= grid.width {
            break;
        }
        grid.cells[(y * grid.width + cx) as usize] = GlyphCell {
            glyph,
            fg: category,
            bg: None,
            vis: Visibility::Live,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(CATEGORIES.len(), 9, "all nine §11.2 categories are keyed");
        for &c in &CATEGORIES {
            assert!(!category_meaning(c).is_empty(), "{c:?} has a meaning");
            assert!(!category_name(c).is_empty(), "{c:?} has a name");
        }
    }

    /// The ability control rows carry each ability's **settled** §11.6 hotkey and
    /// name, straight from [`AbilityId`] — so the card's keys are the keys that
    /// actually activate them, and cannot drift.
    #[test]
    fn the_control_rows_carry_the_real_ability_hotkeys() {
        let rows = control_rows();
        for id in AbilityId::ALL {
            let key = id.hotkey().to_string();
            assert!(
                rows.iter().any(|(k, a)| *k == key && *a == id.name()),
                "the controls must list {} as key {key}",
                id.name(),
            );
        }
        // The help key documents itself.
        assert!(rows.iter().any(|(k, _)| *k == HELP_KEY.to_string()));
    }
}
