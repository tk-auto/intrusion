//! How the facility alert ladder is *shown* (§7.3/§11.2, #375) — the one place the
//! rung becomes a colour and a block of rows.
//!
//! The ladder (§7.3) landed in #311 with teeth but no surface: the near line states an
//! escalation on the turn it happens and is then overwritten by anything louder
//! (§11.7), so a player who blinked never learned the facility had changed its mind
//! about them. An escalation the player cannot perceive is inert in a second way
//! (§2.2), so the ladder needs two things — a place to go and *read* the standing
//! state, and a glance-level tell that says when to go and read it. This module owns
//! both halves' presentation:
//!
//! - [`rung_category`] — the tell. The `[?]` help toggle on the near line is tinted by
//!   rung, so the control the player already knows changes colour when there is
//!   something new behind it (see [`hud`](super::hud)).
//! - [`draw_alert`] — the surface. The **ALERT** section of the help panel's Level info
//!   tab, beside the seed and the modifiers: what is bending the rules right now
//!   (see [`help`](super::help)).
//!
//! They live together because they are one claim made twice. A tint that said *danger*
//! over a panel that said *rung 1* would be worse than no tint at all, so the mapping
//! is written once and both halves read it.
//!
//! **Everything drawn here is derived** (§11.3): the rows come from
//! [`AlertReadout`](crate::AlertReadout), which the ladder generates from its own table
//! and live tuning, so a retuned threshold or a rung that grows teeth (#374) reaches the
//! panel without anything here changing.

use super::{draw, Grid};
use crate::alert::{AlertReadout, NO_ALERT, TOP_RUNG};
use crate::category::Category;
use crate::modifiers::CAPTION_SEPARATOR;

/// The column the section headings sit at — the panel's standing heading indent, one
/// out from the content column [`draw_alert`] is handed.
const HEADING_INDENT: u32 = 2;

/// How the panel names the rung the facility has reached: **`Condition 2 of 3`**.
///
/// The player-facing noun is **condition**, not *rung*. A rung is the shape of the
/// thing in the code — a monotone ladder with a top — and it is the right word for the
/// types ([`AlertReadout::rung`](crate::AlertReadout)) and for §7.3's prose; it is the
/// wrong word on screen, because it names the mechanism rather than the world. A
/// facility's security posture *is* a condition, and a control room says "we are at
/// condition two", so the panel says what the building would say about itself.
///
/// It keeps the **number and the ceiling**, which is the one thing the ladder metaphor
/// was buying the player: `2 of 3` says both how bad it is and how much worse it can
/// still get. A bare name ("sweep", "sealed") would lose the scale — and the two most
/// natural names for the top are already spoken for, by the §7.4 guard states and by the
/// Lockdown ability (§8.3).
///
/// Shared with the near line ([`alert_line`](crate::status::alert_line)), which wears it
/// as *"security condition 2 of 3"*, so the line that sends the player to this panel
/// names the same thing they find on it. Read by the drawing **and** by the width test,
/// so the row the test measures is the row that is drawn.
pub(super) fn condition_line(rung: u32) -> String {
    format!("Condition {rung} of {TOP_RUNG}")
}

/// The §11.2 category a rung reads in — the ladder mapped onto the **standing threat
/// ladder**, and no new colour vocabulary (§11.2 **[SETTLED]**: a system declares a
/// meaning, never a colour, and never invents a category).
///
/// | Rung | Category | Why |
/// |---|---|---|
/// | 0 | System | Not a threat statement at all. The `[?]` is furniture — the tan every HUD control already wears — so an unnoticed raid changes nothing about the screen |
/// | 1 | Caution | *A threat that is unaware.* The facility knows somebody is in it and does not know where |
/// | 2 | Warning | *A threat that is hunting.* Three sightings, or it knows what you came for |
/// | 3 | Danger | *A threat that has you.* The top of the ladder, and the same red a guard with eyes on you wears |
///
/// The yellow → orange → red run is the one the player already reads off a guard's
/// glyph (§11.2), which is exactly why it is reused: the facility's mind escalates in
/// the same colours one guard's does, so there is nothing new to learn. It is also the
/// ladder the palette's tests separate by luminance as well as hue, so the tint
/// survives a red-green deficiency (see `docs/render-reference.md` §4.5).
///
/// Total over the rungs rather than a table lookup: a rung 4 would have to say what it
/// means here before it could be drawn.
pub(super) fn rung_category(rung: u32) -> Category {
    match rung {
        0 => Category::System,
        1 => Category::Caution,
        2 => Category::Warning,
        _ => Category::Danger,
    }
}

/// Draw the Level info tab's **ALERT** section at row `y`, returning the row after it.
///
/// Three kinds of row, in the panel's standing shape (heading in System, content one
/// column further in):
///
/// ```text
/// ALERT
///   Condition 2 of 3
///   Guards never calm: pause 1–3 turns
/// ```
///
/// **The section is always drawn**, rung 0 included, where it prints [`NO_ALERT`]
/// instead of a condition line. A section that appeared only once the facility had
/// noticed you would teach the ladder exists at the exact moment that knowledge stopped
/// being useful — and a row that vanishes reads as a bug rather than as a fact.
///
/// The condition line is tinted by [`rung_category`], the same colour the `[?]` toggle
/// wears on the board, so the tell and the thing it points at agree. Effect rows are
/// drawn in Warning, the standing *this is a rule bent against you* cue the harder
/// modifiers already use — every one of them is retaliation, so none needs its own shade.
pub(super) fn draw_alert(grid: &mut Grid, mut y: u32, readout: &AlertReadout, indent: u32) -> u32 {
    draw(grid, HEADING_INDENT, y, "ALERT", Category::System);
    y += 1;

    if readout.rung == 0 {
        draw(grid, indent, y, NO_ALERT, Category::Ground);
        return y + 1;
    }

    draw(
        grid,
        indent,
        y,
        &condition_line(readout.rung),
        rung_category(readout.rung),
    );
    y += 1;

    for effect in &readout.effects {
        // `name: detail`, the modifier rows' caption shape (§12.6/#248) — one line the
        // player learns to read rather than two ways of saying the same kind of thing.
        let text = match &effect.detail {
            Some(detail) => format!("{}{CAPTION_SEPARATOR}{detail}", effect.name),
            None => effect.name.to_string(),
        };
        draw(grid, indent, y, &text, Category::Warning);
        y += 1;
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alert::AlertEffect;
    use crate::render::blank_grid;

    /// The grid's row `y` as text, trimmed — what the player reads off that line.
    fn row(grid: &Grid, y: u32) -> String {
        (0..grid.width())
            .map(|x| grid.get(x, y).glyph)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    /// #375/§11.2: the tint is the **standing threat ladder** and nothing new. Rung 0
    /// leaves the control the furniture colour it has always been; 1–3 climb
    /// yellow → orange → red, the same run a guard's glyph walks, so the player has no
    /// second vocabulary to learn.
    #[test]
    fn the_rung_tint_is_the_standing_threat_ladder() {
        assert_eq!(rung_category(0), Category::System, "no alert, no claim");
        assert_eq!(rung_category(1), Category::Caution);
        assert_eq!(rung_category(2), Category::Warning);
        assert_eq!(rung_category(TOP_RUNG), Category::Danger);

        // Every rung the ladder can reach has a category, and no two adjacent rungs
        // share one — a tint that did not change on a step would tell the player
        // nothing, which is the whole point of the tell (§2.2).
        for rung in 1..=TOP_RUNG {
            assert_ne!(
                rung_category(rung),
                rung_category(rung - 1),
                "rung {rung} looks the same as the one below it",
            );
        }
    }

    /// §11.8: **the screen never says "rung".** A rung is the shape of this system —
    /// a monotone ladder with a top — which is the right word in §7.3 and in these
    /// identifiers, and the wrong one on a screen the player reads without the design
    /// doc beside them. The facility's own word for that state is a *condition*.
    ///
    /// Walked over every string the two alert surfaces can print, so a later reword that
    /// let the mechanism's name back out fails here rather than shipping. The near line's
    /// half of the same pair is pinned in [`status`](crate::status).
    #[test]
    fn no_alert_row_speaks_the_design_vocabulary() {
        let mut rows = vec![NO_ALERT.to_string()];
        rows.extend((0..=TOP_RUNG).map(condition_line));
        for row in rows {
            let lower = row.to_lowercase();
            for word in ["rung", "ladder", "trigger", "tuning"] {
                assert!(
                    !lower.contains(word),
                    "{row:?} says {word:?} — that is the design's word, not the \
                     player's (§11.8)",
                );
            }
        }
        assert!(
            condition_line(2).starts_with("Condition 2 of "),
            "the player's word, with the scale the rung carried",
        );
    }

    /// #375: **rung 0 still draws a section.** The player should meet the ladder before
    /// it bites, and a heading that appears out of nowhere on the turn you are first
    /// seen is a worse teacher than one that was always there saying "quiet".
    #[test]
    fn a_quiet_facility_still_says_so() {
        let mut grid = blank_grid(40, 10);
        let readout = AlertReadout {
            rung: 0,
            effects: Vec::new(),
        };
        let next = draw_alert(&mut grid, 1, &readout, 3);
        assert_eq!(row(&grid, 1), "  ALERT");
        assert_eq!(row(&grid, 2), format!("   {NO_ALERT}"));
        assert_eq!(next, 3, "the section is two rows and reports its own end");
        assert_eq!(row(&grid, 3), "", "and nothing below it");
    }

    /// #375: the rung and its effects are drawn from the readout — a rung line tinted
    /// by [`rung_category`], then one row per effect in the modifier rows' `name:
    /// detail` shape.
    #[test]
    fn the_rung_and_its_effects_are_drawn_from_the_readout() {
        let mut grid = blank_grid(40, 10);
        let readout = AlertReadout {
            rung: 2,
            effects: vec![AlertEffect {
                rung: 1,
                name: "Guards never calm",
                detail: Some("pause 1–3 turns".to_string()),
            }],
        };
        let next = draw_alert(&mut grid, 0, &readout, 3);
        assert_eq!(row(&grid, 0), "  ALERT");
        assert_eq!(row(&grid, 1), "   Condition 2 of 3");
        assert_eq!(row(&grid, 2), "   Guards never calm: pause 1–3 turns");
        assert_eq!(next, 3);

        // The rung line wears the rung's own colour, the effect rows the standing
        // "bent against you" Warning the harder modifiers use.
        assert_eq!(
            grid.get(3, 1).fg,
            Category::Warning,
            "condition 2 is Warning"
        );
        assert_eq!(grid.get(3, 2).fg, Category::Warning);

        // An effect with no numbers behind it draws its name alone, no dangling `": "`.
        let mut grid = blank_grid(40, 10);
        draw_alert(
            &mut grid,
            0,
            &AlertReadout {
                rung: 3,
                effects: vec![AlertEffect {
                    rung: 3,
                    name: "Everyone is looking",
                    detail: None,
                }],
            },
            3,
        );
        assert_eq!(row(&grid, 2), "   Everyone is looking");
        assert_eq!(grid.get(3, 1).fg, Category::Danger, "condition 3 is Danger");
    }
}
