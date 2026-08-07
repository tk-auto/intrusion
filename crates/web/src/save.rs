//! The run's **autosave** (§12.5/#514): the snapshot, the write policy, and the one
//! boundary a save crosses to reach the browser.
//!
//! §12.5 gave the choice between snapshotting the run and storing `(seed, inputs)`.
//! This is the **snapshot** branch, and the reason is drift: re-feeding a run's inputs
//! reproduces the run *this build* would have played, so a save written before a
//! tuning change silently resumes a different game — where a snapshot either decodes
//! to exactly the state that was written or does not decode at all (appendix 50). It
//! also costs nothing at restore: a long run is a parse, not a re-run of two thousand
//! turns.
//!
//! **There is no save verb** — no key, no menu entry, no button. The run is written as
//! it plays and the title screen grows a *Continue run* row when there is one to
//! resume ([`crate::menu`]). Permadeath shapes the rest (§2.2): one slot, overwritten
//! forward, emptied the moment the run ends. A save is *interruption resume*, never an
//! undo and never a retry.
//!
//! # The write policy
//!
//! A write per turn would mean one JSON encode of the whole world every
//! [`REPEAT_INTERVAL_MS`](crate::input::REPEAT_INTERVAL_MS) while a key is held, so
//! writes are **debounced**: a turn arms a trailing write [`DEBOUNCE_MS`] out, and each
//! further turn re-arms it, so a burst coalesces into one write after the burst. Two
//! things bypass the debounce:
//!
//! - a **terminal** turn — capture, or the run won — which *empties* the slot as part
//!   of resolving the turn, so the one window that would let a player un-die is shut
//!   synchronously rather than left to a timer;
//! - the page **hiding** ([`install`]), which is the moment a phone user is actually
//!   leaving.
//!
//! And one thing bounds it: after [`TURN_CAP`] turns with nothing written, the next
//! turn writes outright. The clock alone would leave a whole held burst at risk; the
//! cap says the window is bounded in *turns* as well as in seconds. Losing what is
//! left of the window — killing the tab mid-burst — costs a few turns, which is an
//! interruption artefact and not an exploit: the player cannot *choose* what is in the
//! slot, which is what designs save-scumming out rather than policing it.
//!
//! # Why the policy is a value and not a pile of callbacks
//!
//! [`Autosave`] decides *what should happen* and answers with a [`Write`]; the caller
//! does it. That split is what makes the boundary testable natively: the policy runs
//! against a [`Slot`] and a [`Timer`](crate::timer::Timer) that are traits, so a test
//! drives a burst of turns through the real policy and **counts the writes** the shell
//! would have made, with no browser anywhere near it (#514's acceptance criterion). The
//! browser implementations of the two traits are the only part that needs a page — and
//! the clock is shared with the other timed surface the shell owns
//! ([`crate::timer`]).

use std::rc::{Rc, Weak};

use intrusion_core::{parse_script, to_script, Campaign, LevelSeed, MapUi, RunOptions, State};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::Storage;

use crate::menu::{SCREEN_MAP, SCREEN_PLAY};
use crate::seed;
use crate::timer::{Timer, WindowTimer};

/// The key the run's save lives under. One slot, and one key: **the settings record
/// (#513) keeps its own**, because ending a run must never reset a preference.
const KEY: &str = "intrusion:run";

/// The snapshot format's version, carried in the record and checked on the way in.
///
/// It is the *deliberate* half of the compatibility check. The accidental half comes
/// free: a save is a serialised [`State`], so a build whose state has a different
/// shape fails to decode on its own. This number is for the change that keeps the
/// shape and moves the meaning — bump it and every save in the wild is discarded,
/// which is the §12.6 "never a bricked page" rule applied to storage.
const FORMAT: u32 = 1;

/// How long after the last turn the trailing write lands. **[START]** — §12.5 asks
/// for "order of a few seconds", and two of them is comfortably longer than the
/// 120 ms auto-repeat that makes a burst a burst, so a held key writes once at the
/// end rather than all the way down.
pub(crate) const DEBOUNCE_MS: i32 = 2_000;

/// Turns with nothing written before a turn stops waiting for the clock and writes.
/// **[START]** — at the 120 ms repeat, twenty turns is about [`DEBOUNCE_MS`] of held
/// key, so the cap bites only during a sustained hold and bounds what a killed tab can
/// cost in *turns* rather than only in seconds.
pub(crate) const TURN_CAP: u32 = 20;

/// A run as it is stored: the world, the things about the run that live outside it,
/// and the stamp that says whether this build may read it back.
///
/// The [`State`] is the bulk of it and the reason this is a snapshot at all. What sits
/// beside it is what the *shell* owns and the state does not: the campaign layer above
/// the facility (§12.7), the run's framing, the row of the map screen if that is where
/// the player was, and the input recording.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Save {
    /// The format stamp ([`FORMAT`]).
    version: u32,
    /// The world, exactly as of the write.
    pub(crate) state: State,
    /// The level the run booted from (§12.4/#245) — so a resumed run's Level info tab
    /// still shows, and copies, the same level-seed token it always did.
    pub(crate) level: LevelSeed,
    /// The run's framing (§14 v2/#138): what the end screen will offer, and what *new
    /// run* re-rolls at.
    pub(crate) options: RunOptions,
    /// The campaign this run is part of (§14 v3/§12.7), or `None` in quick play.
    pub(crate) campaign: Option<Campaign>,
    /// The map screen's marker row when the run was **on the map** between raids, or
    /// `None` when it was inside a facility. Only the row: the map's other view field
    /// is what the hub last said back, and a purchase message restored hours later
    /// would be the screen answering a question nobody had just asked.
    pub(crate) map_row: Option<usize>,
    /// Every input this run has been fed, in the §12.4 replay notation.
    ///
    /// A snapshot does not need it to restore — that is the point of a snapshot — but
    /// the shell's recorder does (§12.4/#411): a resumed run whose recording started
    /// empty would hand the copy-replay control a script that reproduces a *different*
    /// run, silently. Stored as the notation rather than as inputs so it costs a few
    /// bytes a turn and is readable in the slot.
    pub(crate) script: String,
}

impl Save {
    /// A record of the run as it stands. The version stamp is this build's by
    /// construction — a save is only ever written by the build that is running.
    pub(crate) fn new(
        state: State,
        level: LevelSeed,
        options: RunOptions,
        campaign: Option<Campaign>,
        map_row: Option<usize>,
        script: String,
    ) -> Self {
        Self {
            version: FORMAT,
            state,
            level,
            options,
            campaign,
            map_row,
            script,
        }
    }
}

/// Decode a stored record, or `None` if this build cannot read it.
///
/// **Every failure is the same answer** (§12.6's rule for the level-seed token, and
/// #514's for saves): a record from an older build, a truncated write, a hand-edited
/// slot — all of them are simply *no save*, and the menu shows the entries it always
/// had. Nothing here reports an error, because there is no one to report it to and
/// nothing the player could do about it.
pub(crate) fn decode(text: &str) -> Option<Save> {
    let save: Save = serde_json::from_str(text).ok()?;
    (save.version == FORMAT).then_some(save)
}

/// Encode a record for the slot, or `None` if it somehow will not serialise.
pub(crate) fn encode(save: &Save) -> Option<String> {
    serde_json::to_string(save).ok()
}

/// The **storage boundary**: where a save leaves the page, and the seam the policy is
/// tested through. Three calls, no formats and no policy — the browser's storage is
/// one implementation of it and a test's counter is another.
pub(crate) trait Slot {
    /// What is in the slot, if anything.
    fn read(&self) -> Option<String>;
    /// Put `value` in the slot, replacing whatever was there — answering **whether it
    /// landed**.
    ///
    /// A refusal is not exotic: a browser can be out of storage quota, or be one that
    /// hands out a `localStorage` which throws on every write. The answer is what the
    /// policy reacts to, so a write that did not happen is not mistaken for one that
    /// did ([`Autosave::store`]).
    fn write(&self, value: &str) -> bool;
    /// Empty the slot.
    fn clear(&self);
}

/// What a moment in the run asks of the slot — the policy's whole vocabulary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Write {
    /// Nothing to do now. Either a trailing write is armed and will do it, or there is
    /// nothing to write.
    Later,
    /// Write the run to the slot, now.
    Now,
    /// Empty the slot, now — the run is over.
    Clear,
}

/// The shell's save slot and its write policy (see the module docs).
pub(crate) struct Autosave {
    slot: Box<dyn Slot>,
    timer: Box<dyn Timer>,
    /// Turns since the last write. The [`TURN_CAP`] half of the policy, and the answer
    /// to "is there anything worth flushing?" — a page-hide with nothing pending must
    /// not rewrite a slot the run has not moved since.
    pending: u32,
}

impl Autosave {
    pub(crate) fn new(slot: Box<dyn Slot>, timer: Box<dyn Timer>) -> Self {
        Self {
            slot,
            timer,
            pending: 0,
        }
    }

    /// A save-worthy moment passed: a turn resolved, or the run moved on the map.
    /// `finished` says whether the **run** is over — not the facility, which in a
    /// campaign is the middle of the run (§12.7).
    pub(crate) fn moment(&mut self, finished: bool) -> Write {
        if finished {
            // Synchronously, and before anything can be armed over it: the slot must
            // never hold a pre-death state (#514), which is the whole of why a capture
            // cannot be un-died.
            self.timer.cancel();
            self.pending = 0;
            return Write::Clear;
        }
        self.pending += 1;
        if self.pending >= TURN_CAP {
            self.timer.cancel();
            self.pending = 0;
            return Write::Now;
        }
        self.timer.arm(DEBOUNCE_MS);
        Write::Later
    }

    /// Write whatever is outstanding, now — the trailing timer firing, or the page
    /// going away. Answers [`Write::Later`] when nothing has happened since the last
    /// write, so a hidden tab does not rewrite a slot nobody moved.
    pub(crate) fn flush(&mut self) -> Write {
        if self.pending == 0 {
            return Write::Later;
        }
        self.timer.cancel();
        self.pending = 0;
        Write::Now
    }

    /// Forget any outstanding write without performing it — a fresh run replacing the
    /// world. The slot itself is left alone: it is overwritten *forward*, by the new
    /// run's own first write, so a mis-started run does not empty the slot before it
    /// has anything to put in it.
    pub(crate) fn reset(&mut self) {
        self.timer.cancel();
        self.pending = 0;
    }

    /// Store a record, encoding it on the way out.
    ///
    /// **A write that did not land stays outstanding.** The storage itself cannot
    /// half-replace a record — `localStorage` stores a string atomically, so a refused
    /// write leaves the previous *complete* save exactly where it was, which is the
    /// safe direction — but a refusal must not be mistaken for a save. Left as
    /// pending, the next moment writes again, and, more to the point, the page-hide
    /// flush still has something to flush: without this a quota refusal would answer
    /// "nothing outstanding" a moment later and the run would be silently frozen at
    /// whatever older record the slot still held.
    pub(crate) fn store(&mut self, save: &Save) {
        let stored = encode(save).is_some_and(|text| self.slot.write(&text));
        if !stored {
            self.pending = self.pending.max(1);
        }
    }

    /// Empty the slot.
    pub(crate) fn clear(&self) {
        self.slot.clear();
    }
}

/// The save half of [`Game`](crate::Game) — the *when*, beside [`Autosave`]'s *what*.
impl crate::Game {
    /// A save-worthy moment passed: a turn resolved ([`step_and_draw`]), or the run
    /// moved on the map without one (a raid banked, a road bought). Everything that
    /// changes what a player would come back to comes through here, and the policy
    /// decides whether that means a write now, a write shortly, or an empty slot.
    ///
    /// [`step_and_draw`]: crate::Game::step_and_draw
    pub(crate) fn autosave_moment(&mut self) {
        if !self.autosaving() {
            return;
        }
        let write = self.autosave.moment(self.run_finished());
        self.perform(write);
    }

    /// Write anything outstanding immediately — the trailing timer firing, or the page
    /// going away ([`install`]).
    pub(crate) fn flush_save(&mut self) {
        if !self.autosaving() {
            return;
        }
        let write = self.autosave.flush();
        self.perform(write);
    }

    /// Do what the policy asked for. The snapshot is taken **here**, at the moment of
    /// writing, rather than kept up to date turn by turn — a debounced write's whole
    /// point is that the turns in between cost nothing.
    fn perform(&mut self, write: Write) {
        match write {
            Write::Later => {}
            Write::Now => {
                let record = self.record();
                self.autosave.store(&record);
            }
            Write::Clear => self.autosave.clear(),
        }
    }

    /// Whether this page has a run of its own to store at all. A **replay** does not
    /// (§12.4/#197): it is a pure view of someone else's run, its state is derived from
    /// a cursor, and a scrub is not a turn.
    fn autosaving(&self) -> bool {
        self.replay.is_none()
    }

    /// Whether the **run** is over — the moment the slot is emptied (§2.2).
    ///
    /// Not the same question as whether the *facility* is over. In a campaign an
    /// escaped facility is the middle of the run (§12.7/#208), so only the campaign's
    /// own stage may end it; in quick play the two are the same event.
    fn run_finished(&self) -> bool {
        match &self.campaign {
            Some(run) => run.stage().is_over(),
            None => self.run_over(),
        }
    }

    /// The run as it stands, ready for the slot.
    fn record(&self) -> Save {
        Save::new(
            self.state.clone(),
            self.level,
            self.ui.end.options,
            self.campaign.clone(),
            // Where the player is *standing*: on the map between raids, or inside the
            // facility. Restoring one to the other would be a different screen than the
            // one they left.
            self.map_open()
                .then(|| self.ui.map.unwrap_or_default().selected),
            to_script(&self.recorded),
        )
    }

    /// **Resume the saved run** (§12.5/#514) — the title screen's *Continue run*, and
    /// the reload path ([`crate::resumes_in_place`]).
    ///
    /// The mirror of [`reseed`](crate::Game::reseed): that one drops everything a run
    /// owns and keeps the few facts about the *player and the page* that outlive it,
    /// and so does this — the theme, the modality and the debug session all carry, and
    /// everything else comes from the record. The save is **taken**, not copied: it has
    /// been resumed, and the slot is the live run's from here on.
    pub(crate) fn continue_run(&mut self) {
        let Some(save) = self.resume.take() else {
            return;
        };
        // The debug switches are the *session's*, never the save's (§12.6/#459): a run
        // stored with omni-vision on must not turn it on for whoever loads next, and a
        // watcher who has it on keeps it across the resume.
        self.state = save.state.with_debug(self.state.debug());
        self.level = save.level;
        self.campaign = save.campaign;
        // The recording comes back with the run (§12.4/#411), so the copy-replay control
        // still hands over a script that reproduces *this* run rather than one that
        // starts from the resume. A script this build cannot parse costs the recording
        // and not the run — the same fall-through a bad token gets (#110).
        self.recorded = parse_script(&save.script).unwrap_or_default();
        self.ui = self.ui.for_fresh_run();
        // …but **not** the level-start card (§11.4/#497), which that seam raises for a
        // *fresh facility*: a resumed run is one already underway, and a card that said
        // "any key to begin" over a raid three rooms deep would be describing a moment
        // that has passed. The run's rules stay a keypress away on the Level info tab,
        // which is where a player who has been away goes to re-read them.
        self.ui.splash_open = false;
        self.ui.end.options = save.options;
        self.ui.map = save.map_row.map(|selected| MapUi {
            selected,
            ..MapUi::default()
        });
        // Nothing is outstanding: what is in the slot *is* the world now on screen.
        self.autosave.reset();
        seed::reflect_level(&self.level);
        crate::menu::set_screen(if self.map_open() {
            SCREEN_MAP
        } else {
            SCREEN_PLAY
        });
        self.fit_and_draw();
    }
}

/// The browser's `localStorage`, or nothing at all.
///
/// **Absence is ordinary, not an error.** Private-browsing modes and framed pages can
/// refuse storage outright — the getter *throws* rather than answering `None` — and a
/// page without it simply never saves and never offers to continue. That is the same
/// shape as the clipboard's absence ([`crate::clipboard`]) and it is handled the same
/// way: ask once, hold the answer, and let every call be a no-op if the answer was no.
struct LocalSlot {
    store: Option<Storage>,
}

impl LocalSlot {
    fn boot() -> Self {
        Self {
            store: web_sys::window().and_then(|w| w.local_storage().ok().flatten()),
        }
    }
}

impl Slot for LocalSlot {
    fn read(&self) -> Option<String> {
        self.store.as_ref()?.get_item(KEY).ok().flatten()
    }

    fn write(&self, value: &str) -> bool {
        // `setItem` is **atomic**: it stores the whole string or throws, so the slot
        // never holds half a record and a refusal leaves the previous complete one
        // untouched (measured against Chromium at quota, not assumed). What the caller
        // needs back is only which of the two happened.
        self.store
            .as_ref()
            .is_some_and(|store| store.set_item(KEY, value).is_ok())
    }

    fn clear(&self) {
        let Some(store) = self.store.as_ref() else {
            return;
        };
        let _ = store.remove_item(KEY);
        if store.get_item(KEY).ok().flatten().is_none() {
            return;
        }
        // The removal did not take — the one storage failure that would be a **rule**
        // broken rather than a save lost, since a slot still holding the run would let
        // a capture be reloaded away (§2.2). Poison it instead: an empty value is not a
        // record, so it decodes to nothing and the menu offers no continue row. Belt
        // and braces — `removeItem` has no quota to exceed and should never need this.
        let _ = store.set_item(KEY, "");
    }
}

/// The shell's live autosave, wired to the browser.
pub(crate) fn browser(game: Weak<std::cell::RefCell<crate::Game>>) -> Autosave {
    Autosave::new(
        Box::new(LocalSlot::boot()),
        // The trailing write, on the shell's shared one-shot clock ([`crate::timer`]):
        // arming replaces, which is what makes the policy above a debounce and not a
        // queue of writes.
        Box::new(WindowTimer::new(game, |game| game.flush_save())),
    )
}

/// The run in storage at boot, if there is one this build can read — asked once, before
/// the shell decides whether the title screen carries a *Continue run* row.
pub(crate) fn stored() -> Option<Save> {
    decode(&LocalSlot::boot().read()?)
}

/// Install the **page-hide flush**: the immediate write that catches the player
/// leaving, which on a phone is how a session usually ends.
///
/// Two events, because neither is reliable alone. `visibilitychange` fires when the tab
/// is backgrounded or the app is switched away from — the common case, and the one a
/// mobile browser is most likely to kill afterwards without another word;
/// `pagehide` catches the navigation and close paths. Both land on the same flush,
/// which is idempotent: the second one finds nothing pending and writes nothing.
///
/// `unload`/`beforeunload` are deliberately not among them — mobile browsers do not
/// reliably fire them, and a policy that leans on an event that may not arrive is a
/// policy that loses runs.
pub(crate) fn install(
    document: &web_sys::Document,
    game: &Rc<std::cell::RefCell<crate::Game>>,
) -> Result<(), JsValue> {
    {
        let game = game.clone();
        let hidden = document.clone();
        let cb = Closure::<dyn FnMut()>::new(move || {
            if hidden.hidden() {
                game.borrow_mut().flush_save();
            }
        });
        document
            .add_event_listener_with_callback("visibilitychange", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }
    {
        let game = game.clone();
        let cb = Closure::<dyn FnMut()>::new(move || game.borrow_mut().flush_save());
        let win = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        win.add_event_listener_with_callback("pagehide", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use intrusion_core::{start_level, to_script, Difficulty, Direction, Input, RunMode};

    use super::*;

    /// A slot that keeps its value in memory and **counts what crossed it** — the
    /// boundary the write policy is asserted through.
    #[derive(Default)]
    struct MemorySlot {
        value: RefCell<Option<String>>,
        writes: Cell<u32>,
        clears: Cell<u32>,
        /// Whether the slot refuses writes — a browser at its storage quota.
        refuse: Cell<bool>,
    }

    impl Slot for Rc<MemorySlot> {
        fn read(&self) -> Option<String> {
            self.value.borrow().clone()
        }

        fn write(&self, value: &str) -> bool {
            if self.refuse.get() {
                return false;
            }
            *self.value.borrow_mut() = Some(value.to_string());
            self.writes.set(self.writes.get() + 1);
            true
        }

        fn clear(&self) {
            *self.value.borrow_mut() = None;
            self.clears.set(self.clears.get() + 1);
        }
    }

    /// A timer that records arming and cancelling instead of asking the browser for
    /// one. Firing is the test calling [`Autosave::flush`] — which is exactly what the
    /// real timer's callback does.
    #[derive(Default)]
    struct FakeTimer {
        armed: Cell<u32>,
        cancelled: Cell<u32>,
    }

    impl Timer for Rc<FakeTimer> {
        fn arm(&self, _ms: i32) {
            self.armed.set(self.armed.get() + 1);
        }

        fn cancel(&self) {
            self.cancelled.set(self.cancelled.get() + 1);
        }
    }

    /// The policy wired to a counted slot and a fake clock.
    fn rig() -> (Autosave, Rc<MemorySlot>, Rc<FakeTimer>) {
        let slot = Rc::new(MemorySlot::default());
        let timer = Rc::new(FakeTimer::default());
        let auto = Autosave::new(Box::new(slot.clone()), Box::new(timer.clone()));
        (auto, slot, timer)
    }

    /// A run to snapshot: a real level, stepped a few turns so nothing about it is a
    /// default value.
    fn run() -> (State, LevelSeed, Vec<Input>) {
        let level = LevelSeed::quick_play(7);
        let mut state = start_level(&level).expect("the v1 footprint always carves");
        let inputs = vec![
            Input::Step(Direction::North),
            Input::Wait,
            Input::Step(Direction::East),
            Input::Step(Direction::East),
            Input::Wait,
        ];
        for &input in &inputs {
            state.step(input);
        }
        (state, level, inputs)
    }

    fn record(state: State, level: LevelSeed, inputs: &[Input]) -> Save {
        Save::new(
            state,
            level,
            RunOptions {
                mode: RunMode::QuickPlay,
                difficulty: Difficulty::Standard,
            },
            None,
            None,
            to_script(inputs),
        )
    }

    /// **A burst of turns is one write, not one per turn** (#514) — the criterion the
    /// storage boundary exists to make assertable. Ten turns in a row arm the trailing
    /// timer and cross the boundary not once; the timer firing afterwards writes once.
    #[test]
    fn a_burst_of_turns_makes_one_trailing_write() {
        let (mut auto, slot, timer) = rig();
        for turn in 0..10 {
            assert_eq!(auto.moment(false), Write::Later, "turn {turn} defers");
        }
        assert_eq!(slot.writes.get(), 0, "a burst writes nothing while it runs");
        assert_eq!(timer.armed.get(), 10, "each turn re-arms the one timer");

        assert_eq!(auto.flush(), Write::Now);
        auto.store(&record(run().0, run().1, &[]));
        assert_eq!(slot.writes.get(), 1, "the burst lands as exactly one write");
    }

    /// The **turn cap** bounds the at-risk window in turns as well as in seconds: a
    /// hold long enough to reach it stops waiting for the clock and writes, then goes
    /// back to debouncing.
    #[test]
    fn a_long_hold_writes_at_the_turn_cap() {
        let (mut auto, _slot, _timer) = rig();
        for turn in 1..TURN_CAP {
            assert_eq!(auto.moment(false), Write::Later, "turn {turn} defers");
        }
        assert_eq!(auto.moment(false), Write::Now, "the capping turn writes");
        assert_eq!(
            auto.moment(false),
            Write::Later,
            "and the count starts over"
        );
    }

    /// **A flush with nothing pending writes nothing.** The page can hide and come
    /// back any number of times between turns; the slot is only touched when the run
    /// has actually moved.
    #[test]
    fn a_flush_with_nothing_pending_writes_nothing() {
        let (mut auto, slot, _timer) = rig();
        assert_eq!(auto.flush(), Write::Later);
        auto.moment(false);
        assert_eq!(auto.flush(), Write::Now);
        auto.store(&record(run().0, run().1, &[]));
        assert_eq!(auto.flush(), Write::Later, "a second hide adds nothing");
        assert_eq!(slot.writes.get(), 1);
    }

    /// **A terminal turn empties the slot as part of resolving it** — synchronously,
    /// and cancelling the pending write rather than letting it land afterwards. That
    /// pairing is the whole of "a capture cannot be un-died": there is no window in
    /// which the slot holds the turn before the one that ended the run.
    #[test]
    fn a_terminal_turn_empties_the_slot_and_cancels_the_pending_write() {
        let (mut auto, slot, timer) = rig();
        auto.moment(false);
        assert_eq!(auto.flush(), Write::Now);
        auto.store(&record(run().0, run().1, &[]));
        assert!(slot.read().is_some());

        auto.moment(false); // a turn since the write, so one is pending
        let cancelled = timer.cancelled.get();
        assert_eq!(auto.moment(true), Write::Clear);
        auto.clear();
        assert!(
            slot.read().is_none(),
            "the slot is empty the moment it ends"
        );
        assert!(
            timer.cancelled.get() > cancelled,
            "the pending write is cancelled, not left to fire over the empty slot",
        );
        assert_eq!(auto.flush(), Write::Later, "and nothing is outstanding");
    }

    /// **A refused write stays outstanding.** A browser at its storage quota throws
    /// rather than storing half a record, so the *previous* complete save survives —
    /// but the shell must not take the refusal for a save. The run stays pending, so
    /// the next moment tries again and, crucially, the page-hide flush still has
    /// something to flush instead of answering "nothing to do" over a stale slot.
    #[test]
    fn a_refused_write_stays_outstanding_and_is_retried() {
        let (mut auto, slot, _timer) = rig();
        let save = record(run().0, run().1, &[]);

        slot.refuse.set(true);
        auto.moment(false);
        assert_eq!(auto.flush(), Write::Now);
        auto.store(&save);
        assert_eq!(slot.writes.get(), 0, "the slot refused it");
        assert!(slot.read().is_none(), "and holds nothing it did not accept");
        assert_eq!(
            auto.flush(),
            Write::Now,
            "the refused write is still outstanding, so the page-hide flush retries it",
        );

        slot.refuse.set(false);
        auto.store(&save);
        assert_eq!(
            slot.writes.get(),
            1,
            "the retry lands once the slot accepts"
        );
        assert_eq!(
            auto.flush(),
            Write::Later,
            "and nothing is left outstanding"
        );
    }

    /// **Snapshot → restore → the same state**, and the same run from there on: the
    /// restored world, fed the inputs the original is fed, stays byte-for-byte the
    /// world the original becomes (§12.4).
    #[test]
    fn a_snapshot_restores_the_run_exactly() {
        let (state, level, inputs) = run();
        let text = encode(&record(state.clone(), level, &inputs)).expect("a run serialises");
        let restored = decode(&text).expect("this build reads its own writes");

        let shape = |state: &State| serde_json::to_string(state).expect("a state serialises");
        assert_eq!(
            shape(&restored.state),
            shape(&state),
            "identical on restore"
        );
        assert_eq!(restored.level, level, "the level-seed token survives");
        assert_eq!(
            restored.script,
            to_script(&inputs),
            "the recording survives"
        );

        let mut original = state;
        let mut resumed = restored.state;
        for &input in &[
            Input::Step(Direction::South),
            Input::Wait,
            Input::Step(Direction::West),
        ] {
            original.step(input);
            resumed.step(input);
            assert_eq!(shape(&resumed), shape(&original), "and identical after it");
        }
    }

    /// **A save fits in a browser's slot with room to spare.** `localStorage` is a
    /// few megabytes per origin and one run's snapshot of a 40×40 facility is a couple
    /// of hundred kilobytes of it, so the margin is two orders of magnitude — this is
    /// the guard on that staying true, since the whole feature quietly stops working
    /// if a write starts hitting the quota.
    #[test]
    fn a_save_is_small_enough_for_the_slot() {
        let level = LevelSeed::quick_play(7);
        let mut state = start_level(&level).expect("the v1 footprint always carves");
        let mut inputs = Vec::new();
        for turn in 0..300 {
            let input = if turn % 3 == 0 {
                Input::Wait
            } else {
                Input::Step(Direction::North)
            };
            inputs.push(input);
            state.step(input);
        }
        let text = encode(&record(state, level, &inputs)).expect("a long run serialises");
        assert!(
            text.len() < 1_000_000,
            "a run's snapshot is {} bytes, which is no longer comfortably inside a \
             browser's storage quota",
            text.len(),
        );
    }

    /// A **campaign** run rides the record too (§12.7): the layer above the facility,
    /// and the map row when the run was between raids rather than inside one.
    #[test]
    fn a_snapshot_carries_the_campaign_layer() {
        let (state, level, _) = run();
        let campaign = Campaign::new(11);
        let save = Save::new(
            state,
            level,
            RunOptions::default(),
            Some(campaign.clone()),
            Some(2),
            String::new(),
        );
        let restored = decode(&encode(&save).expect("a campaign serialises")).expect("reads back");
        assert_eq!(restored.campaign, Some(campaign));
        assert_eq!(restored.map_row, Some(2), "the map's marker row comes back");
    }

    /// **A save this build cannot read is no save at all** (#514): a record from
    /// another format version, and anything that is not a record, both answer `None`
    /// rather than erroring — the menu then simply shows the entries it always had.
    #[test]
    fn a_save_from_another_build_is_discarded() {
        let (state, level, inputs) = run();
        let text = encode(&record(state, level, &inputs)).expect("a run serialises");
        assert!(decode(&text).is_some(), "this build's own write reads back");

        let stale = text.replacen(
            &format!("\"version\":{FORMAT}"),
            &format!("\"version\":{}", FORMAT + 1),
            1,
        );
        assert_ne!(stale, text, "the stamp is in the record to be found");
        assert!(decode(&stale).is_none(), "another version is discarded");

        for junk in ["", "{}", "not json at all", "{\"version\":1}"] {
            assert!(decode(junk).is_none(), "{junk:?} is discarded");
        }
    }
}
