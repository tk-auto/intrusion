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

/// What a cell's foreground *means* (§11.2). Presentation maps each to a colour; the
/// core never names one.
///
/// The base palette is a 16-colour, colour-blind-safe qualitative set, each usable
/// as a foreground and as a darkened background variant — but that is the shell's
/// table (§11.2), and the full palette lands with the colour-category ticket. Here
/// we only fix the vocabulary the renderer speaks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
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
    Sensed,
    /// A **door that just changed state** away from the player (§9.2/§10.4) — opened
    /// or shut by a guard, or timed shut by an automatic door — sensed at its own
    /// longer range ([`DOOR_SENSE_RANGE`](crate::DOOR_SENSE_RANGE)). Its meaning is
    /// *evidence someone passed*: a background highlight on the door cell that fades
    /// over a few turns, readable around a corner and out of FOV like the sensed dot,
    /// but position only — never who passed or which way (§10.4). Presentation paints
    /// it as a filled cell (like [`Sensed`](Category::Sensed) and the §11.5 danger
    /// overlay), in a hue distinct from both the sensed orange and the danger red so
    /// the three backgrounds never blur; it never carries a glyph of its own, and a
    /// coincident sensed dot or danger cone outranks it (§11.5: being seen outranks).
    Trace,
}
