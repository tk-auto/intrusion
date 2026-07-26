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
    ///
    /// The **door-change cue** (§9.4/§10.4) reuses this same category: a door that
    /// opens or shuts away from the player is *sensed through a wall* exactly as a
    /// guard is, so it reads in the same orange background — one "sense" channel, not
    /// two colours to tell apart. It paints the **whole door**, and (unlike a guard's
    /// standing position) fades over a few turns, since a door change is a discrete
    /// event; it is still position only, never who passed or which way (§10.4). A
    /// coincident danger cone still outranks it (§11.5: being seen outranks).
    Sensed,
    /// Cyan. An **area effect of your own making** (§8.3/§11.5) — Confusion's bubble
    /// today, Lockdown's radius next. Its meaning is *reach*: how far the gadget you
    /// just fired carries, and what it currently holds.
    ///
    /// It speaks in both channels, because an area effect has two things to say and
    /// they land on different cells:
    ///
    /// - As a **background**, the effect's live footprint — the §6.1 box around the
    ///   player, painted through walls like the reach it describes, and the **weakest**
    ///   background there is: a [`Sensed`](Self::Sensed) cue and a
    ///   [`Danger`](Self::Danger) cone both paint over it, so an advisory layer can
    ///   never hide the detection set §11.5 settles as the board's one non-negotiable
    ///   claim.
    /// - As a **foreground**, on an actor the effect currently holds: a frozen guard's
    ///   `g` recolours out of its state ladder (§11.2's yellow → orange → red *is* the
    ///   guard's mind) into this — a mind switched off is not a threat level, so it
    ///   leaves the ladder rather than pretending to sit on a rung. On a guard felt
    ///   only through a wall there is no glyph to recolour, so the mark takes over its
    ///   [`Sensed`](Self::Sensed) background instead: the stronger claim about the very
    ///   same guard, position included, never a cell the fog was hiding.
    ///
    /// Deliberately **not** blue: [`Owned`](Self::Owned) is a *thing* of yours on the
    /// board (you, a decoy, the cupboard you are hidden in), and a frozen guard is
    /// emphatically not one of your things — it is a threat you have bought a few turns
    /// from.
    Effect,
}
