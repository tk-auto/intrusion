//! The end screen (§14 v2/#138): why the run ended, what it came to, and the ways on.
//!
//! §14 v2 asks for "a game-over screen that says **why you lost** (the old one didn't
//! distinguish victory from defeat at all)". This is that screen, and its loss half is
//! where §2.2's promise is kept or broken: permadeath is only fair if every capture is
//! traceable to a decision the player made, and this is the surface the player does
//! the tracing on.
//!
//! # It is an overlay, not a scene
//!
//! The panel is laid over the **finished board**, the way the deployed message log is
//! (§11.7/#300), and for a sharper reason: most of the tracing is the board itself —
//! where the guard came from, which door is open, how far the exit was. A separate
//! scene would take the evidence away at the exact moment it became useful. So the
//! block is clamped to the middle of the map and the run's last frame reads above and
//! below it.
//!
//! # What it may say, and what it may not
//!
//! The cause comes from the **latched** terminal event ([`Ending`]) and never from the
//! finished board: by the time this draws, the capturing guard is standing on the
//! player's cell with whatever mood the last turn left it in. The screen names the
//! guard's *mood at contact* and paints the line in that mood's own §11.2 colour, so a
//! red Chasing capture reads as the end of a hunt and a yellow Calm one as a patrol
//! walking into a cell you thought nobody was coming to — two different mistakes.
//!
//! It does **not** name the guard's index. That is the code's handle on it (§11.8:
//! the screen names the world), and "guard 2" tells a player nothing they can act on.
//!
//! **Nor does it print the contact cell.** The `Ending` carries it — the loop knows
//! exactly where the run ended — but a pair of coordinates is a thing to decode, and
//! the board it names is *right there*, unchanged, with the guard standing on the
//! player's last cell. Saying `24,12` asks the player to look the answer up on a
//! picture that is already showing it. This is the overlay earning its keep: because
//! the evidence stayed on screen, the panel does not have to describe it.

use super::alert::condition_line;
use super::help::seed_token;
use super::hud::{BOTTOM_ROWS, TOP_ROWS};
use super::menu::{centre, ENTRY_SPACING, MARKER, NO_MARKER};
use super::{draw, GlyphCell, Grid, Visibility};
use crate::alert::{rung_category, NO_ALERT};
use crate::category::Category;
use crate::guard::GuardState;
use crate::level_seed::LevelSeed;
use crate::verdict::{EndExit, Ending, RunOptions, RunStats, Verdict};

/// The end screen's **view state**, owned by the shell exactly like
/// [`MenuUi`](super::MenuUi) — it changes no world and costs no turn (§12.1).
///
/// Unlike the menu's, it carries no "is it up" flag: the screen is up exactly when the
/// run has ended ([`State::verdict`](crate::State::verdict)), which is a fact about the
/// state and not a toggle a shell could get wrong. What the shell owns here is the
/// run's framing — which decides the exits — and which exit the marker rests on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct EndUi {
    /// How this run is being played (§2.2/appendix 31): the mode gates the exits, and
    /// the difficulty is what *new run* re-rolls at.
    pub options: RunOptions,
    /// Where the selection marker rests. Walked by
    /// [`end_nav_for_key`](crate::end_nav_for_key) and set by a tap on a row; read
    /// through [`selected`](Self::selected), which will not hand back an exit this
    /// run's mode does not offer.
    pub selected: EndExit,
}

impl EndUi {
    /// The exits this run's mode offers, in draw order (§2.2 — a campaign run is not
    /// replayable).
    pub fn exits(&self) -> &'static [EndExit] {
        self.options.mode.exits()
    }

    /// The exit the marker actually rests on: [`selected`](Self::selected) when this
    /// mode offers it, and the first exit otherwise.
    ///
    /// The validation is here rather than at every reader because the field is
    /// public: a shell that carried a marker across a mode change — or simply started
    /// on the [`Default`] — must not be able to fire a retry the mode has no business
    /// offering. The gate is [`RunMode::exits`](crate::RunMode::exits) and nothing
    /// else reads around it.
    pub fn selected(&self) -> EndExit {
        let exits = self.exits();
        if exits.contains(&self.selected) {
            self.selected
        } else {
            exits.first().copied().unwrap_or_default()
        }
    }

    /// The next exit down the list, wrapping — the counterpart of
    /// [`MenuEntry::next`](super::MenuEntry::next). A mode with one exit stays on it.
    pub fn next(&self) -> EndExit {
        self.step(1)
    }

    /// The previous exit up the list, wrapping.
    pub fn prev(&self) -> EndExit {
        self.step(-1)
    }

    fn step(&self, by: isize) -> EndExit {
        let exits = self.exits();
        let here = exits
            .iter()
            .position(|&exit| exit == self.selected())
            .unwrap_or(0) as isize;
        let len = exits.len() as isize;
        exits[(here + by).rem_euclid(len) as usize]
    }
}

/// The heading each ending wears — the one row that has to be legible at a glance,
/// before any of the detail is read.
const CAPTURED_HEADING: &str = "CAPTURED";
const ENTOMBED_HEADING: &str = "ENTOMBED";
const ESCAPED_HEADING: &str = "ESCAPED";

/// The won run's one line of cause, where a lost run carries its capture line: §1's
/// promise, kept — you came in by your own tunnel and left by it.
const ESCAPED_CAUSE: &str = "the tunnel closes behind you";

/// What the [`Entombed`](Ending::Entombed) loss says (§8.3/#329): the phase window ran
/// out inside a solid with nowhere to be thrown clear to.
const ENTOMBED_CAUSE: &str = "the wall closed around you";

/// The rule that bounds the panel top and bottom — the same glyph the deployed message
/// log closes on (§11.7/#300), so the two overlays read as one family of surface
/// rather than two inventions.
const RULE_GLYPH: char = '─';

/// The footer of a screen with a choice to make, and of one without. A campaign run
/// has a single exit (§2.2), and telling it to choose between one thing would be a
/// small lie of exactly the kind §11.6 keeps out of the input hints.
const FOOTER_CHOOSE: &str = "↑↓ choose · Enter/tap";
const FOOTER_ONE_WAY: &str = "Enter/tap";

/// The label on the seed row (§11.8's untranslated meta vocabulary — this names the
/// run's setup, not anything inside the facility), and the gap after it.
const SEED_LABEL: &str = "seed ";

/// One drawn row of the panel. The layout is built as a list of these **once** and
/// then both drawn ([`overlay_verdict`]) and hit-tested ([`verdict_hit`]) off the same
/// list, so a tap can only ever land on a row that was actually painted (§11.6).
enum Row {
    /// The bounding rule, edge to edge.
    Rule,
    /// A blank row of panel — the surface, with the board hidden behind it.
    Blank,
    /// A line of prose, centred.
    Line(String, Category),
    /// An exit row: a full-width tap target carrying its action.
    Exit(EndExit),
}

/// The panel's rows for `verdict`, top to bottom.
///
/// Every screen has the same shape — rule, heading, cause, ledger, seed, exits, footer,
/// rule — and the two verdicts differ in what they put in the slots, which is what
/// makes them tell apart at a glance while staying one screen to reason about.
fn rows(verdict: Verdict, ui: EndUi, level: Option<LevelSeed>) -> Vec<Row> {
    let mut rows = vec![Row::Rule, Row::Blank];
    let (heading, cause) = match verdict.ending {
        Ending::Captured { state, .. } => (
            (CAPTURED_HEADING, Category::Danger),
            vec![(capture_cause(state).to_string(), state.category())],
        ),
        Ending::Entombed { .. } => (
            (ENTOMBED_HEADING, Category::Danger),
            vec![(ENTOMBED_CAUSE.to_string(), Category::Danger)],
        ),
        // Interest is the goals-and-rewards colour (§11.2): a won run is the one thing
        // on this screen that *is* the reward, and it is the furthest possible read
        // from the losses' red.
        Ending::Escaped => (
            (ESCAPED_HEADING, Category::Interest),
            vec![(ESCAPED_CAUSE.to_string(), Category::Interest)],
        ),
    };
    rows.push(Row::Line(heading.0.to_string(), heading.1));
    rows.push(Row::Blank);
    for (text, category) in cause {
        rows.push(Row::Line(text, category));
    }
    rows.push(Row::Blank);
    for (text, category) in ledger(verdict.stats) {
        rows.push(Row::Line(text, category));
    }

    // The seed row, on both screens (#138): the sharing loop is "seed 8371 got me like
    // this", and the moment worth sharing a run from is the moment it ended. A
    // hand-built state has no token to print ([`seed_token`]) and simply skips the row
    // rather than printing something that boots a different level (#333).
    if let Some(token) = seed_token(level) {
        rows.push(Row::Blank);
        rows.push(Row::Line(
            format!("{SEED_LABEL}{token}"),
            Category::Interest,
        ));
    }

    rows.push(Row::Blank);
    let exits = ui.exits();
    for (i, &exit) in exits.iter().enumerate() {
        if i > 0 {
            // The blank between rows is what makes a full-width row a safe tap target
            // — a mis-aimed tap lands on it and does nothing (§11.6), the menu's rule.
            for _ in 1..ENTRY_SPACING {
                rows.push(Row::Blank);
            }
        }
        rows.push(Row::Exit(exit));
    }
    rows.push(Row::Blank);
    rows.push(Row::Line(
        if exits.len() > 1 {
            FOOTER_CHOOSE.to_string()
        } else {
            FOOTER_ONE_WAY.to_string()
        },
        Category::System,
    ));
    rows.push(Row::Blank);
    rows.push(Row::Rule);
    rows
}

/// How a capture reads, by the mood the guard made contact in (§7.4/§7.6) — the line
/// that turns "you lost" into a mistake you can name.
///
/// Each is a different story about the same event, and the difference is the whole
/// point: a Chasing capture is a hunt you lost, an Alerted one is a search that swept
/// the cell you were holding, and a Calm one is a patrol you had not accounted for —
/// which, if it reads as unfair, is a §7.6/§2.2 design bug this screen has just made
/// visible, and belongs in a ticket rather than in softer wording here.
fn capture_cause(state: GuardState) -> &'static str {
    match state {
        GuardState::Calm => "a patrolling guard walked into you",
        GuardState::Alerted => "a searching guard walked into you",
        GuardState::Investigating => "a guard on a glimpse walked into you",
        GuardState::Chasing => "the guard hunting you ran you down",
        GuardState::Responding => "a guard sent to a dead post found you",
    }
}

/// The run's ledger, in two rows (§14 v2): what you took, and how loud you were.
///
/// The split is the reading: the first row is the **haul** — the run as an objective —
/// and the second is the **noise** it cost, which is the stealth game's own score. Both
/// are label-and-number throughout, so no count needs a plural to be read.
///
/// The alert wears the §11.8 player-facing noun (*condition*) and the rung's own
/// colour, and a facility that never noticed says so in the help panel's own words
/// rather than as `Condition 0 of 3` — the same choice, for the same reason, on every
/// surface that reports the ladder.
fn ledger(stats: RunStats) -> Vec<(String, Category)> {
    let alert = if stats.alert_peak == 0 {
        NO_ALERT.to_string()
    } else {
        condition_line(stats.alert_peak)
    };
    vec![
        (
            format!(
                "turns {} · intel {} of {} · takedowns {}",
                stats.turns, stats.intel, stats.intel_total, stats.takedowns
            ),
            Category::Neutral,
        ),
        (
            format!("seen {} · {alert}", stats.detections),
            rung_category(stats.alert_peak),
        ),
    ]
}

/// Where the panel's first row lands on a `height`-tall screen: the block centred in
/// the **map area**, so the finished board reads above and below it (§2.2's tracing).
///
/// Clamped to the top of the map rather than allowed to climb into the status lines:
/// the near line still carries the run's last message, which is the one sentence the
/// screen's own cause line is the long form of.
fn panel_top(height: u32, rows: usize) -> u32 {
    let map_h = height.saturating_sub(TOP_ROWS + BOTTOM_ROWS);
    TOP_ROWS + map_h.saturating_sub(rows as u32) / 2
}

/// Lay the panel over a finished frame (§14 v2/#138) — the last thing drawn, over the
/// board and over any chrome, because a verdict is the one thing on screen that
/// nothing may sit on top of.
///
/// Every row it uses is **cleared first**, so the board never shows through the words;
/// rows past the frame's end are dropped, exactly as the deployed log drops its tail.
pub(super) fn overlay_verdict(
    grid: &mut Grid,
    verdict: Verdict,
    ui: EndUi,
    level: Option<LevelSeed>,
) {
    let rows = rows(verdict, ui, level);
    let top = panel_top(grid.height, rows.len());
    let selected = ui.selected();
    for (i, row) in rows.iter().enumerate() {
        let y = top + i as u32;
        if y >= grid.height {
            break;
        }
        clear_row(grid, y);
        match row {
            Row::Blank => {}
            Row::Rule => {
                for x in 0..grid.width {
                    grid.cells[(y * grid.width + x) as usize] = GlyphCell {
                        glyph: RULE_GLYPH,
                        fg: Category::System,
                        ..GlyphCell::blank()
                    };
                }
            }
            Row::Line(text, category) => {
                let len = text.chars().count() as u32;
                draw(grid, centre(grid.width, len), y, text, *category);
            }
            Row::Exit(exit) => {
                let marker = if *exit == selected { MARKER } else { NO_MARKER };
                draw(
                    grid,
                    exit_column(grid.width),
                    y,
                    &format!("{marker}{}", exit.label()),
                    Category::Neutral,
                );
            }
        }
    }
}

/// Blank one row of the frame — the panel's surface, so no board glyph reads through
/// the words laid over it.
fn clear_row(grid: &mut Grid, y: u32) {
    for x in 0..grid.width {
        grid.cells[(y * grid.width + x) as usize] = GlyphCell {
            vis: Visibility::Live,
            ..GlyphCell::blank()
        };
    }
}

/// The column the exit block starts at: the widest exit line centred, every row
/// left-aligned inside it — the menu's rule, and for its reason (a list whose labels
/// jitter as the marker moves is a list you have to re-read).
///
/// Measured over **every** exit there is, not only this mode's, so the block sits in
/// the same place whatever the mode offers.
fn exit_column(width: u32) -> u32 {
    let widest = [EndExit::Retry, EndExit::NewRun, EndExit::Menu]
        .iter()
        .map(|exit| (MARKER.chars().count() + exit.label().chars().count()) as u32)
        .max()
        .unwrap_or(0);
    centre(width, widest)
}

/// Which exit a press at screen cell `(x, y)` fires, or `None` for a press the screen
/// swallows (§11.6/#138) — the touch half of
/// [`end_nav_for_key`](crate::end_nav_for_key).
///
/// **The whole row is the target**, at any column: nothing else is drawn on an exit's
/// row, and a generous target is the difference between an end screen that works on a
/// phone and one that traps you on it. It reads the same [`rows`] the drawing does, so
/// an exit is hittable exactly where it is painted — and a mode that offers fewer
/// exits has fewer live rows, with no second gate to keep in step.
#[must_use]
pub fn verdict_hit(
    height: u32,
    verdict: Verdict,
    ui: EndUi,
    level: Option<LevelSeed>,
    y: u32,
) -> Option<EndExit> {
    if y >= height {
        return None;
    }
    let rows = rows(verdict, ui, level);
    let index = y.checked_sub(panel_top(height, rows.len()))? as usize;
    match rows.get(index)? {
        Row::Exit(exit) => Some(*exit),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
