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

    /// Advance the cursor by `count` inputs, clamped at the end. Returns whether it
    /// moved, so a scrub at `K == total` is a silent no-op, never a wrap. A held
    /// scrub advances several at once (the #227 ramp); the state is re-simulated
    /// **once** at the new cursor, not per skipped input, so a big jump stays O(K).
    fn forward_by(&mut self, count: usize) -> bool {
        let target = self.cursor.saturating_add(count).min(self.inputs.len());
        let moved = target != self.cursor;
        self.cursor = target;
        moved
    }

    /// Rewind the cursor by `count` inputs, clamped at `0`. Returns whether it moved.
    fn backward_by(&mut self, count: usize) -> bool {
        let target = self.cursor.saturating_sub(count);
        let moved = target != self.cursor;
        self.cursor = target;
        moved
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

/// How many cursor steps a top-rate held scrub advances per tick (§12.4/#227). The
/// ramp climbs to this and no further, so even a very long hold keeps readable
/// motion instead of skipping the whole run in one blank frame. A [START] value: on
/// the v1 footprint (a few hundred inputs) at [`REPEAT_INTERVAL_MS`], the ramp
/// reaches this after ~1.7 s and thereafter clears ~65 inputs/s — a gentle
/// fast-forward, while a tap still steps one.
const SCRUB_MAX_STEP: usize = 8;

/// How many held ticks add one step to the scrub rate (§12.4/#227) — the ramp's
/// slope. Larger is **gentler**: the hold speeds up more slowly, so a short hold
/// stays fine-grained and only a sustained one fast-forwards. A [START] value.
const SCRUB_RAMP_TICKS_PER_STEP: u32 = 2;

/// The scrub ramp (§12.4/#227): how many cursor steps a single held scrub tick
/// advances, given how many ticks the scrub has been held **unbroken**. The first
/// tick of a hold (`ticks_held == 0`) — and a lone tap — advances exactly one; every
/// [`SCRUB_RAMP_TICKS_PER_STEP`] further ticks add one more, so a sustained hold
/// speeds up gently, capped at [`SCRUB_MAX_STEP`]. Pure in the tick count so the ramp
/// is testable natively, in the spirit of [`scrub_direction`] /
/// [`gesture_input`](crate::input); counting **ticks**, not wall-clock, keeps
/// keyboard auto-repeat and a held swipe feeling the same and keeps the rule
/// clock-free.
fn ramp_steps(ticks_held: u32) -> usize {
    ((ticks_held / SCRUB_RAMP_TICKS_PER_STEP) as usize + 1).min(SCRUB_MAX_STEP)
}

/// The running ramp of a held scrub: the direction currently held and how many ticks
/// it has repeated unbroken. Shared by both scrub inputs — the keyboard's auto-repeat
/// (one instance on [`Game`]) and a held swipe (one per [`ScrubGesture`]) — so
/// acceleration feels the same however the run is scrubbed. It only tracks the tick;
/// the step count itself is the pure [`ramp_steps`].
#[derive(Default)]
pub(crate) struct ScrubRamp {
    dir: Option<Scrub>,
    ticks: u32,
}

impl ScrubRamp {
    /// Advance the ramp for one scrub tick in `dir` and return how many cursor steps
    /// it should move. A tick continuing the same unbroken hold climbs the ramp; a
    /// **reversal** (or the first tick after a [`reset`](Self::reset)) restarts it at
    /// one step. Pure book-keeping around [`ramp_steps`] — no clock, no world.
    fn advance(&mut self, dir: Scrub) -> usize {
        if self.dir == Some(dir) {
            self.ticks += 1;
        } else {
            self.dir = Some(dir);
            self.ticks = 0;
        }
        ramp_steps(self.ticks)
    }

    /// Break the hold, so the next [`advance`](Self::advance) starts slow again.
    /// Called when a scrub stops without the ramp's owner being destroyed — a fresh
    /// key press (as opposed to its auto-repeat), or a swipe tick that finds no scrub
    /// this frame (the finger pulled back inside the threshold).
    fn reset(&mut self) {
        self.dir = None;
        self.ticks = 0;
    }
}

/// The replay-mode half of [`Game`]: drive the time cursor, never the world.
impl Game {
    /// Move the replay cursor `count` steps in `dir`, rebuild the shown state, and
    /// repaint — or do nothing if the cursor is already clamped at that end. The
    /// state is re-simulated from the seed **once** at the new cursor (§12.4), so a
    /// held-scrub jump of many steps stays a single O(K) re-simulation and changes no
    /// world; it is the pure-view seam every replay input (a key, a scrub tick, a
    /// tap) drives.
    fn scrub_by(&mut self, dir: Scrub, count: usize) -> bool {
        let moved = match self.replay.as_mut() {
            Some(view) => match dir {
                Scrub::Forward => view.forward_by(count),
                Scrub::Backward => view.backward_by(count),
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
    ///
    /// A held key **accelerates** (§12.4/#227): `is_repeat` is the browser's own
    /// auto-repeat flag, so a fresh press resets the ramp (one step) and each held
    /// repeat climbs it, exactly as a held swipe does. Releasing the key stops the
    /// OS repeat, so the scrub halts instantly with no timer to leak (§11.6); the
    /// next fresh press starts slow again.
    fn handle_replay_key(&mut self, key: &str, is_repeat: bool) -> bool {
        let dir = match key {
            " " | "ArrowRight" => Scrub::Forward,
            "ArrowLeft" => Scrub::Backward,
            _ => return false,
        };
        if !is_repeat {
            self.key_ramp.reset();
        }
        let steps = self.key_ramp.advance(dir);
        self.scrub_by(dir, steps);
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
            // `e.repeat()` is the browser's held-key auto-repeat flag: false on the
            // deliberate first press, true on every held repeat after it — the ramp's
            // tick source (§12.4/#227), the keyboard counterpart of a held swipe.
            if game.borrow_mut().handle_replay_key(&e.key(), e.repeat()) {
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
    /// The acceleration ramp of this held swipe (§12.4/#227): each unbroken tick in
    /// the same direction advances more of the cursor, up to the cap. Reversing the
    /// swipe or pulling back inside the threshold restarts it; lifting destroys the
    /// gesture and the ramp with it, so a fresh swipe always starts slow.
    ramp: ScrubRamp,
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
                ramp: ScrubRamp::default(),
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
                    // The swipe's first scrub — one step (a fresh ramp), then each
                    // held tick climbs it (§12.4/#227).
                    Some((dir, g.ramp.advance(dir)))
                }
                _ => None,
            }
        };
        if let Some((dir, steps)) = first {
            self.game.borrow_mut().scrub_by(dir, steps);
        }
    }

    /// The armed timer fired: re-read the live displacement and, if it is a swipe,
    /// scrub one step that way; a hold in place (no horizontal swipe) scrubs
    /// nothing. On the one-shot delay, settle into the steady cadence.
    fn on_tick(&self) {
        let scrub = {
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
                    // A held tick climbs the ramp; a reversal restarts it (handled
                    // inside `advance`), so a long hold fast-forwards (§12.4/#227).
                    Some((dir, g.ramp.advance(dir)))
                }
                None => {
                    // Pulled back inside the threshold: no scrub, and the hold is
                    // broken — a fresh swipe out must start slow again.
                    g.ramp.reset();
                    None
                }
            }
        };
        if let Some((dir, steps)) = scrub {
            self.game.borrow_mut().scrub_by(dir, steps);
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
            self.game.borrow_mut().scrub_by(Scrub::Forward, 1);
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

    /// The scrub ramp (§12.4/#227), pure and pinned: the first held tick — and a lone
    /// tap — advances exactly one, the rate climbs one step every
    /// [`SCRUB_RAMP_TICKS_PER_STEP`] ticks (a gentle slope), and it caps at
    /// [`SCRUB_MAX_STEP`] so a very long hold still leaves readable motion instead of
    /// skipping the whole run in one frame.
    #[test]
    fn the_ramp_starts_at_one_and_caps() {
        assert_eq!(ramp_steps(0), 1, "the first tick / a tap steps exactly one");
        assert_eq!(
            ramp_steps(1),
            1,
            "still one part-way through the first stride"
        );
        assert_eq!(ramp_steps(2), 2, "one extra step per stride of ticks");
        assert_eq!(ramp_steps(3), 2);
        assert_eq!(ramp_steps(4), 3);
        // Climbs up to the cap and never past it.
        let cap_tick = (SCRUB_MAX_STEP as u32 - 1) * SCRUB_RAMP_TICKS_PER_STEP;
        assert_eq!(ramp_steps(cap_tick), SCRUB_MAX_STEP, "climbs up to the cap");
        assert_eq!(
            ramp_steps(cap_tick + 1),
            SCRUB_MAX_STEP,
            "and never past it"
        );
        assert_eq!(ramp_steps(10_000), SCRUB_MAX_STEP);
    }

    /// The ramp's state machine: an unbroken hold climbs (gently), while a
    /// **reversal** or an explicit **reset** (a fresh key press, or a swipe that
    /// stops) starts slow again — so a tap always steps one and only a sustained hold
    /// fast-forwards.
    #[test]
    fn the_ramp_climbs_on_hold_and_restarts_on_reverse_or_reset() {
        let mut ramp = ScrubRamp::default();
        // The first held tick (like a tap) is one step; an unbroken hold then speeds
        // up, never slowing, until it reaches the cap.
        let first = ramp.advance(Scrub::Forward);
        assert_eq!(first, 1, "the first held tick steps one");
        let mut prev = first;
        let mut climbed = false;
        for _ in 0..(SCRUB_MAX_STEP as u32 * SCRUB_RAMP_TICKS_PER_STEP) {
            let next = ramp.advance(Scrub::Forward);
            assert!(next >= prev, "an unbroken hold never slows down");
            climbed |= next > first;
            prev = next;
        }
        assert!(climbed, "a sustained hold speeds up");
        assert_eq!(prev, SCRUB_MAX_STEP, "and tops out at the cap");
        // Reversing restarts the ramp at one step.
        assert_eq!(ramp.advance(Scrub::Backward), 1);
        // An explicit reset (a fresh press / a broken hold) restarts it too.
        ramp.advance(Scrub::Backward);
        ramp.reset();
        assert_eq!(ramp.advance(Scrub::Backward), 1);
    }

    /// The cursor's multi-step jumps (the ramp's payload): a held scrub advances the
    /// cursor several inputs at once, clamped at either end and never wrapping — so a
    /// big jump lands on an exact `K` the shown state is then re-simulated from once
    /// (§12.4), and forward/backward to the same `K` land on the same cursor.
    #[test]
    fn the_cursor_jumps_by_count_and_clamps_at_both_ends() {
        let mut view = ReplayView {
            level: LevelSeed::quick_play(1),
            inputs: vec![Input::Wait; 5],
            cursor: 0,
        };
        // A multi-step forward jump advances exactly that many.
        assert!(view.forward_by(3));
        assert_eq!(view.cursor(), 3);
        // Overshooting clamps to the end but still counts as movement.
        assert!(view.forward_by(10));
        assert_eq!(view.cursor(), 5);
        // At the end, a further scrub is a silent no-op, never a wrap.
        assert!(!view.forward_by(4));
        assert_eq!(view.cursor(), 5);
        // Rewind is symmetric and saturates at zero.
        assert!(view.backward_by(2));
        assert_eq!(view.cursor(), 3);
        assert!(view.backward_by(99));
        assert_eq!(view.cursor(), 0);
        assert!(!view.backward_by(1));
        assert_eq!(view.cursor(), 0);
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
