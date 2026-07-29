//! The §11.2 colour table — **the shell's one and only rendering decision, and the
//! single table a recolour edits**.
//!
//! The core tags every grid cell with an information [`Category`] and never names a
//! colour (§11.2 **[SETTLED]**); here, and nowhere else, a category becomes pixels.
//! Keeping it in its own module is what makes that claim checkable: everything the
//! shell knows about colour is in this file, and `lib.rs` is left with the boot, the
//! fit and the paint loop.

use intrusion_core::{Category, Fill, Theme};

/// One row of the base palette (§11.2): a full-strength **foreground**, the
/// **dim** shade the same glyph draws in outside the player's FOV (§11.5 — "the
/// same glyph at low light"), and the two **background variants** — `bg` on
/// a live cell, `bg_dim` beyond the FOV (§11.5 fix #1: watched-but-unseen must
/// read as watched, never as safe dark-on-dark).
///
/// **All four columns carry the §11.2 guarantees, not just `fg`** (#419). The
/// background pair used to be toned by eye while the tests only ever measured
/// foregrounds, and it showed: on the dark theme the threat ladder at `bg_dim` was
/// three near-blacks separated by hue alone, at the luminance where hue
/// discrimination is worst. Every pair is now asserted visibly distinct at
/// background strength too, the ladder is separated by luminance there as it is at
/// full strength, each row's own two variants are tellable apart, and the near
/// line's Neutral words are asserted legible over every band (§11.4).
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

/// A row of the palette, by **role** rather than by colour — the one thing both
/// themes agree on. A category resolves to a row here ([`row`]) and the theme
/// decides what colour that row is, so the two tables can never disagree about
/// *which* row Danger reads from, only about what it looks like.
///
/// The hue names survive the theme (red stays red, gold stays gold); only the first
/// two are named for their job, because they swap: the page is black and the ink is
/// white on dark, and the other way round on light.
/// A spare row stays reachable only through the table itself until a system claims
/// and names it here — the same convention the single palette kept.
const PAGE: usize = 0;
const INK: usize = 1;
const FLOOR: usize = 2;
const SLATE: usize = 4;
const BLUE: usize = 5;
const CYAN: usize = 7;
const GOLD: usize = 10;
const ORANGE: usize = 11;
const RED: usize = 12;
const PURPLE: usize = 13;
const TAN: usize = 14;

/// One whole theme: the sixteen rows, plus the shared dim they mostly draw in out
/// of the FOV. The page backdrop and the memory slate are *rows*, not extra fields
/// — [`Palette::page`] and [`Palette::memory`] read them — so a theme is one table
/// and nothing beside it.
struct Palette {
    rows: [Swatch; 16],
}

impl Palette {
    /// The page backdrop — the colour every unpainted cell is, and the floor the
    /// §11.5 shades recede toward.
    const fn page(&self) -> &'static str {
        self.rows[PAGE].fg
    }

    /// The **remembered** styling (§11.5a): contents known only from tile memory
    /// draw in this muted slate instead of their category colour, so memory reads as
    /// memory — visibly distinct from anything live *and* from the dim (asserted
    /// below, with the categories).
    const fn memory(&self) -> &'static str {
        self.rows[SLATE].fg
    }
}

/// The dark theme's **standard §11.5 dim**: out-of-FOV geometry collapses to this
/// one dark gray — dim but legible — for most rows, receding toward the black page.
/// Distinct from the memory slate so the three knowledge states never collapse into
/// two (§11.5a's note; asserted below). Three rows carry their own dim instead:
/// Ground recedes further (the dots must whisper), Interest keeps a readable purple
/// tint — the exit anchors every escape plan (§7.6) and §11.5a keeps it always
/// visible, so it must not vanish into wall gray — and Effect keeps its cyan for the
/// help card's colour key.
const DARK_DIM: &str = "#4a4a4a";

/// The **dark** palette (§11.2), the game's original and its [`Default`]: a
/// **16-colour, colour-blind-safe qualitative set**, each row a foreground plus
/// darkened background variants. **Full-range [START]** — true black and true white
/// are both here, deliberately: the old palette's gamma curve compressed everything
/// into a washed 0.1–0.9 band with six colours never used at all. Compression gets
/// added back only if something demands it.
///
/// Hues lean on the Okabe–Ito colour-blind-safe set (brightened for the dark
/// backdrop), and the threat ladder yellow→orange→red is additionally separated
/// by luminance so it survives a red-green deficiency; every pair is asserted
/// visibly distinct below. Ten rows carry the §11.2 categories today; the
/// spare rows are ready for the message bar, ability labels, and any category
/// yet to come — claimed by naming them, like the row constants above.
///
/// The **background pair sits well off the page** (#419). Both variants keep their
/// row's hue and are placed by luminance: each rung of the ladder a real step below
/// the last, in the same descending direction the foregrounds take, so a band reads
/// as its rung rather than as "a dark warm colour". Where the dark tier compressed
/// two hues together — tan against orange, cyan against blue, either against the
/// neutral gray — the shade carries **more** saturation than a straight scaling of
/// its foreground would give it, because a dark colour needs more of it to read as a
/// hue at all. The old values put the whole warm cluster inside `#2e2000`–`#521717`
/// and they were, correctly, reported as indistinguishable in play.
const DARK: Palette = Palette {
    rows: [
        sw("#000000", "#000000", "#000000", "#000000"), //  0 true black — the page backdrop
        sw("#ffffff", DARK_DIM, "#646464", "#373737"),  //  1 true white — Neutral
        sw("#4a4a4a", "#262626", "#202020", "#121212"), //  2 dark gray — Ground (floor dots)
        sw("#a8a8a8", DARK_DIM, "#535353", "#353535"),  //  3 light gray — spare (secondary text)
        sw("#667a8a", DARK_DIM, "#303941", "#1e2328"),  //  4 slate — tile memory (§11.5a)
        sw("#4ea6ff", DARK_DIM, "#2a649e", "#194169"),  //  5 blue — Owned
        sw("#2456b8", DARK_DIM, "#183878", "#0e2248"),  //  6 deep blue — spare
        sw("#2ee6d6", "#1f9c92", "#087e74", "#005c53"), //  7 cyan — Effect (dim keeps the tint)
        sw("#3ecf5a", DARK_DIM, "#2b903f", "#1b5927"),  //  8 green — spare
        sw("#157f33", "#0e3f1a", "#126c2c", "#0b401a"), //  9 deep green — spare (darker than the std dim)
        sw("#f0e442", DARK_DIM, "#9b932b", "#777121"),  // 10 yellow — Caution
        sw("#e69f00", DARK_DIM, "#ab7700", "#795400"),  // 11 orange — Warning / Sensed
        sw("#ff3333", DARK_DIM, "#b32424", "#6b1515"),  // 12 red — Danger
        sw("#bd6bd6", "#8a4a9e", "#7c468d", "#573163"), // 13 purple — Interest (dim keeps the tint)
        sw("#9a7040", DARK_DIM, "#714a1d", "#4a2c0b"),  // 14 tan — System
        sw("#ff7ab8", DARK_DIM, "#7f3d5c", "#502639"),  // 15 pink — spare
    ],
};

/// The light theme's standard dim — a light gray, receding toward the white page.
/// Same job as [`DARK_DIM`], opposite direction; the exceptions are the same three
/// rows, for the same reasons.
const LIGHT_DIM: &str = "#b0b0b0";

/// The **light** palette (§11.2/#189): the same sixteen roles re-toned for a white
/// page. Every §11.5 guarantee the dark table holds, this one holds too — the tests
/// below run over both — but the values are **re-chosen, not inverted**, because
/// nothing about the dark table survives a formula:
///
/// - **The hues move.** Okabe–Ito's yellow is bright by construction and vanishes on
///   white, so Caution becomes a dark gold; Danger loses the brightness that made it
///   shout on black and gets its emphasis from contrast against the page instead.
///   The gold/orange/tan cluster is the whole difficulty of a light theme — three
///   warm hues that must stay pairwise distinct while all three are dark enough to
///   read on white — and it is why Caution keeps as much green as it can and Warning
///   is pushed toward red.
/// - **The shades reverse direction.** On black, `dim`, `bg` and `bg_dim` are
///   *darkened* variants receding toward the page; here they are *lightened* ones,
///   receding toward the page just the same. "Darken" was never the rule — "move
///   toward the backdrop" was, and the test that pinned the dark-only spelling now
///   measures distance to [`Palette::page`]. The same holds for the #419 re-tone:
///   what the dark theme achieved by lifting its backgrounds *off* black, this one
///   achieves by pulling them *away* from white, and the constraint both satisfy is
///   the one written in terms of the page.
/// - **Rows 0 and 1 swap.** The page is white and the ink is black, which is exactly
///   why those two rows are named for their role rather than their colour.
const LIGHT: Palette = Palette {
    rows: [
        sw("#ffffff", "#ffffff", "#ffffff", "#ffffff"), //  0 true white — the page backdrop
        sw("#000000", LIGHT_DIM, "#858585", "#adadad"), //  1 true black — Neutral
        sw("#b8b8b8", "#d8d8d8", "#dadada", "#ededed"), //  2 light gray — Ground (floor dots)
        sw("#8a8a8a", LIGHT_DIM, "#9d9d9d", "#c5c5c5"), //  3 mid gray — spare (secondary text)
        sw("#3f5f80", LIGHT_DIM, "#a7b6c5", "#cdd5de"), //  4 slate — tile memory (§11.5a)
        sw("#0060c0", LIGHT_DIM, "#64a3e3", "#8dc0f2"), //  5 blue — Owned
        sw("#082a72", LIGHT_DIM, "#8a9abc", "#bbc5d8"), //  6 deep blue — spare
        sw("#00857c", "#8fcfc9", "#73cbc6", "#a2e0dc"), //  7 cyan — Effect (dim keeps the tint)
        sw("#1a8f38", LIGHT_DIM, "#99cda7", "#c6e3cd"), //  8 green — spare
        sw("#0a4a1a", LIGHT_DIM, "#9eb7a4", "#c7d6cb"), //  9 deep green — spare
        sw("#b09600", LIGHT_DIM, "#dbcf8b", "#ebe5bf"), // 10 gold — Caution
        sw("#cc4c00", LIGHT_DIM, "#eb9966", "#fbbc97"), // 11 orange — Warning / Sensed
        sw("#b00000", LIGHT_DIM, "#d16c6c", "#df9797"), // 12 red — Danger
        sw("#7b2fa0", "#b070d8", "#af80c5", "#ceb2dc"), // 13 purple — Interest (dim keeps the tint)
        sw("#6b4320", LIGHT_DIM, "#9e7e60", "#b1977f"), // 14 tan — System
        sw("#c02a72", LIGHT_DIM, "#d87aa7", "#e8b2cc"), // 15 pink — spare
    ],
};

/// The table the given [`Theme`] paints from — **the entire extent of the shell's
/// theme knowledge**. Everything downstream reads a row from whatever this returns,
/// so a third theme would be one more constant and one more arm here.
const fn palette(theme: Theme) -> &'static Palette {
    match theme {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}

/// The page background for a theme (§11.2) — what [`crate::paint`] fills before it
/// draws a glyph, and the colour the dim and background shades recede toward.
pub(crate) fn page(theme: Theme) -> &'static str {
    palette(theme).page()
}

/// The **remembered** styling for a theme (§11.5a) — see [`Palette::memory`].
pub(crate) fn memory(theme: Theme) -> &'static str {
    palette(theme).memory()
}

/// Map an information category (§11.2) to its palette **row** — **the shell's one
/// and only rendering decision, and the single table a recolour edits**. The core
/// tags each cell with a [`Category`]; here, and nowhere else, category becomes
/// pixels, so an accessibility reskin is a one-table change (asserted below), and a
/// whole extra theme is a second column of that one table (#189).
///
/// Every entry must be **visibly distinct** on its backdrop, in both themes
/// (asserted below): the threat ladder Caution→Warning→Danger reads as
/// yellow→orange→red, and System furniture is the muted brown-tan row rather than a
/// bright tan that would blur into Caution's yellow (the old regression).
const fn row(category: Category) -> usize {
    match category {
        Category::Neutral => INK,     // inert scenery, walls, spent objectives
        Category::Ground => FLOOR,    // floor dots — drawn to recede (§11.5)
        Category::Owned => BLUE,      // you and what you made
        Category::Caution => GOLD,    // a threat, unaware
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

/// The [`Swatch`] a category draws from in a theme — [`row`] resolved against
/// [`palette`]. The row is the meaning and the theme is the appearance, so no caller
/// ever has to know both.
pub(crate) fn swatch(theme: Theme, category: Category) -> Swatch {
    palette(theme).rows[row(category)]
}

/// Map a background category to a fill through the same table as the glyphs: the
/// [`Swatch::bg`] variant on a cell the player sees, the further-receded
/// [`Swatch::bg_dim`] beyond the FOV. The §11.5 danger overlay paints two shades —
/// the stronger red in view, the quieter-but-still-red out of it (fix #1: watched
/// must never look safe) — and any category a future system declares arrives with
/// its variants ready. The §7.6 certain/glimpse zones add two *detection* shades when
/// two-zone detection lands; until then the whole cone is one zone.
///
/// **Sensed is the exception**: a guard sensed through a wall (§9.2) and a door-change
/// cue (§9.4) — the same channel — are certain, position-only knowledge, not fogged,
/// so Sensed paints at full strength (the bright [`Swatch::bg`]) regardless of `vis`,
/// an eye-catching fill rather than sinking into the dim shade the fog would otherwise
/// pick.
pub(crate) fn bg_color(theme: Theme, bg: Category, fill: Fill) -> &'static str {
    let swatch = swatch(theme, bg);
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
    match fill {
        // Threat outranks knowledge (§11.5 **[SETTLED]**): a watched cell in a wing
        // the player has never entered still paints the red overlay, exactly as an
        // explored one does. The schematic changes what the *glyph* claims, never
        // what the detection set says — fix #1 (watched must never look safe) holds
        // over unexplored ground too.
        Fill::Full => swatch.bg,
        Fill::Quiet => swatch.bg_dim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every invariant below runs over both themes** (#189). §11.5's guarantees are
    /// claims about what the player can tell apart, not about a particular set of hex
    /// values, so a second palette that quietly broke one would be a second palette
    /// that lies. Written as a loop over this rather than duplicated per theme, so a
    /// third theme inherits the whole suite by appearing here.
    const THEMES: [Theme; 2] = [Theme::Dark, Theme::Light];

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

    fn lum(hex: &str) -> i32 {
        let (r, g, b) = rgb(hex);
        r + g + b
    }

    /// **How far a colour stands off the page** — the measure that replaced "darker"
    /// when the second theme arrived (#189).
    ///
    /// On black, every §11.5 shade is a *darkened* variant and "darker" and "closer
    /// to the backdrop" are the same sentence; on white they part company, and only
    /// the second one was ever the rule. A dim glyph is the same glyph at low
    /// contrast, a background variant is one that cannot outshine the glyph standing
    /// on it — both are statements about distance from the page, and both hold in
    /// either direction once measured this way.
    fn from_page(theme: Theme, hex: &str) -> i32 {
        (lum(hex) - lum(page(theme))).abs()
    }

    /// The shared §11.5 dim, read off the row **walls actually draw in** rather than
    /// off the constant beside the table — so these checks measure what the board
    /// paints, not what the palette meant to paint.
    fn std_dim(theme: Theme) -> &'static str {
        swatch(theme, Category::Neutral).dim
    }

    /// Every category drawn as a *foreground glyph*. `Sensed` is excluded on
    /// purpose: it only ever paints a background (§9.2), and it deliberately shares
    /// Warning's orange — a fg-distinctness check over it would be meaningless.
    const CATEGORIES: [Category; 9] = [
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

    /// ~70 in RGB distance: the old tan/yellow clash measured ~61 and must fail.
    const MIN_DIST2: i32 = 70 * 70;

    /// Squared distance for large background fills: 40 per channel is an easy
    /// read on area colour even where 70 is the bar for thin glyph strokes.
    const MIN_BG_DIST2: i32 = 40 * 40;

    /// Every category that paints **area colour** — a near-line band (§11.4), the
    /// §11.5 danger overlay, the §9.2 sensed fill, the §8.3 effect wash.
    ///
    /// Two categories are deliberately absent, and both for reasons that would make
    /// the check meaningless rather than for convenience:
    ///
    /// - **`Ground`** is the one category whose job is to *recede* (§11.5). Its fill is
    ///   the absence of a fill, and demanding it be tellable from Neutral's would be
    ///   demanding it stop doing that job. It is never a message band.
    /// - **`Sensed`** shares `Warning`'s row on purpose (§9.2) — it is the same orange,
    ///   used where Warning never paints — so a distinctness check over the pair would
    ///   assert against the design.
    const BG_CATEGORIES: [Category; 8] = [
        Category::Neutral,
        Category::Owned,
        Category::Caution,
        Category::Warning,
        Category::Danger,
        Category::Interest,
        Category::System,
        Category::Effect,
    ];

    /// How far the near line's **words** must stand off the band behind them
    /// (§11.4/#419), in summed-channel luminance — 100 per channel.
    ///
    /// The row is a solid category band with its text in [`Category::Neutral`], so
    /// every band the palette can produce has to leave that ink readable. This is the
    /// bound that stops "lift the backgrounds until the ladder reads" from being
    /// answered by lifting them until the words stop being.
    const MIN_BAND_CONTRAST: i32 = 300;

    /// The background variant a category paints at each knowledge state, as the board
    /// actually paints it ([`bg_color`]) — so the `Sensed`/`Effect` full-strength
    /// exception is honoured here rather than re-derived, and a check can never assert
    /// something about a shade the screen never shows.
    fn bg_shades(theme: Theme, c: Category) -> [&'static str; 2] {
        [
            bg_color(theme, c, Fill::Full),
            bg_color(theme, c, Fill::Quiet),
        ]
    }

    /// Every category must map to a **visibly distinct** colour, in either theme. The
    /// regression this guards: `System` (doors, hideouts) once sat a tan hair away
    /// from `Caution` (unaware guards), so doors, hideouts and guards all read as one
    /// yellow. The threat ladder Caution→Warning→Danger and the furniture brown must
    /// stay apart — and on a white page that is the *harder* half of the problem,
    /// since gold, orange and tan must all be dark enough to read there.
    #[test]
    fn category_colours_are_all_visibly_distinct() {
        for theme in THEMES {
            for (i, &a) in CATEGORIES.iter().enumerate() {
                for &b in &CATEGORIES[i + 1..] {
                    let d = dist2(rgb(swatch(theme, a).fg), rgb(swatch(theme, b).fg));
                    assert!(
                        d >= MIN_DIST2,
                        "{theme:?}: {a:?} and {b:?} are too close to tell apart \
                         (dist^2 {d} < {MIN_DIST2})"
                    );
                }
            }
            // The §11.5a remembered styling must stand apart from every live category —
            // memory that could be mistaken for a live glyph would defeat the three
            // visual states the design demands.
            for &c in &CATEGORIES {
                let d = dist2(rgb(memory(theme)), rgb(swatch(theme, c).fg));
                assert!(
                    d >= MIN_DIST2,
                    "{theme:?}: the remembered colour is too close to {c:?} \
                     (dist^2 {d} < {MIN_DIST2})"
                );
            }
            // And the dim gray must not collapse into the remembered slate — three
            // knowledge states, not two (§11.5a's implementation note).
            let d = dist2(rgb(std_dim(theme)), rgb(memory(theme)));
            assert!(
                d >= MIN_DIST2 / 2,
                "{theme:?}: dimmed and remembered blur (dist^2 {d})"
            );
        }
    }

    /// #419: **the same demand at background strength as at foreground.** Every pair of
    /// area colours the board can put side by side must be tellable apart — in view and
    /// beyond it alike, in both themes.
    ///
    /// The regression this pins is the one reported in play: on the dark theme the
    /// threat ladder beyond the FOV was `#302e0d` / `#2e2000` / `#521717` on a black
    /// page — three near-blacks separated almost entirely by hue, at the luminance where
    /// hue discrimination is worst — and nothing in the suite objected, because every
    /// distinctness guarantee was asserted on the `fg` column alone.
    ///
    /// Measured through [`bg_color`] rather than off the [`Swatch`], so it compares what
    /// the screen actually paints: an out-of-FOV `Sensed` or `Effect` cell paints its
    /// full-strength fill beside an out-of-FOV `Danger` cell's dimmed one, and those two
    /// are exactly the pair a player has to tell apart.
    #[test]
    fn background_fills_are_all_visibly_distinct() {
        for theme in THEMES {
            for (tier, label) in [(0, "in view"), (1, "beyond it")] {
                for (i, &a) in BG_CATEGORIES.iter().enumerate() {
                    for &b in &BG_CATEGORIES[i + 1..] {
                        let (fill_a, fill_b) =
                            (bg_shades(theme, a)[tier], bg_shades(theme, b)[tier]);
                        let d = dist2(rgb(fill_a), rgb(fill_b));
                        assert!(
                            d >= MIN_BG_DIST2,
                            "{theme:?} ({label}): {a:?} {fill_a} and {b:?} {fill_b} are too \
                             close to tell apart (dist^2 {d} < {MIN_BG_DIST2})"
                        );
                    }
                }
            }
        }
    }

    /// §11.5 fix #1, generalised (#419): **a cell you can see and a cell beyond it must
    /// not look the same**, for every category that paints a fill — not only for the
    /// danger overlay, which was the one pair the suite happened to check.
    ///
    /// `Sensed` and `Effect` are excluded because they have no second shade *by design*
    /// (§9.2/§8.3): both are certain, position-only knowledge and paint at full strength
    /// in and out of the FOV. Their exemption is asserted where it belongs — in the two
    /// tests that own it — rather than weakened into a skip here.
    #[test]
    fn each_row_separates_the_seen_cell_from_the_one_beyond_it() {
        for theme in THEMES {
            for c in BG_CATEGORIES {
                if matches!(c, Category::Sensed | Category::Effect) {
                    continue;
                }
                let s = swatch(theme, c);
                let d = dist2(rgb(s.bg), rgb(s.bg_dim));
                assert!(
                    d >= MIN_BG_DIST2,
                    "{theme:?}: {c:?}'s in-view fill {} and beyond-view fill {} blur \
                     (dist^2 {d} < {MIN_BG_DIST2})",
                    s.bg,
                    s.bg_dim,
                );
            }
        }
    }

    /// The threat ladder is separated by luminance at **background** strength too
    /// (§11.2/#419) — the same property the foregrounds are held to, asserted where the
    /// near line's band and the map's fills actually read it.
    ///
    /// Like its foreground twin this is a claim about spacing, not direction: on black
    /// the fills descend (a gold band, a darker orange, a darker red still) and on white
    /// they descend toward the page too. What is forbidden is a rung that doubles back —
    /// which is exactly what the old dark table did, its `bg_dim` red sitting *brighter*
    /// than its orange, so the ladder read as gold, dark, bright rather than as a ladder.
    #[test]
    fn the_threat_ladder_holds_at_background_strength() {
        const MIN_BG_STEP: i32 = 20;
        for theme in THEMES {
            for (tier, label) in [(0, "in view"), (1, "beyond it")] {
                let rungs = [Category::Caution, Category::Warning, Category::Danger]
                    .map(|c| lum(bg_shades(theme, c)[tier]));
                let steps = [rungs[1] - rungs[0], rungs[2] - rungs[1]];
                for (i, step) in steps.iter().enumerate() {
                    assert!(
                        step.abs() >= MIN_BG_STEP,
                        "{theme:?} ({label}): ladder fills {i} and {} sit at the same \
                         brightness ({rungs:?})",
                        i + 1,
                    );
                }
                assert!(
                    steps[0].signum() == steps[1].signum(),
                    "{theme:?} ({label}): the ladder's fills double back ({rungs:?})",
                );
            }
        }
    }

    /// §11.4: the near line is a solid category band with its **words in Neutral** over
    /// it, so every band the palette can produce has to leave that ink readable.
    ///
    /// This is the bound that keeps the #419 re-tone honest. "Lift the backgrounds until
    /// the ladder reads" has an obvious wrong answer — lift them until the words stop
    /// reading — and without this assertion nothing would have caught it.
    #[test]
    fn the_near_lines_words_read_over_every_band() {
        for theme in THEMES {
            let ink = swatch(theme, Category::Neutral).fg;
            for c in BG_CATEGORIES {
                for (band, label) in bg_shades(theme, c).into_iter().zip(["ambient", "message"]) {
                    let gap = (lum(ink) - lum(band)).abs();
                    assert!(
                        gap >= MIN_BAND_CONTRAST,
                        "{theme:?}: the near line's words {ink} are unreadable over a \
                         {label} {c:?} band {band} (gap {gap} < {MIN_BAND_CONTRAST})"
                    );
                }
            }
        }
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
        for theme in THEMES {
            let effect = bg_color(theme, Category::Effect, Fill::Quiet);
            assert_eq!(
                effect,
                bg_color(theme, Category::Effect, Fill::Full),
                "{theme:?}: the effect wash is full-strength in and out of the FOV alike",
            );

            let d = dist2(rgb(effect), rgb(page(theme)));
            assert!(
                d >= MIN_BG_DIST2,
                "{theme:?}: the effect wash vanishes into the page background (dist^2 {d})"
            );
            // Cyan: green and blue both clearly above red — the one hue nothing else on
            // the board uses, so it cannot be read as a threat level.
            let (r, g, b) = rgb(effect);
            assert!(
                g > r + 20 && b > r + 20,
                "{theme:?}: the effect wash must read as cyan, not as another threat colour"
            );

            for other in [
                bg_color(theme, Category::Danger, Fill::Full),
                bg_color(theme, Category::Danger, Fill::Quiet),
                bg_color(theme, Category::Sensed, Fill::Full),
            ] {
                let d = dist2(rgb(effect), rgb(other));
                assert!(
                    d >= MIN_BG_DIST2,
                    "{theme:?}: the effect wash blurs into {other} (dist^2 {d})"
                );
            }
            // Every glyph the board can draw **over** an effect mark (#338) must still
            // read against it: the threat ladder, because a held guard keeps its ladder
            // colour, and `Owned`, because the player and their decoy are the marks
            // still to come (#340/#341). Floor dots are exempt — `Ground` recedes by
            // design, and a wash it disappeared into would be the wash doing its job.
            for over in [
                Category::Caution,
                Category::Warning,
                Category::Danger,
                Category::Owned,
            ] {
                let d = dist2(rgb(swatch(theme, over).fg), rgb(effect));
                assert!(
                    d >= MIN_BG_DIST2,
                    "{theme:?}: {over:?} is unreadable over the effect wash (dist^2 {d}) — \
                     shift the effect colour, never the channel (§11.2)"
                );
            }
        }
    }

    /// §11.5 fix #1, at the colour table: both danger-overlay shades must read
    /// against the page background — the watched-but-unseen shade especially,
    /// since the old version let it sink into dark-on-dark and the most dangerous
    /// cells looked like the safest. The two shades also stay tellable apart, and
    /// both still read as **red** on a white page as they do on a black one.
    #[test]
    fn danger_overlay_shades_read_on_the_backdrop() {
        for theme in THEMES {
            let live = bg_color(theme, Category::Danger, Fill::Full);
            let dimmed = bg_color(theme, Category::Danger, Fill::Quiet);
            for shade in [live, dimmed] {
                let d = dist2(rgb(shade), rgb(page(theme)));
                assert!(
                    d >= MIN_BG_DIST2,
                    "{theme:?}: {shade} vanishes into the page background (dist^2 {d})"
                );
                let (r, g, b) = rgb(shade);
                assert!(
                    r > g + 30 && r > b + 30,
                    "{theme:?}: {shade} must read as *red*"
                );
            }
            let d = dist2(rgb(live), rgb(dimmed));
            assert!(
                d >= MIN_BG_DIST2,
                "{theme:?}: the two danger shades blur (dist^2 {d})"
            );
        }
    }

    /// §9.2: the **sensed** background is the eye-catching orange parallel of the red
    /// danger overlay — it must read on the page background, read as *orange* (not
    /// red), and stay clearly tellable from the danger fill so a sensed cell is never
    /// mistaken for a watched one. It is painted at full strength regardless of `vis`
    /// (the position is certain knowledge, §11.5a), so both visibilities agree.
    #[test]
    fn the_sensed_background_is_orange_and_distinct_from_danger() {
        for theme in THEMES {
            let sensed = bg_color(theme, Category::Sensed, Fill::Quiet);
            assert_eq!(
                sensed,
                bg_color(theme, Category::Sensed, Fill::Full),
                "{theme:?}: the sensed fill is full-strength in and out of the FOV alike",
            );

            let d = dist2(rgb(sensed), rgb(page(theme)));
            assert!(
                d >= MIN_BG_DIST2,
                "{theme:?}: the sensed fill vanishes into the page background (dist^2 {d})"
            );
            // Orange, not red: red and green both present, and green clearly above blue.
            let (r, g, b) = rgb(sensed);
            assert!(
                r > b + 30 && g > b + 20,
                "{theme:?}: the sensed fill must read as orange"
            );

            // Clearly apart from the danger red, both shades — a sensed cell and a
            // watched cell must never look alike.
            for danger in [
                bg_color(theme, Category::Danger, Fill::Full),
                bg_color(theme, Category::Danger, Fill::Quiet),
            ] {
                let d = dist2(rgb(sensed), rgb(danger));
                assert!(
                    d >= MIN_BG_DIST2,
                    "{theme:?}: the sensed orange blurs into the danger red {danger} \
                     (dist^2 {d})"
                );
            }
        }
    }

    /// The §11.2 [START] promise, pinned: each palette is **full-range** — true black
    /// and true white are both present (the old palette's gamma curve allowed
    /// neither) — and all sixteen foregrounds are pairwise tellable apart, the same
    /// bar the category subset must clear. Both themes carry both ends; what differs
    /// is which one is the page and which is the ink.
    #[test]
    fn the_palette_is_full_range_and_pairwise_distinct() {
        for theme in THEMES {
            let rows = &palette(theme).rows;
            assert!(
                rows.iter().any(|s| s.fg == "#000000"),
                "{theme:?}: no true black — the palette is compressed again"
            );
            assert!(
                rows.iter().any(|s| s.fg == "#ffffff"),
                "{theme:?}: no true white — the palette is compressed again"
            );
            for (i, a) in rows.iter().enumerate() {
                for b in &rows[i + 1..] {
                    let d = dist2(rgb(a.fg), rgb(b.fg));
                    assert!(
                        d >= MIN_DIST2,
                        "{theme:?}: palette rows {} and {} are too close \
                         (dist^2 {d} < {MIN_DIST2})",
                        a.fg,
                        b.fg
                    );
                }
            }
        }
    }

    /// §11.2: every palette row's shades **recede toward the page** — the background
    /// variant stands off it less than the foreground does, the out-of-FOV variant
    /// less again, and the dim glyph less than the foreground — so a category used as
    /// a background can never outshine the glyphs on it, and the fog always reads as
    /// less than the live thing.
    ///
    /// This used to be spelled "strictly darker", which was the dark theme's accident
    /// rather than the rule (#189): on a white page the same guarantee is *lighter*.
    /// Measured as distance from [`Palette::page`], it is one sentence for both.
    #[test]
    fn background_variants_recede_toward_the_page() {
        for theme in THEMES {
            for s in &palette(theme).rows {
                if from_page(theme, s.fg) == 0 {
                    continue; // the page row itself: fg and variants share the floor
                }
                assert!(
                    from_page(theme, s.bg) < from_page(theme, s.fg),
                    "{theme:?}: {}'s bg variant {} does not recede toward the page",
                    s.fg,
                    s.bg
                );
                assert!(
                    from_page(theme, s.bg_dim) < from_page(theme, s.bg),
                    "{theme:?}: {}'s out-of-FOV bg {} does not recede past its bg {}",
                    s.fg,
                    s.bg_dim,
                    s.bg
                );
                // The dim shade is the same glyph at *low* light (§11.5): always
                // nearer the page than the row's foreground, whichever dim it uses.
                assert!(
                    from_page(theme, s.dim) < from_page(theme, s.fg),
                    "{theme:?}: {}'s dim shade {} does not recede toward the page",
                    s.fg,
                    s.dim
                );
            }
        }
    }

    /// The floor-dot readability rule (§11.5): **Ground recedes**. It stands off the
    /// page less than every other category — the dots are there to carry the FOV
    /// edge, not to compete with walls and entities — and its own dim shade sits far
    /// enough below it that the edge still reads across open ground.
    ///
    /// On black "recedes" meant darker and on white it means lighter, which is why
    /// this is measured from the page rather than by luminance (#189).
    #[test]
    fn ground_recedes_beneath_every_other_category() {
        for theme in THEMES {
            let ground = swatch(theme, Category::Ground);
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
                    from_page(theme, ground.fg) < from_page(theme, swatch(theme, c).fg),
                    "{theme:?}: a floor dot outshines {c:?}"
                );
            }
            let d = dist2(rgb(ground.fg), rgb(ground.dim));
            assert!(
                d >= 2500,
                "{theme:?}: live and dimmed ground blur (dist^2 {d}) — the FOV edge \
                 would vanish"
            );
        }
    }

    /// §11.6/#360: the **mnemonic mark** on the ability bar is one letter of an entry
    /// lifted to [`Category::Neutral`], and it is the only announcement that letter's
    /// binding ever gets — so it has to read as *lifted out of* the word around it. It
    /// stands off the page in both themes, and it is clear of every colour a bar entry
    /// can otherwise be drawn in ([`bar_category`](intrusion_core::render) — Owned
    /// ready or active, System cooling), or the marked cell would look like just
    /// another letter of the name.
    ///
    /// Ground is deliberately **not** in that list: an entry that recedes keeps its
    /// letter dim rather than being marked at all, so the two never meet on one cell.
    #[test]
    fn the_mnemonic_mark_lifts_out_of_every_entry_colour() {
        for theme in THEMES {
            let mark = swatch(theme, Category::Neutral).fg;
            let d = dist2(rgb(mark), rgb(page(theme)));
            assert!(
                d >= MIN_DIST2,
                "{theme:?}: the mnemonic mark {mark} vanishes into the page (dist^2 {d})"
            );
            for entry in [Category::Owned, Category::System] {
                let other = swatch(theme, entry).fg;
                let d = dist2(rgb(mark), rgb(other));
                assert!(
                    d >= MIN_DIST2,
                    "{theme:?}: a marked letter {mark} reads the same as the {entry:?} \
                     name around it {other} (dist^2 {d})"
                );
            }
        }
    }

    /// §7.6/§11.5a: the exit anchors every escape plan and is always visible — so
    /// out of the FOV the `E` must not sink into wall gray the way it briefly did.
    /// Interest's dim shade still reads as purple, apart from both the standard
    /// dim and the memory slate (a dim exit is not a remembered content).
    #[test]
    fn the_dimmed_exit_still_reads_as_a_goal() {
        for theme in THEMES {
            let dim = swatch(theme, Category::Interest).dim;
            let (r, g, b) = rgb(dim);
            assert!(
                r > g + 30 && b > g + 30,
                "{theme:?}: {dim} must still read as purple"
            );
            let d = dist2(rgb(dim), rgb(std_dim(theme)));
            assert!(
                d >= MIN_DIST2,
                "{theme:?}: the dim exit blurs into dimmed walls ({d})"
            );
            let d = dist2(rgb(dim), rgb(memory(theme)));
            assert!(
                d >= MIN_DIST2 / 2,
                "{theme:?}: the dim exit impersonates memory ({d})"
            );
        }
    }

    /// The threat ladder is separated by **luminance as well as hue** (§11.2), so
    /// Caution → Warning → Danger survives a red-green deficiency — the one property
    /// that keeps three warm colours a *ladder* rather than three warm colours.
    ///
    /// It is a claim about spacing, not direction: on black the rungs climb away from
    /// the page (a bright yellow, a mid orange, a red) and on white they climb toward
    /// it, which is what a light theme forces — every rung has to be dark enough to
    /// read there. So each step is required to be a real step, in a consistent
    /// direction, without pinning which one.
    #[test]
    fn the_threat_ladder_is_separated_by_luminance_too() {
        const MIN_STEP: i32 = 25;
        for theme in THEMES {
            let rungs = [Category::Caution, Category::Warning, Category::Danger]
                .map(|c| lum(swatch(theme, c).fg));
            let steps = [rungs[1] - rungs[0], rungs[2] - rungs[1]];
            for (i, step) in steps.iter().enumerate() {
                assert!(
                    step.abs() >= MIN_STEP,
                    "{theme:?}: threat-ladder rungs {i} and {} sit at the same \
                     brightness ({rungs:?})",
                    i + 1,
                );
            }
            assert!(
                steps[0].signum() == steps[1].signum(),
                "{theme:?}: the threat ladder doubles back on itself ({rungs:?})",
            );
        }
    }

    /// The ticket's acceptance test, end to end across the seam: a **chasing guard**
    /// declares `Danger` (§7.4, core), and the one table maps `Danger` to a colour
    /// that unmistakably reads as **red** — the player sees the guard's mind with no
    /// game system ever naming a colour, in whichever theme is up.
    #[test]
    fn a_chasing_guard_maps_to_danger_red() {
        use intrusion_core::GuardState;
        let category = GuardState::Chasing.category();
        assert_eq!(category, Category::Danger);
        for theme in THEMES {
            let (r, g, b) = rgb(swatch(theme, category).fg);
            assert!(
                r > g + 60 && r > b + 60,
                "{theme:?}: Danger must read as red"
            );
            // Full-range, on either page: on black that means a bright red, on white
            // a deep one. Either way it sits at the far end of its own range rather
            // than washed into the middle, which is what the old gamma curve did to
            // everything — measured against the backdrop, since "bright" was the dark
            // theme's spelling of it and a bright red is invisible on white.
            const MIN_PAGE_DIST2: i32 = 60_000;
            let d = dist2(rgb(swatch(theme, category).fg), rgb(page(theme)));
            assert!(
                d >= MIN_PAGE_DIST2,
                "{theme:?}: Danger red is washed out against its page (dist^2 {d})"
            );
        }
    }

    /// The seam itself (#189): a theme moves **colours and nothing else**. Every
    /// category resolves to the same palette *row* in both themes, and flipping the
    /// theme changes every one of those rows' values — so the meaning of a cell is
    /// theme-independent by construction, and there is no category the toggle
    /// silently leaves behind on the other palette.
    #[test]
    fn a_theme_changes_every_colour_and_no_meaning() {
        for category in CATEGORIES.iter().copied().chain([Category::Sensed]) {
            let (dark, light) = (
                swatch(Theme::Dark, category),
                swatch(Theme::Light, category),
            );
            assert_ne!(dark.fg, light.fg, "{category:?} is the same in both themes");
            assert_ne!(dark.bg, light.bg, "{category:?}'s background never moved");
        }
        assert_ne!(page(Theme::Dark), page(Theme::Light));
        assert_ne!(memory(Theme::Dark), memory(Theme::Light));
        assert_eq!(Theme::Dark.toggled(), Theme::Light);
        assert_eq!(Theme::Light.toggled(), Theme::Dark);
        assert_eq!(Theme::default(), Theme::Dark, "the board opens dark");
    }
}
