//! **What the run is asked for, in words** (§4.5/§11.4) — the one derivation every
//! surface that states the objective draws from.
//!
//! The gate is a level modifier ([`IntelGate`](crate::IntelGate)/#244), so what the exit
//! will ask for is a fact about *this* run and has to be said rather than assumed. Two
//! surfaces say it, and they must not come to say it differently:
//!
//! - the **level-start splash** ([`super::splash`], #497) — before the first turn, under
//!   `THE JOB`;
//! - the help panel's **Level info** tab ([`super::help`], #574) — the same two lines,
//!   for the player who wants the rule back mid-raid.
//!
//! # Why it is not read off the modifier list
//!
//! [`LevelModifiers::active`](crate::LevelModifiers::active) surfaces a knob only when it
//! sits **off** its baseline, which is exactly right for a list of departures and exactly
//! wrong for the objective: §4.5's baseline gate *is* [`IntelGate::AtLeastOne`], so a run
//! playing the baseline rule would be handed a card that said nothing about how it ends.
//! That was tolerable while `AtLeastOne` was the sim's own setting and no human ever read
//! a card under it; the minimum haul (#574) made it the campaign's gate, and a rule the
//! player is held to with no surface naming it is the §2.3 half-feature. So the objective
//! is derived from the run's own gate and contents instead, and **every** run gets a
//! positive statement of what it is being asked to do.

use crate::category::Category;
use crate::modifiers::IntelGate;
use crate::place::LevelConfig;

use super::help;

/// The objective section's heading — what the exit will ask for before it opens (§4.5).
pub(super) const OBJECTIVE_HEADING: &str = "OBJECTIVE";

/// The way out, on every card whatever the gate says: §1's promise stated before the
/// first step rather than discovered at a wall — you came in by your own tunnel and
/// there is no other way out of the building (§4.5/§7.6).
pub(super) const EXIT_LINE: &str = "Get back out through your tunnel";

/// What a facility with **nothing to take** says. Rare — the §12.6 intel knob floors at
/// [`LevelConfig::INTEL_MIN`], so a generated facility always holds consoles — but
/// reachable by a hand-built state, and a card that silently dropped its objective row
/// there would read as broken rather than as empty.
pub(super) const NO_INTEL: &str = "There is no intel here";

/// The **minimum haul** as the card states it (§4.5/#574), on a facility that hides
/// crates as well as consoles: the gate is met by one of either, and a line that named
/// only the intel would send a player past the crate in reach of them.
const HAUL_ANY: &str = "Take one thing — intel or a crate";

/// The minimum haul where there are no crates to take — quick play, the sim (§13.2), and
/// any campaign flavour that hides none (§8.3). Naming a crate that is not in the
/// building would be a promise the facility cannot keep.
const HAUL_INTEL: &str = "Take intel — one is enough";

/// What the run must take before the exit will open, in the §11.2 category the row reads
/// in: Interest — the goal-and-reward colour — where there is something to reach for, and
/// Ground where there is not, the same reading the modifier list's own *none active* row
/// takes.
///
/// `intel` and `caches` are the facility's **whole** counts rather than what is still
/// out: this states the objective, not progress against it.
pub(super) fn objective_line(gate: IntelGate, intel: usize, caches: usize) -> (String, Category) {
    let category = if takeable(gate, intel, caches) == 0 {
        Category::Ground
    } else {
        Category::Interest
    };
    (take_line(gate, intel, caches), category)
}

/// How much of this facility counts towards the gate at all — the denominator behind the
/// *nothing to take* case. [`All`](IntelGate::All) is the complete-the-set objective and
/// a crate is no part of the set, so only consoles count there; the other two gates are
/// met by an objective of either kind (#574).
fn takeable(gate: IntelGate, intel: usize, caches: usize) -> usize {
    match gate {
        IntelGate::All => intel,
        IntelGate::AtLeastOne | IntelGate::None => intel + caches,
    }
}

/// What the gate asks for, in words — one line per §12.6 setting, since the modes want
/// different things of the same facility (§4.5/#244):
///
/// - [`All`](IntelGate::All) — quick play and the archive (#217): every console, then out.
/// - [`AtLeastOne`](IntelGate::AtLeastOne) — the §4.5 baseline, the campaign (#574) and
///   the sim (§13.2): one thing is a complete run, and pressing on for more is the
///   aggressive style's trade rather than a requirement. It names the crate where the
///   facility has crates, because the rule counts them and the player has no other way to
///   learn that.
/// - [`None`](IntelGate::None) — the exit never refuses. The row says both halves,
///   because "no intel required" alone would read as *there is no point taking any*,
///   which is the opposite of true.
pub(super) fn take_line(gate: IntelGate, intel: usize, caches: usize) -> String {
    if takeable(gate, intel, caches) == 0 {
        return NO_INTEL.to_string();
    }
    match (gate, intel) {
        (IntelGate::All, 1) => "Take the one intel".to_string(),
        (IntelGate::All, n) => format!("Take all {n} intel"),
        (IntelGate::AtLeastOne, _) if caches > 0 => HAUL_ANY.to_string(),
        (IntelGate::AtLeastOne, _) => HAUL_INTEL.to_string(),
        (IntelGate::None, _) => "Take intel — none is required".to_string(),
    }
}

/// **The objective's fixed lines fit the board they are drawn on.** Measured at compile
/// time against the narrowest screen a real run renders on (the v1 board, 40 wide —
/// §10.2) with the one-column right margin every card keeps. The one variable line — a
/// console count — is measured over its whole envelope by
/// `no_objective_line_is_clipped_on_the_board`.
///
/// [`draw`](super::draw) clips in silence, so a line that outgrew the board would simply
/// arrive cut, which is worse than a short one: it looks like the whole sentence (§2.3).
const _: () = {
    // Measured in **bytes**, which is what is available to a const, and which can only
    // over-count: an em dash is three bytes and one column, so a line that fits by bytes
    // fits by columns. The two haul lines carry one each and still clear the bound.
    let room = (LevelConfig::V1.width - help::CONTENT_INDENT - 1) as usize;
    assert!(
        EXIT_LINE.len() <= room
            && NO_INTEL.len() <= room
            && HAUL_INTEL.len() <= room
            && HAUL_ANY.len() <= room,
        "an objective line is too long for the cards that draw it — shorten it",
    );
    let heading_room = (LevelConfig::V1.width - help::SECTION_INDENT - 1) as usize;
    assert!(
        OBJECTIVE_HEADING.len() <= heading_room,
        "the objective heading is too long for the board — shorten it",
    );
};

#[cfg(test)]
mod tests;
