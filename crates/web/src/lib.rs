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
//! draws it; arrow keys (or WASD / vi keys) drive [`State::step`], as do screen taps
//! (§11.6's touch slice — edge zones step, the centre waits, see [`tap_input`]), and
//! every input redraws. The **whole level is always visible with no scrolling**, on desktop and
//! mobile alike: the canvas is scaled to fit the viewport (aspect preserved) and its
//! backing store is sized in device pixels so glyphs stay crisp; a resize/orientation
//! change recomputes and redraws. The grid arrives already fogged (§11.5a) and
//! overlaid (§11.5 — `Danger` backgrounds on cells watched by visible guards); this
//! shell maps each cell's knowledge state to styling: full category colour live,
//! dark gray dimmed, muted slate remembered, and two red background shades for the
//! danger overlay. Colours come from the §11.2 base palette below — a full-range,
//! colour-blind-safe 16-colour set behind a single category→swatch table. A
//! player-*centred* viewport (§11.4) and explicit hotkeys (§11.6) are later render
//! tickets.
//! Levels come fully placed from the core (`generate_level`, §10.1.7–9): entry/exit
//! and player in the largest room, intel spread across rooms, guards seated where
//! none eyes the spawn on turn one. The guards stand still until the guard-AI
//! tickets give them patrols; the shell just instantiates what placement chose.

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{
    generate_level, render, Category, Direction, Grid, Guard, Input, LevelConfig, Rng, State,
    Visibility,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    CanvasRenderingContext2d, Document, HtmlCanvasElement, KeyboardEvent, PointerEvent, Window,
};

/// The glyph cell's base aspect (width:height); a monospace glyph reads best in a
/// slightly tall box. Actual on-screen cell size is this scaled to fit the viewport.
const CELL_W: f64 = 14.0;
const CELL_H: f64 = 20.0;

/// One row of the base palette (§11.2): a full-strength **foreground** and its
/// **darkened background variant** — plus a further-darkened background shade for
/// when the same category paints a cell *outside* the player's FOV (§11.5 fix #1:
/// watched-but-unseen must read as watched, never as safe dark-on-dark).
#[derive(Clone, Copy)]
struct Swatch {
    fg: &'static str,
    bg: &'static str,
    bg_dim: &'static str,
}

const fn sw(fg: &'static str, bg: &'static str, bg_dim: &'static str) -> Swatch {
    Swatch { fg, bg, bg_dim }
}

/// The base palette (§11.2): a **16-colour, colour-blind-safe qualitative set**,
/// each row a foreground plus darkened background variants. **Full-range [START]**
/// — true black and true white are both here, deliberately: the old palette's
/// gamma curve compressed everything into a washed 0.1–0.9 band with six colours
/// never used at all. Compression gets added back only if something demands it.
///
/// Hues lean on the Okabe–Ito colour-blind-safe set (brightened for the dark
/// backdrop), and the threat ladder yellow→orange→red is additionally separated
/// by luminance so it survives a red-green deficiency; every pair is asserted
/// visibly distinct below. Seven rows carry the §11.2 categories today; the
/// spare rows are ready for the message bar, ability labels, and any category
/// yet to come — claimed by naming them, like the rows below the table.
const PALETTE: [Swatch; 16] = [
    sw("#000000", "#000000", "#000000"), //  0 true black — the page backdrop
    sw("#ffffff", "#5c5c5c", "#2e2e2e"), //  1 true white — Neutral
    sw("#4a4a4a", "#1e1e1e", "#121212"), //  2 dark gray — out-of-FOV dimming (§11.5)
    sw("#a8a8a8", "#434343", "#222222"), //  3 light gray — spare (secondary text)
    sw("#667a8a", "#293138", "#14181c"), //  4 slate — tile memory (§11.5a)
    sw("#4ea6ff", "#1f4266", "#102133"), //  5 blue — Owned
    sw("#2456b8", "#0e224a", "#071125"), //  6 deep blue — spare
    sw("#2ee6d6", "#125c56", "#092e2b"), //  7 cyan — spare
    sw("#3ecf5a", "#195324", "#0c2a12"), //  8 green — spare
    sw("#157f33", "#083314", "#04190a"), //  9 deep green — spare
    sw("#f0e442", "#605b1a", "#302e0d"), // 10 yellow — Caution
    sw("#e69f00", "#5c4000", "#2e2000"), // 11 orange — Warning
    sw("#ff3333", "#8c2020", "#521717"), // 12 red — Danger
    sw("#bd6bd6", "#4c2b56", "#26152b"), // 13 purple — Interest
    sw("#9a7040", "#3e2d1a", "#1f160d"), // 14 tan — System
    sw("#ff7ab8", "#66314a", "#331825"), // 15 pink — spare
];

// The rows the shell draws with today, named. A spare row stays reachable only
// through [`PALETTE`] until a system claims and names it.
const BLACK: Swatch = PALETTE[0];
const WHITE: Swatch = PALETTE[1];
const DIM_GRAY: Swatch = PALETTE[2];
const SLATE: Swatch = PALETTE[4];
const BLUE: Swatch = PALETTE[5];
const YELLOW: Swatch = PALETTE[10];
const ORANGE: Swatch = PALETTE[11];
const RED: Swatch = PALETTE[12];
const PURPLE: Swatch = PALETTE[13];
const TAN: Swatch = PALETTE[14];

/// The page background: true black — the full-range floor the §11.2 [START] note
/// restores (the old palette had no true black anywhere).
const BG: &str = BLACK.fg;

/// The **remembered** styling (§11.5a): contents known only from tile memory draw
/// in this muted slate instead of their category colour, so memory reads as memory
/// — visibly distinct from anything live *and* from the dimmed gray (asserted
/// below, with the categories).
const MEMORY_COLOR: &str = SLATE.fg;

/// The **dimmed** styling (§11.5): out-of-FOV geometry draws in this dark gray —
/// dim but legible, the same glyph at low light. Distinct from [`MEMORY_COLOR`]
/// so the three knowledge states never collapse into two (§11.5a's note).
const DIM_COLOR: &str = DIM_GRAY.fg;

/// Map an information category (§11.2) to its palette row — **the shell's one and
/// only rendering decision, and the single table a recolour edits**. The core tags
/// each cell with a [`Category`]; here, and nowhere else, category becomes pixels,
/// so an accessibility reskin is a one-table change (asserted below).
///
/// Every entry must be **visibly distinct** on the dark background (asserted
/// below): the threat ladder Caution→Warning→Danger reads as yellow→orange→red,
/// and System furniture is the muted brown-tan row rather than a bright tan that
/// would blur into Caution's yellow (the old regression).
fn swatch(category: Category) -> Swatch {
    match category {
        Category::Neutral => WHITE,   // inert scenery, walls, spent objectives
        Category::Owned => BLUE,      // you and what you made
        Category::Caution => YELLOW,  // a threat, unaware
        Category::Warning => ORANGE,  // a threat, hunting
        Category::Danger => RED,      // a threat that has you
        Category::Interest => PURPLE, // goals and rewards
        Category::System => TAN,      // doors, hideouts — neutral furniture
    }
}

/// Boot the game: generate a facility, place the player, draw it, and start listening
/// for input and resize (§4.2, §13.1's build→play half).
///
/// This is the wasm entry point the page calls after the module initialises. The seed
/// is the one impurity the shell owns (§12.1 keeps the *core* pure): read the clock so
/// each load is a different facility, and hand the core a plain `u64`. The v1 footprint
/// is 40×40 (§10.2). Reload for a new seed; explicit seed entry / sharing (§13.1) is a
/// later ticket.
#[wasm_bindgen]
pub fn start() -> Result<(), JsValue> {
    let seed = js_sys::Date::now() as u64;
    // The full v1 level (§10.2): a carve passing every §10.6 guarantee, with the
    // player, exit, intel and guards placed by the §10.1.7–9 rules. Guards are
    // stationary until the guard-AI tickets land patrols.
    let (layout, placement) = generate_level(&LevelConfig::V1, &mut Rng::new(seed))
        .map_err(|e| JsValue::from_str(&format!("generation failed: {e:?}")))?;
    let guards = placement
        .guards()
        .iter()
        .map(|&c| Guard::stationary(c))
        .collect();

    let state = State::new(
        layout,
        placement.player(),
        Direction::North,
        guards,
        placement.intel().iter().copied(),
        placement.exit(),
    );

    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let canvas = mount_canvas(&document)?;
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .dyn_into::<CanvasRenderingContext2d>()?;

    let game = Rc::new(RefCell::new(Game {
        state,
        canvas,
        ctx,
        metrics: Metrics::base(),
    }));
    game.borrow_mut().fit_and_draw(); // size to the viewport and paint the first frame
    install_input(&document, &game)?;
    install_tap(&document, &game)?;
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

/// The running game, its canvas, and the current fit — the shell's whole mutable world.
struct Game {
    state: State,
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    metrics: Metrics,
}

impl Game {
    /// Map a key to an [`Input`] and, if it is one the loop takes, step and redraw.
    /// Returns whether the key was consumed (so the caller can stop the page from
    /// scrolling on the arrows). Explicit, stable hotkeys are §11.6's ticket; this is
    /// the minimal movement set.
    fn handle_key(&mut self, key: &str) -> bool {
        let input = match key {
            "ArrowUp" | "w" | "k" => Input::Step(Direction::North),
            "ArrowDown" | "s" | "j" => Input::Step(Direction::South),
            "ArrowLeft" | "a" | "h" => Input::Step(Direction::West),
            "ArrowRight" | "d" | "l" => Input::Step(Direction::East),
            "." | "5" => Input::Wait,
            _ => return false,
        };
        self.state.step(input);
        self.draw();
        true
    }

    /// Feed one tap/click at viewport point `(x, y)` to the loop: map it through
    /// [`tap_input`] and, if the viewport was sane, step and redraw (§11.6's touch
    /// slice — the mobile counterpart of [`Self::handle_key`]).
    fn handle_tap(&mut self, x: f64, y: f64, w: f64, h: f64) -> bool {
        let Some(input) = tap_input(x, y, w, h) else {
            return false;
        };
        self.state.step(input);
        self.draw();
        true
    }

    /// Fit the canvas to the viewport and redraw. Compute a uniform scale so the whole
    /// `cols × rows` grid fits within the window on both axes (aspect preserved), size
    /// the backing store in device pixels for crisp glyphs, set the CSS size so the
    /// element itself fits (no scrolling), and paint. Called at boot and on every
    /// resize / orientation change.
    fn fit_and_draw(&mut self) {
        let facility = self.state.layout().facility();
        let (cols, rows) = (facility.width() as f64, facility.height() as f64);
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
        self.draw();
    }

    /// Draw one frame: ask the core to render the whole grid (terrain + entities,
    /// glyph priority resolved), then blit it — colour by category, glyph as given.
    fn draw(&self) {
        paint(&self.ctx, &render(&self.state), &self.metrics);
    }
}

/// Map a tap (or click) at viewport point `(x, y)` in a `w × h` viewport to an
/// [`Input`] — the touch half of §11.6, pure so the zone rule is testable natively.
///
/// The screen is zoned: the **middle third** on both axes is Wait, and everything
/// outside it steps toward the tap's **dominant axis** — left third steps west,
/// right east, top north, bottom south. Dominant-axis (in viewport-normalised
/// units) rather than a fixed 3×3 grid so corner taps still act: movement has no
/// diagonals (§4.1 [SETTLED]), and an exact corner tie goes horizontal. Zones are
/// screen-relative, not canvas-relative — a tap in the letterbox margin counts.
/// A degenerate viewport maps to nothing.
fn tap_input(x: f64, y: f64, w: f64, h: f64) -> Option<Input> {
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    // Displacement from the screen centre, normalised to [-0.5, 0.5] per axis.
    let dx = x / w - 0.5;
    let dy = y / h - 0.5;
    const WAIT_HALF: f64 = 1.0 / 6.0; // middle third of each axis
    if dx.abs() < WAIT_HALF && dy.abs() < WAIT_HALF {
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

/// Install the keydown pump: each keypress drives one [`Game::handle_key`]. The
/// closure owns a clone of the `Rc` so the game outlives `start`; `forget` hands it to
/// the browser for the page's lifetime (the shell never tears down).
fn install_input(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
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

/// Install the tap pump: each pointerdown (touch tap or mouse click, anywhere on
/// the page — the letterbox margins count too) drives one [`Game::handle_tap`]
/// against the current viewport size, so the zones track a rotation the same way
/// [`Game::fit_and_draw`] does. `preventDefault` on the consumed tap stops the
/// browser's gesture follow-ups (double-tap zoom, synthetic mouse events);
/// `touch-action: none` on the page covers the rest (see `web/index.html`).
fn install_tap(document: &Document, game: &Rc<RefCell<Game>>) -> Result<(), JsValue> {
    let game = game.clone();
    let cb = Closure::<dyn FnMut(PointerEvent)>::new(move |e: PointerEvent| {
        let win = web_sys::window().expect("a window");
        let (Some(w), Some(h)) = (
            viewport(&win, Window::inner_width),
            viewport(&win, Window::inner_height),
        ) else {
            return;
        };
        if game
            .borrow_mut()
            .handle_tap(e.client_x() as f64, e.client_y() as f64, w, h)
        {
            e.prevent_default();
        }
    });
    document.add_event_listener_with_callback("pointerdown", cb.as_ref().unchecked_ref())?;
    cb.forget();
    Ok(())
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
fn paint(ctx: &CanvasRenderingContext2d, grid: &Grid, m: &Metrics) {
    ctx.set_fill_style_str(BG);
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
                ctx.set_fill_style_str(bg_color(bg, cell.vis));
                ctx.fill_rect(x as f64 * m.cell_w, y as f64 * m.cell_h, m.cell_w, m.cell_h);
            }
            if cell.glyph == ' ' {
                continue;
            }
            let color = match cell.vis {
                // Live: the full category colour (§11.5).
                Visibility::Live => swatch(cell.fg).fg,
                // Out-of-FOV geometry: dim but legible (§11.5).
                Visibility::Dimmed => DIM_COLOR,
                // Remembered contents read as memory, not as the live thing (§11.5a).
                Visibility::Remembered => MEMORY_COLOR,
            };
            ctx.set_fill_style_str(color);
            draw_char(ctx, x as f64, y as f64, cell.glyph, m);
        }
    }
}

/// Map a background category to a fill through the same table as the glyphs: the
/// darkened [`Swatch::bg`] variant on a cell the player sees, the further-darkened
/// [`Swatch::bg_dim`] beyond the FOV. Today only the §11.5 danger overlay paints a
/// background — Danger's two shades are bright red and darker-but-still-red (fix
/// #1: watched must never look safe) — but any category a future system declares
/// arrives with its variants ready. The §7.6 certain/glimpse zones add two
/// *detection* shades when two-zone detection lands; until then the whole cone is
/// one zone.
fn bg_color(bg: Category, vis: Visibility) -> &'static str {
    let swatch = swatch(bg);
    match vis {
        Visibility::Live => swatch.bg,
        Visibility::Dimmed | Visibility::Remembered => swatch.bg_dim,
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

    /// Parse a `#rrggbb` string into RGB — mirror of what the browser does.
    fn rgb(hex: &str) -> (i32, i32, i32) {
        let h = hex.strip_prefix('#').expect("a #rrggbb colour");
        let n = i32::from_str_radix(h, 16).expect("six hex digits");
        (n >> 16 & 0xff, n >> 8 & 0xff, n & 0xff)
    }

    /// Squared RGB distance — cheap and monotonic, enough to catch two colours that
    /// would read as the same on screen.
    fn dist2(a: (i32, i32, i32), b: (i32, i32, i32)) -> i32 {
        let (dr, dg, db) = (a.0 - b.0, a.1 - b.1, a.2 - b.2);
        dr * dr + dg * dg + db * db
    }

    /// The four edge zones step their direction and the exact screen centre waits,
    /// on both a square and a phone-portrait viewport (zones are viewport-normalised,
    /// so aspect must not matter).
    #[test]
    fn tap_zones_map_edges_to_steps_and_centre_to_wait() {
        for &(w, h) in &[(600.0, 600.0), (390.0, 844.0)] {
            let mid = |n: f64| n / 2.0;
            let cases = [
                (1.0, mid(h), Input::Step(Direction::West)),
                (w - 1.0, mid(h), Input::Step(Direction::East)),
                (mid(w), 1.0, Input::Step(Direction::North)),
                (mid(w), h - 1.0, Input::Step(Direction::South)),
                (mid(w), mid(h), Input::Wait),
            ];
            for (x, y, expected) in cases {
                assert_eq!(
                    tap_input(x, y, w, h),
                    Some(expected),
                    "tap at ({x}, {y}) in {w}x{h}"
                );
            }
        }
    }

    /// The Wait box is the middle third of each axis: just inside its corner still
    /// waits, just outside steps.
    #[test]
    fn tap_wait_box_is_the_middle_third() {
        let (w, h) = (900.0, 600.0);
        // The box spans [w/3, 2w/3] × [h/3, 2h/3]; probe around its top-left corner.
        assert_eq!(tap_input(301.0, 201.0, w, h), Some(Input::Wait));
        assert_eq!(
            tap_input(299.0, 201.0, w, h),
            Some(Input::Step(Direction::West))
        );
        assert_eq!(
            tap_input(301.0, 199.0, w, h),
            Some(Input::Step(Direction::North))
        );
    }

    /// Corner taps resolve by dominant axis — there are no diagonals (§4.1) — and
    /// an exact corner tie goes horizontal.
    #[test]
    fn tap_corners_resolve_by_dominant_axis() {
        let (w, h) = (600.0, 600.0);
        // Exact ties at the corners go horizontal.
        assert_eq!(
            tap_input(0.0, 0.0, w, h),
            Some(Input::Step(Direction::West))
        );
        assert_eq!(tap_input(w, h, w, h), Some(Input::Step(Direction::East)));
        // Off the diagonal, the larger displacement wins.
        assert_eq!(
            tap_input(100.0, 40.0, w, h),
            Some(Input::Step(Direction::North))
        );
        assert_eq!(
            tap_input(40.0, 100.0, w, h),
            Some(Input::Step(Direction::West))
        );
    }

    /// A degenerate viewport (zero, negative, or NaN dimensions) maps to nothing
    /// rather than dividing into garbage.
    #[test]
    fn tap_in_degenerate_viewport_is_ignored() {
        assert_eq!(tap_input(10.0, 10.0, 0.0, 600.0), None);
        assert_eq!(tap_input(10.0, 10.0, 600.0, -1.0), None);
        assert_eq!(tap_input(10.0, 10.0, f64::NAN, 600.0), None);
    }

    /// Every category must map to a **visibly distinct** colour. The regression this
    /// guards: `System` (doors, hideouts) once sat a tan hair away from `Caution`
    /// (unaware guards), so doors, hideouts and guards all read as one yellow. The
    /// threat ladder Caution→Warning→Danger and the furniture brown must stay apart.
    #[test]
    fn category_colours_are_all_visibly_distinct() {
        let categories = [
            Category::Neutral,
            Category::Owned,
            Category::Caution,
            Category::Warning,
            Category::Danger,
            Category::Interest,
            Category::System,
        ];
        // ~70 in RGB distance: the old tan/yellow clash measured ~61 and must fail.
        const MIN_DIST2: i32 = 70 * 70;
        for (i, &a) in categories.iter().enumerate() {
            for &b in &categories[i + 1..] {
                let d = dist2(rgb(swatch(a).fg), rgb(swatch(b).fg));
                assert!(
                    d >= MIN_DIST2,
                    "{a:?} and {b:?} are too close to tell apart (dist^2 {d} < {MIN_DIST2})"
                );
            }
        }
        // The §11.5a remembered styling must stand apart from every live category —
        // memory that could be mistaken for a live glyph would defeat the three
        // visual states the design demands.
        for &c in &categories {
            let d = dist2(rgb(MEMORY_COLOR), rgb(swatch(c).fg));
            assert!(
                d >= MIN_DIST2,
                "the remembered colour is too close to {c:?} (dist^2 {d} < {MIN_DIST2})"
            );
        }
        // And the dimmed gray must not collapse into the remembered slate — three
        // knowledge states, not two (§11.5a's implementation note).
        let d = dist2(rgb(DIM_COLOR), rgb(MEMORY_COLOR));
        assert!(
            d >= MIN_DIST2 / 2,
            "dimmed and remembered blur (dist^2 {d})"
        );
    }

    /// §11.5 fix #1, at the colour table: both danger-overlay shades must read
    /// against the page background — the watched-but-unseen shade especially,
    /// since the old version let it sink into dark-on-dark and the most dangerous
    /// cells looked like the safest. The two shades also stay tellable apart.
    #[test]
    fn danger_overlay_shades_read_on_the_backdrop() {
        // Squared distance for large background fills: 40 per channel is an easy
        // read on area colour even where 70 is the bar for thin glyph strokes.
        const MIN_BG_DIST2: i32 = 40 * 40;
        let live = bg_color(Category::Danger, Visibility::Live);
        let dimmed = bg_color(Category::Danger, Visibility::Dimmed);
        for shade in [live, dimmed] {
            let d = dist2(rgb(shade), rgb(BG));
            assert!(
                d >= MIN_BG_DIST2,
                "{shade} vanishes into the page background (dist^2 {d})"
            );
            let (r, g, b) = rgb(shade);
            assert!(r > g + 30 && r > b + 30, "{shade} must read as *red*");
        }
        let d = dist2(rgb(live), rgb(dimmed));
        assert!(d >= MIN_BG_DIST2, "the two danger shades blur (dist^2 {d})");
    }

    /// The §11.2 [START] promise, pinned: the base palette is **full-range** —
    /// true black and true white are both present (the old palette's gamma curve
    /// allowed neither) — and all sixteen foregrounds are pairwise tellable apart,
    /// the same bar the category subset must clear.
    #[test]
    fn the_palette_is_full_range_and_pairwise_distinct() {
        assert!(
            PALETTE.iter().any(|s| s.fg == "#000000"),
            "no true black — the palette is compressed again"
        );
        assert!(
            PALETTE.iter().any(|s| s.fg == "#ffffff"),
            "no true white — the palette is compressed again"
        );
        const MIN_DIST2: i32 = 70 * 70;
        for (i, a) in PALETTE.iter().enumerate() {
            for b in &PALETTE[i + 1..] {
                let d = dist2(rgb(a.fg), rgb(b.fg));
                assert!(
                    d >= MIN_DIST2,
                    "palette rows {} and {} are too close (dist^2 {d} < {MIN_DIST2})",
                    a.fg,
                    b.fg
                );
            }
        }
    }

    /// §11.2: every palette row's background is a **darkened variant** of its
    /// foreground — strictly darker, and the out-of-FOV shade darker again — so a
    /// category used as a background can never outshine the glyphs on it. (True
    /// black is its own floor; nothing is darker.)
    #[test]
    fn background_variants_darken_their_foreground() {
        let lum = |hex: &str| {
            let (r, g, b) = rgb(hex);
            r + g + b
        };
        for s in &PALETTE {
            if lum(s.fg) == 0 {
                continue; // true black: fg and variants share the floor
            }
            assert!(
                lum(s.bg) < lum(s.fg),
                "{}'s bg variant {} is not darker",
                s.fg,
                s.bg
            );
            assert!(
                lum(s.bg_dim) < lum(s.bg),
                "{}'s out-of-FOV bg {} is not darker than its bg {}",
                s.fg,
                s.bg_dim,
                s.bg
            );
        }
    }

    /// The ticket's acceptance test, end to end across the seam: a **chasing guard**
    /// declares `Danger` (§7.4, core), and the one table maps `Danger` to a colour
    /// that unmistakably reads as **red** — the player sees the guard's mind with no
    /// game system ever naming a colour.
    #[test]
    fn a_chasing_guard_maps_to_danger_red() {
        use intrusion_core::GuardState;
        let category = GuardState::Chasing.category();
        assert_eq!(category, Category::Danger);
        let (r, g, b) = rgb(swatch(category).fg);
        assert!(r > g + 60 && r > b + 60, "Danger must read as red");
        assert!(r > 200, "full-range: Danger red is bright, not washed");
    }
}
