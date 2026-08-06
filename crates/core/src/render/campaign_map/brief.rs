//! The **facility brief** (§11.1/§14 v3, #215): the map's sub-screen for one facility —
//! what may be done about it, and what that costs.
//!
//! # Why picking a facility no longer raids it
//!
//! The map's list used to answer *which facility* and start the raid in the same press.
//! That was right while a facility was a thing you could only walk into; it stopped being
//! right the moment intel could **change** one before you did (§14 v3's pre-level sinks).
//! A hub with prices needs somewhere to put them, and the two candidates were both worse:
//! priced rows mixed into the list would make *"the Vault"* and *"scout the Vault"* two
//! rows that look alike and do opposite things, and a modal shop would be a third screen
//! for one question.
//!
//! So the list answers *which*, and this answers *what about it*. Enter on a facility row
//! opens this; the run has **not moved** ([`Campaign::choose`] has not been called), so a
//! scout bought here is bought **before the run commits** and the player may still back
//! out and take another road — with the intel spent, which is the sink's teeth (#215).
//!
//! # It is the same screen, with the rows swapped
//!
//! Everything above the list is [`picture`](super::picture) — the heading, the alert line,
//! the wallet, the country with this facility marked. Only the rows differ. That is what
//! makes the brief cheap to learn: it is not somewhere you went, it is the row you pressed,
//! opened up.
//!
//! # One irreversible press, and it is drawn as a row
//!
//! *Enter the facility* is the only thing here that cannot be taken back (§2.1), and it
//! sits at the top so the common case is still **Enter, Enter** from the map. *Back* is a
//! drawn, tappable row rather than an `Escape` a phone does not have (§11.6's no-trap
//! rule), and `Escape` works too for the hand that reaches for it.

use super::{
    centre, draw, footer, picture, Category, Grid, MapHit, MapUi, ENTRY_SPACING, MARKER, NO_MARKER,
    PRICE_DIGITS, SEPARATOR,
};
use crate::campaign::{Campaign, Flavour, NodeId, SCOUT_COST};
use crate::place::LevelConfig;

#[cfg(test)]
mod tests;

/// The footer. It names the way on and the way out, both of them reachable by finger as
/// well as by key (§11.6) — the row is the target, and `Esc` is the shortcut.
const FOOTER: &str = "Enter/tap picks · Esc back";

/// What the row that starts the raid says, before the facility's own name — *the Vault*,
/// *the Archive*. The flavour is the facility's name on this screen (§11.8: the map
/// already taught the word), so the row reads as a sentence rather than as a label and a
/// value.
const ENTER_PREFIX: &str = "Enter the ";

/// What the scout row offers, in the sink's own word (§11.8: *scout* is what the hub sells
/// and what the Level info card calls it once you are inside).
const SCOUT_LABEL: &str = "Scout the facility";

/// What the scout row says once it is bought — the label **and** the blurb, because a
/// price that has been paid is not a price any more. It names what the purchase did rather
/// than that it happened: the contents are on your map, which is the fact the player acts
/// on when they walk in.
const SCOUTED_LABEL: &str = "Scouted";
const SCOUTED_BLURB: &str = "contents on your map";

/// The way out, as a row (§11.6). "the map" and not "back": the screen it returns to is one
/// the player has a name for.
const BACK_LABEL: &str = "Back to the map";

/// A **row of the brief** — everything that may be done about one facility.
///
/// An enum rather than an index into a list of strings because each row does something
/// different and the shell acts on the *thing*, not the position: the scout row is absent
/// on a facility that cannot be scouted, so a row number would mean two things on two
/// screens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BriefRow {
    /// **Raid it** — the one irreversible press (§2.1), and the row the marker opens on.
    Enter,
    /// **Scout it** (§11.5a/#215) — buy its contents, drawn remembered from turn one.
    /// Carries whether the run has already bought them, because the row says so rather
    /// than disappearing: a facility you have scouted should read as scouted on the screen
    /// you scouted it from.
    Scout { bought: bool },
    /// **Back to the list**, without touching the run.
    Back,
}

impl BriefRow {
    /// What activating this row does — the [`MapHit`] both input paths resolve to.
    pub(super) fn hit(self, node: NodeId) -> MapHit {
        match self {
            BriefRow::Enter => MapHit::Enter(node),
            BriefRow::Scout { .. } => MapHit::Scout(node),
            BriefRow::Back => MapHit::Back,
        }
    }
}

/// **The rows this facility's brief shows**, in drawing order — also the order the marker
/// walks.
///
/// The scout row is listed when the run may still buy the plan, and when it already has;
/// it is **absent** otherwise, and that absence is a decision rather than an oversight
/// (#215). A facility whose config has no room left in its level-seed token cannot take
/// the rule ([`Campaign::scoutable`]), and a price the hub cannot honour must not be drawn
/// at all — an unaffordable row is still a real offer the player can save up for, where
/// this one would never become takeable however much intel they banked. §11.6's no-trap
/// rule read one screen up: do not draw what cannot be pressed.
pub fn brief_rows(run: &Campaign, node: NodeId) -> Vec<BriefRow> {
    let mut rows = vec![BriefRow::Enter];
    let bought = run.is_scouted(node);
    if bought || run.scoutable(node) {
        rows.push(BriefRow::Scout { bought });
    }
    rows.push(BriefRow::Back);
    rows
}

/// One row's text, marker and all — the [`entry_text`](super::super::menu) of this screen,
/// and one function for the same reason: the drawing and the width it is centred by cannot
/// disagree.
fn row_text(row: BriefRow, flavour: Flavour, selected: bool) -> String {
    let marker = if selected { MARKER } else { NO_MARKER };
    match row {
        BriefRow::Enter => format!("{marker}{ENTER_PREFIX}{}", flavour.label()),
        BriefRow::Scout { bought: true } => {
            format!("{marker}{SCOUTED_LABEL}{SEPARATOR}{SCOUTED_BLURB}")
        }
        BriefRow::Scout { bought: false } => {
            format!("{marker}{SCOUT_LABEL}{SEPARATOR}{SCOUT_COST} intel")
        }
        BriefRow::Back => format!("{marker}{BACK_LABEL}"),
    }
}

/// The §11.2 meaning a row carries.
///
/// **Interest for the marked row**, as on every screen. Off the marker: an ordinary live
/// option is **Neutral**; a scout the wallet cannot cover is **Ground**, the same *on the
/// screen, not available to you* the map gives an unaffordable road, so the price reads as
/// out of reach at a glance rather than when pressed; and a scout already bought is
/// **Owned**, because it is a thing the run has (§11.2), like the intel in the wallet and
/// the `@` on the picture.
fn row_category(run: &Campaign, row: BriefRow, marked: bool) -> Category {
    if marked {
        return Category::Interest;
    }
    match row {
        BriefRow::Scout { bought: true } => Category::Owned,
        BriefRow::Scout { bought: false } if !run.affords(SCOUT_COST) => Category::Ground,
        _ => Category::Neutral,
    }
}

/// Rows the brief needs beneath the picture: one per row at [`ENTRY_SPACING`], for the
/// widest brief there is — enter, scout, back — with a blank row at each end, exactly as
/// the map's list is spaced.
const MAX_ROWS: u32 = 3;
const LIST_ROWS: u32 = (MAX_ROWS - 1) * ENTRY_SPACING + 3;

/// The screen row the brief's row at `index` is drawn on. Shared by the drawing and
/// [`brief_hit`], so a tap lands on exactly the row that was drawn, with the blank between
/// entries buffering a low tap off its neighbour (§11.6).
fn row_of_brief(height: u32, index: usize) -> u32 {
    height.saturating_sub(LIST_ROWS) + index as u32 * ENTRY_SPACING
}

/// The widest row the brief can ever draw, in cells — every row measured at its widest
/// text, so a longer label or a wordier flavour fails the **build** rather than being
/// discovered as a clipped line on a player's screen (the map's rule, one screen over).
const MAX_ROW_WIDTH: usize = {
    let mut widest =
        MARKER.len() + SCOUT_LABEL.len() + SEPARATOR.len() + PRICE_DIGITS + " intel".len();
    let scouted = MARKER.len() + SCOUTED_LABEL.len() + SEPARATOR.len() + SCOUTED_BLURB.len();
    if scouted > widest {
        widest = scouted;
    }
    let back = MARKER.len() + BACK_LABEL.len();
    if back > widest {
        widest = back;
    }
    let mut i = 0;
    while i < Flavour::ALL.len() {
        let enter = MARKER.len() + ENTER_PREFIX.len() + Flavour::ALL[i].label().len();
        if enter > widest {
            widest = enter;
        }
        i += 1;
    }
    widest
};

const _: () = assert!(
    MAX_ROW_WIDTH <= LevelConfig::V1.width as usize,
    "a facility-brief row must fit the v1 board (§10.2): shorten a label",
);

/// Which [`MapHit`] a press on screen row `y` lands on while the brief is up, or `None`
/// for a press that hit nothing and is swallowed.
///
/// **The whole row is the target** at any column, like the map's list and the title
/// screen's: the rows are far apart and a finger is not a cursor (§11.6). The footer's
/// theme control is tested before this by [`map_hit`](super::map_hit), which owns the one
/// thing both screens share.
pub(super) fn brief_hit(height: u32, run: &Campaign, node: NodeId, y: u32) -> Option<MapHit> {
    brief_rows(run, node)
        .into_iter()
        .enumerate()
        .find(|&(i, _)| row_of_brief(height, i) == y)
        .map(|(_, row)| row.hit(node))
}

/// Render the **facility brief** (§11.1/§14 v3/#215) — the whole `width × height` screen,
/// so the shell paints it through the one path it paints a frame with.
///
/// A pure view of campaign state, like the map it is drawn over: it mutates nothing, costs
/// no turn (§4.4/§12.1), and the same `(campaign, ui, node)` always draws the same grid.
///
/// The facility it is about is marked on the picture — so *this option is over there, two
/// lanes across* is still on the screen while its price is being read — and named in the
/// row that raids it.
pub fn render_brief(width: u32, height: u32, run: &Campaign, ui: MapUi, node: NodeId) -> Grid {
    let mut grid = picture(width, height, run, ui, Some(node));
    let rows = brief_rows(run, node);
    let marked = ui.row(rows.len());
    let flavour = run.map().flavour(node);
    let widest = rows
        .iter()
        .map(|&row| row_text(row, flavour, true).chars().count() as u32)
        .max()
        .unwrap_or(0);
    let column = centre(width, widest);
    for (i, &row) in rows.iter().enumerate() {
        draw(
            &mut grid,
            column,
            row_of_brief(height, i),
            &row_text(row, flavour, i == marked),
            row_category(run, row, i == marked),
        );
    }
    footer(&mut grid, FOOTER);
    grid
}
