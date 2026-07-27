//! The **mnemonic letters** (§11.6/#360): the ability bar's secondary keys.
//!
//! #359 bound the bar's four slots to `1`–`4`, which scales for ever — four keys
//! however large the catalogue grows — but gave up the one thing letters were good
//! at: `c` *means* Camouflage, and a digit means only "third along". This module
//! derives a letter to sit **beside** the digit: the first character of an ability's
//! bar name that no entry before it has already claimed.
//!
//! # This is the derivation §11.6 designed out, and here is why it is safe now
//!
//! §11.6's warning is about exactly this shape of rule: the old game derived hotkeys
//! from labels, each ability claiming the first letter not taken by one above it, so
//! `Dephase` answered to `e` because `Decoy` had taken `d` — and **an ability's key
//! silently changed when the list above it changed.** Three things carry the
//! difference:
//!
//! 1. **The claim set is the run's four, not the whole catalogue.** A letter can only
//!    be taken by something you are also holding and also looking at, and a loadout is
//!    fixed for the run (§8.3), so nothing shifts under a player mid-run.
//! 2. **It is not silent.** The bar draws the claimed letter highlighted, on the
//!    entry it fires (§11.4) — the key is a fact you read off the screen, exactly as
//!    the entry's state and name are. The old scheme's failure was invisibility.
//! 3. **The digit is always underneath.** A player who never learns a letter loses
//!    nothing, which is the safety net the old scheme did not have.
//!
//! What is still true and should not be glossed: **the same ability can carry
//! different letters in different runs.** Dephase is `p` on its own and `h` beside
//! Pierce Wall. The letter is a fact about the *loadout*, like a bar slot; only the
//! replay script's letter ([`ability_script_letter`](crate::ability_script_letter))
//! is a fact about the ability.
//!
//! # The rule
//!
//! Left to right through the bar, each name claims the first of its own characters
//! that is neither **reserved** (bound by §11.6's own tables — a mnemonic must never
//! shadow a movement or system key) nor already claimed by an entry to its left. A
//! name with nothing left to claim simply gets **none**: its digit stands alone and
//! nothing is silently reassigned.

use crate::input::{input_for_key, ui_command_for_key};

/// Whether `ch` is already bound by §11.6 and so can never be claimed as a mnemonic.
///
/// Asked of the **tables themselves** rather than a copied list, so a key added to
/// either one is off-limits here from the moment it is added — the drift a
/// hand-written set of reserved letters would invite. `w` waits, `hjkl` step and `m`
/// / `?` / `n` are the UI toggles; a mnemonic that shadowed one of those would make a
/// mis-key routine in a game where a mis-key ends a run.
fn is_reserved(ch: char) -> bool {
    let key = ch.to_string();
    input_for_key(&key).is_some() || ui_command_for_key(&key).is_some()
}

/// Claim one mnemonic per bar entry, in **bar order** — the index within each name of
/// the character that entry answers to, or `None` for an entry that could claim
/// nothing.
///
/// An index rather than the character itself, because both callers need the position:
/// the bar highlights that cell of the entry it drew (§11.4), and the key table reads
/// the character off the same place. Returning one and re-deriving the other is how
/// the drawn letter and the live binding would drift apart.
///
/// Case is folded to lowercase for both claiming and reserving, so `Camo`'s capital
/// `C` claims the `c` a player actually presses.
pub fn claim(bar_names: &[&str]) -> Vec<Option<usize>> {
    let mut claimed: Vec<char> = Vec::with_capacity(bar_names.len());
    let mut out = Vec::with_capacity(bar_names.len());
    for name in bar_names {
        let hit = name.chars().position(|ch| {
            let ch = ch.to_ascii_lowercase();
            !is_reserved(ch) && !claimed.contains(&ch)
        });
        if let Some(i) = hit {
            claimed.push(letter_at(name, i));
        }
        out.push(hit);
    }
    out
}

/// The lowercase character at index `i` of `name` — the letter a claim at that
/// position actually binds. Panics only on an index [`claim`] did not produce.
pub(crate) fn letter_at(name: &str, i: usize) -> char {
    name.chars()
        .nth(i)
        .expect("a claimed index is a character of the name")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::AbilityId;

    /// The letters a set of names claims, as characters — what the tests below read.
    fn letters(names: &[&str]) -> Vec<Option<char>> {
        claim(names)
            .into_iter()
            .zip(names)
            .map(|(i, name)| i.map(|i| letter_at(name, i)))
            .collect()
    }

    /// The plain case: each name claims its own initial, and the letter is the one a
    /// player would guess.
    #[test]
    fn a_name_claims_its_initial_when_it_is_free() {
        assert_eq!(
            letters(&["Run", "Camo", "Decoy", "Sight"]),
            vec![Some('r'), Some('c'), Some('d'), Some('s')],
        );
    }

    /// #360's named case, and the **common** one: three of the catalogue's bar names
    /// start with `D`, so any loadout drawing two of Decoy / Doors / Daze walks the
    /// fallback. Each still gets a distinct letter, scanning its own name left to
    /// right — and the first one along keeps the initial, so the fallback costs the
    /// later entries rather than shuffling everything.
    #[test]
    fn three_names_starting_d_get_three_distinct_letters() {
        let names = ["Decoy", "Doors", "Daze"];
        let claimed = letters(&names);
        assert_eq!(claimed, vec![Some('d'), Some('o'), Some('a')]);

        let chosen: Vec<char> = claimed.into_iter().flatten().collect();
        for (i, a) in chosen.iter().enumerate() {
            for b in &chosen[i + 1..] {
                assert_ne!(a, b, "two entries claimed {a:?}");
            }
            assert!(!is_reserved(*a), "{a:?} is a reserved key");
        }
    }

    /// A mnemonic never shadows a §11.6 key. `Lock` is the case in the shipping
    /// catalogue that proves the check is doing work rather than passing by luck: `l`
    /// steps east and `k` steps north, so the name's own initial is unavailable and
    /// the claim falls through to `o`.
    #[test]
    fn a_reserved_letter_is_skipped_over() {
        assert_eq!(letters(&["Lock"]), vec![Some('o')]);
        // Two of its four characters are reserved, and it survives both: with `o`
        // claimed ahead of it too, it falls again to `c`. (`Oboe` is a stand-in — no
        // shipping bar name claims `o` — because the rule is a fact about names, and
        // the catalogue should not be able to make a hole in the test.)
        assert_eq!(letters(&["Oboe", "Lock"]), vec![Some('o'), Some('c')]);
    }

    /// The rule is **total**: a name with nothing left to claim gets `None` rather
    /// than stealing a letter or wrapping round. Its digit stands alone, which is the
    /// whole reason the digit is the primary key.
    #[test]
    fn a_name_with_nothing_left_to_claim_gets_no_letter() {
        // `Lock` with `o` and `c` both gone ahead of it: `l` and `k` are reserved, so
        // there is nothing of the name left to take.
        assert_eq!(
            letters(&["Oboe", "Cell", "Lock"]),
            vec![Some('o'), Some('c'), None],
        );
        // A name made entirely of reserved characters claims nothing from the start.
        assert_eq!(letters(&["whlkj"]), vec![None]);
    }

    /// Every claim is a real position in its own name, and the letter read back from
    /// that position is the one that was claimed — the property the bar's highlight
    /// depends on, since it colours the cell at exactly this index.
    #[test]
    fn every_claim_indexes_its_own_name() {
        let names: Vec<&str> = AbilityId::ALL.into_iter().map(|id| id.bar_name()).collect();
        for (i, name) in claim(&names).into_iter().zip(&names) {
            let Some(i) = i else { continue };
            assert!(
                i < name.chars().count(),
                "{name}: index {i} is off the name"
            );
            assert!(
                name.to_lowercase().contains(letter_at(name, i)),
                "{name}: the claimed letter is not in it",
            );
        }
    }

    /// No two entries of one bar ever share a letter, over **every** loadout a run
    /// can hold — the collision that would make a mnemonic ambiguous, checked
    /// exhaustively rather than on the hand-picked cases above.
    #[test]
    fn no_loadout_of_the_catalogue_produces_a_duplicate() {
        let all: Vec<AbilityId> = AbilityId::ALL.into_iter().collect();
        for a in 0..all.len() {
            for b in 0..all.len() {
                for c in 0..all.len() {
                    for d in 0..all.len() {
                        let ids = [all[a], all[b], all[c], all[d]];
                        let names: Vec<&str> = ids.iter().map(|id| id.bar_name()).collect();
                        let chosen: Vec<char> = claim(&names)
                            .into_iter()
                            .zip(&names)
                            .filter_map(|(i, name)| i.map(|i| letter_at(name, i)))
                            .collect();
                        for (i, x) in chosen.iter().enumerate() {
                            assert!(
                                !chosen[i + 1..].contains(x),
                                "{names:?} claimed {x:?} twice",
                            );
                        }
                    }
                }
            }
        }
    }
}
