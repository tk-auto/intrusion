//! The §11.2 colour table — **the shell's one and only rendering decision, and the
//! single table a recolour edits**.
//!
//! The core tags every grid cell with an information [`Category`] and never names a
//! colour (§11.2 **[SETTLED]**); here, and nowhere else, a category becomes pixels.
//! Keeping it in its own module is what makes that claim checkable: everything the
//! shell knows about colour is in this file, and `lib.rs` is left with the boot, the
//! fit and the paint loop.

use intrusion_core::{Category, Visibility};

/// One row of the base palette (§11.2): a full-strength **foreground**, the
/// **dim** shade the same glyph draws in outside the player's FOV (§11.5 — "the
/// same glyph at low light"), and the **darkened background variants** — `bg` on
/// a live cell, `bg_dim` beyond the FOV (§11.5 fix #1: watched-but-unseen must
/// read as watched, never as safe dark-on-dark).
#[derive(Clone, Copy)]
pub(crate) struct Swatch {
    pub(crate) fg: &'static str,
    pub(crate) dim: &'static str,
    pub(crate) bg: &'static str,
    pub(crate) bg_dim: &'static str,
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
    sw("#2ee6d6", "#1f9c92", "#134540", "#0b2926"), //  7 cyan — Effect (dim keeps the tint)
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
const CYAN: Swatch = PALETTE[7];
const YELLOW: Swatch = PALETTE[10];
const ORANGE: Swatch = PALETTE[11];
const RED: Swatch = PALETTE[12];
const PURPLE: Swatch = PALETTE[13];
const TAN: Swatch = PALETTE[14];

/// The page background: true black — the full-range floor the §11.2 [START] note
/// restores (the old palette had no true black anywhere).
pub(crate) const BG: &str = BLACK.fg;

/// The **remembered** styling (§11.5a): contents known only from tile memory draw
/// in this muted slate instead of their category colour, so memory reads as memory
/// — visibly distinct from anything live *and* from the dimmed gray (asserted
/// below, with the categories).
pub(crate) const MEMORY_COLOR: &str = SLATE.fg;

/// Map an information category (§11.2) to its palette row — **the shell's one and
/// only rendering decision, and the single table a recolour edits**. The core tags
/// each cell with a [`Category`]; here, and nowhere else, category becomes pixels,
/// so an accessibility reskin is a one-table change (asserted below).
///
/// Every entry must be **visibly distinct** on the dark background (asserted
/// below): the threat ladder Caution→Warning→Danger reads as yellow→orange→red,
/// and System furniture is the muted brown-tan row rather than a bright tan that
/// would blur into Caution's yellow (the old regression).
pub(crate) fn swatch(category: Category) -> Swatch {
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
        // so the two never collide on screen. The door-change cue (§9.4) reuses this
        // same category, so a sensed guard and a sensed door change share the orange.
        Category::Sensed => ORANGE,
        // An ability effect of the player's own making (§8.3/#308/#338): cyan, a hue
        // nothing else on the board uses, so the one layer that is *advisory* can never
        // be mistaken for red detection or orange attention — the risk §11.5 names.
        // On the board it is a **background only** — a quiet teal wash, since a blast's
        // mark covers a 13×13 box and must recede under the glyphs that keep their own
        // meaning. The bright row is spent on the help card's colour key, which names
        // the category in the colour it paints.
        Category::Effect => CYAN,
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
/// **Sensed is the exception**: a guard sensed through a wall (§9.2) and a door-change
/// cue (§9.4) — the same channel — are certain, position-only knowledge, not fogged,
/// so Sensed paints at full strength (the bright [`Swatch::bg`]) regardless of `vis`,
/// an eye-catching fill rather than sinking into the dim shade the fog would otherwise
/// pick.
pub(crate) fn bg_color(bg: Category, vis: Visibility) -> &'static str {
    let swatch = swatch(bg);
    // Sensed is certain, position-only knowledge painted through walls (§9.2/§9.4) —
    // both a guard and a door change — never fogged, so it paints at full strength
    // (the bright [`Swatch::bg`]) regardless of `vis`, rather than sinking into the dim
    // shade the fog would otherwise pick for an out-of-FOV cell.
    //
    // The **effect layer** (§8.3/#308) takes the same exception, and for the same
    // reason: how far your own gadget reaches is certain knowledge, through walls and
    // over ground you have never seen. Most of a 13×13 footprint falls outside the
    // forward FOV, so fogging it would teach the extent only where the player was
    // already looking — which is precisely the corner the flash exists to light.
    if matches!(bg, Category::Sensed | Category::Effect) {
        return swatch.bg;
    }
    match vis {
        Visibility::Live => swatch.bg,
        // Threat outranks knowledge (§11.5 **[SETTLED]**): a watched cell in a wing
        // the player has never entered still paints the red overlay, exactly as an
        // explored one does. The schematic changes what the *glyph* claims, never
        // what the detection set says — fix #1 (watched must never look safe) holds
        // over unexplored ground too.
        Visibility::Explored | Visibility::Unexplored | Visibility::Remembered => swatch.bg_dim,
    }
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
            // Since #338 the effect layer paints no glyph on the board, but the help
            // card's colour key names it in this colour beside every other category, so
            // it still has to be tellable from all of them.
            Category::Effect,
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

    /// §8.3/§11.5 (#308/#338): the **effect layer** must be tellable at a glance from
    /// both of the meanings it sits beside — red detection and orange attention — or
    /// the board degrades into "some coloured backgrounds", which is the one risk the
    /// ticket names. Its wash is deliberately quiet (it covers a 13×13 box) but must
    /// still read against the page, and like `Sensed` it paints at full strength in and
    /// out of the FOV: how far your own gadget reaches is certain knowledge.
    ///
    /// Since #338 it is a **background only**, so the check that matters most is the
    /// last one: every glyph that can stand on an effect mark keeps its own colour, and
    /// all of them must stay legible over the wash. If one ever failed, §11.2's rule is
    /// to shift *this* colour — the channel is not negotiable, the hue is.
    #[test]
    fn the_effect_layer_is_distinct_from_danger_and_sensed() {
        const MIN_BG_DIST2: i32 = 40 * 40;
        let effect = bg_color(Category::Effect, Visibility::Explored);
        assert_eq!(
            effect,
            bg_color(Category::Effect, Visibility::Live),
            "the effect wash is full-strength in and out of the FOV alike",
        );

        let d = dist2(rgb(effect), rgb(BG));
        assert!(
            d >= MIN_BG_DIST2,
            "the effect wash vanishes into the page background (dist^2 {d})"
        );
        // Cyan: green and blue both clearly above red — the one hue nothing else on the
        // board uses, so it cannot be read as a threat level.
        let (r, g, b) = rgb(effect);
        assert!(
            g > r + 20 && b > r + 20,
            "the effect wash must read as cyan, not as another threat colour"
        );

        for other in [
            bg_color(Category::Danger, Visibility::Live),
            bg_color(Category::Danger, Visibility::Explored),
            bg_color(Category::Sensed, Visibility::Live),
        ] {
            let d = dist2(rgb(effect), rgb(other));
            assert!(
                d >= MIN_BG_DIST2,
                "the effect wash blurs into {other} (dist^2 {d})"
            );
        }
        // Every glyph the board can draw **over** an effect mark (#338) must still read
        // against it: the threat ladder, because a held guard keeps its ladder colour,
        // and `Owned`, because the player and their decoy are the marks still to come
        // (#340/#341). Floor dots are exempt — `Ground` recedes by design, and a wash
        // it disappeared into would be the wash doing its job.
        for over in [
            Category::Caution,
            Category::Warning,
            Category::Danger,
            Category::Owned,
        ] {
            let d = dist2(rgb(swatch(over).fg), rgb(effect));
            assert!(
                d >= MIN_BG_DIST2,
                "{over:?} is unreadable over the effect wash (dist^2 {d}) — shift the \
                 effect colour, never the channel (§11.2)"
            );
        }
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
        let dimmed = bg_color(Category::Danger, Visibility::Explored);
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
        let sensed = bg_color(Category::Sensed, Visibility::Explored);
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
            bg_color(Category::Danger, Visibility::Explored),
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
