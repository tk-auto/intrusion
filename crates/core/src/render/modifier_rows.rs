//! The run's **active level modifiers, as rows** (§12.6/§11.2) — the one derivation
//! every surface that lists them draws from.
//!
//! §12.6 asks that "all active modifiers are fields on a single [`LevelModifiers`]
//! value", and §11.2/§11.3 that every row on a card be **derived from the real
//! source** rather than hand-copied. [`LevelModifiers::active`] is that source; this
//! module is the one place its entries become *text and a colour*, so the surfaces
//! that show them cannot come to disagree about what a run is playing under:
//!
//! - the help panel's **Level info** tab ([`super::help`], #248) — the card you call up
//!   mid-raid, with the run's seed token and the facility alert beside it;
//! - the **level-start splash** ([`super::splash`], #497) — the same list, before the
//!   first turn, with nothing else on it but the objective.
//!
//! The §11.2 **direction cue** lives here too, and the campaign map (§14 v3/#208) reads
//! it for its own alert line: a rule bent against you is Warning, a rule bent your way
//! is Owned, on every screen that has anything to say about one. It was written out
//! twice before this module existed, which is exactly the drift the derivation rule is
//! about.

use super::help::CONTENT_INDENT;
use crate::category::Category;
use crate::modifiers::{
    ActiveModifier, Composite, LevelModifiers, ModifierDirection, CAPTIONS, CAPTION_SEPARATOR,
};
use crate::place::LevelConfig;

/// What a **baseline** run reads as (#248): legible as *none active*, never blank or
/// absent — a card that simply had no modifier section would read as the readout being
/// broken rather than as the run being unmodified.
pub(super) const NONE_ACTIVE: &str = "none active — baseline rules";

/// **The caption width bound** (#248). Every surface draws these rows from
/// [`CONTENT_INDENT`] on the narrowest screen a real run renders on — the v1 board
/// ([`LevelConfig::V1`], 40 wide, §10.2) — leaving one column of right margin, the same
/// margin every other column of a card keeps. Anything longer is silently clipped by
/// [`draw`](super::draw), which is how "Sightings called in: one guard converges"
/// reached a screenshot as `…one guard conver`.
///
/// So it is checked **at compile time** against [`CAPTIONS`] — the whole set of captions
/// a run can draw — and a caption that would not fit fails the build instead of the eye.
/// Derived from the board width rather than written as a number, so retuning §10.2 moves
/// the bound with it.
const CAPTION_MAX: usize = (LevelConfig::V1.width - CONTENT_INDENT - 1) as usize;

// The bound bites here, over the complete caption set (§2.3 — a check that cannot be
// bypassed by adding a modifier, because `active` may only draw from `CAPTIONS`).
const _: () = {
    let mut i = 0;
    while i < CAPTIONS.len() {
        assert!(
            CAPTIONS[i].caption_len() <= CAPTION_MAX,
            "a level-modifier caption is too long for the cards that list it — \
             shorten its name or detail (see CAPTION_MAX in render::modifier_rows)",
        );
        i += 1;
    }
    // …and over the **attributed** rows a composite draws (§12.6/#565), which are the
    // longest a list can produce: a composite's name, the separator, and the rule's own
    // short phrasing. Bounded conservatively by the longest label against the longest
    // phrase — a pair no composite actually draws, so a row that fits this fits every
    // real one. It is what makes *"the list grows downward"* a guarantee rather than a
    // hope: one row per active rule, and no row wide enough to wrap.
    assert!(
        Composite::max_label_len() + CAPTION_SEPARATOR.len() + ActiveModifier::max_short_len()
            <= CAPTION_MAX,
        "a composite's attributed row is too long for the cards that list it — \
         shorten a composite label or a caption's short phrasing (see CAPTION_MAX)",
    );
};

/// The §11.2 category a direction reads in — the colour cue the caption is drawn in:
/// Warning (a hunting threat's orange) for *harder*, Owned (yours, calm blue) for
/// *easier*. Pulled from the standing categories, never new ad-hoc styling
/// (§11.2/#248), so the colour a player learns on one screen reads the same on the next.
pub(super) fn direction_category(direction: ModifierDirection) -> Category {
    match direction {
        ModifierDirection::Harder => Category::Warning,
        ModifierDirection::Easier => Category::Owned,
    }
}

/// One modifier's caption, as any card prints it: the name alone, or `name: value` for
/// a bounded knob — and, for a rule a composite set, the composite's name in front of
/// the rule's own short phrasing ([`ActiveModifier::attributed_to`] has already
/// rewritten the entry, so this stays one shape of row).
pub(super) fn caption(m: &ActiveModifier) -> String {
    match m.detail {
        Some(detail) => format!("{}{CAPTION_SEPARATOR}{detail}", m.name),
        None => m.name.to_string(),
    }
}

/// **The modifier list**, as `(caption, category)` rows in reading order — what every
/// surface draws, and the whole of what any of them may say about the run's rules.
///
/// Derived from [`LevelModifiers::active`], so a newly added modifier appears on every
/// card on its own (§11.2/§11.3) and no card can omit one that is in force. A run with
/// nothing active is **one row** ([`NONE_ACTIVE`]) rather than none, so the section is
/// never a blank a player has to interpret.
pub(super) fn modifier_rows(modifiers: LevelModifiers) -> Vec<(String, Category)> {
    let active = modifiers.active();
    if active.is_empty() {
        return vec![(NONE_ACTIVE.to_string(), Category::Ground)];
    }
    active
        .iter()
        .map(|m| {
            let text = caption(m);
            debug_assert!(
                text.chars().count() == m.caption_len(),
                "the drawn caption and the measured one must agree",
            );
            (text, direction_category(m.direction))
        })
        .collect()
}
