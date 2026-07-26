//! On-screen HUD composition for the character grid (§11.4, §11.7).
//!
//! The world render — terrain, fog, entities, the danger overlay — lives in the
//! parent [`render`](super) module as a pure state→[`Grid`](super::Grid). This
//! module owns the *chrome* laid over and around it: the always-on ability bar
//! and its deployable panel, the near line and its message log, the status rows,
//! and the click hit-tests that pair with them. [`render_screen`] composes the
//! two — it draws the world grid, then overlays this. Both halves stay a pure
//! function of state (§11.1/§12.1) and golden-grid testable.
//!
//! # Which way up (§11.4, §15 Q9, #267)
//!
//! The chrome is laid out for **thumb reach**: the *action* surface — the ability
//! bar and its deploy button — sits at the **bottom right**, where a hand holding
//! the device already rests, and the *read-only* status — the near line, the
//! usable line, the help toggle — sits at the **top**, out of the way of both the
//! thumb and the board. It used to be the other way round (ability line as a
//! header, status lines at the foot), which put the one row you tap furthest from
//! the one finger you tap it with.

use super::*;
use crate::ability::{AbilityId, AbilityState, AbilityStatus};
use crate::cell::Direction;
use crate::status::{live_messages, near_line, Message};

/// The rows the screen adds **above** the map (§11.4): the near line and the
/// usable line — read-only status, kept clear of the thumb (#267). A shell
/// fitting the screen sizes for `this + facility height + BOTTOM_ROWS`.
pub const TOP_ROWS: u32 = 2;

/// The row the screen adds **beneath** the map (§11.4): the always-on ability
/// bar, right-aligned so the action surface falls under the thumb (#267). A
/// shell fits for `TOP_ROWS + facility height + this`.
pub const BOTTOM_ROWS: u32 = 1;

/// The near line's row (§11.4/§11.7): the top of the screen, where the message
/// band's colour flash reads without competing with the thumb (#267).
const NEAR_ROW: u32 = 0;

/// The usable line's row (§11.4): directly under the near line, still above the
/// map.
const USABLE_ROW: u32 = 1;

/// The screen row the ability bar occupies, on a map `map_h` tall: the last row
/// of the frame. Shared by the drawing ([`render_screen`]) and the hit-tests
/// ([`ability_at`], [`is_ability_button`]) so a tap lands on the bar that is
/// drawn.
fn ability_row(map_h: u32) -> u32 {
    TOP_ROWS + map_h
}

/// The transient **view state** a shell keeps between frames and hands to
/// [`render_screen`] (§11.4). It is deliberately *not* part of [`State`] — the
/// core stays pure game logic (§12.1), and what the player has merely chosen to
/// *look at* changes no world and costs no turn. The shell owns it, toggles it
/// from [`ui_command_for_key`](crate::input::ui_command_for_key) or a click on the
/// deploy button ([`is_ability_button`]), and passes it in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ScreenUi {
    /// Whether the full ability panel is deployed (§11.4). The compact ability
    /// bar is always drawn; this gates only the expanded, named panel.
    pub ability_panel_open: bool,
    /// Whether the near line's full message list is deployed (§11.7). The near
    /// line always speaks the loudest live message; when more than one is live it
    /// also shows a counter, and this gates the expanded list of them all. The
    /// list is always the current step's live set — deployed or not, it clears on
    /// the next action (§11.7), never a scrollback.
    pub message_log_open: bool,
    /// Whether the help panel is up (§14 v2/#139/#248): a modal, full-screen
    /// reference — the tab bar, the active tab's content, the footer. A pure view
    /// toggle — no world change, no turn (§4.4) — opened by the `?` key
    /// ([`ui_command_for_key`](crate::input::ui_command_for_key)) or the near line's
    /// help button ([`is_help_button`]). While it is up it is modal: the shell
    /// routes input to it ([`help_nav_for_key`](crate::help_nav_for_key) /
    /// [`help_hit`](crate::help_hit)), so the game never steps underneath.
    pub help_open: bool,
    /// Which help tab is showing while [`help_open`](Self::help_open) (§14 v2/#248).
    /// Ignored when the panel is closed; the [`Default`] is the leftmost tab, so the
    /// panel opens on Level info. The shell cycles it from
    /// [`help_nav_for_key`](crate::help_nav_for_key) or a tab tap ([`help_hit`]).
    pub help_tab: HelpTab,
}

/// The deploy button's label on the ability bar (§11.4): an **upward** chevron
/// when the panel is closed — bump it open and it unfolds upward off the bar — and
/// a downward one when it is open, to fold it back down. Both are three cells wide,
/// so the button's footprint is fixed regardless of state.
const BUTTON_CLOSED: &str = "[▴]";
const BUTTON_OPEN: &str = "[▾]";
const BUTTON_LEN: u32 = 3;

/// The help toggle on the near line (§14 v2/#139/#267): a `[?]` at the top-right
/// corner, three cells wide like the deploy button. Opens the help panel, and —
/// since the panel is modal and carries its own `[x]` — the pair is always
/// escapable, the touch path that never traps (§11.6).
const HELP_BUTTON: &str = "[?]";
const HELP_BUTTON_LEN: u32 = 3;

/// The column the deploy button starts at on a screen `width` wide: right-aligned
/// with a one-cell margin, the corner of the bar nearest the thumb (#267). Shared
/// by the drawing ([`ability_bar`]) and the hit-test ([`is_ability_button`]) so the
/// button a click lands on is exactly the button drawn.
fn button_start(width: u32) -> u32 {
    width.saturating_sub(1 + BUTTON_LEN)
}

/// The column the help button starts at: right-aligned on the near line with a
/// one-cell margin (#267). Shared by the drawing and the hit-test
/// ([`is_help_button`]) so a tap lands on exactly the button drawn.
fn help_button_start(width: u32) -> u32 {
    width.saturating_sub(1 + HELP_BUTTON_LEN)
}

/// Whether screen cell `(x, y)` is the near line's help button (§14 v2/#139) — the
/// `[?]` toggle in the screen's top-right corner (#267). A shell maps a click to a
/// screen cell and asks this; a hit flips [`ScreenUi::help_open`] instead of
/// stepping. Kept beside the drawing so the button a tap lands on is exactly the
/// one drawn.
pub fn is_help_button(width: u32, x: u32, y: u32) -> bool {
    let start = help_button_start(width);
    y == NEAR_ROW && x >= start && x < start + HELP_BUTTON_LEN
}

/// Whether screen cell `(x, y)` is the deploy button (§11.4) — the ability bar's
/// right-aligned toggle on the frame's last row, `height` being the full screen
/// height ([`Grid::height`]). A shell maps a click to a screen cell and asks this;
/// a hit flips [`ScreenUi::ability_panel_open`] instead of stepping. It is the one
/// piece of the button's geometry the shell needs, kept here beside the drawing so
/// the two can never disagree.
pub fn is_ability_button(width: u32, height: u32, x: u32, y: u32) -> bool {
    let start = button_start(width);
    y + BOTTOM_ROWS == height && x >= start && x < start + BUTTON_LEN
}

/// The near line's message-log toggle label (§11.7): when `extra` further messages
/// are stacked behind the loudest, the count and a chevron — down to deploy the
/// list, up to fold it back. Both chevrons are one cell, so the label's width
/// tracks the digit count alone; the drawing ([`draw_message_button`]) and the
/// hit-test ([`is_message_button`]) share it so a tap lands on exactly what is
/// drawn.
fn message_button_label(extra: usize, open: bool) -> String {
    let chevron = if open { '▴' } else { '▾' };
    format!("[+{extra} {chevron}]")
}

/// The column the message-log toggle starts at on a screen `width` wide:
/// immediately left of the near line's help button, so the top-right corner reads
/// as one control cluster `[+2 ▾][?]`.
fn message_button_start(width: u32, label_len: u32) -> u32 {
    help_button_start(width).saturating_sub(label_len)
}

/// Whether screen cell `(x, y)` is the near line's message-log toggle (§11.7) —
/// the counter left of the `[?]` that deploys and folds the full live-message
/// list. A shell maps a click to a screen cell and asks this; a hit flips
/// [`ScreenUi::message_log_open`] instead of stepping. There is no button unless
/// **more than one** message is live, so a lone or absent message yields `false`.
/// The geometry is read from `state` — the count sets the label width, and the
/// near line is the screen's first row (#267) — so a click can never miss the
/// toggle the frame drew.
pub fn is_message_button(state: &State, x: u32, y: u32) -> bool {
    let extra = live_messages(state).len().saturating_sub(1);
    if extra == 0 {
        return false;
    }
    let width = state.layout().facility().width();
    let label_len = message_button_label(extra, false).chars().count() as u32;
    let start = message_button_start(width, label_len);
    y == NEAR_ROW && x >= start && x < start + label_len
}

/// Render the full §11.4 **screen**: the two status lines, the map ([`render`]),
/// and the always-on ability bar beneath it — `TOP_ROWS + map height +
/// BOTTOM_ROWS` rows, same width, one [`Grid`], so a whole frame is still a pure
/// function of `(state, ui)` that prints as text (§11.1) and golden-testable to
/// the last row.
///
/// - **Near line** (row `0`): the highest-priority message of the last action, or
///   the ambient floor ([`near_line`], §11.7) — a solid band in the message's
///   category with the words in Neutral on top — plus the right-aligned help
///   toggle ([`is_help_button`]) and, when more than one message is live, the
///   message-log counter beside it.
/// - **Usable line** (row `1`): the adjacent bump affordances
///   ([`State::affordances`]), each in its own category, no band.
/// - **Ability bar** (row `height-1`): the always-on compact readout — every
///   ability's hotkey coloured by state, its active/cooling number inline
///   ([`AbilityStatus::compact`]) — **right-aligned** against the **deploy
///   button** ([`is_ability_button`]) in the bottom-right corner. This is the
///   permanent home for ability state (§11.4/§15 Q9): one row, glanceable, never
///   covering the board, and under the thumb that taps it (#267).
///
/// # The deployable ability panel (§11.4, §15 Q9)
///
/// When the shell has the panel **deployed** (`ui.ability_panel_open`, driven by
/// the deploy button or the `Tab` toggle), the full named panel — each ability's
/// `<key> <Name> <state>` ([`AbilityStatus::label`]) — is overlaid on the map,
/// rising from the bar it deploys off ([`panel_origin`]): the map's bottom-right
/// corner, so the expanded list reads as the bar unfolded rather than a block that
/// lands somewhere else on the board. It is not tied to waiting: an earlier
/// experiment showed it on the wait turn, which buried exactly the 360° guard-sense
/// the wait exists to reveal (§9.1) — so the panel is on demand, folded away by the
/// same tap that opened it. Both the bar and the panel draw the run's **real**
/// ability state ([`State::ability_statuses`]); a click on either resolves to the
/// ability under it ([`ability_at`]) and activates it exactly as its hotkey would.
pub fn render_screen(state: &State, ui: ScreenUi) -> Grid {
    let facility = state.layout().facility();
    let width = facility.width();
    let height = TOP_ROWS + facility.height() + BOTTOM_ROWS;

    // Help is a modal, full-screen reference (§14 v2/#139/#248): while it is up it
    // takes the *whole* screen — not an overlay on the map — and the shell captures
    // input against it, so nothing of the game frame shows and the other overlays
    // are moot. It writes no state, so closing restores the exact frame. The panel
    // draws itself from the run's active modifiers (§12.6) and the chosen tab.
    if ui.help_open {
        return super::help::render_help(
            width,
            height,
            ui.help_tab,
            state.level(),
            state.modifiers(),
        );
    }

    let statuses = state.ability_statuses();

    // The map layer, with any deployed overlays: the ability panel rising from the
    // bar at the bottom-right, and the near line's message log hanging from the top.
    let mut map = render(state);
    let messages = live_messages(state);
    if ui.ability_panel_open {
        let origin = panel_origin(map.width(), map.height(), &statuses);
        overlay_ability_panel(&mut map, origin, &statuses);
    }
    // The step's live messages (§11.7), loudest first: the near line speaks the
    // first, counts the rest, and deploys the whole list over the board here. The
    // list only earns the board when more than one message is live.
    if ui.message_log_open && messages.len() > 1 {
        overlay_message_log(&mut map, &messages);
    }

    // The near line (§11.4/§11.7): the loudest live message as a category band —
    // or the ambient floor when nothing is live — plus the right-aligned help
    // toggle and, when more than one message is live, the counter beside it.
    let top = messages
        .first()
        .cloned()
        .unwrap_or_else(|| near_line(state));
    let extra = messages.len().saturating_sub(1);
    // The words stop a cell short of the corner cluster — the counter when there is
    // one, the `[?]` otherwise — so a long message never runs under the controls.
    let controls = if extra > 0 {
        let label_len = message_button_label(extra, ui.message_log_open)
            .chars()
            .count() as u32;
        message_button_start(width, label_len)
    } else {
        help_button_start(width)
    };
    let mut near = status_row(
        width,
        controls.saturating_sub(1),
        &[(top.text, Category::Neutral)],
        Some(top.category),
    );
    if extra > 0 {
        draw_message_button(&mut near, width, top.category, extra, ui.message_log_open);
    }
    draw_help_button(&mut near, width, top.category);

    // One grid, top to bottom: the two status lines, the map, the ability bar.
    let mut cells = near;
    let usable: Vec<(String, Category)> = state
        .affordances()
        .into_iter()
        .map(|(dir, a)| (format!("{} {}", arrow(dir), a.label()), a.category()))
        .collect();
    debug_assert_eq!(
        cells.len() as u32,
        USABLE_ROW * width,
        "the usable line follows the near line"
    );
    cells.extend(status_row(width, width, &usable, None));
    cells.extend(map.cells);
    cells.extend(ability_bar(width, &statuses, ui.ability_panel_open));

    Grid {
        width,
        height,
        cells,
    }
}

/// Lay the compact ability bar out (§11.4/#267): the start column of each entry
/// that fits, in draw order, as `(status index, start col)`. The strip is
/// **right-aligned** — it ends one space short of the deploy button, so the whole
/// action cluster hugs the bottom-right corner — with entries in deck order and a
/// single space between them. If the deck outgrows the row the tail is dropped (the
/// abilities furthest from the corner go first) and what remains stays flush right.
/// Shared by [`ability_bar`] (drawing) and [`ability_at`] (hit-testing) so a click
/// can never land on an entry the row did not draw.
fn ability_line_layout(width: u32, statuses: &[AbilityStatus]) -> Vec<(usize, u32)> {
    let lens: Vec<u32> = statuses
        .iter()
        .map(|s| s.compact().chars().count() as u32)
        .collect();
    // Width of the first `n` entries laid end to end with single spaces between.
    let strip = |n: usize| -> u32 {
        match n {
            0 => 0,
            n => lens[..n].iter().sum::<u32>() + n as u32 - 1,
        }
    };
    // The room between the row's one-cell left margin and the space before the
    // button; drop entries from the tail until the strip fits it.
    let room = button_start(width).saturating_sub(2);
    let mut shown = lens.len();
    while shown > 0 && strip(shown) > room {
        shown -= 1;
    }

    let mut x = button_start(width).saturating_sub(1 + strip(shown));
    let mut out = Vec::new();
    for (i, len) in lens.iter().take(shown).enumerate() {
        out.push((i, x));
        x += len + 1; // one space between abilities
    }
    out
}

/// The always-on ability bar (§11.4/#267): the frame's last row, carrying every
/// ability's compact readout ([`AbilityStatus::compact`]) right-aligned against the
/// deploy button ([`is_ability_button`]) in the bottom-right corner, each in its
/// state colour ([`panel_category`]). Single spaces between abilities keep the
/// whole set on one row; the button's chevron points up when closed — the panel
/// grows upward off the bar — and down when open. No band — the bar reads as a
/// quiet HUD strip, not a message.
fn ability_bar(width: u32, statuses: &[AbilityStatus], open: bool) -> Vec<GlyphCell> {
    let blank = GlyphCell {
        glyph: ' ',
        fg: Category::Neutral,
        bg: None,
        vis: Visibility::Live,
    };
    let mut cells = vec![blank; width as usize];

    let put = |cells: &mut [GlyphCell], at: u32, text: &str, category: Category| {
        for (i, glyph) in text.chars().enumerate() {
            let x = at + i as u32;
            if x < width {
                cells[x as usize] = GlyphCell {
                    glyph,
                    fg: category,
                    ..blank
                };
            }
        }
    };

    for (i, start) in ability_line_layout(width, statuses) {
        let status = &statuses[i];
        put(
            &mut cells,
            start,
            &status.compact(),
            panel_category(status.state),
        );
    }

    let label = if open { BUTTON_OPEN } else { BUTTON_CLOSED };
    put(&mut cells, button_start(width), label, Category::System);
    cells
}

/// Draw the help toggle over the already-built near line `row` (§14 v2/#139/#267):
/// [`HELP_BUTTON`] right-aligned in System — the HUD control colour, like the
/// deploy button — over the message's own category band, which keeps painting
/// behind it.
fn draw_help_button(row: &mut [GlyphCell], width: u32, band: Category) {
    let start = help_button_start(width);
    for (i, glyph) in HELP_BUTTON.chars().enumerate() {
        let x = start + i as u32;
        if x < width {
            row[x as usize] = GlyphCell {
                glyph,
                fg: Category::System,
                bg: Some(band),
                vis: Visibility::Live,
            };
        }
    }
}

/// The ability entry at screen cell `(x, y)`, or `None` — the **pure**
/// pointer→identity hit-test for both the always-on bar and the deployed panel
/// (§11.4), the sibling of [`is_ability_button`]. A shell maps a click to a screen
/// cell and asks this; a hit fires `Input::Activate(id)` on the returned ability,
/// resolving by **identity**, never by the row it landed on (§11.6) — so it opens
/// no second activation path (the §8.4 regression) and, on a cooling/active entry,
/// refuses for free in the economy (§4.4) with no turn spent.
///
/// The geometry mirrors [`render_screen`] exactly, drawing from the same shared
/// layout ([`ability_line_layout`]) and panel origin ([`panel_origin`]) the render
/// draws with, so a click can never miss the entry that is shown. The last row is
/// the compact bar; when the panel is deployed, its rows are hit-tested on the map
/// layer between the status rows and the bar. The deploy button is never an ability
/// — the strip stops before it and the shell tests the button first — so a tap
/// there toggles the panel and never falls through to an activation underneath.
pub fn ability_at(state: &State, ui: ScreenUi, x: u32, y: u32) -> Option<AbilityId> {
    let statuses = state.ability_statuses();
    let facility = state.layout().facility();
    let (map_w, map_h) = (facility.width(), facility.height());

    // The last row: the always-on compact bar.
    if y == ability_row(map_h) {
        for (i, start) in ability_line_layout(map_w, &statuses) {
            let len = statuses[i].compact().chars().count() as u32;
            if x >= start && x < start + len {
                return Some(statuses[i].id);
            }
        }
        return None;
    }

    // The deployed panel, overlaid on the map layer under the status rows (§11.4).
    if ui.ability_panel_open && y >= TOP_ROWS {
        let (mx, my) = (x, y - TOP_ROWS);
        let (ox, oy) = panel_origin(map_w, map_h, &statuses);
        let band = panel_band_width(&statuses);
        if mx >= ox && mx < ox + band && my >= oy && my < map_h {
            let row = (my - oy) as usize;
            if row < statuses.len() {
                return Some(statuses[row].id);
            }
        }
    }
    None
}

/// The width of the deployed panel's cleared band (§11.4): one cell wider than the
/// longest label, for an even right edge and a hair of padding off the map. Shared
/// by the origin ([`panel_origin`]), the overlay ([`overlay_ability_panel`]) and
/// the hit-test ([`ability_at`]) so all three agree on the block's footprint.
fn panel_band_width(statuses: &[AbilityStatus]) -> u32 {
    statuses
        .iter()
        .map(|s| s.label().chars().count())
        .max()
        .unwrap_or(0) as u32
        + 1
}

/// The map-space corner to anchor the deployed panel at: the map's **bottom-right**
/// (§11.4/#267), so the named list unfolds directly above the bar it deploys off
/// and the whole ability surface — bar, button, panel — stays one thing in one
/// corner. It used to be pinned to the corner *opposite the player*, which kept it
/// off the action but scattered it across the board; now that deploying is a
/// deliberate tap rather than something a wait turn did for you, the same tap folds
/// it away, so predictable beats evasive. A one-cell inset keeps a border of map
/// around it; sizes are clamped so a tiny hand-built board never underflows (the v1
/// board is 40×40, §10.2). Takes the map dimensions rather than the [`Grid`] so the
/// hit-test can reuse it without a rendered frame.
fn panel_origin(map_w: u32, map_h: u32, statuses: &[AbilityStatus]) -> (u32, u32) {
    let panel_w = panel_band_width(statuses);
    let panel_h = statuses.len() as u32;
    let x0 = map_w.saturating_sub(panel_w + 1);
    let y0 = map_h.saturating_sub(panel_h + 1);
    (x0.max(1).min(map_w.saturating_sub(1)), y0.max(1))
}

/// Overlay the deployed ability panel onto the map `grid` at `(ox, oy)` (§11.4):
/// one row per ability, each `<key> <Name> <state>` ([`AbilityStatus::label`])
/// coloured by state ([`panel_category`]). Every row is cleared to a uniform width
/// first so the block reads as a solid panel over the board rather than text
/// tangled with the map beneath.
///
/// Bounds are clamped, never asserted: on a board too small to hold every row (only
/// hand-built test states get that small — the v1 board is 40×40, §10.2) the panel
/// shows as many abilities as fit and stops. It draws over the map layer only,
/// before the header and status rows are added, so it can never collide with them.
fn overlay_ability_panel(grid: &mut Grid, origin: (u32, u32), statuses: &[AbilityStatus]) {
    let (ox, oy) = origin;
    // A uniform band, one space wider than the longest label, so the cleared box
    // has an even right edge and a hair of padding off the map.
    let width = panel_band_width(statuses);

    for (i, status) in statuses.iter().enumerate() {
        let y = oy + i as u32;
        if y >= grid.height {
            break; // out the bottom of a tiny board — show what fits, drop the rest
        }
        // Clear the row's band to background, then write the label over it.
        for dx in 0..width {
            let x = ox + dx;
            if x >= grid.width {
                break;
            }
            grid.cells[(y * grid.width + x) as usize] = GlyphCell {
                glyph: ' ',
                fg: Category::Neutral,
                bg: None,
                vis: Visibility::Live,
            };
        }
        let category = panel_category(status.state);
        for (dx, glyph) in status.label().chars().enumerate() {
            let x = ox + dx as u32;
            if x >= grid.width {
                break;
            }
            grid.cells[(y * grid.width + x) as usize] = GlyphCell {
                glyph,
                fg: category,
                bg: None,
                vis: Visibility::Live,
            };
        }
    }
}

/// The §11.2 category an ability row reads in, by its state: an available ability
/// — ready or active — is **Owned** (blue, "yours, in hand"); a cooling one is
/// **System** (the muted furniture tan, "unavailable, will return"); an unusable
/// one is **Ground** (dim gray, receding) — discoverable but plainly not an option
/// now. The `[N]` / `/N/` notation carries the rest, so ready and active share a
/// colour without ambiguity.
fn panel_category(state: AbilityState) -> Category {
    match state {
        AbilityState::Ready | AbilityState::Active { .. } => Category::Owned,
        AbilityState::Cooling { .. } => Category::System,
        AbilityState::Unusable => Category::Ground,
    }
}

/// The usable line's direction glyph (§11.4): which way to bump for the
/// affordance beside it.
fn arrow(dir: Direction) -> char {
    match dir {
        Direction::North => '↑',
        Direction::East => '→',
        Direction::South => '↓',
        Direction::West => '←',
    }
}

/// Lay one status row out as grid cells: segments left to right from a one-cell
/// margin, two spaces between segments, stopping at column `limit`; `band` paints
/// every cell's background (the §11.4 message band) or none. The row is `width`
/// cells wide either way — `limit` only bounds the *words*, so the near line's
/// corner controls (#267) keep their cells instead of being written over by a long
/// message.
fn status_row(
    width: u32,
    limit: u32,
    segments: &[(String, Category)],
    band: Option<Category>,
) -> Vec<GlyphCell> {
    let blank = GlyphCell {
        glyph: ' ',
        fg: Category::Neutral,
        bg: band,
        vis: Visibility::Live,
    };
    let mut cells = vec![blank; width as usize];
    let limit = (limit as usize).min(cells.len());
    let mut x = 1; // the one-cell left margin
    for (i, (text, category)) in segments.iter().enumerate() {
        if i > 0 {
            x += 2;
        }
        for glyph in text.chars() {
            if x >= limit {
                return cells;
            }
            cells[x] = GlyphCell {
                glyph,
                fg: *category,
                ..blank
            };
            x += 1;
        }
    }
    cells
}

/// Draw the message-log toggle over the already-built near line `row` (§11.7):
/// the [`message_button_label`] right-aligned, its glyphs in System — the HUD
/// control colour, like the ability line's deploy button — over the loudest
/// message's own category band, which keeps painting behind it.
fn draw_message_button(
    row: &mut [GlyphCell],
    width: u32,
    band: Category,
    extra: usize,
    open: bool,
) {
    let label = message_button_label(extra, open);
    let start = message_button_start(width, label.chars().count() as u32);
    for (i, glyph) in label.chars().enumerate() {
        let x = start + i as u32;
        if x < width {
            row[x as usize] = GlyphCell {
                glyph,
                fg: Category::System,
                bg: Some(band),
                vis: Visibility::Live,
            };
        }
    }
}

/// Overlay the deployed message log onto the map `grid` (§11.7/#267): the step's
/// live messages ([`live_messages`]), one per row, **hanging from the near line** —
/// at the map's top-left, the loudest on the first row directly below its own
/// near-line band, each quieter message one row lower. Every row is cleared to a
/// uniform band — a one-cell margin, the longest message, a cell of pad — then the
/// words drawn in the message's own §11.2 category, so the list reads as a solid
/// block over the board and each entry keeps its threat colour, aligned with the
/// band above.
///
/// Bounds are clamped, never asserted: on a board too short to hold every row
/// (only hand-built test states get that small — the v1 board is 40×40, §10.2)
/// the block shows as many as fit from the top and drops the rest.
fn overlay_message_log(grid: &mut Grid, messages: &[Message]) {
    let (width, map_h) = (grid.width, grid.height);
    let band = (messages
        .iter()
        .map(|m| m.text.chars().count())
        .max()
        .unwrap_or(0) as u32
        + 2)
    .min(width);
    let blank = GlyphCell {
        glyph: ' ',
        fg: Category::Neutral,
        bg: None,
        vis: Visibility::Live,
    };
    for (y, message) in messages.iter().enumerate() {
        let y = y as u32;
        if y >= map_h {
            break; // out the bottom of a tiny board — show what fits from the top
        }
        for dx in 0..band {
            grid.cells[(y * width + dx) as usize] = blank;
        }
        // A one-cell left margin, matching the near line, so the list lines up
        // under the band it hangs from.
        for (dx, glyph) in message.text.chars().enumerate() {
            let x = 1 + dx as u32;
            if x >= band {
                break;
            }
            grid.cells[(y * width + x) as usize] = GlyphCell {
                glyph,
                fg: message.category,
                ..blank
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, Direction};
    use crate::guard::Guard;
    use crate::modifiers::LevelModifiers;
    use crate::state::{Input, State};
    use crate::test_support::open_room;

    /// §11.7: when one step raises more than one message the near line speaks the
    /// loudest as its band and shows a right-aligned counter of the rest; deploying
    /// the list ([`ScreenUi::message_log_open`]) stacks every message over the
    /// board, loudest on the row directly above the band. A board wide enough that
    /// the messages are not truncated.
    #[test]
    fn the_near_line_counts_extra_messages_and_deploys_the_list() {
        // The takedown-seen-by-a-witness step: `TakenDown` (priority 0) and
        // `BodyFound` (priority 4) land the same turn — two live messages.
        let mut layout = open_room(40, 14);
        layout.place(Cell::new(5, 5), Terrain::Hideout);
        let mut s = State::new(
            layout,
            Cell::new(5, 5),
            Direction::North,
            vec![
                Guard::stationary(Cell::new(5, 4)),
                Guard::stationary(Cell::new(5, 2)),
            ],
            Vec::new(),
            Cell::new(8, 8),
        );
        s.step(Input::Step(Direction::North));

        let width = s.layout().facility().width();
        let row_text = |g: &Grid, y: u32| (0..width).map(|x| g.get(x, y).glyph).collect::<String>();

        // Collapsed: the band speaks the loudest message and the closed counter of
        // the one further message (a down chevron) sits at the right.
        let g = render_screen(&s, ScreenUi::default());
        let near = row_text(&g, NEAR_ROW);
        assert!(
            near.contains("a body has been found"),
            "the band speaks the loudest message: {near:?}"
        );
        assert!(
            near.contains("[+1 ▾][?]"),
            "a closed counter of the rest, beside the help toggle: {near:?}"
        );

        // The hit-test agrees with the drawn counter, and there is no button off it.
        let label_len = "[+1 ▾]".chars().count() as u32;
        let start = help_button_start(width) - label_len;
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
            row_text(&g, NEAR_ROW).contains("[+1 ▴]"),
            "the deployed counter points up"
        );
        assert!(
            row_text(&g, TOP_ROWS).contains("a body has been found"),
            "the loudest sits nearest the band"
        );
        assert!(
            row_text(&g, TOP_ROWS + 1).contains("the guard drops — a body is left"),
            "the rest stack below it"
        );
    }

    /// §11.7: a single live message shows no counter — the near line is the plain
    /// band it has always been, and the message-log toggle is not a button.
    #[test]
    fn a_lone_message_shows_no_counter() {
        // Taking the intel is one loud message and nothing else this step.
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
        let near: String = (0..width)
            .map(|x| {
                render_screen(&s, ScreenUi::default())
                    .get(x, NEAR_ROW)
                    .glyph
            })
            .collect();
        assert!(
            !near.contains("[+"),
            "no counter for a lone message: {near:?}"
        );
        assert!(
            near.trim_end().ends_with("[?]"),
            "the help toggle keeps the corner: {near:?}"
        );
        assert!(
            (0..width).all(|x| !is_message_button(&s, x, NEAR_ROW)),
            "and nothing to click"
        );
    }

    /// The §11.4 golden test, whole screen (#267): the near and usable lines on
    /// top, then the map, then the always-on ability bar — one grid, printed as
    /// text. The near line rests on ambient floor and carries the `[?]` toggle in
    /// the top-right corner; the usable line offers the adjacent console; the bar
    /// carries the compact ability readout and the closed deploy button, both
    /// flush to the bottom-right. With the panel not deployed the board is whole.
    #[test]
    fn the_full_screen_renders_golden() {
        let s = State::new(
            open_room(24, 6),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            [Cell::new(3, 2)], // a console east of the player
            Cell::new(22, 4),
        );
        let text = render_screen(&s, ScreenUi::default()).to_text();
        assert_eq!(
            text,
            vec![
                " intel remaining: 1 [?] ".to_string(),
                " → console: take intel  ".to_string(),
                "########################".to_string(),
                "#······················#".to_string(),
                "#·@$···················#".to_string(),
                "#······················#".to_string(),
                "#·····················E#".to_string(),
                "########################".to_string(),
                "        r c d x a z [▴] ".to_string(),
            ]
        );
    }

    /// The screen is the map plus the header and status rows, same width — and the
    /// two status rows carry their §11.4 styling: the near line is a full-width
    /// band in the message's category with Neutral words on top; the usable line
    /// has no band and speaks each affordance's own category.
    #[test]
    fn status_rows_carry_the_band_and_the_categories() {
        let mut s = State::new(
            open_room(24, 6),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            [Cell::new(3, 2)],
            Cell::new(22, 4),
        );
        let map = render(&s);
        let g = render_screen(&s, ScreenUi::default());
        assert_eq!(g.width(), map.width());
        assert_eq!(g.height(), TOP_ROWS + map.height() + BOTTOM_ROWS);

        let (near_y, usable_y) = (NEAR_ROW, USABLE_ROW);
        let help = help_button_start(g.width());
        for x in 0..g.width() {
            let cell = g.get(x, near_y);
            assert_eq!(cell.bg, Some(Category::Interest), "the band spans the row");
            assert_eq!(cell.vis, Visibility::Live);
            if cell.glyph != ' ' && x < help {
                assert_eq!(cell.fg, Category::Neutral, "words read Neutral on the band");
            }
            assert_eq!(g.get(x, usable_y).bg, None, "the usable line has no band");
        }
        // The `[?]` rides the band in the HUD control colour, not the words' Neutral.
        assert_eq!(g.get(help, near_y).glyph, '[');
        assert_eq!(g.get(help, near_y).fg, Category::System);
        // The affordance leads with its bump direction and speaks its own
        // category: `→ console: take intel` is Interest (§11.2 — goals and
        // rewards), and the console is east of the player.
        assert_eq!(g.get(1, usable_y).glyph, '→');
        assert_eq!(g.get(1, usable_y).fg, Category::Interest);
        assert_eq!(g.get(3, usable_y).glyph, 'c');

        // A threat message flips the whole band to its category: get captured
        // and the near line reads Danger — the colour flash before the words.
        s = State::new(
            open_room(24, 6),
            Cell::new(2, 3),
            Direction::North,
            // Walking south, its spawn facing, straight into the player — no corner,
            // so no §229 turn tax delays the contact.
            vec![Guard::patrolling_to(Cell::new(2, 1), Cell::new(2, 4))],
            Vec::new(),
            Cell::new(22, 4),
        );
        s.step(Input::Wait); // the guard steps south into the player: caught
        let g = render_screen(&s, ScreenUi::default());
        assert_eq!(g.get(0, NEAR_ROW).bg, Some(Category::Danger));
        assert_eq!(g.get(1, NEAR_ROW).glyph, 'c'); // "caught"
    }

    /// The permanent home of ability state (§11.4/#267): the **always-on ability
    /// bar** on the frame's last row, assembled from the run's real economy
    /// ([`State::ability_statuses`]). A fresh run has every ability ready, so the bar
    /// is the economy keys in deck order — each the bare §11.6 hotkey in Owned —
    /// **right-aligned** against the deploy button in the bottom-right corner. The
    /// two bump verbs (Takedown `t`, Drag `g`) are **not** on it: they live on the
    /// usable line, not the ability economy (§7.2/§8.3).
    #[test]
    fn the_always_on_bar_shows_every_economy_ability() {
        use crate::input::ability_hotkey;

        let s = State::new(
            open_room(30, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 8),
        );
        let g = render_screen(&s, ScreenUi::default());
        let bar = ability_row(10);
        assert_eq!(bar + BOTTOM_ROWS, g.height(), "the bar is the last row");

        // Six ready abilities, one cell each with a space between: an 11-wide strip
        // ending one space short of the button at col 26 — so it starts at col 14.
        for (col, name, glyph) in [
            (14, "Run", 'r'),
            (16, "Camouflage", 'c'),
            (18, "Decoy", 'd'),
            (20, "Dephase", 'x'),
        ] {
            assert_eq!(g.get(col, bar).glyph, glyph, "{name} at col {col}");
            assert_eq!(g.get(col, bar).fg, Category::Owned, "{name} ready colour");
            assert_eq!(Some(glyph), ability_hotkey(name), "{name} hotkey");
        }
        // Nothing before the strip: the bar hugs the right edge, it is not a
        // left-aligned header any more.
        let row: String = (0..g.width()).map(|x| g.get(x, bar).glyph).collect();
        assert!(
            row.starts_with("              r"),
            "the strip is flush right: {row:?}"
        );
        // The bump verbs never appear on the ability bar.
        assert!(!row.contains('t'), "Takedown is not an economy ability");
        assert!(!row.contains('g'), "Drag is not an economy ability");

        // The deploy button, closed, in the bottom-right corner — and
        // `is_ability_button` agrees with where it is drawn.
        let (w, h) = (g.width(), g.height());
        let start = w - 1 - 3;
        assert!(is_ability_button(w, h, start, bar));
        assert!(
            !is_ability_button(w, h, start - 1, bar),
            "just left is not the button"
        );
        assert!(
            !is_ability_button(w, h, start, bar - 1),
            "the row above is the map, not the button"
        );
        assert!(
            !is_ability_button(w, h, start, 0),
            "the near line is not the button either"
        );
        assert_eq!(g.get(start, bar).glyph, '[');
        assert_eq!(g.get(start, bar).fg, Category::System);
    }

    /// The line's live states (§11.4): an **active** ability tucks its `[n]` against
    /// the key in Owned, a **cooling** one its `/n/` in System — the exact numbers
    /// the economy hands over (§8.2). Driven to Run cooling and Camouflage active,
    /// with Decoy and Dephase still ready, so all three notations show at once.
    #[test]
    fn the_line_shows_active_and_cooling_state() {
        let mut s = State::new(
            open_room(30, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 8),
        );
        // Run: activate (Active 4 after the turn's tick) then toggle off — a free
        // action that drops it straight into its full 12 cooldown. Then activate
        // Camouflage: that turn's tick drains Run's cooldown to 11 and leaves
        // Camouflage active with 9 of its 10 left.
        s.step(Input::Activate(AbilityId::Run));
        s.step(Input::Deactivate(AbilityId::Run));
        s.step(Input::Activate(AbilityId::Camouflage));
        assert_eq!(
            s.ability_state(AbilityId::Run),
            AbilityState::Cooling { remaining: 11 }
        );
        assert_eq!(
            s.ability_state(AbilityId::Camouflage),
            AbilityState::Active { remaining: 9 }
        );

        let g = render_screen(&s, ScreenUi::default());
        let bar = ability_row(10);
        let row: String = (0..g.width()).map(|x| g.get(x, bar).glyph).collect();
        // `r/11/` cooling (System), `c[9]` active (Owned), then the ready keys —
        // wider entries push the strip left, but its right edge stays put.
        assert!(
            row.contains("r/11/ c[9] d x a z [▴]"),
            "the live ability bar: {row:?}"
        );
        let run = row.find("r/11/").expect("Run's entry") as u32;
        assert_eq!(g.get(run, bar).fg, Category::System, "cooling reads System");
        assert_eq!(g.get(run + 1, bar).glyph, '/', "cooling shows /N/");
        let cam = run + 6;
        assert_eq!(g.get(cam, bar).glyph, 'c');
        assert_eq!(g.get(cam, bar).fg, Category::Owned, "active reads Owned");
        assert_eq!(g.get(cam + 1, bar).glyph, '[', "active shows [N]");
    }

    /// The move is a **projection**, not a rebinding (§11.6/#267): wherever the bar
    /// sits, every entry on it resolves to the ability its settled hotkey fires, and
    /// each of the four ability states still reads its own colour — ready and active
    /// Owned, cooling System, unusable Ground — so the states stay discoverable in
    /// the new corner.
    #[test]
    fn the_bar_still_projects_the_settled_hotkeys_and_states() {
        use crate::input::ability_input_for_key;

        let mut s = State::new(
            open_room(30, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 8),
        );
        let ui = ScreenUi::default();
        let bar = ability_row(10);

        // Every entry the bar draws resolves to the very id its hotkey fires.
        for (i, start) in ability_line_layout(30, &s.ability_statuses()) {
            let id = s.ability_statuses()[i].id;
            let key = crate::input::ability_hotkey(id.name()).expect("a settled hotkey");
            assert_eq!(
                ability_at(&s, ui, start, bar),
                Some(id),
                "{id:?} under its own entry"
            );
            assert_eq!(
                ability_input_for_key(&key.to_string()),
                Some(Input::Activate(id)),
                "{key} still fires {id:?}"
            );
        }

        // The state colours, in the corner: Run cooling, Camouflage active, the rest
        // ready, and Dephase driven unusable by holding a body (§8.3).
        s.step(Input::Activate(AbilityId::Run));
        s.step(Input::Deactivate(AbilityId::Run));
        s.step(Input::Activate(AbilityId::Camouflage));
        let g = render_screen(&s, ScreenUi::default());
        let entry = |id: AbilityId| {
            let statuses = s.ability_statuses();
            let i = statuses.iter().position(|st| st.id == id).expect("in deck");
            ability_line_layout(30, &statuses)
                .into_iter()
                .find(|(j, _)| *j == i)
                .expect("drawn")
                .1
        };
        assert_eq!(g.get(entry(AbilityId::Run), bar).fg, Category::System);
        assert_eq!(g.get(entry(AbilityId::Camouflage), bar).fg, Category::Owned);
        assert_eq!(g.get(entry(AbilityId::Decoy), bar).fg, Category::Owned);
    }

    /// Deploying the panel (§11.4/#267) unfolds the named ability list **upward off
    /// the bar** — the map's bottom-right corner, directly above the strip it
    /// deploys from — and it is gone the moment the panel is not deployed. The
    /// anchor is fixed, not chased around the board by the player's position: the
    /// same tap that opened it folds it away.
    #[test]
    fn deploying_unfolds_the_panel_above_the_bar() {
        // On a fresh run the widest label is `c Camouflage` (12) → a 13-wide band,
        // six rows on a 30×14 map: map origin (16,7), so the panel's first row sits
        // at screen (16,9) (map row + the two status rows) and its last at (16,14),
        // one row of board above the bar at 16.
        let s = State::new(
            open_room(30, 14),
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 12),
        );
        let closed = render_screen(&s, ScreenUi::default());
        let open = render_screen(
            &s,
            ScreenUi {
                ability_panel_open: true,
                ..ScreenUi::default()
            },
        );
        assert_eq!(
            closed.get(16, 9).glyph,
            '·',
            "not deployed: the board is whole"
        );
        assert_eq!(
            open.get(16, 9).glyph,
            'r',
            "deployed: the panel's first row"
        );
        assert_eq!(open.get(18, 9).glyph, 'R', "…the label reads `r Run`");
        assert_eq!(open.get(16, 9).fg, Category::Owned);
        assert_eq!(
            open.get(16, 14).glyph,
            'z',
            "…and its last row, above the bar"
        );
        // The rest of the board is untouched — the panel is one corner block.
        assert_eq!(open.get(2, 2).glyph, '#', "the far corner stays board");

        // The anchor does not move with the player: a player standing in that very
        // corner still gets the panel there (it folds away with the same tap).
        let s2 = State::new(
            open_room(30, 14),
            Cell::new(24, 11),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(1, 1),
        );
        let open2 = render_screen(
            &s2,
            ScreenUi {
                ability_panel_open: true,
                ..ScreenUi::default()
            },
        );
        assert_eq!(open2.get(16, 9).glyph, 'r', "the corner is fixed");
    }

    /// The deployed panel clamps to a board too small to hold every row rather than
    /// panicking — only hand-built states get this small (the v1 board is 40×40),
    /// but the renderer must never index off the grid.
    #[test]
    fn the_deployed_panel_clamps_on_a_tiny_board() {
        let s = State::new(
            open_room(24, 4),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(22, 2),
        );
        // A 4-tall map cannot fit all six panel rows within its inset; the render
        // shows what fits and stops — no panic, and the screen height is intact.
        let g = render_screen(
            &s,
            ScreenUi {
                ability_panel_open: true,
                ..ScreenUi::default()
            },
        );
        assert_eq!(g.height(), TOP_ROWS + 4 + BOTTOM_ROWS);
        // The panel clamps to the map's top inset: its first row draws at map
        // (10,1), screen (10,3).
        assert_eq!(g.get(10, 3).glyph, 'r', "the first row still draws");
    }

    /// The pointer→identity hit-test (§11.4) on the always-on bar: each compact
    /// entry's cells resolve to *that* ability by identity, the gaps and the deploy
    /// button resolve to nothing (a tap there toggles the panel, it never falls
    /// through to an activation), and the map above is not the bar.
    #[test]
    fn ability_at_resolves_the_compact_bar() {
        let s = State::new(
            open_room(30, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 8),
        );
        let ui = ScreenUi::default();
        let bar = ability_row(10);

        // r@14 c@16 d@18 x@20 a@22 z@24 (all ready → one cell each, flush right),
        // by identity not position.
        for (col, id) in [
            (14, AbilityId::Run),
            (16, AbilityId::Camouflage),
            (18, AbilityId::Decoy),
            (20, AbilityId::Dephase),
            (22, AbilityId::Autodoors),
            (24, AbilityId::Confusion),
        ] {
            assert_eq!(ability_at(&s, ui, col, bar), Some(id), "col {col}");
        }
        // The space between entries is no ability, and neither is the empty left
        // half of the bar the strip no longer reaches.
        assert_eq!(
            ability_at(&s, ui, 15, bar),
            None,
            "the gap resolves to nothing"
        );
        assert_eq!(
            ability_at(&s, ui, 1, bar),
            None,
            "nor the bar's left margin"
        );
        // The deploy button is never an ability, even though it is on the same row —
        // the strip stops before it, so a tap there cannot fall through.
        let start = 30 - 1 - 3;
        assert!(is_ability_button(30, bar + BOTTOM_ROWS, start, bar));
        assert_eq!(
            ability_at(&s, ui, start, bar),
            None,
            "the button is not an ability"
        );
        // The map above the bar is not the bar while the panel is closed.
        assert_eq!(
            ability_at(&s, ui, 14, bar - 1),
            None,
            "the row above is map"
        );
    }

    /// The hit-test on the **deployed panel** (§11.4): its rows, overlaid on the map
    /// beneath the header, resolve by identity to the ability they draw; cells off
    /// the band are nothing; and with the panel closed the same cells are just map.
    #[test]
    fn ability_at_resolves_the_deployed_panel() {
        // Same geometry as `deploying_unfolds_the_panel_above_the_bar`: a fresh run,
        // the panel at map origin (16,7) → screen rows from 9.
        let s = State::new(
            open_room(30, 14),
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 12),
        );
        let open = ScreenUi {
            ability_panel_open: true,
            ..ScreenUi::default()
        };

        // One panel row per economy ability, top to bottom in deck order. Six
        // abilities anchor one row higher than five did — the panel grows upward
        // from the board's bottom edge, above the bar it unfolds from (#267).
        for (screen_y, id) in [
            (9, AbilityId::Run),
            (10, AbilityId::Camouflage),
            (11, AbilityId::Decoy),
            (12, AbilityId::Dephase),
            (13, AbilityId::Autodoors),
            (14, AbilityId::Confusion),
        ] {
            assert_eq!(
                ability_at(&s, open, 16, screen_y),
                Some(id),
                "row at y {screen_y}"
            );
        }
        // A cell left of the band is not the panel; nor is it while the panel closes.
        assert_eq!(ability_at(&s, open, 2, 10), None, "off the band");
        assert_eq!(
            ability_at(&s, ScreenUi::default(), 16, 10),
            None,
            "closed: the panel is not hit-testable"
        );
    }

    /// The click **is** the hotkey (§11.4/§11.6): the id a line cell resolves to is
    /// the very id its §11.6 shortcut fires, and firing it drives the one
    /// `Input::Activate` path — so a click activates a ready ability and, on a
    /// cooling one, refuses for free with no turn spent (§4.4), exactly as the key.
    #[test]
    fn a_click_activates_by_the_same_path_as_the_hotkey() {
        use crate::input::ability_input_for_key;

        let mut s = State::new(
            open_room(30, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 8),
        );
        let ui = ScreenUi::default();
        let bar = ability_row(10);

        // The bar's Run cell resolves to the same id `r` fires — one path, by identity.
        let clicked = ability_at(&s, ui, 14, bar).expect("Run under the pointer");
        assert_eq!(
            ability_input_for_key("r"),
            Some(Input::Activate(clicked)),
            "the click and the shortcut resolve to the same activation",
        );

        // A click on a ready ability activates it (a spent turn).
        let events = s.step(Input::Activate(clicked));
        assert_eq!(s.turn(), 1, "activating spends the turn");
        assert!(!events.is_empty(), "the ability activated");

        // Drive Run to cooling, then a click on its (now cooling) entry refuses
        // cleanly: the same `Input::Activate` is a free no-op — no turn, no change.
        s.step(Input::Deactivate(AbilityId::Run));
        assert!(matches!(
            s.ability_state(AbilityId::Run),
            AbilityState::Cooling { .. }
        ));
        let cooling = ability_at(&s, ui, 14, bar).expect("Run still under the pointer");
        let turn_before = s.turn();
        let refused = s.step(Input::Activate(cooling));
        assert!(refused.is_empty(), "a cooling entry refuses");
        assert_eq!(s.turn(), turn_before, "the mis-click spends no turn");
    }

    /// A message longer than the row truncates at the edge instead of
    /// panicking or wrapping — the status rows are single grid rows.
    #[test]
    fn a_long_status_line_truncates_at_the_edge() {
        let mut s = State::new(
            open_room(12, 6),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            [Cell::new(3, 2)],
            Cell::new(10, 4),
        );
        s.step(Input::Step(Direction::East)); // take the intel: a long message
        let g = render_screen(&s, ScreenUi::default());
        let near: String = (0..g.width()).map(|x| g.get(x, NEAR_ROW).glyph).collect();
        assert_eq!(near.chars().count(), 12, "exactly one grid row wide");
        assert!(
            near.starts_with(" all th "),
            "the words stop a cell short of the corner control: {near:?}"
        );
        assert!(near.ends_with("[?] "), "…which keeps its cells: {near:?}");
    }

    // --- Help panel (§14 v2/#139/#248) ---------------------------------------

    /// A plain full board to render the help panel over.
    fn help_board() -> State {
        State::new(
            open_room(40, 40),
            Cell::new(10, 10),
            Direction::North,
            vec![Guard::stationary(Cell::new(20, 20))],
            Vec::new(),
            Cell::new(30, 30),
        )
    }

    /// §4.4/#139/#248: opening help is a pure view toggle. It changes the frame
    /// while up, and **closing restores the exact frame** — the panel writes no
    /// state, so the frame beneath is byte-identical before and after. The open
    /// frame is the full screen, the game frame's exact size.
    #[test]
    fn help_is_a_modal_full_screen_frame_and_closing_restores_it() {
        let s = help_board();
        let closed = render_screen(&s, ScreenUi::default());
        let open = render_screen(
            &s,
            ScreenUi {
                help_open: true,
                ..ScreenUi::default()
            },
        );
        assert_ne!(open, closed, "the help panel changes the frame while up");
        // The panel is the full screen — the same width and height as the game frame.
        assert_eq!(open.width(), closed.width());
        assert_eq!(open.height(), closed.height());
        let reclosed = render_screen(&s, ScreenUi::default());
        assert_eq!(reclosed, closed, "closing restores the identical frame");
    }

    /// The open frame **is** the modal panel (#248): the whole screen, not an
    /// overlay on the map. Row 0 is the tab bar — the tab labels and the `[x]` close
    /// — not the game's ability line, and the run's active modifiers show through
    /// from `state.modifiers()`.
    #[test]
    fn the_open_frame_is_the_panel_showing_the_run_modifiers() {
        // A run carrying one harder modifier, threaded as the real game would.
        let s = help_board().with_modifiers(LevelModifiers {
            guards_always_search_hideouts: true,
            ..LevelModifiers::default()
        });
        let g = render_screen(
            &s,
            ScreenUi {
                help_open: true,
                ..ScreenUi::default()
            },
        );
        let row0: String = (0..g.width()).map(|x| g.get(x, 0).glyph).collect();
        // The tab bar, not the near line: both tabs and the close control.
        assert!(
            row0.contains("[Level info]"),
            "the tab bar heads the panel: {row0:?}"
        );
        assert!(row0.contains("[Legend]"));
        assert!(row0.contains("[x]"), "a touchable close control");
        assert!(
            !row0.contains("intel remaining"),
            "the game's own near line is gone while modal"
        );
        // The default tab is Level info, so the active modifier reads through.
        let text = g.to_text().join("\n");
        assert!(
            text.contains("Guards search hideouts"),
            "the run's modifier shows"
        );
    }

    /// #272, end to end: a **booted** run's help panel shows that run's own
    /// level-seed string — the whole chain, `start_level` → `State::level` →
    /// `render_screen` → the Level info tab — and looking at it is still free: no
    /// turn, no state written, the frame beneath byte-identical afterwards (§4.4).
    #[test]
    fn the_help_panel_of_a_booted_run_shows_its_seed_for_free() {
        use crate::level_seed::{start_level, LevelSeed};

        let level = LevelSeed::quick_play(8371);
        let s = start_level(&level).expect("the v1 recipe places");
        let before = s.turn();
        let closed = render_screen(&s, ScreenUi::default());
        let open = render_screen(
            &s,
            ScreenUi {
                help_open: true,
                ..ScreenUi::default()
            },
        );
        let text = open.to_text().join("\n");
        assert!(text.contains("LEVEL SEED"), "the section is there");
        assert!(
            text.contains(&level.encode_full()),
            "…showing this run's own token, in full"
        );
        assert_eq!(s.turn(), before, "looking costs no turn");
        assert_eq!(
            render_screen(&s, ScreenUi::default()),
            closed,
            "and writes no state"
        );
    }

    /// §11.6's no-trap rule, kept for the full-screen panel (#248): with the near
    /// line's `[?]` now covered, the panel carries its own escape — the `[x]` close control
    /// hit-tests to [`HelpHit::Close`], and each tab tap switches — while the near
    /// line's `[?]` still opens it when the panel is closed.
    #[test]
    fn the_panel_is_reachable_to_open_and_escapable_once_open() {
        let s = help_board();
        let width = s.layout().facility().width();

        // Closed: the near line's `[?]` opens the panel (its hit-test, and it is
        // drawn).
        let start = help_button_start(width);
        assert!(
            is_help_button(width, start, NEAR_ROW),
            "the [?] cell hit-tests"
        );
        let closed = render_screen(&s, ScreenUi::default());
        let near: String = (0..width).map(|x| closed.get(x, NEAR_ROW).glyph).collect();
        assert!(
            near.contains("[?]"),
            "closed: the near line offers [?]: {near:?}"
        );

        // Open: the panel is escapable by touch — the `[x]` closes, a tab switches.
        assert!(matches!(
            help_hit(width, width - 2, 0),
            Some(HelpHit::Close)
        ));
        assert!(matches!(help_hit(width, 2, 0), Some(HelpHit::Tab(_))));
    }

    /// The help button and the ability deploy button sit in **opposite corners**
    /// (§11.4/#139/#267) — `[?]` top-right on the near line, `[▾]` bottom-right on
    /// the bar — so neither can swallow the other's tap even though both are
    /// right-aligned to the same column.
    #[test]
    fn the_corner_buttons_are_distinct() {
        let (width, height) = (40, 43); // TOP_ROWS + 40 + BOTTOM_ROWS
        let bar = height - BOTTOM_ROWS;
        assert_eq!(
            help_button_start(width),
            button_start(width),
            "both hug the right margin — only the row tells them apart"
        );
        for x in help_button_start(width)..help_button_start(width) + HELP_BUTTON_LEN {
            assert!(is_help_button(width, x, NEAR_ROW));
            assert!(
                !is_ability_button(width, height, x, NEAR_ROW),
                "help ≠ deploy at {x}"
            );
        }
        for x in button_start(width)..button_start(width) + BUTTON_LEN {
            assert!(is_ability_button(width, height, x, bar));
            assert!(!is_help_button(width, x, bar), "deploy ≠ help at {x}");
        }
    }
}
