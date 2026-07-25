//! The replay-viewer mode (§12.4 / #197): play a captured `(seed, inputs)` run
//! back in the browser, with a scrub cursor the player drives.
//!
//! A replay is `(seed, [inputs])` and nothing else (§12.4 [SETTLED]). The viewer
//! needs no snapshot machinery: it holds a cursor `K` and derives the shown
//! [`State`] by **re-running the seed through `inputs[0..K]`** — "replay-minus-N"
//! (§12.4), cheap on the v1 footprint (≤ a few hundred inputs, 40×40). Forward and
//! backward therefore land on *identical* states for the same `K`, because both
//! recompute from the seed rather than stepping or un-stepping a live world.
//!
//! **This is a pure view.** It changes no world and is distinct from the in-game
//! Rewind ability (§14 "Later"), which stays untouched — nothing here steps the
//! player. The mode is chosen once at boot ([`initial_replay`]) behind an explicit
//! carrier, and the shell installs *either* the live pumps *or* this one, never
//! both — so the two gesture maps can never collide (the ticket's headline
//! footgun). Live play reads a swipe as a *direction to move*; a replay reads a
//! swipe as a *direction in time*: right/`→` advances, left/`←` rewinds, a tap
//! steps one, and lifting stops every repeat instantly (§11.6 fairness).

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{parse_script, Input, LevelSeed, State};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, KeyboardEvent, PointerEvent};

use crate::input::{
    clear_timer, RepeatTimer, REPEAT_DELAY_MS, REPEAT_INTERVAL_MS, SWIPE_THRESHOLD_PX,
};
use crate::{new_run, Game};

/// A captured run and the time cursor into it (§12.4): the level, the input stream,
/// and `K` — how many inputs have been replayed. `K` ranges over `0..=inputs.len()`
/// (`0` is the fresh facility, `total` the run's end). The shown state is always
/// `state_at(K)`, re-simulated from the level, so it can never drift from what a
/// live run of the same inputs would show. The level is the whole reproducible config
/// `(seed, modifiers, abilities)` (#245), so a replay of a non-default preset boots
/// the *right* modifiers and loadout, not just the geometry.
pub(crate) struct ReplayView {
    level: LevelSeed,
    inputs: Vec<Input>,
    cursor: usize,
}

impl ReplayView {
    /// The number of inputs — the cursor's upper bound and the HUD's denominator.
    pub(crate) fn total(&self) -> usize {
        self.inputs.len()
    }

    /// The current cursor `K`.
    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    /// Advance the cursor one input, clamped at the end. Returns whether it moved,
    /// so a scrub at `K == total` is a silent no-op, never a wrap.
    fn forward(&mut self) -> bool {
        if self.cursor < self.inputs.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    /// Rewind the cursor one input, clamped at `0`. Returns whether it moved.
    fn backward(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    /// The state at the current cursor: the seed's facility re-run through the first
    /// `K` inputs (replay-minus-N, §12.4). A fresh re-simulation each call — the
    /// design's intended cheap path — so forward and backward to the same `K` are
    /// byte-identical by construction. `pub(crate)` so the boot can paint the
    /// opening frame (`K = 0`) before the input pumps are wired.
    pub(crate) fn state_at(&self) -> Result<State, JsValue> {
        let mut state = new_run(&self.level)?;
        for &input in &self.inputs[..self.cursor] {
            state.step(input);
        }
        Ok(state)
    }
}

/// The replay to boot into, or `None` for ordinary live play. The inputs come from
/// an explicit carrier — a **baked** `window.__intrusionReplay` script string the
/// build stamped in (how a replay Artifact pins its run, slice C), or an `inputs=`
/// field in the page **URL** — paired with the level the shell already resolved
/// ([`crate::seed::initial_level`]). This reuses #110's seed surface and only widens
/// the payload to `(level, inputs)`; it does not invent a second scheme, and the
/// level carries the modifiers and loadout too (#245), so a replay reproduces the
/// exact run, not just its geometry.
///
/// An absent carrier is live play (`None`). A present-but-malformed one is *also*
/// live play, not an error: a bad replay must never brick the page, exactly as a
/// bad token falls through to a fresh run (#110). An empty stream is a valid replay
/// of length zero — just the level's opening facility.
pub(crate) fn initial_replay(level: LevelSeed) -> Option<ReplayView> {
    let inputs = parse_script(&replay_script()?).ok()?;
    Some(ReplayView {
        level,
        inputs,
        cursor: 0,
    })
}

/// The replay script string from its carrier, in priority order: a baked-in
/// `window.__intrusionReplay` global (the build's, artifact-safe — the host strips
/// a URL before the framed page sees it), then an `inputs=` field in the URL
/// query or hash. `None` when neither is present.
fn replay_script() -> Option<String> {
    baked_script().or_else(script_from_url)
}

/// A replay script the *build* stamped in as `window.__intrusionReplay` — how a
/// replay-locked Artifact carries its run with no URL and no typing (slice C's
/// `assemble.py --replay …`), mirroring the baked-seed global (#110). Tolerates a
/// JS string; absent (the normal build) it is `None`.
fn baked_script() -> Option<String> {
    let window = web_sys::window()?;
    js_sys::Reflect::get(&window, &JsValue::from_str("__intrusionReplay"))
        .ok()?
        .as_string()
        .filter(|s| !s.is_empty())
}

/// An `inputs=<script>` field from the page URL — `?inputs=` first, then
/// `#inputs=` — for a host that passes the query/hash through (e.g. the Pages
/// deploy). The value is the raw script notation; whitespace in it is ignored by
/// the parser, so no percent-decoding subtlety is load-bearing here.
fn script_from_url() -> Option<String> {
    let location = web_sys::window()?.location();
    let query = location.search().ok();
    let hash = location.hash().ok();
    query
        .as_deref()
        .and_then(inputs_in)
        .or_else(|| hash.as_deref().and_then(inputs_in))
}

/// Find an `inputs=<value>` field in a `?a=b&…` query or `#a=b&…` hash fragment.
/// Tolerates other fields around it (the shared `seed=` among them) and a leading
/// `?`/`#`.
fn inputs_in(fragment: &str) -> Option<String> {
    fragment
        .trim_start_matches(['?', '#'])
        .split('&')
        .find_map(|pair| pair.strip_prefix("inputs="))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// A direction in *time* a replay gesture drives — the scrub counterpart of a
/// live [`Input`]. Right is forward (toward the run's end), left is rewind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scrub {
    Forward,
    Backward,
}

/// Map a drag displacement `(dx, dy)` — CSS pixels from the press point — to the
/// [`Scrub`] it drives, or `None` for no scrub. The replay half of §11.6, pure so
/// the time-gesture rule is testable natively, deliberately a **different** map
/// from live play's [`gesture_input`](crate::input): time runs along the
/// horizontal axis only.
///
/// A drag scrubs only once it crosses [`SWIPE_THRESHOLD_PX`] horizontally *and* is
/// horizontally dominant — a vertical or sub-threshold drag is `None`, so a resting
/// finger never scrubs and an up/down flick does nothing. Right → forward, left →
/// rewind. Re-read live on every repeat tick, so dragging across the origin
/// reverses the scrub without lifting. A non-finite displacement is `None`.
fn scrub_direction(dx: f64, dy: f64) -> Option<Scrub> {
    if !(dx.is_finite() && dy.is_finite()) {
        return None;
    }
    if dx.abs() < SWIPE_THRESHOLD_PX || dx.abs() < dy.abs() {
        return None;
    }
    Some(if dx < 0.0 {
        Scrub::Backward
    } else {
        Scrub::Forward
    })
}

/// The replay-mode half of [`Game`]: drive the time cursor, never the world.
impl Game {
    /// Move the replay cursor one step in `dir`, rebuild the shown state, and
    /// repaint — or do nothing if the cursor is already clamped at that end. The
    /// state is re-simulated from the seed (§12.4), so this changes no world; it is
    /// the pure-view seam every replay input (a key, a scrub tick, a tap) drives.
    fn scrub(&mut self, dir: Scrub) -> bool {
        let moved = match self.replay.as_mut() {
            Some(view) => match dir {
                Scrub::Forward => view.forward(),
                Scrub::Backward => view.backward(),
            },
            None => return false,
        };
        if moved {
            // Re-borrow immutably to re-simulate; keep the old frame if generation
            // somehow fails (the v1 footprint always carves, §10.6).
            if let Ok(state) = self.replay.as_ref().expect("checked above").state_at() {
                self.state = state;
            }
            self.draw();
        }
        moved
    }

    /// Map a key to a replay scrub (§11.6 parity with the touch scrub): `Space`/`→`
    /// step forward, `←` steps back — each repeating on hold through the browser's
    /// own key auto-repeat, the keyboard counterpart of holding a swipe. Returns
    /// whether the key was consumed, so the page keeps its other keys.
    fn handle_replay_key(&mut self, key: &str) -> bool {
        let dir = match key {
            " " | "ArrowRight" => Scrub::Forward,
            "ArrowLeft" => Scrub::Backward,
            _ => return false,
        };
        self.scrub(dir);
        true
    }

    /// Refresh the replay HUD text (`K / total`) from the cursor — called on every
    /// redraw so the position and the board never disagree. A no-op in live play
    /// (no HUD element) and harmless if the page carries no HUD markup.
    pub(crate) fn update_replay_hud(&self) {
        if let (Some(view), Some(pos)) = (&self.replay, &self.replay_hud) {
            pos.set_text_content(Some(&format!("{} / {}", view.cursor(), view.total())));
        }
    }
}

/// Install the replay-viewer input (§11.6): the keydown pump and the touch scrub
/// pump, plus revealing the HUD. Called at boot **instead of** the live pumps when
/// the shell booted into a replay ([`initial_replay`]), so the live gesture map is
/// never even wired — the two maps cannot collide.
pub(crate) fn install(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    // Reveal the HUD and wire its position element, then paint the opening frame so
    // it reads `0 / total` from the first moment.
    if let Some(bar) = document.get_element_by_id("replaybar") {
        // The div carries no other class, so naming it "on" is enough to trip the
        // `#replaybar.on { display: flex }` reveal rule in web/index.html.
        bar.set_class_name("on");
    }
    game.borrow_mut().replay_hud = document.get_element_by_id("replay-pos");
    game.borrow().update_replay_hud();

    // Keyboard: Space/→ forward, ← back, each repeating on hold (§11.6 parity).
    {
        let game = game.clone();
        let cb = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
            if game.borrow_mut().handle_replay_key(&e.key()) {
                e.prevent_default();
            }
        });
        document.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    install_scrub_gestures(document, game)
}

/// One finger's live scrub gesture — the replay parallel of a live [`Gesture`],
/// tracking a *time* drag rather than a movement one. Exists only while the pointer
/// is down; release or cancel destroys it and its timer together, so no scrub ever
/// lands after the lift.
struct ScrubGesture {
    pointer_id: i32,
    origin: (f64, f64),
    delta: (f64, f64),
    /// Whether a scrub has fired yet. A gesture that lifts without firing is a
    /// **tap** — one step forward — resolved at the lift; a hold in place (no
    /// horizontal swipe) never fires, so it too resolves as that single tap.
    fired: bool,
    timer: RepeatTimer,
}

/// The touch scrub pump — §11.6's touch half for the replay viewer, the time-axis
/// sibling of the live [`GesturePump`](crate::input). A **swipe right/left** scrubs
/// forward/back the instant it crosses [`SWIPE_THRESHOLD_PX`] and keeps scrubbing
/// while held (auto-advance / auto-rewind); a **tap** steps one forward; a **press
/// held in place** does nothing until it swipes. Every tick re-reads the live
/// displacement, so dragging across the origin reverses the scrub.
///
/// Fairness (§2.2/§4.5): each tick drives exactly one [`Game::scrub`] against the
/// current frame, and release/cancel clears the timer before anything else fires —
/// so no scrub lands after the finger lifts, the same contract the live pump keeps
/// on the world.
struct ScrubPump {
    game: Rc<RefCell<Game>>,
    active: RefCell<Option<ScrubGesture>>,
    tick: RefCell<Option<Closure<dyn FnMut()>>>,
}

impl ScrubPump {
    /// Arm the repeat tick — the one-shot delay or the steady interval — and hand
    /// back its id for the gesture to own (mirrors the live pump's `arm`).
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

    /// A pointer pressed: start a scrub gesture and arm the initial delay. Only the
    /// primary button, and a second finger neither starts nor re-aims the first.
    fn on_down(&self, e: &PointerEvent) {
        if e.button() != 0 {
            return;
        }
        let mut active = self.active.borrow_mut();
        if active.is_none() {
            *active = Some(ScrubGesture {
                pointer_id: e.pointer_id(),
                origin: (e.client_x() as f64, e.client_y() as f64),
                delta: (0.0, 0.0),
                fired: false,
                timer: RepeatTimer::Delay(self.arm(REPEAT_DELAY_MS, false)),
            });
        }
        e.prevent_default();
    }

    /// The gesture's pointer moved: track the live displacement, and the instant a
    /// drag first crosses the horizontal threshold fire its scrub — the swipe
    /// declaring itself — restarting the repeat cadence from that scrub.
    fn on_move(&self, e: &PointerEvent) {
        let first = {
            let mut active = self.active.borrow_mut();
            let Some(g) = active.as_mut().filter(|g| g.pointer_id == e.pointer_id()) else {
                return;
            };
            g.delta = (
                e.client_x() as f64 - g.origin.0,
                e.client_y() as f64 - g.origin.1,
            );
            match scrub_direction(g.delta.0, g.delta.1) {
                Some(dir) if !g.fired => {
                    g.fired = true;
                    clear_timer(g.timer);
                    g.timer = RepeatTimer::Delay(self.arm(REPEAT_DELAY_MS, false));
                    Some(dir)
                }
                _ => None,
            }
        };
        if let Some(dir) = first {
            self.game.borrow_mut().scrub(dir);
        }
    }

    /// The armed timer fired: re-read the live displacement and, if it is a swipe,
    /// scrub one step that way; a hold in place (no horizontal swipe) scrubs
    /// nothing. On the one-shot delay, settle into the steady cadence.
    fn on_tick(&self) {
        let dir = {
            let mut active = self.active.borrow_mut();
            let Some(g) = active.as_mut() else {
                return; // released while the tick was in flight — nothing may fire
            };
            if let RepeatTimer::Delay(_) = g.timer {
                g.timer = RepeatTimer::Interval(self.arm(REPEAT_INTERVAL_MS, true));
            }
            match scrub_direction(g.delta.0, g.delta.1) {
                Some(dir) => {
                    g.fired = true;
                    Some(dir)
                }
                None => None,
            }
        };
        if let Some(dir) = dir {
            self.game.borrow_mut().scrub(dir);
        }
    }

    /// The gesture's pointer lifted: stop every repeat immediately, and if it never
    /// scrubbed, resolve it as the **tap** it was — one step forward. That step is
    /// the gesture's own, not a repeat leaking past the lift.
    fn on_up(&self, e: &PointerEvent) {
        let tap = {
            let mut active = self.active.borrow_mut();
            if !matches!(active.as_ref(), Some(g) if g.pointer_id == e.pointer_id()) {
                return;
            }
            let g = active.take().expect("matched just above");
            clear_timer(g.timer);
            !g.fired
        };
        e.prevent_default();
        if tap {
            self.game.borrow_mut().scrub(Scrub::Forward);
        }
    }

    /// The browser took the gesture (`pointercancel`) or the pointer left the page
    /// (`pointerleave`): tear down without scrubbing — not even the tap. A frame
    /// must never advance on a gesture the player did not finish.
    fn on_abort(&self, e: &PointerEvent) {
        let mut active = self.active.borrow_mut();
        if matches!(active.as_ref(), Some(g) if g.pointer_id == e.pointer_id()) {
            clear_timer(active.take().expect("matched just above").timer);
        }
    }
}

/// Install the scrub pump's pointer listeners — the replay parallel of
/// [`install_gestures`](crate::input), wired only in replay mode.
fn install_scrub_gestures(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let pump = Rc::new(ScrubPump {
        game: game.clone(),
        active: RefCell::new(None),
        tick: RefCell::new(None),
    });
    let p = pump.clone();
    *pump.tick.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || p.on_tick()));

    type Handler = fn(&ScrubPump, &PointerEvent);
    let listeners: [(&str, Handler); 5] = [
        ("pointerdown", ScrubPump::on_down),
        ("pointermove", ScrubPump::on_move),
        ("pointerup", ScrubPump::on_up),
        ("pointercancel", ScrubPump::on_abort),
        ("pointerleave", ScrubPump::on_abort),
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

    /// The replay time-gesture rule, pure and pinned: a horizontal swipe past the
    /// threshold scrubs — right forward, left back — while a sub-threshold or
    /// vertically-dominant drag scrubs nothing. This is a *different* map from live
    /// play (a swipe up/down there steps north/south); booting the wrong one is the
    /// ticket's headline footgun, so the rule is nailed down here.
    #[test]
    fn a_horizontal_swipe_scrubs_and_nothing_else_does() {
        let t = SWIPE_THRESHOLD_PX;
        // Right/left past the threshold scrub forward/back.
        assert_eq!(scrub_direction(t, 0.0), Some(Scrub::Forward));
        assert_eq!(scrub_direction(40.0, 8.0), Some(Scrub::Forward));
        assert_eq!(scrub_direction(-t, 0.0), Some(Scrub::Backward));
        assert_eq!(scrub_direction(-40.0, -8.0), Some(Scrub::Backward));
        // Sub-threshold: no scrub (a resting finger never moves time).
        assert_eq!(scrub_direction(t - 0.5, 0.0), None);
        assert_eq!(scrub_direction(0.0, 0.0), None);
        // Vertically dominant: no scrub — time has no vertical axis.
        assert_eq!(scrub_direction(10.0, -40.0), None);
        assert_eq!(scrub_direction(-10.0, 40.0), None);
    }

    /// The live re-evaluation contract (parity with the live pump): the rule is pure
    /// in the displacement, so a repeat tick re-reading a drag dragged across the
    /// origin reverses the scrub, and one pulled back inside the threshold stops it.
    #[test]
    fn a_dragging_finger_reverses_the_scrub_live() {
        assert_eq!(scrub_direction(40.0, 0.0), Some(Scrub::Forward));
        assert_eq!(scrub_direction(-40.0, 0.0), Some(Scrub::Backward));
        assert_eq!(scrub_direction(5.0, 0.0), None);
    }

    /// A non-finite displacement scrubs nothing rather than a garbage jump.
    #[test]
    fn a_non_finite_drag_is_ignored() {
        assert_eq!(scrub_direction(f64::NAN, 0.0), None);
        assert_eq!(scrub_direction(0.0, f64::INFINITY), None);
    }

    /// The URL/hash reader accepts an `inputs=` field from either fragment, tolerates
    /// the shared `seed=` beside it, and rejects an absent or empty value — the
    /// graceful fallback to live play (#110's tolerance, widened to the pair).
    #[test]
    fn an_inputs_field_is_read_from_a_query_or_hash() {
        assert_eq!(inputs_in("?inputs=N+rE").as_deref(), Some("N+rE"));
        assert_eq!(inputs_in("#inputs=SS.").as_deref(), Some("SS."));
        assert_eq!(inputs_in("?seed=42&inputs=NE").as_deref(), Some("NE"));
        assert_eq!(inputs_in("#seed=42").as_deref(), None);
        assert_eq!(inputs_in("?inputs=").as_deref(), None);
        assert_eq!(inputs_in(""), None);
    }
}
