//! On-screen HUD composition for the character grid (§11.4, §11.7).
//!
//! The world render — terrain, fog, entities, the danger overlay — lives in the
//! parent [`render`](super) module as a pure state→[`Grid`](super::Grid). This
//! module owns the *chrome* laid over and around it: the always-on ability line
//! and its deployable panel, the near line and its message log, the status rows,
//! and the click hit-tests that pair with them. [`render_screen`] composes the
//! two — it draws the world grid, then overlays this. Both halves stay a pure
//! function of state (§11.1/§12.1) and golden-grid testable.

use super::*;
use crate::ability::{AbilityId, AbilityState, AbilityStatus};
use crate::cell::Direction;
use crate::status::{live_messages, near_line, Message};

/// The rows the screen adds beneath the map (§11.4): the near line and the
/// usable line. A shell fitting the screen sizes for `HEADER_ROWS + facility
/// height + this`.
pub const STATUS_ROWS: u32 = 2;

/// The row the screen adds **above** the map (§11.4): the always-on ability line.
/// A shell fits for `this + facility height + STATUS_ROWS`.
pub const HEADER_ROWS: u32 = 1;

/// The transient **view state** a shell keeps between frames and hands to
/// [`render_screen`] (§11.4). It is deliberately *not* part of [`State`] — the
/// core stays pure game logic (§12.1), and what the player has merely chosen to
/// *look at* changes no world and costs no turn. The shell owns it, toggles it
/// from [`ui_command_for_key`](crate::input::ui_command_for_key) or a click on the
/// deploy button ([`is_ability_button`]), and passes it in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ScreenUi {
    /// Whether the full ability panel is deployed (§11.4). The compact ability
    /// line is always drawn; this gates only the expanded, named panel.
    pub ability_panel_open: bool,
    /// Whether the near line's full message list is deployed (§11.7). The near
    /// line always speaks the loudest live message; when more than one is live it
    /// also shows a counter, and this gates the expanded list of them all. The
    /// list is always the current step's live set — deployed or not, it clears on
    /// the next action (§11.7), never a scrollback.
    pub message_log_open: bool,
}

/// The deploy button's label on the ability line (§11.4): a downward chevron when
/// the panel is closed (bump it *open*), an upward one when it is open. Both are
/// three cells wide, so the button's footprint is fixed regardless of state.
const BUTTON_CLOSED: &str = "[▾]";
const BUTTON_OPEN: &str = "[▴]";
const BUTTON_LEN: u32 = 3;

/// The column the deploy button starts at on a screen `width` wide: right-aligned
/// with a one-cell margin. Shared by the drawing ([`ability_line`]) and the
/// hit-test ([`is_ability_button`]) so the button a click lands on is exactly the
/// button drawn.
fn button_start(width: u32) -> u32 {
    width.saturating_sub(1 + BUTTON_LEN)
}

/// Whether screen cell `(x, y)` is the deploy button (§11.4) — the header row's
/// right-aligned toggle. A shell maps a click to a screen cell and asks this; a
/// hit flips [`ScreenUi::ability_panel_open`] instead of stepping. It is the one
/// piece of the button's geometry the shell needs, kept here beside the drawing so
/// the two can never disagree.
pub fn is_ability_button(width: u32, x: u32, y: u32) -> bool {
    let start = button_start(width);
    y == 0 && x >= start && x < start + BUTTON_LEN
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
/// right-aligned with a one-cell margin, like the ability line's deploy button.
fn message_button_start(width: u32, label_len: u32) -> u32 {
    width.saturating_sub(1 + label_len)
}

/// Whether screen cell `(x, y)` is the near line's message-log toggle (§11.7) —
/// the right-aligned counter that deploys and folds the full live-message list. A
/// shell maps a click to a screen cell and asks this; a hit flips
/// [`ScreenUi::message_log_open`] instead of stepping. There is no button unless
/// **more than one** message is live, so a lone or absent message yields `false`.
/// The geometry is read from `state` — the count sets the label width, and the
/// near line is the first status row (`HEADER_ROWS + map height`) — so a click can
/// never miss the toggle the frame drew.
pub fn is_message_button(state: &State, x: u32, y: u32) -> bool {
    let extra = live_messages(state).len().saturating_sub(1);
    if extra == 0 {
        return false;
    }
    let facility = state.layout().facility();
    let (width, map_h) = (facility.width(), facility.height());
    let label_len = message_button_label(extra, false).chars().count() as u32;
    let start = message_button_start(width, label_len);
    y == HEADER_ROWS + map_h && x >= start && x < start + label_len
}

/// Render the full §11.4 **screen**: the always-on ability line, the map
/// ([`render`]), and the two status lines beneath it — `HEADER_ROWS + map height +
/// STATUS_ROWS` rows, same width, one [`Grid`], so a whole frame is still a pure
/// function of `(state, ui)` that prints as text (§11.1) and golden-testable to
/// the last row.
///
/// - **Ability line** (row `0`): the always-on compact readout — every ability's
///   hotkey coloured by state, its active/cooling number inline
///   ([`AbilityStatus::compact`]) — plus the right-aligned **deploy button**
///   ([`is_ability_button`]). This is the permanent home for ability state
///   (§11.4): one row, glanceable, never covering the board.
/// - **Near line** (row `height-2`): the highest-priority message of the last
///   action, or the ambient floor ([`near_line`], §11.7) — a solid band in the
///   message's category with the words in Neutral on top.
/// - **Usable line** (row `height-1`): the adjacent bump affordances
///   ([`State::affordances`]), each in its own category, no band.
///
/// # The deployable ability panel (§11.4, §15 Q9)
///
/// When the shell has the panel **deployed** (`ui.ability_panel_open`, driven by
/// the deploy button or the `Tab` toggle), the full named panel — each ability's
/// `<key> <Name> <state>` ([`AbilityStatus::label`]) — is overlaid on the map, in
/// the **corner opposite the player** ([`panel_origin_opposite_player`]) so it
/// never covers where the action is. It is not tied to waiting: an earlier
/// experiment showed it on the wait turn, which buried exactly the 360° guard-sense
/// the wait exists to reveal (§9.1) — so the panel is now on demand, and waiting
/// stays a clear look around. Both the line and the panel draw the run's **real**
/// ability state ([`State::ability_statuses`]); a click on either resolves to the
/// ability under it ([`ability_at`]) and activates it exactly as its hotkey would.
pub fn render_screen(state: &State, ui: ScreenUi) -> Grid {
    let statuses = state.ability_statuses();

    // The map layer, with any deployed overlays: the ability panel opposite the
    // player, and the near line's message log rising from the bottom.
    let mut map = render(state);
    if ui.ability_panel_open {
        let origin =
            panel_origin_opposite_player(map.width(), map.height(), state.player(), &statuses);
        overlay_ability_panel(&mut map, origin, &statuses);
    }
    // The step's live messages (§11.7), loudest first: the near line speaks the
    // first, counts the rest, and deploys the whole list over the board here. The
    // list only earns the board when more than one message is live.
    let messages = live_messages(state);
    if ui.message_log_open && messages.len() > 1 {
        overlay_message_log(&mut map, &messages);
    }
    let width = map.width();
    let height = HEADER_ROWS + map.height() + STATUS_ROWS;

    // One grid, top to bottom: the ability line, the map, the two status lines.
    let mut cells = ability_line(width, &statuses, ui.ability_panel_open);
    cells.extend(map.cells);

    // The near line (§11.4/§11.7): the loudest live message as a category band —
    // or the ambient floor when nothing is live — plus, when more than one message
    // is live, a right-aligned counter toggling the deployed list.
    let top = messages
        .first()
        .cloned()
        .unwrap_or_else(|| near_line(state));
    let extra = messages.len().saturating_sub(1);
    let mut near = status_row(width, &[(top.text, Category::Neutral)], Some(top.category));
    if extra > 0 {
        draw_message_button(&mut near, width, top.category, extra, ui.message_log_open);
    }
    cells.extend(near);
    let usable: Vec<(String, Category)> = state
        .affordances()
        .into_iter()
        .map(|(dir, a)| (format!("{} {}", arrow(dir), a.label()), a.category()))
        .collect();
    cells.extend(status_row(width, &usable, None));

    Grid {
        width,
        height,
        cells,
    }
}

/// Lay the compact ability line out (§11.4): the start column of each entry that
/// fits before the deploy button, in draw order, as `(status index, start col)`.
/// Entries begin at a one-cell margin with a single space between; the strip stops
/// the moment the next entry would run into the button ([`button_start`]). Shared
/// by [`ability_line`] (drawing) and [`ability_at`] (hit-testing) so a click can
/// never land on an entry the row did not draw.
fn ability_line_layout(width: u32, statuses: &[AbilityStatus]) -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    let mut x = 1; // the one-cell left margin
    for (i, status) in statuses.iter().enumerate() {
        let len = status.compact().chars().count() as u32;
        if x + len > button_start(width) {
            break;
        }
        out.push((i, x));
        x += len + 1; // one space between abilities
    }
    out
}

/// The always-on ability line (§11.4): one row carrying every ability's compact
/// readout ([`AbilityStatus::compact`]) from a one-cell left margin, each in its
/// state colour ([`panel_category`]), with the deploy button
/// ([`is_ability_button`]) right-aligned. Single spaces between abilities keep the
/// whole set on one row; the button's chevron points down when closed, up when
/// open. No band — the line reads as a quiet HUD strip, not a message.
fn ability_line(width: u32, statuses: &[AbilityStatus], open: bool) -> Vec<GlyphCell> {
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

/// The ability entry at screen cell `(x, y)`, or `None` — the **pure**
/// pointer→identity hit-test for both the always-on line and the deployed panel
/// (§11.4), the sibling of [`is_ability_button`]. A shell maps a click to a screen
/// cell and asks this; a hit fires `Input::Activate(id)` on the returned ability,
/// resolving by **identity**, never by the row it landed on (§11.6) — so it opens
/// no second activation path (the §8.4 regression) and, on a cooling/active entry,
/// refuses for free in the economy (§4.4) with no turn spent.
///
/// The geometry mirrors [`render_screen`] exactly, drawing from the same shared
/// layout ([`ability_line_layout`]) and panel origin ([`panel_origin_opposite_player`])
/// the render draws with, so a click can never miss the entry that is shown. Row 0
/// is the compact line; when the panel is deployed, its rows are hit-tested on the
/// map layer beneath the header. The deploy button is never an ability — the line
/// stops before it and the shell tests the button first — so a tap there toggles the
/// panel and never falls through to an activation underneath.
pub fn ability_at(state: &State, ui: ScreenUi, x: u32, y: u32) -> Option<AbilityId> {
    let statuses = state.ability_statuses();
    let facility = state.layout().facility();
    let (map_w, map_h) = (facility.width(), facility.height());

    // Row 0: the always-on compact line.
    if y == 0 {
        for (i, start) in ability_line_layout(map_w, &statuses) {
            let len = statuses[i].compact().chars().count() as u32;
            if x >= start && x < start + len {
                return Some(statuses[i].id);
            }
        }
        return None;
    }

    // The deployed panel, overlaid on the map layer below the header (§11.4).
    if ui.ability_panel_open && y >= HEADER_ROWS {
        let (mx, my) = (x, y - HEADER_ROWS);
        let (ox, oy) = panel_origin_opposite_player(map_w, map_h, state.player(), &statuses);
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
/// by the origin ([`panel_origin_opposite_player`]), the overlay
/// ([`overlay_ability_panel`]) and the hit-test ([`ability_at`]) so all three agree
/// on the block's footprint.
fn panel_band_width(statuses: &[AbilityStatus]) -> u32 {
    statuses
        .iter()
        .map(|s| s.label().chars().count())
        .max()
        .unwrap_or(0) as u32
        + 1
}

/// The map-space corner to anchor the deployed panel at, **opposite the player**
/// (§11.4): a player in the left half puts the panel on the right, a player in the
/// top half puts it at the bottom, and so on — so the panel is always as far from
/// the player as the board allows and never covers where they are acting. A
/// one-cell inset keeps a border of map around it; sizes are clamped so a tiny
/// hand-built board never underflows (the v1 board is 40×40, §10.2). Takes the map
/// dimensions rather than the [`Grid`] so the hit-test can reuse it without a
/// rendered frame.
fn panel_origin_opposite_player(
    map_w: u32,
    map_h: u32,
    player: Cell,
    statuses: &[AbilityStatus],
) -> (u32, u32) {
    let panel_w = panel_band_width(statuses);
    let panel_h = statuses.len() as u32;

    // Player left of centre → panel right; player above centre → panel bottom.
    let x0 = if player.x < map_w / 2 {
        map_w.saturating_sub(panel_w + 1)
    } else {
        1
    };
    let y0 = if player.y < map_h / 2 {
        map_h.saturating_sub(panel_h + 1)
    } else {
        1
    };
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
/// margin, two spaces between segments, truncated at the edge; `band` paints
/// every cell's background (the §11.4 message band) or none.
fn status_row(
    width: u32,
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
    let mut x = 1; // the one-cell left margin
    for (i, (text, category)) in segments.iter().enumerate() {
        if i > 0 {
            x += 2;
        }
        for glyph in text.chars() {
            if x >= cells.len() {
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

/// Overlay the deployed message log onto the map `grid` (§11.7): the step's live
/// messages ([`live_messages`]), one per row, **rising from the near line** — at
/// the map's bottom-left, the loudest on the last row directly above its own
/// near-line band, each quieter message one row higher. Every row is cleared to a
/// uniform band — a one-cell margin, the longest message, a cell of pad — then the
/// words drawn in the message's own §11.2 category, so the list reads as a solid
/// block over the board and each entry keeps its threat colour, aligned with the
/// band beneath.
///
/// Bounds are clamped, never asserted: on a board too short to hold every row
/// (only hand-built test states get that small — the v1 board is 40×40, §10.2)
/// the block shows as many as fit from the bottom and drops the rest.
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
    for (i, message) in messages.iter().enumerate() {
        let i = i as u32;
        if i >= map_h {
            break; // out the top of a tiny board — show what fits from the bottom
        }
        let y = map_h - 1 - i;
        for dx in 0..band {
            grid.cells[(y * width + dx) as usize] = blank;
        }
        // A one-cell left margin, matching the near line, so the list lines up
        // above the band it heads.
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
        let near_row = HEADER_ROWS + s.layout().facility().height(); // first status row
        let row_text = |g: &Grid, y: u32| (0..width).map(|x| g.get(x, y).glyph).collect::<String>();

        // Collapsed: the band speaks the loudest message and the closed counter of
        // the one further message (a down chevron) sits at the right.
        let g = render_screen(&s, ScreenUi::default());
        let near = row_text(&g, near_row);
        assert!(
            near.contains("a body has been found"),
            "the band speaks the loudest message: {near:?}"
        );
        assert!(
            near.contains("[+1 ▾]"),
            "a closed counter of the rest: {near:?}"
        );

        // The hit-test agrees with the drawn counter, and there is no button off it.
        let label_len = "[+1 ▾]".chars().count() as u32;
        let start = width - 1 - label_len;
        assert!(
            is_message_button(&s, start, near_row),
            "the counter is hittable"
        );
        assert!(
            !is_message_button(&s, start - 1, near_row),
            "nothing just left of it"
        );

        // Deployed: the chevron flips up and the whole list stacks over the board —
        // the loudest directly above the near line, the quieter one above that.
        let ui = ScreenUi {
            message_log_open: true,
            ..ScreenUi::default()
        };
        let g = render_screen(&s, ui);
        assert!(
            row_text(&g, near_row).contains("[+1 ▴]"),
            "the deployed counter points up"
        );
        assert!(
            row_text(&g, near_row - 1).contains("a body has been found"),
            "the loudest sits nearest the band"
        );
        assert!(
            row_text(&g, near_row - 2).contains("the guard drops — a body is left"),
            "the rest stack above it"
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
        let near_row = HEADER_ROWS + s.layout().facility().height();
        let near: String = (0..width)
            .map(|x| {
                render_screen(&s, ScreenUi::default())
                    .get(x, near_row)
                    .glyph
            })
            .collect();
        assert!(
            !near.contains('['),
            "no counter for a lone message: {near:?}"
        );
        assert!(
            (0..width).all(|x| !is_message_button(&s, x, near_row)),
            "and nothing to click"
        );
    }

    /// The §11.4 golden test, whole screen: the always-on ability line on top, the
    /// map, then the near and usable lines — one grid, printed as text. The header
    /// carries the compact ability readout and the closed deploy button; with the
    /// panel not deployed the map is untouched, the near line rests on ambient
    /// floor, and the usable line offers the adjacent console.
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
        // Row 0 is the always-on ability line: on a fresh run every economy ability
        // is ready, so the compact keys are the four bare hotkeys, deploy button
        // (closed chevron) right. (Its exact glyphs are pinned in the ability-line
        // test; here we assert its shape without pasting the chevron.)
        assert!(
            text[0].starts_with(" r c d x"),
            "the always-on ability line: {:?}",
            text[0]
        );
        assert!(
            text[0].trim_end().ends_with("[▾]"),
            "the closed deploy button: {:?}",
            text[0]
        );
        // Below it, the map and the two status lines — the panel is not deployed,
        // so the board is whole.
        assert_eq!(
            text[1..].to_vec(),
            vec![
                "########################".to_string(),
                "#······················#".to_string(),
                "#·@$···················#".to_string(),
                "#······················#".to_string(),
                "#·····················E#".to_string(),
                "########################".to_string(),
                " intel remaining: 1     ".to_string(),
                " → console: take intel  ".to_string(),
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
        assert_eq!(g.height(), HEADER_ROWS + map.height() + STATUS_ROWS);

        let near_y = HEADER_ROWS + map.height();
        let usable_y = near_y + 1;
        for x in 0..g.width() {
            let cell = g.get(x, near_y);
            assert_eq!(cell.bg, Some(Category::Interest), "the band spans the row");
            assert_eq!(cell.vis, Visibility::Live);
            if cell.glyph != ' ' {
                assert_eq!(cell.fg, Category::Neutral, "words read Neutral on the band");
            }
            assert_eq!(g.get(x, usable_y).bg, None, "the usable line has no band");
        }
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
            Cell::new(2, 2),
            Direction::North,
            vec![Guard::patrolling_to(Cell::new(2, 4), Cell::new(2, 1))],
            Vec::new(),
            Cell::new(22, 4),
        );
        s.step(Input::Wait); // the guard steps north into the player: caught
        let g = render_screen(&s, ScreenUi::default());
        assert_eq!(g.get(0, near_y).bg, Some(Category::Danger));
        assert_eq!(g.get(1, near_y).glyph, 'c'); // "caught"
    }

    /// The permanent home of ability state (§11.4): the **always-on ability line**
    /// on row 0, assembled from the run's real economy ([`State::ability_statuses`]).
    /// A fresh run has every ability ready, so the line is the four economy keys in
    /// deck order, each the bare §11.6 hotkey in Owned — and the two bump verbs
    /// (Takedown `t`, Drag `g`) are **not** on it: they live on the usable line, not
    /// the ability economy (§7.2/§8.3).
    #[test]
    fn the_always_on_line_shows_every_economy_ability() {
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

        // The four economy abilities laid left to right from the one-cell margin,
        // each ready → the bare key in Owned, each key its settled §11.6 hotkey.
        for (col, name, glyph) in [
            (1, "Run", 'r'),
            (3, "Camouflage", 'c'),
            (5, "Decoy", 'd'),
            (7, "Dephase", 'x'),
        ] {
            assert_eq!(g.get(col, 0).glyph, glyph, "{name} at col {col}");
            assert_eq!(g.get(col, 0).fg, Category::Owned, "{name} ready colour");
            assert_eq!(Some(glyph), ability_hotkey(name), "{name} hotkey");
        }
        // The bump verbs never appear on the ability line.
        let row0: String = (0..g.width()).map(|x| g.get(x, 0).glyph).collect();
        assert!(!row0.contains('t'), "Takedown is not an economy ability");
        assert!(!row0.contains('g'), "Drag is not an economy ability");

        // The deploy button, closed, right-aligned — and `is_ability_button` agrees
        // with where it is drawn.
        let start = 30 - 1 - 3;
        assert!(is_ability_button(30, start, 0));
        assert!(
            !is_ability_button(30, start - 1, 0),
            "just left is not the button"
        );
        assert!(
            !is_ability_button(30, start, 1),
            "row 1 is the map, not the button"
        );
        assert_eq!(g.get(start, 0).glyph, '[');
        assert_eq!(g.get(start, 0).fg, Category::System);
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
        let row0: String = (0..g.width()).map(|x| g.get(x, 0).glyph).collect();
        // `r/11/` cooling (System), `c[9]` active (Owned), then the two ready keys.
        assert!(
            row0.starts_with(" r/11/ c[9] d x"),
            "the live ability line: {row0:?}"
        );
        assert_eq!(g.get(1, 0).glyph, 'r');
        assert_eq!(g.get(1, 0).fg, Category::System, "cooling reads System");
        assert_eq!(g.get(2, 0).glyph, '/', "cooling shows /N/");
        assert_eq!(g.get(7, 0).glyph, 'c');
        assert_eq!(g.get(7, 0).fg, Category::Owned, "active reads Owned");
        assert_eq!(g.get(8, 0).glyph, '[', "active shows [N]");
    }

    /// Deploying the panel (§11.4) overlays the named ability list in the corner
    /// **opposite the player**, so it never covers where the action is — and it is
    /// gone the moment the panel is not deployed. The corner tracks the player: a
    /// player top-left puts the panel bottom-right, and moving to the bottom-right
    /// flips it top-left.
    #[test]
    fn deploying_shows_the_panel_opposite_the_player() {
        // Player top-left → panel bottom-right. On a fresh run the widest label is
        // `c Camouflage` (12) → a 13-wide band, four rows: map origin (16,9), so the
        // first row sits at screen (16,10) (map row + the header).
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
        // Closed: that corner is plain map (interior floor). Open: the panel's first
        // row `r Run` starts there, in Owned.
        assert_eq!(
            closed.get(16, 10).glyph,
            '·',
            "not deployed: the board is whole"
        );
        assert_eq!(
            open.get(16, 10).glyph,
            'r',
            "deployed: panel opposite the player"
        );
        assert_eq!(open.get(18, 10).glyph, 'R', "…the label reads `r Run`");
        assert_eq!(open.get(16, 10).fg, Category::Owned);
        // The far side (near the player, top-left) stays board even when deployed.
        assert_eq!(
            open.get(2, 2).glyph,
            '·',
            "the panel never covers the player's corner"
        );

        // Player bottom-right → panel flips to the top-left corner (map origin
        // (1,1), screen row 2).
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
        assert_eq!(open2.get(1, 2).glyph, 'r', "the corner tracks the player");
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
        // A 4-tall map cannot fit all four panel rows within its inset; the render
        // shows what fits and stops — no panic, and the screen height is intact.
        let g = render_screen(
            &s,
            ScreenUi {
                ability_panel_open: true,
                ..ScreenUi::default()
            },
        );
        assert_eq!(g.height(), HEADER_ROWS + 4 + STATUS_ROWS);
        // Player top-left → panel top-right; its first row draws at map (10,1),
        // screen (10,2).
        assert_eq!(g.get(10, 2).glyph, 'r', "the first row still draws");
    }

    /// The pointer→identity hit-test (§11.4) on the always-on line: each compact
    /// entry's cells resolve to *that* ability by identity, the gaps and the deploy
    /// button resolve to nothing (a tap there toggles the panel, it never falls
    /// through to an activation), and the map below is not the line.
    #[test]
    fn ability_at_resolves_the_compact_line() {
        let s = State::new(
            open_room(30, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(28, 8),
        );
        let ui = ScreenUi::default();

        // r@1 c@3 d@5 x@7 (all ready → one cell each), by identity not position.
        for (col, id) in [
            (1, AbilityId::Run),
            (3, AbilityId::Camouflage),
            (5, AbilityId::Decoy),
            (7, AbilityId::Dephase),
        ] {
            assert_eq!(ability_at(&s, ui, col, 0), Some(id), "col {col}");
        }
        // The space between entries is no ability.
        assert_eq!(
            ability_at(&s, ui, 2, 0),
            None,
            "the gap resolves to nothing"
        );
        // The deploy button is never an ability, even though it is on row 0 — the
        // line stops before it, so a tap there cannot fall through to an activation.
        let start = 30 - 1 - 3;
        assert!(is_ability_button(30, start, 0));
        assert_eq!(
            ability_at(&s, ui, start, 0),
            None,
            "the button is not an ability"
        );
        // The map below the header is not the line while the panel is closed.
        assert_eq!(ability_at(&s, ui, 1, 1), None, "row 1 is the map");
    }

    /// The hit-test on the **deployed panel** (§11.4): its rows, overlaid on the map
    /// beneath the header, resolve by identity to the ability they draw; cells off
    /// the band are nothing; and with the panel closed the same cells are just map.
    #[test]
    fn ability_at_resolves_the_deployed_panel() {
        // Same geometry as `deploying_shows_the_panel_opposite_the_player`: a fresh
        // run, player top-left, panel at map origin (16,9) → screen rows from 10.
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

        // One panel row per economy ability, top to bottom in deck order.
        for (screen_y, id) in [
            (10, AbilityId::Run),
            (11, AbilityId::Camouflage),
            (12, AbilityId::Decoy),
            (13, AbilityId::Dephase),
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

        // The line's Run cell resolves to the same id `r` fires — one path, by identity.
        let clicked = ability_at(&s, ui, 1, 0).expect("Run under the pointer");
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
        let cooling = ability_at(&s, ui, 1, 0).expect("Run still under the pointer");
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
        let near_y = HEADER_ROWS + 6; // header + map height
        let near: String = (0..g.width()).map(|x| g.get(x, near_y).glyph).collect();
        assert_eq!(near.chars().count(), 12, "exactly one grid row wide");
        assert!(
            near.starts_with(" intel in h"),
            "the words run to the edge and stop: {near:?}"
        );
    }
}
