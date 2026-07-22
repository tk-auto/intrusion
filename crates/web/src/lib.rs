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
//! and its repeat timers — lives in the [`input`] module; this file keeps the
//! boot, the fit, and the palette.
//! The **whole level is always visible with no scrolling**, on desktop and
//! mobile alike: the canvas is scaled to fit the viewport (aspect preserved) and its
//! backing store is sized in device pixels so glyphs stay crisp; a resize/orientation
//! change recomputes and redraws. The grid arrives already fogged (§11.5a) and
//! overlaid (§11.5 — `Danger` backgrounds on cells watched by visible guards); this
//! shell maps each cell's knowledge state to styling: full category colour live,
//! the row's dim shade out of FOV (dark gray for most; quieter for floor dots,
//! tinted for the exit), muted slate remembered, and two red background shades for
//! the danger overlay. Colours come from the §11.2 base palette below — a full-range,
//! colour-blind-safe 16-colour set behind a single category→swatch table. The frame
//! is the full §11.4 *screen* — the always-on ability line on top, the map, and the
//! near and usable status lines beneath, all composed by `core::render_screen` from
//! the game state plus the shell's `ScreenUi` view state. Keys map through
//! `core::input_for_key` (§11.6) for game actions and `core::ui_command_for_key` for
//! view toggles (`Tab` deploys the ability panel); a tap on the deploy button
//! (`core::is_ability_button`) does the same for touch — so the picture, the
//! bindings, and the button's geometry are all pinned by native tests.
//! Levels come fully placed from the core (`generate_level`, §10.1.7–9): entry/exit
//! and player in the largest room, intel spread across rooms, guards seated where
//! none eyes the spawn on turn one — and the guards arrive as live patrolling
//! actors (§7.5) straight from `Placement::guards`, so the shell never decides what
//! a placed guard is; it just hands what placement built to the core.

mod input;

use std::cell::RefCell;
use std::rc::Rc;

use intrusion_core::{
    generate_level, render_screen, Category, Direction, Grid, LevelConfig, Rng, ScreenUi, State,
    Visibility, HEADER_ROWS, STATUS_ROWS,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement, Window};

/// The glyph cell's base aspect (width:height); a monospace glyph reads best in a
/// slightly tall box. Actual on-screen cell size is this scaled to fit the viewport.
const CELL_W: f64 = 14.0;
const CELL_H: f64 = 20.0;

/// One row of the base palette (§11.2): a full-strength **foreground**, the
/// **dim** shade the same glyph draws in outside the player's FOV (§11.5 — "the
/// same glyph at low light"), and the **darkened background variants** — `bg` on
/// a live cell, `bg_dim` beyond the FOV (§11.5 fix #1: watched-but-unseen must
/// read as watched, never as safe dark-on-dark).
#[derive(Clone, Copy)]
struct Swatch {
    fg: &'static str,
    dim: &'static str,
    bg: &'static str,
    bg_dim: &'static str,
}

const fn sw(fg: &'static str, dim: &'static str, bg: &'static str, bg_dim: &'static str) -> Swatch {
    Swatch {
        fg,
        dim,
        bg,
        bg_dim,
    }
}

/// The standard §11.5 dim: out-of-FOV geometry collapses to this one dark gray —
/// dim but legible — for most rows. Distinct from [`MEMORY_COLOR`] so the three
/// knowledge states never collapse into two (§11.5a's note; asserted below). The
/// exceptions carry their own dim: Ground recedes further (the dots must whisper),
/// and Interest keeps a readable purple tint — the exit anchors every escape plan
/// (§7.6) and §11.5a keeps it always visible, so it must not vanish into wall gray.
const STD_DIM: &str = "#4a4a4a";

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
    sw("#000000", "#000000", "#000000", "#000000"), //  0 true black — the page backdrop
    sw("#ffffff", STD_DIM, "#5c5c5c", "#2e2e2e"),   //  1 true white — Neutral
    sw("#4a4a4a", "#262626", "#1e1e1e", "#121212"), //  2 dark gray — Ground (floor dots)
    sw("#a8a8a8", STD_DIM, "#434343", "#222222"),   //  3 light gray — spare (secondary text)
    sw("#667a8a", STD_DIM, "#293138", "#14181c"),   //  4 slate — tile memory (§11.5a)
    sw("#4ea6ff", STD_DIM, "#1f4266", "#102133"),   //  5 blue — Owned
    sw("#2456b8", STD_DIM, "#0e224a", "#071125"),   //  6 deep blue — spare
    sw("#2ee6d6", STD_DIM, "#0d2523", "#081413"),   //  7 cyan — spare
    sw("#3ecf5a", STD_DIM, "#195324", "#0c2a12"),   //  8 green — spare
    sw("#157f33", "#0e3f1a", "#083314", "#04190a"), //  9 deep green — spare (darker than STD_DIM)
    sw("#f0e442", STD_DIM, "#605b1a", "#302e0d"),   // 10 yellow — Caution
    sw("#e69f00", STD_DIM, "#5c4000", "#2e2000"),   // 11 orange — Warning
    sw("#ff3333", STD_DIM, "#8c2020", "#521717"),   // 12 red — Danger
    sw("#bd6bd6", "#8a4a9e", "#4c2b56", "#26152b"), // 13 purple — Interest (dim keeps the tint)
    sw("#9a7040", STD_DIM, "#3e2d1a", "#1f160d"),   // 14 tan — System
    sw("#ff7ab8", STD_DIM, "#66314a", "#331825"),   // 15 pink — spare
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
        Category::Ground => DIM_GRAY, // floor dots — drawn to recede (§11.5)
        Category::Owned => BLUE,      // you and what you made
        Category::Caution => YELLOW,  // a threat, unaware
        Category::Warning => ORANGE,  // a threat, hunting
        Category::Danger => RED,      // a threat that has you
        Category::Interest => PURPLE, // goals and rewards
        Category::System => TAN,      // doors, hideouts — neutral furniture
        // A guard sensed through a wall (§9.2): an orange *background* highlight, the
        // eye-catching parallel of the red danger overlay. It shares Warning's orange
        // hue but never its role — Sensed only ever paints a background, never a glyph,
        // so the two never collide on screen.
        Category::Sensed => ORANGE,
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
    // player, exit, intel and guards placed by the §10.1.7–9 rules. Guards patrol
    // their territories (§7.5); the reactive states ride on the same seam.
    let (layout, placement) = generate_level(&LevelConfig::V1, &mut Rng::new(seed))
        .map_err(|e| JsValue::from_str(&format!("generation failed: {e:?}")))?;

    let state = State::new(
        layout,
        placement.player(),
        Direction::North,
        placement.guards(),
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
        ui: ScreenUi::default(),
    }));
    game.borrow_mut().fit_and_draw(); // size to the viewport and paint the first frame
    input::install_input(&document, &game)?;
    input::install_gestures(&document, &game)?;
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
}

impl Game {
    /// Fit the canvas to the viewport and redraw. Compute a uniform scale so the whole
    /// `cols × rows` grid fits within the window on both axes (aspect preserved), size
    /// the backing store in device pixels for crisp glyphs, set the CSS size so the
    /// element itself fits (no scrolling), and paint. Called at boot and on every
    /// resize / orientation change.
    fn fit_and_draw(&mut self) {
        let facility = self.state.layout().facility();
        // The screen is the map plus the §11.4 ability line above it and the status
        // lines beneath it.
        let (cols, rows) = (
            facility.width() as f64,
            (facility.height() + HEADER_ROWS + STATUS_ROWS) as f64,
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
        self.draw();
    }

    /// Draw one frame: ask the core to render the whole §11.4 screen (map, near
    /// line, usable line — glyphs, overlaps and categories all decided there),
    /// then blit it — colour by category, glyph as given.
    fn draw(&self) {
        paint(
            &self.ctx,
            &render_screen(&self.state, self.ui),
            &self.metrics,
        );
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
                // Out-of-FOV geometry: the row's dim shade (§11.5) — the standard
                // dark gray for most, quieter for Ground, tinted for the exit.
                Visibility::Dimmed => swatch(cell.fg).dim,
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
/// [`Swatch::bg_dim`] beyond the FOV. The §11.5 danger overlay paints two shades —
/// bright red in view, darker-but-still-red out of it (fix #1: watched must never
/// look safe) — and any category a future system declares arrives with its variants
/// ready. The §7.6 certain/glimpse zones add two *detection* shades when two-zone
/// detection lands; until then the whole cone is one zone.
///
/// **Sensed is the exception**: a guard sensed through a wall (§9.2) is *always* out
/// of the FOV, yet its position is certain knowledge, not fogged — so it paints at
/// full strength (the bright [`Swatch::bg`]) regardless of `vis`, an eye-catching
/// orange fill rather than sinking into the dim shade the fog would otherwise pick.
fn bg_color(bg: Category, vis: Visibility) -> &'static str {
    let swatch = swatch(bg);
    if bg == Category::Sensed {
        return swatch.bg;
    }
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

    /// Every category must map to a **visibly distinct** colour. The regression this
    /// guards: `System` (doors, hideouts) once sat a tan hair away from `Caution`
    /// (unaware guards), so doors, hideouts and guards all read as one yellow. The
    /// threat ladder Caution→Warning→Danger and the furniture brown must stay apart.
    #[test]
    fn category_colours_are_all_visibly_distinct() {
        // Every category drawn as a *foreground glyph*. `Sensed` is excluded on
        // purpose: it only ever paints a background (§9.2), and it deliberately shares
        // Warning's orange — a fg-distinctness check over it would be meaningless.
        let categories = [
            Category::Neutral,
            Category::Ground,
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
        let d = dist2(rgb(STD_DIM), rgb(MEMORY_COLOR));
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

    /// §9.2: the **sensed** background is the eye-catching orange parallel of the red
    /// danger overlay — it must read on the page background, read as *orange* (not
    /// red), and stay clearly tellable from the danger fill so a sensed cell is never
    /// mistaken for a watched one. It is painted at full strength regardless of `vis`
    /// (the position is certain knowledge, §11.5a), so both visibilities agree.
    #[test]
    fn the_sensed_background_is_orange_and_distinct_from_danger() {
        const MIN_BG_DIST2: i32 = 40 * 40;
        let sensed = bg_color(Category::Sensed, Visibility::Dimmed);
        assert_eq!(
            sensed,
            bg_color(Category::Sensed, Visibility::Live),
            "the sensed fill is full-strength in and out of the FOV alike",
        );

        let d = dist2(rgb(sensed), rgb(BG));
        assert!(
            d >= MIN_BG_DIST2,
            "the sensed fill vanishes into the page background (dist^2 {d})"
        );
        // Orange, not red: red and green both present, and green clearly above blue.
        let (r, g, b) = rgb(sensed);
        assert!(
            r > b + 30 && g > b + 20,
            "the sensed fill must read as orange"
        );

        // Clearly apart from the danger red, both shades — a sensed cell and a watched
        // cell must never look alike.
        for danger in [
            bg_color(Category::Danger, Visibility::Live),
            bg_color(Category::Danger, Visibility::Dimmed),
        ] {
            let d = dist2(rgb(sensed), rgb(danger));
            assert!(
                d >= MIN_BG_DIST2,
                "the sensed orange blurs into the danger red {danger} (dist^2 {d})"
            );
        }
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
            // The dim shade is the same glyph at *low* light (§11.5): always
            // strictly darker than the row's foreground, whichever dim it uses.
            assert!(
                lum(s.dim) < lum(s.fg),
                "{}'s dim shade {} is not darker",
                s.fg,
                s.dim
            );
        }
    }

    /// The floor-dot readability rule (§11.5): **Ground recedes**. Its live colour
    /// is dimmer than every other category's — the dots are there to carry the FOV
    /// edge, not to compete with walls and entities — and its own dim shade sits
    /// far enough below it that the edge still reads across open ground.
    #[test]
    fn ground_recedes_beneath_every_other_category() {
        let lum = |hex: &str| {
            let (r, g, b) = rgb(hex);
            r + g + b
        };
        let ground = swatch(Category::Ground);
        for c in [
            Category::Neutral,
            Category::Owned,
            Category::Caution,
            Category::Warning,
            Category::Danger,
            Category::Interest,
            Category::System,
        ] {
            assert!(
                lum(ground.fg) < lum(swatch(c).fg),
                "a floor dot outshines {c:?}"
            );
        }
        let d = dist2(rgb(ground.fg), rgb(ground.dim));
        assert!(
            d >= 2500,
            "live and dimmed ground blur (dist^2 {d}) — the FOV edge would vanish"
        );
    }

    /// §7.6/§11.5a: the exit anchors every escape plan and is always visible — so
    /// out of the FOV the `E` must not sink into wall gray the way it briefly did.
    /// Interest's dim shade still reads as purple, apart from both the standard
    /// dim and the memory slate (a dim exit is not a remembered content).
    #[test]
    fn the_dimmed_exit_still_reads_as_a_goal() {
        let dim = swatch(Category::Interest).dim;
        let (r, g, b) = rgb(dim);
        assert!(r > g + 30 && b > g + 30, "{dim} must still read as purple");
        let d = dist2(rgb(dim), rgb(STD_DIM));
        assert!(d >= 70 * 70, "the dim exit blurs into dimmed walls ({d})");
        let d = dist2(rgb(dim), rgb(MEMORY_COLOR));
        assert!(d >= 70 * 70 / 2, "the dim exit impersonates memory ({d})");
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
