//! The tunnel walk's **clock** (§4.5/§11.1, #466) — the game's first animation, and
//! deliberately its smallest possible one.
//!
//! A run used to open with `@` already standing somewhere on forty by forty glyphs
//! and nothing on the board to draw the eye to it. §1 says the intruder dug the
//! tunnel and came up through it, so the fix is the beat the fiction already
//! describes: the player is drawn coming out of the hole at `E` and **walks** to
//! their spawn cell before control begins. A marker would say *where you are*; the
//! walk says *how you got there*, and leaves the eye exactly where the player will
//! be standing when the game hands over.
//!
//! # What this module is allowed to be
//!
//! **The world is frozen for the whole beat.** Nothing here steps the loop, moves a
//! guard, or advances a clock the game can read: the turn counter is 0 when the walk
//! starts and 0 when control begins, and turn zero's frame is byte-identical whether
//! the walk played, was skipped, or never ran at all (pinned in `core::render`'s
//! tests). That is what lets an animation exist without weakening §11.1 — the
//! renderer stays a pure function of `(state, ui)`, and all this owns is a `f64`.
//!
//! So the split is: **which cells** is the core's ([`tunnel_walk`], a pure function
//! of state), **which glyph goes where** is the core's ([`ScreenUi::walk`], the one
//! thing the shell may say about the picture), and **when** is this module's. The
//! shell decides no glyph, exactly as everywhere else (§11.1).
//!
//! # Skippable, and that is the feature
//!
//! Five seconds is a long time on the twentieth run, so any key and any press ends
//! the walk on the spot and hands over control — and the input that skipped it is
//! **not** also played as a game action (§11.6: no screen may trap, and a wait every
//! player has to sit through is a trap). The skip is wired at the two seams every
//! input crosses, [`Game::handle_key`](crate::Game) and the gesture pump's press, so
//! there is no third way in that could miss it.
//!
//! # Inert everywhere it should be
//!
//! The clock is installed on the live-play boot path only: a **replay** re-runs
//! recorded inputs (§12.4) and never animates, the title screen has no board to walk
//! on, and the headless sim never links this crate at all. [`ScreenUi::walk`]
//! defaults to `None`, so every other frame in the game is drawn exactly as before.
//!
//! # The departure will reuse all of it
//!
//! [`tunnel_walk`] is directionless — reverse its cells and it is the walk back down
//! the hole at the end of a run (§4.5). [`Walk`] takes a path and knows nothing about
//! which end of the level it came from, so the exit animation needs a second caller
//! and no second timeline.

use std::cell::{Cell as CellFlag, RefCell};
use std::rc::{Rc, Weak};

use intrusion_core::{tunnel_walk, Cell};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::Game;

/// The beat spent standing on `E` before the first step — coming up out of the hole.
/// Long enough for the eye to find the one thing that just appeared, short enough
/// that it reads as a pause rather than a stall.
const EMERGE_MS: f64 = 700.0;

/// The beat spent standing on the spawn cell after the last step, before control
/// begins. It is what stops the hand-over landing on the same frame the movement
/// stops: the eye arrives, *then* the game is yours.
const ARRIVE_MS: f64 = 500.0;

/// How long the walking itself is aiming to take — the ticket's "roughly 5 seconds"
/// less the two beats either side of it. It is a **target**, divided by however many
/// steps this level's walk turned out to be and then clamped: the distance from the
/// exit to the spawn is a generated number (`PLACE`'s `PLAYER_EXIT_MIN_DISTANCE` is a
/// floor, not a length), so a fixed per-step pace would make a short walk perfunctory
/// and a long one interminable.
const WALK_MS: f64 = 3_800.0;

/// The floor and ceiling on one step's duration. The floor keeps a long walk from
/// becoming a blur the eye cannot follow a cell of; the ceiling keeps a short one
/// from crawling. Between them a walk lands within about a second of [`WALK_MS`] for
/// every distance this generator produces.
const STEP_MIN_MS: f64 = 150.0;
const STEP_MAX_MS: f64 = 380.0;

/// One playing tunnel walk: the cells to cross, the pace, and where in it we are.
///
/// Pure — it takes a timestamp and answers with a cell — so the whole timeline is
/// pinned by native tests below and the browser half of this module is only
/// `requestAnimationFrame` plumbing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Walk {
    /// The route, both ends included: `[exit, …, spawn]` on the way in, and the
    /// reverse of it on the way out.
    path: Vec<Cell>,
    /// How long one step of `path` is held, derived once in [`Walk::begin`].
    step_ms: f64,
    /// The timestamp of the walk's first frame, once one has arrived. Taken from the
    /// frame callback rather than a clock of our own, so the beat is measured in the
    /// browser's own presentation time and a page that was backgrounded at boot
    /// starts the walk when it is next actually drawn.
    origin: Option<f64>,
}

impl Walk {
    /// Begin a walk over `path`, or `None` when there is nothing to watch: an empty
    /// path (no route — §10.6 says this cannot happen on a level that ships) or a
    /// one-cell one (the player is already standing on the hole). A caller with
    /// `None` simply plays no animation and hands over control immediately, which is
    /// the correct degradation: the opening beat is a flourish, never a gate.
    pub(crate) fn begin(path: Vec<Cell>) -> Option<Self> {
        let steps = path.len().checked_sub(1).filter(|&s| s > 0)?;
        Some(Self {
            step_ms: (WALK_MS / steps as f64).clamp(STEP_MIN_MS, STEP_MAX_MS),
            path,
            origin: None,
        })
    }

    /// How long the whole beat runs: the emergence, the steps, the arrival.
    fn total_ms(&self) -> f64 {
        EMERGE_MS + (self.path.len() - 1) as f64 * self.step_ms + ARRIVE_MS
    }

    /// Where the `@` is shown `elapsed` milliseconds in, or `None` once the beat is
    /// over and the walk should be cleared.
    ///
    /// The player holds on the first cell through the emergence, takes one cell per
    /// [`step_ms`](Self::step_ms) after it, and then stands on the last cell for the
    /// arrival — during which this already answers with the player's real cell, so
    /// the last stretch of the animation is drawing precisely today's frame.
    fn cell_at(&self, elapsed: f64) -> Option<Cell> {
        if elapsed >= self.total_ms() {
            return None;
        }
        let walked = ((elapsed - EMERGE_MS) / self.step_ms).floor();
        let index = if walked < 0.0 {
            0
        } else {
            (walked as usize + 1).min(self.path.len() - 1)
        };
        Some(self.path[index])
    }

    /// Advance to the frame presented at `now` and answer where to draw the `@`, or
    /// `None` when the beat has finished. The first frame to arrive fixes the
    /// origin — no timestamp is taken anywhere else, so nothing about the walk
    /// depends on a clock the shell had to read.
    pub(crate) fn frame(&mut self, now: f64) -> Option<Cell> {
        let origin = *self.origin.get_or_insert(now);
        self.cell_at((now - origin).max(0.0))
    }
}

/// The `requestAnimationFrame` callback's type: the browser hands each frame its
/// own presentation timestamp, which is the only clock this animation reads.
type FrameCallback = Closure<dyn FnMut(f64)>;

/// The shell's frame clock: one `requestAnimationFrame` closure, kept alive for the
/// page and driving whichever walk the game currently has.
///
/// One clock for the page rather than one per run, because a run can be restarted
/// from the end screen any number of times and each restart would otherwise leak its
/// own closure. Like the gesture pump's timer callback it holds itself (the closure
/// captures the `Rc<Clock>` that owns it) and is **deliberately never freed** — the
/// page needs it until the page is gone. Its handle on the game is [`Weak`], so a
/// frame that arrives after the shell is gone finds nothing and does nothing.
pub(crate) struct Clock {
    game: Weak<RefCell<Game>>,
    frame: RefCell<Option<FrameCallback>>,
    /// Whether a frame is already requested. Without it a restart mid-walk would
    /// request a second frame while the first is still in flight, and each of those
    /// would request its own successor — the request count doubling every frame.
    pending: CellFlag<bool>,
}

impl Clock {
    /// Build the page's clock and hand it back. It requests nothing yet: a walk asks
    /// for frames ([`Clock::start`]), and between walks the clock costs nothing at all.
    pub(crate) fn install(game: &Rc<RefCell<Game>>) -> Rc<Self> {
        let clock = Rc::new(Self {
            game: Rc::downgrade(game),
            frame: RefCell::new(None),
            pending: CellFlag::new(false),
        });
        let handle = clock.clone();
        *clock.frame.borrow_mut() = Some(FrameCallback::new(move |now: f64| {
            handle.pending.set(false);
            let Some(game) = handle.game.upgrade() else {
                return; // the page is gone; there is nothing left to draw
            };
            if game.borrow_mut().walk_frame(now) {
                handle.request();
            }
        }));
        clock
    }

    /// Start the walk the game has just been given: draw its first cell, then ask for
    /// frames until it finishes.
    pub(crate) fn start(&self) {
        self.request();
    }

    /// Ask for the next frame, unless one is already coming.
    fn request(&self) {
        if self.pending.get() {
            return;
        }
        let frame = self.frame.borrow();
        let Some(closure) = frame.as_ref() else {
            return;
        };
        let Some(win) = web_sys::window() else {
            return;
        };
        if win
            .request_animation_frame(closure.as_ref().unchecked_ref())
            .is_ok()
        {
            self.pending.set(true);
        }
    }
}

impl Game {
    /// Play the opening walk for the run that has just started (§4.5/#466): the
    /// player comes up at the exit and walks to where placement put them.
    ///
    /// A level with no walk to play — nothing routes, or the spawn *is* the hole —
    /// simply starts, so this is safe to call on every fresh run without asking first.
    pub(crate) fn begin_tunnel_walk(&mut self) {
        self.walk = Walk::begin(tunnel_walk(&self.state));
        let Some(walk) = &self.walk else {
            return;
        };
        // Draw the `@` on the hole before the first frame arrives, so the beat never
        // opens on a frame with the player already standing at their spawn.
        self.ui.walk = Some(walk.path[0]);
        self.draw();
        if let Some(clock) = self.clock.clone() {
            clock.start();
        }
    }

    /// One frame of the walk. Returns whether the beat is still running — that is,
    /// whether the clock should ask for another frame.
    pub(crate) fn walk_frame(&mut self, now: f64) -> bool {
        let Some(walk) = self.walk.as_mut() else {
            return false;
        };
        match walk.frame(now) {
            Some(cell) => {
                // Repaint only when the `@` actually moved: the clock ticks at the
                // display's rate and the walk steps a few times a second, so most
                // frames have nothing to say.
                if self.ui.walk != Some(cell) {
                    self.ui.walk = Some(cell);
                    self.draw();
                }
                true
            }
            None => {
                self.end_tunnel_walk();
                false
            }
        }
    }

    /// End the walk *now* and hand over control — the skip (§11.6). Returns whether
    /// there was a walk to end, which is how the input pumps know to swallow the
    /// press: the input that skips the beat must not also be played as a game action.
    pub(crate) fn skip_tunnel_walk(&mut self) -> bool {
        if self.walk.is_none() {
            return false;
        }
        self.end_tunnel_walk();
        true
    }

    /// Drop the walk and repaint the frame the game was always going to hand over —
    /// the `@` back on the player's own cell, which is where the walk was heading.
    /// Reached both by the beat finishing and by the skip, so the two land on exactly
    /// the same frame.
    fn end_tunnel_walk(&mut self) {
        self.walk = None;
        self.ui.walk = None;
        self.draw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A straight walk of `steps` steps, west to east along row 0.
    fn path(steps: u32) -> Vec<Cell> {
        (0..=steps).map(|x| Cell::new(x, 0)).collect()
    }

    /// The shape of the beat: hold on the hole, one cell per step, hold on the spawn,
    /// then `None` — the signal the clock stops on.
    #[test]
    fn the_walk_emerges_steps_and_then_stands_still() {
        let mut walk = Walk::begin(path(4)).expect("four steps is a walk");
        let step = walk.step_ms;

        assert_eq!(walk.frame(1_000.0), Some(Cell::new(0, 0)), "up the hole");
        assert_eq!(
            walk.frame(1_000.0 + EMERGE_MS - 1.0),
            Some(Cell::new(0, 0)),
            "still on the hole a moment before the first step",
        );
        for i in 1..=4 {
            assert_eq!(
                walk.frame(1_000.0 + EMERGE_MS + step * (i - 1) as f64),
                Some(Cell::new(i, 0)),
                "step {i}",
            );
        }
        assert_eq!(
            walk.frame(1_000.0 + EMERGE_MS + step * 4.0 + ARRIVE_MS - 1.0),
            Some(Cell::new(4, 0)),
            "standing where control will begin",
        );
        assert_eq!(
            walk.frame(1_000.0 + walk.total_ms()),
            None,
            "the beat is over and the clock stops",
        );
    }

    /// The pace adapts to the distance but never past the two bounds, so no generated
    /// walk is a blur and none is a crawl — and every one of them lands near the
    /// five seconds the beat is aiming for.
    #[test]
    fn the_pace_fits_the_distance_between_its_bounds() {
        for steps in 1..=60u32 {
            let walk = Walk::begin(path(steps)).expect("a walk");
            assert!(
                (STEP_MIN_MS..=STEP_MAX_MS).contains(&walk.step_ms),
                "{steps} steps paced at {}ms",
                walk.step_ms,
            );
        }
        // Across the distances placement actually produces (`PLAYER_EXIT_MIN_DISTANCE`
        // is 8, and the largest room bounds the other end), the whole beat stays
        // within a second of five.
        for steps in 8..=30u32 {
            let total = Walk::begin(path(steps)).expect("a walk").total_ms();
            assert!(
                (4_000.0..=6_000.0).contains(&total),
                "{steps} steps runs for {total}ms",
            );
        }
    }

    /// Two degenerate paths the core can hand over (§10.6 makes neither reachable in
    /// play, and neither may crash a boot): nothing to walk is no animation.
    #[test]
    fn a_walk_with_nowhere_to_go_does_not_play() {
        assert_eq!(Walk::begin(Vec::new()), None, "no route, no beat");
        assert_eq!(
            Walk::begin(vec![Cell::new(3, 3)]),
            None,
            "already standing on the hole",
        );
    }

    /// The origin is the *first frame*, not the walk's construction: a boot that sat
    /// in a background tab plays the whole beat when it is next drawn rather than
    /// skipping to the end of it.
    #[test]
    fn the_first_frame_starts_the_clock_whenever_it_arrives() {
        let mut walk = Walk::begin(path(3)).expect("a walk");
        assert_eq!(
            walk.frame(9_000_000.0),
            Some(Cell::new(0, 0)),
            "however late the first frame is, the beat starts at its beginning",
        );
    }
}
