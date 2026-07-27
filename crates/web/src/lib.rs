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
//! the row's dim shade out of FOV (dark gray for most; quieter for floor dots,
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

mod debug;
mod input;
mod menu;
mod palette;
mod replay;
mod seed;
mod tap;

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{
    render_screen, start_level, Grid, LevelSeed, ScreenUi, State, Theme, Visibility, BOTTOM_ROWS,
    TOP_ROWS,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, Document, Element, HtmlCanvasElement, Window};

use crate::palette::{bg_color, memory, page, swatch};

/// The glyph cell's base aspect (width:height); a monospace glyph reads best in a
/// slightly tall box. Actual on-screen cell size is this scaled to fit the viewport.
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
pub(crate) fn new_run(level: &LevelSeed) -> Result<State, JsValue> {
    start_level(level)
        // The build's debug switches (§12.6), applied on top of — never inside — the
        // level: they are not part of what the run *is*, so they ride the boot rather
        // than the token, and a run under them plays the identical game (they widen
        // what the player perceives, never the facility or the guards). This is the
        // shell's one funnel for a fresh run — the
        // first frame, the menu's Quick play, and each replay re-run — so a baked
        // switch holds for every run of the page.
        .map(|state| state.with_debug(debug::baked_debug()))
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
    let chosen = seed::explicit_level();
    let level = chosen.unwrap_or_else(seed::random_level);
    let replay = replay::initial_replay(level);
    let state = match &replay {
        Some(view) => view.state_at()?,
        None => new_run(&level)?,
    };

    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("no document"))?;
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
    let ui = if chosen.is_none() && replay.is_none() {
        menu::opening_ui()
    } else {
        ScreenUi::default()
    };
    let ui = ScreenUi { modality, ..ui };

    let game = Rc::new(RefCell::new(Game {
        state,
        canvas,
        ctx,
        metrics: Metrics::base(),
        ui,
        level,
        replay,
        replay_hud: None,
        key_ramp: replay::ScrubRamp::default(),
    }));
    game.borrow_mut().fit_and_draw(); // size to the viewport and paint the first frame
    if game.borrow().replay.is_some() {
        // A replay is a pure view: only the scrub pump is wired, never the live
        // pumps or the seed bar — so the gesture maps cannot collide (§11.6).
        replay::install(&document, &game)?;
    } else {
        input::install_input(&document, &game)?;
        input::install_gestures(&document, &game)?;
        menu::install(&document, &game)?;
        // A run the load already named is live from the first frame, so its token
        // belongs in the address bar straight away (§13.1/#110). A run chosen from
        // the menu reflects itself when it starts, not before.
        if game.borrow().ui.menu.is_none() {
            seed::reflect_level(&level);
        }
    }
    install_resize(&game)?;
    Ok(())
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
}

impl Game {
    /// Fit the canvas to the viewport and redraw. Compute a uniform scale so the whole
    /// `cols × rows` grid fits within the window on both axes (aspect preserved), size
    /// the backing store in device pixels for crisp glyphs, set the CSS size so the
    /// element itself fits (no scrolling), and paint. Called at boot and on every
    /// resize / orientation change.
    fn fit_and_draw(&mut self) {
        let facility = self.state.layout().facility();
        // The screen is the map plus the §11.4 status lines above it and the
        // ability bar beneath it.
        let (cols, rows) = (
            facility.width() as f64,
            (facility.height() + TOP_ROWS + BOTTOM_ROWS) as f64,
        );
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

    /// Rebuild the run from a new [`LevelSeed`] and repaint (§13.1 seed sharing /
    /// #110/#245): a fresh facility from the same deterministic boot as [`new_run`],
    /// the view state reset to a clean default. A pure reset — no world carries over,
    /// and the board footprint is unchanged (40×40, §10.2), so the existing fit still
    /// holds; we refit-and-draw anyway to paint the first frame. Returns the
    /// generation error only if the seed somehow fails to carve (the v1 footprint
    /// always does, §10.6).
    fn reseed(&mut self, level: LevelSeed) -> Result<(), JsValue> {
        self.state = new_run(&level)?;
        self.level = level;
        // A clean view state, except for what the *player* is — the modality the
        // hint speaks (§11.6/#323) is a fact about their hands, not about the run,
        // so a fresh facility must not send a touch player back to reading keys.
        self.ui = ScreenUi {
            modality: self.ui.modality,
            ..ScreenUi::default()
        };
        self.fit_and_draw();
        Ok(())
    }

    /// Draw one frame: ask the core to render the whole §11.4 screen (map, near
    /// line, usable line — glyphs, overlaps and categories all decided there),
    /// then blit it — colour by category, glyph as given.
    fn draw(&self) {
        paint(
            &self.ctx,
            &render_screen(&self.state, self.ui),
            &self.metrics,
            self.ui.theme,
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
fn paint(ctx: &CanvasRenderingContext2d, grid: &Grid, m: &Metrics, theme: Theme) {
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
                ctx.set_fill_style_str(bg_color(theme, bg, cell.vis));
                ctx.fill_rect(x as f64 * m.cell_w, y as f64 * m.cell_h, m.cell_w, m.cell_h);
            }
            if cell.glyph == ' ' {
                continue;
            }
            let color = match cell.vis {
                // Live: the full category colour (§11.5).
                Visibility::Live => swatch(theme, cell.fg).fg,
                // Out-of-FOV geometry: the row's dim shade (§11.5) — the standard
                // dark gray for most, quieter for Ground, tinted for the exit.
                //
                // Unexplored geometry takes **the same shade** (§11.5a/#307): the
                // schematic separates itself by shape (`≈`/`~`), so it needs no
                // colour of its own. That is the point of choosing the glyph
                // channel — a fourth brightness rung would have had to fit below
                // Ground's already-quiet dim, where a dark palette has no room,
                // and it would have owed a second set of values to light mode
                // (#189). Nothing here changes when the ladder gains a rung.
                Visibility::Explored | Visibility::Unexplored => swatch(theme, cell.fg).dim,
                // Remembered contents read as memory, not as the live thing (§11.5a).
                Visibility::Remembered => memory(theme),
            };
            ctx.set_fill_style_str(color);
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
