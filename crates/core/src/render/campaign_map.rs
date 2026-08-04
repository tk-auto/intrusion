//! The **campaign map screen** (§11.1/§14 v3, #208): the facility graph drawn as a
//! character grid, and the surface a campaign is played from between raids.
//!
//! #207 owns the model — the lanes, the lazy derivation, the flavours, the locked edge
//! — and this owns the pixels, the #133/#134 split one layer up. Nothing here decides
//! what the country *is*; it decides how a country reads.
//!
//! # Two halves, and the split is the whole design
//!
//! **The picture** says where you are. Nodes at their [`MapPos`], the route you took
//! joined behind you, the edges ahead fanning out, and the archive standing at the top
//! of the screen from the first frame — so a run always knows how far it has to go
//! (§14 v3: the map is not fogged). It is a *picture*: it answers "where", not "which".
//!
//! **The list** says what you may do. One row per facility the run may walk into
//! ([`Campaign::ahead`]), marked and walked exactly as the title screen's entries are,
//! because it is the same shape of question and a player who has used one screen has
//! already learned the other. It answers "which".
//!
//! Splitting them is what keeps the screen legible on a 40-column board (§10.2). A map
//! that tried to carry the labels would need a caption beside every node and there is no
//! room; a list alone would be the old flat list with no geography, which is the thing
//! §14 v3 threw out. Together the picture says *this option is over there, two lanes
//! across* and the row says *and it is a Vault*.
//!
//! # It fits the country it is handed
//!
//! The model's coordinates ([`FacilityMap::position`]) are a space, not a layout: the
//! renderer scales them onto whatever band of screen it has (see [`plot`]). So the
//! country's spacing and jitter are free to move as `[START]` numbers without the screen
//! needing to know, and a board of another size draws the same map at another scale —
//! no camera, no scrolling, the whole graph at once (§11.4).
//!
//! # Colour is named, never chosen (§11.2)
//!
//! Every glyph here carries a [`Category`] and the shell owns the table: **Owned** for
//! where you stand (it is yours), **Interest** for the archive and for the row the
//! marker rests on (the thing worth reaching for), **Neutral** for a live option,
//! **Ground** for the road behind you, the facilities you have spent, and the locked
//! edge you cannot take yet.

use super::help::{theme_control, theme_control_len, theme_control_start, FOOTER_INDENT};
use super::menu::{centre, ENTRY_SPACING, MARKER, NO_MARKER};
use super::{blank_grid, draw, Grid};
use crate::campaign::map::{DEPTH_SPACING, LANES, LANE_SPACING};
use crate::campaign::{Campaign, Flavour, MapPos, NodeId, Offer};
use crate::category::Category;
use crate::place::LevelConfig;

#[cfg(test)]
mod tests;

/// The heading, in §11.8's **meta vocabulary** — it names the game around the run, so it
/// reads as plainly as the title screen's own rows.
const HEADING: &str = "THE FACILITY MAP";

/// The footer. It names both input paths (§11.6), like the menu's: a touch player who
/// cannot see a keyboard still reads that tapping a row raids it.
const FOOTER: &str = "↑↓ choose · Enter/tap raids";

/// What an intel-locked row says (§14 v3's alternative-route sink, #212). It names the
/// **currency** rather than a price: the price is #212's to set, and a number invented
/// here would be a cost nobody designed printed on a screen the player believes.
const LOCKED_LABEL: &str = "Alternative route";
const LOCKED_BLURB: &str = "costs intel";

/// The glyph an intel-locked node is drawn with. Not the flavour's own: what stands on
/// unbought ground is a thing the player has not been told, and drawing the flavour would
/// hand over for free the one fact the sink exists to sell.
const LOCKED_GLYPH: char = '?';

/// Where the run stands — the player's own glyph, the same one the board draws them
/// with (§11.3), so "you are here" needs no legend.
const HERE_GLYPH: char = '@';

/// What an edge is drawn with: the faint dotted road between two facilities. Ground, so
/// it joins the nodes into a country without competing with them for the eye.
const EDGE_GLYPH: char = '·';

/// A facility that is **on the map but not in front of you** — every node the run has
/// neither stood on nor been offered.
///
/// Drawn as an empty outline against the **filled square** a plain facility gets — the
/// two are a matched pair at one size, so the contrast is fill and nothing else. And
/// fill is exactly the distinction being drawn: the country's *shape* is public and its
/// *contents* are not. There is no fog on the map (§14 v3) — the graph is all there, and
/// you can see how far the archive is and how much room there is either side of your
/// route — but a
/// facility says what it is when it is **offered**, and not a hop before. Drawing every
/// flavour across the whole country would hand over for free what the scout sinks exist
/// to sell (#215), and would settle a question §14 v3 did not.
///
/// It is the §11.5a rule one scale up, which is why it reads without being taught:
/// geometry always, contents once you are close enough to see.
const UNKNOWN_GLYPH: char = '▫';

/// The screen row the heading sits on, and the first row of the map band beneath it.
const HEADING_ROW: u32 = 0;
const MAP_TOP: u32 = 2;

/// Rows the list of rows-you-may-take needs beneath the map: one per offer at
/// [`ENTRY_SPACING`], for the widest offer a choice point can make — three open edges
/// and the locked one (§14 v3) — with a blank row at each end.
///
/// **Both blanks are load-bearing.** The one above separates the list from the picture,
/// so the rows read as a list rather than as captions that drifted off the map; the one
/// below keeps the last row off the footer, which is prose about the screen rather than
/// a row of it. The four-row case is the tight one and it is the one that must fit — a
/// choice point always offers its locked edge, so it is also the common one.
const MAX_ROWS: u32 = 4;
const LIST_ROWS: u32 = (MAX_ROWS - 1) * ENTRY_SPACING + 3;

/// The widest row the list can ever draw, in cells — the widest flavour label against the
/// widest blurb, plus the marker and the dash between them, and the locked row measured
/// the same way.
const MAX_ROW_WIDTH: usize = {
    let mut widest = row_width(LOCKED_LABEL.len(), LOCKED_BLURB.len());
    let mut i = 0;
    while i < Flavour::ALL.len() {
        let row = row_width(Flavour::ALL[i].label().len(), Flavour::ALL[i].blurb().len());
        if row > widest {
            widest = row;
        }
        i += 1;
    }
    widest
};

/// One row's width from its two halves — the measure and the drawing kept together, so
/// the bound below cannot come to measure something other than what is printed.
///
/// Byte lengths, not character counts: `len()` is what a `const` can see, and it can only
/// **over**-count against a multi-byte glyph — an over-count fails the build early rather
/// than letting a row through that would clip. The dash is the one multi-byte character
/// in a row's furniture and it is counted exactly, in [`SEPARATOR`].
const fn row_width(label: usize, blurb: usize) -> usize {
    MARKER.len() + label + SEPARATOR.len() + blurb
}

/// What stands between a facility's name and what it is worth. Its own constant because
/// [`row_width`] has to measure exactly what [`row_text`] prints.
const SEPARATOR: &str = " — ";

/// **A row must fit the board it is drawn on** (§10.2/§11.4), and this is where that
/// stops being a hope. Every input is derived from the flavours themselves, so a longer
/// blurb, a renamed flavour or a fifth flavour with a wordy tagline fails the **build**
/// rather than being discovered as a truncated line on a player's screen — the ability
/// bar's rule (#287), on the one other screen whose rows are assembled from data.
const _: () = assert!(
    MAX_ROW_WIDTH <= LevelConfig::V1.width as usize,
    "a campaign-map row must fit the v1 board (§10.2): shorten a flavour's blurb or its \
     label",
);

/// The map screen's **view state**, owned by the shell exactly like
/// [`MenuUi`](super::MenuUi) — it changes no world and costs no turn (§12.1).
///
/// One field, and deliberately so: which facility the marker rests on is the only thing
/// about this screen the player can change without the campaign moving.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MapUi {
    /// Which row the marker rests on, as an index into [`Campaign::ahead`]. Clamped at
    /// use rather than trusted ([`selected`](Self::selected)): the offer list changes
    /// under it every time the run moves, and a marker resting past the end of a shorter
    /// list would draw nothing and activate nothing.
    pub selected: usize,
}

impl MapUi {
    /// The row the marker actually rests on, given what is `ahead` — the clamp every
    /// caller shares, so the drawing, the hit-test and the activation cannot disagree
    /// about which row is selected.
    ///
    /// **Locked rows are skipped**, exactly as the menu steps over its *later* entries:
    /// the marker only ever rests somewhere pressing Enter does something (#268). A list
    /// of nothing but locked rows cannot happen — every choice point offers at least
    /// [`MIN_OPEN`](crate::campaign::map::MIN_OPEN) open edges — but if it did, the
    /// marker would sit on row zero and activate nothing, which is the safe direction.
    pub fn selected(self, ahead: &[Offer]) -> usize {
        if ahead.get(self.selected).is_some_and(|o| !o.locked) {
            return self.selected;
        }
        ahead.iter().position(|o| !o.locked).unwrap_or(0)
    }

    /// The marker moved one row `step`-wards, wrapping, landing only on takeable rows.
    /// Gives up and stays put after a full lap, so a list with nothing takeable freezes
    /// rather than spins — [`MenuEntry::seek`](super::MenuEntry)'s rule, one screen over.
    fn seek(self, ahead: &[Offer], step: usize) -> Self {
        let n = ahead.len();
        if n == 0 {
            return self;
        }
        let start = self.selected(ahead);
        let selected = (1..=n)
            .map(|i| (start + step * i) % n)
            .find(|&i| !ahead[i].locked)
            .unwrap_or(start);
        Self { selected }
    }

    /// The next takeable row down, wrapping.
    #[must_use]
    pub fn next(self, ahead: &[Offer]) -> Self {
        self.seek(ahead, 1)
    }

    /// The previous takeable row up, wrapping.
    #[must_use]
    pub fn prev(self, ahead: &[Offer]) -> Self {
        self.seek(ahead, ahead.len().saturating_sub(1))
    }
}

/// What a press on the map screen lands on (§11.6) — the touch half of
/// [`map_nav_for_key`](crate::map_nav_for_key), and the map's counterpart of
/// [`MenuHit`](super::MenuHit).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapHit {
    /// A row of the list — raid that facility. Carries the **node**, not the row index:
    /// the shell acts on a facility, and an index would have to be re-resolved against a
    /// list that may have changed since the frame was drawn.
    Facility(NodeId),
    /// The footer's `theme [n]` control (§11.2/#189), in the same corner it keeps on
    /// every other screen.
    ToggleTheme,
}

/// The glyph a facility of this flavour is drawn with on the map.
///
/// Two of them are borrowed from the board's own vocabulary on purpose (§11.3): `$` is
/// the intel console, so a **Vault** reads as *the place with the loot in it* without a
/// legend, and `★` is the one glyph on this screen the game does not use elsewhere,
/// which is right for the one place a run is trying to reach.
///
/// **The Depot is `▪` and not `■`** — small, not large. A full block is the heaviest ink
/// the grid can put down, and in light mode it made an *unmarked* option pull the eye
/// harder than the marked one, which is the marker's job. It is the same objection
/// `docs/render-reference.md` §2.4 records against `▒` for the schematic, one screen
/// over: a mark that shouts wins an argument it should not be in. `▪` also pairs exactly
/// with [`UNKNOWN_GLYPH`] — one size, differing only in fill — so *known* against
/// *unknown* reads as the single distinction it is.
pub fn flavour_glyph(flavour: Flavour) -> char {
    match flavour {
        Flavour::Outpost => 'o',
        Flavour::Depot => '▪',
        Flavour::Vault => '$',
        Flavour::Archive => '★',
    }
}

/// Where a model position lands on the screen, given the band the map has to draw in.
///
/// **The renderer fits the country; the country does not lay itself out.** The model's
/// span is `LANES × LANE_SPACING` across and `depth × DEPTH_SPACING` along, and both are
/// `[START]` numbers (#207) — so they are scaled onto the available cells here rather
/// than assumed to be cell counts. Depth runs **up** the screen: the archive stands at
/// the top and the run climbs toward it, which is the reading the goal deserves.
fn plot(pos: MapPos, depth: u32, width: u32, map_h: u32) -> (u32, u32) {
    let span_x = (LANES as i32 * LANE_SPACING).max(1);
    let span_y = (depth as i32 * DEPTH_SPACING).max(1);
    let x = (pos.x.clamp(0, span_x) * (width.saturating_sub(1)) as i32) / span_x;
    let along = (pos.y.clamp(0, span_y) * (map_h.saturating_sub(1)) as i32) / span_y;
    // Invert: depth zero at the foot of the band, the archive at its head.
    (x as u32, MAP_TOP + (map_h.saturating_sub(1) - along as u32))
}

/// The screen row the list's entry at `index` is drawn on. Shared by the drawing and
/// [`map_hit`], so a tap lands on exactly the row that was drawn, with the blank between
/// entries ([`ENTRY_SPACING`]) buffering a low tap off its neighbour (§11.6).
fn row_of(height: u32, index: usize) -> u32 {
    list_top(height) + index as u32 * ENTRY_SPACING
}

/// The first row of the list — the block sitting above the footer, with the map band
/// taking everything left over above it.
fn list_top(height: u32) -> u32 {
    height.saturating_sub(LIST_ROWS)
}

/// How many rows the map band gets on a screen `height` tall: everything between the
/// heading and the list.
fn map_height(height: u32) -> u32 {
    list_top(height).saturating_sub(MAP_TOP + 1)
}

/// One row's text, marker and all — the [`entry_text`](super::menu) of this screen, and
/// one function for the same reason: the drawing and the width it is centred by cannot
/// disagree.
fn row_text(offer: Offer, selected: bool) -> String {
    let marker = if selected { MARKER } else { NO_MARKER };
    if offer.locked {
        return format!("{marker}{LOCKED_LABEL}{SEPARATOR}{LOCKED_BLURB}");
    }
    format!(
        "{marker}{}{SEPARATOR}{}",
        offer.flavour.label(),
        offer.flavour.blurb(),
    )
}

/// The column the list starts at: the widest row centred, every row left-aligned inside
/// it. A ragged-right list reads as a list; centring each row on its own would make the
/// labels jitter as the marker moves (the title screen's rule, #268).
fn list_column(width: u32, ahead: &[Offer]) -> u32 {
    let widest = ahead
        .iter()
        .map(|&offer| row_text(offer, true).chars().count() as u32)
        .max()
        .unwrap_or(0);
    centre(width, widest)
}

/// Whether the list is the *"raid the one you are standing on"* single row rather than a
/// choice between successors — true exactly when the only offer names the current node
/// ([`Campaign::ahead`]).
///
/// The two read the same to the player, deliberately: a row says which facility and the
/// `@` on the picture says which one is under your feet, so there is no second wording to
/// learn. It matters only to the drawing, which must not paint an option *over* the
/// player's own glyph.
fn standing_here(run: &Campaign, ahead: &[Offer]) -> bool {
    ahead.len() == 1 && ahead[0].node == run.node()
}

/// Which [`MapHit`] screen cell `(x, y)` lands on, or `None` for a press that hit nothing
/// and is swallowed.
///
/// **The whole row is the target** at any column, with the blank row between entries as
/// the buffer — the title screen's geometry, because it is the same list. The theme
/// control is the one thing tested by column too, since it shares its row with the footer
/// prose. A press on the **picture** hits nothing on purpose: a node is one cell and one
/// cell is not a target a finger can hit (§11.6), and the row that names it is right
/// underneath.
#[must_use]
pub fn map_hit(width: u32, height: u32, run: &Campaign, x: u32, y: u32) -> Option<MapHit> {
    if height > 0 && y == height - 1 {
        let theme = theme_control_start(width);
        return (x >= theme && x < theme + theme_control_len()).then_some(MapHit::ToggleTheme);
    }
    let ahead = run.ahead();
    ahead
        .iter()
        .enumerate()
        .find(|&(i, offer)| !offer.locked && row_of(height, i) == y)
        .map(|(_, offer)| MapHit::Facility(offer.node))
}

/// Render the campaign map (§11.1/§14 v3) — the whole `width × height` screen, not an
/// overlay, so the shell paints it through the one path it paints a frame with and
/// nothing of the last facility shows behind it.
///
/// A pure view of campaign state: it mutates nothing, costs no turn (§4.4/§12.1), and
/// the same `(campaign, ui)` always draws the same grid — which is what makes the golden
/// tests below a real check on the picture.
///
/// Bounds are clamped, never asserted, like every other panel: on a board too small for
/// a row, that row shows what fits and stops.
pub fn render_map(width: u32, height: u32, run: &Campaign, ui: MapUi) -> Grid {
    let mut grid = blank_grid(width, height);
    let map_h = map_height(height);
    let map = run.map();
    let ahead = run.ahead();
    let here = standing_here(run, &ahead);
    let selected = ui.selected(&ahead);
    let drawn: Vec<NodeId> = run
        .path()
        .iter()
        .copied()
        .chain(ahead.iter().map(|o| o.node))
        .collect();

    let len = HEADING.chars().count() as u32;
    draw(
        &mut grid,
        centre(width, len),
        HEADING_ROW,
        HEADING,
        Category::System,
    );

    let at = |node: NodeId| plot(map.position(node), map.depth(), width, map_h);

    // **The road behind you first**, then the roads ahead, then the nodes over both — so
    // a facility is never overdrawn by an edge that happens to pass through its cell.
    for pair in run.path().windows(2) {
        trail(&mut grid, at(pair[0]), at(pair[1]));
    }
    let standing = run.node();
    if !here {
        for offer in &ahead {
            trail(&mut grid, at(standing), at(offer.node));
        }
    }

    // **The country itself**, under everything: every facility the run has not stood on
    // and is not being offered, as an outline. It is what turns a trail with a star at
    // the end into a map — you can see the room either side of your route, and how much
    // of the country a single run never touches.
    for depth in 0..=map.depth() {
        for lane in 0..LANES {
            let node = NodeId::at(depth, lane);
            if !drawn.contains(&node) {
                plot_node(&mut grid, at(node), UNKNOWN_GLYPH, Category::Ground);
            }
        }
    }

    // The archive, always, from the first frame — a run should know how far it has to go
    // (§14 v3: the map is not fogged). Drawn first of the nodes so that arriving *on* it
    // paints the player's own glyph over it rather than beside it.
    let archive = map.archive();
    plot_node(&mut grid, at(archive), '★', Category::Interest);

    // The route already walked: spent facilities, in the same Ground the board gives a
    // used console (§11.2) — they are still on the map, they are simply done with.
    for &node in run.path() {
        let glyph = flavour_glyph(map.flavour(node));
        plot_node(&mut grid, at(node), glyph, Category::Ground);
    }

    // What is on offer: Interest for the row the marker rests on — the thing worth
    // reaching for — Neutral for the other live options, Ground for the locked edge.
    if !here {
        for (i, offer) in ahead.iter().enumerate() {
            let (glyph, category) = match (offer.locked, i == selected) {
                (true, _) => (LOCKED_GLYPH, Category::Ground),
                (false, true) => (flavour_glyph(offer.flavour), Category::Interest),
                (false, false) => (flavour_glyph(offer.flavour), Category::Neutral),
            };
            plot_node(&mut grid, at(offer.node), glyph, category);
        }
    }

    // And where you stand, last of all: nothing may be drawn over the player (§11.3).
    plot_node(&mut grid, at(standing), HERE_GLYPH, Category::Owned);

    let column = list_column(width, &ahead);
    for (i, &offer) in ahead.iter().enumerate() {
        let category = match (offer.locked, i == selected) {
            (true, _) => Category::Ground,
            (false, true) => Category::Interest,
            (false, false) => Category::Neutral,
        };
        draw(
            &mut grid,
            column,
            row_of(height, i),
            &row_text(offer, i == selected),
            category,
        );
    }

    let footer_row = height.saturating_sub(1);
    draw(
        &mut grid,
        FOOTER_INDENT,
        footer_row,
        FOOTER,
        Category::Ground,
    );
    draw(
        &mut grid,
        theme_control_start(width),
        footer_row,
        &theme_control(),
        Category::System,
    );
    grid
}

/// Paint one node's glyph at `(x, y)`, if the cell is on the screen.
fn plot_node(grid: &mut Grid, (x, y): (u32, u32), glyph: char, category: Category) {
    draw(grid, x, y, &glyph.to_string(), category);
}

/// Draw the dotted road between two nodes — a straight run of [`EDGE_GLYPH`], **ends
/// excluded**, so the road joins the facilities without painting over either of them.
///
/// A plain integer walk along the longer axis rather than a full line algorithm: the
/// segments are short, the picture wants a suggestion of a road rather than a surveyed
/// one, and a cell either end left clear is what stops the trail from eating a glyph the
/// player is trying to read.
fn trail(grid: &mut Grid, from: (u32, u32), to: (u32, u32)) {
    let (x0, y0) = (from.0 as i32, from.1 as i32);
    let (x1, y1) = (to.0 as i32, to.1 as i32);
    let steps = (x1 - x0).abs().max((y1 - y0).abs());
    for step in 1..steps {
        let x = x0 + (x1 - x0) * step / steps;
        let y = y0 + (y1 - y0) * step / steps;
        draw(
            grid,
            x as u32,
            y as u32,
            &EDGE_GLYPH.to_string(),
            Category::Ground,
        );
    }
}
