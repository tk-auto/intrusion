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
//! where you stand (it is yours) and for the intel in the wallet (likewise), **Interest**
//! for the archive and for the row the marker rests on (the thing worth reaching for),
//! **Neutral** for a live option, **Ground** for the road behind you, the facilities you
//! have spent, and the locked edge you cannot take yet.
//!
//! # It is also the hub (§14 v3/#211)
//!
//! Intel is the run's currency and this is the one screen it is spent on, so the balance
//! is a line of the picture rather than a panel somewhere else: the prices and the purse
//! are read in one glance. There is no separate shop — a second modal screen carrying one
//! list would be a screen to learn for no fact it could not have said here.

use super::alert::condition_line;
use super::help::{theme_control, theme_control_len, theme_control_start, FOOTER_INDENT};
use super::menu::{centre, ENTRY_SPACING, MARKER, NO_MARKER};
use super::{blank_grid, draw, Grid};
use crate::alert::TOP_RUNG;
use crate::campaign::map::{DEPTH_SPACING, LANES, LANE_SPACING};
use crate::campaign::{
    Campaign, Flavour, Loudness, MapPos, NodeId, Offer, Outlay, ROUTE_UNLOCK_COST,
};
use crate::category::Category;
use crate::modifiers::ModifierDirection;
use crate::place::LevelConfig;

pub mod brief;
#[cfg(test)]
mod tests;

pub use brief::{brief_rows, render_brief, BriefRow};

/// The heading, in §11.8's **meta vocabulary** — it names the game around the run, so it
/// reads as plainly as the title screen's own rows.
const HEADING: &str = "THE FACILITY MAP";

/// The footer. It names both input paths (§11.6), like the menu's: a touch player who
/// cannot see a keyboard still reads that tapping a row raids it.
const FOOTER: &str = "↑↓ choose · Enter/tap raids";

/// What an intel-locked row says (§14 v3's alternative-route sink, #212) — the label, and
/// its **price** in the currency's own word.
///
/// The number comes from [`ROUTE_UNLOCK_COST`] rather than from a literal here, so the
/// screen cannot come to advertise a price the campaign does not charge. The blurb is
/// built at runtime ([`locked_blurb`]) for the same reason the wallet line is: a `const`
/// cannot format one.
const LOCKED_LABEL: &str = "Alternative route";

/// How many digits a price may take before the row-width bound stops being true. Four is
/// far past anything a 2–3 hour run can bank, and it is asserted rather than assumed.
const PRICE_DIGITS: usize = 4;

/// See [`LOCKED_LABEL`] — the price as the row prints it.
fn locked_blurb() -> String {
    format!("{ROUTE_UNLOCK_COST} intel")
}

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

/// The screen row the heading sits on, the row the campaign alert reports on, the row the
/// **wallet** reports on, and the first row of the map band beneath them.
///
/// The alert takes the blank that used to separate the heading from the picture, so the
/// band keeps every row it had: the line is a **subtitle** — what the country ahead
/// currently is — rather than a panel the map has to make room for. It is drawn only
/// when there is a raid behind the run to report on ([`alert_text`]), and the layout does
/// not move when it is not: a map band that changed height between two looks at the same
/// screen would be a picture that jumped.
///
/// The wallet line (#211) costs the band a real row, and it is a **fixed** one for the
/// same reason: it is drawn on every frame, balance zero included, so the picture below it
/// never moves. A readout that appeared with the run's first haul would make the map jump
/// exactly once, at the moment the player was reading it.
const HEADING_ROW: u32 = 0;
const ALERT_ROW: u32 = 1;
const WALLET_ROW: u32 = 2;
const MAP_TOP: u32 = 3;

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
    // The priced row, measured at the widest price the bound admits (`{n} intel`).
    let mut widest = row_width(LOCKED_LABEL.len(), PRICE_DIGITS + " intel".len());
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

/// How the alert line names a run that nobody ever noticed (§7.3 condition 0/#210).
///
/// It does **not** reuse the Level info tab's rung-0 wording (`no alert — you are
/// unnoticed`): that line is about the facility you are standing in, in the present
/// tense, and this one is about the raid you have walked out of. Same fact, two tenses,
/// and the map is the one that has to be in the past.
const LEFT_UNNOTICED: &str = "Left unnoticed";

/// What the alert line says when the last raid's noise carried nothing onward (§7.3
/// condition 1) — the one loudness with no consequence, said plainly rather than left as
/// a blank row the player would read as a bug.
const NOTHING_FOLLOWS: &str = "nothing follows";

/// What it says when the noise *did* carry, but not onto anything the list is showing:
/// at a choice point, the road it settled on is one of the others; on the approach, the
/// facility under your feet is not the one it settled on.
const NOTHING_AHEAD: &str = "nothing ahead";

/// What it says at the top of the ladder, when there is no unwatched road left to steer
/// toward — the whole of the step from condition 2 to condition 3.
const ALL_ALERTED: &str = "all ahead alerted";

/// How the line names what the noise did to the one facility it settled on. The two
/// directions of the §12.6 seam, in the world's own words: a facility that is *expecting
/// you*, or one that is not looking.
const ALERTED: &str = "alerted";
/// See [`ALERTED`].
const OFF_GUARD: &str = "off guard";

/// The width of the panel's condition line (`Condition 2 of 3`) — what the alert line
/// leads with for every loudness but [`LEFT_UNNOTICED`], and the one part of the line
/// whose text is built at runtime.
///
/// Measured off a literal of the same shape rather than off the function, because a
/// `const` cannot call [`condition_line`]. Both numbers are single digits and the assert
/// below is what keeps that true: a ladder that grew a tenth rung would fail the build
/// here rather than clip a line on a player's screen.
const CONDITION_LEN: usize = "Condition 0 of 0".len();
const _: () = assert!(TOP_RUNG < 10, "a two-digit rung would widen the alert line");

/// How the wallet line names the currency (§11.8: *intel* is already the world's word, so
/// there is nothing to translate) — the readout that makes the map the run's **hub**
/// (§14 v3/#211).
///
/// It is here and not in the level's HUD because a campaign's intel is not a thing you
/// carry through a facility: inside one it is the raid's own count, and only a completed
/// raid banks it. The balance is what the *next* decision is made against, so it belongs
/// on the screen that decision is made on.
const WALLET_LABEL: &str = "Intel";

/// What the wallet line says when the run has nothing banked — the whole of the currency's
/// starting state, said rather than left as a bare `Intel 0` the player has to interpret.
///
/// Its own wording because zero is the interesting case for a run that has just walked out
/// of a facility empty-handed: nothing is taken away for that (appendix 47), and the line
/// saying so plainly is the only feedback there is.
const WALLET_EMPTY: &str = "nothing banked";

/// The widest the wallet line can ever be, in cells. The balance is bounded in practice by
/// the consoles a run can carry out, not by the type, so the bound reserves a sane number
/// of digits and asserts the wording fits around them rather than pretending a `u32` could
/// not be wider.
const WALLET_DIGITS: usize = 4;
const WALLET_LINE_MAX: usize = {
    let counted = WALLET_LABEL.len() + 1 + WALLET_DIGITS;
    let empty = WALLET_LABEL.len() + SEPARATOR.len() + WALLET_EMPTY.len();
    if counted > empty {
        counted
    } else {
        empty
    }
};

/// The wallet line fits the board too (§10.2/§11.4), on the terms every other line here
/// does.
const _: () = assert!(
    WALLET_LINE_MAX <= LevelConfig::V1.width as usize,
    "the wallet line must fit the v1 board (§10.2): shorten its wording",
);

/// The widest the alert line can ever be, in cells — the widest lead against the widest
/// tail, with the widest flavour label standing in the tail that names one.
///
/// Byte lengths, like [`row_width`]: `len()` is what a `const` can see, and it can only
/// **over**-count against a multi-byte glyph, so the bound fails early rather than late.
const ALERT_LINE_MAX: usize = {
    let lead = if CONDITION_LEN > LEFT_UNNOTICED.len() {
        CONDITION_LEN
    } else {
        LEFT_UNNOTICED.len()
    };
    let named = widest_label()
        + 1
        + if ALERTED.len() > OFF_GUARD.len() {
            ALERTED.len()
        } else {
            OFF_GUARD.len()
        };
    let mut tail = named;
    if NOTHING_FOLLOWS.len() > tail {
        tail = NOTHING_FOLLOWS.len();
    }
    if NOTHING_AHEAD.len() > tail {
        tail = NOTHING_AHEAD.len();
    }
    if ALL_ALERTED.len() > tail {
        tail = ALL_ALERTED.len();
    }
    lead + SEPARATOR.len() + tail
};

/// The longest flavour label there is — the widest thing the alert line can name.
const fn widest_label() -> usize {
    let mut widest = 0;
    let mut i = 0;
    while i < Flavour::ALL.len() {
        let label = Flavour::ALL[i].label().len();
        if label > widest {
            widest = label;
        }
        i += 1;
    }
    widest
}

/// The alert line fits the board too (§10.2/§11.4), on the same terms its rows do.
const _: () = assert!(
    ALERT_LINE_MAX <= LevelConfig::V1.width as usize,
    "the campaign alert line must fit the v1 board (§10.2): shorten its wording",
);

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
/// Two fields, and both are things the player can change without the campaign moving:
/// which facility the marker rests on, and what the hub last said back to them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MapUi {
    /// Which row the marker rests on, as an index into [`Campaign::ahead`]. Clamped at
    /// use rather than trusted ([`selected`](Self::selected)): the offer list changes
    /// under it every time the run moves, and a marker resting past the end of a shorter
    /// list would draw nothing and activate nothing.
    pub selected: usize,
    /// **What the last spend at the hub did** (#211/#212/#215), or `None` when nothing has
    /// been bought or refused since the marker last moved.
    ///
    /// It rides on the view state rather than on the campaign because it is a *message*,
    /// not a fact about the run: the run's own record of a purchase is the road it can now
    /// take. Cleared by any move of the marker ([`seek`](Self::seek)) — a refusal still
    /// sitting there while the player walks the list would be the screen answering a
    /// question nobody had just asked.
    pub outlay: Option<Outlay>,
    /// **Which of the map's two screens is showing** (#215) — the list of facilities, or
    /// the brief for one of them. The map's own [`MenuScreen`](super::MenuScreen): a
    /// sub-screen rather than a second surface, because everything above the rows is the
    /// same picture.
    pub screen: MapScreen,
    /// Which row of the **brief** the marker rests on, clamped at use like
    /// [`selected`](Self::selected) — and kept apart from it so that leaving the brief
    /// puts the marker back on the facility it was opened from rather than wherever the
    /// brief's own list had reached.
    pub brief_row: usize,
}

/// Which surface of the map screen is up (#215) — the map's counterpart of
/// [`MenuScreen`](super::MenuScreen), and the same shape for the same reason: the two
/// screens answer different questions about the same country, and a pair of flags could
/// claim both at once.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MapScreen {
    /// **The list of facilities** — the map's root, where a run picks which way to walk.
    /// There is nowhere further back from it (§2.2: the last facility is gone).
    #[default]
    List,
    /// **The facility brief** for one node — what may be done about the facility the
    /// marker singled out: raid it, or spend intel to scout it first (#215). Carries the
    /// **node**, not a row index, for [`MapHit`]'s reason: the offer list can change under
    /// a screen that is already drawn.
    Brief(NodeId),
}

impl MapUi {
    /// The row the marker actually rests on, given what is `ahead` — the clamp every
    /// caller shares, so the drawing, the hit-test and the activation cannot disagree
    /// about which row is selected.
    ///
    /// **Every row is restable**, the locked one included (#212). It used to step over
    /// the lock, on the menu's rule that the marker only rests where Enter does something
    /// (#268) — and the rule has not changed, the row has: an intel-locked row is now a
    /// **price**, and pressing Enter on it buys the road or says why it cannot. A row that
    /// answers is a row the marker belongs on.
    pub fn selected(self, ahead: &[Offer]) -> usize {
        if self.selected < ahead.len() {
            return self.selected;
        }
        0
    }

    /// The marker moved one row `step`-wards on whichever screen is up, wrapping, and the
    /// hub's last word dropped — a message about the row you have just left is a message
    /// about nothing.
    fn seek(self, len: usize, step: usize) -> Self {
        if len == 0 {
            return Self {
                outlay: None,
                ..self
            };
        }
        let moved = (self.row(len) + step) % len;
        match self.screen {
            MapScreen::List => Self {
                selected: moved,
                outlay: None,
                ..self
            },
            MapScreen::Brief(_) => Self {
                brief_row: moved,
                outlay: None,
                ..self
            },
        }
    }

    /// The row the marker rests on for the screen that is up, given how many rows it has
    /// — the clamp the drawing, the hit-test and the activation share.
    fn row(self, len: usize) -> usize {
        let raw = match self.screen {
            MapScreen::List => self.selected,
            MapScreen::Brief(_) => self.brief_row,
        };
        if raw < len {
            return raw;
        }
        0
    }

    /// How many rows the screen that is up is showing — the list's offers, or the brief's
    /// controls.
    fn rows(self, run: &Campaign) -> usize {
        match self.screen {
            MapScreen::List => run.ahead().len(),
            MapScreen::Brief(node) => brief_rows(run, node).len(),
        }
    }

    /// The next row down, wrapping.
    #[must_use]
    pub fn next(self, run: &Campaign) -> Self {
        self.seek(self.rows(run), 1)
    }

    /// The previous row up, wrapping.
    #[must_use]
    pub fn prev(self, run: &Campaign) -> Self {
        let len = self.rows(run);
        self.seek(len, len.saturating_sub(1))
    }

    /// **Open the brief** for `node` (#215) — the screen a facility row activates into,
    /// with the marker on its first row and the hub's last word dropped.
    #[must_use]
    pub fn opening(self, node: NodeId) -> Self {
        Self {
            screen: MapScreen::Brief(node),
            brief_row: 0,
            outlay: None,
            ..self
        }
    }

    /// **Back to the list** (#215) — the brief closed, the marker left exactly where it
    /// was on the map, and the hub's last word kept: a refusal the player has just read is
    /// still the answer to the thing they just did.
    #[must_use]
    pub fn closing(self) -> Self {
        Self {
            screen: MapScreen::List,
            brief_row: 0,
            ..self
        }
    }

    /// The same screen with the hub's answer to a spend on it (#212/#215) — what the shell
    /// sets after [`Campaign::unlock`](crate::Campaign::unlock) or
    /// [`Campaign::scout`](crate::Campaign::scout), paid or refused alike.
    #[must_use]
    pub fn saying(self, outlay: Outlay) -> Self {
        Self {
            outlay: Some(outlay),
            ..self
        }
    }
}

/// What a press on the map screen lands on (§11.6) — the touch half of
/// [`map_nav_for_key`](crate::map_nav_for_key), and the map's counterpart of
/// [`MenuHit`](super::MenuHit).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapHit {
    /// A row of the list — **open that facility's brief** (#215). Carries the **node**,
    /// not the row index: the shell acts on a facility, and an index would have to be
    /// re-resolved against a list that may have changed since the frame was drawn.
    ///
    /// It used to raid outright, and moving the raid one screen inward is the point of the
    /// brief: the irreversible press (§2.1) now sits behind a screen that says what it will
    /// cost and what may be bought first, rather than under a stray tap on the map.
    Facility(NodeId),
    /// The **intel-locked** row — buy the road rather than walk down it (§14 v3/#212).
    ///
    /// Its own variant rather than a [`Facility`](Self::Facility) the shell has to
    /// re-classify: the two rows look alike and do opposite things, and a shell that
    /// worked out which by asking the campaign again would be answering from a list that
    /// may have moved since the frame was drawn.
    Unlock(NodeId),
    /// **Raid the facility** the brief is for (#215) — the one irreversible press on
    /// either screen (§2.1), and now the only thing that is.
    Enter(NodeId),
    /// **Scout the facility** the brief is for (§11.5a/#215): buy its contents, drawn
    /// remembered from turn one.
    Scout(NodeId),
    /// **Close the brief** and go back to the list — the drawn, tappable way out §11.6
    /// wants beside the way on, so the sub-screen is never one a touch player can open and
    /// not leave.
    Back,
    /// The footer's `theme [n]` control (§11.2/#189), in the same corner it keeps on
    /// every other screen.
    ToggleTheme,
}

/// The glyph a facility of this flavour is drawn with on the map.
///
/// Three of them are borrowed from the board's own vocabulary on purpose (§11.3): `$` is
/// the intel console, so a **Vault** reads as *the place with the loot in it* without a
/// legend; `¤` is the equipment cache (#209), so a **Workshop** is *the place with the
/// crate in it* on exactly the same terms; and `★` is the one glyph on this screen the
/// game does not use elsewhere, which is right for the one place a run is trying to
/// reach.
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
        Flavour::Workshop => '¤',
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
        return format!("{marker}{LOCKED_LABEL}{SEPARATOR}{}", locked_blurb());
    }
    format!(
        "{marker}{}{SEPARATOR}{}",
        offer.flavour.label(),
        offer.flavour.blurb(),
    )
}

/// **What the last raid left on the ground ahead** (§14 v3/#210), as the line under the
/// heading says it — the text and the §11.2 meaning it is drawn in, or `None` before the
/// run has finished a raid to report on.
///
/// This is the readout that keeps the campaign alert from being the decoration §14 v3
/// complains about. The rule it reports is legible twice over: *inside* the facility the
/// help panel's Level info tab lists it with every other active modifier, derived from
/// the same resolved set (§12.6/#248); *here* is the half that has to arrive **before**
/// the choice, because routing around an alerted facility is the play at condition 2 and
/// a player who learns which road was watched after walking down it has been told
/// nothing.
///
/// **It names the facility by its flavour**, and that is unambiguous by construction: no
/// two open successors ever share one (§14 v3 **[SETTLED]**), so *the Vault is alerted*
/// picks out exactly one row of the list. Naming it is what a 40-column row cannot do —
/// the widest offer already spends 38 of the board's 40 cells — so the mark lives on a
/// line of its own rather than as a second glyph nothing has room for.
///
/// The tail is decided by **what the list is showing**, not by re-deriving the mapping:
/// one reached offer is named, several are *all ahead*, none is *nothing ahead*. That is
/// what makes the line true in both live stages — a choice point showing a fan, and an
/// approach showing the single facility the run has already picked.
fn alert_text(run: &Campaign, ahead: &[Offer]) -> Option<(String, Category)> {
    let loudness = run.loudness()?;
    let lead = match loudness {
        Loudness::Unnoticed => LEFT_UNNOTICED.to_string(),
        _ => condition_line(run.alert()),
    };
    let reached: Vec<Flavour> = ahead
        .iter()
        .filter(|offer| run.alert_reaches(offer.node).is_some())
        .map(|offer| offer.flavour)
        .collect();
    let (tail, category) = match (loudness.direction(), reached.as_slice()) {
        // Condition 1: the raid was noticed and the facility kept it to itself.
        (None, _) => (NOTHING_FOLLOWS.to_string(), Category::Ground),
        // It carried, but not onto anything on this list — a fact about the run, not a
        // threat, so it is Ground like the road behind you.
        (Some(_), []) => (NOTHING_AHEAD.to_string(), Category::Ground),
        (Some(direction), [flavour]) => (
            format!("{} {}", flavour.label(), suffix(direction)),
            direction_category(direction),
        ),
        (Some(direction), _) => (ALL_ALERTED.to_string(), direction_category(direction)),
    };
    Some((format!("{lead}{SEPARATOR}{tail}"), category))
}

/// **What the run has to spend** (§2.2/§14 v3/#211), as the line under the alert says it.
///
/// The map is the campaign's **hub**: it is where intel is spent, and a currency whose
/// balance is not on the screen the prices are on is a currency the player has to keep in
/// their head. So the line is unconditional — a run that has banked nothing says so
/// ([`WALLET_EMPTY`]) rather than showing no line at all, which would read as the readout
/// being broken rather than as the wallet being empty.
///
/// **Owned**, not Interest (§11.2): the intel in the wallet is already yours, in the same
/// sense the `@` on the picture is. Interest is what is worth reaching for, and it is
/// spoken for on this screen by the archive and by the marked row — a third claim on it
/// would blunt both.
/// **The hub's last word replaces the balance**, and that is not a compromise for want of
/// a row (#212). Every [`Outlay::message`] already names the balance — *spent 1 intel — 3
/// left*, *needs 1 intel — you have 0* — so the line still answers the question the
/// readout answers, and it answers the one the player just asked as well. Two lines saying
/// the balance twice would be the screen repeating itself.
///
/// A refusal is **Warning** and a purchase **Owned**: the same two meanings the map already
/// gives a rule bent against you and a thing that is yours (§11.2), so there is no third
/// colour to learn.
fn wallet_text(run: &Campaign, ui: MapUi) -> (String, Category) {
    if let Some(outlay) = ui.outlay {
        let category = if outlay.paid() {
            Category::Owned
        } else {
            Category::Warning
        };
        return (outlay.message(), category);
    }
    let line = match run.intel() {
        0 => format!("{WALLET_LABEL}{SEPARATOR}{WALLET_EMPTY}"),
        banked => format!("{WALLET_LABEL} {banked}"),
    };
    (line, Category::Owned)
}

/// What the line says the noise did to the facility it names.
fn suffix(direction: ModifierDirection) -> &'static str {
    match direction {
        ModifierDirection::Harder => ALERTED,
        ModifierDirection::Easier => OFF_GUARD,
    }
}

/// The §11.2 meaning a bent rule carries — the **same** cue the help card gives the same
/// modifier (§12.6/#248), so the colour a player learns on one screen reads the same on
/// the other: Warning for a threat that is hunting, Owned for a rule bent your way.
fn direction_category(direction: ModifierDirection) -> Category {
    match direction {
        ModifierDirection::Harder => Category::Warning,
        ModifierDirection::Easier => Category::Owned,
    }
}

/// The §11.2 meaning a row carries, and the glyph of the facility it names — one function
/// so the picture and the list cannot come to disagree about what an option currently is.
///
/// **Interest for the marked row** wherever the marker rests, because that is what the
/// marker means on every screen. Off the marker, an open road is **Neutral** — a live
/// option — and the priced one is Neutral too **when the run can afford it**: it is as
/// live as the others, and the row's own text says what it costs.
///
/// **Ground when it cannot** (#212). Ground is what this screen already gave the locked
/// edge and what it gives the road behind you: *on the map, not available to you*. So a
/// price the wallet cannot meet reads as unaffordable at a glance rather than only when
/// pressed — the §2.3 courtesy of showing a cost before charging for the discovery.
fn row_category(run: &Campaign, offer: Offer, marked: bool) -> Category {
    if marked {
        return Category::Interest;
    }
    if offer.locked && !run.affords(ROUTE_UNLOCK_COST) {
        return Category::Ground;
    }
    Category::Neutral
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
///
/// It answers for whichever screen `ui` says is up (#215), so a tap and a keypress on one
/// row cannot come to mean different things — and neither can a tap on the map and a tap
/// on the brief drawn over it.
#[must_use]
pub fn map_hit(width: u32, height: u32, run: &Campaign, ui: MapUi, x: u32, y: u32) -> Option<MapHit> {
    if height > 0 && y == height - 1 {
        let theme = theme_control_start(width);
        return (x >= theme && x < theme + theme_control_len()).then_some(MapHit::ToggleTheme);
    }
    if let MapScreen::Brief(node) = ui.screen {
        return brief::brief_hit(height, run, node, y);
    }
    let ahead = run.ahead();
    ahead
        .iter()
        .enumerate()
        .find(|&(i, _)| row_of(height, i) == y)
        .map(|(_, offer)| hit_of(*offer))
}

/// What **Enter** does on whichever screen is up — the key half of [`map_hit`], and the
/// one place a row's meaning is decided for both input paths (§11.6).
///
/// `None` when the marked row is not there to be pressed: an empty offer list, or a brief
/// whose rows have changed under a marker resting past the end of them.
#[must_use]
pub fn map_activation(run: &Campaign, ui: MapUi) -> Option<MapHit> {
    match ui.screen {
        MapScreen::List => {
            let ahead = run.ahead();
            ahead.get(ui.row(ahead.len())).copied().map(hit_of)
        }
        MapScreen::Brief(node) => {
            let rows = brief_rows(run, node);
            rows.get(ui.row(rows.len())).map(|row| row.hit(node))
        }
    }
}

/// What activating a **list** row does — open its brief (#215), or buy the road to it
/// (§14 v3/#212). Shared by the tap path and the key path, which is what keeps the two
/// saying the same thing about one row.
#[must_use]
pub fn hit_of(offer: Offer) -> MapHit {
    if offer.locked {
        MapHit::Unlock(offer.node)
    } else {
        MapHit::Facility(offer.node)
    }
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
    if let MapScreen::Brief(node) = ui.screen {
        return render_brief(width, height, run, ui, node);
    }
    let ahead = run.ahead();
    let selected = ui.row(ahead.len());
    let marked = ahead.get(selected).map(|offer| offer.node);
    let mut grid = picture(width, height, run, ui, marked);

    let column = list_column(width, &ahead);
    for (i, &offer) in ahead.iter().enumerate() {
        let category = row_category(run, offer, i == selected);
        draw(
            &mut grid,
            column,
            row_of(height, i),
            &row_text(offer, i == selected),
            category,
        );
    }

    footer(&mut grid, FOOTER);
    grid
}

/// **The half of the screen both surfaces share** (#215): the heading, the alert line, the
/// wallet line and the country itself, with `marked` drawn as the one facility currently
/// in question.
///
/// It is a *picture*: it answers "where", not "which" (see the module docs). Splitting it
/// out is what lets the brief be a sub-screen rather than a second surface — the player
/// keeps the map they were reading, and only the rows beneath it change.
///
/// Bounds are clamped, never asserted, like every other panel: on a board too small for a
/// row, that row shows what fits and stops.
pub(super) fn picture(
    width: u32,
    height: u32,
    run: &Campaign,
    ui: MapUi,
    marked: Option<NodeId>,
) -> Grid {
    let mut grid = blank_grid(width, height);
    let map_h = map_height(height);
    let map = run.map();
    let ahead = run.ahead();
    let here = standing_here(run, &ahead);
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

    // What the last raid left on the ground ahead (§14 v3/#210) — the subtitle, drawn
    // only once there is a raid behind the run to report on.
    if let Some((line, category)) = alert_text(run, &ahead) {
        let len = line.chars().count() as u32;
        draw(&mut grid, centre(width, len), ALERT_ROW, &line, category);
    }

    // What the run has to spend (§2.2/#211) — always, so the hub's balance is never a
    // thing the player has to remember.
    let (wallet, wallet_category) = wallet_text(run, ui);
    let len = wallet.chars().count() as u32;
    draw(
        &mut grid,
        centre(width, len),
        WALLET_ROW,
        &wallet,
        wallet_category,
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

    // What is on offer: Interest for the facility currently in question — the row the
    // marker rests on, or the one the brief is about — Neutral for the other live
    // options, Ground for the locked edge.
    if !here {
        for offer in ahead.iter() {
            let glyph = if offer.locked {
                LOCKED_GLYPH
            } else {
                flavour_glyph(offer.flavour)
            };
            let category = row_category(run, *offer, Some(offer.node) == marked);
            plot_node(&mut grid, at(offer.node), glyph, category);
        }
    }

    // And where you stand, last of all: nothing may be drawn over the player (§11.3).
    plot_node(&mut grid, at(standing), HERE_GLYPH, Category::Owned);
    grid
}

/// The footer both surfaces draw: the screen's own prose on the left, and the theme
/// control in the corner it keeps everywhere (§11.2/#189).
pub(super) fn footer(grid: &mut Grid, hint: &str) {
    let row = grid.height.saturating_sub(1);
    draw(grid, FOOTER_INDENT, row, hint, Category::Ground);
    draw(
        grid,
        theme_control_start(grid.width),
        row,
        &theme_control(),
        Category::System,
    );
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
