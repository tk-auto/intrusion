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
//! can carry each one's **name** outright — no key-only compaction, no deploy
//! button, no panel unfolding over the board. Fitting four names and their `[N]` /
//! `/N/` numbers across a 40-wide board (§10.2) is a tight budget, and
//! [`MAX_BAR_WIDTH`] spends it under a `const` assertion: a longer bar name, a
//! three-digit cooldown or a bigger tech grant breaks the **build**, not the frame.

use super::*;
use crate::ability::{AbilityId, AbilityState, AbilityStatus, MAX_BAR_ENTRY};
use crate::cell::Direction;
use crate::mnemonic;
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
/// map. Never blank — with nothing adjacent to offer it teaches the innate verbs
/// instead ([`usable_hint`], #323).
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
/// Position is muscle memory, and since #359 it *is* the key as well (§11.6).
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
    /// Which colour table the shell paints from (§11.2/#189). The core carries the
    /// *flag* and never the colours: it says which of presentation's two columns is
    /// live, and the shell owns both. Like every other field here it is a pure view
    /// choice — no world change, no turn (§4.4) — flipped by
    /// [`UiCommand::ToggleTheme`](crate::UiCommand::ToggleTheme), and from the title
    /// screen or the help panel, the option's home until v2's options screen lands.
    ///
    /// In-session only for now: nothing persists it, so a reload comes back on the
    /// [`Default`] dark theme.
    pub theme: Theme,
    /// Which input vocabulary to teach the innate verbs in (§11.6/#323): the
    /// wording of the usable line's floor ([`usable_hint`]), and nothing else.
    /// The shell answers only *is this a touch session?*; the core keeps the words
    /// and the layout, so the hint stays inside the golden tests (§11.2/§12.1).
    pub modality: InputModality,
}

/// The input vocabulary the player is actually using (§11.6/#323) — the one thing
/// a shell knows about the player's hands that the core cannot derive from state.
/// It decides how the usable line's floor words the two innate verbs
/// ([`usable_hint`]) and nothing else: keys and gestures are both live at all
/// times, whatever this says.
///
/// [`Keys`](Self::Keys) is the [`Default`], so a shell that never sets it — the
/// sim, a test, a native harness — teaches the keyboard, which is the modality
/// every one of them drives.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InputModality {
    /// A keyboard session: arrows to move, `5` / `w` to wait (§11.6).
    #[default]
    Keys,
    /// A touch session: swipe to move, tap to wait (§11.6's touch model).
    Touch,
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
///   ([`State::affordances`]), each in its own category, no band — or, when there
///   are none, the move/wait floor ([`usable_hint`], §11.6/#323).
/// - **Ability bar** (row `height-1`): the always-on named readout — every held
///   ability's bar name coloured by state, its active/cooling number tucked against
///   it ([`AbilityStatus::bar_entry`]) — **right-aligned** into the bottom-right
///   corner. This is the permanent home for ability state (§11.4/§15 Q9): one row,
///   glanceable, never covering the board, and under the thumb that taps it (#267).
///
/// # Named, always, with nothing to deploy (§11.4, §15 Q9, #287)
///
/// The bar used to compress each ability to a bare letter and hide the names
/// behind a deploy button that unfolded a panel over the board. With the held set
/// capped at [`AbilityId::MAX_HELD`] (§8.3) the compression bought nothing worth its
/// cost: the names fit, so they are simply always there and the button and panel are
/// gone. Two experiments preceded it and both lost — showing the list only *while
/// waiting* buried the 360° guard-sense the wait exists to reveal (§9.1), and a
/// left-aligned header strip put the tap target furthest from the thumb (#267).
///
/// The bar draws the run's **real** ability state ([`State::ability_statuses`]); a
/// click on an entry resolves to the ability under it ([`ability_at`]) and activates
/// it exactly as that slot's digit would. Since #359 the bar is the keys' **source**
/// rather than their projection — `1`–`4` fire its first through fourth entries
/// (§11.6) — so the order this row draws in is load-bearing, and the help panel's
/// Abilities tab is where a player reads each pairing off.
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
    // draws itself from the run's active modifiers (§12.6), its ability loadout
    // (§8.3/#343) and the chosen tab.
    if ui.help_open {
        return super::help::render_help(
            width,
            height,
            ui.help_tab,
            state.level(),
            state.modifiers(),
            state.loadout(),
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
    // Nothing to act on: the row teaches the two innate verbs instead of sitting
    // empty (§11.4/#323), in the modality the shell says the player is using. A
    // floor, never a competitor — one adjacent usable and the affordances have the
    // row back.
    let usable = if usable.is_empty() {
        usable_hint(ui.modality)
    } else {
        usable
    };
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
///
/// One cell of each entry is the **mnemonic** (§11.6/#360): the letter that entry
/// answers to, lifted to [`Category::Neutral`] — the ink colour, the brightest thing
/// the palette has — so it stands out of the name around it as *the key you press*.
/// It recolours a cell the entry had already drawn, so it costs no width and §11.4's
/// compile-time slot arithmetic is untouched.
///
/// **An entry you cannot use keeps its letter dim.** A [`Category::Ground`] entry —
/// exhausted or unusable, the two states §11.4 draws as "plainly not an option now" —
/// recedes whole: brightening one cell of it would advertise a key for something that
/// is not on offer, and the eye would be pulled to exactly the entry it should skip.
/// The letter still *works* there (it resolves like any other, and refuses for free in
/// the economy, §4.4); it simply does not shout.
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

    let layout = ability_line_layout(width, statuses);
    let mnemonics = mnemonic::claim(&drawn_bar_names(&layout, statuses));
    for ((i, start), letter) in layout.into_iter().zip(mnemonics) {
        let status = &statuses[i];
        let category = bar_category(status.state);
        put(&mut cells, start, &status.bar_entry(), category);
        // The mark is the letter's own colour — no band behind it, nothing added
        // beside it — so the bar stays the quiet strip §11.4 asks for and the entry
        // reads as one word with one letter picked out of it.
        if category == Category::Ground {
            continue; // an entry you cannot use does not advertise its key
        }
        if let Some(x) = letter.map(|l| start + l as u32).filter(|x| *x < width) {
            cells[x as usize].fg = Category::Neutral;
        }
    }
    cells
}

/// The bar names of the **drawn** slots, in slot order — what the mnemonic claim
/// (§11.6/#360) runs over.
///
/// Drawn rather than held, so a letter is never claimed by an entry the row
/// truncated away: the mark *is* the binding's only announcement, so a key nobody
/// can see must not exist.
fn drawn_bar_names(layout: &[(usize, u32)], statuses: &[AbilityStatus]) -> Vec<&'static str> {
    layout
        .iter()
        .map(|&(i, _)| statuses[i].id.bar_name())
        .collect()
}

/// The **mnemonic letter** of bar slot `slot`, or `None` where that slot has none
/// (§11.6/#360) — the secondary key beside the slot's digit.
///
/// Derived from the run's own drawn bar, so it is the letter the row is highlighting
/// at this very moment ([`ability_bar`]) and the letter the help panel prints — one
/// derivation, three readers, no chance of the screen naming a key that does not fire.
pub fn ability_mnemonic(state: &State, slot: usize) -> Option<char> {
    let statuses = state.ability_statuses();
    let layout = ability_line_layout(state.layout().facility().width(), &statuses);
    let names = drawn_bar_names(&layout, &statuses);
    let index = (*mnemonic::claim(&names).get(slot)?)?;
    Some(mnemonic::letter_at(names[slot], index))
}

/// The bar slot a **mnemonic letter** fires (§11.6/#360), or `None` for a character
/// no entry of this run claimed — the letter counterpart of
/// [`ability_slot_for_code`](crate::ability_slot_for_code).
///
/// Case is folded, so a stray Shift (or Caps Lock) still fires the ability rather
/// than silently costing the turn it was meant to spend. It lands on a *slot*, like
/// the digit and like a tap, so all three meet at [`ability_in_slot`] and none of
/// them can name a different ability from the one under the highlight.
pub fn ability_slot_for_letter(state: &State, key: &str) -> Option<usize> {
    let mut chars = key.chars();
    let ch = match (chars.next(), chars.next()) {
        (Some(c), None) => c.to_ascii_lowercase(),
        _ => return None, // named keys ("Tab", "ArrowUp") are never a mnemonic
    };
    let statuses = state.ability_statuses();
    let layout = ability_line_layout(state.layout().facility().width(), &statuses);
    let names = drawn_bar_names(&layout, &statuses);
    mnemonic::claim(&names)
        .into_iter()
        .enumerate()
        .find(|&(slot, index)| index.is_some_and(|i| mnemonic::letter_at(names[slot], i) == ch))
        .map(|(slot, _)| slot)
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

/// The ability in bar **slot** `slot`, counting from `0` at the row's leftmost
/// *drawn* entry, or `None` for a slot this run does not fill (§11.4/§11.6, #359).
///
/// **The one place a slot becomes an ability.** The keyboard arrives here with a
/// digit ([`ability_slot_for_code`](crate::ability_slot_for_code)) and the pointer
/// with a column ([`ability_at`]), so `1` and the entry under the thumb can never
/// name different abilities — the divergence §11.4 pins against, now that the keys
/// *are* the bar's positions rather than a projection of identity.
///
/// Counted against what is **drawn**, not against the catalogue: the row is flush
/// right (#267) and shorter loadouts sit further in, so slot `0` is the leftmost
/// entry on screen and never a gap where the first catalogue row would have gone. A
/// slot the row truncated away (an oversized hand-built state — see
/// [`ability_line_layout`]) is `None` too, so no key fires an entry nobody can see.
pub fn ability_in_slot(state: &State, slot: usize) -> Option<AbilityId> {
    let statuses = state.ability_statuses();
    ability_line_layout(state.layout().facility().width(), &statuses)
        .get(slot)
        .map(|&(i, _)| statuses[i].id)
}

/// The ability entry at screen cell `(x, y)`, or `None` — the **pure**
/// pointer→identity hit-test for the always-on bar (§11.4). A shell maps a click to
/// a screen cell and asks this; a hit fires `Input::Activate(id)` on the returned
/// ability. It resolves the slot the click landed on and hands it to
/// [`ability_in_slot`], the one seam the keyboard's digits go through too — so it
/// opens no second activation path (the §8.4 regression) and, on a cooling/active
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
    let slot = ability_line_layout(facility.width(), &state.ability_statuses())
        .into_iter()
        .position(|(_, start)| x >= start && x < start + MAX_BAR_ENTRY as u32)?;
    ability_in_slot(state, slot)
}

/// The §11.2 category an ability entry reads in, by its state: an available ability
/// — ready, active, or a passive in effect — is **Owned** (blue, "yours, in hand");
/// a cooling one is **System** (the muted furniture tan, "unavailable, will
/// return"); an unusable one is **Ground** (dim gray, receding) — discoverable but
/// plainly not an option now. The `[N]` / `/N/` / `(N)` / `(on)` notation carries the
/// rest, so those states share a colour without ambiguity.
///
/// The two #302 states take the colour of what they *are*, not of the axis they come
/// from: an ability with uses left is available, so it is Owned like any ready one;
/// an [`Exhausted`](AbilityState::Exhausted) one is not merely waiting — it is done
/// for this facility — so it recedes to Ground beside the other things you cannot do,
/// rather than to the System tan that promises a return.
fn bar_category(state: AbilityState) -> Category {
    match state {
        AbilityState::Ready
        | AbilityState::Active { .. }
        | AbilityState::Limited { .. }
        | AbilityState::Passive => Category::Owned,
        AbilityState::Cooling { .. } => Category::System,
        AbilityState::Exhausted | AbilityState::Unusable => Category::Ground,
    }
}

/// The usable line's floor, on touch (§11.4/§11.6/#323): the two innate verbs in
/// the gesture vocabulary. The third gesture — a press held in place, which
/// repeats Wait — is deliberately unnamed: the hint is a floor, not a manual, and
/// the help panel's Help card is where the full set is read.
const TOUCH_HINT: [&str; 2] = ["swipe: move", "tap: wait"];

/// The usable line's floor, on keys (§11.4/§11.6/#323): the same two verbs off
/// §11.6's own table — the arrows the row already draws, and `w` to wait.
///
/// It names `w` alone rather than the `5/w` it used to (#369). The wait digit is the
/// **numpad**'s `5`, and a floor hint has no room to say which `5` it means — a
/// player reading it off a laptop and pressing the top row would get nothing at all.
/// `w` is the key that is there on every keyboard; the full spelling is one `?` away.
const KEYS_HINT: [&str; 2] = ["↑↓←→: move", "w: wait"];

/// How many **cells** a hint segment occupies: its `char` count, since every glyph
/// the hints use is one grid cell wide. Counts the UTF-8 lead bytes rather than
/// calling `chars`, so the budget below can be spent at compile time.
const fn hint_cells(text: &str) -> u32 {
    let bytes = text.as_bytes();
    let (mut i, mut cells) = (0, 0);
    while i < bytes.len() {
        // Every byte that is not a continuation byte (`10xxxxxx`) starts a char.
        if bytes[i] & 0xC0 != 0x80 {
            cells += 1;
        }
        i += 1;
    }
    cells
}

/// The width a two-segment hint draws to, in cells: [`status_row`]'s one-cell left
/// margin, the two segments, and the two spaces between them.
const fn hint_width(hint: [&str; 2]) -> u32 {
    1 + hint_cells(hint[0]) + 2 + hint_cells(hint[1])
}

/// **Both hints must fit the board they are drawn on**, the way the ability bar's
/// worst case does (#287): a hint clipped mid-word teaches nothing, and discovering
/// that in a screenshot is discovering it too late. Rewording either variant past
/// the v1 width (§10.2) fails the *build*, not the frame.
const _: () = assert!(
    hint_width(TOUCH_HINT) <= LevelConfig::V1.width
        && hint_width(KEYS_HINT) <= LevelConfig::V1.width,
    "the usable line's move/wait hint must fit the v1 board (§10.2): shorten a segment",
);

/// The usable line's **floor** (§11.4/#323): how to move and how to wait, in the
/// vocabulary the player's hands are using, drawn whenever there is no affordance
/// to offer instead.
///
/// The row is the one piece of permanent screen the HUD would otherwise give away
/// for nothing, and it sits directly above a board on which the player has to work
/// out unaided that **waiting is an action at all** — the only 360° look (§9.1),
/// the way a crouch is held (§10.3) and the way a cone is let past (§7.6). Wait has
/// no ability-bar entry by design (the bar is the ability *economy*, §8.3), so
/// without this the two innate verbs live on a row that never mentions them.
///
/// It is the same move the near line already makes one row up — ambient status
/// instead of an empty line (§11.4) — and it is a **floor, never a competitor**:
/// the moment anything is adjacent, the affordances take the row back whole.
///
/// The words draw in [`Owned`](Category::Owned) — *you, and the things you made*
/// (§11.2). [`Ground`](Category::Ground) was the first answer and it was the wrong
/// one on the screen: Ground's meaning is **absence**, drawn to recede so that
/// everything else pops against it, which is precisely the wrong instruction for a
/// row whose whole job is to be read. Owned says what these two verbs actually are
/// — not scenery, not something to bump, but *yours*, the pair you always hold —
/// and it puts them in the same blue as the ability bar's ready entries, so the
/// two surfaces that answer "what can I do right now" answer in one colour.
fn usable_hint(modality: InputModality) -> Vec<(String, Category)> {
    let hint = match modality {
        InputModality::Keys => KEYS_HINT,
        InputModality::Touch => TOUCH_HINT,
    };
    hint.iter()
        .map(|text| ((*text).to_string(), Category::Owned))
        .collect()
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

/// How many **map rows** the deployed message log covers right now (§11.7), or `0`
/// when nothing of it is on the board — the geometry half of
/// [`overlay_message_log`], read by a shell that must know which rows are the log's
/// rather than the board's (#306: a tap on the list you opened to read must never
/// burn a turn).
///
/// Mirrors the drawing exactly: the log earns the board only when it is deployed
/// **and** more than one message is live, it hangs from the top of the map, and it is
/// clamped to the map's height on a board too short to hold every row. `0` while a
/// modal screen is up ([`ScreenUi::menu`] / [`ScreenUi::help_open`]) because then no
/// board is drawn at all.
pub fn message_log_rows(state: &State, ui: ScreenUi) -> u32 {
    if ui.menu.is_some() || ui.help_open || !ui.message_log_open {
        return 0;
    }
    let live = live_messages(state).len() as u32;
    if live < 2 {
        return 0;
    }
    live.min(state.layout().facility().height())
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
    use crate::state::{BoreRefusal, Event, Input, State};
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
    ///
    /// The exit's refusal left this list in #310: naming the gate's real requirement
    /// ("the exit needs 2 more intel") is shorter than the fixed rule it replaced.
    const PRE_EXISTING_OVERFLOW: [&str; 6] = [
        "all the intel — the exit is open",
        "the guard drops — a body is left",
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
            Event::IntelTaken {
                remaining: 0,
                still_needed: 0,
            },
            Event::IntelTaken {
                remaining: 9,
                still_needed: 0,
            },
            Event::IntelTaken {
                remaining: 9,
                still_needed: 9,
            },
            Event::ExitRefused { still_needed: 9 },
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
            Event::Ejected {
                from: at,
                to: at,
                stunned: crate::phase_eject_stun(1),
            },
            Event::Entombed { at },
            Event::RematerializeRefused,
            Event::WallBored { at },
        ];
        // Every bore refusal is a near-line message of its own (§8.4/#303), so each
        // wording is measured rather than just one representative.
        let events = events.into_iter().chain(
            [
                BoreRefusal::NothingToBore,
                BoreRefusal::TooManyWalls,
                BoreRefusal::TheOuterShell,
                BoreRefusal::NoUsesLeft,
            ]
            .map(|reason| Event::BoreRefused { reason }),
        );
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
        // name, so the longest name is what decides whether they fit. The budgeted
        // activation (§8.2/#302) is the longest of them, so both of its wordings —
        // the count and the spent-it-all one — are measured here as well.
        for ability in AbilityId::ALL {
            for event in [
                Event::AbilityActivated {
                    ability,
                    uses_left: None,
                },
                Event::AbilityActivated {
                    ability,
                    uses_left: Some(9),
                },
                Event::AbilityActivated {
                    ability,
                    uses_left: Some(0),
                },
                Event::AbilityDeactivated { ability },
                Event::AbilityExpired { ability },
            ] {
                // A budgeted activation is deliberately silent (§8.2/#302) — nothing
                // to measure, so nothing to fit.
                let Some(m) = crate::status::message_for(event) else {
                    continue;
                };
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

    /// [`message_log_rows`] tells a shell exactly which map rows the deployed list
    /// covers (#306), and it must agree with the drawing: nothing while folded,
    /// nothing while a modal screen is up, one row per live message once deployed —
    /// the rows a tap must never read as the board underneath.
    #[test]
    fn the_message_log_reports_the_rows_it_covers() {
        // The same two-message step as above: `TakenDown` plus `BodyFound`.
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

        let deployed = ScreenUi {
            message_log_open: true,
            ..ScreenUi::default()
        };
        assert_eq!(live_messages(&s).len(), 2, "two messages are live");
        assert_eq!(
            message_log_rows(&s, deployed),
            2,
            "deployed, the list covers one map row per live message"
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

        // One message earns no list at all — the near line simply speaks it (§11.7).
        let quiet = State::new(
            open_room(40, 14),
            Cell::new(5, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(8, 8),
        );
        assert!(live_messages(&quiet).len() < 2);
        assert_eq!(message_log_rows(&quiet, deployed), 0);
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
    ///
    /// The map half also pins the §11.5a schematic (#307) end to end, which a
    /// whole-frame golden shows better than any single assertion: the player's own
    /// ~180° half-disc reads in real glyphs, and everything they have never had eyes
    /// on — the run of wall behind them, the far two-thirds of the room — reads as
    /// `≈` fabric and `~` floor space. The exit keeps its `E` out there regardless,
    /// the one thing on this map that is theirs (§4.5).
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
                " 1 more intel to leave              [?] ".to_string(),
                " → console: take intel                  ".to_string(),
                "##################≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈".to_string(),
                "#·················~~~~~~~~~~~~~~~~~~~~~≈".to_string(),
                "#·@$··············~~~~~~~~~~~~~~~~~~~~~≈".to_string(),
                "#·················~~~~~~~~~~~~~~~~~~~~~≈".to_string(),
                "≈~~~~~············~~~~~~~~~~~~~~~~~~~~E≈".to_string(),
                "≈≈≈≈≈≈≈≈##########≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈".to_string(),
                "Run       Camo      Decoy     Phase     ".to_string(),
            ]
        );
    }

    /// §11.4/#323: with nothing adjacent to act on, the usable line teaches the two
    /// innate verbs instead of sitting blank — in the vocabulary the player's hands
    /// are using ([`ScreenUi::modality`]). The words draw in Owned — *yours*, the
    /// pair you always hold (§11.2), the ability bar's own ready colour — and carry
    /// no band, so the row still reads as status rather than as a message.
    #[test]
    fn the_empty_usable_line_teaches_move_and_wait() {
        // Mid-corridor, nothing adjacent: the common case the blank row used to be.
        let s = State::new(
            open_room(40, 6),
            Cell::new(20, 3),
            Direction::North,
            Vec::new(),
            [Cell::new(2, 2)],
            Cell::new(38, 4),
        );
        assert!(s.affordances().is_empty(), "nothing to act on");

        let row = |ui: ScreenUi| {
            let g = render_screen(&s, ui);
            (0..g.width())
                .map(|x| g.get(x, USABLE_ROW).glyph)
                .collect::<String>()
        };
        assert_eq!(
            row(ScreenUi::default()),
            " ↑↓←→: move  w: wait                    ",
            "keys: §11.6's own table, in the row's `input: action` rhythm"
        );
        assert_eq!(
            row(ScreenUi {
                modality: InputModality::Touch,
                ..ScreenUi::default()
            }),
            " swipe: move  tap: wait                 ",
            "touch: the gesture model, the held press deliberately unnamed"
        );

        // Owned, and no band: the verbs are yours, and the row is still not a message.
        let g = render_screen(&s, ScreenUi::default());
        for x in 0..g.width() {
            let cell = g.get(x, USABLE_ROW);
            assert_eq!(cell.bg, None, "the usable line still has no band");
            if cell.glyph != ' ' {
                assert_eq!(
                    cell.fg,
                    Category::Owned,
                    "the innate verbs are yours (§11.2), in the bar's ready colour"
                );
            }
        }
    }

    /// The hint is a **floor, never a competitor** (§11.4/#323): one adjacent usable
    /// and the affordances have the whole row back, in both modalities — the player
    /// never has to read past a control legend to find what they can bump.
    #[test]
    fn a_real_affordance_takes_the_whole_row_back() {
        let s = State::new(
            open_room(40, 6),
            Cell::new(2, 2),
            Direction::North,
            Vec::new(),
            [Cell::new(3, 2)], // a console east of the player
            Cell::new(38, 4),
        );
        for modality in [InputModality::Keys, InputModality::Touch] {
            let g = render_screen(
                &s,
                ScreenUi {
                    modality,
                    ..ScreenUi::default()
                },
            );
            let row: String = (0..g.width()).map(|x| g.get(x, USABLE_ROW).glyph).collect();
            assert_eq!(
                row.trim_end(),
                " → console: take intel",
                "{modality:?}: {row:?}"
            );
        }
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
            // Sampled one cell in: the entry's *first* cell is its mnemonic, lifted to
            // the ink colour (§11.6/#360), so the state colour is read off any other.
            assert_eq!(
                g.get(col + 1, bar).fg,
                Category::Owned,
                "{name} ready colour"
            );
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

    /// **The bar greys a press that cannot fire** (§11.4/#345): the contextual
    /// `Unusable` the catalog always documented and nothing ever produced. Pierce
    /// Wall is the clearest case, because its precondition is *exactly one adjacent
    /// wall* (§8.3/#303) and the same three cells of board decide it.
    ///
    /// Three stands, one grid each:
    ///
    /// - **in a room** — no wall touches the player, so there is nothing to bore;
    /// - **in a corridor** — two side walls, and the target would be ambiguous, which
    ///   this ability never resolves (§8.4 [SETTLED]);
    /// - **against one wall** — the one geometry it works in.
    ///
    /// The first two draw `Bore—` receding into [`Category::Ground`] beside the other
    /// things you cannot do; the third draws `Bore(3)` in Owned, its budget intact
    /// throughout. Same run, same supply, three cells apart: what changed is the
    /// board, which is exactly what the bar could not say before.
    #[test]
    fn the_bar_greys_an_ability_with_no_target() {
        let borer = |layout| {
            State::new(
                layout,
                Cell::new(15, 5),
                Direction::North,
                Vec::new(),
                Vec::new(),
                Cell::new(38, 8),
            )
            .with_loadout(Loadout::innate().with(AbilityId::PierceWall))
        };
        let bar = ability_row(10);
        let row = |s: &State| -> String {
            let g = render_screen(s, ScreenUi::default());
            (0..g.width()).map(|x| g.get(x, bar).glyph).collect()
        };
        // Bore's entry starts at column 30, so 30 is its mnemonic `B` and 31 is the
        // first cell carrying the plain state colour (§11.6/#360).
        let colour = |s: &State| render_screen(s, ScreenUi::default()).get(31, bar).fg;
        let letter = |s: &State| render_screen(s, ScreenUi::default()).get(30, bar).fg;

        // In the middle of the room: nothing to bore.
        let s = borer(open_room(40, 10));
        assert_eq!(row(&s), "                    Run       Bore—     ");
        assert_eq!(colour(&s), Category::Ground, "greyed, not promised");
        // …and its mnemonic greys with it (#360): an entry that is not on offer does
        // not advertise a key, so the letter is *not* lifted out of the name here.
        assert_eq!(letter(&s), Category::Ground, "the letter recedes too");

        // In a corridor: two side walls, so the target is ambiguous and refused.
        let mut layout = open_room(40, 10);
        layout.place(Cell::new(14, 5), Terrain::Wall);
        layout.place(Cell::new(16, 5), Terrain::Wall);
        let s = borer(layout);
        assert_eq!(row(&s), "                    Run       Bore—     ");
        assert_eq!(colour(&s), Category::Ground, "two walls is no target");

        // Square against one wall face: the one geometry it works in, and the budget
        // it had all along finally shows.
        let mut layout = open_room(40, 10);
        layout.place(Cell::new(16, 5), Terrain::Wall);
        let s = borer(layout);
        assert_eq!(row(&s), "                    Run       Bore(3)   ");
        assert_eq!(colour(&s), Category::Owned, "available, and says how often");
        // Usable again, so the `B` lifts back out of the name and says "press this".
        assert_eq!(letter(&s), Category::Neutral, "the letter marks the key");
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
        // Read one cell in from each entry's start: cells 0 and 10 are the mnemonics
        // `R` and `C`, drawn in the ink colour whatever the state (§11.6/#360).
        assert_eq!(g.get(1, bar).fg, Category::System, "cooling reads System");
        assert_eq!(g.get(3, bar).glyph, '/', "cooling shows /N/");
        assert_eq!(g.get(10, bar).glyph, 'C');
        assert_eq!(g.get(11, bar).fg, Category::Owned, "active reads Owned");
        // The mark survives both states — a cooling ability's key is still its key.
        for x in [0, 10] {
            assert_eq!(g.get(x, bar).fg, Category::Neutral, "the mnemonic at {x}");
        }
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

    /// The bar is the keys' **source** (§11.6/#267/#359): every entry the row draws
    /// is the slot its digit fires, counted from the leftmost drawn entry — and each
    /// ability state still reads its own colour, ready and active Owned, cooling
    /// System, so the states stay discoverable without a letter on the row.
    #[test]
    fn the_bar_slots_are_the_keys_and_the_states_keep_their_colours() {
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

        // Every entry the bar draws is the slot its digit fires, and the tap on that
        // entry resolves to the same ability — the one seam, both ways in.
        for (slot, (i, start)) in ability_line_layout(40, &s.ability_statuses())
            .into_iter()
            .enumerate()
        {
            let id = s.ability_statuses()[i].id;
            assert_eq!(
                ability_at(&s, start, bar),
                Some(id),
                "{id:?} under its own entry"
            );
            assert_eq!(
                ability_in_slot(&s, slot),
                Some(id),
                "the {} key fires {id:?}",
                slot + 1,
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
        // One cell in from each entry's start, since the first cell is its mnemonic
        // and carries the ink colour whatever the state (§11.6/#360).
        assert_eq!(g.get(entry(AbilityId::Run) + 1, bar).fg, Category::System);
        assert_eq!(
            g.get(entry(AbilityId::Camouflage) + 1, bar).fg,
            Category::Owned
        );
        assert_eq!(g.get(entry(AbilityId::Decoy) + 1, bar).fg, Category::Owned);
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
        // One cell in: `S` is the entry's mnemonic and carries the ink colour (#360).
        assert_eq!(g.get(sight + 1, bar).fg, Category::Owned);
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

    /// #360's named case, on a real bar: a loadout of Decoy + Doors + Daze — three bar
    /// names starting `D` — gives three **distinct** letters, each firing its own
    /// ability, none of them a key §11.6 had already bound.
    ///
    /// The letters are the payload, but the assertion that matters is the last one:
    /// each letter resolves to the slot whose entry the bar *highlighted*, so what the
    /// row promises is what the keyboard does.
    #[test]
    fn three_names_starting_d_get_three_distinct_working_letters() {
        let held = [AbilityId::Decoy, AbilityId::Autodoors, AbilityId::Confusion];
        let s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(held.into_iter().fold(Loadout::empty(), Loadout::with));

        let letters: Vec<Option<char>> = (0..held.len())
            .map(|slot| ability_mnemonic(&s, slot))
            .collect();
        assert_eq!(
            letters,
            vec![Some('d'), Some('o'), Some('a')],
            "Decoy keeps `d`; Doors and Daze fall through their own names",
        );

        for (slot, id) in held.into_iter().enumerate() {
            let letter = letters[slot].expect("each of the three claimed a letter");
            assert_eq!(
                ability_slot_for_letter(&s, &letter.to_string()),
                Some(slot),
                "{letter:?} fires slot {slot}",
            );
            assert_eq!(
                ability_in_slot(&s, slot),
                Some(id),
                "…which is where {id:?} is drawn",
            );
            // Uppercase lands on the same slot: a stray Shift must not cost the turn.
            assert_eq!(
                ability_slot_for_letter(&s, &letter.to_uppercase().to_string()),
                Some(slot),
                "{letter:?} fires with Shift held too",
            );
        }
        // A letter nobody claimed fires nothing, and stays the page's.
        for key in ["z", "q", "ArrowUp", "1"] {
            assert_eq!(ability_slot_for_letter(&s, key), None, "{key:?}");
        }
    }

    /// The mark **is** the binding's announcement (§11.6/#360), so the cell the bar
    /// lifts has to be the cell of the letter that fires it — for every entry of a
    /// loadout, not just the ones whose initial was free.
    #[test]
    fn the_marked_cell_is_the_letter_that_fires_it() {
        let held = [
            AbilityId::Run,
            AbilityId::Decoy,
            AbilityId::Autodoors,
            AbilityId::Confusion,
        ];
        let s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(held.into_iter().fold(Loadout::empty(), Loadout::with));
        let grid = render_screen(&s, ScreenUi::default());
        let bar = ability_row(10);
        let layout = ability_line_layout(40, &s.ability_statuses());

        for (slot, start) in layout.iter().map(|&(_, start)| start).enumerate() {
            let letter = ability_mnemonic(&s, slot).expect("every entry here claims one");
            let state = bar_category(s.ability_statuses()[slot].state);
            // Exactly one cell **of the entry** is lifted to the ink colour. The
            // slot's trailing blanks are Neutral too — they are the row's own filler,
            // not part of the word — so the scan is over the glyphs the entry drew.
            let marked: Vec<u32> = (start..start + MAX_BAR_ENTRY as u32)
                .filter(|&x| {
                    let cell = grid.get(x, bar);
                    cell.glyph != ' ' && cell.fg == Category::Neutral
                })
                .collect();
            if state == Category::Ground {
                // Autodoors has no door to work in an open room, so its entry is
                // unusable and unmarked — the rule the sibling test pins, asserted
                // here too so this sweep cannot quietly skip an entry.
                assert!(marked.is_empty(), "slot {slot} is unusable and unmarked");
                continue;
            }
            assert_eq!(marked.len(), 1, "slot {slot} marks one cell");
            // …it is the letter that fires the slot…
            let cell = grid.get(marked[0], bar);
            assert_eq!(
                cell.glyph.to_ascii_lowercase(),
                letter,
                "slot {slot} marks the cell of {letter:?}",
            );
            assert_eq!(ability_slot_for_letter(&s, &letter.to_string()), Some(slot));
            // …and nothing is drawn behind it: the mark is the letter's own colour, so
            // the bar stays a quiet strip rather than growing a band (§11.4).
            assert_eq!(
                cell.bg, None,
                "slot {slot} paints no ground under its letter"
            );
            // The rest of the entry still carries the state colour, which is what the
            // mark must not swallow. Sampled off the first glyph that is *not* the
            // marked one — Daze claims `a`, its second character, so "the cell after
            // the start" is not a safe stand-in for "not the mnemonic".
            let plain = (start..start + MAX_BAR_ENTRY as u32)
                .find(|&x| x != marked[0] && grid.get(x, bar).glyph != ' ')
                .expect("an entry is more than its mnemonic");
            assert_eq!(
                grid.get(plain, bar).fg,
                state,
                "slot {slot} still says what state it is in",
            );
        }
    }

    /// An entry the player **cannot use** keeps its letter dim (§11.6/#360): the ink
    /// mark says "press this", so putting one on an entry that is not on offer would
    /// pull the eye to exactly the thing to skip. Pierce Wall in open ground has no
    /// target, so its whole entry — the `B` included — reads Ground.
    #[test]
    fn an_unusable_entry_does_not_mark_its_letter() {
        let s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(Loadout::innate().with(AbilityId::PierceWall));
        let grid = render_screen(&s, ScreenUi::default());
        let bar = ability_row(10);
        let layout = ability_line_layout(40, &s.ability_statuses());
        let (_, bore_start) = layout[1];

        assert_eq!(
            s.ability_state(AbilityId::PierceWall),
            AbilityState::Unusable,
            "nothing to bore in an open room",
        );
        assert_eq!(
            ability_mnemonic(&s, 1),
            Some('b'),
            "it still claims its letter"
        );
        assert_eq!(
            ability_slot_for_letter(&s, "b"),
            Some(1),
            "…and the key still resolves — it refuses for free (§4.4), not silently",
        );
        for x in bore_start..bore_start + "Bore".len() as u32 {
            assert_eq!(
                grid.get(x, bar).fg,
                Category::Ground,
                "the whole entry recedes, mnemonic included (col {x})",
            );
        }
        // Run, beside it, is usable — so its letter *is* marked. Same frame, so this
        // is the contrast the rule is made of rather than a second run's.
        assert_eq!(grid.get(layout[0].1, bar).fg, Category::Neutral);
    }

    /// #359's binding, against the row it is a binding *on*: a **three**-ability run
    /// answers `1`, `2` and `3` — each firing the ability whose entry the bar drew at
    /// that slot — and `4` fires nothing at all, because the run has no fourth entry.
    ///
    /// The digits count the row **as drawn**, which is the trap the ticket named: the
    /// bar is flush right (#267), so a short loadout starts well in from the left edge
    /// and a slot counted from the catalogue instead would leave `1` dead. The
    /// assertion below reads the id straight out of the drawn cells to keep the count
    /// honest.
    #[test]
    fn a_three_ability_loadout_answers_one_two_three_and_ignores_four() {
        let held = [AbilityId::Run, AbilityId::Camouflage, AbilityId::Decoy];
        let s = State::new(
            open_room(40, 10),
            Cell::new(15, 5),
            Direction::North,
            Vec::new(),
            Vec::new(),
            Cell::new(38, 8),
        )
        .with_loadout(held.into_iter().fold(Loadout::empty(), Loadout::with));
        let grid = render_screen(&s, ScreenUi::default());
        let bar = ability_row(10);

        // Three entries on a 40-wide row, flush right: the first is drawn at column 10
        // and nothing at all is drawn at 0 — the cell a catalogue-counted `1` would
        // have fired.
        let layout = ability_line_layout(40, &s.ability_statuses());
        assert_eq!(
            layout.iter().map(|&(_, x)| x).collect::<Vec<_>>(),
            vec![10, 20, 30],
            "a short bar sits away from the left edge",
        );
        assert_eq!(grid.get(0, bar).glyph, ' ', "…leaving the left edge blank");

        for (slot, id) in held.into_iter().enumerate() {
            assert_eq!(
                ability_in_slot(&s, slot),
                Some(id),
                "the {} key fires {id:?}",
                slot + 1,
            );
            // …and that is the entry the row drew there: the slot's first cells spell
            // its bar name.
            let start = layout[slot].1;
            let drawn: String = (0..id.bar_name().chars().count() as u32)
                .map(|i| grid.get(start + i, bar).glyph)
                .collect();
            assert_eq!(
                drawn,
                id.bar_name(),
                "slot {} draws what it fires",
                slot + 1
            );
            // The tap on that same cell agrees, which is the seam both go through.
            assert_eq!(ability_at(&s, start, bar), Some(id));
        }

        // `4` is a key this run has no entry for: nothing to fire, so nothing happens.
        assert_eq!(
            ability_in_slot(&s, 3),
            None,
            "a digit past the held count fires nothing",
        );
    }

    /// The click **is** the key (§11.4/§11.6): a bar cell and that entry's digit
    /// resolve through the one [`ability_in_slot`] seam to the same id, and both hand
    /// it to the one `State::ability_input` toggle (#304) — so a click activates a
    /// ready ability and, on a cooling one, refuses for free with no turn spent
    /// (§4.4), exactly as the key.
    #[test]
    fn a_click_activates_by_the_same_path_as_the_digit() {
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

        // The bar's first slot resolves to the same id `1` fires — one path, one seam.
        let clicked = ability_at(&s, 0, bar).expect("Run under the pointer");
        assert_eq!(
            ability_in_slot(&s, 0),
            Some(clicked),
            "the click and the digit resolve to the same ability",
        );
        assert_eq!(
            s.ability_input(clicked),
            Input::Activate(clicked),
            "a ready ability switches on from either",
        );

        // A click on a ready ability activates it (a spent turn).
        let events = s.step(s.ability_input(clicked));
        assert_eq!(s.turn(), 1, "activating spends the turn");
        assert!(!events.is_empty(), "the ability activated");

        // The entry is `Run[4]` now, and the bar is a projection of the keys
        // (§11.4/#304): tapping it switches the sprint off, exactly as pressing `r`
        // again does. The tap resolving to `Activate` here was the whole of #304 —
        // there was no reachable way to stop a sprint.
        let active = ability_at(&s, 0, bar).expect("Run still under the pointer");
        assert_eq!(
            s.ability_input(active),
            Input::Deactivate(AbilityId::Run),
            "an active entry is the toggle-off, from tap and key alike",
        );

        // Drive Run to cooling, then a click on its (now cooling) entry refuses
        // cleanly: the same `Input::Activate` is a free no-op — no turn, no change.
        s.step(s.ability_input(active));
        assert!(matches!(
            s.ability_state(AbilityId::Run),
            AbilityState::Cooling { .. }
        ));
        // The entry widened to `Run/12/` inside its slot — which did not move, so
        // the very same cell is still Run (#287).
        let cooling = ability_at(&s, 0, bar).expect("Run still under the pointer");
        assert_eq!(
            s.ability_input(cooling),
            Input::Activate(cooling),
            "a cooling entry is still an activation — the one that refuses",
        );
        let turn_before = s.turn();
        let refused = s.step(s.ability_input(cooling));
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
        assert!(row0.contains("[Abilities]") && row0.contains("[Help]"));
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
    /// level-seed token — the whole chain, `start_level` → `State::level` →
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
            text.contains(&level.encode().expect("a config a run can hold")),
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
        let height = closed.height;
        assert!(matches!(
            help_hit(width, height, width - 2, 0),
            Some(HelpHit::Close)
        ));
        assert!(matches!(
            help_hit(width, height, 2, 0),
            Some(HelpHit::Tab(_))
        ));
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
