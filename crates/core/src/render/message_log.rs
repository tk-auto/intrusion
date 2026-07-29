//! The near line's **deployed message log** (§11.7/#267/#300) — the panel behind
//! the chevron, and the corner control that deploys it.
//!
//! The near line itself is one row and stays one row: it speaks the loudest **live**
//! message and is wiped by the player's next action (§11.4/§11.7). Everything that
//! does not fit in that row is here — the counter beside the `[?]`, the block that
//! hangs from the band over the board when it is deployed, and the geometry a shell
//! reads to know which map rows are the log's rather than the board's (#306).
//!
//! # What the block shows (#300)
//!
//! The current action's messages **the near line did not say** — its loudest is
//! already the band an inch above, and printing it twice spends a row to tell the
//! player something they are looking at — then a **separator rule** and the previous
//! message-bearing action's block, and so on back through [`MessageHistory`]. A radio
//! silence, a call-in and a body find on three consecutive turns is the moment a
//! player wants to read back what set the facility off, and the near line — correctly
//! — kept only the last of them.
//!
//! That is also where an alert raise's **reason** shows up (#418): the raise's
//! subordinate message follows its headline through
//! [`loudest_first`](crate::status::live_messages), so when the near line speaks
//! *"security condition 2 of 3"* the row directly beneath the band says what did it.
//! The near line is one row and keeps the condition alone (§11.4); the *why* is what
//! the player finds on deploying, which is where they go to ask what just happened.
//!
//! So the near line and the panel partition the turn instead of overlapping on it: the
//! band speaks the loudest, the block holds the rest, and the corner control says which
//! of those two states you are in with a single glyph ([`deploy_glyph`]).
//!
//! It is **not** a scrollback the player pages through: there is no camera and no
//! scrolling (§11.4 **[SETTLED]**). The block is bounded twice over — by
//! [`HISTORY_ACTIONS`](crate::status::HISTORY_ACTIONS) and by [`MAX_LOG_ROWS`] — and
//! then clamped to whatever the board can hold, so it either fits or shows what fits
//! from the top. If it ever feels too short, the answer is the **[START]** bound, not
//! a scrollbar.
//!
//! **Now reads louder than then.** The current action's rows draw at full strength and
//! every remembered row — and every rule — draws in its category's dim shade ([`PAST`]),
//! so a glance separates what just happened from what is merely on the record, without
//! having to count rules.

use super::hud::{ScreenUi, BOTTOM_ROWS, NEAR_ROW, TOP_ROWS};
use super::*;
use crate::status::{live_messages, Message, MessageHistory};

/// The most **rows** the deployed block may ever cover, separators included
/// (§11.7/#300). **[START]** — the panel is drawn over the map, and burying the
/// danger overlay (§11.5) behind a wall of text is the failure mode to avoid, so the
/// history's own bound ([`HISTORY_ACTIONS`](crate::status::HISTORY_ACTIONS)) is backed
/// by a hard row budget for the run of loud actions that would otherwise fill the
/// screen between them.
///
/// Half the v1 board's 40 rows (§10.2). The deployed look is a deliberate, momentary
/// one — you take it, read it and fold it — so it can afford to cover more than a
/// glanceable overlay could, and half the facility stays visible underneath while it
/// is up. Below this the bound was doing the cutting rather than the history, which
/// made [`HISTORY_ACTIONS`](crate::status::HISTORY_ACTIONS) the wrong knob to reach for.
pub const MAX_LOG_ROWS: usize = 20;

/// How many of those rows the block's **content** may use: all but the closing rule
/// ([`SEPARATOR_GLYPH`]), which every block ends on.
const CONTENT_ROWS: usize = MAX_LOG_ROWS - 1;

/// The **separator rule** between two actions' blocks (§11.7/#300): a run of this
/// glyph across the block's band, drawn in [`Category::System`] — the HUD control
/// colour every piece of chrome wears.
///
/// It is chrome, not content, and the colour is the whole reason it can be: the
/// message categories are the §11.2 meaning ladder, and a rule in any of them would
/// read as a threat flash that happened to be one cell tall. System says *"this is
/// the frame talking"*, which is exactly what a rule between turns is — and it draws
/// [`PAST`], since everything from it down is.
const SEPARATOR_GLYPH: char = '─';

/// One row of the deployed block: a message, or the rule that divides two actions.
#[derive(Clone, PartialEq, Eq, Debug)]
enum LogRow {
    /// A message, drawn in its own §11.2 category — at full strength when it is the
    /// current action's, in that category's **dim** shade when it is a past one's
    /// ([`PAST`]).
    Message { message: Message, past: bool },
    /// The chrome rule between one action's block and the older one beneath it.
    /// Always dim: it *is* the boundary, so it belongs to the quiet half.
    Separator,
}

impl LogRow {
    /// A row of the current action.
    fn live(message: Message) -> Self {
        Self::Message {
            message,
            past: false,
        }
    }
}

/// How a **past** action's rows are drawn (§11.5/#300): the knowledge state whose
/// palette entry is each category's own dim shade, so a remembered message keeps its
/// §11.2 meaning and simply recedes — a body find two turns ago is still orange, just
/// not shouting over the one that landed this turn.
///
/// Reusing the map's fog channel for chrome is deliberate. The alternative is a second
/// dimming mechanism that the theme has to keep in step with the first, and the seam
/// here is exactly the one [`Visibility`] already names: *how certain and how present
/// is this*, answered per glyph, resolved to a colour by the shell alone (§11.1).
const PAST: Visibility = Visibility::Explored;

/// The **screen** row the deployed block hangs from (§11.7/#300): the usable line's
/// own row, directly under the near line's band, so the block reads as one surface
/// growing out of the line it belongs to rather than a panel floating a row below it.
///
/// It covers the usable line rather than starting under it. That row lists what you
/// could bump into *next* (§11.4/#323) — the one thing you are provably not doing
/// while you have the log open to read what already happened — so it is the cheapest
/// row on the screen to spend, and spending it buys a whole turn of history back. It
/// returns the instant the log folds, and folding costs no turn either way (§4.4).
const LOG_TOP_ROW: u32 = super::hud::USABLE_ROW;

/// Every row the deployed log would draw, top to bottom (§11.7/#300) — the current
/// action's messages **below** the one the near line is already speaking, then a
/// [`LogRow::Separator`] and the previous message-bearing action's block, and so on
/// back through [`MessageHistory`], closed by a final rule, capped at
/// [`MAX_LOG_ROWS`].
///
/// **The one source of truth for the whole log**: the drawing
/// ([`overlay_message_log`]), the geometry a shell taps against
/// ([`message_log_rows`]) and the very existence of the deploy control
/// ([`is_message_button`]) are all this list's length, so a control can never offer a
/// panel that draws nothing and a tap can never read the board through a row the log
/// covered.
///
/// Empty when there is nothing the near line has not already said: a lone live message
/// with no history is simply the band, and there is nothing left over to deploy
/// (§11.7).
fn log_rows(state: &State) -> Vec<LogRow> {
    assemble(live_messages(state), state.message_history())
}

/// [`log_rows`]' whole rule, over its two inputs and nothing else — the live block and
/// the remembered ones — so the stacking, the bounds and the no-stray-rule guarantees
/// are testable without staging a run that happens to produce them.
fn assemble(live: Vec<Message>, history: &MessageHistory) -> Vec<LogRow> {
    if live.len() < 2 && history.is_empty() {
        return Vec::new();
    }

    // Skip the loudest live message: it *is* the near line, drawn as a full category
    // band one row above the block's first row (§11.7). Repeating it would spend the
    // panel's most valuable row saying what the player is already reading, and would
    // leave the corner's `!` promising something the block would not deliver. An action
    // whose only message is on the near line therefore contributes no rows at all, and
    // no rule with them.
    //
    // Exactly *one* row is skipped, so a headline's **subordinate** message — spliced
    // directly under it by `loudest_first` (#418) — survives the skip and leads the
    // block. That is the point: the near line said the fact, and this says why.
    let mut rows: Vec<LogRow> = live.into_iter().skip(1).map(LogRow::live).collect();
    for block in history.blocks() {
        // **Every** remembered block gets its rule, the first one included — so when
        // this action contributed no rows the block opens with a rule rather than with
        // a past message pretending to be current. The rule's job is to say "what
        // follows is not this turn", and that claim is needed most at the top, right
        // under the near line's band. A block with no messages was never filed
        // (`MessageHistory::record`), so no rule can ever land on an empty band, and
        // the trailing one is popped below if the budget cuts its block away.
        rows.push(LogRow::Separator);
        // Every remembered block is dim, including the first one under a silent
        // current action — what makes a row quiet is *which turn it is from*, not
        // whether a rule happens to sit above it.
        rows.extend(block.iter().cloned().map(|message| LogRow::Message {
            message,
            past: true,
        }));
        if rows.len() >= CONTENT_ROWS {
            break;
        }
    }
    // The **closing rule** (§11.7/#300): the block ends on one, always. It is what
    // gives the panel a lower edge against the map — without it the oldest message
    // just stops and the board resumes, and the eye has to find the boundary by
    // noticing the terrain start. Rules top and bottom make the whole thing read as
    // one surface laid over the board rather than as text spilled onto it.
    //
    // Popping a trailing rule first keeps that to exactly one: the loop leaves a bare
    // rule behind whenever the row budget cut the block it introduced.
    rows.truncate(CONTENT_ROWS);
    if rows.last() == Some(&LogRow::Separator) {
        rows.pop();
    }
    rows.push(LogRow::Separator);
    rows
}

/// The near line's deploy control — **three cells, always** (§11.7/#300).
///
/// It used to be `[+2 ▾]`: a chevron and a count of the further live messages. Six
/// cells on every frame the control was up, spent on a number the player gets for free
/// by deploying it — and the near line's words are the scarcest space on the screen
/// (§11.4's row-fits bound). The count is gone and the glyph carries the whole state.
pub(super) const DEPLOY_LEN: u32 = 3;

/// What that one glyph says (§11.7/#300):
///
/// - **`!`** — this action raised more than the near line is showing. The one state
///   worth interrupting for: it is *new*, and the next action clears it (§11.7), so a
///   player who does not look now cannot look later.
/// - **`▾`** — nothing of this turn is unread; what is behind the control is the
///   remembered turns. An invitation, not a nudge.
/// - **`▴`** — deployed. The block is on screen, so nothing is unread by definition and
///   the control's only job is the way back.
fn deploy_glyph(unread_this_action: bool, open: bool) -> char {
    match (open, unread_this_action) {
        (true, _) => '▴',
        (false, true) => '!',
        (false, false) => '▾',
    }
}

/// How many cells the near line leaves the **words** on a screen `width` wide (§11.7):
/// the row minus the `[?]` at its left end and its cell of air, and minus the deploy
/// control at the right and the cell of air before it. The §11.4 row-fits bound is
/// measured against this, so it can never drift from the layout it describes.
#[cfg(test)]
pub(crate) fn near_line_text_max(width: u32) -> usize {
    width.saturating_sub(super::hud::NEAR_LINE_CONTROL_CELLS) as usize
}

/// The deploy control's label, or `None` when there is nothing to deploy — *what* the
/// corner shows, leaving *where* to [`corner_controls`](super::hud::corner_controls),
/// which is the one place the near line's layout lives.
pub(super) fn deploy_label(state: &State, open: bool) -> Option<String> {
    if log_rows(state).is_empty() {
        return None;
    }
    let unread = live_messages(state).len() > 1;
    Some(format!("[{}]", deploy_glyph(unread, open)))
}

/// Whether screen cell `(x, y)` is the near line's message-log toggle (§11.7) —
/// the counter left of the `[?]` that deploys and folds the message log. A shell
/// maps a click to a screen cell and asks this; a hit flips
/// [`ScreenUi::message_log_open`] instead of stepping.
///
/// There is no button unless the log has something the near line has not already
/// said — more than one live message, **or** a non-empty history (#300) — so a lone
/// message on a quiet first turn yields `false`. The geometry is read from `state`,
/// so a click can never miss the toggle the frame drew.
pub fn is_message_button(state: &State, x: u32, y: u32) -> bool {
    let width = state.layout().facility().width();
    // Either chevron is one cell, so which of the two is drawn cannot move the button.
    let Some((label, start)) = super::hud::near_line_controls(state, width, false).log else {
        return false;
    };
    let len = label.chars().count() as u32;
    y == NEAR_ROW && x >= start && x < start + len
}

/// Draw the message-log toggle over the already-built near line `row` (§11.7):
/// the [`deploy_label`] right-aligned, its glyphs in System — the HUD
/// control colour, like the ability line's deploy button — over the loudest
/// message's own category band, which keeps painting behind it.
pub(super) fn draw_message_button(
    row: &mut [GlyphCell],
    width: u32,
    start: u32,
    band: Category,
    fill: Fill,
    label: &str,
) {
    for (i, glyph) in label.chars().enumerate() {
        let x = start + i as u32;
        if x < width {
            row[x as usize] = GlyphCell {
                glyph,
                fg: Category::System,
                bg: Some(band),
                fill,
                ..GlyphCell::blank()
            };
        }
    }
}

/// How many rows the deployed block draws in total (§11.7/#300), clamped to what the
/// frame can hold: it starts at [`LOG_TOP_ROW`] and may reach down to the map's last
/// row, never over the ability bar. `0` when it is drawing nothing.
///
/// Shared by the drawing ([`overlay_message_log`]) and the geometry
/// ([`message_log_rows`]) so a shell can never disagree with the frame about where the
/// block ends.
fn drawn_rows(state: &State, map_h: u32) -> u32 {
    // From LOG_TOP_ROW down to the map's last row inclusive: the usable line's row
    // plus the whole board.
    let budget = map_h + (TOP_ROWS - LOG_TOP_ROW);
    (log_rows(state).len() as u32).min(budget)
}

/// How many **map rows** the deployed message log covers right now (§11.7), or `0`
/// when nothing of it is on the board — the geometry half of
/// [`overlay_message_log`], read by a shell that must know which rows are the log's
/// rather than the board's (#306: a tap on the list you opened to read must never
/// burn a turn).
///
/// Mirrors the drawing exactly: the log earns the board only when it is deployed
/// **and** it has something to show ([`log_rows`]), it hangs from [`LOG_TOP_ROW`], and
/// it is clamped on a frame too short to hold every row. Its **first** row falls on the
/// usable line, which is chrome already, so only what reaches past it is board — a
/// one-row block covers no map row at all. `0` while a modal screen is up
/// ([`ScreenUi::menu`] / [`ScreenUi::help_open`]) because then no board is drawn.
pub fn message_log_rows(state: &State, ui: ScreenUi) -> u32 {
    if ui.menu.is_some() || ui.help_open || !ui.message_log_open {
        return 0;
    }
    let map_h = state.layout().facility().height();
    drawn_rows(state, map_h).saturating_sub(TOP_ROWS - LOG_TOP_ROW)
}

/// Overlay the deployed message log onto the finished screen `grid` (§11.7/#267/#300):
/// the [`log_rows`] one per row, **hanging from the near line** — the current action's
/// loudest *unsaid* message on [`LOG_TOP_ROW`], the row directly below the band that
/// is already speaking its loudest, each quieter message one row lower, then a rule
/// and the previous action's block.
///
/// Every row is cleared **end to end**: the block is the screen's full width, like the
/// near line it grows out of, so the rules run edge to edge and no board shows through
/// between the words and the frame. A one-cell left margin lines the words up under
/// the band, and each message keeps its own §11.2 category — at full strength for the
/// current action, [`PAST`] for a remembered one.
///
/// Bounds are clamped, never asserted: on a frame too short to hold every row (only
/// hand-built test states get that small — the v1 board is 40×40, §10.2) the block
/// shows as many as fit from the top and drops the rest. It never reaches the ability
/// bar: the bar is the one surface always worth reading, log or no log.
pub(super) fn overlay_message_log(grid: &mut Grid, state: &State) {
    let width = grid.width;
    let map_h = grid.height.saturating_sub(TOP_ROWS + BOTTOM_ROWS);
    let rows = log_rows(state);
    let drawn = drawn_rows(state, map_h) as usize;
    let blank = GlyphCell::blank();
    for (i, row) in rows.iter().take(drawn).enumerate() {
        let y = LOG_TOP_ROW + i as u32;
        // A frame too short to hold the block still gets its closing rule: the block
        // genuinely ends at this row, and saying so is what keeps the lower edge
        // readable against the map on any board.
        let row = if i + 1 == drawn && drawn < rows.len() {
            &LogRow::Separator
        } else {
            row
        };
        for dx in 0..width {
            grid.cells[(y * width + dx) as usize] = blank;
        }
        match row {
            // A one-cell left margin, matching the near line, so the list lines up
            // under the band it hangs from.
            LogRow::Message { message, past } => {
                for (dx, glyph) in message.text.chars().enumerate() {
                    let x = 1 + dx as u32;
                    if x >= width {
                        break;
                    }
                    grid.cells[(y * width + x) as usize] = GlyphCell {
                        glyph,
                        fg: message.category,
                        vis: if *past { PAST } else { Visibility::Live },
                        ..blank
                    };
                }
            }
            // The rule runs the whole row: a divider that stopped short of the edge
            // would read as another line of text rather than as the frame speaking.
            LogRow::Separator => {
                for dx in 0..width {
                    grid.cells[(y * width + dx) as usize] = GlyphCell {
                        glyph: SEPARATOR_GLYPH,
                        fg: Category::System,
                        vis: PAST,
                        ..blank
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alert::AlertTrigger;
    use crate::cell::{Cell, Direction};
    use crate::facility::Terrain;
    use crate::guard::Guard;
    use crate::render::hud::{render_screen, TOP_ROWS};
    use crate::render::MenuUi;
    use crate::state::{Event, Input};
    use crate::status::near_line;
    use crate::status::HISTORY_ACTIONS;
    use crate::test_support::open_room;

    /// The board every deployed-log test is built on: a takedown-with-witness set up
    /// one cell from the west wall, so the same fixture can raise a **loud** action
    /// (step north: the guard drops, the second guard finds the body, and the find
    /// sends the §7.3 ladder up — three messages at once) and a **quiet** one (step
    /// west into the wall: one "blocked", a free bump that spends no turn). Wide
    /// enough that no message is truncated.
    fn witnessed_takedown() -> State {
        let mut layout = open_room(40, 14);
        layout.place(Cell::new(1, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(1, 5),
            Direction::North,
            vec![
                Guard::stationary(Cell::new(1, 4)),
                Guard::stationary(Cell::new(1, 2)),
            ],
            Vec::new(),
            Cell::new(8, 8),
        );
        // A wall bump must stay a wall bump: with the §10.1 auto-slide on, walking
        // west into the edge would round the corner instead of saying "blocked", and
        // these tests are about what the near line *says*.
        s.set_auto_slide(false);
        s
    }

    /// That fixture with its one loud action already taken: four live messages, an
    /// empty history behind them — the escalation, the reason riding under it (#418),
    /// the found body and the takedown.
    fn loud_step() -> State {
        let mut s = witnessed_takedown();
        s.step(Input::Step(Direction::North));
        assert_eq!(live_messages(&s).len(), 4, "four messages land at once");
        s
    }

    /// Every glyph of screen row `y`, as a string.
    fn row_text(g: &Grid, y: u32) -> String {
        (0..g.width).map(|x| g.get(x, y).glyph).collect()
    }

    /// §11.7: when one step raises more than one message the near line speaks the
    /// loudest as its band and shows a right-aligned counter of the rest; deploying
    /// the list ([`ScreenUi::message_log_open`]) stacks every message over the
    /// board, loudest on the row directly above the band.
    #[test]
    fn the_near_line_counts_extra_messages_and_deploys_the_list() {
        let s = loud_step();
        let width = s.layout().facility().width();

        // Collapsed: the band speaks the loudest message and the closed counter of
        // the one further message (a down chevron) sits at the right.
        let g = render_screen(&s, ScreenUi::default());
        let near = row_text(&g, NEAR_ROW);
        assert!(
            near.contains("security condition"),
            "the band speaks the loudest message: {near:?}"
        );
        assert!(
            near.starts_with("[?]") && near.ends_with("[!]"),
            "the [?] at the left end, the unread mark at the right: {near:?}"
        );

        // The hit-test agrees with the drawn counter, and there is no button off it.
        // The deploy control sits flush against the row's last column, three cells
        // whatever it has to say.
        let start = width - DEPLOY_LEN;
        assert!(
            is_message_button(&s, start, NEAR_ROW),
            "the counter is hittable"
        );
        assert!(
            !is_message_button(&s, start - 1, NEAR_ROW),
            "nothing just left of it"
        );
        assert!(
            !is_message_button(&s, start, NEAR_ROW + 1),
            "and nothing a row down"
        );

        // Deployed: the chevron flips up and the whole list stacks over the board —
        // the loudest directly above the near line, the quieter one above that.
        let ui = ScreenUi {
            message_log_open: true,
            ..ScreenUi::default()
        };
        let g = render_screen(&s, ui);
        assert!(
            row_text(&g, NEAR_ROW).ends_with("[▴]"),
            "deployed, the control is the chevron back: nothing is unread now"
        );
        // The block is the *rest* of the turn: the near line's own message is not
        // repeated, so the `!` the corner showed resolves to exactly three rows —
        // led by **why** the condition the band is speaking climbed (#418).
        assert!(
            row_text(&g, LOG_TOP_ROW).contains("a guard found a body"),
            "the band's own reason leads the block"
        );
        assert!(
            row_text(&g, LOG_TOP_ROW + 1).contains("a body has been found"),
            "then the loudest message the near line did not say"
        );
        assert!(
            row_text(&g, LOG_TOP_ROW + 2).contains("the guard drops — a body is left"),
            "down to the quietest"
        );
        assert!(
            !row_text(&g, LOG_TOP_ROW + 3).contains("security condition"),
            "and the near line's own message is nowhere in the block"
        );
        assert_eq!(
            message_log_rows(&s, ui) + (TOP_ROWS - LOG_TOP_ROW),
            4,
            "three rows for the three the near line did not say, plus the closing rule"
        );
        assert!(
            row_text(&g, LOG_TOP_ROW + 3)
                .chars()
                .all(|c| c == SEPARATOR_GLYPH),
            "and the block closes on a rule, giving it an edge against the map"
        );
        assert!(
            !row_text(&g, LOG_TOP_ROW).contains("move"),
            "and the usable line's hint is covered, not pushed down"
        );
    }

    /// [`message_log_rows`] tells a shell exactly which map rows the deployed list
    /// covers (#306), and it must agree with the drawing: nothing while folded,
    /// nothing while a modal screen is up, one row per live message once deployed —
    /// the rows a tap must never read as the board underneath.
    #[test]
    fn the_message_log_reports_the_rows_it_covers() {
        let s = loud_step();
        let deployed = ScreenUi {
            message_log_open: true,
            ..ScreenUi::default()
        };
        assert_eq!(live_messages(&s).len(), 4, "four messages are live");
        assert!(
            s.message_history().is_empty(),
            "and nothing behind them yet"
        );
        assert_eq!(
            message_log_rows(&s, deployed),
            3,
            "three rows — the near line keeps the headline — plus the closing rule, of \
             which the first sits on the usable line, so three are board"
        );
        assert_eq!(
            message_log_rows(&s, ScreenUi::default()),
            0,
            "folded, the list covers nothing"
        );
        // A modal screen replaces the whole frame, so no board rows are the log's.
        for ui in [
            ScreenUi {
                help_open: true,
                ..deployed
            },
            ScreenUi {
                menu: Some(MenuUi::default()),
                ..deployed
            },
        ] {
            assert_eq!(message_log_rows(&s, ui), 0, "no board, no log rows");
        }

        // One message and nothing remembered earns no list at all — the near line
        // simply speaks it (§11.7).
        let quiet = State::new(
            open_room(40, 14),
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(8, 8),
        );
        assert!(live_messages(&quiet).len() < 2);
        assert!(quiet.message_history().is_empty());
        assert_eq!(message_log_rows(&quiet, deployed), 0);
    }

    /// §11.7: a single live message with nothing behind it shows no counter — the
    /// near line is the plain band it has always been, and the toggle is not a button.
    #[test]
    fn a_lone_message_shows_no_counter() {
        // Taking the intel is one loud message and nothing else this step — and it is
        // the run's *first* action, so there is no history behind it either.
        let mut s = State::new(
            open_room(20, 10),
            Cell::new(5, 6),
            Direction::North,
            Vec::new(),
            [Cell::new(5, 5)],
            Cell::new(18, 8),
        );
        s.step(Input::Step(Direction::North)); // bump the console: intel taken

        let width = s.layout().facility().width();
        let near = row_text(&render_screen(&s, ScreenUi::default()), NEAR_ROW);
        assert!(
            !near.contains("[+"),
            "no counter for a lone message: {near:?}"
        );
        assert!(
            near.starts_with("[?]"),
            "the help toggle keeps the row's left end: {near:?}"
        );
        assert!(
            !near.contains('▾') && !near.contains('!'),
            "and nothing to deploy leaves the right end to the words: {near:?}"
        );
        assert!(
            (0..width).all(|x| !is_message_button(&s, x, NEAR_ROW)),
            "and nothing to click"
        );
    }

    /// #418: an alert raise **alone** still has something to deploy — its reason. The
    /// near line speaks the condition and nothing else (§11.4, one row), so the block
    /// is exactly the one row saying what did it, plus the rule that closes it.
    #[test]
    fn a_raise_on_its_own_still_deploys_its_reason() {
        let live = crate::status::loudest_first(&[Event::AlertRaised {
            rung: 1,
            trigger: AlertTrigger::MissedPing,
        }]);
        assert_eq!(live.len(), 2, "the headline and its reason");

        let rows = assemble(live.clone(), &MessageHistory::default());
        assert_eq!(
            rows,
            vec![
                LogRow::Message {
                    message: live[1].clone(),
                    past: false,
                },
                LogRow::Separator,
            ],
            "the reason, then the closing rule — and the headline nowhere, since the \
             near line is already speaking it",
        );

        // A lone message with **no** reason under it is still nothing to deploy: the
        // near line says the whole of it (§11.7).
        assert!(assemble(live[..1].to_vec(), &MessageHistory::default()).is_empty());
    }

    /// **The golden block across two turns** (#300): the deployed log lists this
    /// action's messages loudest-first, then a separator rule, then the previous
    /// message-bearing action's block — each message in its own §11.2 category, the
    /// rule in the System chrome colour, past rows dimmed, and no rule where a turn
    /// said nothing.
    #[test]
    fn the_deployed_log_stacks_past_turns_behind_a_rule() {
        // Action 1: a bump into the west wall — one quiet message, which becomes the
        // history. Action 2: the takedown-with-witness step — four messages, of which
        // the near line speaks the loudest and the block gets the other three.
        let mut s = witnessed_takedown();
        s.step(Input::Step(Direction::West));
        s.step(Input::Step(Direction::North));
        assert_eq!(live_messages(&s).len(), 4, "four messages live");
        assert!(
            !s.message_history().is_empty(),
            "and the bump is remembered behind them"
        );

        let deployed = ScreenUi {
            message_log_open: true,
            ..ScreenUi::default()
        };
        let g = render_screen(&s, deployed);

        // The near line speaks the loudest, and counts the three the block will show.
        let near = row_text(&g, NEAR_ROW);
        assert!(near.contains("security condition"), "the band: {near:?}");
        assert!(near.ends_with("[▴]"), "deployed, and foldable: {near:?}");

        // The block: this action's three *unsaid* messages — led by the band's own
        // reason (#418) — a rule, then the remembered action, and the near line's own
        // headline nowhere in it.
        assert!(row_text(&g, LOG_TOP_ROW).contains("a guard found a body"));
        assert!(row_text(&g, LOG_TOP_ROW + 1).contains("a body has been found"));
        assert!(row_text(&g, LOG_TOP_ROW + 2).contains("the guard drops"));
        // The rule spans the whole row, edge to edge, like the near line above it.
        let rule = row_text(&g, LOG_TOP_ROW + 3);
        assert!(
            rule.chars().all(|c| c == SEPARATOR_GLYPH),
            "a rule across the whole row: {rule:?}"
        );
        assert_eq!(
            g.get(0, LOG_TOP_ROW + 3).fg,
            Category::System,
            "the rule is chrome, not a message category"
        );
        assert_eq!(
            g.get(0, LOG_TOP_ROW + 3).vis,
            PAST,
            "and it draws dim — everything from the rule down is past"
        );
        assert!(row_text(&g, LOG_TOP_ROW + 4).contains("blocked"));
        for dy in 0..5 {
            assert!(
                !row_text(&g, LOG_TOP_ROW + dy).contains("security condition"),
                "the near line's own message is never repeated (row {dy})"
            );
        }

        // Each message keeps its own §11.2 colour across the rule — the reason wearing
        // its headline's, because the two are one fact.
        assert_eq!(
            g.get(1, LOG_TOP_ROW).fg,
            Category::Warning,
            "the escalation's reason"
        );
        assert_eq!(
            g.get(1, LOG_TOP_ROW + 1).fg,
            Category::Warning,
            "the body find"
        );
        assert_eq!(
            g.get(1, LOG_TOP_ROW + 2).fg,
            Category::Owned,
            "the takedown"
        );
        assert_eq!(
            g.get(1, LOG_TOP_ROW + 4).fg,
            Category::Neutral,
            "the remembered bump"
        );

        // **Now reads louder than then**: this action's rows are at full strength and
        // every remembered row is dimmed, category intact.
        for dy in 0..=2 {
            assert_eq!(
                g.get(1, LOG_TOP_ROW + dy).vis,
                Visibility::Live,
                "row {dy} is this action's, so it draws at full strength"
            );
        }
        assert_eq!(
            g.get(1, LOG_TOP_ROW + 4).vis,
            PAST,
            "and the remembered row recedes"
        );

        // One rule between the blocks and one closing the whole thing: six rows in
        // all, of which the first sits on the usable line and five are board.
        assert_eq!(message_log_rows(&s, deployed), 5);
        assert!(
            row_text(&g, LOG_TOP_ROW + 5)
                .chars()
                .all(|c| c == SEPARATOR_GLYPH),
            "the closing rule under the oldest block"
        );
        assert!(
            !row_text(&g, LOG_TOP_ROW + 6)
                .chars()
                .any(|c| c == SEPARATOR_GLYPH),
            "and the map resumes immediately below it"
        );
    }

    /// **The words never run under the controls** (§11.4/§11.7/#300). The near line's
    /// text budget is computed *from* the corner cluster
    /// ([`CornerControls::text_max`](super::hud::CornerControls)), so with **both**
    /// controls up — the `[?]` and the deploy toggle beside it — a message too long for
    /// what is left is clipped rather than overdrawn by them.
    ///
    /// Measured on a deliberately narrow board, because that is the only way to make a
    /// real message overrun a real budget: the v1 row is 40 wide (§10.2) and leaves 29
    /// cells beside the pair, which every current message but the listed few fits.
    #[test]
    fn a_long_message_stops_short_of_both_corner_controls() {
        let mut layout = open_room(24, 14);
        layout.place(Cell::new(1, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(1, 5),
            Direction::North,
            vec![
                Guard::stationary(Cell::new(1, 4)),
                Guard::stationary(Cell::new(1, 2)),
            ],
            Vec::new(),
            Cell::new(8, 8),
        );
        s.set_auto_slide(false);
        s.step(Input::Step(Direction::North));

        let width = s.layout().facility().width();
        let controls = super::hud::near_line_controls(&s, width, false);
        let (label, log_start) = controls.log.clone().expect("three live messages deploy");
        assert_eq!(
            controls.text_max,
            log_start - 1,
            "the words stop a cell short of the deploy control"
        );
        assert_eq!(
            controls.text_start,
            super::hud::HELP_BUTTON_LEN + 1,
            "and start clear of the [?] and its cell of air"
        );
        assert!(
            near_line(&s).text.chars().count() > (controls.text_max - controls.text_start) as usize,
            "the fixture's message genuinely overruns the budget"
        );

        let g = render_screen(&s, ScreenUi::default());
        let row = row_text(&g, NEAR_ROW);
        assert!(row.contains("[?]"), "the help toggle is drawn: {row:?}");
        for (i, glyph) in label.chars().enumerate() {
            assert_eq!(
                g.get(log_start + i as u32, NEAR_ROW).glyph,
                glyph,
                "the deploy control is drawn where the layout put it: {row:?}"
            );
        }
        // And no glyph of the *message* reaches them: the words read Neutral on the
        // band (§11.4), the controls read System and the alert tint, so a Neutral
        // glyph at or past the cluster is the overflow this budget exists to stop.
        for x in 0..width {
            if x >= controls.text_start && x < controls.text_max {
                continue; // the message's own span
            }
            let cell = g.get(x, NEAR_ROW);
            assert!(
                cell.glyph == ' ' || cell.fg != Category::Neutral,
                "a message glyph at column {x}, outside the words' span: {row:?}"
            );
        }
    }

    /// The near line is **untouched** by a non-empty history (#300): the same words,
    /// the same category band, and a counter that still counts only what is *live*.
    #[test]
    fn a_history_does_not_change_the_near_line() {
        // The same loud action, once with nothing behind it and once with a quiet bump
        // already filed — identical live sets, different histories.
        let fresh = loud_step();
        let mut carried = witnessed_takedown();
        carried.step(Input::Step(Direction::West)); // "blocked" — files a history block
        carried.step(Input::Step(Direction::North)); // the same loud action

        assert!(fresh.message_history().is_empty());
        assert!(!carried.message_history().is_empty());
        // The near line's own source is untouched: `live_messages` never reads history.
        assert_eq!(live_messages(&carried), live_messages(&fresh));

        let width = fresh.layout().facility().width();
        let before = render_screen(&fresh, ScreenUi::default());
        let g = render_screen(&carried, ScreenUi::default());
        assert_eq!(
            row_text(&g, NEAR_ROW),
            row_text(&before, NEAR_ROW),
            "the same near line, verbatim"
        );
        assert_eq!(
            g.get(0, NEAR_ROW).bg,
            before.get(0, NEAR_ROW).bg,
            "the same category band"
        );
        assert!(
            row_text(&g, NEAR_ROW).ends_with("[!]"),
            "the mark is raised by this turn's own extras, not by the history"
        );
        // And the button is in the same place, so a tap lands where it always did.
        let start = width - DEPLOY_LEN;
        assert!(is_message_button(&carried, start, NEAR_ROW));

        // Deployed, though, the block is the live three, a rule, and the remembered
        // one — the history shows only behind the chevron.
        let deployed = ScreenUi {
            message_log_open: true,
            ..ScreenUi::default()
        };
        assert_eq!(message_log_rows(&fresh, deployed), 3);
        assert_eq!(message_log_rows(&carried, deployed), 5);
    }

    /// The block is **clamped, never asserted** (§11.7): on a board too short to hold
    /// a full history it shows what fits from the top and drops the rest — no panic,
    /// no assertion, and never a row past the map.
    #[test]
    fn the_block_clamps_on_a_board_too_short_to_hold_it() {
        // A four-row facility with a full history stacked behind a live message: the
        // player sits against the north wall and bumps it, a free action that says
        // "blocked" and spends no turn.
        let mut s = State::new(
            open_room(30, 4),
            Cell::new(5, 1),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 2),
        );
        s.set_auto_slide(false);
        for _ in 0..HISTORY_ACTIONS + 2 {
            s.step(Input::Step(Direction::North));
        }
        assert_eq!(
            s.message_history().blocks().count(),
            HISTORY_ACTIONS,
            "the ring is full"
        );

        let deployed = ScreenUi {
            message_log_open: true,
            ..ScreenUi::default()
        };
        let rows = message_log_rows(&s, deployed);
        assert_eq!(rows, 4, "clamped to the board's height");
        assert!(
            log_rows(&s).len() > rows as usize,
            "and there was genuinely more to show"
        );

        // The frame still draws, whole and in bounds — and never over the ability bar,
        // whatever the history wanted.
        let g = render_screen(&s, deployed);
        assert_eq!(g.cells.len() as u32, g.width * g.height);
        // This action's only message is on the near line, so the block is history
        // alone — and it opens with a rule saying exactly that (#300).
        assert!(
            row_text(&g, LOG_TOP_ROW)
                .chars()
                .all(|c| c == SEPARATOR_GLYPH),
            "a leading rule: the first row below the band is already a past turn"
        );
        assert!(row_text(&g, LOG_TOP_ROW + 1).contains("blocked"));
        assert!(
            !row_text(&g, g.height - BOTTOM_ROWS)
                .chars()
                .any(|c| c == SEPARATOR_GLYPH),
            "the ability bar keeps its row"
        );
    }

    /// Both **[START]** bounds hold whatever the run does (#300): the history keeps at
    /// most [`HISTORY_ACTIONS`](crate::status::HISTORY_ACTIONS) blocks, and the block never draws more than
    /// [`MAX_LOG_ROWS`] rows — nor ever ends on a rule that promises a turn it cut.
    #[test]
    fn the_block_is_bounded_and_never_ends_on_a_rule() {
        // A worst case no real turn reaches: a full ring of loud actions, each raising
        // as many messages as the whole block is allowed rows, so the row budget is
        // certain to bite however the two bounds are retuned.
        let per_action = MAX_LOG_ROWS;
        let loud = |n: u32| {
            (0..per_action)
                .map(|i| Message {
                    text: format!("message {n}.{i}"),
                    category: Category::Warning,
                    priority: 0,
                })
                .collect::<Vec<_>>()
        };
        let mut history = MessageHistory::default();
        for n in 0..HISTORY_ACTIONS as u32 + 1 {
            history.record(&[
                Event::Bumped {
                    into: Cell::new(n, 1),
                },
                Event::AlertRaised {
                    rung: 1,
                    trigger: AlertTrigger::Sighting,
                },
            ]);
        }
        assert_eq!(
            history.blocks().count(),
            HISTORY_ACTIONS,
            "the ring is bounded"
        );

        let rows = assemble(loud(0), &history);
        assert!(rows.len() <= MAX_LOG_ROWS, "the row budget holds: {rows:?}");
        assert_eq!(
            rows.last(),
            Some(&LogRow::Separator),
            "the block closes on a rule, so it has a lower edge against the map"
        );
        assert_ne!(
            rows.first(),
            Some(&LogRow::Separator),
            "no leading rule while this action still has rows of its own"
        );
        // No two rules in a row: a silent action files nothing, so it can never put an
        // empty band — or a doubled rule — between two blocks.
        assert!(
            !rows
                .windows(2)
                .any(|w| w[0] == LogRow::Separator && w[1] == LogRow::Separator),
            "no doubled rules: {rows:?}"
        );

        // A current action with nothing left to show *is* a reason for a leading rule
        // (#300): the first row below the near line's band is already a past turn, and
        // the rule is what says so before the player reads it as current.
        let rows = assemble(Vec::new(), &history);
        assert_eq!(
            rows.first(),
            Some(&LogRow::Separator),
            "a block that opens on history opens on a rule"
        );
        assert_eq!(
            rows.get(1),
            Some(&LogRow::Message {
                message: history.blocks().next().unwrap()[0].clone(),
                past: true,
            }),
            "and the newest remembered block follows it, drawn as past"
        );

        // And nothing at all to say is no log — never a bare rule over the board.
        assert!(assemble(Vec::new(), &MessageHistory::default()).is_empty());
        assert!(assemble(loud(0)[..1].to_vec(), &MessageHistory::default()).is_empty());
    }
}
