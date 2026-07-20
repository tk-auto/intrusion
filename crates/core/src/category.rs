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
    /// Cyan. A noise the player *hears* but cannot see (§9.3) — the source cell of
    /// a sound that reached them, flashed for the one turn it was made. Its meaning
    /// is *perception*, not a thing on the map: nothing physical is cyan, a heard
    /// sound is. Presentation owns *how* that reads (a flash, later maybe an edge
    /// arrow or compass, §15.3); the category only names what the cell means.
    Noise,
}
