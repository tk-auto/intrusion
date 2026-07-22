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
    input_for_key, is_ability_button, ui_command_for_key, Direction, Input, UiCommand, HEADER_ROWS,
    STATUS_ROWS,
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
    fn handle_key(&mut self, key: &str) -> bool {
        // UI commands (§11.4) come first: they toggle view state and redraw without
        // ever touching the turn loop. `Tab` deploys the ability panel.
        if let Some(command) = ui_command_for_key(key) {
            self.apply_ui_command(command);
            self.draw();
            return true;
        }
        let Some(input) = input_for_key(key) else {
            return false;
        };
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

    /// Apply a shell-level [`UiCommand`] (§11.4) — a view toggle, never a game
    /// action, so it changes no [`State`](intrusion_core::State).
    fn apply_ui_command(&mut self, command: UiCommand) {
        match command {
            UiCommand::ToggleAbilityPanel => {
                self.ui.ability_panel_open = !self.ui.ability_panel_open;
            }
        }
    }

    /// Whether the viewport point `(client_x, client_y)` lands on the deploy button
    /// (§11.4). Maps the point into the canvas, converts it to a screen cell at the
    /// current fit, and asks the core ([`is_ability_button`]) — the one owner of the
    /// button's geometry, so a click can never miss the button that is drawn.
    fn hit_deploy_button(&self, client_x: f64, client_y: f64) -> bool {
        let rect = self.canvas.get_bounding_client_rect();
        let (rw, rh) = (rect.width(), rect.height());
        if !(rw > 0.0 && rh > 0.0) {
            return false;
        }
        let (lx, ly) = (client_x - rect.left(), client_y - rect.top());
        if lx < 0.0 || ly < 0.0 || lx >= rw || ly >= rh {
            return false; // outside the canvas (a letterbox tap) — not the button
        }
        let facility = self.state.layout().facility();
        let cols = facility.width();
        let rows = facility.height() + HEADER_ROWS + STATUS_ROWS;
        let col = (lx / rw * cols as f64).floor() as u32;
        let row = (ly / rh * rows as f64).floor() as u32;
        is_ability_button(cols, col, row)
    }
}

/// Install the keydown pump: each keypress drives one [`Game::handle_key`]. The
/// closure owns a clone of the `Rc` so the game outlives `start`; `forget` hands it to
/// the browser for the page's lifetime (the shell never tears down).
pub(crate) fn install_input(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let game = game.clone();
    let cb = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
        if game.borrow_mut().handle_key(&e.key()) {
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
/// of a resting finger never walks the player.
const SWIPE_THRESHOLD_PX: f64 = 24.0;

/// The pause between a gesture's first input and its first repeat — the touch
/// counterpart of the keyboard's auto-repeat delay (§11.6's reference cadence).
/// Long enough that one deliberate swipe or press stays a single input.
const REPEAT_DELAY_MS: i32 = 300;

/// The cadence of repeats while the finger stays down — one ordinary [`Input`]
/// per tick through the same seam as a held arrow key, never a batch (§4.1/§4.3).
const REPEAT_INTERVAL_MS: i32 = 120;

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

/// The browser timer currently driving a gesture's repeats: the one-shot initial
/// delay (`setTimeout`) or the steady cadence (`setInterval`). Whichever is
/// armed, release clears it by id — that clear is what guarantees no step or
/// wait ever fires after the finger lifts (§2.2/§4.5 fairness).
#[derive(Clone, Copy)]
enum RepeatTimer {
    Delay(i32),
    Interval(i32),
}

/// Clear an armed [`RepeatTimer`] with the browser. Clearing an id that already
/// fired is a harmless no-op, so teardown never has to know the timer's fate.
fn clear_timer(timer: RepeatTimer) {
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

    /// A pointer pressed: the deploy button toggles the panel (§11.4 — the button
    /// never doubles as a gesture), anything else starts the gesture. Only the
    /// primary button gestures, and a second finger neither starts a second
    /// gesture nor re-aims the first.
    fn on_down(&self, e: &PointerEvent) {
        if e.button() != 0 {
            return; // secondary mouse buttons keep their browser meaning
        }
        let (x, y) = (e.client_x() as f64, e.client_y() as f64);
        {
            let mut game = self.game.borrow_mut();
            if game.hit_deploy_button(x, y) {
                game.apply_ui_command(UiCommand::ToggleAbilityPanel);
                game.draw();
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
            self.game.borrow_mut().step_and_draw(input);
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
}
