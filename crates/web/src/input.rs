//! The shell's input side (§11.6): the keydown pump and the touch gesture pump,
//! both feeding the same one-input-at-a-time seam ([`Game::step_and_draw`]).
//!
//! The shell never interprets a key — the §11.6 bindings live in
//! `core::input_for_key` / `core::ui_command_for_key`, pinned by native tests.
//! What lives *here* is the plumbing the core cannot own: browser listeners,
//! the gesture's live state, and the repeat timers. The one pure rule of this
//! module, [`gesture_input`], is natively tested below like any core table.
//!
//! **The touch model** (replacing the old edge-zone tap slice): a **swipe**
//! steps along the drag's dominant axis and *keeps* stepping while the finger
//! stays down, the direction re-read live from the drag; a **press held in
//! place** waits, repeatedly; a **quick tap** is a single Wait. Lifting the
//! finger stops everything instantly — fairness (§2.2/§4.5) demands no step or
//! wait ever lands after the lift, and every repeat is one ordinary [`Input`]
//! through the same seam as a held arrow key, never a batch.

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{
    ability_at, ability_input_for_key, help_hit, help_nav_for_key, input_for_key,
    is_ability_button, is_help_button, is_message_button, ui_command_for_key, AbilityId, Cell,
    Direction, HelpHit, HelpNav, Input, UiCommand, BOTTOM_ROWS, TOP_ROWS,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, KeyboardEvent, PointerEvent};

use crate::Game;

/// The input-facing half of [`Game`]: how a key or a gesture tick becomes a
/// turn. The rendering half (fit, paint) stays in `lib.rs` beside the palette.
impl Game {
    /// Map a key through the core's §11.6 table and, if it is one the loop takes,
    /// step and redraw. Returns whether the key was consumed (so the caller can
    /// stop the page from scrolling on the arrows). The mapping itself lives in
    /// `core::input_for_key` where native tests pin every binding — this shell
    /// never interprets a key.
    fn handle_key(&mut self, key: &str, is_repeat: bool) -> bool {
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
            return ui_command_for_key(key).is_some()
                || input_for_key(key).is_some()
                || ability_input_for_key(key).is_some();
        }
        // UI commands (§11.4) come next: they toggle view state and redraw without
        // ever touching the turn loop. `Tab` deploys the ability panel; `?` opens help.
        if let Some(command) = ui_command_for_key(key) {
            self.apply_ui_command(command);
            self.draw();
            return true;
        }
        // A game action (movement/wait) or an ability shortcut (§11.6): both resolve
        // in the core and drive the one turn seam. An ability hotkey fires the same
        // `Input::Activate(id)` a click on the ability's row does — one activation
        // path, resolved by identity.
        let Some(input) = input_for_key(key).or_else(|| ability_input_for_key(key)) else {
            return false;
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

    /// Feed one [`Input`] to the loop and repaint — the single seam every input
    /// source (a key, a gesture tick) drives, one ordinary input at a time against
    /// the current frame's state (§2.2 fairness: never a batched multi-step).
    fn step_and_draw(&mut self, input: Input) {
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

    /// Apply a shell-level [`UiCommand`] (§11.4) — a view toggle, never a game
    /// action, so it changes no [`State`](intrusion_core::State).
    fn apply_ui_command(&mut self, command: UiCommand) {
        match command {
            UiCommand::ToggleAbilityPanel => {
                self.ui.ability_panel_open = !self.ui.ability_panel_open;
            }
            UiCommand::ToggleMessageLog => {
                self.ui.message_log_open = !self.ui.message_log_open;
            }
            UiCommand::ToggleHelp => {
                self.ui.help_open = !self.ui.help_open;
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
        }
    }

    /// The [`HelpHit`] under a viewport point while the panel is open, or `None`
    /// (§11.6/#248) — the core ([`help_hit`]) owns the tab bar's geometry, so a tap
    /// resolves to exactly the control drawn (a tab, or the `[x]` close).
    fn help_hit_at(&self, client_x: f64, client_y: f64) -> Option<HelpHit> {
        let (col, row) = self.screen_cell(client_x, client_y)?;
        help_hit(self.state.layout().facility().width(), col, row)
    }

    /// Apply a [`HelpHit`] from a tap on the open panel: switch to the tapped tab or
    /// close. A view action like [`apply_help_nav`](Self::apply_help_nav).
    fn apply_help_hit(&mut self, hit: HelpHit) {
        match hit {
            HelpHit::Close => self.ui.help_open = false,
            HelpHit::Tab(tab) => self.ui.help_tab = tab,
        }
    }

    /// Map a viewport point `(client_x, client_y)` to the **screen cell** under it at
    /// the current fit, or `None` for a point off the canvas (a letterbox tap). The
    /// screen is `map + TOP_ROWS + BOTTOM_ROWS` rows fitted to the canvas, so a
    /// linear scale from the canvas rect gives the `(col, row)` the core drew — the
    /// one place the shell turns pixels into a grid coordinate, shared by every
    /// pointer hit-test so they can never disagree.
    fn screen_cell(&self, client_x: f64, client_y: f64) -> Option<(u32, u32)> {
        let rect = self.canvas.get_bounding_client_rect();
        let (rw, rh) = (rect.width(), rect.height());
        if !(rw > 0.0 && rh > 0.0) {
            return None;
        }
        let (lx, ly) = (client_x - rect.left(), client_y - rect.top());
        if lx < 0.0 || ly < 0.0 || lx >= rw || ly >= rh {
            return None; // outside the canvas (a letterbox tap)
        }
        let facility = self.state.layout().facility();
        let cols = facility.width();
        let rows = facility.height() + TOP_ROWS + BOTTOM_ROWS;
        let col = (lx / rw * cols as f64).floor() as u32;
        let row = (ly / rh * rows as f64).floor() as u32;
        Some((col, row))
    }

    /// Whether the viewport point lands on the deploy button (§11.4) — the core
    /// ([`is_ability_button`]) owns the button's geometry, so a click can never miss
    /// the button that is drawn.
    fn hit_deploy_button(&self, client_x: f64, client_y: f64) -> bool {
        let Some((col, row)) = self.screen_cell(client_x, client_y) else {
            return false;
        };
        let facility = self.state.layout().facility();
        let height = facility.height() + TOP_ROWS + BOTTOM_ROWS;
        is_ability_button(facility.width(), height, col, row)
    }

    /// Whether the viewport point lands on the near line's help toggle (§14
    /// v2/#139/#267) — the core ([`is_help_button`]) owns the `[?]` button's
    /// geometry, so a tap can never miss the button drawn.
    fn hit_help_button(&self, client_x: f64, client_y: f64) -> bool {
        let Some((col, row)) = self.screen_cell(client_x, client_y) else {
            return false;
        };
        is_help_button(self.state.layout().facility().width(), col, row)
    }

    /// Whether the viewport point lands on the near line's message-log toggle
    /// (§11.7) — the core ([`is_message_button`]) owns the counter's geometry, and
    /// whether there is a counter at all, so a click can never miss the toggle
    /// drawn nor hit one that is not there.
    fn hit_message_button(&self, client_x: f64, client_y: f64) -> bool {
        let Some((col, row)) = self.screen_cell(client_x, client_y) else {
            return false;
        };
        is_message_button(&self.state, col, row)
    }

    /// The ability under the viewport point, or `None` (§11.4). Maps the point to a
    /// screen cell and asks the core hit-test ([`ability_at`]), which owns both the
    /// line's and the panel's geometry — so a click resolves to exactly the entry
    /// drawn, by identity, and fires the one `Input::Activate` path a hotkey does.
    fn ability_at_point(&self, client_x: f64, client_y: f64) -> Option<AbilityId> {
        let (col, row) = self.screen_cell(client_x, client_y)?;
        ability_at(&self.state, self.ui, col, row)
    }
}

/// Install the keydown pump: each keypress drives one [`Game::handle_key`]. The
/// closure owns a clone of the `Rc` so the game outlives `start`; `forget` hands it to
/// the browser for the page's lifetime (the shell never tears down).
pub(crate) fn install_input(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let game = game.clone();
    let cb = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
        // `e.repeat()` is the browser's own held-key auto-repeat flag (§11.6): the
        // first keydown is fresh, every held-down repeat after it carries `repeat ==
        // true`. The shell forwards it so the core rule (#223) can treat a held
        // repeat differently from a deliberate press without the pump interpreting.
        if game.borrow_mut().handle_key(&e.key(), e.repeat()) {
            e.prevent_default();
        }
    });
    document.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())?;
    cb.forget();
    Ok(())
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
/// Wait — a turn must never burn on a gesture the player didn't finish.
struct GesturePump {
    game: Rc<RefCell<Game>>,
    /// The live gesture, if a finger is down.
    active: RefCell<Option<Gesture>>,
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

    /// A pointer pressed: the deploy button toggles the panel and an ability entry
    /// activates it (§11.4 — neither doubles as a gesture), anything else starts the
    /// gesture. Only the primary button gestures, and a second finger neither starts
    /// a second gesture nor re-aims the first.
    fn on_down(&self, e: &PointerEvent) {
        if e.button() != 0 {
            return; // secondary mouse buttons keep their browser meaning
        }
        let (x, y) = (e.client_x() as f64, e.client_y() as f64);
        {
            let mut game = self.game.borrow_mut();
            // Modal help (§14 v2/#248): while it is up, every press is the panel's — a
            // tap on a tab switches, on `[x]` closes, anywhere else is swallowed.
            // Nothing starts a gesture or steps the game while the panel captures input.
            if game.ui.help_open {
                if let Some(hit) = game.help_hit_at(x, y) {
                    game.apply_help_hit(hit);
                    game.draw();
                }
                e.prevent_default();
                return;
            }
            // The deploy button is tested first, so a tap on it toggles the panel and
            // never falls through to an activation underneath (§11.4).
            if game.hit_deploy_button(x, y) {
                game.apply_ui_command(UiCommand::ToggleAbilityPanel);
                game.draw();
                e.prevent_default();
                return;
            }
            // The help toggle, a view toggle like the deploy button (§14 v2/#139): a
            // tap opens or closes the reference panel and never starts a gesture. The
            // modal panel carries its own `[x]`, so the pair never traps (§11.6).
            if game.hit_help_button(x, y) {
                game.apply_ui_command(UiCommand::ToggleHelp);
                game.draw();
                e.prevent_default();
                return;
            }
            // The near line's message-log counter, likewise a view toggle (§11.7):
            // a tap deploys or folds the list and never starts a gesture.
            if game.hit_message_button(x, y) {
                game.apply_ui_command(UiCommand::ToggleMessageLog);
                game.draw();
                e.prevent_default();
                return;
            }
            // A tap on a line or panel entry fires the same `Input::Activate(id)` its
            // hotkey does (§11.4/§11.6); a cooling/active entry refuses for free in the
            // economy (§4.4). Consumed either way, so it never also walks the player.
            if let Some(id) = game.ability_at_point(x, y) {
                game.step_and_draw(Input::Activate(id));
                e.prevent_default();
                return;
            }
        }
        let mut active = self.active.borrow_mut();
        if active.is_none() {
            *active = Some(Gesture {
                pointer_id: e.pointer_id(),
                origin: (x, y),
                delta: (0.0, 0.0),
                fired: false,
                timer: RepeatTimer::Delay(self.arm(REPEAT_DELAY_MS, false)),
            });
        }
        // Consumed either way (§11.6): gestures are game input, and the browser's
        // follow-ups (double-tap zoom, synthetic clicks) must not fire off them.
        e.prevent_default();
    }

    /// The gesture's pointer moved: track the live displacement, and the instant
    /// the drag first crosses the swipe threshold fire its step — the swipe
    /// declaring itself — restarting the repeat cadence from that input exactly
    /// as a fresh keydown would.
    fn on_move(&self, e: &PointerEvent) {
        let first_step = {
            let mut active = self.active.borrow_mut();
            let Some(g) = active.as_mut().filter(|g| g.pointer_id == e.pointer_id()) else {
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
        let input = {
            let mut active = self.active.borrow_mut();
            let Some(g) = active.as_mut() else {
                return; // released while the tick was in flight — nothing may fire
            };
            g.fired = true;
            if let RepeatTimer::Delay(_) = g.timer {
                g.timer = RepeatTimer::Interval(self.arm(REPEAT_INTERVAL_MS, true));
            }
            gesture_input(g.delta.0, g.delta.1)
        };
        if let Some(input) = input {
            let mut game = self.game.borrow_mut();
            // A held swipe never auto-walks into visible danger (§11.6/#223): the
            // repeat is swallowed at the cone edge, the cadence left running so
            // dragging to a safe heading fires again — but going deeper needs a
            // fresh gesture. A held Wait (press-in-place) is never gated and keeps
            // waiting.
            if game.repeat_into_danger(input) {
                return;
            }
            game.step_and_draw(input);
        }
    }

    /// The gesture's pointer lifted: stop every repeat immediately, and if the
    /// gesture never fired, resolve it as the tap it was — at the lift point, so
    /// a press in place is one Wait and a flick too fast for a pointermove still
    /// steps. That input is the gesture's own, not a repeat leaking past the lift.
    fn on_up(&self, e: &PointerEvent) {
        let tap = {
            let mut active = self.active.borrow_mut();
            if !matches!(active.as_ref(), Some(g) if g.pointer_id == e.pointer_id()) {
                return;
            }
            let g = active.take().expect("matched just above");
            clear_timer(g.timer);
            if g.fired {
                None
            } else {
                gesture_input(
                    e.client_x() as f64 - g.origin.0,
                    e.client_y() as f64 - g.origin.1,
                )
            }
        };
        e.prevent_default();
        if let Some(input) = tap {
            self.game.borrow_mut().step_and_draw(input);
        }
    }

    /// The browser took the gesture away (`pointercancel`) or the pointer left the
    /// page (`pointerleave`): tear down without emitting anything — not even the
    /// tap's Wait. A turn must never burn on a gesture the player didn't end.
    fn on_abort(&self, e: &PointerEvent) {
        let mut active = self.active.borrow_mut();
        if matches!(active.as_ref(), Some(g) if g.pointer_id == e.pointer_id()) {
            clear_timer(active.take().expect("matched just above").timer);
        }
    }
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
