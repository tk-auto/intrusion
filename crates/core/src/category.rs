//! Information categories — the colour seam (§11.2).
//!
//! **Colours are not chosen by game systems.** A system declares what a cell *means*
//! — an unaware threat, a goal, inert scenery — as a [`Category`], and presentation
//! owns the one table that maps a category to a concrete colour. **[SETTLED]** This
//! is the piece of the old design worth keeping: recolouring or reskinning for
//! accessibility is a one-table edit, and no game system ever names a colour.
//!
//! The category lives in the core because the *meaning* is the core's to state (a
//! guard glyph is re-categorised every turn from its state, §11.2 — yellow → orange
//! → red *is* the guard's mind, made visible). The core→colour mapping does **not**
//! live here: it belongs to whichever platform shell draws the grid (§12.2), because
//! a concrete colour is a platform concern. The renderer (§11.1) tags every grid
//! cell with a category; the shell maps category → pixels and nothing else.

use serde::{Deserialize, Serialize};

/// What a cell's foreground *means* (§11.2). Presentation maps each to a colour; the
/// core never names one.
///
/// The base palette is a 16-colour, colour-blind-safe qualitative set, each usable
/// as a foreground and as a darkened background variant — but that is the shell's
/// table (§11.2), and the full palette lands with the colour-category ticket. Here
/// we only fix the vocabulary the renderer speaks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Category {
    /// White. Inert scenery, spent objectives.
    Neutral,
    /// Dark gray. Traversable ground — the floor dots (§11.5). Its meaning is
    /// *absence*: presentation draws it to recede, so walls, entities and items
    /// pop against it. (§11.3 originally left floor uncategorised because it drew
    /// blank; the §11.5 floor dots gave it a glyph, and a glyph needs a meaning.)
    Ground,
    /// Blue. You, and the things you made (a decoy, the cupboard you are hidden in).
    Owned,
    /// Yellow. A threat that is unaware.
    Caution,
    /// Orange. A threat that is hunting.
    Warning,
    /// Red. A threat that has you.
    Danger,
    /// Purple. Goals and rewards.
    Interest,
    /// Tan. Doors, hideouts — neutral furniture.
    System,
    /// Orange. A guard **sensed through a wall** (§9.2) — the player knows its exact
    /// cell but not its facing or cone. Its meaning is *position without attention*:
    /// an eye-catching **background** highlight on the guard's cell, never a threat
    /// readout. Presentation paints it as a filled cell (like the §11.5 danger overlay,
    /// but orange, not red); it blooms into the full state-coloured `g`-with-cone the
    /// moment the player can actually see the guard, so the orange-cell → seen-guard
    /// transition *is* the sensed/seen distinction made visible (§11.3). It never
    /// carries a danger overlay — knowing where a guard is is not knowing whether it
    /// can see you (§9.2).
    ///
    /// The **door-change cue** (§9.4/§10.4) reuses this same category: a door that
    /// opens or shuts away from the player is *sensed through a wall* exactly as a
    /// guard is, so it reads in the same orange background — one "sense" channel, not
    /// two colours to tell apart. It paints the **whole door**, and (unlike a guard's
    /// standing position) fades over a few turns, since a door change is a discrete
    /// event; it is still position only, never who passed or which way (§10.4). A
    /// coincident danger cone still outranks it (§11.5: being seen outranks).
    Sensed,
    /// Cyan. An **ability effect of your own making** (§8.3/§11.5) — Confusion's blast
    /// and Pierce Wall's hole today, Lockdown's radius next. Its meaning is *what your
    /// gadget did*: where it acted, and what it still holds.
    ///
    /// **Background only** (#338). The glyph on the cell keeps its own meaning — a
    /// guard stays on the §11.2 threat ladder, a thing of yours stays
    /// [`Owned`](Self::Owned) — and the effect is the wash underneath it. One channel
    /// for every effect the game grows, so a new one arrives without inventing a
    /// vocabulary; what varies is where the mark lands (a fixed cell set, or the thing
    /// in a cell) and how long it lives (a moment, or as long as the effect holds).
    ///
    /// Two strengths, because the two placements make different claims:
    ///
    /// - A mark over **cells** is the **weakest** background there is: a
    ///   [`Sensed`](Self::Sensed) cue and a [`Danger`](Self::Danger) cone both paint
    ///   over it, so an advisory layer can never hide the detection set §11.5 settles
    ///   as the board's one non-negotiable claim. It is painted through walls and fog,
    ///   because the reach of your own gadget is your own knowledge.
    /// - A mark over the **thing** in a cell is a *recolour of a cue that thing already
    ///   draws*, not a competing claim: on a guard felt through a wall it replaces the
    ///   [`Sensed`](Self::Sensed) orange with the stronger statement about the very same
    ///   guard — position included, and it cannot move. So it outranks `Sensed`, and
    ///   still yields to `Danger`. It never paints a thing the player cannot perceive,
    ///   so the fog gives nothing away.
    ///
    /// Deliberately **not** blue: [`Owned`](Self::Owned) is a *thing* of yours on the
    /// board (you, a decoy, the cupboard you are hidden in), and a frozen guard is
    /// emphatically not one of your things — it is a threat you have bought a few turns
    /// from.
    Effect,
}

/// Which of the shell's colour tables the screen is painted from (§11.2/#189) — the
/// *only* thing the core knows about a theme.
///
/// A theme is a second [`Category`]→colour table, not a second set of meanings: the
/// core still declares what a cell means and never names a colour, so all a theme
/// changes is which column of the one table presentation reads. That is exactly the
/// payoff §11.2 **[SETTLED]** was written to buy — "recolouring or reskinning for
/// accessibility is a one-table edit" — and it is why this enum carries no colours
/// itself. It rides on [`ScreenUi`](crate::ScreenUi) with the other view state
/// (§11.4): flipping it changes no world and costs no turn (§4.4/§12.1).
///
/// [`Dark`](Self::Dark) is the default because the palette was tuned for a
/// true-black backdrop and every screenshot, artifact and golden test assumes it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Theme {
    /// The original palette: full-strength hues on a true-black page, everything
    /// receding *toward black* out of the field of view.
    #[default]
    Dark,
    /// The light theme: the same categories re-toned for a white page, everything
    /// receding *toward the page* instead. The hues are re-chosen rather than
    /// inverted — a yellow bright enough to carry Caution on black is illegible on
    /// white — but every §11.5 guarantee the dark table holds, this one holds too.
    Light,
}

impl Theme {
    /// The other theme — what the toggle
    /// ([`UiCommand::ToggleTheme`](crate::UiCommand::ToggleTheme)) flips to. There
    /// are two, so this is the whole cycle; a third would make it a list.
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }
}
