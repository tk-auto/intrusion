//! The **pre-level scout** (§11.5a/§14 v3, #215): what a facility gives up before the run
//! walks into it.
//!
//! §11.5a settles that a facility's *contents* are hidden until seen and remembered
//! afterwards. This is the one thing that may buy its way past that rule, and it buys
//! exactly the first half: intel spent at the hub (#211) puts the building's **points of
//! interest** on the board from turn one, in the same **remembered** ink a console you
//! found last turn wears. Where things are, never what they are doing.
//!
//! # What it sells, and what it may never sell
//!
//! One purchase, one facility, and it hands over the three things a raid *goes to* — the
//! consoles it robs (§4.5), the crates it salvages (#209), the cupboards it hides in
//! (§10.3). A plan of the building's contents is the unit, because that is the unit a
//! decision is worth making about: *is knowing this building worth a raid's takings?* is a
//! question; *which third of it do I want* is a shopping list.
//!
//! **Live state is not on the list and cannot be** (§11.5a's third row): guards, bodies, a
//! door's pose and the danger overlay are earned inside the facility and are never
//! remembered even after they are seen, so there is nothing here for intel to buy. The
//! **comms console** (§7.3) is deliberately not revealed either — the counterplay it offers
//! has to be *found*, and selling it would price the §7.3 detour at one intel.
//!
//! # It is remembered because it is *remembered*
//!
//! There is no third knowledge state and no second fog rule. A scout marks those cells in
//! the player's tile memory at boot ([`State::with_scouted`](crate::State::with_scouted)),
//! and the renderer then draws them exactly as it draws a room you walked through and left
//! (§11.5a/`docs/render-reference.md` §3). That is what keeps the promise cheap to state
//! and impossible to drift: a scouted console is *found*, a raid early.

use crate::cell::Cell;
use crate::facility::{Facility, Terrain};

/// **What a scout reveals**: the contents a raid goes to, and the whole of what the sink
/// sells.
///
/// A list rather than a rule over [`Terrain`], because the membership is a design decision
/// per kind and not a property anything else can derive. The two absences are the
/// interesting half: the **comms console** stays hidden because §7.3's counterplay is meant
/// to be found, and a **duct mouth** is not here because it is already on the plans at the
/// §12.6 layout knob's easier end (#450) — a second, differently-priced way to buy the same
/// cell would be two rules about one mark.
const REVEALED: [Terrain; 3] = [Terrain::Console, Terrain::EquipmentCache, Terrain::Hideout];

/// **The cells a scout reveals** in `facility` — every console, crate and cupboard it
/// holds, and nothing else.
///
/// Derived from the finished grid rather than from the placement lists, so what is revealed
/// is whatever the board actually holds: a facility with no crates in it reveals no crates,
/// which is the honest answer and not an error — the flavour said what it hid (§14 v3) and
/// the player bought a plan of the building, not a promise about its contents.
pub fn scouted_cells(facility: &Facility) -> Vec<Cell> {
    REVEALED
        .iter()
        .flat_map(|&terrain| facility.find_all(terrain))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The set is the three things a raid goes to — and neither of the two deliberate
    /// absences, each of which would be a second rule about a cell that already has one.
    #[test]
    fn a_scout_reveals_the_three_things_a_raid_goes_to() {
        assert!(REVEALED.contains(&Terrain::Console));
        assert!(REVEALED.contains(&Terrain::EquipmentCache));
        assert!(REVEALED.contains(&Terrain::Hideout));
        assert!(
            !REVEALED.contains(&Terrain::CommsConsole),
            "§7.3's counterplay has to be found",
        );
        assert!(
            !REVEALED.contains(&Terrain::DuctEntry),
            "a duct mouth is the layout knob's to hand over (#450)",
        );
    }
}
