//! The shell's input side (§11.6): the keydown pump and the touch gesture pump,
//! both feeding the same one-input-at-a-time seam ([`Game::step_and_draw`]).
//!
//! The shell never interprets a key — the §11.6 bindings live in
//! `core::input_for_key` / `core::ui_command_for_key` / `core::ability_slot_for_code`
//! / `core::ability_slot_for_letter`, pinned by native tests. What lives *here* is the plumbing the core cannot own:
//! browser listeners, the gesture's live state, and the repeat timers — plus the
//! *order* those tables are consulted in ([`play_key`]), which is the shell's alone
//! because only the shell holds both halves of the event. Every pure rule of this
//! module is natively tested below like any core table.
//!
//! **The touch model** (replacing the old edge-zone tap slice): a **swipe**
//! steps along the drag's dominant axis and *keeps* stepping while the finger
//! stays down, the direction re-read live from the drag; a **press held in
//! place** waits, repeatedly; a **quick tap** is a single Wait. Lifting the
//! finger stops everything instantly — fairness (§2.2/§4.5) demands no step or
//! wait ever lands after the lift, and every repeat is one ordinary [`Input`]
//! through the same seam as a held arrow key, never a batch.
//!
//! **Where** the finger is decides whether the ambiguous half of that model is
//! allowed at all: the Wait-producing gestures resolve through
//! [`Game::tap_at`](crate::tap) (§11.6/#306), so a tap or a held press that landed on
//! the chrome, in its dead band, or off the canvas does nothing instead of silently
//! spending a turn. Swipes are exempt — a directional drag is unambiguous.

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{
    ability_in_slot, ability_slot_for_code, ability_slot_for_letter, help_nav_for_key,
    input_for_key, key_for_code, menu_nav_for_key, ui_command_for_key, Cell, Direction, HelpHit,
    HelpNav, Input, InputModality, UiCommand, BOTTOM_ROWS, TOP_ROWS,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, KeyboardEvent, PointerEvent};

use crate::tap::{Control, Tap};
use crate::Game;

/// The input-facing half of [`Game`]: how a key or a gesture tick becomes a
/// turn. The rendering half (fit, paint) stays in `lib.rs`, the colour table in
/// [`palette`](crate::palette).
impl Game {
    /// Map a keypress through the core's §11.6 tables and, if it is one the loop
    /// takes, step and redraw. Returns whether the key was consumed (so the caller can
    /// stop the page from scrolling on the arrows). Every mapping lives in
    /// `core::input`, where native tests pin it — this shell never interprets a key.
    ///
    /// It takes **both** halves of the browser's event: the `key` character the layout
    /// produced, and the physical `code` under the finger. Most bindings are on the
    /// character, but the digits bind by position (#359) — the ability bar's `1`–`4`
    /// straight off `Digit1`–`Digit4`, and the numpad folded onto the arrows by
    /// `key_for_code` before any character table is consulted — so an AZERTY or
    /// Dvorak player presses the same physical keys as a QWERTY one. The abilities'
    /// mnemonic letters (#360) go the other way, on the character, because there the
    /// binding is the letter the bar is showing.
    fn handle_key(&mut self, key: &str, code: &str, is_repeat: bool) -> bool {
        // The numpad's meaning is its position, not the character the layout put on
        // it, so it is folded to the §11.6 key it duplicates — the arrows, and `w` for
        // wait — once, here, ahead of every table below (movement, the help panel's
        // tabs, the menu's list), so they cannot drift apart on what the numpad takes.
        // It folds onto the arrows rather than onto `8` `2` `4` `6` deliberately
        // (#369): those characters are the top row's too, and the top row is the bar's.
        let key = key_for_code(code).unwrap_or(key);
        // Before a run starts, the menu owns the keyboard (§14/#268): it is modal in
        // the strongest sense — there is no world to step underneath it. Everything
        // the game would claim is swallowed; a genuinely unowned key (F5, a browser
        // shortcut) is still left to the page.
        if self.ui.menu.is_some() {
            if let Some(nav) = menu_nav_for_key(key) {
                self.apply_menu_nav(nav);
                return true;
            }
            return ui_command_for_key(key).is_some() || self.game_claims_key(key, code);
        }
        // While the help panel is open it is **modal** (§14 v2/#248): it captures
        // input, so keys route to help navigation first and the world never steps
        // underneath. `?`/Esc close it, Tab/←→ switch tabs. A key the panel does not
        // navigate by is *swallowed* if the game would otherwise own it (a move, an
        // ability, a UI toggle) — keeping the world frozen — but a genuinely unowned
        // key (F5, a browser shortcut) is still left to the page, as it is in play.
        if self.ui.help_open {
            if let Some(nav) = help_nav_for_key(key) {
                self.apply_help_nav(nav);
                self.draw();
                return true;
            }
            return ui_command_for_key(key).is_some() || self.game_claims_key(key, code);
        }
        // UI commands (§11.4) come next: they toggle view state and redraw without
        // ever touching the turn loop. `m` deploys the message list; `?` opens help.
        if let Some(command) = ui_command_for_key(key) {
            self.apply_ui_command(command);
            self.draw();
            return true;
        }
        // A game action (movement/wait) or an ability key (§11.6): both resolve in the
        // core and drive the one turn seam. An ability has **two** keys, and they
        // differ in what binds them: the digit names a bar slot by *position*, off the
        // physical code (`ability_slot_for_code`, #359), and the mnemonic names one by
        // the *letter* the bar is highlighting, off the character the layout produced
        // (`ability_slot_for_letter`, #360) — a position binds by position, a letter by
        // letter. Both land on a slot, the core turns that slot into the ability drawn
        // there (`ability_in_slot`) and then into this turn's input
        // (`State::ability_input`) — a **toggle**, so the key that switched the ability
        // on switches it off again (§4.4/#304). A tap on the bar entry goes through the
        // same calls from `ability_at` on, so neither key can disagree with the entry
        // above it.
        let input = match play_key(key, code, |letter| {
            ability_slot_for_letter(&self.state, letter)
        }) {
            Some(PlayKey::Move(input)) => input,
            Some(PlayKey::Slot(slot)) => {
                // A **held** ability key is swallowed (§11.6/#304): now that the key is
                // a toggle, letting the browser's auto-repeat through would switch the
                // ability straight back off a frame after switching it on. Toggling
                // takes a deliberate press, in both directions — and the repeat was a
                // free no-op before this, so nothing is lost. Consumed, so the page is
                // still.
                if is_repeat {
                    return true;
                }
                // A digit past the run's held count fires nothing: no turn, no state
                // change (§11.6 — a miss is free). Still consumed, because the four
                // digits are the game's whether or not this run filled them, and a `4`
                // that scrolled the page on a three-ability run would be worse than one
                // that does nothing. (A mnemonic never lands here — a letter no entry
                // claimed resolves to no slot at all, and stays the page's.)
                let Some(id) = ability_in_slot(&self.state, slot) else {
                    return true;
                };
                self.state.ability_input(id)
            }
            None => return false,
        };
        // A keyboard **auto-repeat** (`KeyboardEvent.repeat`) that would walk the
        // player into visible danger is swallowed here (§11.6/#223): the deliberate
        // first press (`is_repeat` false) always lands, but its held repeat stops at
        // the edge of a seen guard's cone — going deeper takes a fresh press. Still
        // consumed (returns `true`) so the page never scrolls on the swallowed arrow.
        if is_repeat && self.repeat_into_danger(input) {
            return true;
        }
        self.step_and_draw(input);
        true
    }

    /// Whether the game would claim this press *in play*, used by the modal screens
    /// to decide what to swallow. Asking [`play_key`] rather than a list of tables is
    /// what keeps "the menu swallows everything the game would take" true as those
    /// tables move.
    fn game_claims_key(&self, key: &str, code: &str) -> bool {
        play_key(key, code, |letter| {
            ability_slot_for_letter(&self.state, letter)
        })
        .is_some()
    }

    /// Feed one [`Input`] to the loop and repaint — the single seam every input
    /// source (a key, a gesture tick) drives, one ordinary input at a time against
    /// the current frame's state (§2.2 fairness: never a batched multi-step).
    pub(crate) fn step_and_draw(&mut self, input: Input) {
        self.state.step(input);
        self.draw();
    }

    /// Whether a held movement's *repeat* of `input` must be suppressed this tick
    /// because continuing the hold would carry the player into — or deeper through —
    /// visible danger (§11.5/§11.6, #223). Reads the core's own overlay set
    /// ([`State::in_visible_danger`](intrusion_core::State::in_visible_danger)) so
    /// the shell never recomputes detection; the pure rule is [`repeat_suppressed`],
    /// unit-tested natively below. Called for repeats only — a fresh press never
    /// routes here, so a single deliberate step into a cone is always allowed.
    fn repeat_into_danger(&self, input: Input) -> bool {
        let player = self.state.player();
        repeat_suppressed(player, input, |cell| self.state.in_visible_danger(cell))
    }

    /// Note that the player just used `modality`, and repaint if that is news
    /// (§11.6/#323). The only thing it changes is how the usable line's floor words
    /// move and wait — so the hint follows what the player's hands are *doing*, not
    /// what the device could theoretically do: a laptop with a touchscreen is a
    /// keyboard session until a finger lands on it, and a tablet with a keyboard
    /// attached is the reverse.
    ///
    /// The redraw is the point — the row is only ever read between turns, so a
    /// modality that changed without one would otherwise keep teaching the wrong
    /// vocabulary until the next step. It costs a paint on the rare frame the answer
    /// actually flips, and nothing at all on every other input.
    pub(crate) fn note_modality(&mut self, modality: InputModality) {
        if self.ui.modality != modality {
            self.ui.modality = modality;
            self.draw();
        }
    }

    /// Apply a shell-level [`UiCommand`] (§11.4) — a view toggle, never a game
    /// action, so it changes no [`State`](intrusion_core::State).
    pub(crate) fn apply_ui_command(&mut self, command: UiCommand) {
        match command {
            UiCommand::ToggleMessageLog => {
                self.ui.message_log_open = !self.ui.message_log_open;
            }
            UiCommand::ToggleHelp => {
                self.ui.help_open = !self.ui.help_open;
            }
            // The shell holds *both* colour tables and the core holds the flag
            // (§11.2/#189), so switching theme is this one line here and a column of
            // hex in [`palette`](crate::palette) — no game system learns a colour.
            UiCommand::ToggleTheme => {
                self.ui.theme = self.ui.theme.toggled();
            }
        }
    }

    /// Apply a [`HelpNav`] from the open modal panel (§14 v2/#248) — close it, or
    /// cycle the shown tab. Still a pure view action: no [`State`], no turn (§4.4).
    fn apply_help_nav(&mut self, nav: HelpNav) {
        match nav {
            HelpNav::Close => self.ui.help_open = false,
            HelpNav::NextTab => self.ui.help_tab = self.ui.help_tab.next(),
            HelpNav::PrevTab => self.ui.help_tab = self.ui.help_tab.prev(),
            HelpNav::ToggleTheme => self.apply_ui_command(UiCommand::ToggleTheme),
        }
    }

    /// Apply a [`HelpHit`] from a tap on the open panel: switch to the tapped tab or
    /// close. A view action like [`apply_help_nav`](Self::apply_help_nav).
    pub(crate) fn apply_help_hit(&mut self, hit: HelpHit) {
        match hit {
            HelpHit::Close => self.ui.help_open = false,
            HelpHit::Tab(tab) => self.ui.help_tab = tab,
            HelpHit::ToggleTheme => self.apply_ui_command(UiCommand::ToggleTheme),
        }
    }

    /// Map a viewport point `(client_x, client_y)` to the **screen cell** under it at
    /// the current fit, or `None` for a point off the canvas (a letterbox tap). The
    /// screen is `map + TOP_ROWS + BOTTOM_ROWS` rows fitted to the canvas, so a
    /// linear scale from the canvas rect gives the `(col, row)` the core drew — the
    /// one place the shell turns pixels into a grid coordinate, shared by every
    /// pointer hit-test so they can never disagree.
    pub(crate) fn screen_cell(&self, client_x: f64, client_y: f64) -> Option<(u32, u32)> {
        let rect = self.canvas.get_bounding_client_rect();
        let (rw, rh) = (rect.width(), rect.height());
        if !(rw > 0.0 && rh > 0.0) {
            return None;
        }
        let (lx, ly) = (client_x - rect.left(), client_y - rect.top());
        if lx < 0.0 || ly < 0.0 || lx >= rw || ly >= rh {
            return None; // outside the canvas (a letterbox tap)
        }
        let cols = self.state.layout().facility().width();
        let rows = self.screen_height();
        let col = (lx / rw * cols as f64).floor() as u32;
        let row = (ly / rh * rows as f64).floor() as u32;
        Some((col, row))
    }

    /// The screen's height in rows: the map plus the §11.4 status lines above it and
    /// the ability bar beneath. The one arithmetic every hit-test and the menu share,
    /// so none of them can disagree with the frame the core drew.
    pub(crate) fn screen_height(&self) -> u32 {
        self.state.layout().facility().height() + TOP_ROWS + BOTTOM_ROWS
    }
}

/// What a keypress means to the **running game** (§11.6): a bar slot to fire, or an
/// [`Input`] for the turn loop. Resolved by [`play_key`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PlayKey {
    /// An ability-bar slot, counting from `0` at the bar's leftmost drawn entry —
    /// which ability sits there is live state, so the rule stops at the slot.
    Slot(usize),
    /// A movement or wait, straight into the turn loop.
    Move(Input),
}

/// Resolve a keypress against §11.6's tables, **positions first** — the pure rule
/// behind #369, in the spirit of [`gesture_input`] and so natively tested below.
///
/// `key` is the character the layout produced, already folded through
/// `core::key_for_code` so a numpad key arrives as the arrow it means; `code` is the
/// physical key. `slot_for_letter` is the run's mnemonic lookup
/// (`core::ability_slot_for_letter`, #360) — a closure because *which* letters the
/// bar claims is a fact about the live loadout, and this rule is not.
///
/// The order is the fix. The bar's **digit** is asked first, off the code (#359),
/// because it names a *position* and a character table cannot see which of the two
/// digit blocks was pressed: consulting movement first is what made a top-row `2`
/// step south instead of firing slot 2, spending the turn and moving the player in
/// the bargain (§2.2). Then the **character** tables: movement and wait. Then the
/// **mnemonic letter** last, so a letter can never shadow a movement key even if the
/// mnemonic scheme's own reservation rule (`core::mnemonic`) were to change.
fn play_key(
    key: &str,
    code: &str,
    slot_for_letter: impl FnOnce(&str) -> Option<usize>,
) -> Option<PlayKey> {
    if let Some(slot) = ability_slot_for_code(code) {
        return Some(PlayKey::Slot(slot));
    }
    if let Some(input) = input_for_key(key) {
        return Some(PlayKey::Move(input));
    }
    slot_for_letter(key).map(PlayKey::Slot)
}

/// Install the keydown pump: each keypress drives one [`Game::handle_key`]. The
/// closure owns a clone of the `Rc` so the game outlives `start`; `forget` hands it to
/// the browser for the page's lifetime (the shell never tears down).
pub(crate) fn install_input(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let game = game.clone();
    let cb = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
        let mut game = game.borrow_mut();
        // A key — whatever it turns out to mean — says the player is on a keyboard
        // (§11.6/#323), including one they just picked up mid-session.
        game.note_modality(InputModality::Keys);
        // `e.repeat()` is the browser's own held-key auto-repeat flag (§11.6): the
        // first keydown is fresh, every held-down repeat after it carries `repeat ==
        // true`. The shell forwards it so the core rule (#223) can treat a held
        // repeat differently from a deliberate press without the pump interpreting.
        //
        // `e.code()` rides along beside `e.key()` because some bindings are physical
        // (#359): `key` is what the layout printed, `code` is which key it was, and
        // the core decides which of the two each binding reads.
        if game.handle_key(&e.key(), &e.code(), e.repeat()) {
            e.prevent_default();
        }
    });
    document.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())?;
    cb.forget();
    Ok(())
}

/// What a press of this `PointerEvent.pointerType` says about the player's hands
/// (§11.6/#323), or `None` when it says nothing: a **finger or a pen** is a touch
/// session outright, and a **mouse** is left alone deliberately — a click is
/// neither of the gestures the touch hint teaches, and a desktop player who reaches
/// for the ability bar with the pointer has not stopped being a keyboard player.
/// An unknown `pointerType` is treated like the mouse: no claim.
///
/// Pure, so the rule is pinned natively below like [`gesture_input`].
fn modality_of_pointer(pointer_type: &str) -> Option<InputModality> {
    match pointer_type {
        "touch" | "pen" => Some(InputModality::Touch),
        _ => None,
    }
}

/// The modality a fresh load opens on (§11.6/#323), from the `pointer: coarse`
/// media query: what the *device*'s primary pointer is, before the player has
/// touched anything. It is only a seed — the first key or finger corrects it
/// ([`Game::note_modality`]) — which is what makes the query's known weakness
/// harmless here: it answers for the device, and a hybrid device's answer is a
/// guess either way. A browser without `matchMedia` gets [`InputModality::Keys`],
/// the same default the core has.
pub(crate) fn boot_modality() -> InputModality {
    let coarse = web_sys::window()
        .and_then(|w| w.match_media("(pointer: coarse)").ok().flatten())
        .is_some_and(|q| q.matches());
    if coarse {
        InputModality::Touch
    } else {
        InputModality::Keys
    }
}

/// How far a drag must travel from its press point — CSS pixels, on either axis —
/// before it reads as a **swipe** rather than a press held in place. Roughly half
/// a fingertip: short enough that a flick registers, long enough that the jitter
/// of a resting finger never walks the player. Shared with the replay scrub pump
/// ([`crate::replay`]) so the touch feel of a swipe is one number across modes.
pub(crate) const SWIPE_THRESHOLD_PX: f64 = 24.0;

/// The pause between a gesture's first input and its first repeat — the touch
/// counterpart of the keyboard's auto-repeat delay (§11.6's reference cadence).
/// Long enough that one deliberate swipe or press stays a single input.
pub(crate) const REPEAT_DELAY_MS: i32 = 300;

/// The cadence of repeats while the finger stays down — one ordinary [`Input`]
/// per tick through the same seam as a held arrow key, never a batch (§4.1/§4.3).
pub(crate) const REPEAT_INTERVAL_MS: i32 = 120;

/// Map a drag displacement `(dx, dy)` — CSS pixels from where the finger went
/// down to where it is now — to the [`Input`] a gesture fires: the touch half of
/// §11.6, pure so the gesture rule is testable natively.
///
/// Inside [`SWIPE_THRESHOLD_PX`] on both axes the press is a **hold**: Wait.
/// Past it, the drag is a **swipe**: a `Step` along its dominant axis — movement
/// has no diagonals (§4.1 [SETTLED]) — with an exact tie going horizontal. The
/// pump re-reads the live displacement on every repeat tick, so dragging to a
/// new heading re-aims the walk mid-hold and pulling back inside the threshold
/// turns it into waiting; nothing is cached but the gesture's origin. A
/// non-finite displacement maps to nothing rather than a garbage turn.
fn gesture_input(dx: f64, dy: f64) -> Option<Input> {
    if !(dx.is_finite() && dy.is_finite()) {
        return None;
    }
    if dx.abs() < SWIPE_THRESHOLD_PX && dy.abs() < SWIPE_THRESHOLD_PX {
        return Some(Input::Wait);
    }
    let direction = if dx.abs() >= dy.abs() {
        if dx < 0.0 {
            Direction::West
        } else {
            Direction::East
        }
    } else if dy < 0.0 {
        Direction::North
    } else {
        Direction::South
    };
    Some(Input::Step(direction))
}

/// Whether a held movement's **repeat** should be suppressed this tick — the pure
/// §11.6 rule behind #223, in the spirit of [`gesture_input`]. Given the player's
/// cell, the repeat's [`Input`], and a membership test for the §11.5 danger set
/// (`in_danger`, wired to
/// [`State::in_visible_danger`](intrusion_core::State::in_visible_danger)), it says
/// whether this repeat would walk the player into visible danger and so must not
/// fire.
///
/// Only a `Step` is ever gated, and only when it touches the danger set: its
/// destination cell is watched ("a step would move you in"), or the player already
/// stands in one ("you have just entered one" — the deliberate first step landed on
/// the cone edge). A held press-in-place is `Wait` and must keep waiting (§11.6), so
/// it is never gated, and a repeat on safe ground fires normally. Fresh presses
/// never reach here — the caller routes only repeats — so a single deliberate step
/// into a cone is always allowed.
fn repeat_suppressed(player: Cell, input: Input, in_danger: impl Fn(Cell) -> bool) -> bool {
    let Input::Step(direction) = input else {
        return false;
    };
    in_danger(player) || player.step(direction).is_some_and(in_danger)
}

/// The browser timer currently driving a gesture's repeats: the one-shot initial
/// delay (`setTimeout`) or the steady cadence (`setInterval`). Whichever is
/// armed, release clears it by id — that clear is what guarantees no step or
/// wait ever fires after the finger lifts (§2.2/§4.5 fairness). Shared with the
/// replay scrub pump ([`crate::replay`]), which owns the same lift-stops-instantly
/// contract on the time cursor.
#[derive(Clone, Copy)]
pub(crate) enum RepeatTimer {
    Delay(i32),
    Interval(i32),
}

/// Clear an armed [`RepeatTimer`] with the browser. Clearing an id that already
/// fired is a harmless no-op, so teardown never has to know the timer's fate.
pub(crate) fn clear_timer(timer: RepeatTimer) {
    let win = web_sys::window().expect("a window");
    match timer {
        RepeatTimer::Delay(id) => win.clear_timeout_with_handle(id),
        RepeatTimer::Interval(id) => win.clear_interval_with_handle(id),
    }
}

/// What the pointer currently down is doing — decided once, at the press, by
/// [`Game::tap_at`] (§11.6/#306). The two are exclusive: a press on a control arms
/// **only** the control and never also a gesture, or a press dragged onto the board
/// would both abandon the button and walk the player.
enum Pointer {
    /// A press that landed on a chrome control: **armed**, and fired on the lift over
    /// that same control. Resolving on the lift rather than the press is what lets a
    /// mis-press be slid off and abandoned (§2.2/§4.5) — the same rule the gesture
    /// path already honours on `pointercancel` — and it puts both surfaces' resolution
    /// at the same moment, so they behave alike.
    Armed { pointer_id: i32, control: Control },
    /// A press on the board (or on anything the controls declined): the swipe / hold /
    /// tap gesture.
    Gesture(Gesture),
}

impl Pointer {
    /// The pointer that owns this press; other fingers are ignored while it lives.
    fn pointer_id(&self) -> i32 {
        match self {
            Pointer::Armed { pointer_id, .. } => *pointer_id,
            Pointer::Gesture(g) => g.pointer_id,
        }
    }
}

/// What a lift resolved to — decided while the pump's state is borrowed, applied
/// after it is released.
enum Lift {
    /// The armed control, to fire only if the lift is still over it.
    Control(Control),
    /// The unfired gesture's own input: a tap's Wait, or a flick too fast for a
    /// pointermove to have seen.
    Gesture(Input),
    /// Nothing to apply — a gesture that already fired, or an abandoned press.
    Nothing,
}

/// One finger's live gesture: where it pressed, where it is now, and the timer
/// keeping it repeating. Exists only while that pointer is down — release (or a
/// browser cancel) destroys it and its timer together.
struct Gesture {
    /// The pointer that owns the gesture; other fingers are ignored while it lives.
    pointer_id: i32,
    /// Where the pointer went down, in viewport CSS pixels.
    origin: (f64, f64),
    /// Live displacement from `origin`, updated on every pointermove. Each repeat
    /// tick re-reads it through [`gesture_input`], so the heading is never stale.
    delta: (f64, f64),
    /// Whether the gesture has produced its first input yet — the threshold-crossing
    /// step of a swipe, or the first Wait of a matured hold. A release before either
    /// makes the gesture a tap, resolved at the lift.
    fired: bool,
    /// The armed repeat timer, cleared the moment the gesture ends.
    timer: RepeatTimer,
}

impl Gesture {
    /// Where the finger is **now**, in viewport CSS pixels — the origin plus the live
    /// displacement. The Wait-producing gestures are routed against this rather than
    /// the origin (#306), so it is the same point a lift would resolve at: drag out of
    /// the dead band and a held press starts waiting, drag into it and it stops.
    fn point(&self) -> (f64, f64) {
        (self.origin.0 + self.delta.0, self.origin.1 + self.delta.1)
    }
}

/// The gesture pump — §11.6's touch half, replacing the old edge-zone tap model.
///
/// A **swipe** steps along the drag's dominant axis the instant it crosses
/// [`SWIPE_THRESHOLD_PX`], and *keeps* stepping while the finger stays down. A
/// **press held in place** matures into Wait after [`REPEAT_DELAY_MS`], and keeps
/// waiting. A **quick tap** (released before either) is a single Wait, resolved
/// at the lift — the gesture's own input, not a repeat. After a gesture's first
/// input, the next comes [`REPEAT_DELAY_MS`] later for a swipe (a matured hold is
/// already the delay timer firing), then every [`REPEAT_INTERVAL_MS`] — the held
/// arrow key's cadence (§11.6). Every tick re-reads the live displacement, so
/// dragging to a new heading re-aims the walk without lifting.
///
/// Fairness (§2.2/§4.5): each tick feeds exactly one ordinary [`Input`] through
/// [`Game::step_and_draw`] against the current frame — never queued ahead — and
/// release/cancel clears the timer before anything else can fire, so no step or
/// wait ever lands after the finger lifts. A cancelled gesture (the browser took
/// the pointer, or it left the page) emits nothing at all, not even the tap's
/// Wait — a turn must never burn on a gesture the player didn't finish. **A tap the
/// player aimed at a button is such a gesture** (#306), which is why every
/// Wait-producing resolution here is routed through [`Game::tap_at`] and every chrome
/// control resolves on the lift.
struct GesturePump {
    game: Rc<RefCell<Game>>,
    /// The live press, if a finger is down: an armed control or a gesture.
    active: RefCell<Option<Pointer>>,
    /// The repeat tick — **one closure for the page's lifetime**, registered with
    /// `setTimeout`/`setInterval` afresh for each gesture. Storing it here (an Rc
    /// cycle, deliberately never freed) mirrors the `Closure::forget` lifetime
    /// pattern of the listeners below without leaking a closure per gesture.
    tick: RefCell<Option<Closure<dyn FnMut()>>>,
}

impl GesturePump {
    /// Arm the repeat tick with the browser — the one-shot initial delay or the
    /// steady interval — and hand back the id for the gesture to own.
    fn arm(&self, ms: i32, as_interval: bool) -> i32 {
        let win = web_sys::window().expect("a window");
        let tick = self.tick.borrow();
        let f = tick
            .as_ref()
            .expect("the tick closure is installed at boot")
            .as_ref()
            .unchecked_ref();
        if as_interval {
            win.set_interval_with_callback_and_timeout_and_arguments_0(f, ms)
        } else {
            win.set_timeout_with_callback_and_timeout_and_arguments_0(f, ms)
        }
        .expect("the browser arms a timer")
    }

    /// A pointer pressed: route the point once (§11.6/#306) and either **arm** the
    /// control under it — nothing fires yet — or start the gesture. Only the primary
    /// button presses, and a second finger neither starts a second press nor re-aims
    /// the first.
    fn on_down(&self, e: &PointerEvent) {
        if e.button() != 0 {
            return; // secondary mouse buttons keep their browser meaning
        }
        // A finger on the glass says the player is on touch (§11.6/#323) — noted
        // before the press resolves, so the hint is already in the gesture
        // vocabulary on the frame this press draws.
        if let Some(modality) = modality_of_pointer(&e.pointer_type()) {
            self.game.borrow_mut().note_modality(modality);
        }
        let (x, y) = (e.client_x() as f64, e.client_y() as f64);
        let tap = self.game.borrow().tap_at(x, y);
        {
            let mut active = self.active.borrow_mut();
            if active.is_none() {
                *active = match tap {
                    // A control — the menu's entries, the help panel's tabs and `[x]`,
                    // the `[?]` toggle, the message counter, an ability slot. Armed
                    // only: it fires on the lift over the same control, and it starts
                    // no gesture.
                    Tap::Control(control) => Some(Pointer::Armed {
                        pointer_id: e.pointer_id(),
                        control,
                    }),
                    // A modal screen captured the press (§14/#268, §14 v2/#248): not
                    // even a gesture starts, since there is no world underneath to
                    // walk. (The seed box's own presses never reach here; its panel
                    // stops them at itself.)
                    Tap::Captured => None,
                    // Anything else starts a gesture. It may begin *anywhere* the
                    // controls declined — the dead band and the chrome included —
                    // because a swipe from there is unambiguous and must still step;
                    // it is only the Wait that the routing gates, at the moment the
                    // gesture resolves.
                    Tap::Wait | Tap::Nothing => Some(Pointer::Gesture(Gesture {
                        pointer_id: e.pointer_id(),
                        origin: (x, y),
                        delta: (0.0, 0.0),
                        fired: false,
                        timer: RepeatTimer::Delay(self.arm(REPEAT_DELAY_MS, false)),
                    })),
                };
            }
        }
        // Consumed either way (§11.6): gestures are game input, and the browser's
        // follow-ups (double-tap zoom, synthetic clicks) must not fire off them.
        e.prevent_default();
    }

    /// The gesture's pointer moved: track the live displacement, and the instant
    /// the drag first crosses the swipe threshold fire its step — the swipe
    /// declaring itself — restarting the repeat cadence from that input exactly
    /// as a fresh keydown would.
    ///
    /// An armed control ignores moves entirely: the lift re-routes the point, so
    /// sliding off and back on again is decided once, at the end.
    fn on_move(&self, e: &PointerEvent) {
        let first_step = {
            let mut active = self.active.borrow_mut();
            let Some(Pointer::Gesture(g)) =
                active.as_mut().filter(|p| p.pointer_id() == e.pointer_id())
            else {
                return;
            };
            g.delta = (
                e.client_x() as f64 - g.origin.0,
                e.client_y() as f64 - g.origin.1,
            );
            let input = gesture_input(g.delta.0, g.delta.1);
            if !g.fired && matches!(input, Some(Input::Step(_))) {
                g.fired = true;
                clear_timer(g.timer);
                g.timer = RepeatTimer::Delay(self.arm(REPEAT_DELAY_MS, false));
                input
            } else {
                None
            }
        };
        if let Some(input) = first_step {
            self.game.borrow_mut().step_and_draw(input);
        }
    }

    /// The armed timer fired: feed one input re-read from the live displacement —
    /// a hold's Wait, a swipe's step, whichever the finger says *now* — and, if
    /// this was the one-shot delay, settle into the steady cadence.
    fn on_tick(&self) {
        let tick = {
            let mut active = self.active.borrow_mut();
            let Some(Pointer::Gesture(g)) = active.as_mut() else {
                return; // released while the tick was in flight — nothing may fire
            };
            g.fired = true;
            if let RepeatTimer::Delay(_) = g.timer {
                g.timer = RepeatTimer::Interval(self.arm(REPEAT_INTERVAL_MS, true));
            }
            gesture_input(g.delta.0, g.delta.1).map(|input| (input, g.point()))
        };
        if let Some((input, (x, y))) = tick {
            let mut game = self.game.borrow_mut();
            // A held swipe never auto-walks into visible danger (§11.6/#223): the
            // repeat is swallowed at the cone edge, the cadence left running so
            // dragging to a safe heading fires again — but going deeper needs a
            // fresh gesture. A held Wait (press-in-place) is never gated and keeps
            // waiting.
            if game.repeat_into_danger(input) {
                return;
            }
            // A press held **in place** only waits where a tap would (§11.6/#306): a
            // resting finger on the chrome, in the dead band or off the canvas is more
            // likely a missed button than a deliberate hold, so it produces nothing.
            // The cadence is left running — drag out onto clear board and it waits.
            if !gesture_lands(input, game.tap_at(x, y)) {
                return;
            }
            game.step_and_draw(input);
        }
    }

    /// The pointer lifted — **the one moment both surfaces resolve at** (§11.6/#306).
    ///
    /// An armed control fires only if the lift still lands on it, so a press slid off
    /// its cells (or onto a different control) is abandoned, spending nothing. A
    /// gesture stops every repeat immediately and, if it never fired, resolves as the
    /// tap it was — at the lift point, so a press in place is one Wait and a flick too
    /// fast for a pointermove still steps. That input is the gesture's own, not a
    /// repeat leaking past the lift.
    fn on_up(&self, e: &PointerEvent) {
        let (x, y) = (e.client_x() as f64, e.client_y() as f64);
        let lift = {
            let mut active = self.active.borrow_mut();
            if !matches!(active.as_ref(), Some(p) if p.pointer_id() == e.pointer_id()) {
                return;
            }
            match active.take().expect("matched just above") {
                Pointer::Armed { control, .. } => Lift::Control(control),
                Pointer::Gesture(g) => {
                    clear_timer(g.timer);
                    match gesture_input(x - g.origin.0, y - g.origin.1) {
                        Some(input) if !g.fired => Lift::Gesture(input),
                        _ => Lift::Nothing,
                    }
                }
            }
        };
        e.prevent_default();
        match lift {
            Lift::Nothing => {}
            Lift::Control(armed) => {
                let mut game = self.game.borrow_mut();
                if armed_fires(armed, game.tap_at(x, y)) {
                    game.apply_control(armed);
                }
            }
            Lift::Gesture(input) => {
                let mut game = self.game.borrow_mut();
                // The tap's Wait is the ambiguous gesture this ticket gates: it lands
                // only on board clear of the chrome's dead band (#306). A flick's
                // `Step` is unambiguous and lands wherever it was aimed.
                if gesture_lands(input, game.tap_at(x, y)) {
                    game.step_and_draw(input);
                }
            }
        }
    }

    /// The browser took the press away (`pointercancel`) or the pointer left the
    /// page (`pointerleave`): tear down without emitting anything — not even the
    /// tap's Wait, nor an armed control. A turn must never burn on a gesture the
    /// player didn't end.
    fn on_abort(&self, e: &PointerEvent) {
        let mut active = self.active.borrow_mut();
        if !matches!(active.as_ref(), Some(p) if p.pointer_id() == e.pointer_id()) {
            return;
        }
        if let Some(Pointer::Gesture(g)) = active.take() {
            clear_timer(g.timer);
        }
    }
}

/// Whether the `input` a gesture resolved to may land where the finger is (§11.6/#306)
/// — the pure half of the dead band, in the spirit of [`gesture_input`].
///
/// A `Wait` is the **ambiguous** gesture: zero displacement says nothing about what
/// the player meant, so it lands only on [`Tap::Wait`] — board clear of the chrome and
/// its dead band. Every other input is unconditional: **swipes are exempt**, because a
/// directional drag is unambiguous wherever it started, so the band costs no movement.
fn gesture_lands(input: Input, tap: Tap) -> bool {
    input != Input::Wait || tap == Tap::Wait
}

/// Whether an **armed** control fires on the lift: only when the lift still routes to
/// that same control (§11.6/#306). Sliding off its cells — onto the board, onto a
/// neighbouring control, into the letterbox — abandons it, spending nothing, the same
/// rule the gesture path already honours on `pointercancel` (§2.2/§4.5).
fn armed_fires(armed: Control, tap: Tap) -> bool {
    tap == Tap::Control(armed)
}

/// Install the gesture pump (§11.6's touch half): pointer listeners anywhere on
/// the page — the letterbox margins count too — feed one [`GesturePump`], which
/// owns the repeat timer and the live gesture. `preventDefault` on the consumed
/// press stops the browser's gesture follow-ups (double-tap zoom, synthetic mouse
/// events); `touch-action: none` on the page covers the rest (see `web/index.html`).
/// Each listener closure is `forget`ed for the page's lifetime, like the key pump.
pub(crate) fn install_gestures(
    document: &Document,
    game: &Rc<RefCell<Game>>,
) -> Result<(), JsValue> {
    let pump = Rc::new(GesturePump {
        game: game.clone(),
        active: RefCell::new(None),
        tick: RefCell::new(None),
    });
    let p = pump.clone();
    *pump.tick.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || p.on_tick()));

    type Handler = fn(&GesturePump, &PointerEvent);
    let listeners: [(&str, Handler); 5] = [
        ("pointerdown", GesturePump::on_down),
        ("pointermove", GesturePump::on_move),
        ("pointerup", GesturePump::on_up),
        ("pointercancel", GesturePump::on_abort),
        ("pointerleave", GesturePump::on_abort),
    ];
    for (event, handler) in listeners {
        let p = pump.clone();
        let cb = Closure::<dyn FnMut(PointerEvent)>::new(move |e: PointerEvent| handler(&p, &e));
        document.add_event_listener_with_callback(event, cb.as_ref().unchecked_ref())?;
        cb.forget();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use intrusion_core::AbilityId;

    /// #369, the reported bug, at the seam it lived on: a **top-row** `2` fires the
    /// bar's second slot. The press arrives as `code: "Digit2"`, `key: "2"` — the
    /// character the movement table used to answer first, stepping the player south
    /// and spending the turn instead of firing the ability. All four digits, since
    /// slots 1 and 3 worked by luck (nothing claimed `1` or `3`) and that is exactly
    /// what hid the bug.
    #[test]
    fn a_top_row_digit_fires_its_bar_slot_and_never_steps() {
        for (code, key, slot) in [
            ("Digit1", "1", 0),
            ("Digit2", "2", 1),
            ("Digit3", "3", 2),
            ("Digit4", "4", 3),
        ] {
            assert_eq!(
                play_key(key, code, |_| None),
                Some(PlayKey::Slot(slot)),
                "{code} fires slot {slot}",
            );
        }
    }

    /// …and the other half of the same split: the **numpad** still moves. It arrives
    /// with its own codes and is folded to the arrows before this rule sees it, so
    /// `Numpad2` steps south where `Digit2` fires a slot — the two digit blocks kept
    /// apart by the only thing that can tell them apart, the code.
    #[test]
    fn the_numpad_still_steps_and_waits() {
        for (code, expected) in [
            ("Numpad8", Input::Step(Direction::North)),
            ("Numpad2", Input::Step(Direction::South)),
            ("Numpad4", Input::Step(Direction::West)),
            ("Numpad6", Input::Step(Direction::East)),
            ("Numpad5", Input::Wait),
        ] {
            let key = key_for_code(code).expect("the numpad folds");
            assert_eq!(
                play_key(key, code, |_| None),
                Some(PlayKey::Move(expected)),
                "{code}",
            );
        }
    }

    /// The precedence in full (§11.6): a **position** outranks every character table,
    /// then movement, then the run's **mnemonic letter** last — so a letter can never
    /// shadow a step even if the mnemonic scheme stopped reserving the movement keys.
    /// A key no table owns is left to the page.
    #[test]
    fn play_resolves_position_then_movement_then_mnemonic() {
        // A mnemonic lookup greedy enough to claim anything it is offered: it still
        // never sees a digit or a movement key, because both are answered above it.
        let greedy = |_: &str| Some(7);
        assert_eq!(
            play_key("2", "Digit2", greedy),
            Some(PlayKey::Slot(1)),
            "the bar's digit outranks the mnemonic",
        );
        assert_eq!(
            play_key("h", "KeyH", greedy),
            Some(PlayKey::Move(Input::Step(Direction::West))),
            "a movement key outranks the mnemonic",
        );
        assert_eq!(
            play_key("c", "KeyC", greedy),
            Some(PlayKey::Slot(7)),
            "a free letter reaches the mnemonic lookup",
        );
        for (key, code) in [("q", "KeyQ"), ("F5", "F5"), ("5", "Digit5")] {
            assert_eq!(
                play_key(key, code, |_| None),
                None,
                "{key:?} is left to the page",
            );
        }
    }

    /// §11.6's hold rule: a press that never crosses the swipe threshold is Wait —
    /// from the zero-displacement press up to the last sub-threshold pixel, on
    /// both axes and in every quadrant. The resting-finger jitter of a hold must
    /// never walk the player.
    #[test]
    fn a_press_inside_the_threshold_holds_to_wait() {
        let just_under = SWIPE_THRESHOLD_PX - 0.5;
        for (dx, dy) in [
            (0.0, 0.0),
            (just_under, 0.0),
            (0.0, -just_under),
            (-just_under, just_under),
            (just_under, just_under),
        ] {
            assert_eq!(
                gesture_input(dx, dy),
                Some(Input::Wait),
                "drag of ({dx}, {dy})"
            );
        }
    }

    /// A swipe resolves to the nearest cardinal: the dominant axis of the drag,
    /// in all four directions, including well off-axis drags — movement has no
    /// diagonals (§4.1).
    #[test]
    fn a_swipe_steps_its_dominant_axis() {
        for ((dx, dy), direction) in [
            ((-40.0, 10.0), Direction::West),
            ((40.0, -10.0), Direction::East),
            ((10.0, -40.0), Direction::North),
            ((-10.0, 40.0), Direction::South),
        ] {
            assert_eq!(
                gesture_input(dx, dy),
                Some(Input::Step(direction)),
                "drag of ({dx}, {dy})"
            );
        }
    }

    /// The threshold itself swipes — reaching it is crossing it — and an exact
    /// diagonal tie goes horizontal, the old tap model's convention kept.
    #[test]
    fn the_threshold_boundary_swipes_and_ties_go_horizontal() {
        let t = SWIPE_THRESHOLD_PX;
        assert_eq!(
            gesture_input(t, 0.0),
            Some(Input::Step(Direction::East)),
            "the boundary is a swipe"
        );
        assert_eq!(gesture_input(t, t), Some(Input::Step(Direction::East)));
        assert_eq!(gesture_input(-t, -t), Some(Input::Step(Direction::West)));
    }

    /// The live re-evaluation contract: the function is pure in the displacement,
    /// so a repeat tick re-reading the drag changes heading with the finger — a
    /// swipe dragged to a new quadrant re-aims, and one pulled back inside the
    /// threshold becomes a hold. No direction is ever cached.
    #[test]
    fn a_dragging_finger_re_aims_the_repeat_live() {
        assert_eq!(gesture_input(40.0, 0.0), Some(Input::Step(Direction::East)));
        assert_eq!(
            gesture_input(6.0, -35.0),
            Some(Input::Step(Direction::North))
        );
        assert_eq!(gesture_input(3.0, -3.0), Some(Input::Wait));
    }

    /// A non-finite displacement maps to nothing rather than a garbage turn.
    #[test]
    fn a_non_finite_drag_is_ignored() {
        assert_eq!(gesture_input(f64::NAN, 0.0), None);
        assert_eq!(gesture_input(0.0, f64::NEG_INFINITY), None);
    }

    /// #223's core rule, pure and native. Standing on safe ground, a held `Step`
    /// repeat is suppressed once its **destination** is a danger cell — the hold
    /// halts at the cone edge — while a repeat away from the danger fires normally.
    #[test]
    fn a_step_repeat_into_a_cone_is_suppressed_at_the_edge() {
        // A single danger cell to the north of the player: the overlay set as a set.
        let danger = Cell::new(5, 4);
        let player = Cell::new(5, 5);
        let in_danger = |c: Cell| c == danger;
        // North steps *into* the cone — suppressed; the other three are clear.
        assert!(repeat_suppressed(
            player,
            Input::Step(Direction::North),
            in_danger
        ));
        for dir in [Direction::South, Direction::East, Direction::West] {
            assert!(
                !repeat_suppressed(player, Input::Step(dir), in_danger),
                "a {dir:?} repeat onto clear ground fires"
            );
        }
    }

    /// The "just entered" half: once the player *stands* in a danger cell (the
    /// deliberate first step having landed on the cone), every held repeat is
    /// suppressed — even one stepping back out — so escaping the cone is a fresh,
    /// deliberate press each time, never a blind march.
    #[test]
    fn a_step_repeat_while_standing_in_a_cone_is_suppressed_in_any_direction() {
        let player = Cell::new(5, 5);
        let in_danger = |c: Cell| c == player; // the player's own cell is watched
        for dir in Direction::ALL {
            assert!(
                repeat_suppressed(player, Input::Step(dir), in_danger),
                "in danger, a {dir:?} repeat stops"
            );
        }
    }

    /// A held press-in-place is `Wait`, and an activation repeat is `Activate` —
    /// neither is a step across the cone edge, so neither is ever gated. A held Wait
    /// must keep waiting (§11.6's hold model), even while standing in danger.
    #[test]
    fn only_step_repeats_are_gated() {
        let player = Cell::new(5, 5);
        let all_danger = |_: Cell| true; // the worst case: everything is watched
        assert!(!repeat_suppressed(player, Input::Wait, all_danger));
        assert!(!repeat_suppressed(
            player,
            Input::Activate(AbilityId::Run),
            all_danger
        ));
    }

    /// #306's dead band, from the gesture's side: **a swipe is exempt.** A `Step`
    /// lands wherever the finger is — the band, the chrome, off the canvas — because a
    /// directional drag is unambiguous, so the band costs no movement. Only the
    /// zero-displacement `Wait` is gated, and it lands on [`Tap::Wait`] alone.
    #[test]
    fn only_the_ambiguous_wait_is_gated_by_where_the_finger_is() {
        for tap in [Tap::Wait, Tap::Nothing, Tap::Captured] {
            assert!(
                gesture_lands(Input::Step(Direction::North), tap),
                "a swipe steps at {tap:?}"
            );
        }
        assert!(
            gesture_lands(Input::Wait, Tap::Wait),
            "board clear of the bars"
        );
        for tap in [
            Tap::Nothing, // the chrome, the dead band, the letterbox
            Tap::Captured,
            Tap::Control(Control::HelpToggle),
        ] {
            assert!(
                !gesture_lands(Input::Wait, tap),
                "a tap or a held press at {tap:?} spends no turn"
            );
        }
    }

    /// #306's lift rule: an armed control fires only if the lift is still over that
    /// same control. Sliding off onto the board, onto a *different* control, or into
    /// the letterbox abandons the press — nothing fires and nothing steps.
    #[test]
    fn an_armed_control_fires_only_where_it_was_armed() {
        let armed = Control::Ability(AbilityId::Run);
        assert!(armed_fires(armed, Tap::Control(armed)));
        for tap in [
            Tap::Control(Control::HelpToggle),
            Tap::Control(Control::Ability(AbilityId::Camouflage)),
            Tap::Wait,
            Tap::Nothing,
            Tap::Captured,
        ] {
            assert!(
                !armed_fires(armed, tap),
                "lifting at {tap:?} abandons the press"
            );
        }
    }

    /// §11.6/#323: a press only claims the touch modality when it is a **finger or a
    /// pen**. A mouse — and anything the browser will not name — leaves the hint's
    /// vocabulary alone, because a click is neither of the two gestures the touch
    /// hint teaches and a desktop player clicking the bar is still on the keyboard.
    #[test]
    fn only_a_finger_or_a_pen_claims_the_touch_modality() {
        assert_eq!(
            modality_of_pointer("touch"),
            Some(InputModality::Touch),
            "a finger is touch"
        );
        assert_eq!(
            modality_of_pointer("pen"),
            Some(InputModality::Touch),
            "so is a stylus"
        );
        for kind in ["mouse", "", "trackpad"] {
            assert_eq!(
                modality_of_pointer(kind),
                None,
                "a {kind:?} press claims nothing"
            );
        }
    }

    /// A step that would leave the grid's north/west edge has no destination cell
    /// ([`Cell::step`] is `None`): the repeat is judged on the player's own cell
    /// alone — safe there, it fires (a harmless bump); in danger there, it stops.
    #[test]
    fn a_step_off_the_grid_edge_is_judged_on_the_player_cell_alone() {
        let corner = Cell::new(0, 0);
        // Off-grid destination, player on clear ground: not suppressed.
        assert!(!repeat_suppressed(
            corner,
            Input::Step(Direction::North),
            |_| { false }
        ));
        // Off-grid destination, but the player already stands in danger: suppressed.
        assert!(repeat_suppressed(
            corner,
            Input::Step(Direction::West),
            |c| c == corner
        ));
    }
}
