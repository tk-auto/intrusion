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
//! - **THE JOB's objective** — what the exit will ask for ([`take_line`]) and the fact
//!   that there is only one way out of the building, which is the one you dug (§1/§4.5).
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
use super::{clear_row, draw, draw_rule, help, overlay_top, Grid};
use crate::category::Category;
use crate::modifiers::{IntelGate, LevelModifiers};
use crate::place::LevelConfig;

/// The card's own heading (§11.8): the run stated as the **job** it is.
///
/// Not *Level* (the help panel's tab bar), not *THIS RUN* (that tab's own heading) and
/// not *brief* (the campaign map's facility brief, §14 v3) — three words already in play
/// on three surfaces. It is also the one heading here that is not meta vocabulary: a
/// *seed* and a *level modifier* belong to the player choosing a run (§11.8), and the
/// job belongs to the intruder standing in the building.
const HEADING: &str = "THE JOB";

/// The objective section's heading — what the exit will ask for before it opens (§4.5).
const OBJECTIVE_HEADING: &str = "OBJECTIVE";

/// The way out, on every card whatever the gate says: §1's promise stated before the
/// first step rather than discovered at a wall — you came in by your own tunnel and
/// there is no other way out of the building (§4.5/§7.6).
const EXIT_LINE: &str = "Get back out through your tunnel";

/// What a facility with **no consoles** says. Rare — the §12.6 intel knob floors at
/// [`LevelConfig::INTEL_MIN`] — but reachable by a hand-built state, and a card that
/// silently dropped its objective row there would read as broken rather than as empty.
const NO_INTEL: &str = "There is no intel here";

/// The dismissal hint, in the vocabulary the player's hands are actually using
/// (§11.6/#323) — the same choice the usable line's floor makes, and for its reason: a
/// hint that teaches keys to a thumb is a hint nobody follows. Both work at all times.
const BEGIN_KEYS: &str = "any key to begin";
const BEGIN_TOUCH: &str = "tap to begin";

/// One drawn row of the card. The layout is built as a list of these once and then
/// drawn from it, the verdict's own shape — there is nothing to hit-test here, because
/// the card carries no control: *every* press dismisses it.
enum Row {
    /// The bounding rule, edge to edge.
    Rule,
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
fn rows(modifiers: LevelModifiers, intel: usize, modality: InputModality) -> Vec<Row> {
    let mut rows = vec![
        Row::Rule,
        Row::Blank,
        Row::heading(HEADING, Category::Interest),
        Row::Blank,
        Row::heading(OBJECTIVE_HEADING, Category::System),
    ];
    rows.push(objective_row(modifiers.intel_to_exit, intel));
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
    rows.push(Row::Rule);
    rows
}

/// The objective's first row: what the exit will ask for.
///
/// **It cannot be read off [`LevelModifiers::active`]** (§4.5/§12.6): that surfaces the
/// gate only when it is *non-baseline*, so a baseline run — the sim's, and any run whose
/// card says `none active` — would be handed a card with no objective on it at all. It
/// is derived from the run's own gate and console count instead, so **every** run gets a
/// positive statement of what it is being asked to do.
///
/// A facility with nothing to take says so under every gate: the exit is vacuously open
/// (an empty `all` is satisfied), and the row that would have named a number instead
/// names the emptiness.
fn objective_row(gate: IntelGate, intel: usize) -> Row {
    let category = if intel == 0 {
        // Nothing to reach for, so nothing wears the reward colour: the row recedes to
        // Ground, the same reading the modifier list's own "none active" row takes.
        Category::Ground
    } else {
        Category::Interest
    };
    Row::content(take_line(gate, intel), category)
}

/// What the gate asks for, in words — one line per §12.6 setting, since the three modes
/// want three different things of the same facility (§4.5/#244):
///
/// - [`All`](IntelGate::All) — quick play: every console, then out.
/// - [`AtLeastOne`](IntelGate::AtLeastOne) — the §4.5 baseline and the sim (§13.2): one
///   is a complete run, and pressing on for more is the aggressive style's trade rather
///   than a requirement, which is exactly what the row has to say.
/// - [`None`](IntelGate::None) — campaign (§14 v3): intel is currency (§2.2), not an
///   exit key. The row says both halves, because "no intel required" alone would read as
///   *there is no point taking any*, which is the opposite of true.
fn take_line(gate: IntelGate, intel: usize) -> String {
    match (gate, intel) {
        (_, 0) => NO_INTEL.to_string(),
        (IntelGate::All, 1) => "Take the one intel".to_string(),
        (IntelGate::All, n) => format!("Take all {n} intel"),
        (IntelGate::AtLeastOne, _) => "Take intel — one is enough".to_string(),
        (IntelGate::None, _) => "Take intel — none is required".to_string(),
    }
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
/// captions are bounded by [`super::modifier_rows`], and the objective's one variable
/// row — a console count — is measured over its whole envelope by
/// `no_objective_line_is_clipped_on_the_board`.
///
/// [`draw`] clips in silence, so a line that outgrew the board would simply arrive cut,
/// which is worse than a short one: it looks like the whole sentence (§2.3).
const _: () = {
    let room = (LevelConfig::V1.width - help::CONTENT_INDENT - 1) as usize;
    assert!(
        EXIT_LINE.len() <= room && NO_INTEL.len() <= room,
        "an objective line is too long for the level-start splash — shorten it",
    );
    let heading_room = (LevelConfig::V1.width - help::SECTION_INDENT - 1) as usize;
    assert!(
        HEADING.len() <= heading_room
            && OBJECTIVE_HEADING.len() <= heading_room
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
/// log drop their tails.
///
/// `intel` is the facility's **whole** console count rather than what is still out: the
/// card states the objective, not progress against it, and at level start the two are
/// the same number anyway.
pub(super) fn overlay_splash(
    grid: &mut Grid,
    modifiers: LevelModifiers,
    intel: usize,
    modality: InputModality,
) {
    let rows = rows(modifiers, intel, modality);
    let top = overlay_top(grid.height, rows.len());
    for (i, row) in rows.iter().enumerate() {
        let y = top + i as u32;
        if y >= grid.height {
            break;
        }
        clear_row(grid, y);
        match row {
            Row::Blank => {}
            Row::Rule => draw_rule(grid, y),
            Row::Text(text, category, indent) => draw(grid, *indent, y, text, *category),
        }
    }
}

#[cfg(test)]
mod tests;
