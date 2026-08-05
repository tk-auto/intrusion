//! **The exchange** (§8.3/§8.4/#266): what a full run is offered at a crate, and how
//! the choice resolves.
//!
//! A run carries [`AbilityId::MAX_TECH_HELD`] pieces of salvaged tech and no more
//! (§8.3). Until now the cap was a dead end — a bump on a crate with full hands was
//! refused for free and the tech stayed in the box — which made the third pickup of a
//! campaign the last decision the tech axis ever offered. This module is the other
//! half: the crate **offers**, and the run decides what to give up for it.
//!
//! # It is a set of four, and you press the one you drop
//!
//! An open exchange has exactly four candidates: the three pieces of tech the run
//! holds, and the one in the crate. Pressing any of them **discards** it — three of
//! those choices are a trade, and the fourth (the crate's own) is the decline, which is
//! why there is no separate cancel verb to keep in step with anything. `Escape` is
//! spelled as that same discard, so the two ways of saying *no* cannot drift apart
//! ([`Choice`]).
//!
//! Innate abilities are **not** candidates (§8.3): Run is never found, never drawn and
//! never traded, so it is not on the table — the exchange only ever moves the tech.
//!
//! # What it deliberately is not
//!
//! Not a screen. §8.4 says build the targeting up front and reuse it, and the ability
//! bar is already a four-slot selection surface with a digit, a mnemonic letter and a
//! tap hit-test all resolving through one seam (`ability_in_slot`). So the exchange is
//! **drawn on the bar**, in the same four slots, and the keys that fire an ability fire
//! the choice instead ([`State::ability_input`](crate::State::ability_input)) — a fifth
//! modal screen would have been a second selection spine for one decision.
//!
//! Not a turn, either, until it is decided: opening the offer changes nothing, so it is
//! free like any refused bump (§4.4), and it is the **trade** that spends the turn — the
//! same one turn a plain salvage costs. A decline costs what the old refusal cost, which
//! is nothing.
//!
//! # Nothing else happens while it is open
//!
//! The world does not step underneath an unanswered offer: [`State::step`](crate::State)
//! answers only the discard while one is live. That rule lives in the **core** rather
//! than in the shell that draws the bar, so a replay, the sim and the browser all obey
//! it — a run cannot walk away from a crate mid-decision in one of them and not in the
//! others (§12.4).

use crate::ability::{AbilityId, Loadout};
use crate::cell::Cell;

/// A live offer from an equipment cache the run has no room for (§8.3/#266).
///
/// Held on the [`State`](crate::State) because it is world state and not a view: what
/// the player has merely chosen to *look at* costs no turn (§12.1), and this is a
/// decision the facility is waiting on. It carries the crate's cell so the trade can
/// mark that crate opened — the same crate, however the player got here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Exchange {
    /// The tech in the crate: the fourth thing, and the only candidate not already held.
    offered: AbilityId,
    /// Where the crate stands. The bump that opened this offer was free and changed
    /// nothing, so the crate is still unopened and still holds [`offered`](Self::offered).
    at: Cell,
}

/// How a discard resolves an open [`Exchange`] (§8.3/#266) — one of exactly three
/// answers, so no caller has to work out what a press meant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    /// Trade: drop the held tech named here and take the crate's. Spends the turn.
    Trade { dropped: AbilityId },
    /// Decline: leave the crate standing with its tech in it. Free — the loadout is
    /// untouched, so nothing changed and nothing is charged (§4.4).
    Decline,
}

impl Exchange {
    /// Open an offer of `offered`, held by the crate at `at`.
    pub(crate) fn new(offered: AbilityId, at: Cell) -> Self {
        Self { offered, at }
    }

    /// The tech the crate is offering — the entry the bar marks as the new one.
    pub fn offered(self) -> AbilityId {
        self.offered
    }

    /// Where the crate stands.
    pub(crate) fn at(self) -> Cell {
        self.at
    }

    /// **The four candidates**, in the order the bar draws them and the digits fire
    /// them: the run's salvaged tech in the fixed [`AbilityId::ALL`] order, then the
    /// crate's own last.
    ///
    /// The offer goes at the **end** rather than in catalogue order among the rest, so
    /// its slot is the same slot on every exchange a run ever opens — the row's first
    /// three entries are the bar you have been reading all run, and the new thing is
    /// always the one on the right.
    ///
    /// Innate abilities are filtered out: they are not tradeable and never were, so a
    /// row that listed Run would be offering a press that has to refuse.
    pub fn candidates(self, held: Loadout) -> Vec<AbilityId> {
        held.iter()
            .filter(|id| !id.is_innate())
            .chain(std::iter::once(self.offered))
            .collect()
    }

    /// What discarding `id` means here, or `None` for an ability this offer has nothing
    /// to do with — one the run does not hold, or an innate one. `None` is a mis-input,
    /// and the caller resolves it the way §4.4 resolves every other: for free, changing
    /// nothing.
    pub fn resolve(self, held: Loadout, id: AbilityId) -> Option<Choice> {
        if id == self.offered {
            return Some(Choice::Decline);
        }
        (held.contains(id) && !id.is_innate()).then_some(Choice::Trade { dropped: id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full run's loadout: the innate set plus three pieces of tech (§8.3).
    fn full_hands() -> Loadout {
        Loadout::innate()
            .with(AbilityId::Camouflage)
            .with(AbilityId::Decoy)
            .with(AbilityId::Vision)
    }

    fn offer() -> Exchange {
        Exchange::new(AbilityId::Lockdown, Cell::new(4, 7))
    }

    /// **Four candidates, and the crate's is last** (§8.3/#266): the three held pieces
    /// of tech in catalogue order, then the offer — so the new thing is always in the
    /// same slot, whatever the run is carrying.
    #[test]
    fn the_row_is_the_held_tech_then_the_offer() {
        assert_eq!(
            offer().candidates(full_hands()),
            vec![
                AbilityId::Camouflage,
                AbilityId::Decoy,
                AbilityId::Vision,
                AbilityId::Lockdown,
            ],
        );
    }

    /// **Innate abilities are not on the table** (§8.3): Run is never found and never
    /// traded, so it is neither drawn as a candidate nor accepted as one — a row that
    /// offered it would be offering a press that has to refuse.
    #[test]
    fn run_is_never_a_candidate() {
        assert!(full_hands().contains(AbilityId::Run));
        assert!(!offer().candidates(full_hands()).contains(&AbilityId::Run));
        assert_eq!(offer().resolve(full_hands(), AbilityId::Run), None);
    }

    /// Every candidate resolves, and each says which of the two things it is: the three
    /// held ones trade, the crate's own declines. There is no fourth answer, which is
    /// what keeps *cancel* from being a second verb that could drift.
    #[test]
    fn each_candidate_resolves_to_a_trade_or_the_decline() {
        let held = full_hands();
        for id in [AbilityId::Camouflage, AbilityId::Decoy, AbilityId::Vision] {
            assert_eq!(
                offer().resolve(held, id),
                Some(Choice::Trade { dropped: id }),
                "{id:?} is held, so discarding it is the trade",
            );
        }
        assert_eq!(
            offer().resolve(held, AbilityId::Lockdown),
            Some(Choice::Decline),
            "discarding the crate's own tech is how declining is spelled",
        );
    }

    /// An ability the run does not hold is **not** a candidate: a press naming one is a
    /// mis-input the caller refuses for free (§4.4), never a trade of something that was
    /// never there.
    #[test]
    fn an_unheld_ability_resolves_to_nothing() {
        assert_eq!(offer().resolve(full_hands(), AbilityId::Autodoors), None);
    }

    /// The candidate list is exactly [`AbilityId::MAX_TECH_HELD`] + 1 long on the run
    /// that can actually open an exchange — a full one — which is what lets the bar's
    /// four slots carry it without a second width budget (§11.4).
    #[test]
    fn a_full_run_offers_one_row_of_choices() {
        assert_eq!(
            offer().candidates(full_hands()).len(),
            AbilityId::MAX_TECH_HELD + 1,
        );
    }
}
