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

use crate::ability::AbilityId;
use crate::cell::Direction;
use crate::state::Input;

/// A **shell-level** command a key drives that is *not* a game action (§11.4) —
/// it changes what the screen shows, never the world, so it never enters the turn
/// loop, costs no turn, and produces no [`Event`](crate::state::Event). Kept in
/// the core beside [`input_for_key`] so the binding is pinned by a native test
/// like every other, even though the state it toggles lives in the shell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UiCommand {
    /// Deploy or dismiss the full ability panel (§11.4). The compact ability line
    /// is always on; this expands it to the named panel and folds it back. The
    /// on-screen deploy button drives the same toggle for touch and mouse.
    ToggleAbilityPanel,
    /// Deploy or dismiss the near line's full message list (§11.7). The near line
    /// always speaks the loudest live message; when more than one is live it shows
    /// a counter, and this expands the whole list and folds it back. The on-screen
    /// counter drives the same toggle for touch and mouse.
    ToggleMessageLog,
    /// Open or close the help overlay (§14 v2/#139): the glyph legend, colour key,
    /// and controls. A pure view toggle — no world change, no turn (§4.4) — so no
    /// guard moves while it is up. The header's `[?]` button drives the same toggle
    /// for touch and mouse.
    ToggleHelp,
}

/// Map a key to the [`UiCommand`] it drives, or `None` for a key that is not a UI
/// control. The shell consults this *before* [`input_for_key`]: a key claimed here
/// toggles view state and redraws without ever touching [`State`](crate::State).
///
/// `Tab` deploys the ability panel — a conventional "toggle the HUD" key, and one
/// that collides with neither a movement key nor an ability hotkey (§11.6). `m`
/// deploys the message list: a free letter (no movement key, no ability hotkey),
/// mnemonic for *messages*.
pub fn ui_command_for_key(key: &str) -> Option<UiCommand> {
    match key {
        "Tab" => Some(UiCommand::ToggleAbilityPanel),
        "m" => Some(UiCommand::ToggleMessageLog),
        // `?` opens the help card (§14 v2/#139): the conventional roguelike help key,
        // a free character that collides with no movement key or ability hotkey.
        "?" => Some(UiCommand::ToggleHelp),
        _ => None,
    }
}

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

/// Map a key to the ability **activation** it drives (§11.6 shortcut), or `None`.
///
/// A single-character key is matched by **identity** against the settled hotkey of
/// each economy ability ([`AbilityId::ALL`]) — never a list position — and resolves
/// to the one `Input::Activate(id)` the loop already runs (§8.2). This is the
/// keyboard half of the one activation path a pointer click also drives
/// ([`ability_at`](crate::ability_at)); the two share the identity resolution so a
/// key and a click can never disagree on what an ability's shortcut does. The bump
/// verbs Takedown and Drag are **not** activated (they are done by walking into
/// their target, §7.2/§8.3), so their letters resolve to nothing here.
pub fn ability_input_for_key(key: &str) -> Option<Input> {
    let mut chars = key.chars();
    let ch = match (chars.next(), chars.next()) {
        (Some(c), None) => c,
        _ => return None, // named keys ("Tab", "ArrowUp") are never a hotkey
    };
    AbilityId::ALL
        .into_iter()
        .find(|id| id.hotkey() == ch)
        .map(Input::Activate)
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
    /// scrolling and shortcuts. `Tab` is *not* here: it is a UI control
    /// ([`ui_command_for_key`]), not a game action, but it still returns `None`
    /// from [`input_for_key`] — the two tables are disjoint.
    #[test]
    fn unowned_keys_are_left_to_the_page() {
        for key in ["q", "F5", "Meta", " ", "PageDown"] {
            assert_eq!(input_for_key(key), None, "key {key:?}");
        }
    }

    /// The UI-command table (§11.4/§11.7): `Tab` deploys the ability panel and `m`
    /// the message list, and both are *shell* commands, never a game [`Input`] — so
    /// `input_for_key` stays `None` for them and neither toggle enters the turn
    /// loop. `m` is a UI key, so it also owns no ability activation. Other keys own
    /// no UI command.
    #[test]
    fn the_ui_keys_toggle_their_panels_and_are_not_game_inputs() {
        assert_eq!(
            ui_command_for_key("Tab"),
            Some(UiCommand::ToggleAbilityPanel)
        );
        assert_eq!(ui_command_for_key("m"), Some(UiCommand::ToggleMessageLog));
        // `?` opens the help card (§14 v2/#139) — a view toggle, so it never steps
        // the world: no turn passes and no guard moves while help is up (§4.4).
        assert_eq!(ui_command_for_key("?"), Some(UiCommand::ToggleHelp));
        for key in ["Tab", "m", "?"] {
            assert_eq!(input_for_key(key), None, "{key:?} is not a game action");
            assert_eq!(
                ability_input_for_key(key),
                None,
                "{key:?} is a UI key, not an ability"
            );
        }
        for key in ["w", "5", "r", "ArrowUp", "Escape"] {
            assert_eq!(
                ui_command_for_key(key),
                None,
                "key {key:?} owns no UI command"
            );
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

    /// The keyboard activation shortcut (§11.6): each economy ability's settled
    /// hotkey resolves to its `Input::Activate` by identity — the same input a
    /// pointer click fires — while the bump verbs Takedown and Drag, which are not
    /// activated, resolve to nothing even though they own hotkeys.
    #[test]
    fn an_ability_hotkey_activates_by_identity() {
        use crate::ability::AbilityId;
        for id in AbilityId::ALL {
            let key = id.hotkey().to_string();
            assert_eq!(
                ability_input_for_key(&key),
                Some(Input::Activate(id)),
                "{} shortcut",
                id.name()
            );
        }
        // The bump verbs own hotkeys but are not activated (§7.2/§8.3): 't' and 'g'
        // drive no activation.
        for key in ["t", "g"] {
            assert_eq!(ability_input_for_key(key), None, "bump verb key {key:?}");
        }
        // A movement key and a named key own no ability activation.
        for key in ["k", "5", "Tab", "ArrowUp"] {
            assert_eq!(ability_input_for_key(key), None, "key {key:?}");
        }
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
