//! On-screen HUD composition for the character grid (§11.4, §11.7).
//!
//! The world render — terrain, fog, entities, the danger overlay — lives in the
//! parent [`render`](super) module as a pure state→[`Grid`](super::Grid). This
//! module owns the *chrome* laid over and around it: the always-on ability bar,
//! the near line and its message log, the status rows, and the click hit-tests
//! that pair with them. [`render_screen`] composes the two — it draws the world
//! grid, then overlays this. Both halves stay a pure function of state
//! (§11.1/§12.1) and golden-grid testable.
//!
//! # Which way up (§11.4, §15 Q9, #267)
//!
//! The chrome is laid out for **thumb reach**: the *action* surface — the ability
//! bar — sits at the **bottom right**, where a hand holding the device already
//! rests, and the *read-only* status — the near line, the usable line, the help
//! toggle — sits at the **top**, out of the way of both the thumb and the board.
//! It used to be the other way round (ability line as a header, status lines at
//! the foot), which put the one row you tap furthest from the one finger you tap
//! it with.
//!
//! # The bar names everything, and it provably fits (§11.4, #287)
//!
//! A run holds at most [`AbilityId::MAX_HELD`] economy abilities (§8.3), so the bar
//! can carry each one's **name** outright — no hotkey-only compaction, no deploy
//! button, no panel unfolding over the board. Fitting four names and their `[N]` /
//! `/N/` numbers across a 40-wide board (§10.2) is a tight budget, and
//! [`MAX_BAR_WIDTH`] spends it under a `const` assertion: a longer bar name, a
//! three-digit cooldown or a bigger tech grant breaks the **build**, not the frame.

use super::*;
use crate::ability::{AbilityId, AbilityState, AbilityStatus, MAX_BAR_ENTRY};
use crate::cell::Direction;
use crate::place::LevelConfig;
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
/// of the frame. Shared by the drawing ([`render_screen`]) and the hit-test
/// ([`ability_at`]) so a tap lands on the bar that is drawn.
fn ability_row(map_h: u32) -> u32 {
    TOP_ROWS + map_h
}

/// The trailing cell of every ability-bar slot (§11.4): one cell of air after the
/// entry, separating it from the next — and, on the last slot, from the frame's
/// edge, so the strip never runs into the corner.
const BAR_GAP: u32 = 1;

/// One ability's **fixed slot** on the bar, in cells (§11.4/#287): the widest entry
/// any ability can draw ([`MAX_BAR_ENTRY`]) plus its trailing [`BAR_GAP`].
///
/// Fixed, not fitted: an entry is drawn **left-aligned inside its slot** and the
/// slot is the same width whatever state the ability is in, so a cooldown ticking
/// from `/9/` to `/10/` — or appearing at all — never shifts the ability beside it.
/// A bar whose names slide around as numbers come and go is a bar you have to
/// *read* every time; one whose names hold still is one you learn the shape of and
/// then only glance at, which is the whole point of it being always-on (§11.4).
/// Position is muscle memory, exactly like the hotkeys it projects (§11.6).
const BAR_SLOT: u32 = MAX_BAR_ENTRY as u32 + BAR_GAP;

/// The widest the ability bar can ever be, in cells (§11.4/#287): one [`BAR_SLOT`]
/// for every ability a run can hold ([`AbilityId::MAX_HELD`]).
const MAX_BAR_WIDTH: u32 = AbilityId::MAX_HELD as u32 * BAR_SLOT;

/// **The bar must fit the board it is drawn under.** The whole point of naming every
/// entry (#287) is that the held set is small enough to; this is where that stops
/// being a hope. Every input is derived — the held cap from the innate set and the
/// tech grant (§8.3), the entry width from the ability names and the catalog's own
/// durations and cooldowns (§8.2) — so renaming an ability, pushing a cooldown past
/// 99, or granting a fourth tech fails the *build* rather than quietly truncating the
/// row on a player's screen.
const _: () = assert!(
    MAX_BAR_WIDTH <= LevelConfig::V1.width,
    "the worst-case ability bar must fit the v1 board (§10.2): shorten a bar name, \
     lower a cooldown, or lower AbilityId::MAX_TECH_HELD",
);

/// The transient **view state** a shell keeps between frames and hands to
/// [`render_screen`] (§11.4). It is deliberately *not* part of [`State`] — the
/// core stays pure game logic (§12.1), and what the player has merely chosen to
/// *look at* changes no world and costs no turn. The shell owns it, toggles it
/// from [`ui_command_for_key`](crate::input::ui_command_for_key) or a click on one
/// of the near line's controls, and passes it in.
///
/// The ability bar is **not** in here: it names every held ability on every frame
/// (§11.4/#287), so there is nothing about it left to toggle.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ScreenUi {
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
    /// The title screen / main menu, while it is up (§14/#268) — `None` once a run
    /// is playing. Like [`help_open`](Self::help_open) it is modal and full-screen:
    /// [`render_screen`] draws it *instead of* the game frame and the shell routes
    /// input to it ([`menu_nav_for_key`](crate::menu_nav_for_key) /
    /// [`menu_hit`](crate::menu_hit)). Its own screen lives in
    /// [`menu`](super::menu).
    pub menu: Option<MenuUi>,
    /// Which help tab is showing while [`help_open`](Self::help_open) (§14 v2/#248).
    /// Ignored when the panel is closed; the [`Default`] is the leftmost tab, so the
    /// panel opens on Level info. The shell cycles it from
    /// [`help_nav_for_key`](crate::help_nav_for_key) or a tab tap ([`help_hit`]).
    pub help_tab: HelpTab,
}

/// The help toggle on the near line (§14 v2/#139/#267): a `[?]` at the top-right
/// corner, three cells wide. Opens the help panel, and — since the panel is modal
/// and carries its own `[x]` — the pair is always escapable, the touch path that
/// never traps (§11.6).
const HELP_BUTTON: &str = "[?]";
const HELP_BUTTON_LEN: u32 = 3;

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
/// - **Ability bar** (row `height-1`): the always-on named readout — every held
///   ability's bar name coloured by state, its active/cooling number tucked against
///   it ([`AbilityStatus::bar_entry`]) — **right-aligned** into the bottom-right
///   corner. This is the permanent home for ability state (§11.4/§15 Q9): one row,
///   glanceable, never covering the board, and under the thumb that taps it (#267).
///
/// # Named, always, with nothing to deploy (§11.4, §15 Q9, #287)
///
/// The bar used to compress each ability to its bare hotkey and hide the names
/// behind a deploy button that unfolded a panel over the board. With the held set
/// capped at [`AbilityId::MAX_HELD`] (§8.3) the compression bought nothing worth its
/// cost: the names fit, so they are simply always there and the button and panel are
/// gone. Two experiments preceded it and both lost — showing the list only *while
/// waiting* buried the 360° guard-sense the wait exists to reveal (§9.1), and a
/// left-aligned header strip put the tap target furthest from the thumb (#267).
///
/// The bar draws the run's **real** ability state ([`State::ability_statuses`]); a
/// click on an entry resolves to the ability under it ([`ability_at`]) and activates
/// it exactly as its hotkey would. The hotkeys themselves are unchanged and
/// unaffected — the bar is a projection of them, never their source (§11.6) — and
/// the help panel's Legend card is where a player reads each key off.
pub fn render_screen(state: &State, ui: ScreenUi) -> Grid {
    let facility = state.layout().facility();
    let width = facility.width();
    let height = TOP_ROWS + facility.height() + BOTTOM_ROWS;

    // The title screen (§14/#268) comes first of all: before a run starts there is
    // nothing of the game to show, so the menu takes the whole screen — sized to the
    // board behind it, so starting a run changes what is drawn and never the fit.
    if let Some(menu) = ui.menu {
        return super::menu::render_menu(width, height, menu);
    }

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

    // The map layer, with the near line's message log hanging from the top if it is
    // deployed. Nothing else overlays the board any more: the ability bar names its
    // whole set on its own row (#287), so the board stays whole while you read it.
    let mut map = render(state);
    let messages = live_messages(state);
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
    cells.extend(ability_bar(width, &statuses));

    Grid {
        width,
        height,
        cells,
    }
}

/// Lay the ability bar out (§11.4/#267/#287): the start column of each ability's
/// **fixed slot**, in draw order, as `(status index, slot start col)`. Slots are
/// [`BAR_SLOT`] wide regardless of what the ability is doing, laid end to end and
/// **right-aligned** so the action surface hugs the bottom-right corner.
///
/// The geometry depends only on *how many* abilities the run holds — never on their
/// state — and a loadout is fixed for the whole run (§8.3), so a column here is a
/// column for the entire run. That is the property worth having: a number appearing
/// or growing by a digit moves nothing.
///
/// A legal run loadout always fits: [`MAX_BAR_WIDTH`] says so at compile time. The
/// truncation below is for the oversized hand-built states that outrun that bound
/// (a `Loadout::full` test board, a narrow board): the **deck's tail** is dropped,
/// last slot first, and what remains stays flush right. Shared by [`ability_bar`]
/// (drawing) and [`ability_at`] (hit-testing) so a click can never land on a slot
/// the row did not draw.
fn ability_line_layout(width: u32, statuses: &[AbilityStatus]) -> Vec<(usize, u32)> {
    let mut shown = statuses.len();
    while shown > 0 && shown as u32 * BAR_SLOT > width {
        shown -= 1;
    }
    let x0 = width - shown as u32 * BAR_SLOT;
    (0..shown).map(|i| (i, x0 + i as u32 * BAR_SLOT)).collect()
}

/// The always-on ability bar (§11.4/#267/#287): the frame's last row, carrying every
/// held ability by name with its state notation tucked against it
/// ([`AbilityStatus::bar_entry`]), each **left-aligned in a fixed slot**
/// ([`ability_line_layout`]) and coloured by state ([`bar_category`]), the whole
/// strip right-aligned into the bottom-right corner. Slots are a fixed width so a
/// number appearing or ticking never shifts a neighbour. No band — the bar reads as
/// a quiet HUD strip, not a message.
fn ability_bar(width: u32, statuses: &[AbilityStatus]) -> Vec<GlyphCell> {
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
            &status.bar_entry(),
            bar_category(status.state),
        );
    }
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
/// pointer→identity hit-test for the always-on bar (§11.4). A shell maps a click to
/// a screen cell and asks this; a hit fires `Input::Activate(id)` on the returned
/// ability, resolving by **identity**, never by the column it landed on (§11.6) — so
/// it opens no second activation path (the §8.4 regression) and, on a cooling/active
/// entry, refuses for free in the economy (§4.4) with no turn spent.
///
/// The geometry mirrors [`render_screen`] exactly, drawing from the same shared
/// layout ([`ability_line_layout`]) the render draws with, so a click can never miss
/// the entry that is shown — nor hit one the row truncated away.
///
/// The target is the **whole slot**, not just the glyphs in it: the slot is fixed
/// width (see [`BAR_SLOT`]), so a tap lands on the same ability whether it is
/// showing `Camo` or `Camo/20/`, and a short name is no harder to hit than a long
/// one. Only the trailing [`BAR_GAP`] is dead, keeping neighbouring targets apart.
pub fn ability_at(state: &State, x: u32, y: u32) -> Option<AbilityId> {
    let facility = state.layout().facility();
    if y != ability_row(facility.height()) {
        return None; // the bar is the frame's last row and nothing else is the bar
    }
    let statuses = state.ability_statuses();
    ability_line_layout(facility.width(), &statuses)
        .into_iter()
        .find(|(_, start)| x >= *start && x < start + MAX_BAR_ENTRY as u32)
        .map(|(i, _)| statuses[i].id)
}

/// The §11.2 category an ability entry reads in, by its state: an available ability
/// — ready, active, or a passive in effect — is **Owned** (blue, "yours, in hand");
/// a cooling one is **System** (the muted furniture tan, "unavailable, will
/// return"); an unusable one is **Ground** (dim gray, receding) — discoverable but
/// plainly not an option now. The `[N]` / `/N/` / `(on)` notation carries the rest,
/// so those three share a colour without ambiguity.
fn bar_category(state: AbilityState) -> Category {
    match state {
        AbilityState::Ready | AbilityState::Active { .. } | AbilityState::Passive => {
            Category::Owned
        }
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
    use crate::ability::{AbilityMode, Loadout};
    use crate::cell::{Cell, Direction};
    use crate::guard::Guard;
    use crate::modifiers::LevelModifiers;
    use crate::state::{Event, Input, State};
    use crate::test_support::open_room;

    /// A **legal** run loadout (§8.3/#244): innate Run plus a three-tech grant — the
    /// shape quick play resolves, and the shape the bar's width bound is sized for.
    /// A hand-built [`State::new`] boots the innate set alone, so the bar tests ask
    /// for a full grant explicitly rather than measuring a one-entry bar.
    fn granted() -> Loadout {
        Loadout::innate()
            .with(AbilityId::Camouflage)
            .with(AbilityId::Decoy)
            .with(AbilityId::Dephase)
    }

    /// The same grant with the **passive** in it (#264/#287) — Run, two activated
    /// tech, and Vision — so the bar's always-on marker is exercised beside the
    /// clocks it has to sit next to.
    fn granted_with_passive() -> Loadout {
        Loadout::innate()
            .with(AbilityId::Camouflage)
            .with(AbilityId::Decoy)
            .with(AbilityId::Vision)
    }

    /// **The near-line message width bound** (§11.7): how many cells a message has
    /// on the v1 board ([`LevelConfig::V1`], 40 wide — §10.2) before
    /// [`status_row`] clips it. Computed from the very functions that lay the row
    /// out — the words stop one cell short of the corner cluster, and the widest
    /// that cluster gets in practice is the message counter beside the help button
    /// — so the bound cannot drift from the layout it is meant to describe.
    ///
    /// It lives here rather than as a `const` because most messages are built with
    /// `format!` at runtime (an ability's name, an alert level): there is no const
    /// string to measure, so the check is a test that walks the real
    /// [`message_for`](crate::status::message_for) instead. ("the body has been
    /// reported — guards are converging" was 49 cells and reached a screenshot cut
    /// at "…reported —".)
    fn near_line_text_max() -> usize {
        let width = LevelConfig::V1.width;
        let label = message_button_label(1, false).chars().count() as u32;
        message_button_start(width, label).saturating_sub(1) as usize
    }

    /// Near-line messages that were **already** over the bound before
    /// the bound existed (§11.7). They are player-facing wording, not a bug in any
    /// one feature, so rewording them is its own change rather than a silent edit
    /// smuggled in beside an unrelated one — see the follow-up ticket. Listed
    /// explicitly so the bound still bites for every *new* message: adding one here
    /// is a deliberate act, and the list only ever shrinks.
    ///
    /// Two of these clip even with no message counter beside them ("you stow the
    /// body…" at 42, "intel in hand… (n more out)" at 45); the rest fit alone and
    /// clip only when a second message stacks the counter into the row.
    const PRE_EXISTING_OVERFLOW: [&str; 7] = [
        "all the intel — the exit is open",
        "the guard drops — a body is left",
        "the exit needs intel in hand first",
        "you slip away — the run is won",
        "you stow the body — the cupboard is sealed",
        "the facility is on alert — level 99",
        "intel in hand — the exit is open (9 more out)",
    ];

    /// §11.7: **every** message the near line can show fits the row it is shown on.
    /// `status_row` clips rather than asserting — right for a hand-built test state,
    /// but it means an over-long message fails silently in the one place it matters
    /// ("the body has been reported — guards are converging" was drawn cut at
    /// "…reported —"). Walking `message_for` covers the `format!`-built messages a
    /// const bound could not reach.
    #[test]
    fn every_near_line_message_fits() {
        let at = Cell::new(3, 3);
        // One representative of every variant. The compiler does not enumerate a
        // match's arms for us here, so this list is the thing to extend when an
        // event is added — the assertion below is what makes forgetting expensive.
        let events = [
            Event::Moved { to: at },
            Event::Bumped { into: at },
            Event::EnteredHideout { at },
            Event::EnteredDuct { at },
            Event::DuctCrawled { to: at },
            Event::Crouched { behind: at },
            Event::DoorOpened {
                at,
                by_player: true,
            },
            Event::DoorOpened {
                at,
                by_player: false,
            },
            Event::DoorClosed {
                at,
                by_player: true,
            },
            Event::DoorClosed {
                at,
                by_player: false,
            },
            Event::IntelTaken { remaining: 0 },
            Event::IntelTaken { remaining: 9 },
            Event::ExitRefused,
            Event::Won,
            Event::Captured { by: at },
            Event::TakenDown { at },
            Event::Detected { by: at },
            Event::BodyFound { at },
            Event::RadioSilence { at },
            Event::CalledIn { at },
            Event::BodyCalledIn { at },
            Event::AlertRaised { level: 99 },
            Event::BodyGrabbed { at },
            Event::BodyReleased { at },
            Event::BodyStored { at },
            Event::DecoyDied { at },
            Event::Entombed { at },
        ];
        let max = near_line_text_max();
        for event in events {
            let Some(m) = crate::status::message_for(event) else {
                continue; // a silent event says nothing to measure
            };
            let len = m.text.chars().count();
            if PRE_EXISTING_OVERFLOW.contains(&m.text.as_str()) {
                continue;
            }
            assert!(
                len <= max,
                "{:?} is {len} cells, over the {max} the near line leaves beside \
                 its controls: {:?}",
                event,
                m.text,
            );
        }

        // Every ability's activation line too — those are `format!`-built from a
        // name, so the longest name is what decides whether they fit.
        for ability in AbilityId::ALL {
            for event in [
                Event::AbilityActivated { ability },
                Event::AbilityDeactivated { ability },
                Event::AbilityExpired { ability },
            ] {
                let m = crate::status::message_for(event).expect("an ability speaks");
                assert!(
                    m.text.chars().count() <= max,
                    "{:?} does not fit the near line",
                    m.text,
                );
            }
        }
    }

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

    /// The §11.4 golden test, whole screen (#267/#287): the near and usable lines on
    /// top, then the map, then the always-on ability bar — one grid, printed as
    /// text. The near line rests on ambient floor and carries the `[?]` toggle in
    /// the top-right corner; the usable line offers the adjacent console; the bar
    /// **names** every held ability, flush to the bottom-right with its one-cell
    /// margin. Nothing covers the board — there is no panel left to deploy.
    #[test]
    fn the_full_screen_renders_golden() {
        let s = State::new(
            open_room(40, 6),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            [Cell::new(3, 2)], // a console east of the player
            Cell::new(38, 4),
        )
        .with_loadout(granted());
        let text = render_screen(&s, ScreenUi::default()).to_text();
        assert_eq!(
            text,
            vec![
                " intel remaining: 1                 [?] ".to_string(),
                " → console: take intel                  ".to_string(),
                "########################################".to_string(),
                "#······································#".to_string(),
                "#·@$···································#".to_string(),
                "#······································#".to_string(),
                "#·····································E#".to_string(),
                "########################################".to_string(),
                "Run       Camo      Decoy     Phase     ".to_string(),
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

    /// The permanent home of ability state (§11.4/#267/#287): the **always-on
    /// ability bar** on the frame's last row, assembled from the run's real economy
    /// ([`State::ability_statuses`]). A fresh run has every held ability ready, so the
    /// bar is their **names** in deck order, each in Owned, **right-aligned** into the
    /// bottom-right corner behind a one-cell margin. The two bump verbs (Takedown,
    /// Drag) are **not** on it: they live on the usable line, not the ability economy
    /// (§7.2/§8.3).
    #[test]
    fn the_always_on_bar_names_every_held_ability() {
        let s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(granted());
        let g = render_screen(&s, ScreenUi::default());
        let bar = ability_row(10);
        assert_eq!(bar + BOTTOM_ROWS, g.height(), "the bar is the last row");

        // Four ready abilities in four ten-cell slots, filling the 40-wide row.
        for (col, name) in [(0, "Run"), (10, "Camo"), (20, "Decoy"), (30, "Phase")] {
            let drawn: String = (col..col + name.len() as u32)
                .map(|x| g.get(x, bar).glyph)
                .collect();
            assert_eq!(drawn, name, "{name} at col {col}");
            assert_eq!(g.get(col, bar).fg, Category::Owned, "{name} ready colour");
        }
        // Each name left-aligned in its slot, the strip flush right, one cell of air
        // after the last: the four slots exactly fill the v1 row.
        let row: String = (0..g.width()).map(|x| g.get(x, bar).glyph).collect();
        assert_eq!(row, "Run       Camo      Decoy     Phase     ", "{row:?}");
        // The bump verbs never appear on the ability bar.
        assert!(
            !row.contains("Takedown"),
            "Takedown is not an economy ability"
        );
        assert!(!row.contains("Drag"), "Drag is not an economy ability");
        // Nor does an ability the run was not granted (#244).
        assert!(!row.contains("Doors"), "Autodoors was not in the loadout");
    }

    /// The bar's live states (§11.4): an **active** ability tucks its `[n]` against
    /// its name in Owned, a **cooling** one its `/n/` in System — the exact numbers
    /// the economy hands over (§8.2). Driven to Run cooling and Camouflage active,
    /// with Decoy and Dephase still ready, so all three notations show at once.
    #[test]
    fn the_bar_shows_active_and_cooling_state() {
        let mut s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(granted());
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
        // `Run/11/` cooling (System) and `Camo[9]` active (Owned) grew into their
        // slots — and **every name is still in the column it started in**.
        assert_eq!(row, "Run/11/   Camo[9]   Decoy     Phase     ", "{row:?}");
        assert_eq!(g.get(0, bar).fg, Category::System, "cooling reads System");
        assert_eq!(g.get(3, bar).glyph, '/', "cooling shows /N/");
        assert_eq!(g.get(10, bar).glyph, 'C');
        assert_eq!(g.get(10, bar).fg, Category::Owned, "active reads Owned");
        assert_eq!(g.get(14, bar).glyph, '[', "active shows [N]");
    }

    /// **Nothing moves** (§11.4/#287): the fixed slots mean an ability's column is a
    /// fact about the run, not about the frame. Drive the deck through activation,
    /// an early toggle-off, a two-digit cooldown draining to one digit, and back to
    /// ready — and every ability starts on the same cell it started the run on. A
    /// bar whose names slide as numbers come and go is one you re-read every glance.
    #[test]
    fn a_ticking_number_never_shifts_a_neighbour() {
        let mut s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(granted());
        let bar = ability_row(10);
        // Where each name sits on the very first frame, before anything is used.
        let columns = |s: &State| -> Vec<u32> {
            ability_line_layout(40, &s.ability_statuses())
                .into_iter()
                .map(|(_, start)| start)
                .collect()
        };
        let first = columns(&s);
        assert_eq!(first, vec![0, 10, 20, 30]);

        // Run: on, off, and then the whole 12-turn cooldown drained — `/12/` down
        // through `/9/` to nothing, the digit count changing on the way.
        s.step(Input::Activate(AbilityId::Run));
        s.step(Input::Deactivate(AbilityId::Run));
        for _ in 0..14 {
            assert_eq!(columns(&s), first, "the columns held at turn {}", s.turn());
            let name: String = (0..3)
                .map(|x| {
                    render_screen(&s, ScreenUi::default())
                        .get(30 + x, bar)
                        .glyph
                })
                .collect();
            assert_eq!(name, "Pha", "…and the far slot is still Phase");
            s.step(Input::Wait);
        }
        assert_eq!(
            s.ability_state(AbilityId::Run),
            AbilityState::Ready,
            "the cooldown really did run out under the test",
        );
        assert_eq!(columns(&s), first, "and back to ready moved nothing");
    }

    /// The bar is a **projection**, not a rebinding (§11.6/#267/#287): dropping the
    /// hotkey letter off the bar moved no key — every entry still resolves to the
    /// ability its settled hotkey fires — and each ability state still reads its own
    /// colour, ready and active Owned, cooling System, so the states stay
    /// discoverable without the letter.
    #[test]
    fn the_bar_still_projects_the_settled_hotkeys_and_states() {
        use crate::input::ability_input_for_key;

        let mut s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(granted());
        let bar = ability_row(10);

        // Every entry the bar draws resolves to the very id its hotkey fires.
        for (i, start) in ability_line_layout(40, &s.ability_statuses()) {
            let id = s.ability_statuses()[i].id;
            let key = crate::input::ability_hotkey(id.name()).expect("a settled hotkey");
            assert_eq!(
                ability_at(&s, start, bar),
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
        // ready.
        s.step(Input::Activate(AbilityId::Run));
        s.step(Input::Deactivate(AbilityId::Run));
        s.step(Input::Activate(AbilityId::Camouflage));
        let g = render_screen(&s, ScreenUi::default());
        let entry = |id: AbilityId| {
            let statuses = s.ability_statuses();
            let i = statuses.iter().position(|st| st.id == id).expect("in deck");
            ability_line_layout(40, &statuses)
                .into_iter()
                .find(|(j, _)| *j == i)
                .expect("drawn")
                .1
        };
        assert_eq!(g.get(entry(AbilityId::Run), bar).fg, Category::System);
        assert_eq!(g.get(entry(AbilityId::Camouflage), bar).fg, Category::Owned);
        assert_eq!(g.get(entry(AbilityId::Decoy), bar).fg, Category::Owned);
    }

    /// A **passive** on the bar (#264/#287): it reads `Sight(on)` — named like every
    /// other entry, marked always-on where an activated ability carries its clock,
    /// and in the Owned colour because it is in effect. Undecorated it would have
    /// looked exactly like the ready abilities beside it, which is the one thing it
    /// is not: there is nothing to press.
    #[test]
    fn a_held_passive_reads_as_always_on() {
        let s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(granted_with_passive());
        let g = render_screen(&s, ScreenUi::default());
        let bar = ability_row(10);
        let row: String = (0..g.width()).map(|x| g.get(x, bar).glyph).collect();
        assert_eq!(row, "Run       Camo      Decoy     Sight(on) ", "{row:?}");

        // In effect, so Owned — the same colour as the ready entries it sits beside,
        // with the marker rather than the colour carrying "you cannot press this".
        let sight = row.find("Sight").expect("the passive's entry") as u32;
        assert_eq!(g.get(sight, bar).fg, Category::Owned);
        assert_eq!(
            s.ability_state(AbilityId::Vision),
            AbilityState::Passive,
            "held is on (§8.2/#264)",
        );
        // And it still hit-tests to itself, marker included.
        for x in sight..sight + "Sight(on)".len() as u32 {
            assert_eq!(ability_at(&s, x, bar), Some(AbilityId::Vision), "col {x}");
        }
    }

    /// **The width budget, end to end** (§11.4/#287). The worst bar a run can ever
    /// produce — [`AbilityId::MAX_HELD`] abilities, each the widest entry the catalog
    /// allows — is drawn whole on the v1 board: nothing truncated, the right margin
    /// intact, and not a cell past the frame's left edge. This is the runtime twin of
    /// the `const` assertion on [`MAX_BAR_WIDTH`]; if either ever fails, the other is
    /// what tells you why.
    #[test]
    fn the_widest_possible_bar_fits_the_v1_board() {
        let width = LevelConfig::V1.width;
        assert_eq!(BAR_SLOT, 10, "nine cells of entry, one of air");
        assert_eq!(MAX_BAR_WIDTH, 40, "four slots of ten");
        assert!(
            MAX_BAR_WIDTH <= width,
            "and the board is at least that wide"
        );

        // Every ability in the widest state its own mode can reach — the longest
        // cooling number, or the passive marker — and the worst `MAX_HELD` kept.
        let mut worst: Vec<AbilityStatus> = AbilityId::ALL
            .into_iter()
            .map(|id| AbilityStatus {
                id,
                state: match id.def().mode() {
                    AbilityMode::Passive => AbilityState::Passive,
                    AbilityMode::Activated(economy) => AbilityState::Cooling {
                        remaining: economy.cooldown(),
                    },
                },
            })
            .collect();
        worst.sort_by_key(|s| std::cmp::Reverse(s.bar_entry().chars().count()));
        worst.truncate(AbilityId::MAX_HELD);

        let layout = ability_line_layout(width, &worst);
        assert_eq!(layout.len(), AbilityId::MAX_HELD, "no entry is dropped");
        // Even at their widest, no entry overruns its slot — and the last one still
        // leaves the trailing cell of air at the frame's edge.
        for (i, start) in &layout {
            let len = worst[*i].bar_entry().chars().count() as u32;
            assert!(len <= MAX_BAR_ENTRY as u32, "{:?} fits its slot", worst[*i]);
            assert!(start + len <= width - BAR_GAP, "…inside the row");
        }
        assert_eq!(layout[0].1, 0, "and the four slots fill the row exactly");
    }

    /// A bar wider than its row **truncates** rather than panicking or wrapping. No
    /// legal loadout gets here — [`MAX_BAR_WIDTH`] is asserted against the board at
    /// compile time — but a hand-built [`Loadout::full`] state or a narrow test board
    /// can, and the deck's last slots are what go.
    #[test]
    fn an_oversized_bar_drops_its_tail_and_stays_flush_right() {
        let s = State::new(
            open_room(24, 4),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(22, 2),
        )
        .with_loadout(Loadout::full());
        assert_eq!(
            s.ability_statuses().len(),
            AbilityId::ALL.len(),
            "every ability at once — well over the cap",
        );
        let g = render_screen(&s, ScreenUi::default());
        assert_eq!(
            g.height(),
            TOP_ROWS + 4 + BOTTOM_ROWS,
            "the frame is intact"
        );
        let row: String = (0..g.width())
            .map(|x| g.get(x, ability_row(4)).glyph)
            .collect();
        assert_eq!(row.chars().count(), 24, "exactly one grid row wide");
        // Seven slots need 70 cells and 24 are on offer, so the deck's last five go
        // — and the two that remain keep their full slots, flush right.
        assert_eq!(row, "    Run       Camo      ", "{row:?}");
    }

    /// The pointer→identity hit-test (§11.4) on the always-on bar: each entry's cells
    /// resolve to *that* ability by identity, the gaps and the empty left of the row
    /// resolve to nothing, and the map above is not the bar.
    #[test]
    fn ability_at_resolves_the_bar() {
        let s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(granted());
        let bar = ability_row(10);

        // Four fixed slots at 0/10/20/30, by identity not position — and the target
        // is the **whole slot**, so the blank cells after a short name hit it too.
        // That is what makes a slot a stable tap target rather than a moving word.
        for (slot, id) in [
            (0, AbilityId::Run),
            (10, AbilityId::Camouflage),
            (20, AbilityId::Decoy),
            (30, AbilityId::Dephase),
        ] {
            for x in slot..slot + MAX_BAR_ENTRY as u32 {
                assert_eq!(ability_at(&s, x, bar), Some(id), "col {x}");
            }
            // …but the trailing cell of air is dead, keeping the targets apart.
            assert_eq!(
                ability_at(&s, slot + MAX_BAR_ENTRY as u32, bar),
                None,
                "the gap after slot {slot} resolves to nothing",
            );
        }
        // The map above the bar is not the bar.
        assert_eq!(ability_at(&s, 0, bar - 1), None, "the row above is map");
        assert_eq!(ability_at(&s, 0, NEAR_ROW), None, "nor is the near line");
    }

    /// The click **is** the hotkey (§11.4/§11.6): the id a bar cell resolves to is
    /// the very id its §11.6 shortcut fires, and firing it drives the one
    /// `Input::Activate` path — so a click activates a ready ability and, on a
    /// cooling one, refuses for free with no turn spent (§4.4), exactly as the key.
    #[test]
    fn a_click_activates_by_the_same_path_as_the_hotkey() {
        use crate::input::ability_input_for_key;

        let mut s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(granted());
        let bar = ability_row(10);

        // The bar's Run slot resolves to the same id `r` fires — one path, by identity.
        let clicked = ability_at(&s, 0, bar).expect("Run under the pointer");
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
        // The entry widened to `Run/12/` inside its slot — which did not move, so
        // the very same cell is still Run (#287).
        let cooling = ability_at(&s, 0, bar).expect("Run still under the pointer");
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

    /// The `[?]` toggle is the near line's alone (§11.4/#139/#267): it hit-tests on
    /// the top row and nowhere else, so the bar's own right-hand corner — the same
    /// columns, the frame's last row — can never swallow a tap meant for the board's
    /// bottom-right, nor the other way round.
    #[test]
    fn the_help_toggle_belongs_to_the_near_line_only() {
        let (width, height) = (40, 43); // TOP_ROWS + 40 + BOTTOM_ROWS
        let bar = height - BOTTOM_ROWS;
        for x in help_button_start(width)..help_button_start(width) + HELP_BUTTON_LEN {
            assert!(is_help_button(width, x, NEAR_ROW));
            assert!(
                !is_help_button(width, x, bar),
                "not on the bar's row at {x}"
            );
        }
    }

    /// #268: the title screen takes the **whole** frame and takes it *first* — the
    /// game's chrome does not show through, not even the help panel a stale
    /// `help_open` would otherwise draw. It is the board's own size, so starting a
    /// run swaps what is drawn without moving the fit, and it writes no state:
    /// clearing it restores the identical frame.
    #[test]
    fn the_menu_replaces_the_whole_frame_and_leaves_it_untouched() {
        let s = help_board();
        let playing = render_screen(&s, ScreenUi::default());
        let menu = render_screen(
            &s,
            ScreenUi {
                menu: Some(MenuUi::default()),
                // Set alongside every other overlay: the menu still wins outright.
                help_open: true,
                message_log_open: true,
                ..ScreenUi::default()
            },
        );
        assert_ne!(menu, playing);
        assert_eq!(
            (menu.width(), menu.height()),
            (playing.width(), playing.height())
        );
        assert!(
            menu.to_text()
                .join("\n")
                .contains(MenuEntry::QuickPlay.label()),
            "the frame is the menu, not the help panel behind it",
        );
        assert_eq!(
            render_screen(&s, ScreenUi::default()),
            playing,
            "leaving the menu restores the identical frame",
        );
    }
}
