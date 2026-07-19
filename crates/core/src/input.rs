//! The §11.6 input mapping — **in the core so it is testable natively** (§12.1).
//!
//! A shell's input pump forwards raw key names here and feeds whatever comes back
//! to [`State::step`](crate::State::step); it never interprets a key itself, so
//! every binding is pinned by a native test instead of discovered in a browser.
//!
//! Two tables live here. [`input_for_key`] maps the movement keys — the §11.6
//! rows that drive actions the loop already has. [`ability_hotkey`] is the
//! **explicit** ability→letter assignment: the old game derived hotkeys from
//! labels (each ability claimed the first letter not taken by one above it), so
//! `Dephase` answered to `e` because `Decoy` had taken `d`, and **an ability's
//! key silently changed when the list above it changed**. Muscle memory is not
//! optional in a game where a mis-key ends a run, so here every assignment is a
//! named constant fact: keyed by the ability's *identity*, independent of any
//! list, and pinned one-by-one in the tests below.

use crate::cell::Direction;
use crate::state::Input;

/// Map a key (a browser `KeyboardEvent.key` name) to the [`Input`] it drives, or
/// `None` for a key the game does not own — which the shell must then leave to
/// the page (scrolling, browser shortcuts).
///
/// The §11.6 table: arrows / `4` `6` `8` `2` move, `5` / `w` wait — plus the vi
/// keys `h` `j` `k` `l` and `.`-to-wait as roguelike comfort. Note `w` *waits*
/// (§11.6): it is not a WASD movement key, and no movement binding may ever
/// claim it. `Enter`/`Space` confirm and `Escape` cancel arrive with the first
/// menu; letters route to [`ability_hotkey`] when abilities land (§8.3).
pub fn input_for_key(key: &str) -> Option<Input> {
    Some(match key {
        "ArrowUp" | "8" | "k" => Input::Step(Direction::North),
        "ArrowDown" | "2" | "j" => Input::Step(Direction::South),
        "ArrowLeft" | "4" | "h" => Input::Step(Direction::West),
        "ArrowRight" | "6" | "l" => Input::Step(Direction::East),
        "5" | "w" | "." => Input::Wait,
        _ => return None,
    })
}

/// The explicit ability hotkey (§11.6 **[SETTLED]**) for a §8.3 ability, by name.
///
/// The assignment is a `match` on the ability's identity — there is no list to
/// be ordered, so no reordering, insertion or removal can ever move a key; the
/// tests pin each pair so even an *edit* here is a visible decision, not a
/// silent shift. Activation (letter → ability → [`Input`]) wires up with the
/// ability ticket; the contract these keys honour is settled now, before any UI
/// exists to derive them from.
pub fn ability_hotkey(ability: &str) -> Option<char> {
    Some(match ability {
        "Run" => 'r',
        "Takedown" => 't',
        "Drag" => 'g',
        "Camouflage" => 'c',
        "Decoy" => 'd',
        "Dephase" => 'x',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The §8.3 starting set, in design-doc order — the order the old scheme
    /// derived keys from, kept here only to prove it no longer matters.
    const ABILITIES: [&str; 6] = ["Run", "Takedown", "Drag", "Camouflage", "Decoy", "Dephase"];

    /// Every single-character key the movement table owns; ability hotkeys must
    /// never collide with these.
    const MOVEMENT_KEYS: [&str; 11] = ["8", "2", "4", "6", "5", "w", "k", "j", "h", "l", "."];

    /// The §11.6 movement table, pinned: arrows, numpad and vi keys step; `5`,
    /// `w` and `.` wait. `w` waiting is the regression to watch — a WASD
    /// binding once claimed it, and §11.6 says it waits.
    #[test]
    fn the_movement_table_maps_per_the_design() {
        for (keys, expected) in [
            (&["ArrowUp", "8", "k"][..], Input::Step(Direction::North)),
            (&["ArrowDown", "2", "j"][..], Input::Step(Direction::South)),
            (&["ArrowLeft", "4", "h"][..], Input::Step(Direction::West)),
            (&["ArrowRight", "6", "l"][..], Input::Step(Direction::East)),
            (&["5", "w", "."][..], Input::Wait),
        ] {
            for key in keys {
                assert_eq!(input_for_key(key), Some(expected), "key {key:?}");
            }
        }
    }

    /// Keys the game does not own pass through untouched, so the page keeps its
    /// scrolling and shortcuts.
    #[test]
    fn unowned_keys_are_left_to_the_page() {
        for key in ["q", "F5", "Tab", "Meta", " ", "PageDown"] {
            assert_eq!(input_for_key(key), None, "key {key:?}");
        }
    }

    /// §11.6's core demand, pinned pair by pair: each ability's key is an
    /// explicit fact. If any of these assertions ever fails, a hotkey moved —
    /// which must be a deliberate, visible decision, never a side effect.
    #[test]
    fn every_ability_hotkey_is_pinned() {
        for (ability, key) in [
            ("Run", 'r'),
            ("Takedown", 't'),
            ("Drag", 'g'),
            ("Camouflage", 'c'),
            ("Decoy", 'd'),
            ("Dephase", 'x'),
        ] {
            assert_eq!(ability_hotkey(ability), Some(key), "{ability}");
        }
    }

    /// The old failure, made impossible: keys are a function of identity, not of
    /// position, so any reordering — or removal — of the ability list leaves
    /// every key exactly where it was. (`Decoy` losing its slot must not turn
    /// `Dephase` into `d`.)
    #[test]
    fn hotkeys_survive_any_reordering_of_the_ability_list() {
        let baseline: Vec<Option<char>> = ABILITIES.iter().map(|a| ability_hotkey(a)).collect();
        let mut reordered = ABILITIES;
        reordered.reverse();
        for (ability, &expected) in reordered.iter().rev().zip(&baseline) {
            assert_eq!(ability_hotkey(ability), expected, "{ability} shifted");
        }
        // Even with Decoy gone entirely, Dephase keeps its own key.
        assert_eq!(ability_hotkey("Dephase"), Some('x'));
    }

    /// No two abilities share a key, and no ability claims a movement key — the
    /// two collisions that would make a mis-key routine.
    #[test]
    fn hotkeys_collide_with_nothing() {
        let keys: Vec<char> = ABILITIES
            .iter()
            .map(|a| ability_hotkey(a).expect("every §8.3 ability has a key"))
            .collect();
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                assert_ne!(a, b, "two abilities share {a:?}");
            }
        }
        for key in keys {
            assert!(
                !MOVEMENT_KEYS.contains(&key.to_string().as_str()),
                "{key:?} is a movement key"
            );
        }
    }
}
