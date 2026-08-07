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
use super::menu::{centre, ENTRY_SPACING, MARKER, NO_MARKER};
use super::{clear_row, draw, draw_rule, overlay_top, Grid};
use crate::alert::{rung_category, NO_ALERT};
use crate::category::Category;
use crate::guard::GuardState;
use crate::level_seed::LevelSeed;
use crate::score::{Axis, Score, STAR_EARNED, STAR_MISSED};
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

/// What the [`Entombed`](Ending::Entombed) loss says (§8.3/#329): the phase window ran
/// out inside a solid with nowhere to be thrown clear to.
const ENTOMBED_CAUSE: &str = "the wall closed around you";

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
/// Every screen has the same shape — rule, heading, cause, score, ledger, seed, exits,
/// footer, rule — and the two verdicts differ in what they put in the slots, which is
/// what makes them tell apart at a glance while staying one screen to reason about.
///
/// **A won run has no cause line.** A loss owes the player one — §2.2's promise is that
/// every capture is traceable, and the guard's mood at contact is what makes it so — but
/// a win's cause was never a cause: it restated the heading in prettier words, and the
/// rows under it now carry something the player can actually read. The slot simply goes
/// unfilled, which is what makes it a slot.
///
/// **The score sits above the numbers**, because it is the reading and they are the
/// evidence: *you were quick and quiet and you left a console* first, then the turns and
/// the haul it was worked out from.
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
        Ending::Escaped => ((ESCAPED_HEADING, Category::Interest), Vec::new()),
    };
    rows.push(Row::Line(heading.0.to_string(), heading.1));
    for (text, category) in cause {
        rows.push(Row::Blank);
        rows.push(Row::Line(text, category));
    }

    // The three stars (#563), on a won run only: a capture has no score, and rating a
    // loss would put a verdict where §14 v2 owes the player a reason.
    if let Some(score) = verdict.score() {
        let mut block = score_rows(score).into_iter();
        if let Some((text, category)) = block.next() {
            // The glance row stands off on its own — it is the headline, and the three
            // rows under it are what it is made of.
            rows.push(Row::Blank);
            rows.push(Row::Line(text, category));
            rows.push(Row::Blank);
        }
        // …and the axes read **contiguously**, as one list. A blank between each would
        // make three unrelated lines out of the one thing the block exists to be.
        for (text, category) in block {
            rows.push(Row::Line(text, category));
        }
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

/// The score block (§15 Q4/#563): the glance form, then **one row per axis**, each
/// naming itself, whether it was earned, and what it was for.
///
/// The named rows are the deliverable and the `★★☆` headline is the garnish, not the
/// other way round. A bare mark row would tell a player they scored two and leave them to
/// work out which two — and the only thing a rating is worth in a game with no
/// meta-progression is *which one you missed* (§2.2: what carries over is what you
/// learned). The headline says **whose** it is (`your score:`) rather than standing as
/// three loose marks: it is the one row on this screen that is a judgement of the player
/// rather than a fact about the run, and it says so.
///
/// Every row is padded to one width so the block centres as a block: the labels line up
/// in a column, the marks in another, and the reasons in a third. Three centred rows of
/// different lengths would stagger, and a staggered list of three things reads as three
/// unrelated lines rather than as one score.
///
/// **The block wears the player's own colour** (§11.2): **Owned** — the blue the `@` and
/// the cupboard you are hidden in are drawn in — for a star earned, **Ground** for one
/// missed. It used to be Interest, and Interest is spoken for twice on this screen
/// already: the `ESCAPED` heading and the seed row both wear it, so a third claim left
/// the score reading as more of the same. Owned is the right word as well as the right
/// contrast: the stars are a verdict on *you*, in the same sense the `@` is you.
fn score_rows(score: Score) -> Vec<(String, Category)> {
    let cell = |axis: Axis| {
        format!(
            "{:<label$} {} {}",
            axis.label(),
            if score.earned(axis) {
                STAR_EARNED
            } else {
                STAR_MISSED
            },
            axis.blurb(),
            label = AXIS_LABEL_WIDTH,
        )
    };
    let width = Axis::ALL
        .iter()
        .map(|&axis| cell(axis).chars().count())
        .max()
        .unwrap_or(0);
    let mut rows = vec![(format!("{SCORE_LABEL}{}", score.marks()), Category::Owned)];
    rows.extend(Axis::ALL.iter().map(|&axis| {
        (
            format!("{:<width$}", cell(axis), width = width),
            if score.earned(axis) {
                Category::Owned
            } else {
                Category::Ground
            },
        )
    }));
    rows
}

/// What the glance row leads with (§11.8's plain register). *Score* names the game's
/// judgement of the run, not anything inside the facility, so it stays the meta word —
/// and *your* is what makes the row a verdict addressed to the player rather than three
/// marks floating above the numbers.
const SCORE_LABEL: &str = "your score: ";

/// The column the axis marks line up in: the widest axis label, so *speed* and *stealth*
/// put their star in the same place. Derived from [`Axis::label`] rather than typed, so a
/// re-worded axis cannot leave the block ragged.
const AXIS_LABEL_WIDTH: usize = {
    let mut widest = 0;
    let mut i = 0;
    while i < Axis::ALL.len() {
        let len = Axis::ALL[i].label().len();
        if len > widest {
            widest = len;
        }
        i += 1;
    }
    widest
};

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
    // Centred in the map area, so the run's last frame — the evidence — reads above and
    // below it (§2.2's tracing), and never over the near line, which still carries the
    // run's last message: the one sentence this screen's own cause line is the long form
    // of.
    let top = overlay_top(grid.height, rows.len());
    let selected = ui.selected();
    for (i, row) in rows.iter().enumerate() {
        let y = top + i as u32;
        if y >= grid.height {
            break;
        }
        clear_row(grid, y);
        match row {
            Row::Blank => {}
            Row::Rule => draw_rule(grid, y),
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
    let index = y.checked_sub(overlay_top(height, rows.len()))? as usize;
    match rows.get(index)? {
        Row::Exit(exit) => Some(*exit),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
