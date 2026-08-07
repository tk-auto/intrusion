//! The **level-start splash** (§11.4/§12.6/#497): what this raid is for, and what is
//! bending its rules, before the first turn is taken.
//!
//! A run used to start with the player already standing in the facility and nothing
//! having told them what *this* facility is. The rules bending the run were on the help
//! panel's Level info tab ([`super::help`]) — but only for a player who thought to press
//! `?` before their first step, which is exactly the player who does not need telling.
//!
//! # What it says, and what it deliberately does not
//!
//! Two sections, and nothing else:
//!
//! - **THE JOB's objective** — what the exit will ask for and the fact that there is
//!   only one way out of the building, which is the one you dug (§1/§4.5). Derived by
//!   [`objective`](super::objective), which the Level info tab draws from too, so the
//!   two cards cannot come to state the same gate differently.
//! - **MODIFIERS** — [`LevelModifiers::active`] through the one derivation every card
//!   draws ([`modifier_rows`](super::modifier_rows)), each row in its §11.2 direction
//!   colour, so a rule that is in force here is in force on the Level info tab too and
//!   neither can list one the other does not.
//!
//! It is a *reduced* Level info tab, and the omissions are the point. No **level-seed
//! token** and no `copy [c]`: those are the run's setup, read and shared at leisure from
//! a panel you call up, and this card is gone after one keypress. No **facility alert**
//! either — it is the one section of that tab which moves while you play (§7.3/#375), so
//! it belongs where it can be looked at again, and at level start it has nothing to say.
//!
//! # It waits, and it is never on a clock
//!
//! **There is no timeout, here or anywhere in the feature.** An auto-dismiss has a
//! losing race in it: the player presses a key at roughly the moment it fires, the card
//! is already gone, and the press is read as their first action — a move, an ability,
//! possibly a step into a cone, in a permadeath run with no undo (§2.1). Making the
//! window smaller does not close it. Input-only dismissal has no race at all: the card
//! waits, *every* input dismisses it, and the dismissing input is consumed rather than
//! also reaching the world ([`dismisses_splash`](crate::dismisses_splash)).
//!
//! Like the help panel it is pure **view** state ([`ScreenUi::splash_open`](super::ScreenUi)):
//! it changes no world and costs no turn (§4.4), so no guard moves under it — and it
//! stays escapable by construction (§11.6's no-trap rule), since there is no input that
//! does not dismiss it. Being view state is also what keeps a replay honest: the
//! dismissal is not an [`Input`](crate::Input), so it never enters the recorded stream
//! and a replay's identity stays `(seed, [inputs])` about the *game* (§12.4).
//!
//! # It is an overlay, not a scene
//!
//! §11.4 is **[SETTLED]** that the screen is the board. The card is laid over the middle
//! of the map exactly as the verdict is ([`super::verdict`]), so the facility the player
//! is about to walk into reads above and below it — and dismissing it changes what is
//! *drawn*, never the fit.

use super::hud::InputModality;
use super::modifier_rows::{modifier_rows, MODIFIERS_HEADING};
use super::objective::{objective_line, EXIT_LINE, OBJECTIVE_HEADING};
use super::{clear_row, draw, draw_rule, help, overlay_top, Grid};
use crate::category::Category;
use crate::modifiers::LevelModifiers;
use crate::place::LevelConfig;

/// The card's own heading (§11.8): the run stated as the **job** it is.
///
/// Not *Level* (the help panel's tab bar), not *THIS RUN* (that tab's own heading) and
/// not *brief* (the campaign map's facility brief, §14 v3) — three words already in play
/// on three surfaces. It is also the one heading here that is not meta vocabulary: a
/// *seed* and a *level modifier* belong to the player choosing a run (§11.8), and the
/// job belongs to the intruder standing in the building.
const HEADING: &str = "THE JOB";

/// The dismissal hint, in the vocabulary the player's hands are actually using
/// (§11.6/#323) — the same choice the usable line's floor makes, and for its reason: a
/// hint that teaches keys to a thumb is a hint nobody follows. Both work at all times.
const BEGIN_KEYS: &str = "any key to begin";
const BEGIN_TOUCH: &str = "tap to begin";

/// **The card is a box, not a pair of rules** (#497). Two horizontal rules alone read
/// as a *cut through the level* — the board above and the board below look like two
/// halves of the facility with something wedged between them — and the first frame of a
/// run is the worst possible place to be unsure what you are looking at. The sides and
/// the corners are what say *this is one object, laid on top*.
///
/// The horizontal run is the overlay family's own [`RULE_GLYPH`](super::RULE_GLYPH), and
/// these are its corners and its verticals: the same box-drawing block, so a font that
/// has the rule the verdict and the deployed log already draw has these too.
const CORNER_TOP_LEFT: char = '┌';
const CORNER_TOP_RIGHT: char = '┐';
const CORNER_BOTTOM_LEFT: char = '└';
const CORNER_BOTTOM_RIGHT: char = '┘';
const SIDE_GLYPH: char = '│';

/// One drawn row of the card. The layout is built as a list of these once and then
/// drawn from it, the verdict's own shape — there is nothing to hit-test here, because
/// the card carries no control: *every* press dismisses it.
enum Row {
    /// The box's top edge — corners and the rule between them.
    Top,
    /// Its bottom edge.
    Bottom,
    /// A blank row of card — the surface, with the board hidden behind it.
    Blank,
    /// A row of text at `indent`, in its own §11.2 category.
    Text(String, Category, u32),
}

impl Row {
    /// A section heading, at the panels' standing heading indent — `THE JOB`'s own
    /// columns, and the Level info tab's, so the two cards read as one family.
    fn heading(text: &str, category: Category) -> Self {
        Row::Text(text.to_string(), category, help::SECTION_INDENT)
    }

    /// A row of content, one column in from the heading that names it — the indent the
    /// modifier captions are bounded against.
    fn content(text: String, category: Category) -> Self {
        Row::Text(text, category, help::CONTENT_INDENT)
    }
}

/// The card's rows for this run, top to bottom.
///
/// Shaped so it can only grow **downward**: every row is one line, the objective is at
/// most two, and the modifier rows are bounded at the caption width the whole set is
/// checked against at compile time ([`super::modifier_rows`]).
fn rows(
    modifiers: LevelModifiers,
    intel: usize,
    caches: usize,
    modality: InputModality,
) -> Vec<Row> {
    let mut rows = vec![
        Row::Top,
        Row::Blank,
        Row::heading(HEADING, Category::Interest),
        Row::Blank,
        Row::heading(OBJECTIVE_HEADING, Category::System),
    ];
    let (text, category) = objective_line(modifiers.intel_to_exit, intel, caches);
    rows.push(Row::content(text, category));
    // Interest is the goal-and-reward colour (§11.2), and the way out is the goal every
    // run shares: the exit is the one piece of this building that is yours (§4.5), drawn
    // in the same tint the board gives your own tunnel.
    rows.push(Row::content(EXIT_LINE.to_string(), Category::Interest));
    rows.push(Row::Blank);
    rows.push(Row::heading(MODIFIERS_HEADING, Category::System));
    for (text, category) in modifier_rows(modifiers) {
        rows.push(Row::content(text, category));
    }
    rows.push(Row::Blank);
    rows.push(Row::heading(begin_hint(modality), Category::Ground));
    rows.push(Row::Blank);
    rows.push(Row::Bottom);
    rows
}

/// How to dismiss, in the modality the shell says the player is using (§11.6/#323).
fn begin_hint(modality: InputModality) -> &'static str {
    match modality {
        InputModality::Keys => BEGIN_KEYS,
        InputModality::Touch => BEGIN_TOUCH,
    }
}

/// **The card's rows fit the board they are drawn on.** Every fixed line is measured at
/// compile time against the narrowest screen a real run renders on (the v1 board, 40
/// wide — §10.2) with the one-column right margin every card keeps; the modifier
/// captions are bounded by [`super::modifier_rows`] and the objective's lines by
/// [`super::objective`].
///
/// [`draw`] clips in silence, so a line that outgrew the board would simply arrive cut,
/// which is worse than a short one: it looks like the whole sentence (§2.3).
const _: () = {
    let heading_room = (LevelConfig::V1.width - help::SECTION_INDENT - 1) as usize;
    assert!(
        HEADING.len() <= heading_room
            && BEGIN_KEYS.len() <= heading_room
            && BEGIN_TOUCH.len() <= heading_room,
        "a level-start splash heading is too long for the board — shorten it",
    );
};

/// Lay the card over the frame (§11.4/#497) — over the board and the chrome, because
/// until it is dismissed there is nothing underneath it the player may act on.
///
/// Every row it uses is **cleared first**, so the facility never reads through the
/// words; rows past the frame's end are dropped, exactly as the verdict and the deployed
/// log drop their tails. Each cleared row then gets its **sides** ([`draw_sides`]), which
/// is what makes the block one boxed object rather than a horizontal slice out of the
/// level.
///
/// `intel` and `caches` are the facility's **whole** counts rather than what is still
/// out: the card states the objective, not progress against it, and at level start the
/// two are the same number anyway.
pub(super) fn overlay_splash(
    grid: &mut Grid,
    modifiers: LevelModifiers,
    intel: usize,
    caches: usize,
    modality: InputModality,
) {
    let rows = rows(modifiers, intel, caches, modality);
    let top = overlay_top(grid.height, rows.len());
    for (i, row) in rows.iter().enumerate() {
        let y = top + i as u32;
        if y >= grid.height {
            break;
        }
        clear_row(grid, y);
        match row {
            Row::Blank => draw_sides(grid, y),
            Row::Top => draw_edge(grid, y, CORNER_TOP_LEFT, CORNER_TOP_RIGHT),
            Row::Bottom => draw_edge(grid, y, CORNER_BOTTOM_LEFT, CORNER_BOTTOM_RIGHT),
            Row::Text(text, category, indent) => {
                draw(grid, *indent, y, text, *category);
                draw_sides(grid, y);
            }
        }
    }
}

/// Draw one of the box's horizontal edges: the corners, and
/// [`RULE_GLYPH`](super::RULE_GLYPH) between them.
///
/// The rule is drawn first and the corners over its ends, so the edge is exactly the row
/// [`draw_rule`] already draws with its two outermost cells replaced — one width
/// arithmetic, not two.
fn draw_edge(grid: &mut Grid, y: u32, left: char, right: char) {
    draw_rule(grid, y);
    put(grid, 0, y, left);
    put(grid, grid.width.saturating_sub(1), y, right);
}

/// Draw the box's verticals on row `y` — the first and last cell of the card.
///
/// They cost the content nothing: every column of the card is drawn from
/// [`SECTION_INDENT`](help::SECTION_INDENT) or further in, and the modifier captions are
/// bounded to leave the last column free (the one-cell right margin every card keeps), so
/// the sides land on cells no row was using.
///
/// A grid too narrow to hold both — only a hand-built test state gets that small — draws
/// the one it can, which is [`put`]'s clamp rather than a rule of its own.
fn draw_sides(grid: &mut Grid, y: u32) {
    put(grid, 0, y, SIDE_GLYPH);
    put(grid, grid.width.saturating_sub(1), y, SIDE_GLYPH);
}

/// Write one frame glyph, in the System tan the overlay family's rule already wears —
/// the card's frame is furniture, and the words on it are what carry the §11.2 meaning.
fn put(grid: &mut Grid, x: u32, y: u32, glyph: char) {
    draw(grid, x, y, &glyph.to_string(), Category::System);
}

#[cfg(test)]
mod tests;
