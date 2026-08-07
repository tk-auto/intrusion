//! The thin web shell (§12.2): the wasm-bindgen entry point, a canvas2d blitter, and
//! the input pump. It stays deliberately thin — all game logic *and all rendering*
//! live in `intrusion-core`; this crate feeds the core input and paints the grid the
//! core hands back.
//!
//! **The rendering seam (§11.1, and see `core::render`).** The core produces a
//! [`Grid`] of `(glyph, fg-category, bg)` — it decides every glyph, resolves every
//! overlap (glyph priority, §11.3), and tags each cell with an information *category*
//! (§11.2). This shell does exactly **one** rendering job: map each cell's
//! [`Category`] to a concrete colour and draw the glyph. It never decides a glyph,
//! never overlays an entity itself, never picks a colour from game state — if it did,
//! the core would stop being the single source of truth for what the game looks like.
//!
//! It runs the turn loop (§4.2): boot generates a facility, drops the player in, and
//! draws it; arrow keys (or WASD / vi keys) drive [`State::step`], as do touch
//! gestures (§11.6's touch slice — swipe to walk and keep walking, press to wait),
//! and every input redraws. All input plumbing — the key pump, the gesture pump
//! and its repeat timers — lives in the [`input`] module, and the §11.2 colour
//! table in [`palette`]; this file keeps the boot, the fit, and the paint loop.
//! The **whole level is always visible with no scrolling**, on desktop and
//! mobile alike: the canvas is scaled to fit the viewport (aspect preserved) and its
//! backing store is sized in device pixels so glyphs stay crisp; a resize/orientation
//! change recomputes and redraws. The grid arrives already fogged (§11.5a) and
//! overlaid (§11.5 — `Danger` backgrounds on cells watched by visible guards); this
//! shell maps each cell's knowledge state to styling: full category colour live,
//! the row's dim shade out of FOV (dark grey for most; quieter for floor dots,
//! tinted for the exit), muted slate remembered, and two red background shades for
//! the danger overlay. Colours all come from [`palette`] — a full-range,
//! colour-blind-safe 16-colour set behind a single category→swatch table. The frame
//! is the full §11.4 *screen* — the near and usable status lines on top, the map,
//! and the always-on ability bar beneath, all composed by `core::render_screen` from
//! the game state plus the shell's `ScreenUi` view state. Keys map through
//! `core::input_for_key` (§11.6) for game actions, `core::ability_for_key` for the
//! ability shortcuts, and `core::ui_command_for_key` for view toggles (`m` deploys
//! the message list, `?` opens help, `n` flips the colour theme — the shell picks
//! which of [`palette`]'s two tables to paint from, and the core only ever moves the
//! flag, §11.2/#189); an ability shortcut is a **toggle**, so the
//! resolved identity goes through `core::State::ability_input` for the
//! activate-or-switch-off choice (§4.4/#304), and for pointer and touch a tap on an
//! ability-bar entry (`core::ability_at`) drives that same input — so the picture,
//! the bindings, and every hit-test's geometry are all pinned by native tests.
//! Levels come fully placed from the core (`generate_level`, §10.1.7–9): entry/exit
//! and player in the largest room, intel spread across rooms, guards seated where
//! none eyes the spawn on turn one — and the guards arrive as live patrolling
//! actors (§7.5) straight from `Placement::guards`, so the shell never decides what
//! a placed guard is; it just hands what placement built to the core.

mod campaign;
mod clipboard;
mod debug;
mod input;
mod menu;
mod palette;
mod replay;
mod save;
mod screen_settings;
mod seed;
mod settings;
mod tap;
mod tiles;
mod timer;

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use intrusion_core::{
    render_map, render_screen, start_level, Campaign, DebugModifiers, Grid, Input, LevelSeed,
    ScreenUi, State, Theme, Visibility, BOTTOM_ROWS, TOP_ROWS,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, Document, Element, HtmlCanvasElement, Window};

use crate::palette::{bg_colour, memory, page, swatch};

/// The glyph cell's base aspect (width:height); a monospace glyph reads best in a
/// slightly tall box. Actual on-screen cell size is this scaled to fit the viewport.
///
/// **The tile renderer ([`tiles`], #460) authors to this same box**, so turning it on
/// changes nothing about the fit or about where a hit test lands. A square-cell tile
/// mode would need the map to carry its own metric while the HUD rows kept the text
/// one, which is a different and much larger change.
const CELL_W: f64 = 14.0;
const CELL_H: f64 = 20.0;

/// Build a fresh run from a [`LevelSeed`] — the **exact** boot the headless sim and
/// the replay viewer use ([`start_level`]: `Rng::new(seed)` → generation →
/// `State::new` facing north, then the seed's modifiers and loadout, §13.2), so a
/// level here is the very run a `--bot`/`--script` run or a shared link played, and
/// a sim finding reproduces in the browser (§12.4).
///
/// A run is `(seed, modifiers, abilities)` (#245): the [`LevelSeed`] carries all
/// three, and quick play (#244) is its default preset — the intel gate at *all*,
/// the innate set plus a seeded tech grant. One seed per run (§12.4): the stream
/// that carves the level continues into the turn loop (§10.4/#146).
///
/// `pub(crate)` so the replay viewer ([`replay`]) can re-run a level through
/// `inputs[0..K]` to derive the state at its cursor (replay-minus-N, §12.4).
pub(crate) fn new_run(level: &LevelSeed, debug: DebugModifiers) -> Result<State, JsValue> {
    start_level(level)
        // The session's debug switches (§12.6), applied on top of — never inside — the
        // level: they are not part of what the run *is*, so they ride the boot rather
        // than the token, and a run under them plays the identical game (they widen
        // what the player perceives, never the facility or the guards). This is the
        // shell's one funnel for a fresh run — the
        // first frame, the menu's Quick play, and each replay re-run — so the switches
        // hold for every run of the page. They are **passed**, not read from the build:
        // since #459 a debug session can flip omni-vision mid-run, and a fresh facility
        // should carry on with the switch the watcher last set rather than snapping
        // back to whatever the build stamped.
        .map(|state| state.with_debug(debug))
        .map_err(|e| JsValue::from_str(&format!("generation failed: {e:?}")))
}

/// Boot the game: pick the run's seed, generate its facility, draw it, and start
/// listening for input and resize (§4.2, §13.1's build→play half).
///
/// This is the wasm entry point the page calls after the module initialises. The
/// level is the one impurity the shell owns (§12.1 keeps the *core* pure): it comes
/// from the URL when a `…#seed=<token>` link was shared, from a baked global in a
/// seed-locked artifact, and otherwise off the clock ([`seed`]). It is the whole
/// reproducible config `(seed, modifiers, abilities)` (#245), re-enterable through
/// the menu's seed prompt ([`menu`]) as a compact level-seed token — the
/// seed-sharing loop (§13.1/#110/#244).
///
/// **Where a load lands** (#268): a load that was *told* which level to play — a
/// shared link, a baked artifact, a replay — boots straight into it, because the
/// sharer already chose the run. A bare load was told nothing, so it opens on the
/// **title screen** and the player chooses. Either way the shell builds a run here:
/// it sizes the canvas, and it is the run the menu's Quick play replaces a moment
/// later with a fresh roll off the clock.
#[wasm_bindgen]
pub fn start() -> Result<(), JsValue> {
    // The level comes from #110's surface (baked global or URL), decoded from its
    // level-seed token (#245); a replay widens the payload again to `(level,
    // inputs)` (§12.4/#197). When one is present the shell boots into the **replay
    // viewer** — a pure playback of the captured run — otherwise into ordinary live
    // play. The mode is fixed here, at boot, behind this one flag: the two input maps
    // are wired mutually exclusively below, so a time-scrub swipe and a movement
    // swipe can never reach the same handler.
    // The debug channel (§12.6/#459), read once here: whether this session has the
    // panel's Debug tab, and the switches it starts under. Read *before* the run is
    // built, because the switches ride the boot — and it is what consumes and strips
    // a `?debug=intruded` activation, so the address bar is already clean by the time
    // the run reflects its own token into it below.
    let debug = debug::boot_debug();
    // The autosaved run, if this build can read one (§12.5/#514). Asked once, here,
    // because two decisions below hang on it: whether the title screen carries a
    // *Continue run* row, and whether a load that names a level is a reload of the run
    // in progress rather than a fresh roll of it.
    let resume = save::stored();
    let resume_level = resume.as_ref().map(|save| save.level);
    let chosen = seed::explicit_level();
    let level = chosen.unwrap_or_else(seed::random_level);
    let replay = replay::initial_replay(level);
    let state = match &replay {
        Some(view) => view.state_at(debug.flags)?,
        None => new_run(&level, debug.flags)?,
    };

    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("no document"))?;
    // The page chrome's colours, injected from the one palette table (§11.2/#546):
    // index.html holds only `var(--chrome-…)` references, so this <style> is what
    // gives them values — before the first paint of anything the shell owns, and
    // for both themes at once (the flip rides `body[data-theme]` like the board's).
    let chrome = document.create_element("style")?;
    chrome.set_text_content(Some(&palette::chrome_css()));
    document
        .head()
        .ok_or_else(|| JsValue::from_str("no head"))?
        .append_child(&chrome)?;
    let canvas = mount_canvas(&document)?;
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .dyn_into::<CanvasRenderingContext2d>()?;

    // A load nobody aimed at a particular run opens on the title screen; one that was
    // aimed — a shared link, a baked seed, a replay — goes straight in (#268). Either
    // way the view state opens on the device's own input modality (§11.6/#323), which
    // the first key or finger then corrects.
    let modality = input::boot_modality();
    // The player's stored preferences (§14 v2/#513), and this load's override of them:
    // the theme and the renderer come back from the settings record, and a `?tiles=`
    // URL (or a baked preview build) states the renderer for this load over the top.
    let preferences = settings::Settings::boot(tiles::boot_choice());
    let ui = match (chosen, &replay) {
        // A bare load opens on the title screen; the run it starts a moment later
        // raises its own level-start card through `for_fresh_run` (#497).
        (None, None) => menu::opening_ui(resume.is_some()),
        // A load that was *told* which run to play boots straight into it (#268), so
        // this is that run's level start and the card is up for it — the same first
        // frame the menu's Quick play would have produced.
        (Some(_), None) => ScreenUi::default().for_fresh_run(),
        // A replay is a pure view of a run that has already been played (§12.4): there
        // is no first turn to stand in front of, and the card would only be a thing to
        // dismiss before the playback could be scrubbed.
        (_, Some(_)) => ScreenUi::default(),
    };
    // Whether the panel carries the **Debug tab** — and with it the copy-replay
    // control (#411/#478) — is a fact about the *session* (#459), decided once here
    // and never again: no run, and nothing a player can be handed, may switch it on.
    // It used to take a second flag for whether the *build* had a recorder behind
    // that control; every build has one now, so there is one question left to ask.
    let ui = ScreenUi {
        modality,
        theme: preferences.theme,
        renderer: preferences.renderer,
        debug_mode: debug.mode,
        ..ui
    };

    // `new_cyclic` because the shell has to be able to reach *itself* from a browser
    // callback that finishes later — the clipboard write is the one action here whose
    // answer arrives after the call that started it ([`Game::handle`]).
    let game = Rc::new_cyclic(|handle: &Weak<RefCell<Game>>| {
        RefCell::new(Game {
            state,
            canvas,
            ctx,
            metrics: Metrics::base(),
            ui,
            level,
            replay,
            replay_hud: None,
            key_ramp: replay::ScrubRamp::default(),
            recorded: Vec::new(),
            tiles: tiles::Tiles::boot(),
            campaign: None,
            autosave: save::browser(handle.clone()),
            resume,
            handle: handle.clone(),
        })
    });
    game.borrow_mut().fit_and_draw(); // size to the viewport and paint the first frame
                                      // The spritesheet decodes asynchronously, so this only *starts* the load; the
                                      // frames before it lands paint as text, and the shell redraws when it arrives
                                      // (§11.1/#460). A no-op unless the load asked for tiles.
    tiles::install(&game)?;
    if game.borrow().replay.is_some() {
        // A replay is a pure view: only the scrub pump is wired, never the live
        // pumps or the seed bar — so the gesture maps cannot collide (§11.6).
        replay::install(&document, &game)?;
    } else {
        input::install_input(&document, &game)?;
        input::install_gestures(&document, &game)?;
        menu::install(&document, &game)?;
        // The page-hide flush (§12.5/#514) — live play only: a replay has no run of its
        // own to write.
        save::install(&document, &game)?;
        // A run the load already named is live from the first frame, so its token
        // belongs in the address bar straight away (§13.1/#110). A run chosen from
        // the menu reflects itself when it starts, not before.
        if game.borrow().ui.menu.is_none() {
            seed::reflect_level(&level);
        }
        // …and if that named run is the one already in progress, the load is a
        // **reload**, not a fresh start: resume it over the frame just built (§12.5).
        if resume_level.is_some_and(|saved| resumes_in_place(chosen, saved)) {
            game.borrow_mut().continue_run();
        }
    }
    install_resize(&game)?;
    Ok(())
}

/// Whether a load that was **told** to play `chosen` should resume the saved run
/// instead of rolling it fresh (§12.5/#514 × §13.1/#110).
///
/// The shell reflects a live run's token into the address bar the moment the run
/// starts, so a **refresh mid-run** arrives back here carrying that token and looks
/// exactly like a shared link. Booting it fresh would throw away the run the player is
/// in the middle of — the one thing the autosave exists to prevent — so the token
/// matching the save's own level is read as *this is that run*, and the save wins.
///
/// A token naming a **different** level is a genuine link to somebody else's run and
/// starts fresh, overwriting the slot forward as any new run does. A load that names
/// nothing does not come here at all: it opens on the title screen, where *Continue
/// run* is a row the player chooses.
fn resumes_in_place(chosen: Option<LevelSeed>, saved: LevelSeed) -> bool {
    chosen == Some(saved)
}

/// On-screen cell geometry in **backing-store (device) pixels** — the scale that fits
/// the whole level to the viewport at the current device pixel ratio.
#[derive(Clone, Copy)]
struct Metrics {
    cell_w: f64,
    cell_h: f64,
    font: f64,
}

impl Metrics {
    /// A pre-fit placeholder; [`Game::fit_and_draw`] replaces it before the first paint.
    fn base() -> Self {
        Self {
            cell_w: CELL_W,
            cell_h: CELL_H,
            font: CELL_H - 2.0,
        }
    }
}

/// The running game, its canvas, the current fit, and the transient view state —
/// the shell's whole mutable world. The rendering half of its behaviour (fit,
/// paint) lives below; the input half (keys, gestures) in the [`input`] module.
struct Game {
    state: State,
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    metrics: Metrics,
    /// View state the shell owns (§11.4): whether the ability panel is deployed.
    /// Not part of [`State`] — it changes no world and costs no turn (§12.1).
    ui: ScreenUi,
    /// The level the current run booted from (§12.4/#245) — the shell's, not the
    /// core's: the whole reproducible config `(seed, modifiers, abilities)`. Held so
    /// the seed bar can show its [level-seed token](LevelSeed::encode) and a
    /// `…#seed=<token>` link can carry it, modifiers and loadout and all ([`seed`]).
    level: LevelSeed,
    /// The replay being played back, or `None` in ordinary live play (#197). When
    /// `Some`, the shell is in the pure-view replay mode: [`state`](Game::state) is
    /// `view.state_at(K)` and input drives the cursor, not the world ([`replay`]).
    replay: Option<replay::ReplayView>,
    /// The replay HUD's position element (`K / total`), wired in replay mode only —
    /// `None` in live play. Held so every redraw can refresh it ([`replay`]).
    replay_hud: Option<Element>,
    /// The keyboard scrub's acceleration ramp (§12.4/#227), used in replay mode
    /// only: a held Space/→/← climbs it and a fresh press resets it. Its touch
    /// counterpart lives on each scrub gesture; live play never touches either.
    key_ramp: replay::ScrubRamp,
    /// Every [`Input`] this live run has been fed, in order — the recorder half of
    /// the copy-replay control (§12.4/#411). §12.4 [SETTLED]: a replay is
    /// `(seed, [inputs])` and nothing else, so this plus [`level`](Game::level) *is*
    /// the run, and re-feeding it reproduces the run byte-for-byte. Appended at the
    /// one seam every input crosses ([`Game::step_and_draw`]) and cleared only when
    /// a fresh run replaces the world ([`Game::reseed`]) — never by the panel, so
    /// copying twice hands over the run so far, then the run so further.
    ///
    /// **Every build records** (#478). It used to be a `debug-tools` build's alone,
    /// from back when the control lived on a tab every player could see (#411); the
    /// Debug tab (#459) is the gate now, and a deployed page that could lift the fog
    /// but not hand over the run was missing the more useful half. The cost is one
    /// small `Copy` enum per turn — a couple of thousand of them in a long session,
    /// tens of kilobytes — against a strange run being reproducible by whoever it
    /// happened to. Always empty in replay mode, whose inputs drive a cursor, not a
    /// world.
    recorded: Vec<Input>,
    /// The tile renderer (§11.1/#460), or an inert one when this load did not ask for
    /// tiles. Held here because the sheet and its tinted atlases outlive a frame —
    /// they are the shell's, like the canvas, not the run's: [`Game::reseed`] leaves
    /// them alone, since a fresh facility is drawn with the same art.
    tiles: tiles::Tiles,
    /// The **campaign** this run is part of (§14 v3/§12.7), or `None` in quick play —
    /// the layer above [`state`](Game::state), which knows only its own facility.
    ///
    /// Held here rather than on [`ui`](Game::ui) because it is not view state: it is the
    /// run, and it survives every facility the run walks through. `MapUi` on the view
    /// state says whether the map is *showing*; this says whether there is a map at all.
    campaign: Option<Campaign>,
    /// The run's **autosave** (§12.5/#514): the storage slot and the write policy that
    /// decides when the run crosses it. Held here rather than in the view state for the
    /// same reason the campaign is — it is a fact about the page's run, not about what
    /// is drawn — and it is the page's, not the run's: a fresh facility keeps the same
    /// slot and simply overwrites it.
    autosave: save::Autosave,
    /// The saved run this load found and has **not yet resumed**, or `None`.
    ///
    /// Read once at boot and taken by [`Game::continue_run`], so a run can be resumed
    /// exactly once and the menu can offer *Continue run* knowing the record already
    /// decoded — an entry that could fail when chosen would be worse than no entry.
    resume: Option<save::Save>,
    /// A weak handle back to the shell's own cell, closed at construction
    /// (`Rc::new_cyclic`). Every other input the shell takes is answered inside the
    /// call that raised it, so nothing else needs this; the clipboard write (§13.1/
    /// #353) is the exception — the browser replies with a promise, and the reply has
    /// to find its way back here to record the outcome and repaint. **Weak**, so the
    /// cycle it closes is not a leak, and an upgrade that fails simply means the page
    /// is gone and there is nothing left to tell.
    handle: Weak<RefCell<Game>>,
}

impl Game {
    /// Fit the canvas to the viewport and redraw. Compute a uniform scale so the whole
    /// `cols × rows` grid fits within the window on both axes (aspect preserved), size
    /// the backing store in device pixels for crisp glyphs, set the CSS size so the
    /// element itself fits (no scrolling), and paint. Called at boot and on every
    /// resize / orientation change.
    fn fit_and_draw(&mut self) {
        let (cols, rows) = self.screen_size();
        let (cols, rows) = (cols as f64, rows as f64);
        let win = web_sys::window().expect("a window");

        let avail_w = viewport(&win, Window::inner_width).unwrap_or(cols * CELL_W);
        let avail_h = viewport(&win, Window::inner_height).unwrap_or(rows * CELL_H);
        let dpr = win.device_pixel_ratio().max(1.0);

        // CSS pixels per base cell so the level fits both dimensions.
        let scale = (avail_w / (cols * CELL_W)).min(avail_h / (rows * CELL_H));
        let css_w = cols * CELL_W * scale;
        let css_h = rows * CELL_H * scale;

        // Backing store in device pixels; CSS box in layout pixels. Drawing in device
        // pixels then keeps text sharp on high-DPI and mobile screens.
        self.canvas.set_width((css_w * dpr).round() as u32);
        self.canvas.set_height((css_h * dpr).round() as u32);
        let _ = self
            .canvas
            .set_attribute("style", &format!("width:{css_w}px;height:{css_h}px"));

        self.metrics = Metrics {
            cell_w: CELL_W * scale * dpr,
            cell_h: CELL_H * scale * dpr,
            font: (CELL_H - 2.0) * scale * dpr,
        };
        // Tell the page how big a glyph is now (in CSS pixels, so the same number the
        // stylesheet works in): the seed box types itself at the board's own size
        // rather than a fixed one that would tower over a small fit ([`menu`]).
        menu::set_glyph_size((CELL_H - 2.0) * scale);
        self.draw();
    }

    /// The whole frame's size in cells: the board plus the §11.4 status lines above it
    /// and the ability bar beneath it.
    ///
    /// **Every screen is this size**, including the ones with no board behind them — the
    /// menu, the end screen, the campaign map. Sizing them to the board is what keeps
    /// starting a run a change to what is *drawn* and never to the fit, so no screen
    /// transition ever resizes the canvas under the player.
    fn screen_size(&self) -> (u32, u32) {
        let facility = self.state.layout().facility();
        (facility.width(), facility.height() + TOP_ROWS + BOTTOM_ROWS)
    }

    /// Rebuild the run from a new [`LevelSeed`] and repaint (§13.1 seed sharing /
    /// #110/#245): a fresh facility from the same deterministic boot as [`new_run`],
    /// the view state reset to a clean default. A pure reset — no world carries over,
    /// and the board footprint is unchanged (40×40, §10.2), so the existing fit still
    /// holds; we refit-and-draw anyway to paint the first frame. Returns the
    /// generation error only if the seed somehow fails to carve (the v1 footprint
    /// always does, §10.6).
    fn reseed(&mut self, level: LevelSeed) -> Result<(), JsValue> {
        // The session's live debug switches carry over (§12.6/#459): a watcher who
        // turned omni-vision on is watching the *page*, not one facility, so a fresh
        // run keeps the switch rather than snapping back to the build's stamp.
        self.state = new_run(&level, self.state.debug())?;
        self.level = level;
        // A fresh world means a fresh recording (§12.4/#411): the old inputs were
        // fed to a run that no longer exists, and a replay stitched across two
        // worlds would reproduce neither.
        self.recorded.clear();
        // And a fresh world means the *old* world's outstanding write is moot
        // (§12.5/#514). The slot itself is left holding whatever it holds: it is
        // overwritten **forward**, by this run's own first write a couple of seconds
        // from now, so starting a run never empties the slot before there is anything
        // to put in it.
        self.autosave.reset();
        // A clean view state, except for what the *player* is — the modality the
        // hint speaks (§11.6/#323) is a fact about their hands and the colour theme
        // (§11.2/#189) a fact about their eyes, neither of them about the run, so a
        // fresh facility must not send a touch player back to reading keys nor open
        // in a theme they turned off — and for what the *build and the session* are:
        // the copy-replay offer (#411) and the debug session's tab (#459) are the
        // page's, not the run's. The carried set is named once, beside the fields, so
        // a new one cannot be forgotten here (#473).
        self.ui = self.ui.for_fresh_run();
        self.fit_and_draw();
        Ok(())
    }

    /// Draw one frame: ask the core to render the whole §11.4 screen (map, near
    /// line, usable line — glyphs, overlaps and categories all decided there),
    /// then blit it — colour by category, glyph as given.
    fn draw(&self) {
        // The campaign map is the one screen that is **not** a view of a [`State`]
        // (§14 v3/#208): it draws the run, which sits above the facility, so it is asked
        // for by name rather than composed by `render_screen`. Everything else about the
        // frame — the fit, the paint, the theme, the tile layer — is identical, so the
        // map is a full screen like the menu and never an overlay.
        //
        // It needs no tile decision of its own: every cell it draws is
        // [`Surface::Chrome`](intrusion_core::Surface) — it is a panel, like the title
        // screen and the help card — and the tile renderer draws only board cells
        // (#460). So the layer is passed through the one paint call and declines by
        // itself, rather than by this branch knowing anything about art.
        let (cols, rows) = self.screen_size();
        let grid = match (self.map_open(), self.campaign.as_ref()) {
            (true, Some(run)) => render_map(cols, rows, run, self.ui.map.unwrap_or_default()),
            _ => render_screen(&self.state, self.ui),
        };
        paint(
            &self.ctx,
            &grid,
            &self.metrics,
            self.ui.theme,
            self.tiles.layer(self.ui.renderer),
        );
        reflect_theme(self.ui.theme);
        // In replay mode, keep the `K / total` HUD in step with the board every
        // frame; a no-op in live play (§12.4/#197).
        self.update_replay_hud();
    }
}

/// Mirror the live theme onto `<body data-theme>` so the **page around the board**
/// follows it too (§11.2/#189), the same way `data-screen` publishes which surface
/// is up ([`menu`]).
///
/// The canvas is fitted to the viewport with the aspect preserved, so there is
/// always a letterbox beside or beneath it — and the page's own backdrop is CSS, not
/// something [`paint`] can reach. Without this the light board sat in a black frame:
/// the one part of the screen where the theme was still the old one. The shell says
/// only *which* theme; the two page colours live in the stylesheet with the rest of
/// the page chrome.
///
/// Best-effort, like `data-screen`: a page without a body simply keeps the default
/// dark chrome, which is the safe direction.
fn reflect_theme(theme: Theme) {
    let name = match theme {
        Theme::Dark => "dark",
        Theme::Light => "light",
    };
    if let Some(body) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.body())
    {
        let _ = body.set_attribute("data-theme", name);
    }
}

/// Read a viewport dimension (`inner_width` / `inner_height`) as an `f64`, if the
/// browser gives one.
fn viewport(win: &Window, get: fn(&Window) -> Result<JsValue, JsValue>) -> Option<f64> {
    get(win).ok().and_then(|v| v.as_f64())
}

/// Create the canvas, mount it, and hand it back. Its size is set later by
/// [`Game::fit_and_draw`], which fits it to the viewport.
fn mount_canvas(document: &Document) -> Result<HtmlCanvasElement, JsValue> {
    // Mount into #app if the page provides it, else the body.
    let mount = document
        .get_element_by_id("app")
        .or_else(|| document.body().map(Into::into))
        .ok_or_else(|| JsValue::from_str("no mount point"))?;

    let canvas: HtmlCanvasElement = document
        .create_element("canvas")?
        .dyn_into::<HtmlCanvasElement>()?;
    mount.append_child(&canvas)?;
    Ok(canvas)
}

/// Install the resize pump: refit the canvas to the window on resize / orientation
/// change, so the whole level stays visible without scrolling.
fn install_resize(game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let game = game.clone();
    let cb = Closure::<dyn FnMut()>::new(move || game.borrow_mut().fit_and_draw());
    let win = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    win.add_event_listener_with_callback("resize", cb.as_ref().unchecked_ref())?;
    cb.forget();
    Ok(())
}

/// Blit a rendered [`Grid`] to the canvas: fill the background, then draw each
/// non-blank glyph centred in its cell, coloured by its category ([`swatch`]).
/// Blank cells (floor) are left as background. This is the shell's whole rendering
/// job — the glyphs, overlaps and categories were all decided by `core::render`.
///
/// `theme` picks which of [`palette`]'s two columns every colour is read from
/// (§11.2/#189) and is the only thing about it this loop knows: the grid is
/// identical either way, because the core never named a colour to begin with.
///
/// `tiles` is the same shape of argument one step further out (§11.1/#460): the
/// **second implementation of the cell primitive**, or `None` for the text renderer
/// this shell has always been. It is consulted after the colour is resolved and never
/// before — a tile is drawn in exactly the colour the glyph would have been — and it
/// answers `false` for any cell it did not draw, which falls straight through to
/// [`draw_char`]. So the tile mode adds a branch to one line of this loop and changes
/// nothing else about it: same grid, same colours, same backgrounds, same order.
fn paint(
    ctx: &CanvasRenderingContext2d,
    grid: &Grid,
    m: &Metrics,
    theme: Theme,
    tiles: Option<&tiles::Tiles>,
) {
    ctx.set_fill_style_str(page(theme));
    ctx.fill_rect(
        0.0,
        0.0,
        grid.width() as f64 * m.cell_w,
        grid.height() as f64 * m.cell_h,
    );

    ctx.set_font(&format!("{:.1}px ui-monospace, monospace", m.font));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");

    for y in 0..grid.height() {
        for x in 0..grid.width() {
            let cell = grid.get(x, y);
            // The danger overlay (§11.5) first: a background paints even under a
            // blank glyph — a watched open doorway is still watched.
            if let Some(bg) = cell.bg {
                ctx.set_fill_style_str(bg_colour(theme, bg, cell.fill));
                ctx.fill_rect(x as f64 * m.cell_w, y as f64 * m.cell_h, m.cell_w, m.cell_h);
            }
            if cell.glyph == ' ' {
                continue;
            }
            let colour = match cell.vis {
                // Live: the full category colour (§11.5).
                Visibility::Live => swatch(theme, cell.fg).fg,
                // Out-of-FOV geometry: the row's dim shade (§11.5) — the standard
                // dark grey for most, quieter for Ground, tinted for the exit.
                //
                // Unexplored geometry takes **the same shade** (§11.5a/#307): the
                // schematic separates itself by shape (`□`, and blank where a plan
                // shows floor space), so it needs no colour of its own. That is the
                // point of choosing the glyph
                // channel — a fourth brightness rung would have had to fit below
                // Ground's already-quiet dim, where a dark palette has no room,
                // and it would have owed a second set of values to light mode
                // (#189). Nothing here changes when the ladder gains a rung.
                Visibility::Explored | Visibility::Unexplored => swatch(theme, cell.fg).dim,
                // Remembered contents read as memory, not as the live thing (§11.5a).
                Visibility::Remembered => memory(theme),
            };
            // The one line the tile mode touches: a sprite in that same colour if
            // this cell has one, and the character otherwise (§11.1/#460). It is
            // handed the **grid**, not the cell (#461): a wall's sprite is chosen from
            // its neighbours, and the grid is the only thing it may read them from.
            if tiles.is_some_and(|tiles| tiles.draw(ctx, grid, x, y, colour, m)) {
                continue;
            }
            ctx.set_fill_style_str(colour);
            draw_char(ctx, x as f64, y as f64, cell.glyph, m);
        }
    }
}

/// Paint a single character centred in cell `(x, y)` with the current fill style.
fn draw_char(ctx: &CanvasRenderingContext2d, x: f64, y: f64, glyph: char, m: &Metrics) {
    let px = x * m.cell_w + m.cell_w / 2.0;
    let py = y * m.cell_h + m.cell_h / 2.0;
    // fill_text only errors on an invalid surface; ignore the unit Ok.
    let _ = ctx.fill_text(&glyph.to_string(), px, py);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A reload of the run in progress resumes it** (§12.5/#514). The shell writes a
    /// live run's token into the address bar (§13.1/#110), so a refresh comes back
    /// naming the level it was already playing — and booting that fresh would throw
    /// away the very run the autosave exists to keep. A token naming a *different*
    /// level is a real link and starts fresh; a load naming nothing never asks.
    #[test]
    fn a_reload_of_the_saved_level_resumes_it_and_another_link_does_not() {
        let saved = LevelSeed::quick_play(4);
        let other = LevelSeed::quick_play(5);
        assert!(resumes_in_place(Some(saved), saved), "the same run reloads");
        assert!(
            !resumes_in_place(Some(other), saved),
            "another link is a run"
        );
        assert!(!resumes_in_place(None, saved), "a bare load opens the menu");
    }
}
