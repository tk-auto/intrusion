//! The loud message's **pop-in box** (§11.7/§11.4, #576): the second surface the top
//! rung of the ladder gets, drawn over the board next to what it is about.
//!
//! Which messages come here is the ladder's to say ([`status::pop_in`](crate::pop_in));
//! *when* the box goes is the shell's clock, out in the web crate where a wall clock
//! belongs (§12.1). What is left — the shape, and where on the board it lands — is
//! here, and it is a pure function of the finished frame plus the message, like every
//! other overlay in this module.
//!
//! # It never covers what it is talking about (§11.7)
//!
//! Every event on the rung today is something the player did to a thing they are
//! standing next to, or to themselves: a console bumped, a crate opened, a key off a
//! belt, the exit. [`Message`] carries no source cell yet (#576 deliberately left that
//! plumbing out — it is not needed until a loud message is ever about something far
//! away), so the box anchors on the **player's cell** and is placed clear of it *and
//! all eight of its neighbours*. That ring is where the thing being reported is, so
//! clearing it is what makes §11.7's rule true for every message on the rung without
//! knowing which one is being shown.
//!
//! # It does not bury the lose condition (§11.5)
//!
//! Burying the danger overlay is the failure mode every surface drawn over the board is
//! bounded against. This one is small, transient, and — among the placements that are
//! legal at all — takes the one covering the **fewest** `Danger` cells. It scores the
//! overlay as *painted*, by reading the finished frame's backgrounds, so it can never
//! drift from the set §11.5 actually drew.

use super::{
    hud, Category, Cell, Fill, GlyphCell, Grid, Visibility, CORNER_BOTTOM_LEFT,
    CORNER_BOTTOM_RIGHT, CORNER_TOP_LEFT, CORNER_TOP_RIGHT, RULE_GLYPH, SIDE_GLYPH,
};
use crate::status::PopIn;

/// How many cells of **words** a line of the box holds (§11.7/#576). **[START]**.
///
/// The box has to wrap where the near line clips: the objective rung runs to 45 cells
/// (*"intel in hand — the exit is open (9 more out)"*) against a 40-wide board (§10.2)
/// and a near line that only ever had 32 to give. Narrow enough that the box stays a
/// note beside the player rather than a panel across the room, wide enough that the
/// rung's messages land in two lines or fewer — `every_pop_in_message_fits_its_box`
/// walks them and fails the build's tests if one ever needs a third.
const TEXT_MAX: usize = 24;

/// The most lines of words the box will draw (§11.7/#576) — the bound that keeps it
/// small enough to be laid over the board at all.
///
/// [`wrap`] truncates at it, silently, which is the §2.3 failure this module's own test
/// exists to catch: a clipped line looks like the whole sentence. One line of headroom
/// over what the rung actually needs, so a slightly longer message wraps rather than
/// vanishing, and the test still fails on the way to needing a fourth.
const MAX_LINES: usize = 3;

/// The cells a line of words costs beyond the words: the two sides, and one cell of air
/// inside each of them so the text never touches the frame.
const FRAME_CELLS: u32 = 4;

/// The cells the box costs beyond its lines: the top edge and the bottom edge.
const EDGE_ROWS: u32 = 2;

/// Lay the pop-in over the finished frame (§11.7/#576).
///
/// Drawn **over the board** — that is the whole point of it, the eye being on the board
/// and not on the top row — and **under** every modal surface: the deployed log, the
/// level-start card and the verdict are all laid on after this one in
/// [`render_screen`](super::render_screen), so a surface the player opened, or one the
/// run ended on, always wins over a transient they did not ask for.
///
/// Draws nothing when no placement is legal — a board too small to hold the box clear of
/// the player, which only a hand-built test state ever is. A frame that cannot show the
/// box honestly shows none of it: half a box over the ring it is meant to keep clear
/// would break the one rule this surface has.
pub(super) fn overlay_pop_in(grid: &mut Grid, pop_in: PopIn, player: Cell) {
    let message = pop_in.message();
    let lines = wrap(&message.text, TEXT_MAX);
    let width = lines
        .iter()
        .map(|line| line.chars().count() as u32)
        .max()
        .unwrap_or(0)
        + FRAME_CELLS;
    let height = lines.len() as u32 + EDGE_ROWS;
    let board = Board::of(grid);
    // The player in **screen** coordinates: the board's own rows start below the two
    // status lines (§11.4), and every placement decision below is made on the screen.
    let player = (player.x, player.y + hud::TOP_ROWS);
    let Some((x, y)) = placement(board, player, width, height, |x, y| {
        grid.get(x, y).bg == Some(Category::Danger)
    }) else {
        return;
    };
    draw_box(grid, x, y, width, height, &lines, message.category);
}

/// The board's extent in **screen** coordinates: the rows between the two status lines
/// above it and the ability bar below (§11.4).
#[derive(Clone, Copy)]
struct Board {
    width: u32,
    top: u32,
    height: u32,
}

impl Board {
    fn of(grid: &Grid) -> Self {
        Self {
            width: grid.width(),
            top: hud::TOP_ROWS,
            height: grid
                .height()
                .saturating_sub(hud::TOP_ROWS + hud::BOTTOM_ROWS),
        }
    }
}

/// Where the box goes (§11.7/#576), or `None` if nowhere legal on this board.
///
/// Four candidates — above, below, right, left of the player — each butted up against
/// the ring of neighbours it must clear and centred on the other axis, then **clamped
/// to the board** so the box is never drawn off-screen. A clamp can slide a candidate
/// back over the ring on a cramped board, so legality is checked *after* it: the ring
/// rule is absolute, the position is not.
///
/// Among the legal ones the winner is the one covering the fewest `Danger` cells
/// (§11.5) — `min_by_key` keeps the **first** minimum, so an all-quiet board resolves in
/// the fixed order above → below → right → left rather than arbitrarily. Above leads
/// because the near line saying the same thing is up there: with the box between the
/// player and the row, both are one glance.
fn placement(
    board: Board,
    player: (u32, u32),
    width: u32,
    height: u32,
    danger: impl Fn(u32, u32) -> bool,
) -> Option<(u32, u32)> {
    let (px, py) = (player.0 as i64, player.1 as i64);
    let (w, h) = (width as i64, height as i64);
    // The free axis is centred on the player and clamped into the board — the box's
    // *offset* along it does not matter, only that it clears the ring, which the fixed
    // axis has already guaranteed.
    let centred_x = clamp(px - w / 2, 0, board.width as i64 - w);
    let centred_y = clamp(
        py - h / 2,
        board.top as i64,
        board.top as i64 + board.height as i64 - h,
    );
    let candidates = [
        (centred_x, py - 1 - h), // above: its last row is two clear of the player's
        (centred_x, py + 2),     // below
        (px + 2, centred_y),     // right
        (px - 1 - w, centred_y), // left
    ];
    candidates
        .into_iter()
        .filter_map(|(x, y)| {
            let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
            (fits(board, x, y, width, height) && clears_the_ring(x, y, width, height, player))
                .then_some((x, y))
        })
        .min_by_key(|&(x, y)| {
            (y..y + height)
                .flat_map(|cy| (x..x + width).map(move |cx| (cx, cy)))
                .filter(|&(cx, cy)| danger(cx, cy))
                .count()
        })
}

fn clamp(value: i64, low: i64, high: i64) -> i64 {
    value.max(low).min(high.max(low))
}

/// Whether the box lies wholly inside the board (§11.7: clamped, never clipped).
fn fits(board: Board, x: u32, y: u32, width: u32, height: u32) -> bool {
    x + width <= board.width && y >= board.top && y + height <= board.top + board.height
}

/// Whether the box covers neither the player's cell nor any of its eight neighbours —
/// the ring the thing being reported is standing in (§11.7).
fn clears_the_ring(x: u32, y: u32, width: u32, height: u32, player: (u32, u32)) -> bool {
    let (px, py) = player;
    let overlaps = |lo: u32, len: u32, at: u32| at + 1 >= lo && at <= lo + len;
    !(overlaps(x, width, px) && overlaps(y, height, py))
}

/// Draw the box: its cells cleared, its frame in the message's §11.2 category, its
/// words in the ink every other status surface writes them in.
///
/// **Cleared first**, so the facility never reads through the words — the same thing the
/// splash and the deployed log do, for the same reason. The cleared cells carry the
/// screen's own backdrop, which is the fill that keeps the text legible for the whole of
/// the box's life however loud the board underneath it gets.
///
/// The **border** carries the colour, not the text: the category is the message's meaning
/// (§11.2) and the frame is the largest run of cells the box has to say it with, while the
/// words stay the near line's Neutral ink so they read at the same weight there and here.
/// Every cell is [`Surface::Chrome`](super::Surface) by construction ([`GlyphCell::blank`]),
/// so a tile renderer draws the sentence as a sentence (#460).
fn draw_box(
    grid: &mut Grid,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    lines: &[String],
    category: Category,
) {
    for row in y..y + height {
        for col in x..x + width {
            put(grid, col, row, ' ', category);
        }
    }
    let bottom = y + height - 1;
    for col in x + 1..x + width - 1 {
        put(grid, col, y, RULE_GLYPH, category);
        put(grid, col, bottom, RULE_GLYPH, category);
    }
    put(grid, x, y, CORNER_TOP_LEFT, category);
    put(grid, x + width - 1, y, CORNER_TOP_RIGHT, category);
    put(grid, x, bottom, CORNER_BOTTOM_LEFT, category);
    put(grid, x + width - 1, bottom, CORNER_BOTTOM_RIGHT, category);
    for (i, line) in lines.iter().enumerate() {
        let row = y + 1 + i as u32;
        put(grid, x, row, SIDE_GLYPH, category);
        put(grid, x + width - 1, row, SIDE_GLYPH, category);
        for (j, glyph) in line.chars().enumerate() {
            put(grid, x + 2 + j as u32, row, glyph, Category::Neutral);
        }
    }
}

/// Write one cell of the box — live, unfogged and backgroundless, so the box reads at
/// full strength wherever on the board it lands and no overlay shows through it.
fn put(grid: &mut Grid, x: u32, y: u32, glyph: char, category: Category) {
    grid.cells[(y * grid.width + x) as usize] = GlyphCell {
        glyph,
        fg: category,
        vis: Visibility::Live,
        fill: Fill::Full,
        ..GlyphCell::blank()
    };
}

/// Break `text` into lines of at most `width` cells, at word boundaries.
///
/// A word longer than a whole line is **cut** rather than dropped — no message on the
/// rung has one, and a line that silently disappeared would be worse than a broken word.
/// The result is capped at [`MAX_LINES`]; `every_pop_in_message_fits_its_box` is what
/// keeps that cap from ever biting a real message.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word;
        while word.chars().count() > width {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            let cut = word
                .char_indices()
                .nth(width)
                .map_or(word.len(), |(index, _)| index);
            lines.push(word[..cut].to_string());
            word = &word[cut..];
        }
        if word.is_empty() {
            continue;
        }
        if line.is_empty() {
            line.push_str(word);
        } else if line.chars().count() + 1 + word.chars().count() <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines.truncate(MAX_LINES);
    lines
}

#[cfg(test)]
mod tests;
