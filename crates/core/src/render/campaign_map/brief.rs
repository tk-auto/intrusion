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

use super::super::modifier_rows::{caption, direction_category};
use super::{
    centre, draw, footer, picture, Category, Grid, MapHit, MapUi, ENTRY_SPACING, MARKER, NO_MARKER,
    PRICE_DIGITS, SEPARATOR,
};
use crate::ability::AbilityId;
use crate::campaign::{Campaign, Flavour, NodeId, GATE_RULES_MAX, MANIFEST_COST, SCOUT_COST};
use crate::modifiers::{ActiveModifier, CAPTIONS};
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

/// What the manifest row offers (§8.3/#550) — the crates named, never placed.
///
/// It says the thing itself rather than *cache manifest*: the player has never been shown
/// that word, and the row is the question the sink answers (§11.8). It is also as long as
/// a priced row can be — the compile-time bound below rejected the wordier phrasing this
/// replaced, which is what that bound is for.
const MANIFEST_LABEL: &str = "What the crates hold";

/// The heading the bought manifest stands under, and the bullet each crate is listed with.
///
/// The glyph is the **equipment cache's own** (§11.3): the same `¤` the board draws a crate
/// with, so the list reads as *these boxes* without a legend, and a player who has opened
/// one already knows the mark.
const MANIFEST_HEADING: &str = "The crates hold";
const CRATE_BULLET: char = '¤';

/// The way out, as a row (§11.6). "the map" and not "back": the screen it returns to is one
/// the player has a name for.
const BACK_LABEL: &str = "Back to the map";

/// The heading the **archive's** drawn rules stand under (§4.6/§14 v3/#573), and the line
/// that stands there when the run's stars have taken every one of them off.
///
/// This is *"legible before the choice, not after"* (§14 v3 **[SETTLED]**) at the one
/// press it matters most for. The player decides **when** to walk into the terminus — a
/// facility they may have raided six others to prepare for — and choosing without knowing
/// what is inside it is a coin flip they paid the whole run for. The alert line earns that
/// rule at one hop; the ending earns it at the length of a campaign.
///
/// The cleared line names the **reason** rather than the absence: the gauge's entire payout
/// is this moment, and *nothing drawn* on its own would read as the screen having nothing
/// to say.
const GATE_HEADING: &str = "The rules it is drawn";
const GATE_CLEARED: &str = "Nothing drawn — your stars paid";

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
    /// **Learn what its crates hold** (§8.3/#550) — the manifest, bought once and then
    /// standing as the heading over the list of tech it revealed.
    ///
    /// It is one row in both states, unlike the scout, because a bought manifest has
    /// something to *show*: the row stops being a price and becomes the heading its
    /// captions hang under.
    Manifest { bought: bool },
    /// **Back to the list**, without touching the run.
    Back,
}

impl BriefRow {
    /// What activating this row does — the [`MapHit`] both input paths resolve to.
    pub(super) fn hit(self, node: NodeId) -> MapHit {
        match self {
            BriefRow::Enter => MapHit::Enter(node),
            BriefRow::Scout { .. } => MapHit::Scout(node),
            BriefRow::Manifest { .. } => MapHit::Manifest(node),
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
    let scouted = run.is_scouted(node);
    if scouted || run.scoutable(node) {
        rows.push(BriefRow::Scout { bought: scouted });
    }
    // The manifest (#550), on the same terms and for the sharper reason: a facility that
    // hides no crates has nothing to sell, and its row is absent rather than refusing.
    let manifested = run.has_manifest(node);
    if manifested || run.manifest_on_sale(node) {
        rows.push(BriefRow::Manifest { bought: manifested });
    }
    rows.push(BriefRow::Back);
    rows
}

/// **A line of the brief as it is drawn** — a row the marker may rest on, or a caption that
/// belongs to the row above it.
///
/// The distinction is the whole of what the manifest's expansion costs the screen: the
/// crates it reveals are **not rows**. Nothing happens when Enter is pressed on one, so by
/// the menu's own rule (#268) the marker must not stop there — and a tap must not resolve to
/// one either. Keeping the two kinds in one list, in drawing order, is what makes the
/// geometry answer both questions from one place: `brief_rows` is this filtered to the rows,
/// and every screen row a press can land on comes from the same walk that drew it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Line {
    Row(BriefRow),
    /// One crate of a bought manifest — drawn under the row that revealed it.
    Crate(AbilityId),
    /// A caption belonging to the row above it, in fixed words — the archive gate's
    /// heading, or the line that says it drew nothing (#573).
    Note(&'static str),
    /// One rule the **archive** is drawn (#573), named under the row that would enter it,
    /// in the same caption the Level info tab will print once the player is inside.
    Rule(ActiveModifier),
}

/// **Every line the brief draws**, in order, with the crates of a bought manifest expanded
/// beneath their heading (#550).
///
/// The expansion happens *here* rather than on a third screen, and that is the design: at
/// most three names is not a screen's worth of anything, and putting them one press further
/// away would separate the fact from the two decisions it informs — whether to walk the
/// detour, and whether to raid this facility at all. The brief already answers *what about
/// this facility*; the manifest is one more sentence in that answer.
fn brief_lines(run: &Campaign, node: NodeId) -> Vec<Line> {
    let mut lines = Vec::new();
    for row in brief_rows(run, node) {
        lines.push(Line::Row(row));
        if matches!(row, BriefRow::Manifest { bought: true }) {
            lines.extend(
                run.manifest(node)
                    .unwrap_or_default()
                    .into_iter()
                    .map(Line::Crate),
            );
        }
        // **The archive's gate, under the row that would walk into it** (#573). Under
        // *Enter* and not somewhere lower down, because it is a fact about that press and
        // about no other row on the screen.
        if matches!(row, BriefRow::Enter) && run.map().is_archive(node) {
            let rules = run.archive_rules();
            if rules.is_empty() {
                lines.push(Line::Note(GATE_CLEARED));
            } else {
                lines.push(Line::Note(GATE_HEADING));
                lines.extend(rules.into_iter().map(Line::Rule));
            }
        }
    }
    lines
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
        // Bought, the row is the **heading** of the list under it rather than a statement
        // about itself: what the player wants to read next is the names, and a row saying
        // "bought" above them would be the screen congratulating itself.
        BriefRow::Manifest { bought: true } => format!("{marker}{MANIFEST_HEADING}"),
        BriefRow::Manifest { bought: false } => {
            format!("{marker}{MANIFEST_LABEL}{SEPARATOR}{MANIFEST_COST} intel")
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
        // A bought manifest is a **heading**, so it takes the quiet Ground its captions
        // hang under rather than the Owned a purchase reads as: what is yours here is the
        // list, and the names carry that colour themselves.
        BriefRow::Manifest { bought: true } => Category::Ground,
        BriefRow::Manifest { bought: false } if !run.affords(MANIFEST_COST) => Category::Ground,
        _ => Category::Neutral,
    }
}

/// One revealed crate's line — the cache's own glyph and the ability's name, indented under
/// the heading so the list reads as belonging to the row above it.
fn crate_text(tech: AbilityId) -> String {
    format!("{CAPTION_INDENT}{CRATE_BULLET} {}", tech.name())
}

/// One drawn rule's line — **the caption the Level info tab will print**, indented under
/// the heading, so what the brief promises and what the panel reports once the player is
/// inside are the same string from the same derivation (§11.3/#248). A brief with its own
/// wording for a rule would be the second copy that comes to disagree.
fn rule_text(rule: &ActiveModifier) -> String {
    format!("{CAPTION_INDENT}{}", caption(rule))
}

/// A fixed caption's line — the gate's heading, or the line that says it drew nothing.
fn note_text(note: &str) -> String {
    format!("{CAPTION_INDENT}{note}")
}

/// How far a crate line is indented past its heading. Two cells: enough that the list is
/// plainly subordinate, and not so much that a long ability name has nowhere to go.
const CAPTION_INDENT: &str = "  ";

/// The **widest brief there is**, in lines: four rows — enter, scout, manifest, back — and
/// the three crates a [`Vault`](Flavour::Vault) can reveal beneath one of them.
///
/// It is the tight case and the one that must fit; both halves can be at their maximum at
/// once, since a facility rich enough to hide three crates is one both sinks are for sale
/// on.
const MAX_ROWS: u32 = 4;
const MAX_CRATES: u32 = crate::modifiers::CacheCount::MAX as u32;

/// The **archive's** shape, which is the other candidate for the tallest block (#573):
/// three rows — enter, scout, back — under a heading and the [`GATE_RULES_MAX`] rules the
/// star gate can deal.
///
/// **Three rows and not four**, and it is a fact rather than an omission: the terminus
/// hides no equipment caches (`Composite::Archive` sets them to none, §14 v3), so the
/// manifest is never on sale there and the two expansions can never appear on one screen.
/// `the_archive_never_offers_a_manifest_beside_its_gate` is what keeps that true.
const MAX_GATE_ROWS: u32 = 3;
const MAX_GATE_LINES: u32 = 1 + GATE_RULES_MAX as u32;

/// How tall the brief's block is, given how many lines of each kind it is drawing.
///
/// The spacing rule is *a blank before every row but the first*: a row keeps the buffer
/// that stops a low tap landing on its neighbour (§11.6/[`ENTRY_SPACING`]), and a **crate
/// line is packed tight under the heading it belongs to**. That is what makes the expansion
/// read as one row's answer rather than as three more things to press — and it is why the
/// blank goes *before* a row rather than after it: the list has to hug its heading, and the
/// way out still has to have its buffer.
const fn block_rows(rows: u32, crates: u32) -> u32 {
    // Every row and every crate line, plus one blank ahead of each row after the first,
    // plus the blank that keeps the last line off the footer — which is prose about the
    // screen rather than a line of the block.
    rows + crates + rows.saturating_sub(1) + 2
}

/// The tallest the block can ever be — what the layout reserves so the picture above it
/// never changes height between one brief and another.
///
/// **The block is a fixed height, not a fitted one.** A brief that grew when a manifest was
/// bought would move the map under the player at the exact moment they were reading it —
/// the same objection the wallet line answers on the map screen by being drawn even when
/// the wallet is empty (#211).
///
/// It is the **taller of the two expansions** a brief can carry — a bought manifest's
/// crates, and the archive's drawn rules — for the same reason it is fixed at all.
pub(super) const LIST_ROWS: u32 = {
    let crates = block_rows(MAX_ROWS, MAX_CRATES);
    let gate = block_rows(MAX_GATE_ROWS, MAX_GATE_LINES);
    if gate > crates {
        gate
    } else {
        crates
    }
};

/// The screen row each drawn line sits on, paired with the line — the one walk the drawing
/// and the hit-test share, so a tap lands on exactly the row that was drawn.
///
/// The block starts at a **fixed top** ([`LIST_ROWS`] up from the footer), never one fitted
/// to what it happens to be showing. Buying a manifest therefore pushes the *way out* down
/// by the length of the list and leaves every row above it exactly where it was: a screen
/// whose rows slid upward as the list grew would move the marker under the player's finger
/// at the moment they pressed it.
fn laid_out(run: &Campaign, node: NodeId, height: u32) -> Vec<(u32, Line)> {
    let lines = brief_lines(run, node);
    let mut y = height.saturating_sub(LIST_ROWS);
    let mut placed = Vec::with_capacity(lines.len());
    for (i, line) in lines.into_iter().enumerate() {
        if i > 0 && matches!(line, Line::Row(_)) {
            y += ENTRY_SPACING - 1; // the buffer blank, ahead of the row it protects
        }
        placed.push((y, line));
        y += 1;
    }
    placed
}

/// The screen row the brief's **row** at `index` is drawn on — [`laid_out`] filtered to the
/// lines a marker may rest on and a press may land on.
///
/// The drawing and the hit-test both walk [`laid_out`] whole, so this exists for the tests
/// that ask about one row by number.
#[cfg(test)]
fn row_of_brief(run: &Campaign, node: NodeId, height: u32, index: usize) -> Option<u32> {
    laid_out(run, node, height)
        .into_iter()
        .filter_map(|(y, line)| match line {
            Line::Row(row) => Some((y, row)),
            Line::Crate(_) | Line::Note(_) | Line::Rule(_) => None,
        })
        .nth(index)
        .map(|(y, _)| y)
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
    let priced =
        MARKER.len() + MANIFEST_LABEL.len() + SEPARATOR.len() + PRICE_DIGITS + " intel".len();
    if priced > widest {
        widest = priced;
    }
    let heading = MARKER.len() + MANIFEST_HEADING.len();
    if heading > widest {
        widest = heading;
    }
    // Every crate line, measured at the longest ability name there is: a wordier ability
    // fails the build here rather than clipping in a player's manifest.
    let mut t = 0;
    while t < AbilityId::TECH.len() {
        let line = CAPTION_INDENT.len() + 2 + AbilityId::TECH[t].name().len();
        if line > widest {
            widest = line;
        }
        t += 1;
    }
    let mut i = 0;
    while i < Flavour::ALL.len() {
        let enter = MARKER.len() + ENTER_PREFIX.len() + Flavour::ALL[i].label().len();
        if enter > widest {
            widest = enter;
        }
        i += 1;
    }
    // The archive gate's lines (#573): its two fixed captions, and **every** caption a
    // drawn rule can print — the whole set, so a re-worded modifier fails the build here
    // rather than clipping on the one screen where the player is deciding whether to walk
    // into it.
    let heading = CAPTION_INDENT.len() + GATE_HEADING.len();
    if heading > widest {
        widest = heading;
    }
    let cleared = CAPTION_INDENT.len() + GATE_CLEARED.len();
    if cleared > widest {
        widest = cleared;
    }
    let mut c = 0;
    while c < CAPTIONS.len() {
        let rule = CAPTION_INDENT.len() + CAPTIONS[c].caption_len();
        if rule > widest {
            widest = rule;
        }
        c += 1;
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
    laid_out(run, node, height)
        .into_iter()
        .find_map(|(row_y, line)| match line {
            // A **caption is not a target** (#550/#573): nothing happens when a crate line
            // or a drawn rule is pressed, so a press that lands on one is swallowed exactly
            // as a press on the blank between rows is. A list that answered taps would be
            // three buttons that do nothing.
            Line::Row(row) if row_y == y => Some(row.hit(node)),
            _ => None,
        })
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
    let lines = laid_out(run, node, height);
    let widest = lines
        .iter()
        .map(|&(_, line)| match line {
            Line::Row(row) => row_text(row, flavour, true).chars().count() as u32,
            Line::Crate(tech) => crate_text(tech).chars().count() as u32,
            Line::Note(note) => note_text(note).chars().count() as u32,
            Line::Rule(rule) => rule_text(&rule).chars().count() as u32,
        })
        .max()
        .unwrap_or(0);
    let column = centre(width, widest);
    let mut index = 0;
    for (y, line) in lines {
        match line {
            Line::Row(row) => {
                draw(
                    &mut grid,
                    column,
                    y,
                    &row_text(row, flavour, index == marked),
                    row_category(run, row, index == marked),
                );
                index += 1;
            }
            // **Owned** (§11.2): a revealed crate is a thing the run has been told and now
            // has, on the same footing as the intel in its wallet — and a colour the
            // heading above it deliberately does not share, so the list lifts off it.
            Line::Crate(tech) => draw(&mut grid, column, y, &crate_text(tech), Category::Owned),
            // **Ground** for the heading, the same quiet the manifest's heading takes, so
            // the rules under it lift off; **Owned** for the cleared line, because a gate
            // the run's stars have emptied is a thing the player earned, in the colour this
            // screen already gives one.
            Line::Note(note) => {
                let category = if note == GATE_CLEARED {
                    Category::Owned
                } else {
                    Category::Ground
                };
                draw(&mut grid, column, y, &note_text(note), category);
            }
            // The rule's own §11.2 direction cue — Warning, since the gate only ever deals
            // harder rules — read from the same derivation the Level info tab reads it
            // from, so the colour the player learns here is the colour they meet inside.
            Line::Rule(rule) => draw(
                &mut grid,
                column,
                y,
                &rule_text(&rule),
                direction_category(rule.direction),
            ),
        }
    }
    footer(&mut grid, FOOTER);
    grid
}
